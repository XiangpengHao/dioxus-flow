//! Node rendering: the positioned wrapper, connection handles, and the
//! default node view.

use dioxus::prelude::*;

use crate::state::{ConnectionState, DragState, FlowCore, Interaction};
use crate::types::{HandleGeom, HandleKey, HandleKind, Id, Node, Point, Side, Size};

/// Context available to custom node views.
#[derive(Clone, PartialEq)]
pub struct NodeViewCtx<T: Clone + PartialEq + 'static> {
    pub node: Node<T>,
    /// Whether this node is currently being dragged.
    pub dragging: bool,
}

/// Identifies the node that a [`Handle`] belongs to. Provided by the node
/// wrapper, consumed by handles rendered anywhere inside the node's view.
#[derive(Clone, PartialEq)]
pub(crate) struct NodeScope {
    pub id: Id,
}

fn client_point(coords: dioxus::html::geometry::ClientPoint) -> Point {
    Point::new(coords.x, coords.y)
}

/// The positioned, draggable wrapper around each node's content.
#[component]
pub(crate) fn NodeItem<T: Clone + PartialEq + 'static>(
    nodes: Signal<Vec<Node<T>>>,
    node: Node<T>,
    node_view: Option<Callback<NodeViewCtx<T>, Element>>,
    on_node_click: Option<EventHandler<Id>>,
) -> Element {
    let core = use_context::<FlowCore>();
    use_context_provider(|| NodeScope {
        id: node.id.clone(),
    });

    // Subscribes only to gesture start/end, not per-frame drag state.
    let dragging = *core.interaction.read() == Interaction::DragNode
        && core.drag.peek().grabs.iter().any(|(id, _)| id == &node.id);

    let class = format!(
        "df-node{}{}{}",
        if node.selected { " df-selected" } else { "" },
        if dragging { " df-dragging" } else { "" },
        node.class
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default(),
    );
    let size_style = node
        .size
        .map(|s| format!("width:{}px;height:{}px;", s.width, s.height))
        .unwrap_or_default();
    let style = format!(
        "transform:translate({}px,{}px);z-index:{};{}{}",
        node.position.x,
        node.position.y,
        if dragging || node.selected { 1000 } else { 0 },
        size_style,
        node.style.as_deref().unwrap_or_default(),
    );

    let id_for_drag = node.id.clone();
    let id_for_resize = node.id.clone();
    let id_for_keys = node.id.clone();
    let draggable = node.draggable;
    let selectable = node.selectable;
    let label = node.label.clone();

    let content = match node_view {
        Some(view) => view.call(NodeViewCtx {
            node: node.clone(),
            dragging,
        }),
        None => rsx! {
            DefaultNodeView::<T> {
                ctx: NodeViewCtx { node: node.clone(), dragging },
            }
        },
    };

    rsx! {
        div {
            class,
            style,
            tabindex: "0",
            role: "group",
            aria_label: "{label}",
            onpointerdown: move |evt| {
                node_pointer_down(core, nodes, &id_for_drag, draggable, selectable, &on_node_click, evt)
            },
            onkeydown: move |evt| {
                node_key_down(core, nodes, &id_for_keys, draggable, selectable, evt)
            },
            onresize: move |evt| {
                if let Ok(size) = evt.data().get_border_box_size() {
                    store_measured(core, nodes, &id_for_resize, Size::new(size.width, size.height));
                }
            },
            {content}
        }
    }
}

/// Keyboard access to the pointer gestures: Enter/Space (toggle-)selects,
/// arrows nudge the focused node — or the whole selection when it belongs to
/// one — by 10px (1px with Shift). Delete bubbles up to the flow's handler.
fn node_key_down<T: Clone + PartialEq + 'static>(
    core: FlowCore,
    mut nodes: Signal<Vec<Node<T>>>,
    id: &Id,
    draggable: bool,
    selectable: bool,
    evt: Event<KeyboardData>,
) {
    let step = if evt.modifiers().shift() { 1.0 } else { 10.0 };
    let delta = match evt.key() {
        Key::ArrowUp => Point::new(0.0, -step),
        Key::ArrowDown => Point::new(0.0, step),
        Key::ArrowLeft => Point::new(-step, 0.0),
        Key::ArrowRight => Point::new(step, 0.0),
        Key::Enter => {
            evt.prevent_default();
            select_node(core, nodes, id, selectable, evt.modifiers().shift());
            return;
        }
        Key::Character(c) if c == " " => {
            evt.prevent_default();
            select_node(core, nodes, id, selectable, evt.modifiers().shift());
            return;
        }
        _ => return,
    };
    if !(draggable && core.config.peek().nodes_draggable) {
        return;
    }
    evt.prevent_default();
    core.cancel_animations();
    let focused_selected = nodes
        .peek()
        .iter()
        .find(|n| &n.id == id)
        .map(|n| n.selected)
        .unwrap_or(false);
    nodes.with_mut(|nodes| {
        for node in nodes.iter_mut() {
            let moves = if focused_selected {
                node.selected && node.draggable
            } else {
                &node.id == id
            };
            if moves {
                node.position = node.position + delta;
            }
        }
    });
}

