//! Seat-based ports: discrete connection points packed around a node's
//! rounded rim, and the curves between them.
//!
//! This is an alternative to [`crate::Handle`]-based anchoring for editors
//! where connections may attach anywhere on a node's border: a rim offers a
//! whole number of *seats* one [`SEAT_PITCH`] apart, [`solve_ports`] packs
//! every connection end into a seat of its own (deterministically, so the
//! same graph always renders the same), and [`edge_geometry`] draws the
//! curve between two seats, arrowheads and label anchor included.
//!
//! Everything here is headless — pure geometry over keyed rectangles — so it
//! can drive custom edge layers, exports and thumbnails alike.

/// Re-exported for convenience: a seat names its face with the shared
/// [`Side`] type.
pub use crate::types::Side;
use crate::types::{Id, Point, Rect};
use std::collections::BTreeMap;
use std::f64::consts::{FRAC_PI_2, PI};

/// Which ends of a connection carry an arrowhead. The plane only needs to
/// know which ends to leave room for; what an arrow means belongs to the
/// application.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Arrows {
    pub start: bool,
    pub end: bool,
}

impl Arrows {
    pub const NONE: Self = Self {
        start: false,
        end: false,
    };

    pub const fn new(start: bool, end: bool) -> Self {
        Self { start, end }
    }
}

/// How far apart the seats on a node's rim are, and so the least room any two
/// beads can have between them.
///
/// This is the document's grid cell on purpose. A node's frame comes to rest on
/// that grid, so every seat lands on a line the paper already draws: a port is
/// never at an arbitrary offset, and a connection leaves along structure the
/// reader can already see.
pub const SEAT_PITCH: f64 = 12.0;
/// The bead's own radius, which the surface draws and this reserves room for.
///
/// Small enough to read as a joint rather than a knob: where a connection meets
/// a card is a detail of the connection, not a control competing with it. The
/// grab target is its own, larger circle, so this answers only to the eye.
pub const PORT_RADIUS: f64 = 3.6;
/// How rounded a node's corners are. The stylesheet has to agree with this, or
/// beads will sit off the rim they are packed onto.
///
/// It is exactly one seat, which is what keeps the lattice exact: the corner arc
/// spans one cell, so the straight runs begin and end on a seat and every
/// interior seat has an axis-aligned normal.
pub const CORNER_RADIUS: f64 = SEAT_PITCH;
/// How near a connection's end a label may be dragged. A label exactly on the end
/// would sit under the arrowhead and the node both.
pub const MIN_LABEL_POSITION: f64 = 0.04;
pub const MAX_LABEL_POSITION: f64 = 0.96;
/// Where a label rests until it is moved.
pub const DEFAULT_LABEL_POSITION: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
    pub nx: f64,
    pub ny: f64,
}

