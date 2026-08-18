//! Automatic layered (Sugiyama-style) graph layout.
//!
//! The algorithm:
//! 1. break cycles (DFS, back edges reversed),
//! 2. assign ranks by longest path,
//! 3. order nodes within ranks with a few barycenter sweeps,
//! 4. assign cross-axis coordinates by stacking, then refine by aligning
//!    each node to the mean of its neighbors without introducing overlap.
//!
//! [`compute_layout`] is a pure function over node sizes and edges, so it can
//! be used standalone. Inside a [`crate::Flow`] use
//! [`crate::FlowHandle::auto_layout`], which also animates node positions and
//! flips handle sides to match the layout direction.

use std::collections::HashMap;

use crate::types::{Id, Point, Side, Size};

/// Direction of rank progression.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LayoutDirection {
    /// Ranks flow top to bottom.
    #[default]
    TopToBottom,
    /// Ranks flow left to right.
    LeftToRight,
    BottomToTop,
    RightToLeft,
}

impl LayoutDirection {
    /// Handle sides implied by this direction: `(target_side, source_side)`.
    pub fn handle_sides(&self) -> (Side, Side) {
        match self {
            LayoutDirection::TopToBottom => (Side::Top, Side::Bottom),
            LayoutDirection::BottomToTop => (Side::Bottom, Side::Top),
            LayoutDirection::LeftToRight => (Side::Left, Side::Right),
            LayoutDirection::RightToLeft => (Side::Right, Side::Left),
        }
    }
}

/// Options for [`compute_layout`].
#[derive(Clone, PartialEq, Debug)]
pub struct LayoutOptions {
    pub direction: LayoutDirection,
    /// Gap between adjacent nodes within a rank, in flow units.
    pub node_gap: f64,
    /// Gap between ranks, in flow units.
    pub rank_gap: f64,
    /// When applied through the flow, also point node handles along the
    /// layout direction (e.g. left/right for horizontal layouts).
    pub update_handle_sides: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            direction: LayoutDirection::default(),
            node_gap: 50.0,
            rank_gap: 90.0,
            update_handle_sides: true,
        }
    }
}

impl LayoutOptions {
    pub fn direction(mut self, direction: LayoutDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn gaps(mut self, node_gap: f64, rank_gap: f64) -> Self {
        self.node_gap = node_gap;
        self.rank_gap = rank_gap;
        self
    }
}

/// Node input to [`compute_layout`].
#[derive(Clone, PartialEq, Debug)]
pub struct LayoutNode {
    pub id: Id,
    pub size: Size,
}

/// Compute positions (top-left corners) for `nodes` given directed `edges`
/// (pairs of node ids). Unknown ids in edges are ignored; cycles are fine.
pub fn compute_layout(
    nodes: &[LayoutNode],
    edges: &[(Id, Id)],
    opts: &LayoutOptions,
) -> HashMap<Id, Point> {
    let n = nodes.len();
    if n == 0 {
        return HashMap::new();
    }

    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id.as_str(), i))
        .collect();

    // Adjacency on indices, self-loops and unknown endpoints dropped.
    let mut arcs: Vec<(usize, usize)> = edges
        .iter()
        .filter_map(|(s, t)| {
            let (s, t) = (*index.get(s.as_str())?, *index.get(t.as_str())?);
            (s != t).then_some((s, t))
        })
        .collect();
    arcs.sort_unstable();
    arcs.dedup();

    reverse_back_edges(n, &mut arcs);
    let ranks = assign_ranks(n, &arcs);
    let order = order_ranks(n, &arcs, &ranks);

    // Main axis: rank progression. Cross axis: position within rank.
    let horizontal = matches!(
        opts.direction,
        LayoutDirection::LeftToRight | LayoutDirection::RightToLeft
    );
    let main_size = |i: usize| {
        if horizontal {
            nodes[i].size.width
        } else {
            nodes[i].size.height
        }
    };
    let cross_size = |i: usize| {
        if horizontal {
            nodes[i].size.height
        } else {
            nodes[i].size.width
        }
    };

    // Cross-axis centers: stack each rank centered on zero…
    let mut cross = vec![0.0f64; n];
    for rank in &order {
        let total: f64 = rank.iter().map(|&i| cross_size(i)).sum::<f64>()
            + opts.node_gap * (rank.len() - 1) as f64;
        let mut cursor = -total / 2.0;
        for &i in rank {
            cross[i] = cursor + cross_size(i) / 2.0;
            cursor += cross_size(i) + opts.node_gap;
        }
    }
    // …then refine by aligning nodes to the mean of their neighbors.
    let (preds, succs) = neighbor_lists(n, &arcs);
    for _ in 0..2 {
        for rank in order.iter().skip(1) {
            align_rank(rank, &preds, &cross_size, opts.node_gap, &mut cross);
        }
        for rank in order.iter().rev().skip(1) {
            align_rank(rank, &succs, &cross_size, opts.node_gap, &mut cross);
        }
    }

    // Main-axis centers, rank by rank.
    let mut main = vec![0.0f64; n];
    let mut cursor = 0.0;
    for rank in &order {
        let depth = rank.iter().map(|&i| main_size(i)).fold(0.0, f64::max);
        for &i in rank {
            main[i] = cursor + depth / 2.0;
        }
        cursor += depth + opts.rank_gap;
    }

    nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let center = match opts.direction {
                LayoutDirection::TopToBottom => Point::new(cross[i], main[i]),
                LayoutDirection::BottomToTop => Point::new(cross[i], -main[i]),
                LayoutDirection::LeftToRight => Point::new(main[i], cross[i]),
                LayoutDirection::RightToLeft => Point::new(-main[i], cross[i]),
            };
            let top_left = Point::new(
                center.x - node.size.width / 2.0,
                center.y - node.size.height / 2.0,
            );
            (node.id.clone(), top_left)
        })
        .collect()
}

