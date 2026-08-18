//! Built-in zoom / fit-view control buttons.

use dioxus::prelude::*;

use crate::state::{use_overlay_inset, FlowCore};
use crate::types::Side;

/// Zoom-in, zoom-out and fit-view buttons. Render as a child of
/// [`crate::Flow`]; add extra buttons as children.
#[component]
pub fn Controls(class: Option<String>, children: Element) -> Element {
    let core = use_context::<FlowCore>();
    // Panel footprint (3 × 28px buttons + borders + 14px offset) plus
    // breathing room, so fit-view keeps nodes clear of the controls.
    use_overlay_inset(Side::Bottom, 112.0);
    let class = format!(
        "df-controls{}",
        class
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default()
    );
    rsx! {
        div {
            class,
            // Keep pane gestures from starting on the controls.
            onpointerdown: move |evt| evt.stop_propagation(),
            button {
                class: "df-control-btn",
                r#type: "button",
                title: "Zoom in",
                aria_label: "Zoom in",
                onclick: move |_| core.zoom_in(200),
                svg {
                    view_box: "0 0 16 16",
                    path { d: "M8 3.5v9M3.5 8h9", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", fill: "none" }
                }
            }
            button {
                class: "df-control-btn",
                r#type: "button",
                title: "Zoom out",
                aria_label: "Zoom out",
                onclick: move |_| core.zoom_out(200),
                svg {
                    view_box: "0 0 16 16",
                    path { d: "M3.5 8h9", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", fill: "none" }
                }
            }
            button {
                class: "df-control-btn",
                r#type: "button",
                title: "Fit view",
                aria_label: "Fit view",
                onclick: move |_| core.fit_view(400),
                svg {
                    view_box: "0 0 16 16",
                    path {
                        d: "M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10",
                        stroke: "currentColor",
                        stroke_width: "1.6",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        fill: "none",
                    }
                }
            }
            {children}
        }
    }
}
