//! Demo toolbar: drives the flow programmatically through a [`FlowHandle`].

use dioxus::prelude::*;
use dioxus_flow::prelude::*;

use super::CardData;

/// One undo step: the full node and edge lists before a mutation.
pub type Snapshot = (Vec<Node<CardData>>, Vec<Edge>);

const BTN: &str = "rounded-lg border border-zinc-200 bg-white px-3 py-1.5 text-[13px] \
                   font-medium text-zinc-600 shadow-sm transition-colors hover:bg-zinc-100 \
                   hover:text-zinc-900 active:bg-zinc-200 disabled:pointer-events-none \
                   disabled:opacity-40 dark:border-zinc-700 \
                   dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 \
                   dark:hover:text-zinc-100";

#[component]
pub fn Toolbar(
    flow: FlowHandle<CardData>,
    nodes: Signal<Vec<Node<CardData>>>,
    history: Signal<Vec<Snapshot>>,
    snapshot: Callback<()>,
    undo: Callback<()>,
) -> Element {
    let mut added = use_signal(|| 0usize);

    let add_node = move |_| {
        snapshot.call(());
        let n = added() + 1;
        added.set(n);
        // Footprint of a fresh card node, for collision testing.
        let size = Size::new(176.0, 42.0);
        // Place near the center of the current view, then cascade diagonally
        // until the spot doesn't cover an existing node.
        let (center, occupied) = flow
            .core()
            .map(|core| {
                let rect = *core.container.peek();
                let center = core.client_to_flow(Point::new(
                    rect.x + rect.width / 2.0,
                    rect.y + rect.height / 2.0,
                ));
                let occupied: Vec<Rect> = core.geoms.peek().iter().map(|g| g.rect).collect();
                (center, occupied)
            })
            .unwrap_or((Point::ZERO, Vec::new()));
        let base = Point::new(center.x - size.width / 2.0, center.y - size.height / 2.0);
        let mut pos = base;
        for step in 1..=64 {
            let free = !occupied.iter().any(|r| {
                r.x < pos.x + size.width + 16.0
                    && pos.x - 16.0 < r.max_x()
                    && r.y < pos.y + size.height + 16.0
                    && pos.y - 16.0 < r.max_y()
            });
            if free {
                break;
            }
            pos = base + Point::new(28.0 * step as f64, 28.0 * step as f64);
        }
        let mut node = Node::with_data(
            format!("new-{n}"),
            format!("Step {n}"),
            (pos.x, pos.y),
            CardData::default(),
        );
        // Select the newcomer (and only it) so it's easy to spot and move.
        node.selected = true;
        nodes.with_mut(|nodes| {
            for existing in nodes.iter_mut() {
                existing.selected = false;
            }
            nodes.push(node);
        });
        // If the cascade pushed it out of view, glide the view to it.
        if let Some(core) = flow.core() {
            let vp = *core.viewport.peek();
            let rect = *core.container.peek();
            let visible = Rect::from_points(
                vp.screen_to_flow(Point::ZERO),
                Size::new(rect.width / vp.zoom, rect.height / vp.zoom),
            );
            let new_center = Point::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
            if !visible.contains(new_center) {
                core.center_on(new_center, 250);
            }
        }
    };

    rsx! {
        header { class: "flex flex-wrap items-center gap-2 border-b border-zinc-200 bg-white px-4 py-2.5 dark:border-zinc-800 dark:bg-zinc-900",
            div { class: "mr-2 flex items-baseline gap-2",
                span { class: "text-[15px] font-bold tracking-tight text-zinc-900 dark:text-zinc-50",
                    "dioxus-flow"
                }
                span { class: "text-xs text-zinc-500 dark:text-zinc-400", "demo" }
            }
            button {
                class: BTN,
                onclick: move |_| {
                    snapshot.call(());
                    flow.auto_layout(&LayoutOptions::default())
                },
                "Layout ↓"
            }
            button {
                class: BTN,
                onclick: move |_| {
                    snapshot.call(());
                    flow.auto_layout(
                        &LayoutOptions::default().direction(LayoutDirection::LeftToRight),
                    )
                },
                "Layout →"
            }
            button { class: BTN, onclick: move |_| flow.fit_view(400), "Fit view" }
            button { class: BTN, onclick: add_node, "+ Add node" }
            button {
                class: BTN,
                disabled: history.read().is_empty(),
                title: "Undo (Ctrl+Z)",
                onclick: move |_| undo.call(()),
                "Undo"
            }
            span { class: "ml-auto hidden select-none text-xs text-zinc-500 sm:block dark:text-zinc-400",
                "drag canvas to pan · scroll to zoom · drag between handles to connect · ⌫ deletes · Ctrl+Z undoes"
            }
        }
    }
}
