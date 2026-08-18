//! Edge path construction: bezier, straight, and smooth-step (orthogonal)
//! paths, plus label anchor points.

use crate::types::{EdgeKind, Point, Rect, Side};

/// Inputs for building an edge path between two handle anchors.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EdgeGeometry {
    pub source: Point,
    pub source_side: Side,
    pub target: Point,
    pub target_side: Side,
    /// Bounds of the source node, when known. Smooth-step routing detours
    /// around these instead of cutting through the node.
    pub source_rect: Option<Rect>,
    /// Bounds of the target node, when known.
    pub target_rect: Option<Rect>,
}

impl EdgeGeometry {
    pub fn new(source: Point, source_side: Side, target: Point, target_side: Side) -> Self {
        Self {
            source,
            source_side,
            target,
            target_side,
            source_rect: None,
            target_rect: None,
        }
    }

    pub fn with_rects(mut self, source_rect: Rect, target_rect: Rect) -> Self {
        self.source_rect = Some(source_rect);
        self.target_rect = Some(target_rect);
        self
    }
}

/// A rendered edge path: the SVG `d` attribute and a label anchor point.
#[derive(Clone, PartialEq, Debug)]
pub struct EdgePath {
    pub d: String,
    pub label: Point,
}

/// Build the path for the given edge kind.
pub fn edge_path(kind: EdgeKind, geo: &EdgeGeometry) -> EdgePath {
    match kind {
        EdgeKind::Bezier => bezier_path(geo, 0.25),
        EdgeKind::Straight => straight_path(geo),
        EdgeKind::SmoothStep => smooth_step_path(geo, 8.0),
    }
}

fn fmt(v: f64) -> f64 {
    // Round to limit path string churn/size; sub-0.01px is invisible.
    (v * 100.0).round() / 100.0
}

/// Distance a bezier control point extends from its anchor. Mirrors
/// react-flow: half the forward distance, or a curvature-scaled pullback when
/// the target lies "behind" the anchor.
fn control_offset(dist: f64, curvature: f64) -> f64 {
    if dist >= 0.0 {
        0.5 * dist
    } else {
        curvature * 25.0 * (-dist).sqrt()
    }
}

fn control_point(p: Point, side: Side, other: Point, curvature: f64) -> Point {
    match side {
        Side::Left => Point::new(p.x - control_offset(p.x - other.x, curvature), p.y),
        Side::Right => Point::new(p.x + control_offset(other.x - p.x, curvature), p.y),
        Side::Top => Point::new(p.x, p.y - control_offset(p.y - other.y, curvature)),
        Side::Bottom => Point::new(p.x, p.y + control_offset(other.y - p.y, curvature)),
    }
}

/// Cubic bezier between the anchors, curving out of each side.
pub fn bezier_path(geo: &EdgeGeometry, curvature: f64) -> EdgePath {
    let s = geo.source;
    let t = geo.target;
    let c1 = control_point(s, geo.source_side, t, curvature);
    let c2 = control_point(t, geo.target_side, s, curvature);
    let d = format!(
        "M{},{} C{},{} {},{} {},{}",
        fmt(s.x),
        fmt(s.y),
        fmt(c1.x),
        fmt(c1.y),
        fmt(c2.x),
        fmt(c2.y),
        fmt(t.x),
        fmt(t.y)
    );
    // Cubic bezier evaluated at t = 0.5.
    let label = Point::new(
        (s.x + 3.0 * c1.x + 3.0 * c2.x + t.x) / 8.0,
        (s.y + 3.0 * c1.y + 3.0 * c2.y + t.y) / 8.0,
    );
    EdgePath { d, label }
}

/// A straight line between the anchors.
pub fn straight_path(geo: &EdgeGeometry) -> EdgePath {
    let s = geo.source;
    let t = geo.target;
    EdgePath {
        d: format!("M{},{} L{},{}", fmt(s.x), fmt(s.y), fmt(t.x), fmt(t.y)),
        label: s.lerp(t, 0.5),
    }
}

/// Orthogonal path with rounded corners.
pub fn smooth_step_path(geo: &EdgeGeometry, radius: f64) -> EdgePath {
    let points = step_points(geo, 20.0);
    let label = polyline_midpoint(&points);
    EdgePath {
        d: rounded_polyline(&points, radius),
        label,
    }
}

