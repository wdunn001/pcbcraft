//! Automatic part placement: analytic global placement, a legaliser and a
//! bounded simulated annealing polish (issues 1121 and 1122, parent 97).
//!
//! Context: the owner's method is that a stronger model frames the math and
//! small slices fill the functions. This tree holds the settings with every
//! number named and justified, the plain data types, the seeded generator, and
//! the function signatures. Functions that are not built yet return
//! `Err("not built yet: slice-<issue>-<letter>")` naming the slice that fills
//! them; the ignored tests in `crates/engine/tests/placement/` say what each
//! slice must make pass.
//!
//! Local rules:
//! - Every loop is bounded by a count, never by time. Same input and same seed
//!   give the same output, byte for byte.
//! - Wasm first: no `std::fs`, `std::time`, temp dirs or process ids here.
//! - Hard rules are never traded for wirelength: a locked part never moves, a
//!   move that adds a courtyard overlap, a keepout hit or an edge violation is
//!   rejected and counted.
//! - Order of work is `place_global` (solve, then legalise) and then `polish`.
//!
//! Dependencies: `crates/engine/src/wirelength.rs` (the metric these steps
//! agree with), `crates/engine/src/scope.rs` (what may move).
use crate::scope::Scope;
use pcbcraft_doc::Project;

pub mod anneal;
pub mod legalize;
pub mod model;
pub mod quadratic;
pub mod rng;

use anneal::anneal;
pub use model::{Cell, Model, NetSpring, Pin, Rect, build_model};
pub use rng::Rng;

/// The error text of a function that a later slice fills.
///
/// # Rules
/// - Always `not built yet: <slice key>`; tests and slices search for it.
pub fn not_built(slice: &str) -> String {
    format!("not built yet: {slice}")
}

