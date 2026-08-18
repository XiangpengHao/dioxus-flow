//! Turns transient rectangle geometry into a valid resting layout.
//!
//! Direct manipulation is deliberately unconstrained: a rectangle in a user's
//! hand may pass through, or temporarily cover, anything. The boundary here is
//! the other side of that gesture. Given the frames the gesture asked for, this
//! module keeps the resting layout anchored and relocates the frames the gesture
//! moved to their nearest clear landing.
//!
//! The application decides when a gesture has ended and which ids express its
//! intent. This module knows only opaque ids, rectangles, a grid and a gap.

use std::collections::{BTreeMap, BTreeSet};

use crate::types::{Grid, Id, Point, Rect};

/// Whether two rectangles come closer to one another than `gap`.
///
/// Exactly `gap` apart is clear rather than a clash: that is a valid resting
/// position and the boundary at which layout settlement stops. This predicate
/// deliberately does not constrain a gesture — something in the user's hand
/// may overlap anything; [`settled`] uses it only when transient geometry
/// becomes a resting layout.
pub fn clash(a: Rect, b: Rect, gap: f64) -> bool {
    a.x < b.x + b.width + gap
        && b.x < a.x + a.width + gap
        && a.y < b.y + b.height + gap
        && b.y < a.y + a.height + gap
}

/// Settles `frames` into a layout in which no pair comes closer than `gap`.
///
/// `moving` names the frames the gesture just authored — the node under the
/// pointer, the group travelling with it, or the node whose editor just grew.
/// When one of those clashes with a resting frame, the moving frame gives way.
/// A moving group that was clear internally stays rigid; malformed input in
/// which two moving frames already clash is still repaired deterministically.
///
/// Clear frames are returned byte-for-byte unchanged. A displaced frame moves
/// along one axis to the nearest clear grid position, and is tested against the
/// complete current layout before it is accepted, so resolving one collision
/// never creates another.
pub fn settled(
    frames: &BTreeMap<Id, Rect>,
    moving: &BTreeSet<Id>,
    gap: f64,
    grid: Grid,
) -> BTreeMap<Id, Rect> {
    let gap = if gap.is_finite() { gap.max(0.0) } else { 0.0 };
    let mut answer = frames.clone();

    // A selection travelled as one shape in the user's hand, so it must also
    // search for a landing as one shape. Translating the complete group keeps
    // every internal distance unchanged while all frames that were already at
    // rest remain anchors.
    relocate_group(&mut answer, moving, gap, grid);

    // Moving one frame clear of the complete current layout removes every
    // collision involving it and cannot introduce one. There can therefore be
    // at most one productive pass per frame; the extra pass observes completion.
    for _ in 0..=frames.len() {
        let Some((keeper, displaced)) = first_collision(&answer, moving, gap) else {
            break;
        };
        let start = answer[&displaced];
        let occupied: Vec<Rect> = answer
            .iter()
            .filter(|(id, _)| *id != &displaced)
            .map(|(_, frame)| *frame)
            .collect();
        let next = nearest_clear(start, answer[&keeper], &occupied, gap, grid);
        debug_assert_ne!(
            next, start,
            "a clashing frame must have somewhere to settle"
        );
        answer.insert(displaced, next);
    }

    debug_assert!(clear(&answer, gap), "layout settlement left an overlap");
    answer
}

/// Relocates a clear moving group by one shared grid translation. The fallback
/// loop in [`settled`] handles malformed groups that already overlap internally.
fn relocate_group(frames: &mut BTreeMap<Id, Rect>, moving: &BTreeSet<Id>, gap: f64, grid: Grid) {
    let group: Vec<Rect> = frames
        .iter()
        .filter(|(id, _)| moving.contains(*id))
        .map(|(_, frame)| *frame)
        .collect();
    let fixed: Vec<Rect> = frames
        .iter()
        .filter(|(id, _)| !moving.contains(*id))
        .map(|(_, frame)| *frame)
        .collect();
    if group.is_empty() || !clear_frames(&group, gap) || groups_clear(&group, &fixed, gap) {
        return;
    }
    let Some(offset) = nearest_group_translation(&group, &fixed, gap, grid) else {
        return;
    };
    debug_assert!(offset.x != 0.0 || offset.y != 0.0);
    for (id, frame) in frames.iter_mut() {
        if moving.contains(id) {
            *frame = translated(*frame, offset);
        }
    }
}