/// Waypoints of the orthogonal route, including both anchors. Routes that
/// would double back through a node instead detour around its bounds (grown
/// by the stub length), so back-edges never cut through either endpoint node.
fn step_points(geo: &EdgeGeometry, stub: f64) -> Vec<Point> {
    let s = geo.source;
    let t = geo.target;
    let ss = geo.source_side;
    let ts = geo.target_side;
    let s2 = s + ss.normal() * stub;
    let t2 = t + ts.normal() * stub;
    // Clearance boxes: node bounds grown by the stub so corridors keep stub
    // distance from the nodes. Unknown bounds collapse to the anchor point.
    let sr = inflate(
        geo.source_rect
            .unwrap_or_else(|| Rect::new(s.x, s.y, 0.0, 0.0)),
        stub,
    );
    let tr = inflate(
        geo.target_rect
            .unwrap_or_else(|| Rect::new(t.x, t.y, 0.0, 0.0)),
        stub,
    );

    let mut pts = vec![s, s2];
    match (ss, ts) {
        // Opposite horizontal sides: straight-through when the target lies
        // ahead of the source stub, otherwise around via a clear row.
        (Side::Right, Side::Left) | (Side::Left, Side::Right) => {
            let forward = if ss == Side::Right {
                t2.x >= s2.x
            } else {
                t2.x <= s2.x
            };
            if forward {
                let mid_x = (s2.x + t2.x) / 2.0;
                pts.push(Point::new(mid_x, s2.y));
                pts.push(Point::new(mid_x, t2.y));
            } else {
                let mid_y = clear_lane(sr.y, sr.max_y(), tr.y, tr.max_y(), s2.y, t2.y);
                pts.push(Point::new(s2.x, mid_y));
                pts.push(Point::new(t2.x, mid_y));
            }
        }
        // Opposite vertical sides: mirror of the above.
        (Side::Bottom, Side::Top) | (Side::Top, Side::Bottom) => {
            let forward = if ss == Side::Bottom {
                t2.y >= s2.y
            } else {
                t2.y <= s2.y
            };
            if forward {
                let mid_y = (s2.y + t2.y) / 2.0;
                pts.push(Point::new(s2.x, mid_y));
                pts.push(Point::new(t2.x, mid_y));
            } else {
                let mid_x = clear_lane(sr.x, sr.max_x(), tr.x, tr.max_x(), s2.x, t2.x);
                pts.push(Point::new(mid_x, s2.y));
                pts.push(Point::new(mid_x, t2.y));
            }
        }
        // Same horizontal side: run along the outer column shared by both.
        (Side::Right, Side::Right) => {
            let outer = s2.x.max(t2.x).max(sr.max_x()).max(tr.max_x());
            pts.push(Point::new(outer, s2.y));
            pts.push(Point::new(outer, t2.y));
        }
        (Side::Left, Side::Left) => {
            let outer = s2.x.min(t2.x).min(sr.x).min(tr.x);
            pts.push(Point::new(outer, s2.y));
            pts.push(Point::new(outer, t2.y));
        }
        // Same vertical side.
        (Side::Bottom, Side::Bottom) => {
            let outer = s2.y.max(t2.y).max(sr.max_y()).max(tr.max_y());
            pts.push(Point::new(s2.x, outer));
            pts.push(Point::new(t2.x, outer));
        }
        (Side::Top, Side::Top) => {
            let outer = s2.y.min(t2.y).min(sr.y).min(tr.y);
            pts.push(Point::new(s2.x, outer));
            pts.push(Point::new(t2.x, outer));
        }
        // Horizontal source, vertical target: one corner when it bends
        // forward at both ends, otherwise around the target's clear column.
        (Side::Right | Side::Left, _) => {
            let source_ok = if ss == Side::Right {
                t2.x >= s2.x
            } else {
                t2.x <= s2.x
            };
            let target_ok = if ts == Side::Top {
                s2.y <= t2.y
            } else {
                s2.y >= t2.y
            };
            if source_ok && target_ok {
                pts.push(Point::new(t2.x, s2.y));
            } else {
                let x_out = if ss == Side::Right {
                    s2.x.max(tr.max_x())
                } else {
                    s2.x.min(tr.x)
                };
                pts.push(Point::new(x_out, s2.y));
                pts.push(Point::new(x_out, t2.y));
            }
        }
        // Vertical source, horizontal target: mirror of the above.
        (_, Side::Right | Side::Left) => {
            let source_ok = if ss == Side::Bottom {
                t2.y >= s2.y
            } else {
                t2.y <= s2.y
            };
            let target_ok = if ts == Side::Left {
                s2.x <= t2.x
            } else {
                s2.x >= t2.x
            };
            if source_ok && target_ok {
                pts.push(Point::new(s2.x, t2.y));
            } else {
                let y_out = if ss == Side::Bottom {
                    s2.y.max(tr.max_y())
                } else {
                    s2.y.min(tr.y)
                };
                pts.push(Point::new(s2.x, y_out));
                pts.push(Point::new(t2.x, y_out));
            }
        }
    }
    pts.push(t2);
    pts.push(t);
    simplify_points(pts)
}

