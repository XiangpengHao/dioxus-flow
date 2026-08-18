//! # dioxus-flow
//!
//! A [react-flow](https://reactflow.dev)-style node graph component library
//! for [Dioxus](https://dioxuslabs.com): pannable/zoomable canvas, draggable
//! nodes, connectable handles, selectable animated edges, automatic layered
//! layout, minimap, controls and background — with fully customizable node
//! and edge rendering.
//!
//! ```ignore
//! use dioxus::prelude::*;
//! use dioxus_flow::prelude::*;
//!
//! #[component]
//! fn App() -> Element {
//!     let nodes = use_signal(|| vec![
//!         Node::new("1", "Source", (0.0, 0.0)).node_type("input"),
//!         Node::new("2", "Sink", (0.0, 140.0)).node_type("output"),
//!     ]);
//!     let edges = use_signal(|| vec![Edge::new("1", "2").animated(true)]);
//!
//!     rsx! {
//!         div { style: "width: 100vw; height: 100vh;",
//!             Flow { nodes, edges, fit_view: true,
//!                 Background {}
//!                 Controls {}
//!                 MiniMap {}
//!             }
//!         }
//!     }
//! }
//! ```

mod anim;
mod background;
mod controls;
mod edge;
mod flow;
mod layout;
mod minimap;
mod node;
pub mod paper;
mod path;
pub mod place;
pub mod ports;
pub mod press;
pub mod settle;
mod state;
mod types;

pub use background::{Background, BackgroundVariant};
pub use controls::Controls;
pub use edge::{EdgeViewCtx, SeatEdgeLabels, SeatEdgeViewCtx, SeatEdges};
pub use flow::{Canvas, Flow, WorldLayer, STYLE as STYLESHEET};
pub use layout::{compute_layout, LayoutDirection, LayoutNode, LayoutOptions};
pub use minimap::MiniMap;
pub use node::{DefaultNodeView, Handle, NodeViewCtx};
pub use path::{bezier_path, edge_path, smooth_step_path, straight_path, EdgeGeometry, EdgePath};
pub use state::{use_flow, use_flow_handle, use_overlay_inset, FlowCore, FlowHandle, Interaction};
pub use types::{
    side_point, AnchorMode, ConnectEnd, Connection, DeleteRequest, Edge, EdgeKind, Grid,
    HandleGeom, HandleKey, HandleKind, Id, MarkerKind, Node, NodeGeom, Point, Rect, Side, Size,
    Viewport,
};

/// Everything you typically need.
pub mod prelude {
    pub use crate::{
        use_flow, use_flow_handle, use_overlay_inset, AnchorMode, Background, BackgroundVariant,
        ConnectEnd, Connection, Controls, DefaultNodeView, DeleteRequest, Edge, EdgeKind,
        EdgeViewCtx, Flow, FlowHandle, Handle, HandleKind, LayoutDirection, LayoutOptions,
        MarkerKind, MiniMap, Node, NodeViewCtx, Point, Rect, Side, Size, Viewport,
    };
}
