//! Shared flow state: the non-generic core handed to every child component
//! via context, plus the public [`FlowHandle`] for programmatic control from
//! outside the flow.

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::anim::{bump_epoch, tween};
use crate::layout::{compute_layout, LayoutNode, LayoutOptions};
use crate::types::{
    side_point, Edge, HandleGeom, HandleKey, HandleKind, Id, NodeGeom, Point, Rect, Side, Viewport,
};

/// Coarse interaction state. Per-frame details (drag offsets, last pointer
/// position) live in the non-reactive [`DragState`] so pointer-move frames
/// don't invalidate subscribers of this signal.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Interaction {
    #[default]
    None,
    /// Panning the canvas.
    Pan,
    /// Dragging one or more nodes.
    DragNode,
    /// Dragging a new connection out of a handle.
    Connect,
    /// The pane was pressed but panning is disabled; a release without
    /// movement still counts as a pane click.
    PanePressed,
    /// A child claimed the pointer (e.g. edge click); no pan/drag behavior.
    Pressed,
}

/// Per-gesture scratch state. Only ever accessed with `peek`/`write` from
/// event handlers, so writes don't trigger renders.
#[derive(Clone, Debug, Default)]
pub struct DragState {
    pub last_client: Point,
    pub moved: bool,
    /// Nodes being dragged: `(id, grab offset)` where
    /// `position = cursor_flow - grab`.
    pub grabs: Vec<(Id, Point)>,
}

/// A snap candidate for the in-progress connection.
#[derive(Clone, PartialEq, Debug)]
pub struct SnapTarget {
    pub key: HandleKey,
    pub point: Point,
    pub side: Side,
}

/// The in-progress connection gesture.
#[derive(Clone, PartialEq, Debug)]
pub struct ConnectionState {
    pub from: HandleKey,
    pub cursor: Point,
    pub snap: Option<SnapTarget>,
}

/// Static-ish configuration mirrored from `Flow` props.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FlowConfig {
    pub min_zoom: f64,
    pub max_zoom: f64,
    pub pan_on_drag: bool,
    pub zoom_on_scroll: bool,
    pub nodes_draggable: bool,
    /// Snap radius for completing connections, in screen pixels.
    pub connection_radius: f64,
    pub fit_view_padding: f64,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            // Low enough to overview a large graph, high enough that a
            // stray zoom-out never strands the user on an unreadable speck.
            min_zoom: 0.25,
            max_zoom: 4.0,
            pan_on_drag: true,
            zoom_on_scroll: true,
            nodes_draggable: true,
            connection_radius: 28.0,
            fit_view_padding: 0.12,
        }
    }
}

/// The non-generic heart of a flow, shared through context with every child
/// (layers, handles, `Background`, `Controls`, `MiniMap`, and user
/// components). All fields are `Copy` handles to reactive state.
#[derive(Clone, Copy)]
pub struct FlowCore {
    /// Unique per-flow-instance id, used to namespace SVG defs.
    pub iid: usize,
    pub viewport: Signal<Viewport>,
    /// Container rect in client (page) coordinates.
    pub container: Signal<Rect>,
    pub interaction: Signal<Interaction>,
    pub connection: Signal<Option<ConnectionState>>,
    pub handles: Signal<HashMap<HandleKey, HandleGeom>>,
    pub edges: Signal<Vec<Edge>>,
    /// Geometry snapshot of all nodes, derived from the node list.
    pub geoms: Memo<Vec<NodeGeom>>,
    pub config: Signal<FlowConfig>,
    pub(crate) drag: Signal<DragState>,
    pub(crate) epoch: Signal<u64>,
    /// Key of the current snap target — a narrow memo so handles don't
    /// re-render on every connection cursor move.
    pub(crate) snap_key: Memo<Option<HandleKey>>,
    /// The handle a connection is being dragged from, if any.
    pub(crate) connect_from: Memo<Option<HandleKey>>,
    /// Type-erased "deselect all nodes", so non-generic components (edges,
    /// pane) can clear node selection.
    pub(crate) deselect_nodes: Callback<()>,
    /// Screen-space bands reserved by overlay panels (minimap, controls…),
    /// keyed per overlay instance: fit-view keeps the graph clear of them.
    pub(crate) overlay_insets: Signal<HashMap<usize, (Side, f64)>>,
    /// Measured node sizes awaiting a batched write into `nodes`. Only ever
    /// peeked/written, never subscribed to: per-node resize events land here
    /// so N nodes mounting costs one re-render wave instead of N (which made
    /// mounting quadratic).
    pub(crate) pending_sizes: Signal<Vec<(Id, crate::types::Size)>>,
    /// Whether a size flush is already scheduled for this frame.
    pub(crate) size_flush_queued: Signal<bool>,
    /// Handle registrations/removals awaiting a batched write into `handles`
    /// (`None` = remove). Same coalescing rationale as `pending_sizes`: every
    /// handle registers in its own effect, and letting each registration
    /// re-render the edge layer made mounting N connected nodes O(N²).
    pub(crate) pending_handles: Signal<Vec<(HandleKey, Option<HandleGeom>)>>,
    /// Whether a handle flush is already scheduled for this frame.
    pub(crate) handle_flush_queued: Signal<bool>,
}