fn inflate(r: Rect, m: f64) -> Rect {
    Rect::new(r.x - m, r.y - m, r.width + 2.0 * m, r.height + 2.0 * m)
}

/// A coordinate for a corridor that clears both spans `(a0..a1)` and
/// `(b0..b1)`: the middle of the gap between them when they are disjoint,
/// otherwise just past whichever outer edge is closer to the two stubs.
fn clear_lane(a0: f64, a1: f64, b0: f64, b1: f64, stub_a: f64, stub_b: f64) -> f64 {
    if a1 < b0 {
        return (a1 + b0) / 2.0;
    }
    if b1 < a0 {
        return (b1 + a0) / 2.0;
    }
    let lo = a0.min(b0);
    let hi = a1.max(b1);
    let before = (stub_a - lo) + (stub_b - lo);
    let after = (hi - stub_a) + (hi - stub_b);
    if before <= after {
        lo
    } else {
        hi
    }
}

/// Drop consecutive duplicates and interior points that continue straight,
/// so corners in the output are genuine turns.
fn simplify_points(pts: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    for p in pts {
        if out.last().map(|l| l.distance(p) > 0.01).unwrap_or(true) {
            out.push(p);
        }
    }
    let mut i = 1;
    while i + 1 < out.len() {
        let ab = out[i] - out[i - 1];
        let bc = out[i + 1] - out[i];
        let cross = ab.x * bc.y - ab.y * bc.x;
        let dot = ab.x * bc.x + ab.y * bc.y;
        if cross.abs() < 1e-6 && dot > 0.0 {
            out.remove(i);
        } else {
            i += 1;
        }
    }
    out
}

/// Build an SVG path from a polyline, rounding interior corners with
/// quadratic curves.
fn rounded_polyline(pts: &[Point], radius: f64) -> String {
    if pts.is_empty() {
        return String::new();
    }
    let mut d = format!("M{},{}", fmt(pts[0].x), fmt(pts[0].y));
    for i in 1..pts.len().saturating_sub(1) {
        let prev = pts[i - 1];
        let p = pts[i];
        let next = pts[i + 1];
        let len_in = prev.distance(p);
        let len_out = p.distance(next);
        // A collinear point (straight continuation or 180° reversal) has no
        // corner to round; rounding it would emit a zero-length curve.
        let ab = p - prev;
        let bc = next - p;
        let collinear = (ab.x * bc.y - ab.y * bc.x).abs() < 1e-6;
        let r = radius.min(len_in / 2.0).min(len_out / 2.0);
        if r < 0.1 || collinear {
            d.push_str(&format!(" L{},{}", fmt(p.x), fmt(p.y)));
            continue;
        }
        let a = p + (prev - p) * (r / len_in);
        let b = p + (next - p) * (r / len_out);
        d.push_str(&format!(
            " L{},{} Q{},{} {},{}",
            fmt(a.x),
            fmt(a.y),
            fmt(p.x),
            fmt(p.y),
            fmt(b.x),
            fmt(b.y)
        ));
    }
    if let Some(last) = pts.last() {
        if pts.len() > 1 {
            d.push_str(&format!(" L{},{}", fmt(last.x), fmt(last.y)));
        }
    }
    d
}

/// The point halfway along a polyline (by arc length).
fn polyline_midpoint(pts: &[Point]) -> Point {
    if pts.is_empty() {
        return Point::ZERO;
    }
    let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    if total <= f64::EPSILON {
        return pts[0];
    }
    let mut remaining = total / 2.0;
    for w in pts.windows(2) {
        let len = w[0].distance(w[1]);
        if remaining <= len {
            return w[0].lerp(w[1], remaining / len);
        }
        remaining -= len;
    }
    *pts.last().unwrap()
}

