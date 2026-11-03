//! Analytic global placement: solve the quadratic wirelength system.
//!
//! Context: minimising the sum of squared distances between pins is a linear
//! problem. Each net becomes springs, fixed parts and pads are the anchors, and
//! the free parts settle where the spring forces balance. The x and y axes are
//! independent, so two small systems are solved with conjugate gradient.
//!
//! Local rules:
//! - Net model: a net of `k` pins is a clique with weight `w / (k - 1)` per pair
//!   (the standard split, so a net pulls as hard as one spring in total). Pairs on
//!   the same cell are dropped. Nets with more than `max_net_pins` pins are skipped.
//! - Energy per spring is `w * ((xa + oa) - (xb + ob))^2` where `o` is the pin
//!   offset turned with its cell. Each free cell also gets a tie of weight
//!   `regularization` to its start position, which makes the matrix positive
//!   definite and leaves a part with no net where it was.
//! - Solver: conjugate gradient started from the current positions, stopped
//!   when the residual is below `tolerance` or after `cg_bound(free)` steps.
//!   In exact arithmetic it finishes in at most one step per free cell, so a
//!   bound of twice that is a bound by count that still converges.
//!
//! Dependencies: `crates/engine/src/placement/model.rs` (`Model`).
use super::{Model, PlacementSettings, not_built};

/// One spring between two pins on different cells.
///
/// # Rules
/// - `weight` already includes the 1/(k-1) clique split and the net weight.
/// - Offsets are the pin offsets already turned with their cells.
///
/// # Dependencies
/// - crates/engine/src/placement/model.rs (`Model`, `NetSpring`)
///
/// # Do not
/// - Build springs between two pins of one cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Spring {
    /// Cell index of the first pin.
    pub a: usize,
    /// Cell index of the second pin.
    pub b: usize,
    /// Pin offset of the first pin, x and y.
    pub off_a: (f64, f64),
    /// Pin offset of the second pin, x and y.
    pub off_b: (f64, f64),
    /// Spring weight.
    pub weight: f64,
}

/// What the solver returns.
///
/// # Rules
/// - `positions` lists every free cell as `(cell index, x, y)` in cell order.
/// - `iterations` is the larger step count of the two axes.
///
/// # Dependencies
/// - crates/engine/src/placement/mod.rs (`place_global` writes the positions back)
///
/// # Do not
/// - Include a fixed cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Solution {
    /// Free cell positions.
    pub positions: Vec<(usize, f64, f64)>,
    /// Conjugate gradient steps used.
    pub iterations: usize,
    /// Nets skipped for having more than `max_net_pins` pins.
    pub skipped_nets: usize,
}

/// Turn the nets of `model` into springs (slice 1121-b).
///
/// Why: the solver wants a flat list of pairs. Nets are taken in model order,
/// pairs `(i, j)` with `i < j` in pin order, so the summation order (and with it
/// the last bits of the answer) is the same for every implementation.
///
/// # Rules
/// - Weight is `net.weight / (k - 1)` for a net of `k` pins.
/// - Skip nets with `k < 2` or `k > settings.max_net_pins`, and pairs on one cell.
/// - Offsets are turned with `model::turn_offset` by the cell's turn count.
///
/// # Dependencies
/// - crates/engine/src/placement/model.rs (`Model`, `turn_offset`)
///
/// # Do not
/// - Sort or reorder pairs.
pub fn build_springs(_model: &Model, _settings: &PlacementSettings) -> Result<Vec<Spring>, String> {
    Err(not_built("slice-1121-b"))
}

/// Solve both axes by conjugate gradient (slice 1121-b).
///
/// Why: this is the analytic placement. For each axis build `A x = b` over the
/// free cells (diagonal gets the sum of its spring weights plus
/// `regularization`; an off diagonal gets minus the weight between two free
/// cells; a spring to a fixed cell adds `weight * (fixed + off_b - off_a)` to the
/// right side), then iterate from the current positions.
///
/// # Rules
/// - Per axis: `r = b - A x`, `p = r`, then at most `cg_bound(free)` steps of
///   `alpha = rr / (p . A p)`, `x += alpha p`, `r -= alpha A p`,
///   `beta = rr_new / rr`, `p = r + beta p`; stop when `rr <= tolerance^2`.
/// - Matrix free: apply `A` by walking the springs, never store it.
/// - Fixed cells and cells with no spring keep their position.
/// - Refuse more than `max_cells` free cells and a model with no free cell.
///
/// # Dependencies
/// - crates/engine/src/placement/quadratic.rs (`build_springs`)
///
/// # Do not
/// - Use random numbers or time; the solve is a pure function of the model.
pub fn solve(_model: &Model, _settings: &PlacementSettings) -> Result<Solution, String> {
    Err(not_built("slice-1121-b"))
}