/// Every number the placement steps use, named, defaulted and justified.
///
/// Why each default (the PR body repeats this as a table):
/// - `power_net_weight` 0.1: one decade under signal nets, so a ground net with
///   dozens of pins cannot pull the whole board into a clump; the signal nets
///   decide where parts go.
/// - `max_net_pins` 64: a clique of 64 pins is 2016 springs; larger nets are
///   clocks, resets and rails whose pull is meaningless, so they are skipped.
/// - `regularization` 1e-6: ties each free part to where it started so the
///   system is never singular; the pull is a millionth of a net, bias under
///   0.00003 mm on the golden vectors.
/// - `iterations_per_cell` 2, `max_iterations` 4000: conjugate gradient is exact
///   in at most one step per free part; twice that absorbs rounding.
///
/// # Rules
/// - Call `validate` before use; it refuses every out of range value.
/// - `Default` is the reviewed set. Change a number only with its reason.
///
/// # Dependencies
/// - crates/engine/src/placement/quadratic.rs (net model and solver numbers)
/// - crates/engine/src/placement/legalize.rs (legaliser numbers)
/// - crates/engine/src/placement/anneal.rs (annealing numbers)
///
/// # Do not
/// - Add a number to a step without adding it here.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementSettings {
    /// Seed of the random generator. Default 0.
    pub seed: u64,
    /// Spring weight of power and ground nets. Default 0.1.
    pub power_net_weight: f64,
    /// Spring weight of every other net. Default 1.0.
    pub signal_net_weight: f64,
    /// Nets with more pins than this are skipped by the solver. Default 64.
    pub max_net_pins: usize,
    /// A part whose box is this close to the board edge is an anchor, in
    /// millimetres. Default 1.0: twice the usual 0.5 mm copper to edge rule, so
    /// flush connectors count and parts with a real gap stay free.
    pub edge_anchor_mm: f64,
    /// Most free parts one run may move. Default 2000, the cap of `layout.report`.
    pub max_cells: usize,
    /// Pull of each free part toward its start position. Default 1e-6.
    pub regularization: f64,
    /// Conjugate gradient step bound; `None` means `cg_bound(free parts)`.
    pub iterations: Option<usize>,
    /// Conjugate gradient steps allowed per free part. Default 2.
    pub iterations_per_cell: usize,
    /// Hard cap of conjugate gradient steps. Default 4000.
    pub max_iterations: usize,
    /// Stop when the residual length is below this. Default 1e-9.
    pub tolerance: f64,
    /// Space kept between a part box and the board edge, in millimetres.
    /// Default 0.5, the common fabrication minimum for copper to edge.
    pub edge_clearance_mm: f64,
    /// Space kept between two part boxes, in millimetres. Default 0.0:
    /// courtyards already carry their own margin.
    pub clearance_mm: f64,
    /// Overlap smaller than this does not count, in millimetres. Default 1e-9.
    pub overlap_epsilon_mm: f64,
    /// Grid moved parts snap to, in millimetres; `None` means no grid.
    /// Accepted range 0.01 to 10.
    pub grid_mm: Option<f64>,
    /// Spacing of the legaliser's search lattice without a grid. Default 0.25.
    pub lattice_step_mm: f64,
    /// Rings the legaliser searches around a part. Default 400 (100 mm).
    pub max_rings: usize,
    /// Base translate step of the annealer, in millimetres. Default 0.5.
    pub step_mm: f64,
    /// Multipliers of the base step, picked uniformly. Default 1, 2 and 4.
    pub step_multipliers: [f64; 3],
    /// Chance of a translate move in percent. Default 60.
    pub translate_percent: u32,
    /// Chance of a quarter turn in percent. Default 20; swaps take the rest.
    pub rotate_percent: u32,
    /// Cost in millimetres of wire per square millimetre of overlap. Default 100.
    pub overlap_weight: f64,
    /// Share of uphill moves accepted at the start. Default 0.8.
    pub start_accept: f64,
    /// Trial moves used to measure the mean uphill cost. Default 100.
    pub probe_moves: usize,
    /// Start temperature when the probe finds no uphill move. Default 1.0.
    pub fallback_temperature: f64,
    /// Temperature factor per epoch. Default 0.9.
    pub cooling: f64,
    /// Epochs the move budget is split into. Default 50.
    pub epochs: usize,
    /// Stop when fewer than this share of a window's moves are accepted. Default 0.01.
    pub freeze_accept: f64,
    /// Moves per freeze check. Default 100, so 1 percent means fewer than one
    /// accepted move; a shorter window would freeze on noise.
    pub freeze_window: usize,
    /// Move budget; `None` means `moves_for(movable parts)`.
    pub moves: Option<usize>,
    /// Moves per movable part when the budget is automatic. Default 50.
    pub moves_per_part: usize,
    /// Smallest automatic budget. Default 500.
    pub min_moves: usize,
    /// Largest budget, automatic or asked for. Default 20000.
    pub max_moves: usize,
    /// Most movable parts one polish may take. Default 500.
    pub max_polish_cells: usize,
    /// How many of the first decisions to record. Default 0, at most 256.
    pub trace_len: usize,
}

impl Default for PlacementSettings {
    fn default() -> Self {
        Self {
            seed: 0,
            power_net_weight: 0.1,
            signal_net_weight: 1.0,
            max_net_pins: 64,
            edge_anchor_mm: 1.0,
            max_cells: 2000,
            regularization: 1e-6,
            iterations: None,
            iterations_per_cell: 2,
            max_iterations: 4000,
            tolerance: 1e-9,
            edge_clearance_mm: 0.5,
            clearance_mm: 0.0,
            overlap_epsilon_mm: 1e-9,
            grid_mm: None,
            lattice_step_mm: 0.25,
            max_rings: 400,
            step_mm: 0.5,
            step_multipliers: [1.0, 2.0, 4.0],
            translate_percent: 60,
            rotate_percent: 20,
            overlap_weight: 100.0,
            start_accept: 0.8,
            probe_moves: 100,
            fallback_temperature: 1.0,
            cooling: 0.9,
            epochs: 50,
            freeze_accept: 0.01,
            freeze_window: 100,
            moves: None,
            moves_per_part: 50,
            min_moves: 500,
            max_moves: 20_000,
            max_polish_cells: 500,
            trace_len: 0,
        }
    }
}

