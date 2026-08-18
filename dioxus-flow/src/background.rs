//! The canvas background pattern (dots, lines, or crosses), kept in lockstep
//! with the viewport transform.

use dioxus::prelude::*;

use crate::state::FlowCore;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BackgroundVariant {
    #[default]
    Dots,
    Lines,
    Cross,
}

/// A pan/zoom-aware background pattern. Render as a child of
/// [`crate::Flow`].
#[component]
pub fn Background(
    #[props(default)] variant: BackgroundVariant,
    /// Grid spacing in flow units.
    #[props(default = 24.0)]
    gap: f64,
    /// Dot radius / line width in flow units.
    #[props(default = 1.0)]
    size: f64,
    class: Option<String>,
) -> Element {
    let core = use_context::<FlowCore>();
    let vp = *core.viewport.read();
    let scaled = (gap * vp.zoom).max(1.0);
    let x = vp.offset.x.rem_euclid(scaled);
    let y = vp.offset.y.rem_euclid(scaled);
    let pattern_id = format!("df-bg-{}", core.iid);
    let class = format!(
        "df-background{}",
        class
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default()
    );

    rsx! {
        svg { class,
            defs {
                pattern {
                    id: "{pattern_id}",
                    x,
                    y,
                    width: scaled,
                    height: scaled,
                    "patternUnits": "userSpaceOnUse",
                    match variant {
                        BackgroundVariant::Dots => rsx! {
                            circle {
                                class: "df-background-dot",
                                cx: scaled / 2.0,
                                cy: scaled / 2.0,
                                r: (size * vp.zoom).max(0.4),
                            }
                        },
                        BackgroundVariant::Lines => rsx! {
                            path {
                                class: "df-background-line",
                                d: "M {scaled} 0 H 0 V {scaled}",
                                fill: "none",
                                stroke_width: (size * vp.zoom).max(0.3),
                            }
                        },
                        BackgroundVariant::Cross => rsx! {
                            path {
                                class: "df-background-line",
                                d: {
                                    let c = scaled / 2.0;
                                    let arm = (3.0 * vp.zoom).max(1.5);
                                    format!(
                                        "M {} {c} H {} M {c} {} V {}",
                                        c - arm,
                                        c + arm,
                                        c - arm,
                                        c + arm,
                                    )
                                },
                                fill: "none",
                                stroke_width: (size * vp.zoom).max(0.3),
                            }
                        },
                    }
                }
            }
            rect {
                width: "100%",
                height: "100%",
                fill: "url(#{pattern_id})",
            }
        }
    }
}