impl Anchor {
    pub const fn point(self) -> Point {
        Point::new(self.x, self.y)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Endpoint {
    Start,
    End,
}

impl Endpoint {
    pub const fn other(self) -> Self {
        match self {
            Self::Start => Self::End,
            Self::End => Self::Start,
        }
    }
}

/// One of the discrete places on a node's rim where a bead may rest.
///
/// The [`Side`] names which face of the rim the seat is on; `Top` and `Bottom`
/// count their cells from the left corner, `Left` and `Right` from the top
/// corner — the common origin that makes a seat survive a resize.
///
/// A rim offers a whole number of seats a cell apart, so a port position is an
/// integer rather than a distance. That is what lets a bead dragged away and
/// back leave the document byte-identical — and therefore leave no undo step
/// behind — and what makes two beads sharing a seat a countable fact rather than
/// a floating-point near-miss.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct PortSeat {
    pub side: Side,
    pub cell: u16,
}

impl PortSeat {
    pub const fn new(side: Side, cell: u16) -> Self {
        Self { side, cell }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Terminal {
    Node(Id),
    Point(Point),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub id: Id,
    pub start: Terminal,
    pub end: Terminal,
    /// The seat the user pinned this end to. `None` leaves it to the solver,
    /// which is where every connection starts.
    pub start_seat: Option<PortSeat>,
    pub end_seat: Option<PortSeat>,
}

impl Link {
    /// A connection with both ends left to the solver.
    pub fn solved(id: impl Into<Id>, start: Terminal, end: Terminal) -> Self {
        Self {
            id: id.into(),
            start,
            end,
            start_seat: None,
            end_seat: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeAnchors {
    pub start: Anchor,
    pub end: Anchor,
}

#[derive(Clone, Copy, Debug)]
struct Ring {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    radius: f64,
    across: f64,
    down: f64,
    corner: f64,
    length: f64,
}

fn ring(frame: Rect) -> Ring {
    let radius = CORNER_RADIUS
        .min(frame.width / 2.0)
        .min(frame.height / 2.0)
        .max(0.0);
    let across = (frame.width - radius * 2.0).max(0.0);
    let down = (frame.height - radius * 2.0).max(0.0);
    let corner = PI * radius / 2.0;
    Ring {
        x0: frame.x + radius,
        y0: frame.y + radius,
        x1: frame.x + frame.width - radius,
        y1: frame.y + frame.height - radius,
        radius,
        across,
        down,
        corner,
        length: across * 2.0 + down * 2.0 + corner * 4.0,
    }
}

pub fn ring_length(frame: Rect) -> f64 {
    ring(frame).length
}

pub fn ring_point(frame: Rect, at: f64) -> Anchor {
    let ring = ring(frame);
    if ring.length == 0.0 {
        return Anchor {
            x: ring.x0,
            y: ring.y0,
            nx: 0.0,
            ny: -1.0,
        };
    }
    let mut s = at.rem_euclid(ring.length);
    if s < ring.across {
        return Anchor {
            x: ring.x0 + s,
            y: ring.y0 - ring.radius,
            nx: 0.0,
            ny: -1.0,
        };
    }
    s -= ring.across;
    if s < ring.corner {
        return arc(ring, ring.x1, ring.y0, -FRAC_PI_2 + s / ring.radius);
    }
    s -= ring.corner;
    if s < ring.down {
        return Anchor {
            x: ring.x1 + ring.radius,
            y: ring.y0 + s,
            nx: 1.0,
            ny: 0.0,
        };
    }
    s -= ring.down;
    if s < ring.corner {
        return arc(ring, ring.x1, ring.y1, s / ring.radius);
    }
    s -= ring.corner;
    if s < ring.across {
        return Anchor {
            x: ring.x1 - s,
            y: ring.y1 + ring.radius,
            nx: 0.0,
            ny: 1.0,
        };
    }
    s -= ring.across;
    if s < ring.corner {
        return arc(ring, ring.x0, ring.y1, FRAC_PI_2 + s / ring.radius);
    }
    s -= ring.corner;
    if s < ring.down {
        return Anchor {
            x: ring.x0 - ring.radius,
            y: ring.y1 - s,
            nx: -1.0,
            ny: 0.0,
        };
    }
    s -= ring.down;
    arc(ring, ring.x0, ring.y0, PI + s / ring.radius)
}

fn arc(ring: Ring, cx: f64, cy: f64, angle: f64) -> Anchor {
    let nx = angle.cos();
    let ny = angle.sin();
    Anchor {
        x: cx + nx * ring.radius,
        y: cy + ny * ring.radius,
        nx,
        ny,
    }
}

pub fn nearest_on_ring(frame: Rect, target: Point) -> f64 {
    let ring = ring(frame);
    if ring.length == 0.0 {
        return 0.0;
    }
    let cx = target.x.clamp(ring.x0, ring.x1);
    let cy = target.y.clamp(ring.y0, ring.y1);
    let dx = target.x - cx;
    let dy = target.y - cy;
    if dx == 0.0 && dy == 0.0 {
        let gaps = [
            target.y - frame.y,
            frame.x + frame.width - target.x,
            frame.y + frame.height - target.y,
            target.x - frame.x,
        ];
        let nearest = gaps
            .iter()
            .enumerate()
            .min_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .unwrap_or(0);
        return match nearest {
            0 => (target.x - ring.x0).clamp(0.0, ring.across),
            1 => ring.across + ring.corner + (target.y - ring.y0).clamp(0.0, ring.down),
            2 => {
                ring.across
                    + ring.corner
                    + ring.down
                    + ring.corner
                    + (ring.x1 - target.x).clamp(0.0, ring.across)
            }
            _ => {
                ring.across * 2.0
                    + ring.corner * 3.0
                    + ring.down
                    + (ring.y1 - target.y).clamp(0.0, ring.down)
            }
        };
    }

    let top_right = ring.across;
    let right_run = top_right + ring.corner;
    let bottom_right = right_run + ring.down;
    let bottom_run = bottom_right + ring.corner;
    let bottom_left = bottom_run + ring.across;
    let left_run = bottom_left + ring.corner;
    let top_left = left_run + ring.down;

    if dx == 0.0 {
        return if dy < 0.0 {
            cx - ring.x0
        } else {
            bottom_run + ring.x1 - cx
        };
    }
    if dy == 0.0 {
        return if dx > 0.0 {
            right_run + cy - ring.y0
        } else {
            left_run + ring.y1 - cy
        };
    }
    let angle = dy.atan2(dx);
    if dx > 0.0 && dy < 0.0 {
        top_right + (angle + FRAC_PI_2) * ring.radius
    } else if dx > 0.0 {
        bottom_right + angle * ring.radius
    } else if dy > 0.0 {
        bottom_left + (angle - FRAC_PI_2) * ring.radius
    } else {
        top_left + ((if angle < 0.0 { angle + PI * 2.0 } else { angle }) - PI) * ring.radius
    }
}

/// How many seats long a frame's horizontal and vertical runs are.
///
/// A resting frame is a whole number of cells, so this is exact; during a drag
/// it rounds, and the seats slide with the edge until the release snaps both
/// back onto the grid.
fn seat_counts(frame: Rect) -> (u16, u16) {
    let count = |length: f64| {
        let cells = (length / SEAT_PITCH).round();
        if cells.is_finite() {
            (cells as i64).clamp(1, 4096) as u16
        } else {
            1
        }
    };
    (count(frame.width), count(frame.height))
}

/// Every seat a frame offers, walking the rim clockwise from the top-left
/// corner. The four corners belong to the horizontal runs, so no place on the
/// rim is named twice.
pub fn seats(frame: Rect) -> Vec<PortSeat> {
    let (cols, rows) = seat_counts(frame);
    let mut all = Vec::with_capacity(usize::from(cols) * 2 + usize::from(rows) * 2);
    all.extend((0..=cols).map(|cell| PortSeat::new(Side::Top, cell)));
    all.extend((1..rows).map(|cell| PortSeat::new(Side::Right, cell)));
    all.extend(
        (0..=cols)
            .rev()
            .map(|cell| PortSeat::new(Side::Bottom, cell)),
    );
    all.extend((1..rows).rev().map(|cell| PortSeat::new(Side::Left, cell)));
    all
}

/// The nearest seat this frame actually has to the one asked for. A node shrunk
/// past one of its own ports leaves that port at the corner rather than off the
/// end of the rim.
pub fn clamp_seat(frame: Rect, seat: PortSeat) -> PortSeat {
    let (cols, rows) = seat_counts(frame);
    match seat.side {
        Side::Top | Side::Bottom => PortSeat::new(seat.side, seat.cell.min(cols)),
        Side::Left | Side::Right if rows < 2 => PortSeat::new(Side::Top, seat.cell.min(cols)),
        Side::Left | Side::Right => PortSeat::new(seat.side, seat.cell.clamp(1, rows - 1)),
    }
}

/// Where a seat sits on the rim, with the outward normal a connection leaves
/// along. The seat names a point on the plain rectangle; the rim itself is
/// rounded, so a corner seat resolves onto its arc.
pub fn seat_point(frame: Rect, seat: PortSeat) -> Anchor {
    let seat = clamp_seat(frame, seat);
    // A seat is a whole number of cells from its own corner, so that is what it
    // measures — not a fraction of the run it sits on. The two agree exactly on a
    // resting frame, and only the first of them holds *during* a resize: a frame
    // mid-drag is any width at all, and a bead on an edge nobody is dragging has
    // to stay where it is rather than slide along proportionally and jump
    // whenever the run's cell count rounds to the next whole one.
    let along = |cell: u16, length: f64| (f64::from(cell) * SEAT_PITCH).min(length.max(0.0));
    let sharp = match seat.side {
        Side::Top => Point::new(frame.x + along(seat.cell, frame.width), frame.y),
        Side::Bottom => Point::new(
            frame.x + along(seat.cell, frame.width),
            frame.y + frame.height,
        ),
        Side::Left => Point::new(frame.x, frame.y + along(seat.cell, frame.height)),
        Side::Right => Point::new(
            frame.x + frame.width,
            frame.y + along(seat.cell, frame.height),
        ),
    };
    ring_point(frame, nearest_on_ring(frame, sharp))
}

/// The seat nearest a point that nothing is sitting in yet.
///
/// Used when the editor is choosing for the user rather than the other way
/// round: a band dropped into the middle of a card takes the seat nearest
/// whatever pulls it, and steps aside if a bead is already there.
pub fn nearest_free_seat(frame: Rect, target: Point, taken: &[PortSeat]) -> PortSeat {
    let all = seats(frame);
    if all.is_empty() {
        return PortSeat::new(Side::Top, 0);
    }
    let wish = nearest_seat(frame, target);
    let mut occupied = vec![false; all.len()];
    for seat in taken {
        let seat = clamp_seat(frame, *seat);
        if let Some(index) = all.iter().position(|candidate| *candidate == seat) {
            occupied[index] = true;
        }
    }
    let start = all.iter().position(|seat| *seat == wish).unwrap_or(0);
    all[nearest_free(&occupied, start)]
}

/// The seat nearest a point, which is what a dragged bead lands on.
pub fn nearest_seat(frame: Rect, target: Point) -> PortSeat {
    seats(frame)
        .into_iter()
        .map(|seat| (seat, seat_point(frame, seat).point().distance(target)))
        .min_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.cmp(&right.0))
        })
        .map_or(PortSeat::new(Side::Top, 0), |(seat, _)| seat)
}

#[derive(Clone, Debug)]
struct Bead {
    edge_id: Id,
    endpoint: Endpoint,
    /// The seat the user pinned this bead to. A pinned bead is furniture: the
    /// solver packs the free ones around it and never moves it.
    pinned: Option<PortSeat>,
    /// What the band is pulling this bead towards.
    target: Point,
}

#[derive(Clone, Copy)]
enum Resolved<'a> {
    Hooked { frame: Rect, node_id: &'a str },
    Pinned(Point),
}

impl<'a> Resolved<'a> {
    fn node_id(self) -> Option<&'a str> {
        match self {
            Self::Hooked { node_id, .. } => Some(node_id),
            Self::Pinned(_) => None,
        }
    }
}

pub fn solve_ports(frames: &BTreeMap<Id, Rect>, links: &[Link]) -> BTreeMap<Id, EdgeAnchors> {
    let mut anchors = BTreeMap::new();
    let mut beads_by_node: BTreeMap<Id, Vec<Bead>> = BTreeMap::new();
    // Ends held at a pointer rather than hooked on a rim. They are filled in
    // once the rims have settled, so a held band aims at the bead it will meet.
    let mut held: Vec<(Id, Endpoint, Point)> = Vec::new();

    for link in links {
        let Some(start) = resolve(frames, &link.start) else {
            continue;
        };
        let Some(end) = resolve(frames, &link.end) else {
            continue;
        };
        if start.node_id().is_some() && start.node_id() == end.node_id() {
            continue;
        }
        if matches!(start, Resolved::Pinned(_)) && matches!(end, Resolved::Pinned(_)) {
            continue;
        }
        // Where each end pulls the other. A pinned seat and a held pointer are
        // already somewhere; two free ends aim at each other, which one pass
        // settles well enough to decide the seat they land on.
        let far = settle(end, link.end_seat, centre(start));
        let near = settle(start, link.start_seat, far);
        let far = settle(end, link.end_seat, near);

        let mut hang = |resolved: Resolved<'_>, endpoint, seat, target| match resolved {
            Resolved::Hooked { node_id, .. } => {
                beads_by_node.entry(node_id.into()).or_default().push(Bead {
                    edge_id: link.id.clone(),
                    endpoint,
                    pinned: seat,
                    target,
                });
            }
            Resolved::Pinned(at) => held.push((link.id.clone(), endpoint, at)),
        };
        hang(start, Endpoint::Start, link.start_seat, far);
        hang(end, Endpoint::End, link.end_seat, near);
    }

