//! The [`Canvas`] and [`Flow`] components: canvas, pan/zoom, pointer state
//! machine, and the node/edge render layers.
//!
//! [`Canvas`] is the lower layer: a pannable/zoomable surface with the shared
//! [`FlowCore`] context, the pane gesture state machine, and nothing drawn on
//! it. [`Flow`] builds the node/edge layers, connection gesture defaults and
//! keyboard handling on top. Applications with their own node/edge rendering
//! (custom editors, seat-based ports…) can use [`Canvas`] directly and draw
//! into its `world` slot.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::html::geometry::WheelDelta;
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;

use crate::edge::{EdgeItem, EdgeMarkers, EdgeViewCtx, HANDLE_RIM};
use crate::node::{NodeItem, NodeViewCtx};
use crate::path::connection_path;
use crate::state::{
    orient_connection, ConnectionState, DragState, FlowApi, FlowConfig, FlowCore, FlowHandle,
    Interaction,
};
use crate::types::{
    AnchorMode, ConnectEnd, Connection, DeleteRequest, Edge, HandleKey, HandleKind, Id, Node,
    NodeGeom, Point, Rect, Viewport,
};

/// The component styles [`Canvas`] injects at runtime, public so that tests
/// and server-side renderers can install the same sheet themselves.
pub static STYLE: &str = include_str!("style.css");

static NEXT_IID: AtomicUsize = AtomicUsize::new(0);

fn client_point(coords: dioxus::html::geometry::ClientPoint) -> Point {
    Point::new(coords.x, coords.y)
}

/// How a wheel notch converts to pixels, per delta unit. A notch reports
/// lines on Firefox and pixels on Chromium; without the conversion one notch
/// moves the canvas by three pixels.
fn wheel_pixels(delta: WheelDelta, page: f64) -> Point {
    match delta {
        WheelDelta::Pixels(v) => Point::new(v.x, v.y),
        WheelDelta::Lines(v) => Point::new(v.x * 16.0, v.y * 16.0),
        WheelDelta::Pages(v) => {
            let scale = page.max(240.0);
            Point::new(v.x * scale, v.y * scale)
        }
    }
}

/// Exponent per scrolled pixel while pinch-zooming (ctrl/meta + wheel, which
/// is also what browsers report for a trackpad pinch).
const PINCH_ZOOM_SENSITIVITY: f64 = 0.0025;

