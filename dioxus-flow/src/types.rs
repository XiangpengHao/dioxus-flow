//! Core geometry and graph types.

use std::ops::{Add, Div, Mul, Sub};

/// Identifier for nodes, edges and handles.
pub type Id = String;

/// A point (or vector) in 2D space. Flow coordinates unless stated otherwise.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_sq(&self, other: Point) -> f64 {
        let d = *self - other;
        d.x * d.x + d.y * d.y
    }

    pub fn distance(&self, other: Point) -> f64 {
        self.distance_sq(other).sqrt()
    }

    pub fn lerp(&self, other: Point, t: f64) -> Point {
        Point::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

impl From<(f64, f64)> for Point {
    fn from((x, y): (f64, f64)) -> Self {
        Point::new(x, y)
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Point {
    type Output = Point;
    fn mul(self, rhs: f64) -> Point {
        Point::new(self.x * rhs, self.y * rhs)
    }
}

impl Div<f64> for Point {
    type Output = Point;
    fn div(self, rhs: f64) -> Point {
        Point::new(self.x / rhs, self.y / rhs)
    }
}

/// A width/height pair.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

impl From<(f64, f64)> for Size {
    fn from((width, height): (f64, f64)) -> Self {
        Size::new(width, height)
    }
}

/// An axis-aligned rectangle.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_points(origin: Point, size: Size) -> Self {
        Self::new(origin.x, origin.y, size.width, size.height)
    }

    /// The rectangle two corners describe, in any order — a marquee, a
    /// selection bounds, a drag rectangle.
    pub fn between(a: Point, b: Point) -> Self {
        Self {
            x: a.x.min(b.x),
            y: a.y.min(b.y),
            width: (a.x - b.x).abs(),
            height: (a.y - b.y).abs(),
        }
    }

    pub fn origin(&self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }

    pub fn max_y(&self) -> f64 {
        self.y + self.height
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(
            x,
            y,
            self.max_x().max(other.max_x()) - x,
            self.max_y().max(other.max_y()) - y,
        )
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.max_x() && p.y >= self.y && p.y <= self.max_y()
    }

    /// Whether the rectangles touch or overlap (touching counts).
    pub fn intersects(&self, other: Rect) -> bool {
        self.x <= other.max_x()
            && self.max_x() >= other.x
            && self.y <= other.max_y()
            && self.max_y() >= other.y
    }

    /// The same rectangle with `by` of room added on every side.
    pub fn expanded(&self, by: f64) -> Rect {
        Rect::new(
            self.x - by,
            self.y - by,
            self.width + by * 2.0,
            self.height + by * 2.0,
        )
    }

    /// How far a point is from this rectangle's outline: negative inside,
    /// positive outside, zero exactly on it.
    ///
    /// A band around the outline is then one comparison rather than four, and
    /// it can straddle the line — which is what a border a person can grab has
    /// to do, because the line they are aiming at has no thickness.
    pub fn distance_to_edge(&self, point: Point) -> f64 {
        let dx = (self.x - point.x).max(point.x - self.max_x());
        let dy = (self.y - point.y).max(point.y - self.max_y());
        if dx > 0.0 || dy > 0.0 {
            dx.max(0.0).hypot(dy.max(0.0))
        } else {
            dx.max(dy)
        }
    }

    /// The rectangle covering all of `rects`, or `None` when there are none.
    pub fn bounds(rects: impl IntoIterator<Item = Self>) -> Option<Self> {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for rect in rects {
            min_x = min_x.min(rect.x);
            min_y = min_y.min(rect.y);
            max_x = max_x.max(rect.max_x());
            max_y = max_y.max(rect.max_y());
        }
        min_x
            .is_finite()
            .then_some(Self::new(min_x, min_y, max_x - min_x, max_y - min_y))
    }
}

/// The lattice positions come to rest on. Snapping is the plane's business —
/// the background draws this grid, and a release lands on it — while what may
/// occupy a cell is the application's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    size: f64,
}

