//! Bounded simulated annealing polish with hard rule rejection.
//!
//! Context: after the global pass and the legaliser, small moves can still
//! shorten wire. The annealer tries translate, quarter turn and swap moves,
//! never breaks a hard rule, and keeps the best state it saw.
//!
//! Local rules:
//! - Cost is plain total HPWL (`Model::hpwl_of`) plus `overlap_weight` times
//!   the overlap area of cell boxes with each other and with keepouts. The
//!   weight is 100 mm of wire per square millimetre: a translate moves a pin at
//!   most 2 mm along each axis (4 * step), so it changes one net by at most 4 mm,
//!   and a part rarely sits on more than 20 nets, so the wire gain of a move stays
//!   under 80 mm and cannot pay for a square millimetre of overlap.
//! - Hard rules: a move is rejected when it touches a fixed cell, or when the
//!   touched cells have more overlaps, more keepout hits or more edge
//!   violations afterwards than before. A start that already overlaps can
//!   therefore improve, and no move can make it worse.
//! - Draw order per move (it is part of the golden vectors): one `next_f64` for
//!   the kind (below `translate_percent` translate, below `translate_percent +
//!   rotate_percent` rotate, else swap), one `below(movable.len())` for the cell;
//!   translate then draws `below(8)` for the direction (compass order east,
//!   north-east, north, north-west, west, south-west, south, south-east) and
//!   `below(3)` for the step multiplier; swap draws `below(n)` over the other
//!   movable cells of the same size within 1e-6 (and falls back to translate when
//!   there is none); rotate draws nothing more. The acceptance test draws one
//!   `next_f64`, and only when the cost went up and the hard rules allowed it.
//! - Schedule: geometric. Start temperature `T0 = -mean_uphill / ln(start_accept)`
//!   so `exp(-mean_uphill / T0)` equals 0.8 and about 80 percent of average
//!   uphill moves are accepted first. The mean comes from `probe_moves` trial
//!   moves drawn from `Rng::new(seed ^ PROBE_SALT)` and undone; no uphill sample
//!   gives `fallback_temperature`. Every `epoch_len(moves)` moves, `T *= cooling`.
//! - End: the move budget is used, or a window of `freeze_window` (100) moves
//!   accepted fewer than `freeze_accept` (1 percent) of them, that is none. The
//!   window is separate from the cooling epoch because an epoch can be a single
//!   move for a small budget, and one refused move must not stop a run. The best
//!   state seen is restored, so cost never ends above its start.
//!
//! Dependencies: `crates/engine/src/placement/model.rs`, `rng.rs`, `mod.rs`.
use super::{AnnealOutcome, Candidate, Model, Move, PlacementSettings, Rng, Verdict, not_built};

/// XOR mask that separates the probe stream from the main stream.
pub const PROBE_SALT: u64 = 0xA5A5_A5A5_A5A5_A5A5;

/// Draw the next move (slice 1122-a).
///
/// Why: the move mix and the draw order are what make a run reproducible.
///
/// # Rules
/// - Follow the draw order in the module notes exactly.
/// - `movable` holds cell indexes in ascending order, none of them fixed.
/// - Returns `Ok(None)` when `movable` is empty.
/// - Translate offsets are `direction * step_mm * multiplier` on each axis.
///
/// # Dependencies
/// - crates/engine/src/placement/rng.rs (`Rng`)
/// - crates/engine/src/placement/mod.rs (`Move`, `PlacementSettings`)
///
/// # Do not
/// - Name a fixed cell, or draw more or fewer numbers than the notes say.
pub fn next_move(
    _rng: &mut Rng,
    _movable: &[usize],
    _model: &Model,
    _settings: &PlacementSettings,
) -> Result<Option<Move>, String> {
    Err(not_built("slice-1122-a"))
}

/// Apply a move to the model (slice 1122-a).
///
/// # Rules
/// - Translate adds the offsets, rotate adds one turn modulo 4, swap exchanges
///   the two centres and leaves turns alone.
/// - Refuses an index out of range and a move that names a fixed cell.
///
/// # Dependencies
/// - crates/engine/src/placement/model.rs (`Cell`)
///
/// # Do not
/// - Apply a move to a fixed cell.
pub fn apply_move(_model: &mut Model, _mv: &Move) -> Result<(), String> {
    Err(not_built("slice-1122-a"))
}

/// Undo a move exactly (slice 1122-a).
///
/// # Rules
/// - `apply_move` then `undo_move` restores the model bit for bit for rotate
///   and swap; translate subtracts the same offsets.
///
/// # Dependencies
/// - crates/engine/src/placement/anneal.rs (`apply_move`)
///
/// # Do not
/// - Call it for a move that was not applied.
pub fn undo_move(_model: &mut Model, _mv: &Move) -> Result<(), String> {
    Err(not_built("slice-1122-a"))
}