    let mut rings: Vec<RingState> = beads_by_node
        .into_iter()
        .filter_map(|(node_id, beads)| Some(seat_beads(*frames.get(&node_id)?, beads)))
        .collect();
    for ring in &rings {
        place_beads(&mut anchors, ring);
    }
    untangle(&mut anchors, &mut rings);
    for (edge_id, endpoint, at) in held {
        pin(&mut anchors, &edge_id, endpoint, at);
    }
    anchors
}

/// What a link's far end is, before its own rim has been settled.
fn centre(resolved: Resolved<'_>) -> Point {
    match resolved {
        Resolved::Hooked { frame, .. } => frame.center(),
        Resolved::Pinned(at) => at,
    }
}

/// Where one end comes to rest given what is pulling it: a pinned bead does not
/// move, a held pointer is already where it is, and a free bead slides to the
/// nearest place on its own rim.
fn settle(resolved: Resolved<'_>, seat: Option<PortSeat>, toward: Point) -> Point {
    match (resolved, seat) {
        (Resolved::Pinned(at), _) => at,
        (Resolved::Hooked { frame, .. }, Some(seat)) => seat_point(frame, seat).point(),
        (Resolved::Hooked { frame, .. }, None) => {
            ring_point(frame, nearest_on_ring(frame, toward)).point()
        }
    }
}

struct RingState {
    frame: Rect,
    seats: Vec<PortSeat>,
    beads: Vec<Bead>,
    /// The seat index each bead holds, parallel to `beads`.
    taken: Vec<usize>,
}

/// Sits every bead on this node's rim in a seat of its own.
///
/// Pinned beads go down first and are immovable; the free ones then take the
/// seat nearest whatever pulls them, and the nearest empty one when that is
/// already spoken for.
fn seat_beads(frame: Rect, beads: Vec<Bead>) -> RingState {
    let all = seats(frame);
    let index_of: BTreeMap<PortSeat, usize> = all
        .iter()
        .enumerate()
        .map(|(index, seat)| (*seat, index))
        .collect();
    let mut occupied = vec![false; all.len()];
    let mut taken = vec![0usize; beads.len()];
    let mut wishes: Vec<(usize, usize)> = Vec::new();

    for (index, bead) in beads.iter().enumerate() {
        match bead.pinned {
            Some(seat) => {
                let at = index_of.get(&clamp_seat(frame, seat)).copied().unwrap_or(0);
                taken[index] = at;
                occupied[at] = true;
            }
            None => {
                let wish = index_of
                    .get(&nearest_seat(frame, bead.target))
                    .copied()
                    .unwrap_or(0);
                wishes.push((index, wish));
            }
        }
    }

    // Ordering the wishes — and breaking a tie by identity rather than by
    // arrival — is what makes the arrangement the same for the same geometry,
    // whatever order the connections were drawn in, and the same again after a
    // save, a reload, an undo and a redo.
    wishes.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| beads[left.0].edge_id.cmp(&beads[right.0].edge_id))
            .then_with(|| order_of(beads[left.0].endpoint).cmp(&order_of(beads[right.0].endpoint)))
    });
    for (index, wish) in wishes {
        let at = nearest_free(&occupied, wish);
        occupied[at] = true;
        taken[index] = at;
    }

    RingState {
        frame,
        seats: all,
        beads,
        taken,
    }
}