impl Grid {
    /// A grid of `size` units. A size that is not a positive number would make
    /// every snap meaningless, so it falls back to one unit.
    pub const fn new(size: f64) -> Self {
        Self {
            size: if size.is_finite() && size > 0.0 {
                size
            } else {
                1.0
            },
        }
    }

    pub const fn size(self) -> f64 {
        self.size
    }

    pub fn snap(self, value: f64) -> f64 {
        (value / self.size).round() * self.size
    }

    /// Snapped away from zero, for a measurement that must not be cut short —
    /// a node grown to fit its text, say.
    pub fn snap_up(self, value: f64) -> f64 {
        (value / self.size).ceil() * self.size
    }

    pub fn snap_point(self, point: Point) -> Point {
        Point::new(self.snap(point.x), self.snap(point.y))
    }

    pub fn snap_rect(self, rect: Rect) -> Rect {
        Rect::new(
            self.snap(rect.x),
            self.snap(rect.y),
            self.snap(rect.width),
            self.snap(rect.height),
        )
    }
}

/// The pan/zoom state of the flow canvas.
///
/// `x`/`y` are the screen point (relative to the container) that the flow
/// origin is drawn at, so a flow-space point `p` appears on screen at
/// `p * zoom + offset()`. The same shape — and, with the `serde` feature, the
/// same serialization — as react-flow's viewport.
#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Viewport {
    /// Horizontal translation, in screen pixels.
    pub x: f64,
    /// Vertical translation, in screen pixels.
    pub y: f64,
    /// Zoom factor.
    pub zoom: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

impl Viewport {
    pub const fn new(x: f64, y: f64, zoom: f64) -> Self {
        Self { x, y, zoom }
    }

    /// The translation as a point.
    pub const fn offset(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// The same viewport translated to `offset`.
    pub const fn with_offset(self, offset: Point) -> Self {
        Self {
            x: offset.x,
            y: offset.y,
            zoom: self.zoom,
        }
    }

    /// Convert a point relative to the container (screen pixels) to flow space.
    pub fn screen_to_flow(&self, p: Point) -> Point {
        (p - self.offset()) / self.zoom
    }

    /// Convert a flow-space point to screen pixels relative to the container.
    pub fn flow_to_screen(&self, p: Point) -> Point {
        p * self.zoom + self.offset()
    }

    /// Zooms about a screen point, so whatever is under it stays under it.
    ///
    /// A zoom that is not a number is refused rather than clamped — `f64::clamp`
    /// keeps a NaN, and a NaN here would spread to every coordinate drawn from
    /// this viewport, with nothing left on screen to say why.
    pub fn zoom_about(self, zoom: f64, screen: Point, min_zoom: f64, max_zoom: f64) -> Self {
        if !zoom.is_finite() {
            return self;
        }
        let zoom = zoom.clamp(min_zoom, max_zoom);
        let flow = self.screen_to_flow(screen);
        Self {
            x: screen.x - flow.x * zoom,
            y: screen.y - flow.y * zoom,
            zoom,
        }
    }

    /// Pans by a screen-space delta.
    pub fn panned(self, by: Point) -> Self {
        Self {
            x: self.x + by.x,
            y: self.y + by.y,
            ..self
        }
    }

    /// Whether this could have come from an editor rather than from a corrupt
    /// or hand-edited file: every coordinate a number, and the zoom within the
    /// given limits.
    pub fn is_sane(self, min_zoom: f64, max_zoom: f64) -> bool {
        self.x.is_finite() && self.y.is_finite() && (min_zoom..=max_zoom).contains(&self.zoom)
    }

    /// Frames `drawing` inside `safe` — the part of the screen that is actually
    /// clear, which is not the whole of it when panels float over the edges. An
    /// empty drawing centres the origin instead, so a fresh canvas opens with
    /// room on every side. The zoom is clamped to `[min_zoom, max_zoom]`; pass
    /// a `max_zoom` below the interactive limit so fitting a single small node
    /// never magnifies it to fill the screen.
    pub fn fit(drawing: Option<Rect>, safe: Rect, min_zoom: f64, max_zoom: f64) -> Self {
        let Some(drawing) = drawing else {
            return Self {
                x: safe.x + safe.width / 2.0,
                y: safe.y + safe.height / 2.0,
                zoom: 1.0,
            };
        };
        let width = drawing.width.max(1.0);
        let height = drawing.height.max(1.0);
        let wanted = (safe.width / width).min(safe.height / height);
        let zoom = if wanted.is_finite() {
            wanted.clamp(min_zoom, max_zoom)
        } else {
            1.0
        };
        Self {
            x: safe.x + (safe.width - width * zoom) / 2.0 - drawing.x * zoom,
            y: safe.y + (safe.height - height * zoom) / 2.0 - drawing.y * zoom,
            zoom,
        }
    }

    pub fn lerp(&self, other: &Viewport, t: f64) -> Viewport {
        Viewport {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
            zoom: self.zoom + (other.zoom - self.zoom) * t,
        }
    }
}

/// A side of a node, used for handle placement, seat naming and edge routing.
///
/// For port seats: `Top` and `Bottom` count their cells from the left corner,
/// `Left` and `Right` from the top corner. That common origin is what makes a
/// seat survive a resize — growing a node rightwards or downwards moves only
/// the edge being dragged, so every seat that is not on it keeps its place.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    /// Outward unit vector of this side.
    pub fn normal(&self) -> Point {
        match self {
            Side::Top => Point::new(0.0, -1.0),
            Side::Right => Point::new(1.0, 0.0),
            Side::Bottom => Point::new(0.0, 1.0),
            Side::Left => Point::new(-1.0, 0.0),
        }
    }

