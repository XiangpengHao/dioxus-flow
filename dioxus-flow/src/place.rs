//! Where something new goes when nothing said exactly where.
//!
//! Each step is separate on purpose: a pointer position is honoured exactly, so
//! only the steps a caller asks for are taken. What size a new thing is, and what
//! counts as occupied, belong to the document — these take both as arguments.

use crate::types::{Grid, Point, Rect};

/// The frame an item of `width` by `height` occupies centred on `centre`, snapped
/// to the grid.
pub fn centred(centre: Point, width: f64, height: f64, grid: Grid) -> Rect {
    grid.snap_rect(Rect::new(
        centre.x - width / 2.0,
        centre.y - height / 2.0,
        width,
        height,
    ))
}

/// The frame of a new item that a band was just dropped at, sized `width` by
/// `height` and placed so `at` lands on the edge facing `source`.
///
/// Centring it on the drop instead would swallow the pointer inside the new
/// card and, for anything wider than the drag, shove the card somewhere the
/// pointer never was. The band is what the pointer is holding, so it is the band
/// that has to stay put: the card grows away from the drop, in whichever
/// direction the drag was already going.
pub fn beside(at: Point, source: Rect, width: f64, height: f64, grid: Grid) -> Rect {
    let centre = source.center();
    let away = Point::new(at.x - centre.x, at.y - centre.y);
    let frame = if away.x.abs() >= away.y.abs() {
        Rect::new(
            if away.x >= 0.0 { at.x } else { at.x - width },
            at.y - height / 2.0,
            width,
            height,
        )
    } else {
        Rect::new(
            at.x - width / 2.0,
            if away.y >= 0.0 { at.y } else { at.y - height },
            width,
            height,
        )
    };
    grid.snap_rect(frame)
}

/// Pushes `frame` out past `source` along whichever axis `frame` is already
/// further along, leaving `gap` between them. A frame already clear of `source` is
/// left alone — this is for something dropped so close to where it came from that
/// it would land on top of it.
pub fn pushed_clear(frame: Rect, source: Rect, gap: f64, grid: Grid) -> Rect {
    if !source.expanded(gap).intersects(frame) {
        return frame;
    }
    let centre = frame.center();
    let away = Point::new(centre.x - source.center().x, centre.y - source.center().y);
    let mut frame = frame;
    if away.x.abs() >= away.y.abs() {
        frame.x = if away.x >= 0.0 {
            source.x + source.width + gap
        } else {
            source.x - frame.width - gap
        };
        frame.y = grid.snap(centre.y - frame.height / 2.0);
    } else {
        frame.y = if away.y >= 0.0 {
            source.y + source.height + gap
        } else {
            source.y - frame.height - gap
        };
        frame.x = grid.snap(centre.x - frame.width / 2.0);
    }
    frame
}