impl PlacementSettings {
    /// Refuse a value outside its allowed range, with a plain sentence.
    ///
    /// # Rules
    /// - Weights above 0 and at most 1000; counts at least 1; percents sum to at
    ///   most 100; `start_accept` strictly between 0 and 1; `cooling` strictly
    ///   between 0 and 1; `grid_mm` between 0.01 and 10 when set;
    ///   `iterations` 1 to `max_iterations`; `moves` 1 to `max_moves`;
    ///   `trace_len` at most 256.
    pub fn validate(&self) -> Result<(), String> {
        let weight = |name: &str, v: f64| {
            if v.is_finite() && v > 0.0 && v <= 1000.0 {
                Ok(())
            } else {
                Err(format!("{name} must be above 0 and at most 1000"))
            }
        };
        weight("power_net_weight", self.power_net_weight)?;
        weight("signal_net_weight", self.signal_net_weight)?;
        weight("overlap_weight", self.overlap_weight)?;
        if self.max_net_pins < 2
            || self.max_cells < 1
            || self.max_rings < 1
            || self.epochs < 1
            || self.freeze_window < 1
        {
            return Err(
                "pin, cell, ring, epoch and window counts must be at least 1 (pins at least 2)"
                    .into(),
            );
        }
        if self.max_iterations < 1 || self.max_iterations > 100_000 || self.iterations_per_cell < 1
        {
            return Err("iteration bounds must be at least 1 and the cap at most 100000".into());
        }
        if self
            .iterations
            .is_some_and(|n| n < 1 || n > self.max_iterations)
        {
            return Err("iterations must be between 1 and max_iterations".into());
        }
        if self.moves.is_some_and(|n| n < 1 || n > self.max_moves) {
            return Err("moves must be between 1 and max_moves".into());
        }
        if self.translate_percent + self.rotate_percent > 100 {
            return Err("translate and rotate percent together may not pass 100".into());
        }
        let open_unit = |v: f64| v.is_finite() && v > 0.0 && v < 1.0;
        if !open_unit(self.start_accept)
            || !open_unit(self.cooling)
            || !open_unit(self.freeze_accept)
        {
            return Err("start_accept, cooling and freeze_accept must be between 0 and 1".into());
        }
        if self.grid_mm.is_some_and(|g| !(0.01..=10.0).contains(&g)) {
            return Err("grid must be between 0.01 and 10 mm".into());
        }
        if self.trace_len > 256 {
            return Err("trace_len is at most 256".into());
        }
        let nonneg = [
            self.edge_anchor_mm,
            self.edge_clearance_mm,
            self.clearance_mm,
            self.regularization,
            self.tolerance,
            self.overlap_epsilon_mm,
        ];
        if nonneg.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err("distances, regularization and tolerance may not be negative".into());
        }
        if !(self.step_mm.is_finite()
            && self.step_mm > 0.0
            && self.lattice_step_mm.is_finite()
            && self.lattice_step_mm > 0.0)
        {
            return Err("step sizes must be above 0".into());
        }
        Ok(())
    }

    /// Conjugate gradient step bound for `free` free parts.
    ///
    /// # Rules
    /// - `iterations` when set, else `iterations_per_cell * free`, at least 1,
    ///   at most `max_iterations`.
    pub fn cg_bound(&self, free: usize) -> usize {
        self.iterations
            .unwrap_or(self.iterations_per_cell.saturating_mul(free))
            .clamp(1, self.max_iterations)
    }

    /// Annealing move budget for `movable` movable parts.
    ///
    /// # Rules
    /// - `moves` when set, else `moves_per_part * movable` clamped to
    ///   `min_moves` and `max_moves`.
    pub fn moves_for(&self, movable: usize) -> usize {
        match self.moves {
            Some(n) => n.clamp(1, self.max_moves),
            None => {
                (self.moves_per_part.saturating_mul(movable)).clamp(self.min_moves, self.max_moves)
            }
        }
    }

    /// Moves in one cooling epoch for a budget of `moves`.
    ///
    /// # Rules
    /// - `ceil(moves / epochs)`, at least 1.
    pub fn epoch_len(&self, moves: usize) -> usize {
        moves.div_ceil(self.epochs.max(1)).max(1)
    }
}