fn select_node<T: Clone + PartialEq + 'static>(
    core: FlowCore,
    mut nodes: Signal<Vec<Node<T>>>,
    id: &Id,
    selectable: bool,
    toggle: bool,
) {
    if !selectable {
        return;
    }
    if toggle {
        nodes.with_mut(|nodes| {
            if let Some(node) = nodes.iter_mut().find(|n| &n.id == id) {
                node.selected = !node.selected;
            }
        });
    } else {
        nodes.with_mut(|nodes| {
            for node in nodes.iter_mut() {
                node.selected = &node.id == id;
            }
        });
        crate::flow::deselect_edges(core.edges);
    }
}

/// Record a node's measured size. Sizes are batched into one `nodes` write
/// per frame: resize events arrive once per node, and letting each of them
/// re-render the graph makes mounting N nodes O(N²).
fn store_measured<T: Clone + PartialEq + 'static>(
    core: FlowCore,
    mut nodes: Signal<Vec<Node<T>>>,
    id: &Id,
    size: Size,
) {
    let changed = nodes
        .peek()
        .iter()
        .find(|n| &n.id == id)
        .map(|n| match n.measured {
            Some(m) => (m.width - size.width).abs() > 0.5 || (m.height - size.height).abs() > 0.5,
            None => true,
        })
        .unwrap_or(false);
    if !changed {
        return;
    }
    core.pending_sizes.clone().write().push((id.clone(), size));
    let mut queued = core.size_flush_queued;
    if *queued.peek() {
        return;
    }
    queued.set(true);
    // Outlives the node that scheduled it, so a mid-flight deletion can't
    // strand `size_flush_queued`; the flow itself may also unmount before
    // this runs, so every access is fallible.
    dioxus::core::spawn_forever(async move {
        crate::anim::sleep_ms(0).await;
        let mut queued_signal = core.size_flush_queued;
        let Ok(mut queued) = queued_signal.try_write() else {
            return;
        };
        *queued = false;
        drop(queued);
        let mut pending_signal = core.pending_sizes;
        let Ok(mut pending) = pending_signal.try_write() else {
            return;
        };
        let sizes = std::mem::take(&mut *pending);
        drop(pending);
        if sizes.is_empty() {
            return;
        }
        let map: std::collections::HashMap<Id, Size> = sizes.into_iter().collect();
        let Ok(mut nodes) = nodes.try_write() else {
            return;
        };
        for node in nodes.iter_mut() {
            if let Some(size) = map.get(&node.id) {
                node.measured = Some(*size);
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn node_pointer_down<T: Clone + PartialEq + 'static>(
    core: FlowCore,
    mut nodes: Signal<Vec<Node<T>>>,
    id: &Id,
    draggable: bool,
    selectable: bool,
    on_node_click: &Option<EventHandler<Id>>,
    evt: Event<PointerData>,
) {
    // A handle inside this node may have claimed the pointer already.
    if *core.interaction.peek() != Interaction::None {
        return;
    }
    core.cancel_animations();
    let shift = evt.modifiers().shift();

    if selectable {
        let already_selected = nodes
            .peek()
            .iter()
            .find(|n| &n.id == id)
            .map(|n| n.selected)
            .unwrap_or(false);
        if shift {
            nodes.with_mut(|nodes| {
                if let Some(node) = nodes.iter_mut().find(|n| &n.id == id) {
                    node.selected = !node.selected;
                }
            });
        } else if !already_selected {
            nodes.with_mut(|nodes| {
                for node in nodes.iter_mut() {
                    node.selected = &node.id == id;
                }
            });
            crate::flow::deselect_edges(core.edges);
        }
    }
    if let Some(handler) = on_node_click {
        handler.call(id.clone());
    }

    let config = *core.config.peek();
    if !(draggable && config.nodes_draggable) {
        core.interaction.clone().set(Interaction::Pressed);
        return;
    }

    let cursor_flow = core.client_to_flow(client_point(evt.client_coordinates()));
    // Drag every selected draggable node as a group; always include the
    // pressed node itself.
    let grabs: Vec<(Id, Point)> = nodes
        .peek()
        .iter()
        .filter(|n| (n.selected && n.draggable) || &n.id == id)
        .map(|n| (n.id.clone(), cursor_flow - n.position))
        .collect();
    let mut drag = core.drag;
    {
        let mut state = drag.write();
        *state = DragState {
            pointer_id: Some(evt.pointer_id()),
            last_client: client_point(evt.client_coordinates()),
            moved: false,
            suppress_click: false,
            grabs,
        };
    }
    core.interaction.clone().set(Interaction::DragNode);
}

/// A connection point on a node. Place handles anywhere inside a custom node
/// view; edges anchor to them and new connections can be dragged out of them.
#[component]
pub fn Handle(
    /// Whether edges start (`Source`) or end (`Target`) here.
    kind: HandleKind,
    /// Which side of the node the handle sits on.
    position: ReadSignal<Side>,
    /// Optional handle id, referenced by `Edge::source_handle` /
    /// `Edge::target_handle`. Needed when a node has multiple handles of the
    /// same kind.
    id: Option<String>,
    /// Fraction (0..=1) along the side, defaults to centered.
    #[props(default = 0.5)]
    offset: f64,
    /// Extra classes (e.g. Tailwind utilities) for the handle dot.
    class: Option<String>,
) -> Element {
    let core = use_context::<FlowCore>();
    let scope = use_context::<NodeScope>();
    let key = use_hook(|| HandleKey {
        node: scope.id.clone(),
        kind,
        id: id.clone().unwrap_or_default(),
    });

    // Keep the registry in sync with (possibly reactive) geometry props.
    // Writes are queued and flushed in one batch per frame via the core.
    {
        let key = key.clone();
        use_effect(move || {
            let geom = HandleGeom {
                side: *position.read(),
                offset,
            };
            if core.handles.peek().get(&key) != Some(&geom) {
                core.queue_handle_write(key.clone(), Some(geom));
            }
        });
    }
    {
        let key = key.clone();
        use_drop(move || {
            core.queue_handle_write(key, None);
        });
    }

    // Narrow memos: these only change on connection start/end and snap
    // enter/leave, not on every pointer move.
    let connect_from = core.connect_from.read();
    let is_snap_target = core.snap_key.read().as_ref() == Some(&key);
    let is_connect_source = connect_from.as_ref() == Some(&key);
    let is_valid_target = connect_from
        .as_ref()
        .map(|from| from.kind != kind && from.node != key.node)
        .unwrap_or(false);

    let side = *position.read();
    let class = format!(
        "df-handle df-handle-{} df-handle-{}{}{}{}{}",
        match kind {
            HandleKind::Source => "source",
            HandleKind::Target => "target",
        },
        side.class_name(),
        if is_connect_source {
            " df-connecting-from"
        } else {
            ""
        },
        if is_valid_target {
            " df-valid-target"
        } else {
            ""
        },
        if is_snap_target { " df-snap" } else { "" },
        class
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default(),
    );
    let pct = offset * 100.0;
    let style = match side {
        Side::Top => format!("left:{pct}%;top:0;"),
        Side::Bottom => format!("left:{pct}%;top:100%;"),
        Side::Left => format!("left:0;top:{pct}%;"),
        Side::Right => format!("left:100%;top:{pct}%;"),
    };

    let key_for_down = key.clone();
    rsx! {
        div {
            class,
            style,
            onpointerdown: move |evt| {
                if *core.interaction.peek() != Interaction::None {
                    return;
                }
                core.cancel_animations();
                let client = client_point(evt.client_coordinates());
                let cursor = core.client_to_flow(client);
                {
                    let mut drag = core.drag;
                    let mut state = drag.write();
                    *state = DragState {
                        pointer_id: Some(evt.pointer_id()),
                        last_client: client,
                        moved: false,
                        suppress_click: false,
                        grabs: Vec::new(),
                    };
                }
                core.connection.clone().set(Some(ConnectionState {
                    from: key_for_down.clone(),
                    cursor,
                    snap: None,
                }));
                core.interaction.clone().set(Interaction::Connect);
            },
        }
    }
}

/// The built-in node view: a simple labeled box. Nodes typed `"input"` omit
/// the target handle, `"output"` omits the source handle.
///
/// Custom `node_view` callbacks can delegate to this for node types they
/// don't handle.
#[component]
pub fn DefaultNodeView<T: Clone + PartialEq + 'static>(ctx: NodeViewCtx<T>) -> Element {
    let node = &ctx.node;
    let is_input = node.node_type.as_deref() == Some("input");
    let is_output = node.node_type.as_deref() == Some("output");
    rsx! {
        div { class: "df-node-default",
            if !is_input {
                Handle { kind: HandleKind::Target, position: node.target_side }
            }
            span { class: "df-node-label", "{node.label}" }
            if !is_output {
                Handle { kind: HandleKind::Source, position: node.source_side }
            }
        }
    }
}