/// Jumps `frame` down past whatever is covering it until it clears everything in
/// `occupied`, and reports where it came to rest.
///
/// Each pass goes to the far side of every obstacle it is currently touching, so
/// it retires at least one of them for good — a card taller than the frame's own
/// row included, which stepping down by that row would never get past. That also
/// bounds the walk by the number of obstacles rather than by the distance.
pub fn settled_below(frame: Rect, occupied: &[Rect], gap: f64, grid: Grid) -> Point {
    let mut frame = frame;
    for _ in 0..=occupied.len() {
        let Some(below) = occupied
            .iter()
            .filter(|other| other.expanded(gap).intersects(frame))
            .map(|other| other.y + other.height + gap)
            .reduce(f64::max)
        else {
            break;
        };
        // Touching counts as intersecting, so a frame parked exactly `gap` clear
        // of what is above it still reads as covered. It is already where it
        // belongs, and moving it would be moving it to where it already is.
        if below <= frame.y {
            break;
        }
        frame.y = below;
    }
    grid.snap_point(Point::new(frame.x, frame.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID: Grid = Grid::new(12.0);

    #[test]
    fn a_clear_spot_is_used_exactly() {
        let frame = centred(Point::new(0.0, 0.0), 216.0, 48.0, GRID);
        assert_eq!(
            settled_below(frame, &[], 48.0, GRID),
            Point::new(frame.x, frame.y)
        );
    }

    #[test]
    fn a_taken_spot_is_stepped_past_and_the_result_clears_everything() {
        let taken = [
            Rect::new(-108.0, -24.0, 216.0, 48.0),
            Rect::new(-108.0, 96.0, 216.0, 48.0),
        ];
        let frame = centred(Point::new(0.0, 0.0), 216.0, 48.0, GRID);
        let at = settled_below(frame, &taken, 48.0, GRID);
        let placed = Rect::new(at.x, at.y, frame.width, frame.height);
        assert!(taken.iter().all(|other| !other.intersects(placed)));
        assert!(at.y > frame.y, "it should have moved down, not up");
    }

    /// One obstacle taller than the frame's own row used to defeat the walk
    /// entirely: it stepped down by a row a bounded number of times and gave up
    /// still inside the card it was trying to clear.
    #[test]
    fn a_tall_obstacle_is_cleared_however_much_taller_than_the_new_item_it_is() {
        let tall = [Rect::new(0.0, 0.0, 216.0, 1200.0)];
        let frame = centred(Point::new(108.0, 24.0), 216.0, 48.0, GRID);
        let at = settled_below(frame, &tall, 12.0, GRID);
        let placed = Rect::new(at.x, at.y, frame.width, frame.height);

        assert!(
            !tall[0].intersects(placed),
            "{placed:?} is still inside {:?}",
            tall[0]
        );
    }

    #[test]
    fn a_drop_beside_its_source_is_pushed_off_the_source() {
        let source = Rect::new(0.0, 0.0, 216.0, 48.0);
        let gap = 48.0;
        for towards in [
            Point::new(240.0, 10.0),
            Point::new(-240.0, 10.0),
            Point::new(10.0, 240.0),
            Point::new(10.0, -240.0),
        ] {
            let frame = centred(towards, 216.0, 48.0, GRID);
            let pushed = pushed_clear(frame, source, gap, GRID);
            assert!(
                !source.expanded(gap - 1.0).intersects(pushed),
                "a drop towards {towards:?} stayed on its source",
            );
        }
    }

    #[test]
    fn a_drop_already_clear_of_its_source_is_left_where_it_is() {
        let source = Rect::new(0.0, 0.0, 216.0, 48.0);
        let frame = centred(Point::new(600.0, 600.0), 216.0, 48.0, GRID);
        assert_eq!(pushed_clear(frame, source, 48.0, GRID), frame);
    }

    /// What the pointer is holding is the band, so the drop point stays on the
    /// new card's edge and the card grows away from it.
    #[test]
    fn a_new_item_grows_away_from_the_drop_rather_than_around_it() {
        let source = Rect::new(0.0, 0.0, 216.0, 48.0);
        let (width, height) = (216.0, 48.0);
        for (at, edge) in [
            (Point::new(600.0, 24.0), "left"),
            (Point::new(-600.0, 24.0), "right"),
            (Point::new(108.0, 600.0), "top"),
            (Point::new(108.0, -600.0), "bottom"),
        ] {
            let frame = beside(at, source, width, height, GRID);
            let touching = match edge {
                "left" => frame.x,
                "right" => frame.x + frame.width,
                "top" => frame.y,
                _ => frame.y + frame.height,
            };
            let along = if matches!(edge, "left" | "right") {
                at.x
            } else {
                at.y
            };
            assert!(
                (touching - along).abs() <= GRID.size() / 2.0,
                "dropping at {at:?} put the {edge} edge at {touching}, not {along}"
            );
            assert!(
                !source.intersects(frame),
                "dropping at {at:?} landed on the card it came from"
            );
        }
    }
}
