//! Legalisation: remove overlaps and edge violations with the fewest moves.
//!
//! Context: the analytic solve stacks parts on top of each other. The
//! legaliser keeps every part that is already legal where it is and moves only
//! the others, each to the nearest free spot of a search lattice.
//!
//! Local rules:
//! - Order of work: free cells sorted by box area, largest first, then by index.
//!   A free cell whose box is already legal stays; otherwise it moves. So when two
//!   free parts overlap, the one with the smaller box (higher index when equal)
//!   moves, and a fixed part never does.
//! - Why not pairwise pushes: a pairwise push loop was measured on stacks of equal
//!   parts at one point. Ten parts needed 42 passes, twenty needed 112 and forty
//!   did not finish in 300, so a pass bound would have to be guessed. The lattice
//!   search is one pass with a ring bound and always finishes.
//! - A spot is free when the box lies inside the board shrunk by
//!   `edge_clearance_mm` and overlaps no fixed part, keepout or already placed
//!   part (see `model::boxes_overlap`, with `clearance_mm` and
//!   `overlap_epsilon_mm`).
//! - Lattice: step `grid_mm` when set (origin is the start position rounded to
//!   the grid, `f64::round`), else `lattice_step_mm` with the start position as
//!   origin. Ring `r` holds the points with Chebyshev distance `r`, taken row by
//!   row from the lowest y; ring 0 is the origin. The first ring with a free spot
//!   wins; inside it the smallest Euclid distance to the start wins; equal
//!   distances (within 1e-9) are broken with one `Rng::below` draw over the tied
//!   spots, in ring order, only when there is more than one.
//!
//! Dependencies: `crates/engine/src/placement/model.rs`, `rng.rs`.
use super::{Model, PlacementSettings, Rng, not_built};

/// What the legaliser reports.
///
/// # Rules
/// - `moved` lists cell indexes that changed position, in the order processed.
/// - `unresolved` lists free cells that found no free spot within `max_rings`;
///   they stay where they were.
/// - `fixed_overlaps` lists pairs of fixed cells that overlap, lower index first.
///
/// # Dependencies
/// - crates/engine/src/placement/mod.rs (`place_global` copies these)
///
/// # Do not
/// - Treat a non empty `unresolved` as success.
#[derive(Clone, Debug, PartialEq)]
pub struct LegalizeReport {
    /// Cells that moved.
    pub moved: Vec<usize>,
    /// Free cells with no spot.
    pub unresolved: Vec<usize>,
    /// Overlapping pairs of fixed cells.
    pub fixed_overlaps: Vec<(usize, usize)>,
}

/// Remove overlaps, keepout hits and edge violations (slice 1121-c).
///
/// Why: the command must hand back a layout the courtyard and edge checks
/// accept, moving as little as possible. See the module notes for the method.
///
/// # Rules
/// - Changes `model` positions in place; never touches a fixed cell or a turn.
/// - Cells already legal against fixed parts, keepouts and earlier placed cells
///   keep their exact position, even off the grid.
/// - With `grid_mm` set a moved cell lands on the grid.
/// - Deterministic for one seed: the only draw is the tie break.
///
/// # Dependencies
/// - crates/engine/src/placement/model.rs (`boxes_overlap`, `Rect`)
/// - crates/engine/src/placement/rng.rs (`Rng::below`)
///
/// # Do not
/// - Search past `max_rings`, or loop on time.
pub fn legalize(
    _model: &mut Model,
    _settings: &PlacementSettings,
    _rng: &mut Rng,
) -> Result<LegalizeReport, String> {
    Err(not_built("slice-1121-c"))
}