/// The empty seat nearest `wish`, searched outwards in both directions so a
/// crowded rim packs around the wish rather than drifting off one way.
fn nearest_free(occupied: &[bool], wish: usize) -> usize {
    let count = occupied.len();
    if count == 0 {
        return 0;
    }
    let wish = wish.min(count - 1);
    if !occupied[wish] {
        return wish;
    }
    for step in 1..count {
        let after = (wish + step) % count;
        if !occupied[after] {
            return after;
        }
        let before = (wish + count - step) % count;
        if !occupied[before] {
            return before;
        }
    }
    // Every seat on the rim is taken. Sharing one is better than losing a bead.
    wish
}

const fn order_of(endpoint: Endpoint) -> u8 {
    match endpoint {
        Endpoint::Start => 0,
        Endpoint::End => 1,
    }
}

fn place_beads(anchors: &mut BTreeMap<Id, EdgeAnchors>, ring: &RingState) {
    for (index, bead) in ring.beads.iter().enumerate() {
        let anchor = seat_point(ring.frame, ring.seats[ring.taken[index]]);
        let pair = anchors.entry(bead.edge_id.clone()).or_insert(EdgeAnchors {
            start: anchor,
            end: anchor,
        });
        match bead.endpoint {
            Endpoint::Start => pair.start = anchor,
            Endpoint::End => pair.end = anchor,
        }
    }
}

/// Two bands wanting the same pair of seats have nothing to decide which takes
/// which, and a crossed pair is strictly longer than the same pair uncrossed —
/// so neighbouring beads trade seats until no trade would shorten them. A pinned
/// bead never trades: the user placed it.
fn untangle(anchors: &mut BTreeMap<Id, EdgeAnchors>, rings: &mut [RingState]) {
    const PASSES: usize = 8;
    const EPSILON: f64 = 0.01;
    for _ in 0..PASSES {
        let mut traded_any = false;
        for ring in rings.iter_mut() {
            let mut ring_changed = false;
            let mut order: Vec<usize> = (0..ring.beads.len()).collect();
            order.sort_by_key(|index| ring.taken[*index]);
            for pair in order.windows(2) {
                let (near, far) = (pair[0], pair[1]);
                if ring.beads[near].pinned.is_some() || ring.beads[far].pinned.is_some() {
                    continue;
                }
                // Where each band's other end is *now*, not where it was
                // provisionally guessed to be. Two identical parallel bands guess
                // identically, so a fixed guess can never tell a crossed pair
                // from an uncrossed one; the settled far end can.
                let (Some(near_partner), Some(far_partner)) = (
                    partner(anchors, &ring.beads[near]),
                    partner(anchors, &ring.beads[far]),
                ) else {
                    continue;
                };
                let near_seat = seat_point(ring.frame, ring.seats[ring.taken[near]]).point();
                let far_seat = seat_point(ring.frame, ring.seats[ring.taken[far]]).point();
                let held = near_seat.distance(near_partner) + far_seat.distance(far_partner);
                let swapped = far_seat.distance(near_partner) + near_seat.distance(far_partner);
                if swapped < held - EPSILON {
                    ring.taken.swap(near, far);
                    traded_any = true;
                    ring_changed = true;
                }
            }
            if ring_changed {
                place_beads(anchors, ring);
            }
        }
        if !traded_any {
            return;
        }
    }
}

fn partner(anchors: &BTreeMap<Id, EdgeAnchors>, bead: &Bead) -> Option<Point> {
    let pair = anchors.get(&bead.edge_id)?;
    Some(match bead.endpoint {
        Endpoint::Start => pair.end.point(),
        Endpoint::End => pair.start.point(),
    })
}

fn resolve<'a>(frames: &'a BTreeMap<Id, Rect>, terminal: &'a Terminal) -> Option<Resolved<'a>> {
    match terminal {
        Terminal::Point(point) => Some(Resolved::Pinned(*point)),
        Terminal::Node(node_id) => Some(Resolved::Hooked {
            frame: *frames.get(node_id)?,
            node_id,
        }),
    }
}

/// The end a pointer is holding: it is wherever the pointer is, and it aims at
/// the bead it will meet, so the preview is the geometry the release commits.
fn pin(anchors: &mut BTreeMap<Id, EdgeAnchors>, edge_id: &Id, endpoint: Endpoint, at: Point) {
    let Some(pair) = anchors.get_mut(edge_id) else {
        return;
    };
    let bead = match endpoint {
        Endpoint::Start => pair.end,
        Endpoint::End => pair.start,
    };
    let dx = bead.x - at.x;
    let dy = bead.y - at.y;
    let length = dx.hypot(dy).max(f64::EPSILON);
    let anchor = Anchor {
        x: at.x,
        y: at.y,
        nx: dx / length,
        ny: dy / length,
    };
    match endpoint {
        Endpoint::Start => pair.start = anchor,
        Endpoint::End => pair.end = anchor,
    }
}

type Cubic = [Point; 4];

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeGeometry {
    pub path: String,
    pub outline: String,
    pub start_arrow: Option<String>,
    pub end_arrow: Option<String>,
    pub label: Point,
    curve: Cubic,
}

