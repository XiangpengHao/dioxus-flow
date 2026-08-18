//! Edge rendering: default paths, labels, arrow markers, selection, and the
//! escape hatch for fully custom edges.

use dioxus::prelude::*;

use crate::path::{edge_path, EdgeGeometry, EdgePath};
use crate::state::{FlowCore, Interaction};
use crate::types::{Edge, Id, MarkerKind, Point, Rect, Side};

/// Distance from a handle's center to its rim (half the 10px dot). Edge
/// paths stop here so arrowheads render in front of the dot, not under it.
pub(crate) const HANDLE_RIM: f64 = 5.0;

/// Everything a custom edge view needs: the edge, its resolved anchors, and
/// the default path (so custom edges can restyle without redoing the math).
#[derive(Clone, PartialEq)]
pub struct EdgeViewCtx {
    pub edge: Edge,
    pub source: Point,
    pub source_side: Side,
    pub target: Point,
    pub target_side: Side,
    /// The path the default renderer would draw (`d` attribute + label
    /// anchor).
    pub path: EdgePath,
    /// `url(#...)` reference for the edge's configured end marker, if any.
    pub marker_end: Option<String>,
}

#[component]
pub(crate) fn EdgeItem(
    edge: Edge,
    source: Point,
    source_side: Side,
    target: Point,
    target_side: Side,
    source_rect: Rect,
    target_rect: Rect,
    edge_view: Option<Callback<EdgeViewCtx, Element>>,
    on_edge_click: Option<EventHandler<Id>>,
) -> Element {
    let core = use_context::<FlowCore>();
    let path = edge_path(
        edge.kind,
        &EdgeGeometry::new(source, source_side, target, target_side)
            .with_rects(source_rect, target_rect),
    );
    let marker_end = match edge.marker_end {
        MarkerKind::ArrowClosed => Some(format!("url(#df-arrowclosed-{})", core.iid)),
        MarkerKind::Arrow => Some(format!("url(#df-arrow-{})", core.iid)),
        MarkerKind::None => None,
    };

    let class = format!(
        "df-edge{}{}{}",
        if edge.selected { " df-selected" } else { "" },
        if edge.animated { " df-animated" } else { "" },
        edge.class
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default(),
    );

    if let Some(view) = edge_view {
        let ctx = EdgeViewCtx {
            edge: edge.clone(),
            source,
            source_side,
            target,
            target_side,
            path: path.clone(),
            marker_end: marker_end.clone(),
        };
        let custom = view.call(ctx);
        let id = edge.id.clone();
        let selectable = edge.selectable;
        return rsx! {
            g { class,
                {custom}
                // Invisible fat path so custom edges stay clickable.
                path {
                    class: "df-edge-hit",
                    d: "{path.d}",
                    onpointerdown: move |evt| {
                        edge_pointer_down(core, &id, selectable, &on_edge_click, evt)
                    },
                }
            }
        };
    }

    let id = edge.id.clone();
    let selectable = edge.selectable;
    rsx! {
        g { class,
            path {
                class: "df-edge-path",
                d: "{path.d}",
                "marker-end": marker_end,
                style: edge.style.as_deref().unwrap_or_default(),
            }
            path {
                class: "df-edge-hit",
                d: "{path.d}",
                onpointerdown: move |evt| {
                    edge_pointer_down(core, &id, selectable, &on_edge_click, evt)
                },
            }
            if let Some(label) = edge.label.as_deref() {
                text {
                    class: "df-edge-label",
                    x: path.label.x,
                    y: path.label.y,
                    "{label}"
                }
            }
        }
    }
}

fn edge_pointer_down(
    core: FlowCore,
    id: &Id,
    selectable: bool,
    on_edge_click: &Option<EventHandler<Id>>,
    evt: Event<PointerData>,
) {
    if *core.interaction.peek() != Interaction::None {
        return;
    }
    core.cancel_animations();
    // Claim the pointer so the pane doesn't start panning or clear the
    // selection we're about to make.
    core.interaction.clone().set(Interaction::Pressed);
    if selectable {
        let shift = evt.modifiers().shift();
        let mut edges = core.edges;
        edges.with_mut(|edges| {
            for edge in edges.iter_mut() {
                if &edge.id == id {
                    edge.selected = if shift { !edge.selected } else { true };
                } else if !shift {
                    edge.selected = false;
                }
            }
        });
        if !shift {
            core.deselect_nodes.call(());
        }
    }
    if let Some(handler) = on_edge_click {
        handler.call(id.clone());
    }
}

/// Arrowhead marker definitions, namespaced per flow instance. Markers use
/// `context-stroke` so they match each edge's stroke color.
#[component]
pub(crate) fn EdgeMarkers(iid: usize) -> Element {
    rsx! {
        marker {
            id: "df-arrowclosed-{iid}",
            "markerWidth": "12",
            "markerHeight": "12",
            "viewBox": "-10 -10 20 20",
            "refX": "0",
            "refY": "0",
            "markerUnits": "strokeWidth",
            orient: "auto-start-reverse",
            path {
                d: "M-7,-4.5 L0,0 L-7,4.5 Z",
                fill: "context-stroke",
                stroke: "none",
            }
        }
        marker {
            id: "df-arrow-{iid}",
            "markerWidth": "12",
            "markerHeight": "12",
            "viewBox": "-10 -10 20 20",
            "refX": "0",
            "refY": "0",
            "markerUnits": "strokeWidth",
            orient: "auto-start-reverse",
            path {
                d: "M-7,-4.5 L0,0 L-7,4.5",
                fill: "none",
                stroke: "context-stroke",
                stroke_width: "1.5",
                stroke_linecap: "round",
            }
        }
    }
}