/// The pannable/zoomable surface every flow is drawn on.
///
/// Owns the [`FlowCore`] context, the container geometry, and the pane
/// gestures: pan (drag or scroll), zoom (scroll or pinch), pane clicks, and
/// the in-flight connection state machine that [`crate::Handle`]s feed.
/// Draws nothing itself: [`Flow`] passes its node/edge layers through the
/// `world` slot, and an application using [`Canvas`] directly renders its own
/// content there (in flow coordinates) and overlays as `children` (in screen
/// coordinates).
///
/// An application-level gesture that starts on content inside the canvas can
/// take the pointer away from the pane with [`FlowCore::claim_pointer`]; the
/// pane then neither pans nor reports a pane click for that press.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn Canvas(
    #[props(default = 0.25)] min_zoom: f64,
    #[props(default = 4.0)] max_zoom: f64,
    /// Pan the canvas by dragging empty space.
    #[props(default = true)]
    pan_on_drag: bool,
    /// Zoom with the mouse wheel / trackpad.
    #[props(default = true)]
    zoom_on_scroll: bool,
    /// Scrolling pans instead of zooming (shift swaps the axis, ctrl/meta —
    /// a trackpad pinch included — zooms about the pointer). Takes precedence
    /// over `zoom_on_scroll`.
    #[props(default = false)]
    pan_on_scroll: bool,
    /// Master switch for node dragging (read by [`Flow`]'s node layer).
    #[props(default = true)]
    nodes_draggable: bool,
    /// How far (screen px) a press on a node must travel before it moves the
    /// node, so a sloppy click never nudges one.
    #[props(default = 0.0)]
    drag_threshold: f64,
    /// Snap radius (screen px) for completing a connection near a handle.
    #[props(default = 28.0)]
    connection_radius: f64,
    #[props(default = 0.12)] fit_view_padding: f64,
    /// `id` attribute for the root element, so applications can find, focus,
    /// measure, or capture pointers to the canvas by id.
    id: Option<String>,
    /// Accessible name for the canvas.
    #[props(default = "Node graph".to_string())]
    aria_label: String,
    /// Extra classes for the root element.
    class: Option<String>,
    /// A primary press reaching the pane, before the pane decides to pan:
    /// the hook for application-level pane gestures (marquee selection,
    /// pulling a connection from a node border…). A handler that starts one
    /// claims the pointer with [`FlowCore::claim_pointer`]; the pane then
    /// leaves this press alone.
    on_pane_press: Option<Callback<Event<PointerData>>>,
    /// The caller's edges, when it has any (the [`Flow`] layers and the
    /// default connect behavior read and write these through the core).
    edges: Option<Signal<Vec<Edge>>>,
    /// Node geometry snapshot, when the caller renders nodes ([`Flow`] passes
    /// its memo; standalone canvases leave it empty).
    geoms: Option<Memo<Vec<NodeGeom>>>,
    /// Type-erased "deselect all nodes" for pane clicks and edge selection,
    /// provided by [`Flow`] which knows the node type.
    deselect_nodes: Option<Callback<()>>,
    /// Called when the user completes a connection between two handles. When
    /// absent, the edge is added to `edges` automatically.
    on_connect: Option<EventHandler<Connection>>,
    /// A connection drag has left a handle (started, not completed).
    on_connect_start: Option<EventHandler<HandleKey>>,
    /// A connection drag ended — wherever it ended. `connection` is `None`
    /// when the release was over nothing, and the point says where: the hook
    /// for "drop on empty canvas to create the node there".
    on_connect_end: Option<EventHandler<ConnectEnd>>,
    /// The application's say over which connections may complete. A target
    /// that fails is never offered as a snap and never completes.
    is_valid_connection: Option<Callback<Connection, bool>>,
    /// A node drag actually began (the press travelled past
    /// `drag_threshold`), with the ids being dragged: the moment to snapshot
    /// for undo.
    on_node_drag_start: Option<EventHandler<Vec<Id>>>,
    /// A node drag ended, with the ids that were dragged. Positions are
    /// already final in the node list: the moment to snap, settle, persist.
    on_node_drag_stop: Option<EventHandler<Vec<Id>>>,
    /// Click on empty canvas; the point is in flow coordinates. Fires only
    /// when the press neither travelled nor was claimed by content.
    on_pane_click: Option<EventHandler<Point>>,
    /// Double-click on the canvas; the point is in flow coordinates. The
    /// pane cannot tell content from paper here — an application that must
    /// can hit-test the client point itself before acting.
    on_pane_double_click: Option<EventHandler<Point>>,
    /// Keyboard events reaching the canvas root, after the canvas's own
    /// Escape handling. [`Flow`] wires Delete/Backspace through this.
    on_canvas_key_down: Option<Callback<Event<KeyboardData>>>,
    /// Pointer moves while a node drag is in flight, in flow coordinates.
    /// [`Flow`] applies the drag to its typed node list through this.
    on_drag_move: Option<Callback<Point>>,
    /// Content drawn inside the viewport transform, in flow coordinates.
    world: Option<Element>,
    /// Overlays drawn over the canvas in screen coordinates ([`Background`],
    /// [`Controls`], [`MiniMap`], or your own — they can call [`use_flow`]).
    ///
    /// [`Background`]: crate::Background
    /// [`Controls`]: crate::Controls
    /// [`MiniMap`]: crate::MiniMap
    /// [`use_flow`]: crate::use_flow
    children: Element,
) -> Element {
    let viewport = use_signal(Viewport::default);
    let container = use_signal(|| Rect::ZERO);
    let interaction = use_signal(Interaction::default);
    let connection = use_signal(|| None::<ConnectionState>);
    let handles = use_signal(HashMap::new);
    let mut config = use_signal(FlowConfig::default);
    let drag = use_signal(DragState::default);
    let epoch = use_signal(|| 0u64);
    let pending_sizes = use_signal(Vec::new);
    let size_flush_queued = use_signal(|| false);
    let pending_handles = use_signal(Vec::new);
    let handle_flush_queued = use_signal(|| false);
    let snap_key = use_memo(move || {
        connection
            .read()
            .as_ref()
            .and_then(|c| c.snap.as_ref())
            .map(|s| s.key.clone())
    });
    let connect_from = use_memo(move || connection.read().as_ref().map(|c| c.from.clone()));
    let overlay_insets = use_signal(HashMap::new);
    let own_edges = use_signal(Vec::new);
    let edges = edges.unwrap_or(own_edges);
    let empty_geoms = use_memo(Vec::new);
    let geoms = geoms.unwrap_or(empty_geoms);
    let deselect_nodes = deselect_nodes.unwrap_or_else(|| use_callback(move |_: ()| {}));

    let core = use_hook(|| FlowCore {
        iid: NEXT_IID.fetch_add(1, Ordering::Relaxed),
        viewport,
        container,
        interaction,
        connection,
        handles,
        edges,
        geoms,
        config,
        drag,
        epoch,
        snap_key,
        connect_from,
        deselect_nodes,
        overlay_insets,
        pending_sizes,
        size_flush_queued,
        pending_handles,
        handle_flush_queued,
        on_connect_start,
        valid_connection: is_valid_connection,
    });
    use_context_provider(|| core);

    // Mirror config props into the shared config signal.
    let cfg = FlowConfig {
        min_zoom,
        max_zoom,
        pan_on_drag,
        zoom_on_scroll,
        pan_on_scroll,
        nodes_draggable,
        drag_threshold,
        connection_radius,
        fit_view_padding,
    };
    if *config.peek() != cfg {
        config.set(cfg);
    }

    // Container geometry tracking.
    let mounted: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let refresh_rect = use_callback(move |_: ()| {
        let element = mounted.peek().clone();
        let mut container = container;
        if let Some(element) = element {
            spawn(async move {
                if let Ok(rect) = element.get_client_rect().await {
                    let rect = Rect::new(rect.origin.x, rect.origin.y, rect.width(), rect.height());
                    if *container.peek() != rect {
                        container.set(rect);
                    }
                }
            });
        }
    });

    // ---- Pointer state machine ----------------------------------------

    let end_gesture = use_callback(move |_: ()| {
        let mut interaction = interaction;
        let mut connection = connection;
        if *interaction.peek() != Interaction::None {
            interaction.set(Interaction::None);
        }
        if connection.peek().is_some() {
            connection.set(None);
        }
        let mut drag = drag;
        let mut state = drag.write();
        state.pointer_id = None;
        state.suppress_click = false;
    });

    let on_pointer_down = move |evt: Event<PointerData>| {
        refresh_rect.call(());
        core.cancel_animations();
        // A node, handle, edge or overlay may have claimed this pointer
        // already (children's handlers run first while bubbling).
        if *interaction.peek() != Interaction::None {
            return;
        }
        if evt.trigger_button() != Some(MouseButton::Primary) {
            return;
        }
        // Offer the press to the application first; it may claim the pointer
        // for a gesture of its own, in which case the pane stays out of it.
        if let Some(handler) = &on_pane_press {
            handler.call(evt.clone());
            if *interaction.peek() != Interaction::None {
                return;
            }
        }
        let client = client_point(evt.client_coordinates());
        {
            let mut drag = drag;
            let mut state = drag.write();
            state.pointer_id = Some(evt.pointer_id());
            state.origin_client = client;
            state.last_client = client;
            state.moved = false;
            state.suppress_click = false;
            state.grabs.clear();
        }
        let mut interaction = interaction;
        if pan_on_drag {
            interaction.set(Interaction::Pan);
        } else {
            interaction.set(Interaction::PanePressed);
        }
    };

    let on_pointer_move = move |evt: Event<PointerData>| {
        let current = *interaction.peek();
        if current == Interaction::None {
            return;
        }
        // A gesture belongs to the pointer that started it: a second finger
        // must not steer the first one's pan.
        if drag
            .peek()
            .pointer_id
            .is_some_and(|id| id != evt.pointer_id())
        {
            return;
        }
        // Self-heal: if the pointer was released outside the container we
        // never saw the pointerup.
        if evt.held_buttons().is_empty() {
            end_gesture.call(());
            return;
        }
        let client = client_point(evt.client_coordinates());
        match current {
            Interaction::Pan => {
                let delta = {
                    let mut drag = drag;
                    let mut state = drag.write();
                    let delta = client - state.last_client;
                    state.last_client = client;
                    state.moved = true;
                    delta
                };
                let mut viewport = viewport;
                let vp = *viewport.peek();
                viewport.set(vp.panned(delta));
            }
            Interaction::DragNode => {
                // The press has to travel before it moves anything, so a
                // sloppy click never nudges a node. Crossing the threshold is
                // the moment the drag really starts — the snapshot-for-undo
                // moment — so that is when `on_node_drag_start` fires.
                let began = {
                    let mut drag = drag;
                    let mut state = drag.write();
                    state.last_client = client;
                    let travelled = state.origin_client.distance(client);
                    let passed = state.moved || travelled >= config.peek().drag_threshold;
                    let began = passed && !state.moved;
                    if passed {
                        state.moved = true;
                    }
                    if !passed {
                        return;
                    }
                    began
                };
                if began {
                    if let Some(handler) = &on_node_drag_start {
                        let ids: Vec<Id> =
                            drag.peek().grabs.iter().map(|(id, _)| id.clone()).collect();
                        handler.call(ids);
                    }
                }
                let flow = core.client_to_flow(client);
                if let Some(handler) = &on_drag_move {
                    handler.call(flow);
                }
            }
            Interaction::Connect => {
                let flow = core.client_to_flow(client);
                let mut connection = connection;
                let from = connection.peek().as_ref().map(|c| c.from.clone());
                if let Some(from) = from {
                    let snap = core.find_snap(&from, flow);
                    connection.set(Some(ConnectionState {
                        from,
                        cursor: flow,
                        snap,
                    }));
                }
            }
            _ => {}
        }
    };

    let on_pointer_up = move |evt: Event<PointerData>| {
        if drag
            .peek()
            .pointer_id
            .is_some_and(|id| id != evt.pointer_id())
        {
            return;
        }
        let current = *interaction.peek();
        match current {
            // A click on empty canvas (no pan movement happened).
            Interaction::Pan | Interaction::PanePressed => {
                let state = drag.peek().clone();
                let is_click =
                    (current == Interaction::PanePressed || !state.moved) && !state.suppress_click;
                if is_click {
                    let client = client_point(evt.client_coordinates());
                    let flow = core.client_to_flow(client);
                    if !evt.modifiers().shift() {
                        deselect_nodes.call(());
                        deselect_edges(edges);
                    }
                    if let Some(handler) = &on_pane_click {
                        handler.call(flow);
                    }
                }
            }
            Interaction::Connect => {
                let done = connection.peek().clone();
                if let Some(done) = done {
                    let completed = done
                        .snap
                        .as_ref()
                        .map(|snap| orient_connection(&done.from, &snap.key));
                    if let Some(conn) = completed.clone() {
                        match &on_connect {
                            Some(handler) => handler.call(conn),
                            None => add_edge_for_connection(edges, conn),
                        }
                    }
                    // However it ended: the release point plus what (if
                    // anything) completed. A `None` connection with a point is
                    // the drop-on-empty-canvas hook.
                    if let Some(handler) = &on_connect_end {
                        let client = client_point(evt.client_coordinates());
                        handler.call(ConnectEnd {
                            point: core.client_to_flow(client),
                            connection: completed,
                        });
                    }
                }
            }
            Interaction::DragNode if drag.peek().moved => {
                if let Some(handler) = &on_node_drag_stop {
                    let ids: Vec<Id> = drag.peek().grabs.iter().map(|(id, _)| id.clone()).collect();
                    handler.call(ids);
                }
            }
            _ => {}
        }
        end_gesture.call(());
    };

    let on_wheel = move |evt: Event<WheelData>| {
        let config = *config.peek();
        if !config.pan_on_scroll && !config.zoom_on_scroll {
            return;
        }
        evt.prevent_default();
        core.cancel_animations();
        let client = client_point(evt.client_coordinates());
        let page = container.peek().height;
        let delta = wheel_pixels(evt.delta(), page);
        let modifiers = evt.modifiers();
        // A pinch (or ctrl/meta scroll) zooms about the pointer in either
        // scroll mode.
        if config.pan_on_scroll {
            if modifiers.ctrl() || modifiers.meta() {
                if delta.y != 0.0 {
                    let factor = (-delta.y * PINCH_ZOOM_SENSITIVITY).exp();
                    core.zoom_by(factor, Some(client), 0);
                }
                return;
            }
            let mut viewport = viewport;
            let vp = *viewport.peek();
            // Shift turns a vertical wheel into horizontal travel, as
            // everywhere else.
            let by = if modifiers.shift() && delta.x == 0.0 {
                Point::new(-delta.y, 0.0)
            } else {
                Point::new(-delta.x, -delta.y)
            };
            viewport.set(vp.panned(by));
            return;
        }
        if delta.y == 0.0 {
            return;
        }
        let factor = (-delta.y * 0.0022).exp().clamp(0.5, 2.0);
        core.zoom_by(factor, Some(client), 0);
    };

    let on_key_down = move |evt: Event<KeyboardData>| {
        if evt.key() == Key::Escape {
            end_gesture.call(());
        }
        if let Some(handler) = &on_canvas_key_down {
            handler.call(evt);
        }
    };

    // Reading `interaction` here keeps cursor feedback classes fresh; it only
    // changes on gesture start/end, never per pointer-move frame.
    let gesture = *interaction.read();
    let root_class = format!(
        "dioxus-flow{}{}",
        match gesture {
            Interaction::Pan => " df-panning",
            Interaction::Connect => " df-connecting",
            _ => "",
        },
        class
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default()
    );

    rsx! {
        document::Style { {STYLE} }
        div {
            id,
            class: root_class,
            tabindex: "0",
            role: "application",
            aria_label,
            onmounted: move |evt| {
                let mut mounted = mounted;
                mounted.set(Some(evt.data()));
                refresh_rect.call(());
            },
            onresize: move |_| refresh_rect.call(()),
            onpointerdown: on_pointer_down,
            onpointermove: on_pointer_move,
            onpointerup: on_pointer_up,
            onpointercancel: move |evt: Event<PointerData>| {
                let owner = drag.peek().pointer_id;
                if owner.is_none() || owner == Some(evt.pointer_id()) {
                    end_gesture.call(());
                }
            },
            onwheel: on_wheel,
            ondoubleclick: move |evt: Event<MouseData>| {
                if let Some(handler) = &on_pane_double_click {
                    let client = client_point(evt.client_coordinates());
                    handler.call(core.client_to_flow(client));
                }
            },
            onkeydown: on_key_down,
            ViewportPane { {world} }
            {children}
        }
    }
}

