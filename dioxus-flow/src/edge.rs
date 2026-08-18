//! Edge rendering: default paths, labels, arrow markers, selection, and the
//! escape hatch for fully custom edges — for both anchor modes.

use dioxus::prelude::*;

use crate::path::{edge_path, EdgeGeometry, EdgePath};
use crate::ports;
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

/// Everything a seat-anchored custom edge (or label) view needs: the edge,
/// its solved anchors (point *and* outward normal at each end), and the full
/// rim-aware geometry — trimmed path, untrimmed outline for halos and grab
/// bands, arrowhead polygons, the label point, and `point_at`/
/// `nearest_label_position` for label interactions. Computed once per edge
/// with the edge's own `weight`, `label_position` and markers, so a custom
/// view never redoes the math.
#[derive(Clone, PartialEq)]
pub struct SeatEdgeViewCtx {
    pub edge: Edge,
    pub anchors: ports::EdgeAnchors,
    pub geometry: ports::EdgeGeometry,
}

/// The full geometry for one seat-anchored edge, from its own parameters.
fn seat_geometry(edge: &Edge, anchors: ports::EdgeAnchors) -> ports::EdgeGeometry {
    let arrows = ports::Arrows::new(
        edge.marker_start != MarkerKind::None,
        edge.marker_end != MarkerKind::None,
    );
    ports::edge_geometry(
        anchors.start,
        anchors.end,
        arrows,
        edge.weight,
        edge.label_position,
        false,
    )
}

/// The curves of seat-anchored edges, as one world-space layer.
///
/// Both inputs are the caller's: the edge list, and the solved anchors — so
/// an application whose gestures decide what the solver sees (transient
/// preview links, a hidden edge mid-drag) runs [`ports::solve_ports`] itself,
/// once, and every layer drawn from it — this one, its own beads, its own
/// previews — agrees. [`crate::Flow`]'s seat mode wires both up from its node
/// and edge signals.
///
/// Labels deliberately do not render here: they belong to
/// [`SeatEdgeLabels`], a separate layer, so a crossing edge's stroke never
/// draws through another's words.
#[component]
pub fn SeatEdges(
    /// The edges to draw, in paint order.
    edges: ReadSignal<Vec<Edge>>,
    /// Solved anchors by edge id, from [`ports::solve_ports`].
    anchors: ReadSignal<std::collections::BTreeMap<Id, ports::EdgeAnchors>>,
    /// Custom renderer for an edge's SVG. Receives the full geometry.
    edge_view: Option<Callback<SeatEdgeViewCtx, Element>>,
    /// Pointer down on an edge's default hit path (see `hit_paths`).
    on_edge_click: Option<EventHandler<Id>>,
    /// Whether this layer adds an invisible grab band per edge, with the
    /// default select-on-press behavior. Applications whose custom views
    /// carry their own hit paths and press semantics turn it off.
    #[props(default = true)]
    hit_paths: bool,
    /// Extra classes for the layer's `<svg>`.
    class: Option<String>,
) -> Element {
    let class = format!(
        "df-edges{}",
        class
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default()
    );
    let edges = edges.read();
    let anchors = anchors.read();
    rsx! {
        svg { class, "aria-hidden": "true",
            for edge in edges.iter() {
                if let Some(pair) = anchors.get(&edge.id) {
                    SeatEdgeItem {
                        key: "{edge.id}",
                        edge: edge.clone(),
                        anchors: *pair,
                        edge_view,
                        on_edge_click,
                        hit_paths,
                    }
                }
            }
        }
    }
}

/// The labels of seat-anchored edges, as one world-space layer over every
/// curve — so a connection crossing another never draws through its words.
///
/// With no `label_view`, edges that carry a label get it as text at their
/// `label_position`. With one, it is called for *every* edge — the
/// application decides which edges say something (a label being edited may
/// not be in the edge's `label` yet) and returns empty for the rest.
#[component]
pub fn SeatEdgeLabels(
    /// The edges, in paint order.
    edges: ReadSignal<Vec<Edge>>,
    /// Solved anchors by edge id, from [`ports::solve_ports`].
    anchors: ReadSignal<std::collections::BTreeMap<Id, ports::EdgeAnchors>>,
    /// Custom renderer for one edge's label (SVG content).
    label_view: Option<Callback<SeatEdgeViewCtx, Element>>,
    /// Extra classes for the layer's `<svg>`.
    class: Option<String>,
) -> Element {
    let class = format!(
        "df-edges df-edge-labels{}",
        class
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default()
    );
    let edges = edges.read();
    let anchors = anchors.read();
    rsx! {
        svg { class, "aria-hidden": "true",
            for edge in edges.iter() {
                if let Some(pair) = anchors.get(&edge.id) {
                    if let Some(view) = label_view {
                        {view.call(SeatEdgeViewCtx {
                            edge: edge.clone(),
                            anchors: *pair,
                            geometry: seat_geometry(edge, *pair),
                        })}
                    } else if let Some(label) = edge.label.as_deref() {
                        {
                            let at = seat_geometry(edge, *pair).label;
                            rsx! {
                                text {
                                    key: "{edge.id}",
                                    class: "df-edge-label",
                                    x: at.x,
                                    y: at.y,
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One edge under [`crate::AnchorMode::Seats`]: the rim-aware curve between
/// two solved anchors, with arrowheads as filled polygons trimmed from the
/// stroke (so a two-headed short edge keeps both heads).
#[component]
fn SeatEdgeItem(
    edge: Edge,
    anchors: ports::EdgeAnchors,
    edge_view: Option<Callback<SeatEdgeViewCtx, Element>>,
    on_edge_click: Option<EventHandler<Id>>,
    hit_paths: bool,
) -> Element {
    let core = use_context::<FlowCore>();
    let geometry = seat_geometry(&edge, anchors);

    let class = format!(
        "df-edge{}{}{}",
        if edge.selected { " df-selected" } else { "" },
        if edge.animated { " df-animated" } else { "" },
        edge.class
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default(),
    );

    let id = edge.id.clone();
    let selectable = edge.selectable;
    // The invisible grab band, on the untrimmed curve so it runs bead to
    // bead. Custom views that carry their own hit paths opt out.
    let hit = hit_paths.then(|| {
        let d = geometry.outline.clone();
        rsx! {
            path {
                class: "df-edge-hit",
                d,
                onpointerdown: move |evt| {
                    edge_pointer_down(core, &id, selectable, &on_edge_click, evt)
                },
            }
        }
    });

    if let Some(view) = edge_view {
        let custom = view.call(SeatEdgeViewCtx {
            edge: edge.clone(),
            anchors,
            geometry,
        });
        return rsx! {
            g { class,
                {custom}
                {hit}
            }
        };
    }

    rsx! {
        g { class,
            path {
                class: "df-edge-path",
                d: "{geometry.path}",
                style: edge.style.as_deref().unwrap_or_default(),
            }
            if let Some(arrow) = geometry.start_arrow.as_deref() {
                path { class: "df-edge-arrow", d: "{arrow}" }
            }
            if let Some(arrow) = geometry.end_arrow.as_deref() {
                path { class: "df-edge-arrow", d: "{arrow}" }
            }
            {hit}
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