impl EdgeGeometry {
    pub fn nearest_label_position(&self, point: Point) -> f64 {
        nearest_position(self.curve, point)
    }

    pub fn point_at(&self, t: f64) -> Point {
        point_at(self.curve, t)
    }
}

const ARROW_TIP_INSET: f64 = PORT_RADIUS + 2.0;
/// How much of the distance a port already faces becomes handle. Two ports
/// facing each other dead on draw a straight line, which is what a straight
/// relationship should look like.
const ALONG_RATIO: f64 = 0.45;
/// How hard a sideways offset bends the exit. A seated port frequently faces
/// across the chord rather than along it — two cards side by side joined from
/// their top edges — and this is what makes that an arch rather than a kink.
const ACROSS_RATIO: f64 = 0.40;
/// A port facing away from the far end has to get clear of its own card before
/// the curve can turn back. `REACH` carries it out along its normal, `SWEEP`
/// carries it round the side.
const BEHIND_REACH: f64 = 0.30;
const BEHIND_SWEEP: f64 = 0.45;
const MIN_CURVE: f64 = 10.0;
const MAX_CURVE: f64 = 220.0;
const ARROW_ASPECT: f64 = 0.58;
const MIN_ARROW_LENGTH: f64 = 5.0;
const MIN_ARROW_SPAN: f64 = 7.0;
const ARROW_SAMPLES: usize = 48;

pub fn edge_geometry(
    from: Anchor,
    to: Anchor,
    arrows: Arrows,
    weight: u8,
    label_position: f64,
    bare: bool,
) -> EdgeGeometry {
    let start_inset = if bare {
        0.0
    } else if arrows.start {
        ARROW_TIP_INSET
    } else {
        PORT_RADIUS
    };
    let end_inset = if bare {
        0.0
    } else if arrows.end {
        ARROW_TIP_INSET
    } else {
        PORT_RADIUS
    };
    let start = Point::new(
        from.x + from.nx * start_inset,
        from.y + from.ny * start_inset,
    );
    let end = Point::new(to.x + to.nx * end_inset, to.y + to.ny * end_inset);
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let span = dx.hypot(dy);
    let (ux, uy) = if span > f64::EPSILON {
        (dx / span, dy / span)
    } else {
        (1.0, 0.0)
    };
    // The side a port facing the wrong way swings out towards. Both ends pick
    // the same one, so a curve that has to come round the back of a card does it
    // in a single sweep instead of kinking in the middle.
    let (px, py) = (-uy, ux);
    // How much of the way to the far end each port faces. The projection is
    // signed, not absolute — a port facing away must not be handed a long handle
    // pointing backwards, which folds the curve past its own endpoint and turns
    // the arrowhead round with it.
    let outlook = |anchor: Anchor, toward_x: f64, toward_y: f64| {
        let along = toward_x * anchor.nx + toward_y * anchor.ny;
        let across = (toward_x * anchor.ny - toward_y * anchor.nx).abs();
        (along, across)
    };
    let (from_along, from_across) = outlook(from, dx, dy);
    let (to_along, to_across) = outlook(to, -dx, -dy);
    // One port facing backwards decides the shape of the whole band: both ends
    // are carried the same distance to the same side, so the curve travels
    // clear of the card it has to get around instead of cutting back through it
    // to meet a far end that stayed on the axis.
    let sweep = (-from_along).max(-to_along).clamp(0.0, MAX_CURVE) * BEHIND_SWEEP;
    let control = |anchor: Anchor, at: Point, along: f64, across: f64| {
        let reach = (along.max(0.0) * ALONG_RATIO
            + across * ACROSS_RATIO
            + (-along).max(0.0) * BEHIND_REACH)
            .clamp(MIN_CURVE, MAX_CURVE);
        // The sweep runs along the chord's perpendicular, which for a port facing
        // across the chord points partly back into that port's own card — enough
        // of it, since the sweep can be larger than the reach, to send the handle
        // behind its own bead. Only the part of the sweep that runs along this
        // port's own rail is kept, so the handle always leaves along the outward
        // normal by exactly `reach`, and both ends still swing to the same side.
        let behind = px * anchor.nx + py * anchor.ny;
        let (sx, sy) = (px - anchor.nx * behind, py - anchor.ny * behind);
        Point::new(
            at.x + anchor.nx * reach + sx * sweep,
            at.y + anchor.ny * reach + sy * sweep,
        )
    };
    let curve = [
        start,
        control(from, start, from_along, from_across),
        control(to, end, to_along, to_across),
        end,
    ];
    let nominal: f64 = match weight.clamp(1, 3) {
        1 => 7.5,
        2 => 8.5,
        _ => 9.5,
    };
    let head_length = if arrows == Arrows::NONE || span < MIN_ARROW_SPAN {
        0.0
    } else {
        nominal.min(span * 0.42).max(MIN_ARROW_LENGTH)
    };
    let mut start_t = 0.0;
    let mut end_t = 1.0;
    let mut start_arrow = None;
    let mut end_arrow = None;
    if head_length > 0.0 {
        if arrows.start {
            start_t = forward_from_start(curve, head_length);
        }
        if arrows.end {
            end_t = back_from_end(curve, head_length);
        }
        // Two heads on a short band each reach past the other, and a stroke
        // trimmed to a crossed pair is drawn backwards — which turns both
        // arrowheads round, exactly what the signed handle above exists to
        // prevent. They meet in the middle instead: the band gives up its stroke
        // rather than its heads, and every head still points out at its own end.
        if start_t > end_t {
            let met = (start_t + end_t) / 2.0;
            start_t = met;
            end_t = met;
        }
        if arrows.start {
            start_arrow = Some(arrow_path(point_at(curve, start_t), start, head_length));
        }
        if arrows.end {
            end_arrow = Some(arrow_path(point_at(curve, end_t), end, head_length));
        }
    }
    let label_position = label_position.clamp(MIN_LABEL_POSITION, MAX_LABEL_POSITION);
    EdgeGeometry {
        path: to_path(slice(curve, start_t, end_t)),
        outline: to_path(curve),
        start_arrow,
        end_arrow,
        label: point_at(curve, label_position),
        curve,
    }
}