/// An interactive node-graph canvas, in the spirit of react-flow.
///
/// Nodes and edges are owned by the caller as signals; the flow mutates them
/// in response to user interaction (dragging, selection, connecting) and the
/// caller can mutate them at any time (adding nodes, changing data…).
///
/// ```ignore
/// let nodes = use_signal(|| vec![
///     Node::new("1", "Input", (0.0, 0.0)).node_type("input"),
///     Node::new("2", "Process", (0.0, 120.0)),
/// ]);
/// let edges = use_signal(|| vec![Edge::new("1", "2").animated(true)]);
/// rsx! {
///     Flow { nodes, edges, fit_view: true,
///         Background {}
///         Controls {}
///         MiniMap {}
///     }
/// }
/// ```
#[component]
pub fn Flow<T: Clone + PartialEq + 'static>(
    /// The nodes, owned by the caller.
    nodes: Signal<Vec<Node<T>>>,
    /// The edges, owned by the caller.
    edges: Signal<Vec<Edge>>,
    /// How edges find their endpoints: [`AnchorMode::Handles`] (default) or
    /// [`AnchorMode::Seats`] — solver-packed positions around each node's rim,
    /// drawn with rim-aware curves and beads.
    #[props(default)]
    anchor: AnchorMode,
    #[props(default = 0.25)] min_zoom: f64,
    #[props(default = 4.0)] max_zoom: f64,
    /// Pan the canvas by dragging empty space.
    #[props(default = true)]
    pan_on_drag: bool,
    /// Zoom with the mouse wheel / trackpad.
    #[props(default = true)]
    zoom_on_scroll: bool,
    /// Scrolling pans instead of zooming (ctrl/meta or a pinch zooms).
    #[props(default = false)]
    pan_on_scroll: bool,
    /// Master switch for node dragging (individual nodes can also opt out).
    #[props(default = true)]
    nodes_draggable: bool,
    /// How far (screen px) a press on a node must travel before it moves the
    /// node, so a sloppy click never nudges one.
    #[props(default = 0.0)]
    drag_threshold: f64,
    /// Snap radius (screen px) for completing a connection near a handle.
    #[props(default = 28.0)]
    connection_radius: f64,
    /// Fit all nodes into view once nodes are measured after mount.
    #[props(default = false)]
    fit_view: bool,
    #[props(default = 0.12)] fit_view_padding: f64,
    /// Delete selected nodes/edges with Delete/Backspace.
    #[props(default = true)]
    delete_key: bool,
    /// `id` attribute for the root element.
    id: Option<String>,
    /// Extra classes for the root element.
    class: Option<String>,
    /// Custom renderer for node contents. Receives a [`NodeViewCtx`]; fall
    /// back to [`crate::DefaultNodeView`] for types you don't customize.
    node_view: Option<Callback<NodeViewCtx<T>, Element>>,
    /// Custom renderer for edges (SVG content).
    edge_view: Option<Callback<EdgeViewCtx, Element>>,
    /// Called when the user completes a connection between two handles. When
    /// absent, the edge is added automatically.
    on_connect: Option<EventHandler<Connection>>,
    /// A connection drag has left a handle (started, not completed).
    on_connect_start: Option<EventHandler<HandleKey>>,
    /// A connection drag ended — wherever it ended. `connection` is `None`
    /// when the release was over nothing, and the point says where: the hook
    /// for "drop on empty canvas to create the node there".
    on_connect_end: Option<EventHandler<ConnectEnd>>,
    /// The application's say over which connections may complete. A target
    /// that fails is never offered as a snap and never completes.
    is_valid_connection: Option<Callback<Connection, bool>>,
    /// A node drag actually began (the press travelled past
    /// `drag_threshold`), with the ids being dragged: the moment to snapshot
    /// for undo.
    on_node_drag_start: Option<EventHandler<Vec<Id>>>,
    /// A node drag ended, with the ids that were dragged. Positions are
    /// already final in the node list: the moment to snap, settle, persist.
    on_node_drag_stop: Option<EventHandler<Vec<Id>>>,
    /// Called when Delete/Backspace is pressed with a selection. When absent,
    /// the selection (plus connected edges) is deleted automatically; when
    /// present, nothing is deleted — call
    /// [`FlowHandle::delete_selected`](crate::FlowHandle::delete_selected)
    /// from the handler to perform the default cascade (after confirming,
    /// snapshotting for undo…).
    on_delete: Option<EventHandler<DeleteRequest>>,
    on_node_click: Option<EventHandler<Id>>,
    on_edge_click: Option<EventHandler<Id>>,
    /// Click on empty canvas; the point is in flow coordinates.
    on_pane_click: Option<EventHandler<Point>>,
    /// Double-click on the canvas; the point is in flow coordinates.
    on_pane_double_click: Option<EventHandler<Point>>,
    /// Attach a [`FlowHandle`] (from [`crate::use_flow_handle`]) for
    /// programmatic control: fit view, zoom, auto-layout…
    handle: Option<FlowHandle<T>>,
    /// Overlays such as [`crate::Background`], [`crate::Controls`],
    /// [`crate::MiniMap`], or your own (they can call [`crate::use_flow`]).
    children: Element,
) -> Element {
    let geoms = use_memo(move || {
        nodes
            .read()
            .iter()
            .map(|node| NodeGeom {
                id: node.id.clone(),
                rect: node.rect(),
                selected: node.selected,
                source_side: node.source_side,
                target_side: node.target_side,
                measured: node.size.is_some() || node.measured.is_some(),
            })
            .collect::<Vec<_>>()
    });
    let deselect_nodes = use_callback(move |_: ()| {
        if nodes.peek().iter().any(|n| n.selected) {
            nodes.clone().with_mut(|nodes| {
                for node in nodes.iter_mut() {
                    node.selected = false;
                }
            });
        }
    });

    // Wired to the canvas once it mounts (the core is created inside it).
    let attach_core: Signal<Option<FlowCore>> = use_signal(|| None);

    // Attach the programmatic handle, if provided.
    use_effect(move || {
        let Some(core) = *attach_core.read() else {
            return;
        };
        if let Some(handle) = handle {
            let mut inner = handle.inner;
            if inner.peek().is_none() {
                inner.set(Some(FlowApi { core, nodes }));
            }
        }
    });

    // Initial fit-view: wait until the container and all nodes are measured.
    let mut did_initial_fit = use_signal(|| false);
    use_effect(move || {
        let Some(core) = *attach_core.read() else {
            return;
        };
        let container_ready = core.container.read().width > 0.0;
        let geoms = geoms.read();
        let nodes_ready = !geoms.is_empty() && geoms.iter().all(|g| g.measured);
        if fit_view && !*did_initial_fit.peek() && container_ready && nodes_ready {
            did_initial_fit.set(true);
            core.fit_view(0);
        }
    });

    let on_drag_move = use_callback(move |flow: Point| {
        let Some(core) = *attach_core.peek() else {
            return;
        };
        let grabs = core.drag.peek().grabs.clone();
        let mut nodes = nodes;
        nodes.with_mut(|nodes| {
            for (id, grab) in &grabs {
                if let Some(node) = nodes.iter_mut().find(|n| &n.id == id) {
                    node.position = flow - *grab;
                }
            }
        });
    });

    let on_canvas_key_down = use_callback(move |evt: Event<KeyboardData>| match evt.key() {
        Key::Delete | Key::Backspace if delete_key => {
            let Some(core) = *attach_core.peek() else {
                return;
            };
            let request = delete_request(nodes, core.edges);
            if request.nodes.is_empty() && request.edges.is_empty() {
                return;
            }
            match &on_delete {
                Some(handler) => handler.call(request),
                None => delete_selected(nodes, core.edges),
            }
        }
        _ => {}
    });

    rsx! {
        Canvas {
            min_zoom,
            max_zoom,
            pan_on_drag,
            zoom_on_scroll,
            pan_on_scroll,
            nodes_draggable,
            drag_threshold,
            connection_radius,
            fit_view_padding,
            id,
            class,
            edges,
            geoms,
            deselect_nodes,
            on_connect,
            on_connect_start,
            on_connect_end,
            is_valid_connection,
            on_node_drag_start,
            on_node_drag_stop,
            on_pane_click,
            on_pane_double_click,
            on_canvas_key_down,
            on_drag_move,
            world: rsx! {
                CoreProbe { attach_core }
                match anchor {
                    AnchorMode::Handles => rsx! {
                        EdgesLayer { edge_view, on_edge_click }
                        NodesLayer { nodes, node_view, on_node_click }
                    },
                    AnchorMode::Seats => rsx! {
                        SeatGraphLayers {
                            nodes,
                            node_view,
                            on_node_click,
                            edge_view,
                            on_edge_click,
                        }
                    },
                }
                ConnectionLine {}
            },
            {children}
        }
    }
}

