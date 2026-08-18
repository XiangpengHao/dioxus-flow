//! Core geometry and graph types.

use std::ops::{Add, Div, Mul, Sub};

/// Identifier for nodes, edges and handles.
pub type Id = String;

/// A point (or vector) in 2D space. Flow coordinates unless stated otherwise.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub fn new(x: f64, y: f64) -> Self {
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

    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
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
}

/// The pan/zoom state of the flow canvas.
///
/// A flow-space point `p` appears on screen (relative to the container) at
/// `p * zoom + offset`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Viewport {
    /// Translation, in screen pixels.
    pub offset: Point,
    /// Zoom factor.
    pub zoom: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            offset: Point::ZERO,
            zoom: 1.0,
        }
    }
}

impl Viewport {
    /// Convert a point relative to the container (screen pixels) to flow space.
    pub fn screen_to_flow(&self, p: Point) -> Point {
        (p - self.offset) / self.zoom
    }

    /// Convert a flow-space point to screen pixels relative to the container.
    pub fn flow_to_screen(&self, p: Point) -> Point {
        p * self.zoom + self.offset
    }

    pub fn lerp(&self, other: &Viewport, t: f64) -> Viewport {
        Viewport {
            offset: self.offset.lerp(other.offset, t),
            zoom: self.zoom + (other.zoom - self.zoom) * t,
        }
    }
}

/// A side of a node, used for handle placement and edge routing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
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

/// Arrowhead marker at the target end of an edge.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MarkerKind {
    #[default]
    ArrowClosed,
    Arrow,
    None,
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
    pub label: Option<String>,
    pub kind: EdgeKind,
    /// Animated edges render a marching-dashes effect.
    pub animated: bool,
    pub selected: bool,
    pub selectable: bool,
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
            label: None,
            kind: EdgeKind::default(),
            animated: false,
            selected: false,
            selectable: true,
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

    pub fn kind(mut self, kind: EdgeKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }

    pub fn marker_end(mut self, marker: MarkerKind) -> Self {
        self.marker_end = marker;
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
        let vp = Viewport {
            offset: Point::new(13.0, -7.0),
            zoom: 1.7,
        };
        let p = Point::new(100.0, 250.0);
        let q = vp.screen_to_flow(vp.flow_to_screen(p));
        assert!(p.distance(q) < 1e-9);
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
