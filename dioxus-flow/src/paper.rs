//! The graph paper behind the map: dots on the snap grid, with a heavier mark
//! every fifth one.
//!
//! Two things make that harder than it looks, and both are settled here rather
//! than left to a CSS background.
//!
//! A single world pitch cannot serve a fifteen-fold zoom range. Held at the snap
//! grid the dots crowd into a smear on the way out; held at the heavy mark
//! instead, the paper coarsens abruptly and then crowds again. So the lattice is
//! promoted five cells at a time until it is legible, and the levels cross-fade:
//! as the fine lattice dissolves, the heavy mark takes over its weight and a new
//! heavy mark rises five times coarser again. Promotion happens exactly where the
//! dissolving level has reached nothing, so what is drawn either side of it is the
//! same paper — see `promotion_changes_nothing_that_is_drawn`.
//!
//! What draws them is a tiled SVG pattern per level, one dot to a tile — the same
//! shape React Flow's background has, and the reason panning and zooming cost a
//! handful of attribute changes rather than any drawing work. A CSS background
//! would be cheaper still, but it lays its tiles out on whole pixels: the dots
//! land up to a pixel from where the map puts them, the error grows across the
//! screen, and it shifts as the zoom changes, which is what makes a tiled
//! background snap and flicker while zooming. An SVG pattern is placed by a
//! matrix, so it stays exactly where this says it does — and
//! `every_level_lands_on_the_maps_own_grid` is what says it.

use crate::types::{Grid, Point, Viewport};

/// Dots closer together than this, in screen pixels, have dissolved: the gap is
/// down to the width of a dot and the lattice reads as haze. This is also where
/// a level is promoted, so the promotion cannot be seen.
const DISSOLVED: f64 = 6.0;
/// How many cells to a heavier mark, and so how far one promotion travels.
const RUN: f64 = 5.0;
/// A dot's radius in screen pixels, from a fine dot to a heavy mark. A painted
/// circle carries a little less ink than the hard-edged one a CSS gradient drew
/// at the same radius, so these are the radii that keep the paper's weight.
const FINE_RADIUS: f64 = 1.1;
const HEAVY_RADIUS: f64 = 1.45;
/// Below this a level contributes nothing a reader could see, so it is left out
/// of the drawing altogether.
pub const INVISIBLE: f64 = 0.02;

/// One lattice of dots, ready to paint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    /// The gap between neighbouring dots, in screen pixels.
    pub pitch: f64,
    /// The first dot at or after the canvas's top-left corner, in canvas pixels.
    /// Every other dot of this level is a whole number of pitches from it.
    pub first: Point,
    /// How present this level is: 0 draws nothing, 1 draws it in full.
    pub alpha: f64,
    /// 0 draws a fine dot, 1 a heavy mark, in between while this level is taking
    /// over the role of the one below it.
    pub weight: f64,
}

impl Level {
    pub fn radius(self) -> f64 {
        FINE_RADIUS + (HEAVY_RADIUS - FINE_RADIUS) * self.weight.clamp(0.0, 1.0)
    }

    /// Whether this level is worth asking the painter for.
    pub fn visible(self) -> bool {
        self.alpha > INVISIBLE && self.pitch.is_finite() && self.pitch > 0.0
    }

    /// How much of the heavy ink this level's dots are mixed from, as a
    /// percentage for `color-mix`. The rest is the fine ink.
    pub fn heaviness(self) -> f64 {
        self.weight.clamp(0.0, 1.0) * 100.0
    }
}

/// The levels that draw the paper, finest first. Three is what a cross-fade
/// needs: the level dissolving, the one taking its place, and the one rising
/// behind it.
pub type Paper = [Level; 3];

/// The paper for one viewport of a grid.
pub fn paper(viewport: Viewport, grid: Grid) -> Paper {
    // A lattice this far apart is drawn in full: the grid itself at 100% zoom, so
    // that zoom draws plain graph paper.
    let drawn = grid.size();
    let zoom = if viewport.zoom.is_finite() && viewport.zoom > 0.0 {
        viewport.zoom
    } else {
        1.0
    };
    let mut pitch = grid.size() * zoom;
    // Promote until the finest level is legible. One promotion covers the whole
    // zoom range; the bound is a guard against a nonsensical zoom, not a limit.
    for _ in 0..8 {
        if pitch >= DISSOLVED {
            break;
        }
        pitch *= RUN;
    }
    let blend = ((pitch - DISSOLVED) / (drawn - DISSOLVED)).clamp(0.0, 1.0);
    [
        // The snap grid itself, for as long as it can be read.
        level(viewport, pitch, blend, 0.0),
        // Every fifth cell: the heavy mark, which becomes the fine lattice of the
        // next level as the blend runs out.
        level(viewport, pitch * RUN, 1.0, blend),
        // The mark that will be the heavy one after the next promotion.
        level(viewport, pitch * RUN * RUN, 1.0 - blend, 1.0),
    ]
}