/// One part's final place after a placement step.
///
/// # Rules
/// - Lists only parts whose position or turn differs from the start.
///
/// # Do not
/// - Include fixed cells.
#[derive(Clone, Debug, PartialEq)]
pub struct MovedCell {
    /// Footprint id (`Cell::id`).
    pub id: u64,
    /// New centre x.
    pub x: f64,
    /// New centre y.
    pub y: f64,
    /// Quarter turns now applied since the model was built.
    pub turns: u8,
}

/// What `place_global` reports: the move list and both wirelengths.
///
/// # Rules
/// - `hpwl_before` and `hpwl_after` are plain unweighted HPWL in millimetres.
/// - `unresolved_cells` and `fixed_overlaps` are indexes into `Model::cells`.
///
/// # Dependencies
/// - crates/engine/src/placement/mod.rs (`place_global` builds it)
///
/// # Do not
/// - Claim success while `unresolved_cells` is not empty.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementResult {
    /// Parts that ended somewhere else, in cell order.
    pub moved: Vec<MovedCell>,
    /// Wirelength before the run.
    pub hpwl_before: f64,
    /// Wirelength after the solve and the legaliser.
    pub hpwl_after: f64,
    /// Conjugate gradient steps used (the larger of the two axes).
    pub iterations: usize,
    /// The seed that was used.
    pub seed: u64,
    /// Free parts the legaliser found no spot for.
    pub unresolved_cells: Vec<usize>,
    /// Pairs of fixed parts that overlap each other.
    pub fixed_overlaps: Vec<(usize, usize)>,
    /// Nets the solver skipped for having too many pins.
    pub skipped_nets: usize,
}

/// One small change to the layout that the annealer may try.
///
/// # Rules
/// - Cells are indexes into `Model::cells`. A move never names a fixed cell.
/// - `Rotate` is one quarter turn, the sense of `turn_offset`; the command
///   maps it to a footprint rotation of minus 90 degrees.
///
/// # Dependencies
/// - crates/engine/src/placement/anneal.rs (`next_move`, `apply_move`)
///
/// # Do not
/// - Add a move kind without a golden vector and an undo.
#[derive(Clone, Debug, PartialEq)]
pub enum Move {
    /// Shift one cell by `dx`, `dy` millimetres.
    Translate {
        /// Index of the cell.
        cell: usize,
        /// Shift along x.
        dx: f64,
        /// Shift along y.
        dy: f64,
    },
    /// Turn one cell a quarter turn.
    Rotate {
        /// Index of the cell.
        cell: usize,
    },
    /// Exchange the centres of two cells of the same size.
    Swap {
        /// Index of the first cell.
        a: usize,
        /// Index of the second cell.
        b: usize,
    },
}

/// Why a move was refused by the hard rules.
///
/// # Rules
/// - Checked in this order: `Locked`, `Overlap`, `Keepout`, `Edge`.
///
/// # Dependencies
/// - crates/engine/src/placement/anneal.rs (`judge`)
///
/// # Do not
/// - Add a soft reason; a soft cost belongs in the cost, not the gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The move breaks no rule.
    Allowed,
    /// The move touches a fixed cell.
    Locked,
    /// The move adds a part to part overlap.
    Overlap,
    /// The move adds a keepout hit.
    Keepout,
    /// The move puts a box inside the edge clearance or off the board.
    Edge,
}

/// A move together with its verdict and its change in cost.
///
/// # Rules
/// - `delta_cost` is new cost minus old cost, only meaningful when the verdict
///   is `Allowed`; for any other verdict it is 0.
///
/// # Dependencies
/// - crates/engine/src/placement/anneal.rs (`evaluate` builds it)
///
/// # Do not
/// - Apply a candidate whose verdict is not `Allowed`.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// The move that was tried.
    pub mv: Move,
    /// Whether the hard rules allow it.
    pub verdict: Verdict,
    /// Cost change if applied.
    pub delta_cost: f64,
}

/// What happened to one annealing move.
///
/// # Rules
/// - Recorded for the first `trace_len` moves only.
///
/// # Do not
/// - Use the trace for logic; it is for tests and the preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// The move was applied.
    Accepted,
    /// A hard rule refused it.
    RejectedByRule,
    /// The cost test refused it.
    RejectedByCost,
}