/// Path used for the in-progress connection line (a bezier from the source
/// handle toward the cursor / snap target).
pub fn connection_path(from: Point, from_side: Side, to: Point, to_side: Option<Side>) -> String {
    let geo = EdgeGeometry::new(
        from,
        from_side,
        to,
        to_side.unwrap_or_else(|| from_side.opposite()),
    );
    bezier_path(&geo, 0.25).d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geo(s: (f64, f64), ss: Side, t: (f64, f64), ts: Side) -> EdgeGeometry {
        EdgeGeometry::new(s.into(), ss, t.into(), ts)
    }

    fn assert_no_reversals(pts: &[Point]) {
        for w in pts.windows(3) {
            let ab = w[1] - w[0];
            let bc = w[2] - w[1];
            let cross = ab.x * bc.y - ab.y * bc.x;
            let dot = ab.x * bc.x + ab.y * bc.y;
            assert!(
                cross.abs() > 1e-6 || dot > 0.0,
                "route doubles back at {:?} in {:?}",
                w[1],
                pts
            );
        }
    }

    fn strictly_inside(r: &Rect, p: Point) -> bool {
        p.x > r.x && p.x < r.max_x() && p.y > r.y && p.y < r.max_y()
    }

    #[test]
    fn bezier_endpoints() {
        let g = geo((0.0, 0.0), Side::Bottom, (100.0, 200.0), Side::Top);
        let p = bezier_path(&g, 0.25);
        assert!(p.d.starts_with("M0,0 C"));
        assert!(p.d.ends_with("100,200"));
        // Label sits between the endpoints.
        assert!(p.label.y > 0.0 && p.label.y < 200.0);
        assert_eq!(p.label.x, 50.0);
    }

    #[test]
    fn straight_midpoint() {
        let g = geo((0.0, 0.0), Side::Right, (10.0, 10.0), Side::Left);
        let p = straight_path(&g);
        assert_eq!(p.label, Point::new(5.0, 5.0));
    }

    #[test]
    fn smooth_step_valid_path() {
        let g = geo((0.0, 0.0), Side::Right, (200.0, 100.0), Side::Left);
        let p = smooth_step_path(&g, 8.0);
        assert!(p.d.starts_with("M0,0"));
        assert!(p.d.ends_with("L200,100"));
        assert!(p.d.contains('Q'), "expected rounded corners: {}", p.d);
    }

    #[test]
    fn smooth_step_mixed_sides() {
        let g = geo((0.0, 0.0), Side::Bottom, (200.0, 100.0), Side::Left);
        let p = smooth_step_path(&g, 8.0);
        assert!(p.d.starts_with("M0,0"));
        assert!(p.d.ends_with("L200,100"));
    }

    #[test]
    fn smooth_step_degenerate_same_point() {
        let g = geo((50.0, 50.0), Side::Right, (50.0, 50.0), Side::Left);
        let p = smooth_step_path(&g, 8.0);
        assert!(p.d.starts_with("M50,50"));
    }

    #[test]
    fn smooth_step_back_edge_detours_horizontally() {
        // Left-to-right layout, feedback edge: target sits behind the source
        // on the same row. The route must leave through a clear lane above or
        // below both nodes instead of cutting back through them.
        let source_rect = Rect::new(400.0, 80.0, 160.0, 56.0);
        let target_rect = Rect::new(100.0, 80.0, 160.0, 56.0);
        let g = geo((560.0, 108.0), Side::Right, (100.0, 108.0), Side::Left)
            .with_rects(source_rect, target_rect);
        let pts = step_points(&g, 20.0);
        assert_no_reversals(&pts);
        for p in &pts {
            assert!(
                !strictly_inside(&source_rect, *p) && !strictly_inside(&target_rect, *p),
                "waypoint {p:?} crosses a node in {pts:?}"
            );
        }
        assert!(
            pts.iter().any(|p| p.y <= 60.0 || p.y >= 156.0),
            "no clear lane in {pts:?}"
        );
    }

    #[test]
    fn smooth_step_back_edge_detours_vertically() {
        // Vertical stack, feedback edge from the lower node's bottom to the
        // upper node's top: must loop around the side, not overlap the
        // forward edge's column.
        let target_rect = Rect::new(160.0, 450.0, 180.0, 56.0);
        let source_rect = Rect::new(160.0, 600.0, 180.0, 56.0);
        let g = geo((250.0, 656.0), Side::Bottom, (250.0, 450.0), Side::Top)
            .with_rects(source_rect, target_rect);
        let pts = step_points(&g, 20.0);
        assert_no_reversals(&pts);
        for p in &pts {
            assert!(
                !strictly_inside(&source_rect, *p) && !strictly_inside(&target_rect, *p),
                "waypoint {p:?} crosses a node in {pts:?}"
            );
        }
        assert!(
            pts.iter().any(|p| p.x <= 140.0 || p.x >= 360.0),
            "route never left the forward edge's column: {pts:?}"
        );
    }

    #[test]
    fn smooth_step_same_side_routes_outside_both() {
        let source_rect = Rect::new(0.0, -20.0, 100.0, 40.0);
        let target_rect = Rect::new(200.0, 180.0, 100.0, 40.0);
        let g = geo((100.0, 0.0), Side::Right, (300.0, 200.0), Side::Right)
            .with_rects(source_rect, target_rect);
        let pts = step_points(&g, 20.0);
        assert_no_reversals(&pts);
        let max_x = pts.iter().fold(f64::MIN, |m, p| m.max(p.x));
        assert!(max_x >= 320.0, "same-side route stayed inside: {pts:?}");
        assert_eq!(*pts.last().unwrap(), Point::new(300.0, 200.0));
    }
}