/// The nearest one-axis translation that clears a moving group from every
/// anchored frame. A candidate beyond the outermost anchor always exists.
fn nearest_group_translation(
    group: &[Rect],
    fixed: &[Rect],
    gap: f64,
    grid: Grid,
) -> Option<Point> {
    let mut candidates = Vec::with_capacity(group.len() * fixed.len() * 4);
    for frame in group {
        for other in fixed {
            candidates.extend([
                Point::new(
                    snap_before(other.x - frame.width - gap, grid) - frame.x,
                    0.0,
                ),
                Point::new(snap_after(other.x + other.width + gap, grid) - frame.x, 0.0),
                Point::new(
                    0.0,
                    snap_before(other.y - frame.height - gap, grid) - frame.y,
                ),
                Point::new(
                    0.0,
                    snap_after(other.y + other.height + gap, grid) - frame.y,
                ),
            ]);
        }
    }
    candidates
        .into_iter()
        .filter(|offset| {
            group.iter().all(|frame| {
                fixed
                    .iter()
                    .all(|other| !clash(translated(*frame, *offset), *other, gap))
            })
        })
        .min_by_key(|offset| translation_key(*offset))
}

fn translated(frame: Rect, offset: Point) -> Rect {
    Rect::new(
        frame.x + offset.x,
        frame.y + offset.y,
        frame.width,
        frame.height,
    )
}

fn groups_clear(group: &[Rect], fixed: &[Rect], gap: f64) -> bool {
    group
        .iter()
        .all(|frame| fixed.iter().all(|other| !clash(*frame, *other, gap)))
}

fn clear_frames(frames: &[Rect], gap: f64) -> bool {
    frames.iter().enumerate().all(|(index, frame)| {
        frames[index + 1..]
            .iter()
            .all(|other| !clash(*frame, *other, gap))
    })
}

/// The first collision in stable id order, expressed as the frame that keeps
/// its place and the one that gives way.
fn first_collision(
    frames: &BTreeMap<Id, Rect>,
    moving: &BTreeSet<Id>,
    gap: f64,
) -> Option<(Id, Id)> {
    let entries: Vec<_> = frames.iter().collect();
    for (index, (a_id, a)) in entries.iter().enumerate() {
        for (b_id, b) in &entries[index + 1..] {
            if !clash(**a, **b, gap) {
                continue;
            }
            return Some(match (moving.contains(*a_id), moving.contains(*b_id)) {
                (true, false) => ((*b_id).clone(), (*a_id).clone()),
                (false, true) => ((*a_id).clone(), (*b_id).clone()),
                // Equal mobility is resolved in stable id order. For the normal
                // gesture case, moving frames began clear and never reach this
                // branch against one another.
                _ => ((*a_id).clone(), (*b_id).clone()),
            });
        }
    }
    None
}