/// The node layer sandwiched between seat-anchored edges and their beads.
///
/// One component so the three share one solve: the edge curves render under
/// the nodes, but the beads — the dots where a connection meets a rim — sit
/// over them, because a bead is threaded on the rim, not tucked behind it.
#[component]
fn SeatGraphLayers<T: Clone + PartialEq + 'static>(
    nodes: Signal<Vec<Node<T>>>,
    node_view: Option<Callback<NodeViewCtx<T>, Element>>,
    on_node_click: Option<EventHandler<Id>>,
    edge_view: Option<Callback<EdgeViewCtx, Element>>,
    on_edge_click: Option<EventHandler<Id>>,
) -> Element {
    let core = use_context::<FlowCore>();
    // The one expensive step, behind a memo: re-solves when node geometry or
    // the edge list changes, never on pan or zoom. Applications with their
    // own gesture policy run this solve themselves and hand the result to
    // [`SeatEdges`]; here the edges signal is the whole story.
    let anchors = use_memo(move || {
        let geoms = core.geoms.read();
        let frames: std::collections::BTreeMap<Id, Rect> = geoms
            .iter()
            .map(|geom| (geom.id.clone(), geom.rect))
            .collect();
        let links: Vec<crate::ports::Link> = core
            .edges
            .read()
            .iter()
            .map(|edge| crate::ports::Link {
                id: edge.id.clone(),
                start: crate::ports::Terminal::Node(edge.source.clone()),
                end: crate::ports::Terminal::Node(edge.target.clone()),
                start_seat: edge.source_seat,
                end_seat: edge.target_seat,
            })
            .collect();
        crate::ports::solve_ports(&frames, &links)
    });

    // `Flow`'s `edge_view` speaks the handle-mode context; hand it the seat
    // geometry through the same shape. Views that want the full rim-aware
    // geometry use [`SeatEdges`] directly.
    let adapted_edge_view = edge_view.map(|view| {
        Callback::new(move |ctx: crate::edge::SeatEdgeViewCtx| {
            view.call(EdgeViewCtx {
                edge: ctx.edge.clone(),
                source: ctx.anchors.start.point(),
                source_side: ctx.anchors.start.side(),
                target: ctx.anchors.end.point(),
                target_side: ctx.anchors.end.side(),
                path: crate::path::EdgePath {
                    d: ctx.geometry.path.clone(),
                    label: ctx.geometry.label,
                },
                // Seat-mode arrowheads are drawn geometry, not markers.
                marker_end: None,
            })
        })
    });

    let edges = core.edges;
    let solved = anchors.read();
    rsx! {
        crate::edge::SeatEdges {
            edges,
            anchors,
            edge_view: adapted_edge_view,
            on_edge_click,
        }
        crate::edge::SeatEdgeLabels { edges, anchors }
        NodesLayer { nodes, node_view, on_node_click }
        // The beads, over the nodes they are threaded on.
        svg { class: "df-edges df-ports", "aria-hidden": "true",
            for edge in edges.read().iter() {
                if let Some(pair) = solved.get(&edge.id) {
                    g {
                        key: "{edge.id}",
                        class: if edge.selected { "df-selected" },
                        circle {
                            class: "df-port",
                            cx: pair.start.x,
                            cy: pair.start.y,
                            r: crate::ports::PORT_RADIUS,
                        }
                        circle {
                            class: "df-port",
                            cx: pair.end.x,
                            cy: pair.end.y,
                            r: crate::ports::PORT_RADIUS,
                        }
                    }
                }
            }
        }
    }
}