    pub fn opposite(&self) -> Side {
        match self {
            Side::Top => Side::Bottom,
            Side::Right => Side::Left,
            Side::Bottom => Side::Top,
            Side::Left => Side::Right,
        }
    }

    pub fn is_horizontal(&self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }

    pub(crate) fn class_name(&self) -> &'static str {
        match self {
            Side::Top => "top",
            Side::Right => "right",
            Side::Bottom => "bottom",
            Side::Left => "left",
        }
    }
}

/// The point on the boundary of `rect` at fraction `frac` (0..=1) along `side`.
pub fn side_point(rect: &Rect, side: Side, frac: f64) -> Point {
    match side {
        Side::Top => Point::new(rect.x + rect.width * frac, rect.y),
        Side::Bottom => Point::new(rect.x + rect.width * frac, rect.max_y()),
        Side::Left => Point::new(rect.x, rect.y + rect.height * frac),
        Side::Right => Point::new(rect.max_x(), rect.y + rect.height * frac),
    }
}

/// Fallback node size used before a node has been measured.
pub(crate) const DEFAULT_NODE_SIZE: Size = Size {
    width: 150.0,
    height: 40.0,
};

/// A node in the flow graph.
///
/// `T` is a user-defined payload available to custom node renderers.
#[derive(Clone, PartialEq, Debug)]
pub struct Node<T = ()> {
    pub id: Id,
    /// Position of the node's top-left corner, in flow coordinates.
    pub position: Point,
    /// Label rendered by the default node view.
    pub label: String,
    /// Custom payload for custom node views.
    pub data: T,
    /// Node type tag. The default view recognizes `"input"` (source handle
    /// only) and `"output"` (target handle only); custom views can match on
    /// any value.
    pub node_type: Option<String>,
    /// Side where outgoing edges leave (used by the default view's source
    /// handle and as the anchor fallback).
    pub source_side: Side,
    /// Side where incoming edges arrive.
    pub target_side: Side,
    pub draggable: bool,
    pub selectable: bool,
    pub selected: bool,
    /// Explicit size. When `None` (default) the node sizes to its content and
    /// is measured automatically.
    pub size: Option<Size>,
    /// Measured size, maintained by the framework.
    pub measured: Option<Size>,
    /// Extra CSS classes for the node wrapper (e.g. Tailwind utilities).
    pub class: Option<String>,
    /// Extra inline CSS for the node wrapper.
    pub style: Option<String>,
}