fn point_at([p0, p1, p2, p3]: Cubic, t: f64) -> Point {
    let a = lerp(p0, p1, t);
    let b = lerp(p1, p2, t);
    let c = lerp(p2, p3, t);
    lerp(lerp(a, b, t), lerp(b, c, t), t)
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn nearest_position(curve: Cubic, target: Point) -> f64 {
    let samples = 40;
    let mut best = 0.0;
    let mut best_distance = f64::INFINITY;
    for index in 0..=samples {
        let t = index as f64 / samples as f64;
        let distance = squared(point_at(curve, t), target);
        if distance < best_distance {
            best = t;
            best_distance = distance;
        }
    }
    let mut low = (best - 1.0 / samples as f64).max(0.0);
    let mut high = (best + 1.0 / samples as f64).min(1.0);
    for _ in 0..16 {
        let left = low + (high - low) / 3.0;
        let right = high - (high - low) / 3.0;
        if squared(point_at(curve, left), target) < squared(point_at(curve, right), target) {
            high = right;
        } else {
            low = left;
        }
    }
    ((low + high) / 2.0).clamp(MIN_LABEL_POSITION, MAX_LABEL_POSITION)
}

fn squared(a: Point, b: Point) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

fn split([p0, p1, p2, p3]: Cubic, t: f64) -> (Cubic, Cubic) {
    let a = lerp(p0, p1, t);
    let b = lerp(p1, p2, t);
    let c = lerp(p2, p3, t);
    let d = lerp(a, b, t);
    let e = lerp(b, c, t);
    let midpoint = lerp(d, e, t);
    ([p0, a, d, midpoint], [midpoint, e, c, p3])
}

fn slice(curve: Cubic, start: f64, end: f64) -> Cubic {
    if start <= 0.0 && end >= 1.0 {
        return curve;
    }
    let before_end = if end < 1.0 {
        split(curve, end).0
    } else {
        curve
    };
    if start <= 0.0 {
        before_end
    } else {
        split(before_end, start / end).1
    }
}

fn back_from_end(curve: Cubic, distance: f64) -> f64 {
    let end = curve[3];
    for index in 1..=ARROW_SAMPLES {
        let t = 1.0 - index as f64 / ARROW_SAMPLES as f64;
        if point_at(curve, t).distance(end) >= distance {
            return refine(curve, end, t, t + 1.0 / ARROW_SAMPLES as f64, distance);
        }
    }
    0.0
}

fn forward_from_start(curve: Cubic, distance: f64) -> f64 {
    let start = curve[0];
    for index in 1..=ARROW_SAMPLES {
        let t = index as f64 / ARROW_SAMPLES as f64;
        if point_at(curve, t).distance(start) >= distance {
            return refine(curve, start, t, t - 1.0 / ARROW_SAMPLES as f64, distance);
        }
    }
    1.0
}

fn refine(curve: Cubic, anchor: Point, outside: f64, inside: f64, distance: f64) -> f64 {
    let mut far = outside;
    let mut near = inside;
    for _ in 0..12 {
        let mid = (far + near) / 2.0;
        if point_at(curve, mid).distance(anchor) >= distance {
            far = mid;
        } else {
            near = mid;
        }
    }
    (far + near) / 2.0
}

fn to_path([p0, p1, p2, p3]: Cubic) -> String {
    format!(
        "M{},{} C{},{} {},{} {},{}",
        round(p0.x),
        round(p0.y),
        round(p1.x),
        round(p1.y),
        round(p2.x),
        round(p2.y),
        round(p3.x),
        round(p3.y)
    )
}

fn arrow_path(base: Point, tip: Point, length: f64) -> String {
    let axis = Point::new(tip.x - base.x, tip.y - base.y);
    let axis_length = axis.x.hypot(axis.y).max(f64::EPSILON);
    let half = length * ARROW_ASPECT / 2.0;
    let px = -axis.y / axis_length * half;
    let py = axis.x / axis_length * half;
    format!(
        "M{},{} L{},{} L{},{} Z",
        round(base.x + px),
        round(base.y + py),
        round(tip.x),
        round(tip.y),
        round(base.x - px),
        round(base.y - py)
    )
}

fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 216.0,
            height: 48.0,
        }
    }

    #[test]
    fn ring_walk_round_trips() {
        let frame = frame(10.0, 20.0);
        let length = ring_length(frame);
        for step in 0..128 {
            let at = length * step as f64 / 128.0;
            let point = ring_point(frame, at);
            let found = nearest_on_ring(frame, point.point());
            let delta = (found - at).abs().min(length - (found - at).abs());
            assert!(delta < 0.001, "{at} vs {found}");
            assert!((point.nx.hypot(point.ny) - 1.0).abs() < 0.001);
        }
    }

    fn node(id: &str) -> Terminal {
        Terminal::Node(id.into())
    }

    fn pair() -> BTreeMap<Id, Rect> {
        BTreeMap::from([
            ("a".into(), frame(0.0, 0.0)),
            ("b".into(), frame(360.0, 0.0)),
        ])
    }

    #[test]
    fn lone_band_uses_closest_points() {
        let frames = BTreeMap::from([
            ("a".into(), frame(0.0, 0.0)),
            ("b".into(), frame(360.0, 120.0)),
        ]);
        let result = solve_ports(&frames, &[Link::solved("edge", node("a"), node("b"))]);
        let anchors = result["edge"];
        assert!(anchors.start.x > 200.0);
        assert!(anchors.end.x < 370.0);
    }

    #[test]
    fn crowded_ports_keep_a_seat_each() {
        let frames = pair();
        let links: Vec<_> = (0..5)
            .map(|index| Link::solved(format!("edge-{index}"), node("a"), node("b")))
            .collect();
        let result = solve_ports(&frames, &links);
        let mut seats: Vec<_> = result
            .values()
            .map(|anchors| nearest_seat(frames["a"], anchors.start.point()))
            .collect();
        seats.sort();
        seats.dedup();
        assert_eq!(seats.len(), 5, "five bands must take five distinct seats");
        for window in seats.windows(2) {
            let gap = seat_point(frames["a"], window[0])
                .point()
                .distance(seat_point(frames["a"], window[1]).point());
            // A cell apart along the straight runs. Across a corner the rim cuts
            // the angle, so two seats either side of one are nearer than that —
            // still never near enough for their beads to touch.
            assert!(gap >= PORT_RADIUS * 2.0, "beads {gap} apart");
        }
    }

    #[test]
    fn held_band_uses_the_same_solver() {
        let frames = BTreeMap::from([("a".into(), frame(0.0, 0.0))]);
        let result = solve_ports(
            &frames,
            &[Link::solved(
                "held",
                node("a"),
                Terminal::Point(Point::new(400.0, 120.0)),
            )],
        );
        assert_eq!(result["held"].end.point(), Point::new(400.0, 120.0));
    }

    /// The whole point of a seat: it is where the user put it, whatever the
    /// solver would have preferred.
    #[test]
    fn a_pinned_bead_stays_where_it_was_put() {
        let frames = pair();
        let seat = PortSeat::new(Side::Left, 2);
        let mut link = Link::solved("edge", node("a"), node("b"));
        link.start_seat = Some(seat);
        let result = solve_ports(&frames, &[link]);
        assert_eq!(
            result["edge"].start.point(),
            seat_point(frames["a"], seat).point(),
            "a pinned bead must not be re-solved onto the facing side"
        );
    }

    /// A pinned bead is furniture. Free beads pack around it and never evict it.
    #[test]
    fn free_beads_make_room_for_a_pinned_one() {
        let frames = pair();
        let taken = PortSeat::new(Side::Right, 2);
        let mut links: Vec<_> = (0..4)
            .map(|index| Link::solved(format!("edge-{index}"), node("a"), node("b")))
            .collect();
        links[0].start_seat = Some(taken);
        let result = solve_ports(&frames, &links);
        assert_eq!(
            nearest_seat(frames["a"], result["edge-0"].start.point()),
            taken
        );
        for index in 1..4 {
            let seat = nearest_seat(frames["a"], result[&format!("edge-{index}")].start.point());
            assert_ne!(seat, taken, "edge-{index} evicted the pinned bead");
        }
    }

    /// Parallel bands run alongside each other rather than crossing in the gap.
    ///
    /// This is the case that cannot be settled from where each band *would* rest
    /// if it were alone: three identical bands guess identically, so they all
    /// want the same seat and the tie is broken by identity. Read down one rim
    /// and up the other and the order reverses, which is a crossing — so the
    /// trade has to be judged against where each band's far end actually ended
    /// up.
    #[test]
    fn parallel_bands_do_not_cross_in_the_gap() {
        let frames = pair();
        let links: Vec<_> = (0..3)
            .map(|index| Link::solved(format!("edge-{index}"), node("a"), node("b")))
            .collect();
        let solved = solve_ports(&frames, &links);

        let mut by_start: Vec<_> = links
            .iter()
            .map(|link| {
                let anchors = solved[&link.id];
                (anchors.start.y, anchors.end.y)
            })
            .collect();
        by_start.sort_by(|left, right| left.0.total_cmp(&right.0));
        for window in by_start.windows(2) {
            assert!(
                window[0].1 < window[1].1,
                "bands cross: {:?} then {:?}",
                window[0],
                window[1]
            );
        }
    }

    /// Ports are solved from the frames alone, so the same map must produce the
    /// same rim whatever order it was built in — which is what makes a save and
    /// reload, an undo and a redo, leave the picture untouched.
    #[test]
    fn seating_does_not_depend_on_the_order_connections_were_made() {
        let frames = pair();
        let forwards: Vec<_> = (0..5)
            .map(|index| Link::solved(format!("edge-{index}"), node("a"), node("b")))
            .collect();
        let mut backwards = forwards.clone();
        backwards.reverse();
        assert_eq!(
            solve_ports(&frames, &forwards),
            solve_ports(&frames, &backwards)
        );
    }

    #[test]
    fn a_seat_survives_a_resize_by_keeping_its_distance_from_its_own_corner() {
        let seat = PortSeat::new(Side::Top, 3);
        let before = seat_point(frame(0.0, 0.0), seat).point();
        // Growing the card to the right moves only the right edge, so a seat
        // measured from the left corner has not moved at all.
        let after = seat_point(Rect::new(0.0, 0.0, 360.0, 48.0), seat).point();
        assert_eq!(before, after);
    }

    /// And it survives the *drag* that gets there, which is the harder half: a
    /// frame between two grid stops is any width at all, and a seat read as a
    /// fraction of its run slid along the edge the whole way — jumping several
    /// pixels each time the run's cell count rounded to the next whole one.
    #[test]
    fn a_bead_on_an_edge_nobody_is_dragging_does_not_move_while_another_edge_does() {
        let seat = PortSeat::new(Side::Top, 9);
        let resting = seat_point(frame(0.0, 0.0), seat).point();
        assert_eq!(resting.x, 9.0 * SEAT_PITCH);

        let mut width = 216.0;
        while width <= 480.0 {
            let at = seat_point(Rect::new(0.0, 0.0, width, 48.0), seat).point();
            assert!(
                (at.x - resting.x).abs() < 1e-9 && (at.y - resting.y).abs() < 1e-9,
                "a right-edge resize to {width} moved a top-edge bead to {at:?}"
            );
            width += 0.25;
        }
    }

    /// A band shorter than the two arrowheads it carries used to be drawn
    /// backwards: each head was trimmed off from its own end, the two trims
    /// crossed, and the slice between them came out reversed with both
    /// arrowheads facing the wrong way.
    #[test]
    fn two_arrowheads_on_a_short_band_meet_rather_than_reverse_it() {
        for gap in [8.0, 12.0, 14.0, 16.0, 17.0, 18.0, 20.0, 24.0, 30.0, 40.0] {
            let from = Anchor {
                x: 0.0,
                y: 0.0,
                nx: 1.0,
                ny: 0.0,
            };
            let to = Anchor {
                x: gap,
                y: 0.0,
                nx: -1.0,
                ny: 0.0,
            };
            let geometry = edge_geometry(from, to, Arrows::new(true, true), 1, 0.5, false);
            // The untrimmed band runs whichever way its inset ends do; trimming
            // the heads off it must not turn that round.
            let points = path_points(&geometry.path);
            let whole = path_points(&geometry.outline);
            let drawn = points[points.len() - 1].x - points[0].x;
            let chord = whole[whole.len() - 1].x - whole[0].x;
            assert!(
                drawn * chord >= 0.0,
                "a {gap}px band runs {chord} but is drawn {drawn}: {}",
                geometry.path
            );
            // And each head still points out at the end it belongs to, which is
            // the other thing a crossed trim used to reverse.
            for (arrow, tip, away) in [
                (geometry.start_arrow.as_deref(), from.x, to.x),
                (geometry.end_arrow.as_deref(), to.x, from.x),
            ] {
                let Some(arrow) = arrow else { continue };
                let head = path_points(arrow);
                let base = (head[0].x + head[2].x) / 2.0;
                assert!(
                    (base - head[1].x) * (away - tip) >= 0.0,
                    "a {gap}px band's arrowhead points backwards: {arrow}"
                );
            }
        }
    }

    /// Every coordinate pair in a path command string, in order.
    fn path_points(path: &str) -> Vec<Point> {
        let numbers: Vec<f64> = path
            .replace(['M', 'C', 'L', 'Z', ','], " ")
            .split_whitespace()
            .map(|value| value.parse().expect("a path is made of numbers"))
            .collect();
        numbers
            .chunks_exact(2)
            .map(|pair| Point::new(pair[0], pair[1]))
            .collect()
    }

    #[test]
    fn a_seat_shrunk_off_the_end_of_its_run_lands_on_the_corner() {
        let narrow = Rect::new(0.0, 0.0, 120.0, 48.0);
        let clamped = clamp_seat(narrow, PortSeat::new(Side::Top, 40));
        assert_eq!(clamped, PortSeat::new(Side::Top, 10));
        assert!(seats(narrow).contains(&clamped));
    }

    /// Every seat names a place on the rim, and no place is named twice.
    #[test]
    fn the_seats_of_a_frame_are_distinct_and_on_the_rim() {
        let frame = frame(0.0, 0.0);
        let all = seats(frame);
        assert_eq!(all.len(), 2 * 18 + 2 * 4);
        let mut seen = std::collections::BTreeSet::new();
        for seat in &all {
            assert!(seen.insert(*seat), "{seat:?} named twice");
            let anchor = seat_point(frame, *seat);
            let on_rim = nearest_on_ring(frame, anchor.point());
            let back = ring_point(frame, on_rim).point();
            assert!(
                back.distance(anchor.point()) < 0.001,
                "{seat:?} is off the rim"
            );
            assert!((anchor.nx.hypot(anchor.ny) - 1.0).abs() < 0.001);
        }
    }

    #[test]
    fn every_seat_is_the_nearest_seat_to_itself() {
        let frame = frame(0.0, 0.0);
        for seat in seats(frame) {
            assert_eq!(nearest_seat(frame, seat_point(frame, seat).point()), seat);
        }
    }

    /// A port facing away from the far end used to be handed a full-length
    /// handle pointing backwards, which folded the curve past its own endpoint
    /// and reversed the arrowhead sitting on it.
    #[test]
    fn a_port_facing_away_does_not_fold_the_curve_back_past_its_end() {
        let away = Anchor {
            x: 0.0,
            y: 0.0,
            nx: -1.0,
            ny: 0.0,
        };
        let facing = Anchor {
            x: 300.0,
            y: 0.0,
            nx: -1.0,
            ny: 0.0,
        };
        let geometry = edge_geometry(away, facing, Arrows::new(false, true), 2, 0.5, true);
        let mut previous = geometry.point_at(0.0);
        let mut travelled = 0.0;
        for step in 1..=64 {
            let point = geometry.point_at(f64::from(step) / 64.0);
            travelled += previous.distance(point);
            previous = point;
        }
        let direct = geometry.point_at(0.0).distance(geometry.point_at(1.0));
        assert!(
            travelled < direct * 2.0,
            "the curve wanders {travelled} over a {direct} span"
        );
    }

    /// A band leaves along its port's own outward normal, whatever the rest of
    /// the shape is doing. The sweep that carries a backwards-facing port round
    /// its card is a chord perpendicular, and for a port facing *across* the chord
    /// that perpendicular points partly back into the card — which sent the handle
    /// behind its own bead and drew the first stretch of the band through the card
    /// it was leaving.
    #[test]
    fn every_band_leaves_its_bead_pointing_out_of_its_own_card() {
        let a = Rect::new(0.0, 0.0, 216.0, 48.0);
        let mut worst = 1.0;
        let mut at_worst = String::new();
        for offset in [
            (60.0, 180.0),
            (-300.0, 180.0),
            (300.0, -180.0),
            (0.0, -240.0),
            (480.0, 0.0),
            (-40.0, 90.0),
        ] {
            let b = Rect::new(a.x + offset.0, a.y + offset.1, 216.0, 48.0);
            for from in seats(a) {
                for to in seats(b) {
                    let geometry = edge_geometry(
                        seat_point(a, from),
                        seat_point(b, to),
                        Arrows::NONE,
                        1,
                        0.5,
                        true,
                    );
                    // The tangent at each end, against that end's own normal.
                    for (anchor, near, far) in [
                        (seat_point(a, from), 0.0, 0.02),
                        (seat_point(b, to), 1.0, 0.98),
                    ] {
                        let here = geometry.point_at(near);
                        let along = geometry.point_at(far);
                        let (dx, dy) = (along.x - here.x, along.y - here.y);
                        let length = dx.hypot(dy).max(f64::EPSILON);
                        let outward = (dx * anchor.nx + dy * anchor.ny) / length;
                        if outward < worst {
                            worst = outward;
                            at_worst = format!("{offset:?} {from:?} -> {to:?}");
                        }
                    }
                }
            }
        }
        assert!(
            worst > 0.0,
            "a band leaves its bead pointing inward ({worst}) for {at_worst}"
        );
    }

    /// Two seats facing across the chord rather than along it — two cards side
    /// by side joined from their top edges — must arch, not kink.
    #[test]
    fn ports_facing_across_the_chord_actually_curve() {
        let up = |x: f64| Anchor {
            x,
            y: 0.0,
            nx: 0.0,
            ny: -1.0,
        };
        let geometry = edge_geometry(up(0.0), up(300.0), Arrows::NONE, 2, 0.5, true);
        let middle = geometry.point_at(0.5);
        assert!(
            middle.y < -40.0,
            "the arch only reaches {} above the chord",
            -middle.y
        );
    }
}