/// Why an annealing run ended.
///
/// # Do not
/// - Add a time based reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The whole move budget was used.
    Budget,
    /// A window of `freeze_window` moves accepted fewer than `freeze_accept` of them.
    Frozen,
    /// There was no movable part.
    NothingToMove,
}

/// What `anneal` reports.
///
/// # Rules
/// - `cost_after <= cost_before` always holds: the best state is restored.
/// - `moves_used` counts only the main loop, not the probe.
///
/// # Dependencies
/// - crates/engine/src/placement/anneal.rs (`anneal` builds it)
///
/// # Do not
/// - Report the last state when a better one was seen.
#[derive(Clone, Debug, PartialEq)]
pub struct AnnealOutcome {
    /// Cost at the start.
    pub cost_before: f64,
    /// Cost of the restored best state.
    pub cost_after: f64,
    /// Plain HPWL at the start.
    pub hpwl_before: f64,
    /// Plain HPWL of the restored best state.
    pub hpwl_after: f64,
    /// Start temperature.
    pub start_temperature: f64,
    /// Moves tried in the main loop.
    pub moves_used: usize,
    /// Moves applied.
    pub accepted: usize,
    /// Moves refused by a hard rule.
    pub rejected_by_rule: usize,
    /// Moves refused by the cost test.
    pub rejected_by_cost: usize,
    /// Why the run ended.
    pub stop: Stop,
    /// The first `trace_len` decisions.
    pub trace: Vec<Decision>,
    /// Cells that ended somewhere else or turned, in cell order.
    pub moved: Vec<MovedCell>,
}

/// Run the whole global pass on a model: solve, then legalise (slice 1121-d).
///
/// Why: this is the one call the command makes. It records the wirelength,
/// solves with `quadratic::solve`, writes the free cell positions into the
/// model, runs `legalize::legalize`, records the wirelength again and lists
/// the cells that moved.
///
/// # Rules
/// - Validate the settings; refuse more than `max_cells` free cells and a model
///   with no free cell ("nothing to move").
/// - The model is left in its final state. The result is deterministic.
/// - `hpwl_after` may be above `hpwl_before` for a start that was already good;
///   the command layer compares and refuses to apply a worse result.
///
/// # Dependencies
/// - crates/engine/src/placement/quadratic.rs (`solve`)
/// - crates/engine/src/placement/legalize.rs (`legalize`)
///
/// # Do not
/// - Read the clock, or touch a fixed cell.
pub fn place_global(
    _model: &mut Model,
    _settings: &PlacementSettings,
) -> Result<PlacementResult, String> {
    Err(not_built("slice-1121-d"))
}

/// The command body of `layout.place_global` (slice 1121-d).
///
/// Why: the engine command needs one function that turns a project and a scope
/// into moves. It builds the model, runs `place_global`, and writes the moved
/// positions into the footprints (never a locked one) as one undo step; the
/// preview is `Session::dry_run`, so nothing special is needed for a dry run.
///
/// # Rules
/// - Refuse an empty scope, zero free parts and more than `max_cells`.
/// - Nothing is changed when any step fails.
/// - Also registers the three `lib.rs` hooks like `crates/engine/src/teardrop.rs`.
///
/// # Dependencies
/// - crates/engine/src/placement/model.rs (`build_model`)
/// - crates/engine/src/scope.rs (`Scope`)
///
/// # Do not
/// - Apply a result whose wirelength is worse than the start.
pub fn run_place_global(
    _project: &mut Project,
    _scope: &Scope,
    _settings: &PlacementSettings,
) -> Result<PlacementResult, String> {
    Err(not_built("slice-1121-d"))
}