/// Hands the canvas's core out to the owning [`Flow`], which renders above
/// the canvas and so cannot `use_context` it.
#[component]
fn CoreProbe(attach_core: Signal<Option<FlowCore>>) -> Element {
    let core = use_context::<FlowCore>();
    let mut attach_core = attach_core;
    if attach_core.peek().is_none() {
        attach_core.set(Some(core));
    }
    rsx! {}
}

/// The pannable/zoomable transform layer. Isolated so per-frame viewport
/// updates re-render only this tiny component, not the node/edge layers.
#[component]
fn ViewportPane(children: Element) -> Element {
    let core = use_context::<FlowCore>();
    let vp = *core.viewport.read();
    rsx! {
        div {
            class: "df-viewport",
            style: "transform: translate({vp.x}px, {vp.y}px) scale({vp.zoom});",
            {children}
        }
    }
}

/// A layer inside the canvas that shares the viewport transform: children are
/// laid out in flow coordinates. Render as a child of [`Canvas`] or [`Flow`]
/// for world-space overlays (annotations, guides, custom edge layers…).
#[component]
pub fn WorldLayer(class: Option<String>, children: Element) -> Element {
    let core = use_context::<FlowCore>();
    let vp = *core.viewport.read();
    let class = format!(
        "df-world-layer{}",
        class
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default()
    );
    rsx! {
        div {
            class,
            style: "transform: translate({vp.x}px, {vp.y}px) scale({vp.zoom});",
            {children}
        }
    }
}

