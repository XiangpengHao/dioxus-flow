//! When a press has become a drag.
//!
//! This is one number and one comparison, and it is here on purpose. Whether a
//! press is still a click decides three separate things — whether a gesture has
//! started, whether releasing counts as a click, and when the pointer may be
//! captured — and the last of those is easy to get wrong: capturing on the press
//! itself makes the browser re-target `click` and `dblclick` at the capturing
//! element, so every double-click on anything inside the surface is lost. Capture
//! belongs to a drag, not to a press, and [`Press::is_drag`] is what says which
//! one is in hand.

use crate::types::Point;

/// How far a mouse or trackpad must travel before a press is a drag, in surface
/// pixels.
pub const PRECISE_THRESHOLD: f64 = 4.0;
/// The same for touch and pen, which are noisier.
pub const COARSE_THRESHOLD: f64 = 8.0;

/// The press a gesture began with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Press {
    /// Where it landed, in surface pixels.
    pub at: Point,
    /// How far it has to travel to become a drag.
    pub threshold: f64,
}

impl Press {
    pub const fn new(at: Point, threshold: f64) -> Self {
        Self { at, threshold }
    }

    /// A press from a pointer that reports its own position precisely, or not.
    pub const fn from_pointer(at: Point, coarse: bool) -> Self {
        Self::new(
            at,
            if coarse {
                COARSE_THRESHOLD
            } else {
                PRECISE_THRESHOLD
            },
        )
    }

    /// Whether the pointer has travelled far enough for this to be a drag.
    pub fn is_drag(self, now: Point) -> bool {
        self.at.distance(now) >= self.threshold
    }

    /// The same question for a pointer tracked in map coordinates, where the
    /// threshold still means screen pixels however far the view is zoomed.
    pub fn is_drag_in_world(self, moved: f64, zoom: f64) -> bool {
        moved * zoom >= self.threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_still_pointer_is_never_a_drag_and_a_travelled_one_always_is() {
        let press = Press::from_pointer(Point::new(100.0, 100.0), false);
        assert!(!press.is_drag(press.at));
        assert!(!press.is_drag(Point::new(103.0, 100.0)));
        assert!(press.is_drag(Point::new(104.0, 100.0)));
        assert!(press.is_drag(Point::new(97.0, 97.0)));
    }

    #[test]
    fn touch_is_allowed_more_wobble_than_a_mouse() {
        let at = Point::new(0.0, 0.0);
        let wobble = Point::new(6.0, 0.0);
        assert!(Press::from_pointer(at, false).is_drag(wobble));
        assert!(!Press::from_pointer(at, true).is_drag(wobble));
    }

    /// The threshold is a screen distance, so the further in the view is zoomed
    /// the less of the map a drag has to cross.
    #[test]
    fn the_threshold_means_screen_pixels_at_any_zoom() {
        let press = Press::from_pointer(Point::new(0.0, 0.0), false);
        assert!(!press.is_drag_in_world(3.0, 1.0));
        assert!(press.is_drag_in_world(3.0, 2.0));
        assert!(!press.is_drag_in_world(3.0, 0.5));
    }
}
