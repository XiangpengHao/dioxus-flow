//! A custom node view styled entirely with Tailwind, demonstrating
//! `Flow { node_view }` customization.

use dioxus::prelude::*;
use dioxus_flow::prelude::*;

/// Payload carried by demo nodes (`Node<CardData>`).
#[derive(Clone, PartialEq, Default)]
pub struct CardData {
    pub subtitle: String,
    pub badge: Option<String>,
}

struct Accent {
    icon_wrap: &'static str,
    icon: Element,
}

fn accent_for(node_type: Option<&str>) -> Accent {
    match node_type {
        Some("source") => Accent {
            icon_wrap:
                "bg-emerald-100 text-emerald-600 dark:bg-emerald-500/15 dark:text-emerald-400",
            icon: rsx! {
                // database
                svg { class: "size-4", view_box: "0 0 24 24", fill: "none",
                    stroke: "currentColor", stroke_width: "2", stroke_linecap: "round",
                    ellipse { cx: "12", cy: "5", rx: "8", ry: "3" }
                    path { d: "M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" }
                }
            },
        },
        Some("model") => Accent {
            icon_wrap: "bg-indigo-100 text-indigo-600 dark:bg-indigo-500/15 dark:text-indigo-400",
            icon: rsx! {
                // cpu
                svg { class: "size-4", view_box: "0 0 24 24", fill: "none",
                    stroke: "currentColor", stroke_width: "2", stroke_linecap: "round",
                    rect { x: "5", y: "5", width: "14", height: "14", rx: "2" }
                    rect { x: "10", y: "10", width: "4", height: "4" }
                    path { d: "M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3" }
                }
            },
        },
        _ => Accent {
            icon_wrap: "bg-rose-100 text-rose-600 dark:bg-rose-500/15 dark:text-rose-400",
            icon: rsx! {
                // chart
                svg { class: "size-4", view_box: "0 0 24 24", fill: "none",
                    stroke: "currentColor", stroke_width: "2", stroke_linecap: "round",
                    path { d: "M3 3v18h18" }
                    path { d: "M7 15l4-6 4 3 5-8" }
                }
            },
        },
    }
}

/// Card-style node with icon, title, subtitle and optional badge.
/// `"source"` nodes only emit edges, `"sink"` nodes only receive them.
#[component]
pub fn CardNode(ctx: NodeViewCtx<CardData>) -> Element {
    let node = &ctx.node;
    let node_type = node.node_type.as_deref();
    let accent = accent_for(node_type);
    let has_target = node_type != Some("source");
    let has_source = node_type != Some("sink");

    let ring = if node.selected {
        "ring-2 ring-indigo-500 border-transparent"
    } else {
        "border-zinc-200 dark:border-zinc-700 hover:border-zinc-300 dark:hover:border-zinc-600"
    };
    let shadow = if ctx.dragging {
        "shadow-xl"
    } else {
        "shadow-sm"
    };

    rsx! {
        div { class: "min-w-44 rounded-xl border bg-white transition-shadow duration-150 dark:bg-zinc-900 {ring} {shadow}",
            if has_target {
                Handle { kind: HandleKind::Target, position: node.target_side }
            }
            if node_type == Some("model") {
                // Dedicated feedback in-port (see `Edge::target_handle("tune")`),
                // offset from the main port so converging arrowheads stay distinct.
                Handle {
                    kind: HandleKind::Target,
                    position: node.target_side,
                    id: "tune",
                    offset: 0.78,
                }
            }
            div { class: "flex items-center gap-2.5 px-3.5 py-2.5",
                div { class: "grid size-8 shrink-0 place-items-center rounded-lg {accent.icon_wrap}",
                    {accent.icon}
                }
                div { class: "min-w-0",
                    div { class: "flex items-center gap-1.5",
                        span { class: "truncate text-[13px] font-semibold text-zinc-800 dark:text-zinc-100",
                            "{node.label}"
                        }
                        if let Some(badge) = node.data.badge.as_deref() {
                            span { class: "rounded-full bg-zinc-100 px-1.5 py-px text-[10px] font-semibold uppercase tracking-wide text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300",
                                "{badge}"
                            }
                        }
                    }
                    div { class: "truncate text-xs text-zinc-500 dark:text-zinc-400",
                        "{node.data.subtitle}"
                    }
                }
            }
            if has_source {
                Handle { kind: HandleKind::Source, position: node.source_side }
            }
        }
    }
}