impl PartialEq for FlowCore {
    fn eq(&self, other: &Self) -> bool {
        self.iid == other.iid
    }
}

/// Access the surrounding flow's state from any component rendered inside a
/// [`crate::Flow`] (custom nodes, edges, controls, overlays…).
pub fn use_flow() -> FlowCore {
    use_context::<FlowCore>()
}

static NEXT_OVERLAY_KEY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Reserve a screen-space band of `thickness` pixels along `side` of the
/// container, from inside an overlay component (the built-in [`crate::Controls`]
/// and [`crate::MiniMap`] do this). Fit-view centers the graph in the
/// remaining area so nodes don't land underneath overlay panels.
pub fn use_overlay_inset(side: Side, thickness: f64) {
    let core = use_context::<FlowCore>();
    let key = use_hook(|| NEXT_OVERLAY_KEY.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let mut insets = core.overlay_insets;
    if insets.peek().get(&key) != Some(&(side, thickness)) {
        insets.write().insert(key, (side, thickness));
    }
    use_drop(move || {
        core.overlay_insets.clone().write().remove(&key);
    });
}

impl FlowCore {
    /// Queue a handle registration (`Some(geom)`) or removal (`None`) for a
    /// batched write into the `handles` registry at the end of the frame.
    pub(crate) fn queue_handle_write(&self, key: HandleKey, geom: Option<HandleGeom>) {
        self.pending_handles.clone().write().push((key, geom));
        let mut queued = self.handle_flush_queued;
        if *queued.peek() {
            return;
        }
        queued.set(true);
        let core = *self;
        // Detached from the enqueuing handle's scope so its unmount can't
        // strand `handle_flush_queued`; the flow itself may also unmount
        // before this runs, so every access is fallible.
        dioxus::core::spawn_forever(async move {
            crate::anim::sleep_ms(0).await;
            let mut queued_signal = core.handle_flush_queued;
            let Ok(mut queued) = queued_signal.try_write() else {
                return;
            };
            *queued = false;
            drop(queued);
            let mut pending_signal = core.pending_handles;
            let Ok(mut pending_ref) = pending_signal.try_write() else {
                return;
            };
            let pending = std::mem::take(&mut *pending_ref);
            drop(pending_ref);
            if pending.is_empty() {
                return;
            }
            let mut handles = core.handles;
            let changed = match handles.try_peek() {
                Ok(current) => pending.iter().any(|(key, geom)| match geom {
                    Some(geom) => current.get(key) != Some(geom),
                    None => current.contains_key(key),
                }),
                Err(_) => return,
            };
            if !changed {
                return;
            }
            let Ok(mut current) = handles.try_write() else {
                return;
            };
            for (key, geom) in pending {
                match geom {
                    Some(geom) => {
                        current.insert(key, geom);
                    }
                    None => {
                        current.remove(&key);
                    }
                }
            }
        });
    }

    /// Convert client (page) coordinates to flow coordinates.
    pub fn client_to_flow(&self, client: Point) -> Point {
        let rect = *self.container.peek();
        self.viewport.peek().screen_to_flow(client - rect.origin())
    }

    /// Convert flow coordinates to client (page) coordinates.
    pub fn flow_to_client(&self, flow: Point) -> Point {
        let rect = *self.container.peek();
        self.viewport.peek().flow_to_screen(flow) + rect.origin()
    }

    /// Cancel any in-flight animation.
    pub fn cancel_animations(&self) {
        bump_epoch(self.epoch);
    }

    /// Bounding box of all nodes in flow coordinates, if any.
    pub fn nodes_bounds(&self) -> Option<Rect> {
        let geoms = self.geoms.peek();
        let mut iter = geoms.iter();
        let first = iter.next()?.rect;
        Some(iter.fold(first, |acc, geom| acc.union(&geom.rect)))
    }

    /// Animate (or jump, with `duration_ms == 0`) to the given viewport.
    pub fn set_viewport(&self, target: Viewport, duration_ms: u64) {
        let mut viewport = self.viewport;
        if duration_ms == 0 {
            self.cancel_animations();
            viewport.set(target);
            return;
        }
        let from = *viewport.peek();
        tween(self.epoch, duration_ms, move |t| {
            viewport.set(from.lerp(&target, t));
        });
    }

    /// Zoom by `factor` keeping `anchor_client` (client coordinates, defaults
    /// to the container center) stationary.
    pub fn zoom_by(&self, factor: f64, anchor_client: Option<Point>, duration_ms: u64) {
        let config = *self.config.peek();
        let rect = *self.container.peek();
        let vp = *self.viewport.peek();
        let anchor = anchor_client
            .map(|c| c - rect.origin())
            .unwrap_or_else(|| Point::new(rect.width / 2.0, rect.height / 2.0));
        let zoom = (vp.zoom * factor).clamp(config.min_zoom, config.max_zoom);
        let flow_anchor = vp.screen_to_flow(anchor);
        let target = Viewport {
            offset: anchor - flow_anchor * zoom,
            zoom,
        };
        self.set_viewport(target, duration_ms);
    }

    pub fn zoom_in(&self, duration_ms: u64) {
        self.zoom_by(1.25, None, duration_ms);
    }

    pub fn zoom_out(&self, duration_ms: u64) {
        self.zoom_by(0.8, None, duration_ms);
    }

    /// Fit the given flow-space bounds into the container.
    pub fn fit_bounds(&self, bounds: Rect, padding: f64, duration_ms: u64) {
        if let Some(target) = fit_viewport(self, bounds, padding) {
            self.set_viewport(target, duration_ms);
        }
    }

    /// Fit all nodes into view.
    pub fn fit_view(&self, duration_ms: u64) {
        let padding = self.config.peek().fit_view_padding;
        if let Some(bounds) = self.nodes_bounds() {
            self.fit_bounds(bounds, padding, duration_ms);
        }
    }

    /// Center the given flow point in the container, keeping the zoom.
    pub fn center_on(&self, flow: Point, duration_ms: u64) {
        let rect = *self.container.peek();
        let zoom = self.viewport.peek().zoom;
        let target = Viewport {
            offset: Point::new(
                rect.width / 2.0 - flow.x * zoom,
                rect.height / 2.0 - flow.y * zoom,
            ),
            zoom,
        };
        self.set_viewport(target, duration_ms);
    }

    /// Resolve the anchor point and side of an edge endpoint on `geom`,
    /// preferring a registered handle and falling back to the node's default
    /// side for that kind. The final `bool` says whether a real handle was
    /// found (so callers can offset the path to the handle's rim).
    pub(crate) fn resolve_anchor(
        &self,
        handles: &HashMap<HandleKey, HandleGeom>,
        geom: &NodeGeom,
        kind: HandleKind,
        handle_id: &Option<Id>,
    ) -> (Point, Side, bool) {
        let key = HandleKey {
            node: geom.id.clone(),
            kind,
            id: handle_id.clone().unwrap_or_default(),
        };
        anchor_from_geom(handles.get(&key), geom, kind)
    }

    /// Resolve a registered handle key to its anchor point and side.
    pub(crate) fn anchor_of(&self, key: &HandleKey) -> Option<(Point, Side)> {
        let handles = self.handles.peek();
        let geoms = self.geoms.peek();
        let geom = geoms.iter().find(|geom| geom.id == key.node)?;
        let id = (!key.id.is_empty()).then(|| key.id.clone());
        let (point, side, _) = self.resolve_anchor(&handles, geom, key.kind, &id);
        Some((point, side))
    }

    /// Find the closest compatible handle within the snap radius of `cursor`
    /// (flow coordinates).
    pub(crate) fn find_snap(&self, from: &HandleKey, cursor: Point) -> Option<SnapTarget> {
        let radius = self.config.peek().connection_radius / self.viewport.peek().zoom.max(1e-6);
        let handles = self.handles.peek();
        let geoms = self.geoms.peek();
        let geom_by_id: HashMap<&str, &NodeGeom> =
            geoms.iter().map(|geom| (geom.id.as_str(), geom)).collect();

        let mut best: Option<(f64, SnapTarget)> = None;
        for (key, hg) in handles.iter() {
            if key.kind == from.kind || key.node == from.node {
                continue;
            }
            let Some(geom) = geom_by_id.get(key.node.as_str()) else {
                continue;
            };
            let point = side_point(&geom.rect, hg.side, hg.offset);
            let d2 = point.distance_sq(cursor);
            if d2 <= radius * radius && best.as_ref().map(|(bd, _)| d2 < *bd).unwrap_or(true) {
                best = Some((
                    d2,
                    SnapTarget {
                        key: key.clone(),
                        point,
                        side: hg.side,
                    },
                ));
            }
        }
        best.map(|(_, target)| target)
    }
}

/// Anchor point/side for an edge endpoint: the handle's position when one is
/// registered, otherwise the center of the node's default side for `kind`.
/// The `bool` reports whether a real handle was found.
pub(crate) fn anchor_from_geom(
    handle: Option<&HandleGeom>,
    geom: &NodeGeom,
    kind: HandleKind,
) -> (Point, Side, bool) {
    if let Some(hg) = handle {
        return (side_point(&geom.rect, hg.side, hg.offset), hg.side, true);
    }
    let side = match kind {
        HandleKind::Source => geom.source_side,
        HandleKind::Target => geom.target_side,
    };
    (side_point(&geom.rect, side, 0.5), side, false)
}

/// Orient a completed connection gesture into a `source -> target`
/// [`crate::Connection`], regardless of which end the drag started from.
pub(crate) fn orient_connection(from: &HandleKey, to: &HandleKey) -> crate::types::Connection {
    let (source, target) = match from.kind {
        HandleKind::Source => (from, to),
        HandleKind::Target => (to, from),
    };
    crate::types::Connection {
        source: source.node.clone(),
        target: target.node.clone(),
        source_handle: (!source.id.is_empty()).then(|| source.id.clone()),
        target_handle: (!target.id.is_empty()).then(|| target.id.clone()),
    }
}

/// The typed API attached to a [`FlowHandle`] once the flow mounts.
pub struct FlowApi<T: 'static> {
    pub core: FlowCore,
    pub nodes: Signal<Vec<crate::types::Node<T>>>,
}