/// A level's dots sit on the grid-line intersections. Node frames snap to those
/// same boundaries, so their resting edges pass through dots rather than through
/// the middle of the spaces between them.
fn level(viewport: Viewport, pitch: f64, alpha: f64, weight: f64) -> Level {
    Level {
        pitch,
        first: Point::new(viewport.x.rem_euclid(pitch), viewport.y.rem_euclid(pitch)),
        alpha,
        weight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The zoom range the sweep covers — a superset of what any host editor
    /// allows, including the promotion boundaries.
    const MIN_ZOOM: f64 = 0.2;
    const MAX_ZOOM: f64 = 3.0;

    const GRID: Grid = Grid::new(12.0);

    fn paper(zoom: f64) -> Paper {
        super::paper(view(zoom), GRID)
    }

    fn view(zoom: f64) -> Viewport {
        Viewport {
            x: 544.3,
            y: -217.8,
            zoom,
        }
    }

    /// Every zoom a viewer can reach, plus the promotion boundaries either side.
    fn zooms() -> Vec<f64> {
        let mut zooms = vec![MIN_ZOOM, MAX_ZOOM, 1.0];
        let mut zoom = MIN_ZOOM;
        while zoom < MAX_ZOOM {
            zooms.push(zoom);
            zoom *= 1.01;
        }
        for boundary in [DISSOLVED / GRID.size(), DISSOLVED * RUN / GRID.size()] {
            zooms.extend([boundary - 1e-9, boundary, boundary + 1e-9]);
        }
        zooms.retain(|zoom| (MIN_ZOOM..=MAX_ZOOM).contains(zoom));
        zooms
    }

    /// The property that makes the paper belong to the map: every dot is drawn
    /// on a boundary of the map's own grid, at every level and zoom.
    #[test]
    fn every_level_lands_on_the_maps_own_grid() {
        for zoom in zooms() {
            let viewport = view(zoom);
            for level in paper(zoom) {
                for (axis, first, pan) in [
                    ("x", level.first.x, viewport.x),
                    ("y", level.first.y, viewport.y),
                ] {
                    let world = (first - pan) / zoom;
                    let cell = world.rem_euclid(GRID.size());
                    assert!(
                        cell.abs() < 1e-6 || (cell - GRID.size()).abs() < 1e-6,
                        "zoom {zoom}: pitch {} sits {cell} into an {axis} cell, not on its boundary",
                        level.pitch,
                    );
                }
                let step = level.pitch / zoom;
                assert!(
                    (step / GRID.size() - (step / GRID.size()).round()).abs() < 1e-6,
                    "zoom {zoom}: pitch {} is not a whole number of cells",
                    level.pitch,
                );
            }
        }
    }

    /// Crossing a promotion must not change the paper: the level that dissolved
    /// has reached nothing, and every level still drawn is drawn the same way.
    #[test]
    fn promotion_changes_nothing_that_is_drawn() {
        for boundary in [DISSOLVED / GRID.size(), DISSOLVED * RUN / GRID.size()] {
            let drawn = |zoom: f64| {
                paper(zoom)
                    .into_iter()
                    .filter(|level| level.visible())
                    .collect::<Vec<_>>()
            };
            // A hair either side of the promotion: any difference left is the
            // hair itself, not the promotion.
            let before = drawn(boundary + 1e-6);
            let after = drawn(boundary - 1e-6);
            assert_eq!(
                before.len(),
                after.len(),
                "a level appears or vanishes across the promotion at zoom {boundary}"
            );
            for (before, after) in before.iter().zip(&after) {
                let same = |name: &str, before: f64, after: f64, tolerance: f64| {
                    assert!(
                        (before - after).abs() <= tolerance,
                        "{name} jumps from {before} to {after} \
                         across the promotion at zoom {boundary}"
                    );
                };
                same("pitch", before.pitch, after.pitch, 0.01);
                same("alpha", before.alpha, after.alpha, 0.001);
                same("weight", before.weight, after.weight, 0.001);
                same("phase", before.first.x, after.first.x, 0.01);
                same("phase", before.first.y, after.first.y, 0.01);
            }
        }
    }

    /// However far out the view goes, the dots a reader can see never crowd into
    /// a smear — the reason the lattice is promoted at all.
    #[test]
    fn visible_dots_never_crowd_together() {
        for zoom in zooms() {
            for level in paper(zoom) {
                assert!(
                    !level.visible() || level.pitch >= DISSOLVED,
                    "zoom {zoom}: a visible level has dots {}px apart",
                    level.pitch,
                );
            }
        }
    }

    /// The paper reads the same at any zoom: something legible is always drawn,
    /// and never so coarse that the surface looks blank.
    #[test]
    fn some_legible_lattice_is_always_drawn() {
        for zoom in zooms() {
            let paper = paper(zoom);
            let solid = paper
                .into_iter()
                .filter(|level| level.alpha >= 0.5)
                .map(|level| level.pitch)
                .fold(f64::INFINITY, f64::min);
            assert!(
                (DISSOLVED..=GRID.size() * RUN * RUN).contains(&solid),
                "zoom {zoom}: the finest solid lattice is {solid}px",
            );
        }
    }

    #[test]
    fn levels_stay_within_their_ranges() {
        for zoom in zooms() {
            for level in paper(zoom) {
                assert!((0.0..=1.0).contains(&level.alpha), "alpha {}", level.alpha);
                assert!(
                    (0.0..=1.0).contains(&level.weight),
                    "weight {}",
                    level.weight
                );
                assert!(level.pitch.is_finite() && level.pitch > 0.0);
                assert!(level.first.x >= 0.0 && level.first.x < level.pitch);
                assert!(level.first.y >= 0.0 && level.first.y < level.pitch);
                assert!((FINE_RADIUS..=HEAVY_RADIUS).contains(&level.radius()));
            }
        }
    }

    /// A viewport that should not exist must still not hang the painter.
    #[test]
    fn a_nonsense_viewport_still_describes_a_drawable_paper() {
        for zoom in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e9] {
            for level in super::paper(
                Viewport {
                    x: 0.0,
                    y: 0.0,
                    zoom,
                },
                GRID,
            ) {
                assert!(level.pitch.is_finite() && level.pitch > 0.0, "zoom {zoom}");
                assert!(level.first.x.is_finite() && level.first.y.is_finite());
            }
        }
    }
}