/// Count the hard rule violations of one cell (slice 1122-b).
///
/// Why: the gate compares this before and after a move.
///
/// # Rules
/// - Returns `(overlaps, keepout_hits, edge)`: other cells whose box overlaps
///   this one by more than `overlap_epsilon_mm` (with `clearance_mm`), keepouts
///   overlapped the same way, and 1 when the box is not inside the board
///   shrunk by `edge_clearance_mm`.
///
/// # Dependencies
/// - crates/engine/src/placement/model.rs (`boxes_overlap`, `Rect`)
///
/// # Do not
/// - Count a pair twice for one cell.
pub fn violations(
    _model: &Model,
    _settings: &PlacementSettings,
    _cell: usize,
) -> Result<(usize, usize, usize), String> {
    Err(not_built("slice-1122-b"))
}

/// Judge a move against the hard rules (slice 1122-b).
///
/// # Rules
/// - `Locked` when any named cell is fixed; else apply the move, sum
///   `violations` over the touched cells before and after, undo the move, and
///   return the first of `Overlap`, `Keepout`, `Edge` whose count rose; else
///   `Allowed`. The model is unchanged on return.
///
/// # Dependencies
/// - crates/engine/src/placement/anneal.rs (`violations`, `apply_move`, `undo_move`)
///
/// # Do not
/// - Let a move through because its wirelength is better.
pub fn judge(
    _model: &mut Model,
    _settings: &PlacementSettings,
    _mv: &Move,
) -> Result<Verdict, String> {
    Err(not_built("slice-1122-b"))
}

/// Cost of a model: HPWL plus the overlap penalty (slice 1122-c).
///
/// # Rules
/// - `model.hpwl_of()` plus `overlap_weight` times the sum, over cell pairs and
///   cell and keepout pairs, of the overlap area `min(ox, 2 * min(hx)) *
///   min(oy, 2 * min(hy))` where `ox`, `oy` are the overlaps with `clearance_mm`
///   added and only positive overlaps count.
///
/// # Dependencies
/// - crates/engine/src/placement/model.rs (`Model::hpwl_of`)
///
/// # Do not
/// - Weight nets here; the reported number is plain HPWL.
pub fn cost(_model: &Model, _settings: &PlacementSettings) -> Result<f64, String> {
    Err(not_built("slice-1122-c"))
}

/// Start temperature from the mean uphill cost (slice 1122-c).
///
/// # Rules
/// - Use `Rng::new(seed ^ PROBE_SALT)`, draw `probe_moves` moves with
///   `next_move`, skip any the gate refuses, measure each cost change by apply,
///   `cost`, undo; mean of the positive changes `m`;
///   `T0 = -m / ln(start_accept)`.
/// - No positive change gives `fallback_temperature`.
///
/// # Dependencies
/// - crates/engine/src/placement/anneal.rs (`next_move`, `judge`, `cost`)
///
/// # Do not
/// - Touch the main stream or leave the model changed.
pub fn start_temperature(
    _model: &mut Model,
    _movable: &[usize],
    _settings: &PlacementSettings,
) -> Result<f64, String> {
    Err(not_built("slice-1122-c"))
}

/// Score one move without keeping it (slice 1122-c).
///
/// # Rules
/// - Verdict from `judge`; when `Allowed`, `delta_cost` is cost after minus
///   `cost_now`. The model is unchanged on return.
///
/// # Dependencies
/// - crates/engine/src/placement/anneal.rs (`judge`, `cost`)
///
/// # Do not
/// - Draw from the generator.
pub fn evaluate(
    _model: &mut Model,
    _settings: &PlacementSettings,
    _mv: Move,
    _cost_now: f64,
) -> Result<Candidate, String> {
    Err(not_built("slice-1122-c"))
}

/// The annealing loop (slice 1122-d).
///
/// Why: this ties the pieces together under a count bound. It follows the
/// schedule in the module notes and returns the outcome of the best state.
///
/// # Rules
/// - `movable` is every non fixed cell, ascending. Refuse more than
///   `max_polish_cells`.
/// - Per move: `next_move`, `evaluate`; a refused verdict counts one rule
///   rejection and draws nothing more; a cost drop is accepted with no draw; a
///   rise draws one `next_f64` and is accepted when it is below
///   `exp(-delta / T)`.
/// - Track the best state; restore it at the end.
/// - Record the first `trace_len` decisions.
///
/// # Dependencies
/// - crates/engine/src/placement/anneal.rs (every function above)
///
/// # Do not
/// - Read the clock, or accept a move the gate refused.
pub fn anneal(_model: &mut Model, _settings: &PlacementSettings) -> Result<AnnealOutcome, String> {
    Err(not_built("slice-1122-d"))
}