#[component]
fn NodesLayer<T: Clone + PartialEq + 'static>(
    nodes: Signal<Vec<Node<T>>>,
    node_view: Option<Callback<NodeViewCtx<T>, Element>>,
    on_node_click: Option<EventHandler<Id>>,
) -> Element {
    rsx! {
        div { class: "df-nodes",
            for node in nodes.read().iter() {
                NodeItem::<T> {
                    key: "{node.id}",
                    nodes,
                    node: node.clone(),
                    node_view,
                    on_node_click,
                }
            }
        }
    }
}

#[component]
fn EdgesLayer(
    edge_view: Option<Callback<EdgeViewCtx, Element>>,
    on_edge_click: Option<EventHandler<Id>>,
) -> Element {
    let core = use_context::<FlowCore>();
    let edges = core.edges.read();
    let geoms = core.geoms.read();
    let handles = core.handles.read();
    let geom_by_id: HashMap<&str, &NodeGeom> =
        geoms.iter().map(|geom| (geom.id.as_str(), geom)).collect();
    // Borrowed lookup index: this layer re-renders every frame while a node
    // is dragged, and going through `resolve_anchor` would clone two key
    // Strings per edge per frame.
    let handle_idx: HashMap<(&str, HandleKind, &str), &crate::types::HandleGeom> = handles
        .iter()
        .map(|(key, geom)| ((key.node.as_str(), key.kind, key.id.as_str()), geom))
        .collect();
    let anchor = |geom: &NodeGeom, kind: HandleKind, handle_id: &Option<Id>| {
        let key = (geom.id.as_str(), kind, handle_id.as_deref().unwrap_or(""));
        crate::state::anchor_from_geom(handle_idx.get(&key).copied(), geom, kind)
    };

    let items: Vec<_> = edges
        .iter()
        .filter_map(|edge| {
            let source_geom = geom_by_id.get(edge.source.as_str())?;
            let target_geom = geom_by_id.get(edge.target.as_str())?;
            let (source, source_side, source_on_handle) =
                anchor(source_geom, HandleKind::Source, &edge.source_handle);
            let (target, target_side, target_on_handle) =
                anchor(target_geom, HandleKind::Target, &edge.target_handle);
            // End the visible path at the handle's rim instead of its center
            // so arrowheads stay in front of the handle dot.
            let source = if source_on_handle {
                source + source_side.normal() * HANDLE_RIM
            } else {
                source
            };
            let target = if target_on_handle {
                target + target_side.normal() * HANDLE_RIM
            } else {
                target
            };
            Some((
                edge.clone(),
                source,
                source_side,
                target,
                target_side,
                source_geom.rect,
                target_geom.rect,
            ))
        })
        .collect();

    rsx! {
        // Decorative for assistive tech: nodes expose the graph's content,
        // and edge hit-paths are pointer-only.
        svg { class: "df-edges", "aria-hidden": "true",
            defs { EdgeMarkers { iid: core.iid } }
            for (edge, source, source_side, target, target_side, source_rect, target_rect) in items {
                EdgeItem {
                    key: "{edge.id}",
                    edge,
                    source,
                    source_side,
                    target,
                    target_side,
                    source_rect,
                    target_rect,
                    edge_view,
                    on_edge_click,
                }
            }
        }
    }
}