/// The nearest one-axis landing that clears every occupied frame.
fn nearest_clear(frame: Rect, keeper: Rect, occupied: &[Rect], gap: f64, grid: Grid) -> Rect {
    let mut candidates = Vec::with_capacity(occupied.len() * 4);
    for other in occupied {
        candidates.extend([
            Rect::new(
                snap_before(other.x - frame.width - gap, grid),
                grid.snap(frame.y),
                frame.width,
                frame.height,
            ),
            Rect::new(
                snap_after(other.x + other.width + gap, grid),
                grid.snap(frame.y),
                frame.width,
                frame.height,
            ),
            Rect::new(
                grid.snap(frame.x),
                snap_before(other.y - frame.height - gap, grid),
                frame.width,
                frame.height,
            ),
            Rect::new(
                grid.snap(frame.x),
                snap_after(other.y + other.height + gap, grid),
                frame.width,
                frame.height,
            ),
        ]);
    }

    let away_x = frame.center().x - keeper.center().x;
    let away_y = frame.center().y - keeper.center().y;
    candidates
        .into_iter()
        .filter(|candidate| occupied.iter().all(|other| !clash(*candidate, *other, gap)))
        .min_by(|a, b| {
            candidate_key(*a, frame, away_x, away_y)
                .partial_cmp(&candidate_key(*b, frame, away_x, away_y))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        // At least the candidate beyond the outermost occupied edge is clear.
        // Keeping this total makes malformed, non-finite inputs no more harmful
        // here than they already were when supplied.
        .unwrap_or(frame)
}

/// Distance first; when two clear landings are equally near, prefer the side
/// away from the frame that caused this one to give way, then a stable direction.
fn candidate_key(
    candidate: Rect,
    start: Rect,
    away_x: f64,
    away_y: f64,
) -> (Ordered, u8, u8, Ordered, Ordered) {
    let dx = candidate.x - start.x;
    let dy = candidate.y - start.y;
    let dot = dx * away_x + dy * away_y;
    let away_rank = if dot > 0.0 {
        0
    } else if dot == 0.0 {
        1
    } else {
        2
    };
    let direction = if dx > 0.0 {
        0 // right
    } else if dy > 0.0 {
        1 // down
    } else if dx < 0.0 {
        2 // left
    } else {
        3 // up
    };
    (
        Ordered(dx.abs() + dy.abs()),
        away_rank,
        direction,
        Ordered(candidate.x),
        Ordered(candidate.y),
    )
}

fn translation_key(offset: Point) -> (Ordered, u8, Ordered, Ordered) {
    let direction = if offset.x > 0.0 {
        0 // right
    } else if offset.y > 0.0 {
        1 // down
    } else if offset.x < 0.0 {
        2 // left
    } else {
        3 // up
    };
    (
        Ordered(offset.x.abs() + offset.y.abs()),
        direction,
        Ordered(offset.x),
        Ordered(offset.y),
    )
}

/// A tiny total-order wrapper for tuple comparison without making geometry's
/// public scalar type anything other than `f64`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ordered(f64);

impl Eq for Ordered {}

impl PartialOrd for Ordered {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Ordered {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

fn snap_before(value: f64, grid: Grid) -> f64 {
    (value / grid.size()).floor() * grid.size()
}

fn snap_after(value: f64, grid: Grid) -> f64 {
    (value / grid.size()).ceil() * grid.size()
}

fn clear(frames: &BTreeMap<Id, Rect>, gap: f64) -> bool {
    clear_frames(&frames.values().copied().collect::<Vec<_>>(), gap)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID: Grid = Grid::new(12.0);
    const GAP: f64 = 12.0;

    fn card(x: f64, y: f64) -> Rect {
        Rect::new(x, y, 216.0, 48.0)
    }

    fn map(entries: &[(&str, Rect)]) -> BTreeMap<Id, Rect> {
        entries
            .iter()
            .map(|(id, frame)| ((*id).into(), *frame))
            .collect()
    }

    #[test]
    fn a_gap_apart_is_clear_and_anything_closer_is_not() {
        let a = card(0.0, 0.0);
        assert!(!clash(a, Rect::new(228.0, 0.0, 216.0, 48.0), GAP));
        assert!(clash(a, Rect::new(227.0, 0.0, 216.0, 48.0), GAP));
    }

    #[test]
    fn separation_on_either_axis_is_enough() {
        let a = card(0.0, 0.0);
        assert!(!clash(a, Rect::new(0.0, 60.0, 216.0, 48.0), GAP));
        assert!(!clash(a, Rect::new(-228.0, 0.0, 216.0, 48.0), GAP));
    }

    #[test]
    fn a_clear_layout_is_returned_unchanged() {
        let frames = map(&[("a", card(0.0, 0.0)), ("b", card(240.0, 0.0))]);
        assert_eq!(settled(&frames, &BTreeSet::new(), GAP, GRID), frames);
    }

    #[test]
    fn the_dragged_frame_moves_and_the_resting_frame_stays_anchored() {
        let frames = map(&[
            ("dragged", card(240.0, 0.0)),
            ("resting", card(240.0, 0.0)),
            ("unrelated", card(1200.0, 0.0)),
        ]);
        let answer = settled(&frames, &BTreeSet::from(["dragged".into()]), GAP, GRID);

        assert_eq!(answer["dragged"], card(240.0, 60.0));
        assert_eq!(answer["resting"], frames["resting"]);
        assert_eq!(answer["unrelated"], frames["unrelated"]);
        assert!(clear(&answer, GAP));
    }

    #[test]
    fn a_dragged_group_moves_rigidly_while_its_obstacle_stays_anchored() {
        let frames = map(&[
            ("a", card(0.0, 0.0)),
            ("b", card(0.0, 60.0)),
            ("obstacle", card(0.0, 60.0)),
        ]);
        let moving = BTreeSet::from(["a".into(), "b".into()]);
        let answer = settled(&frames, &moving, GAP, GRID);

        let a_offset = Point::new(answer["a"].x - frames["a"].x, answer["a"].y - frames["a"].y);
        let b_offset = Point::new(answer["b"].x - frames["b"].x, answer["b"].y - frames["b"].y);
        assert_ne!(a_offset, Point::default());
        assert_eq!(a_offset, b_offset);
        assert_eq!(answer["obstacle"], frames["obstacle"]);
        assert!(clear(&answer, GAP));
    }

    #[test]
    fn several_overlaps_are_all_repaired_on_the_grid() {
        let frames = map(&[
            ("a", card(0.0, 0.0)),
            ("b", card(0.0, 0.0)),
            ("c", card(0.0, 0.0)),
            ("d", card(0.0, 0.0)),
        ]);
        let answer = settled(&frames, &BTreeSet::from(["a".into()]), GAP, GRID);

        assert!(clear(&answer, GAP));
        for frame in answer.values() {
            assert_eq!(GRID.snap(frame.x), frame.x);
            assert_eq!(GRID.snap(frame.y), frame.y);
        }
    }

    #[test]
    fn a_grown_node_relocates_while_the_node_below_stays_anchored() {
        let grown = Rect::new(0.0, 0.0, 216.0, 120.0);
        let below = card(0.0, 60.0);
        let frames = map(&[("grown", grown), ("below", below)]);
        let answer = settled(&frames, &BTreeSet::from(["grown".into()]), GAP, GRID);

        assert_ne!(answer["grown"], grown);
        assert_eq!(answer["below"], below);
        assert!(clear(&answer, GAP));
    }
}