impl<T> Clone for FlowApi<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for FlowApi<T> {}

/// A handle for controlling a [`crate::Flow`] from the component that owns it.
///
/// ```ignore
/// let flow = use_flow_handle();
/// rsx! {
///     button { onclick: move |_| flow.auto_layout(&LayoutOptions::default()), "Layout" }
///     Flow { nodes, edges, handle: flow }
/// }
/// ```
pub struct FlowHandle<T: 'static = ()> {
    pub(crate) inner: Signal<Option<FlowApi<T>>>,
}

impl<T> Clone for FlowHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for FlowHandle<T> {}

impl<T> PartialEq for FlowHandle<T> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// Create a [`FlowHandle`] to pass to a [`crate::Flow`]'s `handle` prop.
pub fn use_flow_handle<T: 'static>() -> FlowHandle<T> {
    FlowHandle {
        inner: use_signal(|| None),
    }
}

impl<T: Clone + PartialEq + 'static> FlowHandle<T> {
    fn api(&self) -> Option<FlowApi<T>> {
        *self.inner.peek()
    }

    /// The flow's shared core, once mounted.
    pub fn core(&self) -> Option<FlowCore> {
        self.api().map(|api| api.core)
    }

    /// Current viewport (non-reactive read).
    pub fn viewport(&self) -> Option<Viewport> {
        self.api().map(|api| *api.core.viewport.peek())
    }

    pub fn set_viewport(&self, viewport: Viewport, duration_ms: u64) {
        if let Some(api) = self.api() {
            api.core.set_viewport(viewport, duration_ms);
        }
    }

    pub fn fit_view(&self, duration_ms: u64) {
        if let Some(api) = self.api() {
            api.core.fit_view(duration_ms);
        }
    }

    pub fn zoom_in(&self, duration_ms: u64) {
        if let Some(api) = self.api() {
            api.core.zoom_in(duration_ms);
        }
    }

    pub fn zoom_out(&self, duration_ms: u64) {
        if let Some(api) = self.api() {
            api.core.zoom_out(duration_ms);
        }
    }

    /// Convert client (page) coordinates to flow coordinates, e.g. for
    /// placing a node at a click position.
    pub fn client_to_flow(&self, client: Point) -> Option<Point> {
        self.api().map(|api| api.core.client_to_flow(client))
    }

    /// Delete the selected nodes (with their edges) and selected edges — the
    /// same cascade the Delete key performs by default. Call this from an
    /// `on_delete` handler after confirming or snapshotting for undo.
    pub fn delete_selected(&self) {
        if let Some(api) = self.api() {
            crate::flow::delete_selected(api.nodes, api.core.edges);
        }
    }

    /// Re-layout the graph with animated node movement, then fit it into
    /// view. Handle sides follow the layout direction when
    /// `opts.update_handle_sides` is set.
    pub fn auto_layout(&self, opts: &LayoutOptions) {
        let Some(api) = self.api() else { return };
        let mut nodes = api.nodes;
        let core = api.core;

        let layout_nodes: Vec<LayoutNode> = nodes
            .peek()
            .iter()
            .map(|node| LayoutNode {
                id: node.id.clone(),
                size: node.rect().size(),
            })
            .collect();
        let edge_pairs: Vec<(Id, Id)> = core
            .edges
            .peek()
            .iter()
            .map(|edge| (edge.source.clone(), edge.target.clone()))
            .collect();
        let targets = compute_layout(&layout_nodes, &edge_pairs, opts);

        if opts.update_handle_sides {
            let (target_side, source_side) = opts.direction.handle_sides();
            nodes.with_mut(|nodes| {
                for node in nodes.iter_mut() {
                    node.target_side = target_side;
                    node.source_side = source_side;
                }
            });
        }

        let starts: HashMap<Id, Point> = nodes
            .peek()
            .iter()
            .map(|node| (node.id.clone(), node.position))
            .collect();

        // Final bounds of the layout, for the parallel fit-view tween below.
        let mut bounds: Option<Rect> = None;
        for layout_node in &layout_nodes {
            if let Some(pos) = targets.get(&layout_node.id) {
                let rect = Rect::from_points(*pos, layout_node.size);
                bounds = Some(bounds.map(|b| b.union(&rect)).unwrap_or(rect));
            }
        }

        tween(core.epoch, 420, move |t| {
            nodes.with_mut(|nodes| {
                for node in nodes.iter_mut() {
                    if let (Some(start), Some(end)) = (starts.get(&node.id), targets.get(&node.id))
                    {
                        node.position = start.lerp(*end, t);
                    }
                }
            });
        });

        // Fit the final layout into view, in parallel with the node tween.
        if let Some(bounds) = bounds {
            let padding = core.config.peek().fit_view_padding;
            fit_bounds_without_cancel(core, bounds, padding);
        }
    }
}