impl Node<()> {
    /// Create a node with the default (unit) payload.
    pub fn new(id: impl Into<Id>, label: impl Into<String>, position: impl Into<Point>) -> Self {
        Self::with_data(id, label, position, ())
    }
}

impl<T> Node<T> {
    /// Create a node carrying a custom payload.
    pub fn with_data(
        id: impl Into<Id>,
        label: impl Into<String>,
        position: impl Into<Point>,
        data: T,
    ) -> Self {
        Self {
            id: id.into(),
            position: position.into(),
            label: label.into(),
            data,
            node_type: None,
            source_side: Side::Bottom,
            target_side: Side::Top,
            draggable: true,
            selectable: true,
            selected: false,
            size: None,
            measured: None,
            class: None,
            style: None,
        }
    }

    pub fn node_type(mut self, ty: impl Into<String>) -> Self {
        self.node_type = Some(ty.into());
        self
    }

    pub fn size(mut self, size: impl Into<Size>) -> Self {
        self.size = Some(size.into());
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }

    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    pub fn draggable(mut self, draggable: bool) -> Self {
        self.draggable = draggable;
        self
    }

    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// Set both handle sides at once, e.g. for horizontal layouts
    /// `sides(Side::Left, Side::Right)` (incoming left, outgoing right).
    pub fn sides(mut self, target: Side, source: Side) -> Self {
        self.target_side = target;
        self.source_side = source;
        self
    }

    /// The node's current rectangle (explicit size, else measured, else a
    /// default estimate).
    pub fn rect(&self) -> Rect {
        let size = self.size.or(self.measured).unwrap_or(DEFAULT_NODE_SIZE);
        Rect::from_points(self.position, size)
    }
}

/// How an edge's path is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EdgeKind {
    /// A cubic bezier curve (the classic react-flow default).
    #[default]
    Bezier,
    /// A straight line.
    Straight,
    /// An orthogonal path with rounded corners.
    SmoothStep,
}

/// Arrowhead marker at the end of an edge.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MarkerKind {
    #[default]
    ArrowClosed,
    Arrow,
    None,
}

/// How a [`crate::Flow`]'s edges find their endpoints on nodes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AnchorMode {
    /// Edges attach to [`crate::Handle`]s (or a node's default side when the
    /// edge names none). The react-flow model, and the default.
    #[default]
    Handles,
    /// Edges attach to *seats* — discrete positions packed around each node's
    /// rounded rim by [`crate::ports::solve_ports`]. Ends the user pinned
    /// (via [`Edge::source_seat`]/[`Edge::target_seat`]) stay put; free ends
    /// are placed nearest their partner, deterministically. Edges are drawn
    /// with rim-aware curves and a bead where they meet the node.
    Seats,
}

/// An edge connecting two nodes.
#[derive(Clone, PartialEq, Debug)]
pub struct Edge {
    pub id: Id,
    pub source: Id,
    pub target: Id,
    /// Optional id of a specific source [`crate::Handle`] on the source node.
    pub source_handle: Option<Id>,
    /// Optional id of a specific target [`crate::Handle`] on the target node.
    pub target_handle: Option<Id>,
    /// Under [`AnchorMode::Seats`]: the seat this end is pinned to. `None`
    /// leaves the end to the solver, which is where every connection starts.
    pub source_seat: Option<crate::ports::PortSeat>,
    /// See [`Edge::source_seat`].
    pub target_seat: Option<crate::ports::PortSeat>,
    pub label: Option<String>,
    /// Where the label sits along the edge, as a fraction of the curve
    /// (clamped to the drawable range). Seat-anchored edges honour it; the
    /// default is the midpoint.
    pub label_position: f64,
    /// Stroke emphasis, 1–3. Seat-anchored arrowheads scale with it, so a
    /// heavier edge carries a proportionate head. Styling the stroke itself
    /// stays in `style`/CSS.
    pub weight: u8,
    pub kind: EdgeKind,
    /// Animated edges render a marching-dashes effect.
    pub animated: bool,
    pub selected: bool,
    pub selectable: bool,
    pub marker_start: MarkerKind,
    pub marker_end: MarkerKind,
    /// Extra CSS classes for the edge group.
    pub class: Option<String>,
    /// Extra inline CSS for the visible edge path (e.g. `stroke: #f43f5e`).
    pub style: Option<String>,
}

