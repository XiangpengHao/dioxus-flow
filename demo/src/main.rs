use dioxus::prelude::*;
use dioxus_flow::prelude::*;

mod components;

use components::{CardData, CardNode, Snapshot, Toolbar};

const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let nodes = use_signal(initial_nodes);
    let edges = use_signal(initial_edges);
    let flow = use_flow_handle::<CardData>();
    let history = use_signal(Vec::<Snapshot>::new);

    // Single-step-style undo: every mutating action snapshots first.
    let snapshot = use_callback(move |_: ()| {
        let mut history = history;
        history.with_mut(|h| {
            h.push((nodes.peek().clone(), edges.peek().clone()));
            if h.len() > 20 {
                h.remove(0);
            }
        });
    });
    let undo = use_callback(move |_: ()| {
        let mut history = history;
        if let Some((n, e)) = history.with_mut(|h| h.pop()) {
            let mut nodes = nodes;
            let mut edges = edges;
            nodes.set(n);
            edges.set(e);
        }
    });

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        div {
            class: "flex h-screen w-screen flex-col bg-zinc-50 dark:bg-zinc-950",
            onkeydown: move |evt| {
                let mods = evt.modifiers();
                if (mods.ctrl() || mods.meta()) && evt.key() == Key::Character("z".to_string()) {
                    evt.prevent_default();
                    undo.call(());
                }
            },
            Toolbar { flow, nodes, history, snapshot, undo }
            main { class: "relative min-h-0 flex-1",
                Flow {
                    nodes,
                    edges,
                    fit_view: true,
                    handle: flow,
                    on_delete: move |_| {
                        snapshot.call(());
                        flow.delete_selected();
                    },
                    on_connect: move |conn: Connection| {
                        let exists = edges.peek().iter().any(|e| {
                            e.source == conn.source
                                && e.target == conn.target
                                && e.source_handle == conn.source_handle
                                && e.target_handle == conn.target_handle
                        });
                        if !exists {
                            snapshot.call(());
                            let mut edges = edges;
                            edges.write().push(conn.into_edge());
                        }
                    },
                    node_view: move |ctx: NodeViewCtx<CardData>| {
                        match ctx.node.node_type.as_deref() {
                            Some("source" | "model" | "sink") => rsx! {
                                CardNode { ctx }
                            },
                            _ => rsx! {
                                DefaultNodeView::<CardData> { ctx }
                            },
                        }
                    },
                    Background { variant: BackgroundVariant::Dots }
                    Controls {}
                    MiniMap {}
                    EmptyHint { nodes }
                }
            }
        }
    }
}

/// Centered guidance when everything has been deleted; isolated so node
/// mutations re-render only this tiny component, not the whole app.
#[component]
fn EmptyHint(nodes: Signal<Vec<Node<CardData>>>) -> Element {
    // Narrow the subscription to the empty/non-empty flip so node drags
    // don't re-render this component every frame.
    let empty = use_memo(move || nodes.read().is_empty());
    if !empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "pointer-events-none absolute inset-0 z-10 grid place-items-center",
            div { class: "text-center",
                p { class: "text-sm font-medium text-zinc-500 dark:text-zinc-400",
                    "The canvas is clear"
                }
                p { class: "mt-1 text-[13px] text-zinc-400 dark:text-zinc-500",
                    "Add a node with “+ Add node”, or press Ctrl+Z to bring everything back"
                }
            }
        }
    }
}

fn card(
    id: &str,
    label: &str,
    ty: &str,
    pos: (f64, f64),
    subtitle: &str,
    badge: Option<&str>,
) -> Node<CardData> {
    Node::with_data(
        id,
        label,
        pos,
        CardData {
            subtitle: subtitle.to_string(),
            badge: badge.map(str::to_string),
        },
    )
    .node_type(ty)
}

fn plain(id: &str, label: &str, pos: (f64, f64)) -> Node<CardData> {
    Node::with_data(id, label, pos, CardData::default())
}

/// `?stress=N` swaps the showcase graph for an N-node grid, so canvas
/// performance is measurable (and demonstrable) at scale.
fn stress_count() -> Option<usize> {
    #[cfg(target_arch = "wasm32")]
    {
        let search = web_sys::window()?.location().search().ok()?;
        let n: usize = search
            .strip_prefix("?stress=")?
            .split('&')
            .next()?
            .parse()
            .ok()?;
        return Some(n.clamp(1, 5000));
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

fn stress_graph(n: usize) -> (Vec<Node<CardData>>, Vec<Edge>) {
    let cols = (n as f64).sqrt().ceil() as usize;
    let nodes = (0..n)
        .map(|i| {
            let (row, col) = (i / cols, i % cols);
            plain(
                &format!("s{i}"),
                &format!("Step {i}"),
                (col as f64 * 190.0, row as f64 * 110.0),
            )
        })
        .collect();
    // Chain each row left-to-right and stitch rows together at the ends:
    // every node gets an edge, like a real (if boring) pipeline.
    let edges = (1..n)
        .map(|i| {
            if i % cols == 0 {
                Edge::new(format!("s{}", i - cols), format!("s{i}"))
            } else {
                Edge::new(format!("s{}", i - 1), format!("s{i}"))
            }
        })
        .collect();
    (nodes, edges)
}

fn initial_nodes() -> Vec<Node<CardData>> {
    if let Some(n) = stress_count() {
        return stress_graph(n).0;
    }
    vec![
        card(
            "events",
            "Event Stream",
            "source",
            (0.0, 0.0),
            "events.stream",
            Some("kafka"),
        ),
        card(
            "users",
            "User Profiles",
            "source",
            (320.0, 0.0),
            "users.parquet",
            None,
        ),
        plain("clean", "Deduplicate", (0.0, 150.0)),
        plain("join", "Join on user_id", (160.0, 300.0)),
        card(
            "train",
            "Train Model",
            "model",
            (160.0, 450.0),
            "XGBoost v3",
            Some("GPU"),
        ),
        card(
            "eval",
            "Evaluate",
            "model",
            (160.0, 600.0),
            "AUC · precision",
            None,
        ),
        card("dash", "Dashboard", "sink", (0.0, 750.0), "grafana", None),
        card(
            "alerts",
            "Alerts",
            "sink",
            (320.0, 750.0),
            "pagerduty",
            None,
        ),
    ]
}

fn initial_edges() -> Vec<Edge> {
    if let Some(n) = stress_count() {
        return stress_graph(n).1;
    }
    vec![
        Edge::new("events", "clean").animated(true).label("50k/s"),
        Edge::new("users", "join"),
        Edge::new("clean", "join"),
        Edge::new("join", "train").label("features"),
        Edge::new("train", "eval"),
        Edge::new("eval", "dash"),
        Edge::new("eval", "alerts").animated(true),
        Edge::new("eval", "train")
            .label("tune")
            .kind(EdgeKind::SmoothStep)
            .target_handle("tune")
            .class("tune")
            .style("stroke: #f59e0b; stroke-dasharray: 4 3;"),
    ]
}