/// The viewport that fits `bounds` into the container, centered in the area
/// left free by overlay insets (each side capped so overlays can never
/// squeeze the fit area away entirely).
fn fit_viewport(core: &FlowCore, bounds: Rect, padding: f64) -> Option<Viewport> {
    let rect = *core.container.peek();
    if rect.width <= 0.0 || rect.height <= 0.0 || (bounds.width <= 0.0 && bounds.height <= 0.0) {
        return None;
    }
    let (mut left, mut right, mut top, mut bottom) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (side, thickness) in core.overlay_insets.peek().values() {
        match side {
            Side::Left => left = left.max(*thickness),
            Side::Right => right = right.max(*thickness),
            Side::Top => top = top.max(*thickness),
            Side::Bottom => bottom = bottom.max(*thickness),
        }
    }
    let cap_x = rect.width * 0.35;
    let cap_y = rect.height * 0.35;
    let (left, right) = (left.min(cap_x), right.min(cap_x));
    let (top, bottom) = (top.min(cap_y), bottom.min(cap_y));
    let free_w = rect.width - left - right;
    let free_h = rect.height - top - bottom;

    let config = *core.config.peek();
    let zoom_x = free_w / bounds.width.max(1.0);
    let zoom_y = free_h / bounds.height.max(1.0);
    let zoom =
        (zoom_x.min(zoom_y) * (1.0 - padding).max(0.05)).clamp(config.min_zoom, config.max_zoom);
    let center = bounds.center();
    Some(Viewport {
        offset: Point::new(
            left + free_w / 2.0 - center.x * zoom,
            top + free_h / 2.0 - center.y * zoom,
        ),
        zoom,
    })
}

/// Like `FlowCore::fit_bounds`, but rides the same epoch as a concurrently
/// running tween instead of cancelling it.
fn fit_bounds_without_cancel(core: FlowCore, bounds: Rect, padding: f64) {
    let Some(target) = fit_viewport(&core, bounds, padding) else {
        return;
    };
    let mut viewport = core.viewport;
    let from = *viewport.peek();
    let epoch = core.epoch;
    let my_epoch = *epoch.peek();
    spawn(async move {
        let start = web_time::Instant::now();
        loop {
            crate::anim::sleep_ms(16).await;
            if *epoch.peek() != my_epoch {
                return;
            }
            let t = (start.elapsed().as_secs_f64() * 1000.0 / 420.0).min(1.0);
            viewport.set(from.lerp(&target, crate::anim::ease_in_out_cubic(t)));
            if t >= 1.0 {
                return;
            }
        }
    });
}
