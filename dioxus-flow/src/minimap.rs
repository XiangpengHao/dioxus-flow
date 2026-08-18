//! A minimap overview of the graph with the current viewport indicated.

use dioxus::prelude::*;

use crate::state::{use_overlay_inset, FlowCore, Interaction};
use crate::types::{Point, Rect, Side};

/// A minimap showing all nodes and the visible viewport. Click to jump.
/// Render as a child of [`crate::Flow`].
#[component]
pub fn MiniMap(
    #[props(default = 200.0)] width: f64,
    #[props(default = 140.0)] height: f64,
    class: Option<String>,
) -> Element {
    let core = use_context::<FlowCore>();
    // Panel height + 14px offset + breathing room: fit-view keeps nodes
    // from landing underneath the minimap.
    use_overlay_inset(Side::Bottom, height + 26.0);
    let geoms = core.geoms.read();
    let vp = *core.viewport.read();
    let container = *core.container.read();

    // An empty graph has nothing to map; a blank card is just noise.
    if geoms.is_empty() {
        return rsx! {};
    }

    // Visible region of the canvas, in flow coordinates.
    let visible = Rect::from_points(
        vp.screen_to_flow(Point::ZERO),
        (container.width / vp.zoom, container.height / vp.zoom).into(),
    );
    let mut world = visible;
    for geom in geoms.iter() {
        world = world.union(&geom.rect);
    }
    // Pad the world a little so rects don't touch the minimap border.
    let pad = (world.width.max(world.height) * 0.05).max(10.0);
    let world = Rect::new(
        world.x - pad,
        world.y - pad,
        world.width + 2.0 * pad,
        world.height + 2.0 * pad,
    );

    let class = format!(
        "df-minimap{}",
        class
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default()
    );

    let on_pointer_down = move |evt: Event<PointerData>| {
        evt.stop_propagation();
        core.interaction.clone().set(Interaction::Pressed);
        // Map the click (svg element coords, uniform "meet" scaling) back to
        // flow coordinates and center there.
        let p = evt.element_coordinates();
        let scale = (width / world.width).min(height / world.height);
        let dx = (width - world.width * scale) / 2.0;
        let dy = (height - world.height * scale) / 2.0;
        let flow = Point::new(world.x + (p.x - dx) / scale, world.y + (p.y - dy) / scale);
        core.center_on(flow, 250);
    };
    let on_pointer_up = move |_| {
        let mut interaction = core.interaction;
        if *interaction.peek() == Interaction::Pressed {
            interaction.set(Interaction::None);
        }
    };

    rsx! {
        svg {
            class,
            width,
            height,
            view_box: "{world.x} {world.y} {world.width} {world.height}",
            preserve_aspect_ratio: "xMidYMid meet",
            "role": "img",
            "aria-label": "Graph overview; click to move the view",
            onpointerdown: on_pointer_down,
            onpointerup: on_pointer_up,
            MiniMapNodes {}
            rect {
                class: "df-minimap-viewport",
                x: visible.x,
                y: visible.y,
                width: visible.width,
                height: visible.height,
            }
        }
    }
}

/// The node rects, isolated so that panning/zooming — which re-renders the
/// parent every frame for the viewBox and viewport indicator — only diffs a
/// handful of attributes instead of rebuilding one rect per node per frame.
#[component]
fn MiniMapNodes() -> Element {
    let core = use_context::<FlowCore>();
    let geoms = core.geoms.read();
    rsx! {
        for geom in geoms.iter() {
            rect {
                class: if geom.selected { "df-minimap-node df-selected" } else { "df-minimap-node" },
                x: geom.rect.x,
                y: geom.rect.y,
                width: geom.rect.width,
                height: geom.rect.height,
                rx: 2.0,
            }
        }
    }
}