/// The dashed preview while dragging a new connection from a handle.
#[component]
fn ConnectionLine() -> Element {
    let core = use_context::<FlowCore>();
    let connection = core.connection.read();
    let Some(conn) = connection.as_ref() else {
        return rsx! {};
    };
    let Some((from, from_side)) = core.anchor_of(&conn.from) else {
        return rsx! {};
    };
    let (to, to_side) = match &conn.snap {
        Some(snap) => (snap.point, Some(snap.side)),
        None => (conn.cursor, None),
    };
    let d = connection_path(from, from_side, to, to_side);
    rsx! {
        svg { class: "df-connection",
            path { class: "df-connection-path", d }
        }
    }
}

pub(crate) fn deselect_edges(mut edges: Signal<Vec<Edge>>) {
    if edges.peek().iter().any(|e| e.selected) {
        edges.with_mut(|edges| {
            for edge in edges.iter_mut() {
                edge.selected = false;
            }
        });
    }
}

/// Default behavior when no `on_connect` handler is given: add the edge,
/// skipping exact duplicates and de-duplicating the generated id.
fn add_edge_for_connection(mut edges: Signal<Vec<Edge>>, conn: Connection) {
    let duplicate = edges.peek().iter().any(|e| {
        e.source == conn.source
            && e.target == conn.target
            && e.source_handle == conn.source_handle
            && e.target_handle == conn.target_handle
    });
    if duplicate {
        return;
    }
    let mut edge = conn.into_edge();
    let base = edge.id.clone();
    let mut n = 2;
    while edges.peek().iter().any(|e| e.id == edge.id) {
        edge.id = format!("{base}-{n}");
        n += 1;
    }
    edges.with_mut(|edges| edges.push(edge));
}