impl Edge {
    /// Create an edge from `source` to `target` with an autogenerated id.
    pub fn new(source: impl Into<Id>, target: impl Into<Id>) -> Self {
        let source = source.into();
        let target = target.into();
        Self {
            id: format!("{source}->{target}"),
            source,
            target,
            source_handle: None,
            target_handle: None,
            source_seat: None,
            target_seat: None,
            label: None,
            label_position: 0.5,
            weight: 2,
            kind: EdgeKind::default(),
            animated: false,
            selected: false,
            selectable: true,
            marker_start: MarkerKind::None,
            marker_end: MarkerKind::default(),
            class: None,
            style: None,
        }
    }

    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = id.into();
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Place the label at `position` (0..=1) along the curve.
    pub fn label_position(mut self, position: f64) -> Self {
        self.label_position = position;
        self
    }

    /// Stroke emphasis, 1–3; seat-anchored arrowheads scale with it.
    pub fn weight(mut self, weight: u8) -> Self {
        self.weight = weight;
        self
    }

    pub fn kind(mut self, kind: EdgeKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }

    pub fn marker_start(mut self, marker: MarkerKind) -> Self {
        self.marker_start = marker;
        self
    }

    pub fn marker_end(mut self, marker: MarkerKind) -> Self {
        self.marker_end = marker;
        self
    }

    /// Pin this edge's source end to a seat (used under [`AnchorMode::Seats`]).
    pub fn source_seat(mut self, seat: crate::ports::PortSeat) -> Self {
        self.source_seat = Some(seat);
        self
    }

    /// Pin this edge's target end to a seat (used under [`AnchorMode::Seats`]).
    pub fn target_seat(mut self, seat: crate::ports::PortSeat) -> Self {
        self.target_seat = Some(seat);
        self
    }

    pub fn source_handle(mut self, id: impl Into<Id>) -> Self {
        self.source_handle = Some(id.into());
        self
    }

    pub fn target_handle(mut self, id: impl Into<Id>) -> Self {
        self.target_handle = Some(id.into());
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }

    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }
}

/// A pending or completed connection between two handles.
#[derive(Clone, PartialEq, Debug)]
pub struct Connection {
    pub source: Id,
    pub target: Id,
    pub source_handle: Option<Id>,
    pub target_handle: Option<Id>,
}

/// How a connection gesture ended, whatever it ended on. Passed to
/// `on_connect_end`; `connection` is `None` when the drag was released over
/// nothing — the hook for "drop on empty canvas to create a node there".
#[derive(Clone, PartialEq, Debug)]
pub struct ConnectEnd {
    /// Where the pointer let go, in flow coordinates.
    pub point: Point,
    /// The connection that completed, if the release was on (or snapped to) a
    /// compatible handle.
    pub connection: Option<Connection>,
}

/// What a Delete/Backspace press would remove: the selected nodes, plus
/// edges that are selected or touch a selected node. Passed to `Flow`'s
/// `on_delete` so apps can confirm, snapshot for undo, or veto.
#[derive(Clone, PartialEq, Debug)]
pub struct DeleteRequest {
    pub nodes: Vec<Id>,
    pub edges: Vec<Id>,
}

impl Connection {
    /// Build the default edge for this connection.
    pub fn into_edge(self) -> Edge {
        let mut edge = Edge::new(self.source, self.target);
        edge.source_handle = self.source_handle;
        edge.target_handle = self.target_handle;
        // Disambiguate the autogenerated id if specific handles are involved.
        if let Some(h) = &edge.source_handle {
            edge.id = format!("{}#{}", edge.id, h);
        }
        if let Some(h) = &edge.target_handle {
            edge.id = format!("{}#{}", edge.id, h);
        }
        edge
    }
}