/// The command body of `layout.polish` (slice 1122-e).
///
/// Why: the polish command builds the model, anneals it, checks the final
/// state once with the real rule set (`route::errors`, courtyard pairs) and
/// writes the moves into the footprints as one undo step.
///
/// # Rules
/// - Refuse an empty scope, no free part, and more than `max_polish_cells`.
/// - If the real rule set finds an error the start did not have, change
///   nothing and say so in a plain sentence.
///
/// # Dependencies
/// - crates/engine/src/placement/anneal.rs (`anneal`)
/// - crates/engine/src/route/mod.rs (`errors`)
///
/// # Do not
/// - Apply a state with a new error code or a new overlap.
pub fn run_polish(
    project: &mut Project,
    scope: &Scope,
    settings: &PlacementSettings,
) -> Result<AnnealOutcome, String> {
    if scope.layers.is_empty() && scope.nets.is_empty() && scope.components.is_empty() {
        return Err("scope is empty".into());
    }
    settings.validate()?;

    let mut model = build_model(project, scope, settings)?;
    if model.cells.iter().all(|c| c.fixed) {
        return Err("no movable parts in scope".into());
    }
    let outcome = anneal(&mut model, settings)?;

    let blocking = |p: &Project| -> std::collections::BTreeSet<(&'static str, Vec<u64>)> {
        pcbcraft_rules::drc(p)
            .iter()
            .filter(|f| f.severity == "error")
            .map(|f| {
                let mut o = f.objects.clone();
                o.sort_unstable();
                (f.code, o)
            })
            .collect()
    };
    let before = blocking(project);

    // Work on a copy so a refused result leaves the project exactly as it was.
    let mut trial = project.clone();
    for m in &outcome.moved {
        let footprint = trial
            .board
            .footprints
            .iter_mut()
            .find(|f| f.id == m.id)
            .ok_or_else(|| format!("footprint {} not found", m.id))?;
        if scope.respect_locked && footprint.locked == Some(true) {
            return Err(format!("footprint {} is locked", m.id));
        }
        footprint.position.x = m.x;
        footprint.position.y = m.y;
        footprint.rotation = (footprint.rotation - f64::from(m.turns) * 90.0).rem_euclid(360.0);
    }
    if !blocking(&trial).is_subset(&before) {
        return Err("polish introduced a new error".into());
    }
    *project = trial;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_are_valid_and_carry_the_reviewed_numbers() {
        let s = PlacementSettings::default();
        assert_eq!(s.validate(), Ok(()));
        assert_eq!((s.power_net_weight, s.signal_net_weight), (0.1, 1.0));
        assert_eq!((s.max_net_pins, s.max_cells, s.max_rings), (64, 2000, 400));
        assert_eq!((s.step_mm, s.cooling, s.start_accept), (0.5, 0.9, 0.8));
    }

    #[test]
    fn bounds_follow_the_stated_formulas() {
        let s = PlacementSettings::default();
        assert_eq!(s.cg_bound(2), 4);
        assert_eq!(s.cg_bound(0), 1);
        assert_eq!(s.cg_bound(5000), 4000);
        assert_eq!(s.moves_for(2), 500);
        assert_eq!(s.moves_for(40), 2000);
        assert_eq!(s.moves_for(10_000), 20_000);
        assert_eq!(s.epoch_len(500), 10);
        assert_eq!(s.epoch_len(1), 1);
        let asked = PlacementSettings {
            moves: Some(99_999),
            ..PlacementSettings::default()
        };
        assert_eq!(asked.moves_for(3), 20_000);
    }

    #[test]
    fn validate_refuses_each_out_of_range_value() {
        let d = PlacementSettings::default;
        let bad = [
            PlacementSettings {
                power_net_weight: 0.0,
                ..d()
            },
            PlacementSettings {
                grid_mm: Some(0.001),
                ..d()
            },
            PlacementSettings {
                cooling: 1.0,
                ..d()
            },
            PlacementSettings {
                translate_percent: 90,
                rotate_percent: 20,
                ..d()
            },
            PlacementSettings {
                iterations: Some(0),
                ..d()
            },
            PlacementSettings {
                moves: Some(0),
                ..d()
            },
            PlacementSettings {
                trace_len: 257,
                ..d()
            },
            PlacementSettings {
                edge_clearance_mm: -1.0,
                ..d()
            },
        ];
        for s in bad {
            assert!(s.validate().is_err(), "{s:?}");
        }
    }

    #[test]
    fn unbuilt_functions_name_their_slice() {
        assert_eq!(not_built("slice-1121-b"), "not built yet: slice-1121-b");
    }
}