/// Reverse back edges found by an iterative DFS so the graph becomes a DAG.
fn reverse_back_edges(n: usize, arcs: &mut [(usize, usize)]) {
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, &(s, _)) in arcs.iter().enumerate() {
        out[s].push(k);
    }
    // 0 = unvisited, 1 = on stack, 2 = done
    let mut state = vec![0u8; n];
    let mut back: Vec<usize> = Vec::new();
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        // Stack of (node, next out-arc cursor).
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        state[root] = 1;
        while let Some(&(v, cursor)) = stack.last() {
            if cursor < out[v].len() {
                stack.last_mut().unwrap().1 += 1;
                let arc_idx = out[v][cursor];
                let t = arcs[arc_idx].1;
                match state[t] {
                    0 => {
                        state[t] = 1;
                        stack.push((t, 0));
                    }
                    1 => back.push(arc_idx),
                    _ => {}
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
    for k in back {
        let (s, t) = arcs[k];
        arcs[k] = (t, s);
    }
}

/// Longest-path ranks over a DAG.
fn assign_ranks(n: usize, arcs: &[(usize, usize)]) -> Vec<usize> {
    let mut indeg = vec![0usize; n];
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(s, t) in arcs {
        indeg[t] += 1;
        out[s].push(t);
    }
    let mut rank = vec![0usize; n];
    let mut queue: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut head = 0;
    while head < queue.len() {
        let v = queue[head];
        head += 1;
        for &t in &out[v] {
            rank[t] = rank[t].max(rank[v] + 1);
            indeg[t] -= 1;
            if indeg[t] == 0 {
                queue.push(t);
            }
        }
    }
    rank
}

/// Group nodes into ranks and reduce crossings with barycenter sweeps.
fn order_ranks(n: usize, arcs: &[(usize, usize)], ranks: &[usize]) -> Vec<Vec<usize>> {
    let max_rank = ranks.iter().copied().max().unwrap_or(0);
    let mut order: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for i in 0..n {
        order[ranks[i]].push(i);
    }
    let (preds, succs) = neighbor_lists(n, arcs);

    let mut pos = vec![0.0f64; n];
    let write_pos = |order: &[Vec<usize>], pos: &mut [f64]| {
        for rank in order {
            for (p, &i) in rank.iter().enumerate() {
                pos[i] = p as f64;
            }
        }
    };
    write_pos(&order, &mut pos);

    for _ in 0..4 {
        for rank in order.iter_mut().skip(1) {
            barycenter_sort(rank, &preds, &pos);
        }
        write_pos(&order, &mut pos);
        for r in (0..order.len().saturating_sub(1)).rev() {
            barycenter_sort(&mut order[r], &succs, &pos);
            write_pos(&order, &mut pos);
        }
    }
    order
}

fn neighbor_lists(n: usize, arcs: &[(usize, usize)]) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut succs: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(s, t) in arcs {
        preds[t].push(s);
        succs[s].push(t);
    }
    (preds, succs)
}

fn barycenter_sort(rank: &mut [usize], neighbors: &[Vec<usize>], pos: &[f64]) {
    let mut keyed: Vec<(f64, usize, usize)> = rank
        .iter()
        .enumerate()
        .map(|(current, &i)| {
            let ns = &neighbors[i];
            let key = if ns.is_empty() {
                current as f64
            } else {
                ns.iter().map(|&p| pos[p]).sum::<f64>() / ns.len() as f64
            };
            (key, current, i)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for (slot, (_, _, i)) in keyed.into_iter().enumerate() {
        rank[slot] = i;
    }
}

/// Move each node of `rank` toward the mean cross-position of its neighbors,
/// enforcing minimum separation and preserving the rank's overall centering.
fn align_rank(
    rank: &[usize],
    neighbors: &[Vec<usize>],
    cross_size: &impl Fn(usize) -> f64,
    gap: f64,
    cross: &mut [f64],
) {
    if rank.is_empty() {
        return;
    }
    let desired: Vec<f64> = rank
        .iter()
        .map(|&i| {
            let ns = &neighbors[i];
            if ns.is_empty() {
                cross[i]
            } else {
                ns.iter().map(|&p| cross[p]).sum::<f64>() / ns.len() as f64
            }
        })
        .collect();
    // Forward pass: place at desired position, pushing right when overlapping
    // the previous node.
    let mut placed: Vec<f64> = Vec::with_capacity(rank.len());
    for (k, &i) in rank.iter().enumerate() {
        let min_pos = if k == 0 {
            f64::NEG_INFINITY
        } else {
            placed[k - 1] + cross_size(rank[k - 1]) / 2.0 + gap + cross_size(i) / 2.0
        };
        placed.push(desired[k].max(min_pos));
    }
    // Re-center so the constrained solution doesn't drift.
    let drift: f64 =
        placed.iter().zip(&desired).map(|(p, d)| p - d).sum::<f64>() / rank.len() as f64;
    for (k, &i) in rank.iter().enumerate() {
        cross[i] = placed[k] - drift;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(ids: &[&str]) -> Vec<LayoutNode> {
        ids.iter()
            .map(|id| LayoutNode {
                id: id.to_string(),
                size: Size::new(100.0, 40.0),
            })
            .collect()
    }

    fn e(pairs: &[(&str, &str)]) -> Vec<(Id, Id)> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn chain_top_to_bottom() {
        let ns = nodes(&["a", "b", "c"]);
        let pos = compute_layout(
            &ns,
            &e(&[("a", "b"), ("b", "c")]),
            &LayoutOptions::default(),
        );
        // Same column, strictly increasing y with at least the rank gap.
        assert_eq!(pos["a"].x, pos["b"].x);
        assert_eq!(pos["b"].x, pos["c"].x);
        assert!(pos["b"].y - pos["a"].y >= 40.0 + 90.0);
        assert!(pos["c"].y - pos["b"].y >= 40.0 + 90.0);
    }

    #[test]
    fn chain_left_to_right() {
        let ns = nodes(&["a", "b"]);
        let opts = LayoutOptions::default().direction(LayoutDirection::LeftToRight);
        let pos = compute_layout(&ns, &e(&[("a", "b")]), &opts);
        assert_eq!(pos["a"].y, pos["b"].y);
        assert!(pos["b"].x - pos["a"].x >= 100.0 + 90.0);
    }

    #[test]
    fn diamond_no_overlap() {
        let ns = nodes(&["a", "b", "c", "d"]);
        let pos = compute_layout(
            &ns,
            &e(&[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")]),
            &LayoutOptions::default(),
        );
        // b and c share a rank and must not overlap.
        assert_eq!(pos["b"].y, pos["c"].y);
        assert!((pos["b"].x - pos["c"].x).abs() >= 100.0 + 50.0);
        // a is centered over the diamond, d below it.
        assert!((pos["a"].x - pos["d"].x).abs() < 1.0);
        assert!(pos["d"].y > pos["b"].y);
    }

    #[test]
    fn cycle_does_not_panic() {
        let ns = nodes(&["a", "b", "c"]);
        let pos = compute_layout(
            &ns,
            &e(&[("a", "b"), ("b", "c"), ("c", "a")]),
            &LayoutOptions::default(),
        );
        assert_eq!(pos.len(), 3);
        // All three land on distinct ranks.
        let mut ys: Vec<i64> = pos.values().map(|p| p.y as i64).collect();
        ys.sort_unstable();
        ys.dedup();
        assert_eq!(ys.len(), 3);
    }

    #[test]
    fn disconnected_and_unknown_edges() {
        let ns = nodes(&["a", "b", "lonely"]);
        let pos = compute_layout(
            &ns,
            &e(&[("a", "b"), ("a", "ghost")]),
            &LayoutOptions::default(),
        );
        assert_eq!(pos.len(), 3);
    }

    #[test]
    fn empty() {
        let pos = compute_layout(&[], &[], &LayoutOptions::default());
        assert!(pos.is_empty());
    }

    #[test]
    fn bottom_to_top_flips() {
        let ns = nodes(&["a", "b"]);
        let opts = LayoutOptions::default().direction(LayoutDirection::BottomToTop);
        let pos = compute_layout(&ns, &e(&[("a", "b")]), &opts);
        assert!(pos["b"].y < pos["a"].y);
    }
}