/// Whether a handle is a source (edges start here) or a target (edges end
/// here).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HandleKind {
    Source,
    Target,
}

/// Uniquely identifies a handle within a flow.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct HandleKey {
    pub node: Id,
    pub kind: HandleKind,
    /// User handle id; empty string for the node's default handle of that
    /// kind.
    pub id: Id,
}

/// Geometry of a registered handle, relative to its node.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct HandleGeom {
    pub side: Side,
    /// Fraction (0..=1) along the side.
    pub offset: f64,
}

/// Lightweight per-node geometry snapshot used by edges, minimap and
/// fit-view. Derived reactively from the node list.
#[derive(Clone, PartialEq, Debug)]
pub struct NodeGeom {
    pub id: Id,
    pub rect: Rect,
    pub selected: bool,
    pub source_side: Side,
    pub target_side: Side,
    /// Whether `rect` reflects a real (measured or explicit) size.
    pub measured: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_roundtrip() {
        let vp = Viewport::new(13.0, -7.0, 1.7);
        let p = Point::new(100.0, 250.0);
        let q = vp.screen_to_flow(vp.flow_to_screen(p));
        assert!(p.distance(q) < 1e-9);
    }

    /// The property that makes zooming feel like zooming: the thing under the
    /// pointer does not move.
    #[test]
    fn zooming_holds_the_point_it_is_given() {
        let vp = Viewport::new(137.0, -42.0, 1.4);
        let at = Point::new(400.0, 300.0);
        for zoom in [0.2, 0.5, 1.0, 1.7, 3.0, 12.0] {
            let before = vp.screen_to_flow(at);
            let after = vp.zoom_about(zoom, at, 0.2, 3.0).screen_to_flow(at);
            assert!(
                before.distance(after) < 1e-9,
                "zoom {zoom} moved the canvas under the pointer",
            );
        }
    }

    #[test]
    fn zoom_stays_within_its_limits() {
        let vp = Viewport::new(137.0, -42.0, 1.4);
        let at = Point::ZERO;
        assert_eq!(vp.zoom_about(99.0, at, 0.2, 3.0).zoom, 3.0);
        assert_eq!(vp.zoom_about(0.0, at, 0.2, 3.0).zoom, 0.2);
        // Refused, not clamped: a NaN would otherwise survive the clamp.
        assert_eq!(vp.zoom_about(f64::NAN, at, 0.2, 3.0), vp);
        assert_eq!(vp.zoom_about(f64::INFINITY, at, 0.2, 3.0), vp);
    }

    #[test]
    fn a_viewport_from_outside_is_only_trusted_when_it_makes_sense() {
        assert!(Viewport::new(137.0, -42.0, 1.4).is_sane(0.2, 3.0));
        assert!(Viewport::default().is_sane(0.2, 3.0));
        for broken in [
            Viewport::new(f64::NAN, 0.0, 1.0),
            Viewport::new(0.0, f64::INFINITY, 1.0),
            Viewport::new(0.0, 0.0, 0.0),
            Viewport::new(0.0, 0.0, 6.0),
            Viewport::new(0.0, 0.0, f64::NAN),
        ] {
            assert!(!broken.is_sane(0.2, 3.0), "{broken:?}");
        }
    }

    #[test]
    fn fitting_puts_the_whole_drawing_inside_the_clear_area() {
        let safe = Rect::new(32.0, 32.0, 1216.0, 560.0);
        for drawing in [
            Rect::new(-400.0, -300.0, 900.0, 600.0),
            Rect::new(0.0, 0.0, 60.0, 40.0),
            Rect::new(1000.0, 1000.0, 4000.0, 200.0),
        ] {
            let fitted = Viewport::fit(Some(drawing), safe, 0.2, 1.35);
            let top_left = fitted.flow_to_screen(drawing.origin());
            let bottom_right = fitted.flow_to_screen(Point::new(drawing.max_x(), drawing.max_y()));
            let slack = 1e-6;
            assert!(
                top_left.x >= safe.x - slack && top_left.y >= safe.y - slack,
                "{drawing:?} starts outside the clear area",
            );
            assert!(
                bottom_right.x <= safe.max_x() + slack && bottom_right.y <= safe.max_y() + slack,
                "{drawing:?} runs past the clear area",
            );
            assert!((0.2..=1.35).contains(&fitted.zoom));
        }
    }