/// What a delete keypress would remove, given the current selection.
fn delete_request<T: Clone + PartialEq + 'static>(
    nodes: Signal<Vec<Node<T>>>,
    edges: Signal<Vec<Edge>>,
) -> DeleteRequest {
    let removed: std::collections::HashSet<Id> = nodes
        .peek()
        .iter()
        .filter(|n| n.selected)
        .map(|n| n.id.clone())
        .collect();
    let edge_ids = edges
        .peek()
        .iter()
        .filter(|e| e.selected || removed.contains(&e.source) || removed.contains(&e.target))
        .map(|e| e.id.clone())
        .collect();
    DeleteRequest {
        nodes: removed.into_iter().collect(),
        edges: edge_ids,
    }
}

pub(crate) fn delete_selected<T: Clone + PartialEq + 'static>(
    mut nodes: Signal<Vec<Node<T>>>,
    mut edges: Signal<Vec<Edge>>,
) {
    let removed: std::collections::HashSet<Id> = nodes
        .peek()
        .iter()
        .filter(|n| n.selected)
        .map(|n| n.id.clone())
        .collect();
    let any_edges = edges
        .peek()
        .iter()
        .any(|e| e.selected || removed.contains(&e.source) || removed.contains(&e.target));
    if !removed.is_empty() {
        nodes.with_mut(|nodes| nodes.retain(|n| !n.selected));
    }
    if any_edges {
        edges.with_mut(|edges| {
            edges.retain(|e| {
                !e.selected && !removed.contains(&e.source) && !removed.contains(&e.target)
            })
        });
    }
}