    #[test]
    fn fitting_a_nonsense_drawing_still_gives_a_usable_view() {
        let safe = Rect::new(0.0, 0.0, 800.0, 600.0);
        for drawing in [
            Rect::new(0.0, 0.0, f64::NAN, 10.0),
            Rect::new(0.0, 0.0, 0.0, 0.0),
            Rect::new(f64::NAN, 0.0, 10.0, 10.0),
        ] {
            let fitted = Viewport::fit(Some(drawing), safe, 0.2, 1.35);
            assert!(fitted.zoom.is_finite() && fitted.zoom > 0.0, "{drawing:?}");
        }
    }

    #[test]
    fn fitting_nothing_centres_the_origin() {
        let safe = Rect::new(0.0, 0.0, 800.0, 600.0);
        let fitted = Viewport::fit(None, safe, 0.2, 1.35);
        assert_eq!(fitted.flow_to_screen(Point::ZERO), Point::new(400.0, 300.0));
        assert_eq!(fitted.zoom, 1.0);
    }

    #[test]
    fn a_grid_snaps_both_ways_and_survives_a_nonsense_size() {
        let grid = Grid::new(12.0);
        assert_eq!(grid.snap(17.0), 12.0);
        assert_eq!(grid.snap(19.0), 24.0);
        assert_eq!(grid.snap(-17.0), -12.0);
        assert_eq!(grid.snap_up(13.0), 24.0);
        assert_eq!(grid.snap_up(24.0), 24.0);
        for size in [0.0, -12.0, f64::NAN, f64::INFINITY] {
            assert_eq!(Grid::new(size).size(), 1.0, "size {size}");
        }
    }

    #[test]
    fn a_rectangle_between_two_corners_is_the_same_whichever_corner_comes_first() {
        let a = Point::new(30.0, -10.0);
        let b = Point::new(-6.0, 22.0);
        assert_eq!(Rect::between(a, b), Rect::between(b, a));
        assert_eq!(Rect::between(a, b), Rect::new(-6.0, -10.0, 36.0, 32.0));
    }

    #[test]
    fn bounds_covers_every_rect_or_nothing_at_all() {
        assert_eq!(Rect::bounds([]), None);
        let rects = [
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Rect::new(-5.0, 20.0, 5.0, 5.0),
        ];
        assert_eq!(Rect::bounds(rects), Some(Rect::new(-5.0, 0.0, 15.0, 25.0)));
    }

    #[test]
    fn distance_to_edge_is_signed() {
        let r = Rect::new(0.0, 0.0, 100.0, 50.0);
        assert!(r.distance_to_edge(Point::new(50.0, 25.0)) < 0.0);
        assert_eq!(r.distance_to_edge(Point::new(0.0, 25.0)), 0.0);
        assert!(r.distance_to_edge(Point::new(110.0, 25.0)) > 0.0);
    }

    #[test]
    fn side_points() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(side_point(&r, Side::Top, 0.5), Point::new(60.0, 20.0));
        assert_eq!(side_point(&r, Side::Bottom, 0.5), Point::new(60.0, 70.0));
        assert_eq!(side_point(&r, Side::Left, 0.5), Point::new(10.0, 45.0));
        assert_eq!(side_point(&r, Side::Right, 0.25), Point::new(110.0, 32.5));
    }

    #[test]
    fn rect_union() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(20.0, -5.0, 10.0, 10.0);
        let u = a.union(&b);
        assert_eq!(u, Rect::new(0.0, -5.0, 30.0, 15.0));
    }
}
