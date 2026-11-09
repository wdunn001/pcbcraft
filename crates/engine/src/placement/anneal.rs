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
use super::{
    AnnealOutcome, Candidate, Decision, Model, Move, MovedCell, PlacementSettings, Rng, Stop,
    Verdict,
};

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
    rng: &mut Rng,
    movable: &[usize],
    model: &Model,
    settings: &PlacementSettings,
) -> Result<Option<Move>, String> {
    if movable.is_empty() {
        return Ok(None);
    }

    let kind = rng.next_f64() * 100.0;
    let cell_idx = rng.below(movable.len());
    let cell = movable[cell_idx];

    if kind < settings.translate_percent as f64 {
        translate_move(rng, cell, settings)
    } else if kind < (settings.translate_percent + settings.rotate_percent) as f64 {
        Ok(Some(Move::Rotate { cell }))
    } else {
        let (hx, hy) = model.cells[cell].half();
        let mut candidates = Vec::new();
        for &c in movable {
            if c == cell {
                continue;
            }
            let (chx, chy) = model.cells[c].half();
            if (hx - chx).abs() < 1e-6 && (hy - chy).abs() < 1e-6 {
                candidates.push(c);
            }
        }
        if candidates.is_empty() {
            translate_move(rng, cell, settings)
        } else {
            let other = rng.below(candidates.len());
            Ok(Some(Move::Swap {
                a: cell,
                b: candidates[other],
            }))
        }
    }
}

fn translate_move(
    rng: &mut Rng,
    cell: usize,
    settings: &PlacementSettings,
) -> Result<Option<Move>, String> {
    let dir = rng.below(8);
    let mult = rng.below(3);
    let (dx, dy) = match dir {
        0 => (1.0, 0.0),
        1 => (1.0, 1.0),
        2 => (0.0, 1.0),
        3 => (-1.0, 1.0),
        4 => (-1.0, 0.0),
        5 => (-1.0, -1.0),
        6 => (0.0, -1.0),
        7 => (1.0, -1.0),
        _ => return Err("bad direction".into()),
    };
    let step = settings.step_mm * settings.step_multipliers[mult];
    Ok(Some(Move::Translate {
        cell,
        dx: dx * step,
        dy: dy * step,
    }))
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
pub fn apply_move(model: &mut Model, mv: &Move) -> Result<(), String> {
    match mv {
        Move::Translate { cell, dx, dy } => {
            let c = model
                .cells
                .get_mut(*cell)
                .ok_or_else(|| format!("cell {cell} out of range"))?;
            if c.fixed {
                return Err(format!("cell {cell} is fixed"));
            }
            c.x += *dx;
            c.y += *dy;
        }
        Move::Rotate { cell } => {
            let c = model
                .cells
                .get_mut(*cell)
                .ok_or_else(|| format!("cell {cell} out of range"))?;
            if c.fixed {
                return Err(format!("cell {cell} is fixed"));
            }
            c.turns = (c.turns + 1) % 4;
        }
        Move::Swap { a, b } => {
            let first = model
                .cells
                .get(*a)
                .ok_or_else(|| format!("cell {a} out of range"))?;
            if first.fixed {
                return Err(format!("cell {a} is fixed"));
            }
            let (ax, ay) = (first.x, first.y);
            let second = model
                .cells
                .get(*b)
                .ok_or_else(|| format!("cell {b} out of range"))?;
            if second.fixed {
                return Err(format!("cell {b} is fixed"));
            }
            let (bx, by) = (second.x, second.y);
            if let Some(ca) = model.cells.get_mut(*a) {
                ca.x = bx;
                ca.y = by;
            }
            if let Some(cb) = model.cells.get_mut(*b) {
                cb.x = ax;
                cb.y = ay;
            }
        }
    }
    Ok(())
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
pub fn undo_move(model: &mut Model, mv: &Move) -> Result<(), String> {
    match mv {
        Move::Translate { cell, dx, dy } => {
            let c = model
                .cells
                .get_mut(*cell)
                .ok_or_else(|| format!("cell {cell} out of range"))?;
            c.x -= *dx;
            c.y -= *dy;
        }
        Move::Rotate { cell } => {
            let c = model
                .cells
                .get_mut(*cell)
                .ok_or_else(|| format!("cell {cell} out of range"))?;
            c.turns = (c.turns + 3) % 4;
        }
        Move::Swap { a, b } => {
            let first = model
                .cells
                .get(*a)
                .ok_or_else(|| format!("cell {a} out of range"))?;
            let (ax, ay) = (first.x, first.y);
            let second = model
                .cells
                .get(*b)
                .ok_or_else(|| format!("cell {b} out of range"))?;
            let (bx, by) = (second.x, second.y);
            if let Some(ca) = model.cells.get_mut(*a) {
                ca.x = bx;
                ca.y = by;
            }
            if let Some(cb) = model.cells.get_mut(*b) {
                cb.x = ax;
                cb.y = ay;
            }
        }
    }
    Ok(())
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
    model: &Model,
    settings: &PlacementSettings,
    cell: usize,
) -> Result<(usize, usize, usize), String> {
    let c = model
        .cells
        .get(cell)
        .ok_or_else(|| format!("cell {cell} out of range"))?;
    let (hx, hy) = c.half();
    let box_a = (c.x, c.y, hx, hy);

    let mut overlaps = 0;
    for (i, other) in model.cells.iter().enumerate() {
        if i == cell {
            continue;
        }
        let (ohx, ohy) = other.half();
        let box_b = (other.x, other.y, ohx, ohy);
        if super::model::boxes_overlap(
            box_a,
            box_b,
            settings.clearance_mm,
            settings.overlap_epsilon_mm,
        ) {
            overlaps += 1;
        }
    }

    let mut keepouts = 0;
    for ko in &model.keepouts {
        let kx = (ko.x0 + ko.x1) / 2.0;
        let ky = (ko.y0 + ko.y1) / 2.0;
        let kxh = (ko.x1 - ko.x0) / 2.0;
        let kyh = (ko.y1 - ko.y0) / 2.0;
        let box_ko = (kx, ky, kxh, kyh);
        if super::model::boxes_overlap(
            box_a,
            box_ko,
            settings.clearance_mm,
            settings.overlap_epsilon_mm,
        ) {
            keepouts += 1;
        }
    }

    let e = settings.edge_clearance_mm;
    let eps = settings.overlap_epsilon_mm;
    let board = &model.board;
    let edge = if (c.x - hx) < (board.x0 + e) - eps
        || (c.x + hx) > (board.x1 - e) + eps
        || (c.y - hy) < (board.y0 + e) - eps
        || (c.y + hy) > (board.y1 - e) + eps
    {
        1
    } else {
        0
    };

    Ok((overlaps, keepouts, edge))
}

/// Decide whether a move may be taken, without keeping it.
///
/// # Rules
/// - A move that touches a fixed cell is `Locked`.
/// - The move is applied, the touched cells are checked, then it is undone.
/// - A move that adds an overlap, a keepout hit or an edge hit is refused with that name.
/// - Overlap is checked first, then keepout, then edge.
/// - The model is the same after the call as before it.
///
/// # Do not
/// - Keep the move here; the caller applies it when the verdict is `Allowed`.
pub fn judge(
    model: &mut Model,
    settings: &PlacementSettings,
    mv: &Move,
) -> Result<Verdict, String> {
    let touched = match mv {
        Move::Translate { cell, .. } => vec![*cell],
        Move::Rotate { cell } => vec![*cell],
        Move::Swap { a, b } => vec![*a, *b],
    };
    for &i in &touched {
        if let Some(c) = model.cells.get(i)
            && c.fixed
        {
            return Ok(Verdict::Locked);
        }
    }

    let mut before = (0usize, 0usize, 0usize);
    for &i in &touched {
        let v = violations(model, settings, i)?;
        before.0 += v.0;
        before.1 += v.1;
        before.2 += v.2;
    }

    apply_move(model, mv)?;

    let mut after = (0usize, 0usize, 0usize);
    for &i in &touched {
        let v = violations(model, settings, i)?;
        after.0 += v.0;
        after.1 += v.1;
        after.2 += v.2;
    }

    undo_move(model, mv)?;

    if after.0 > before.0 {
        Ok(Verdict::Overlap)
    } else if after.1 > before.1 {
        Ok(Verdict::Keepout)
    } else if after.2 > before.2 {
        Ok(Verdict::Edge)
    } else {
        Ok(Verdict::Allowed)
    }
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
pub fn cost(model: &Model, settings: &PlacementSettings) -> Result<f64, String> {
    let mut overlap = 0.0;
    for (i, a) in model.cells.iter().enumerate() {
        let (ahx, ahy) = a.half();
        let box_a = (a.x, a.y, ahx, ahy);
        for b in model.cells.iter().skip(i + 1) {
            let (bhx, bhy) = b.half();
            overlap += overlap_area(box_a, (b.x, b.y, bhx, bhy), settings.clearance_mm);
        }
        for ko in &model.keepouts {
            let box_ko = (
                (ko.x0 + ko.x1) / 2.0,
                (ko.y0 + ko.y1) / 2.0,
                (ko.x1 - ko.x0) / 2.0,
                (ko.y1 - ko.y0) / 2.0,
            );
            overlap += overlap_area(box_a, box_ko, settings.clearance_mm);
        }
    }
    Ok(model.hpwl_of() + settings.overlap_weight * overlap)
}

/// Overlap area of two boxes given as centre and half size, with clearance added.
fn overlap_area(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64), clearance: f64) -> f64 {
    let ox = a.2 + b.2 + clearance - (a.0 - b.0).abs();
    let oy = a.3 + b.3 + clearance - (a.1 - b.1).abs();
    if ox > 0.0 && oy > 0.0 {
        ox.min(2.0 * a.2.min(b.2)) * oy.min(2.0 * a.3.min(b.3))
    } else {
        0.0
    }
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
    model: &mut Model,
    movable: &[usize],
    settings: &PlacementSettings,
) -> Result<f64, String> {
    let mut rng = Rng::new(settings.seed ^ PROBE_SALT);
    let base_cost = cost(model, settings)?;
    let mut sum_uphill = 0.0;
    let mut count_uphill = 0usize;

    for _ in 0..settings.probe_moves {
        let mv_opt = next_move(&mut rng, movable, model, settings)?;
        let mv = match mv_opt {
            Some(m) => m,
            None => continue,
        };
        let verdict = judge(model, settings, &mv)?;
        if verdict != Verdict::Allowed {
            continue;
        }
        apply_move(model, &mv)?;
        let new_cost = cost(model, settings)?;
        undo_move(model, &mv)?;
        let delta = new_cost - base_cost;
        if delta > 0.0 {
            sum_uphill += delta;
            count_uphill += 1;
        }
    }

    if count_uphill == 0 {
        return Ok(settings.fallback_temperature);
    }

    let mean_uphill = sum_uphill / count_uphill as f64;
    Ok(-mean_uphill / settings.start_accept.ln())
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
    model: &mut Model,
    settings: &PlacementSettings,
    mv: Move,
    cost_now: f64,
) -> Result<Candidate, String> {
    let verdict = judge(model, settings, &mv)?;
    let delta_cost = if verdict == Verdict::Allowed {
        apply_move(model, &mv)?;
        let cost_after = cost(model, settings)?;
        undo_move(model, &mv)?;
        cost_after - cost_now
    } else {
        0.0
    };
    Ok(Candidate {
        mv,
        verdict,
        delta_cost,
    })
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
pub fn anneal(model: &mut Model, settings: &PlacementSettings) -> Result<AnnealOutcome, String> {
    let movable: Vec<usize> = (0..model.cells.len())
        .filter(|&i| !model.cells[i].fixed)
        .collect();

    if movable.is_empty() {
        return Ok(AnnealOutcome {
            cost_before: 0.0,
            cost_after: 0.0,
            hpwl_before: 0.0,
            hpwl_after: 0.0,
            start_temperature: 0.0,
            moves_used: 0,
            accepted: 0,
            rejected_by_rule: 0,
            rejected_by_cost: 0,
            stop: Stop::NothingToMove,
            trace: vec![],
            moved: vec![],
        });
    }

    if movable.len() > settings.max_polish_cells {
        return Err(format!(
            "too many movable parts: {} exceeds {}",
            movable.len(),
            settings.max_polish_cells
        ));
    }

    let start = model.clone();
    let cost_before = cost(model, settings)?;
    let hpwl_before = model.hpwl_of();
    let mut rng = Rng::new(settings.seed);

    let t0 = start_temperature(model, &movable, settings)?;
    let mut temperature = t0;

    let budget = settings.moves_for(movable.len());
    let epoch = settings.epoch_len(budget);

    let mut moves_used = 0;
    let mut accepted = 0;
    let mut rejected_by_rule = 0;
    let mut rejected_by_cost = 0;
    let mut cost_now = cost_before;

    let mut best_model = start.clone();
    let mut best_cost = cost_before;

    let mut window_accepted = 0;
    let mut window_total = 0;
    let mut freeze = false;

    let mut trace = vec![];
    let trace_cap = settings.trace_len;

    loop {
        if freeze {
            break;
        }

        let mv_opt = next_move(&mut rng, &movable, model, settings)?;
        let mv = match mv_opt {
            Some(m) => m,
            None => break,
        };

        let cand = evaluate(model, settings, mv, cost_now)?;

        if cand.verdict != Verdict::Allowed {
            rejected_by_rule += 1;
            if window_total >= settings.freeze_window {
                if (window_accepted as f64)
                    < (settings.freeze_accept * settings.freeze_window as f64)
                {
                    freeze = true;
                }
                window_accepted = 0;
                window_total = 0;
            }
            window_total += 1;
            if trace_cap > 0 && trace.len() < trace_cap {
                trace.push(Decision::RejectedByRule);
            }
        } else if cand.delta_cost <= 0.0 {
            apply_move(model, &cand.mv)?;
            cost_now += cand.delta_cost;
            accepted += 1;
            window_accepted += 1;
            if cost_now < best_cost {
                best_cost = cost_now;
                best_model = model.clone();
            }
            if window_total >= settings.freeze_window {
                if (window_accepted as f64)
                    < (settings.freeze_accept * settings.freeze_window as f64)
                {
                    freeze = true;
                }
                window_accepted = 0;
                window_total = 0;
            }
            window_total += 1;
            if trace_cap > 0 && trace.len() < trace_cap {
                trace.push(Decision::Accepted);
            }
        } else {
            let threshold = (-cand.delta_cost / temperature).exp();
            let draw = rng.next_f64();
            if draw < threshold {
                apply_move(model, &cand.mv)?;
                cost_now += cand.delta_cost;
                accepted += 1;
                window_accepted += 1;
                if cost_now < best_cost {
                    best_cost = cost_now;
                    best_model = model.clone();
                }
                if window_total >= settings.freeze_window {
                    if (window_accepted as f64)
                        < (settings.freeze_accept * settings.freeze_window as f64)
                    {
                        freeze = true;
                    }
                    window_accepted = 0;
                    window_total = 0;
                }
                window_total += 1;
                if trace_cap > 0 && trace.len() < trace_cap {
                    trace.push(Decision::Accepted);
                }
            } else {
                rejected_by_cost += 1;
                if window_total >= settings.freeze_window {
                    if (window_accepted as f64)
                        < (settings.freeze_accept * settings.freeze_window as f64)
                    {
                        freeze = true;
                    }
                    window_accepted = 0;
                    window_total = 0;
                }
                window_total += 1;
                if trace_cap > 0 && trace.len() < trace_cap {
                    trace.push(Decision::RejectedByCost);
                }
            }
        }

        moves_used += 1;

        if moves_used >= budget {
            break;
        }

        if moves_used > 0 && moves_used % epoch == 0 {
            temperature *= settings.cooling;
        }
    }

    let hpwl_after_best = best_model.hpwl_of();
    let cost_after_best = cost(&best_model, settings)?;

    *model = best_model;

    let mut moved = vec![];
    for (i, c) in model.cells.iter().enumerate() {
        let s = &start.cells[i];
        if c.x != s.x || c.y != s.y || c.turns != s.turns {
            moved.push(MovedCell {
                id: c.id,
                x: c.x,
                y: c.y,
                turns: c.turns,
            });
        }
    }

    Ok(AnnealOutcome {
        cost_before,
        cost_after: cost_after_best,
        hpwl_before,
        hpwl_after: hpwl_after_best,
        start_temperature: t0,
        moves_used,
        accepted,
        rejected_by_rule,
        rejected_by_cost,
        stop: if freeze { Stop::Frozen } else { Stop::Budget },
        trace,
        moved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placement::{Cell, NetSpring, Pin, Rect};

    fn cell(id: u64, x: f64, fixed: bool) -> Cell {
        Cell {
            id,
            x,
            y: 0.0,
            half_x: 1.0,
            half_y: 1.0,
            turns: 0,
            fixed,
        }
    }

    fn pair(second_x: f64, fixed: bool) -> Model {
        let pin = |cell: usize| Pin {
            cell,
            dx: 0.0,
            dy: 0.0,
        };
        Model {
            cells: vec![cell(1, 10.0, fixed), cell(2, second_x, fixed)],
            nets: vec![NetSpring {
                net: 1,
                weight: 1.0,
                pins: vec![pin(0), pin(1)],
            }],
            board: Rect {
                x0: -50.0,
                y0: -50.0,
                x1: 80.0,
                y1: 50.0,
            },
            keepouts: vec![],
        }
    }

    fn near(a: f64, b: f64) -> Result<(), String> {
        if (a - b).abs() < 1e-9 {
            Ok(())
        } else {
            Err(format!("{a} is not {b}"))
        }
    }

    #[test]
    fn cost_is_wire_plus_weighted_overlap_area() -> Result<(), String> {
        // Boxes 2 by 2 whose centres are 1 apart overlap 1 by 2 = 2 mm squared.
        let model = pair(11.0, false);
        let s = PlacementSettings::default();
        near(cost(&model, &s)?, 1.0 + s.overlap_weight * 2.0)
    }

    #[test]
    fn a_layout_without_overlap_has_no_penalty() -> Result<(), String> {
        let model = pair(15.0, false);
        let s = PlacementSettings::default();
        near(cost(&model, &s)?, model.hpwl_of())
    }

    #[test]
    fn the_start_temperature_falls_back_without_an_uphill_move() -> Result<(), String> {
        let mut model = pair(15.0, true);
        let s = PlacementSettings::default();
        let t = start_temperature(&mut model, &[], &s)?;
        near(t, s.fallback_temperature)
    }

    #[test]
    fn the_start_temperature_is_positive_and_repeatable() -> Result<(), String> {
        let s = PlacementSettings::default();
        let mut model = pair(15.0, false);
        let before = model.clone();
        let first = start_temperature(&mut model, &[0, 1], &s)?;
        let second = start_temperature(&mut model, &[0, 1], &s)?;
        if first <= 0.0 {
            return Err(format!("temperature {first} is not positive"));
        }
        if model != before {
            return Err("the probe changed the model".into());
        }
        near(first, second)
    }

    #[test]
    fn evaluate_reports_the_cost_change_and_leaves_the_model() -> Result<(), String> {
        let mut model = pair(11.0, false);
        let s = PlacementSettings::default();
        let now = cost(&model, &s)?;
        let before = model.clone();
        let mv = Move::Translate {
            cell: 1,
            dx: 4.0,
            dy: 0.0,
        };
        let c = evaluate(&mut model, &s, mv, now)?;
        if model != before {
            return Err("evaluate changed the model".into());
        }
        near(c.delta_cost, 5.0 - now)
    }

    fn pin(cell: usize) -> Pin {
        Pin {
            cell,
            dx: 0.0,
            dy: 0.0,
        }
    }

    fn square(id: u64, x: f64, y: f64, fixed: bool) -> Cell {
        Cell {
            id,
            x,
            y,
            half_x: 1.0,
            half_y: 1.0,
            turns: 0,
            fixed,
        }
    }

    fn net(a: usize, b: usize) -> NetSpring {
        NetSpring {
            net: 1,
            weight: 1.0,
            pins: vec![pin(a), pin(b)],
        }
    }

    fn board(half: f64) -> Rect {
        Rect {
            x0: -half,
            y0: -half,
            x1: half,
            y1: half,
        }
    }

    /// Sum of every hard rule violation of the movable cells, by re-running the gate check.
    fn movable_violations(model: &Model, s: &PlacementSettings) -> Result<usize, String> {
        let mut total = 0;
        for (i, c) in model.cells.iter().enumerate() {
            if c.fixed {
                continue;
            }
            let (o, k, e) = violations(model, s, i)?;
            total += o + k + e;
        }
        Ok(total)
    }

    /// Four movable parts on a loose grid, joined in a ring, with one fixed anchor.
    fn ring_board() -> Model {
        Model {
            cells: vec![
                square(1, -10.0, -10.0, false),
                square(2, 10.0, -10.0, false),
                square(3, 10.0, 10.0, false),
                square(4, -10.0, 10.0, false),
                square(5, 0.0, 0.0, true),
            ],
            nets: vec![net(0, 1), net(1, 2), net(2, 3), net(3, 0), net(0, 4)],
            board: board(30.0),
            keepouts: vec![],
        }
    }

    fn with_moves(seed: u64, moves: usize) -> PlacementSettings {
        PlacementSettings {
            seed,
            moves: Some(moves),
            ..PlacementSettings::default()
        }
    }

    #[test]
    fn polish_never_accepts_a_clearance_worsening_move() -> Result<(), String> {
        // The movable part touches a fixed one and its net pulls it into that
        // part, so every pull move would overlap and must be refused.
        let mut model = Model {
            cells: vec![square(1, 2.0, 0.0, false), square(2, 0.0, 0.0, true)],
            nets: vec![net(0, 1)],
            board: board(50.0),
            keepouts: vec![],
        };
        let s = with_moves(3, 400);
        let start = model.clone();
        if movable_violations(&start, &s)? != 0 {
            return Err("the fixture must start clean".into());
        }
        let out = anneal(&mut model, &s)?;
        if out.rejected_by_rule == 0 {
            return Err("no move was refused by a hard rule".into());
        }
        if movable_violations(&model, &s)? != 0 {
            return Err("the final model breaks a hard rule".into());
        }
        if model != start {
            return Err("the part did not end where it started".into());
        }
        Ok(())
    }

    #[test]
    fn polish_never_enters_a_keepout() -> Result<(), String> {
        let mut model = Model {
            cells: vec![square(1, 4.0, 0.0, false), square(2, -20.0, 0.0, true)],
            nets: vec![net(0, 1)],
            board: board(50.0),
            keepouts: vec![Rect {
                x0: -2.0,
                y0: -5.0,
                x1: 1.0,
                y1: 5.0,
            }],
        };
        let s = with_moves(11, 600);
        let out = anneal(&mut model, &s)?;
        if out.rejected_by_rule == 0 {
            return Err("no move was refused by the keepout".into());
        }
        if movable_violations(&model, &s)? != 0 {
            return Err("the final model touches the keepout".into());
        }
        Ok(())
    }

    #[test]
    fn polish_never_leaves_the_board_edge_clearance() -> Result<(), String> {
        // The fixed partner lies outside the board, so the net pulls the part
        // toward the edge and the edge rule must stop it.
        let mut model = Model {
            cells: vec![square(1, 47.5, 0.0, false), square(2, 60.0, 0.0, true)],
            nets: vec![net(0, 1)],
            board: board(50.0),
            keepouts: vec![],
        };
        let s = with_moves(5, 600);
        let out = anneal(&mut model, &s)?;
        if out.rejected_by_rule == 0 {
            return Err("no move was refused by the edge rule".into());
        }
        if movable_violations(&model, &s)? != 0 {
            return Err("the final model is past the edge clearance".into());
        }
        Ok(())
    }

    #[test]
    fn polish_never_makes_a_start_overlap_worse() -> Result<(), String> {
        let mut model = Model {
            cells: vec![
                square(1, 0.0, 0.0, false),
                square(2, 1.0, 0.0, false),
                square(3, 30.0, 0.0, true),
            ],
            nets: vec![net(0, 2), net(1, 2)],
            board: board(50.0),
            keepouts: vec![],
        };
        let s = with_moves(9, 800);
        let before = movable_violations(&model, &s)?;
        anneal(&mut model, &s)?;
        let after = movable_violations(&model, &s)?;
        if after > before {
            return Err(format!("violations rose from {before} to {after}"));
        }
        Ok(())
    }

    #[test]
    fn polish_is_deterministic_for_one_seed() -> Result<(), String> {
        let mut s = with_moves(42, 500);
        s.trace_len = 64;
        let mut first = ring_board();
        let mut second = ring_board();
        let a = anneal(&mut first, &s)?;
        let b = anneal(&mut second, &s)?;
        if a != b {
            return Err("two runs with one seed gave different outcomes".into());
        }
        if first != second {
            return Err("two runs with one seed gave different positions".into());
        }
        Ok(())
    }

    #[test]
    fn polish_differs_for_another_seed_or_is_still_valid() -> Result<(), String> {
        let mut one = with_moves(1, 500);
        one.trace_len = 64;
        let mut two = with_moves(2, 500);
        two.trace_len = 64;
        let mut first = ring_board();
        let mut second = ring_board();
        let a = anneal(&mut first, &one)?;
        let b = anneal(&mut second, &two)?;
        // Different seeds draw different moves, so the traces differ; either
        // way both results must obey every rule.
        if a.trace == b.trace && first != second {
            return Err("equal traces gave different positions".into());
        }
        if movable_violations(&first, &one)? != 0 || movable_violations(&second, &two)? != 0 {
            return Err("a seeded run broke a hard rule".into());
        }
        Ok(())
    }

    #[test]
    fn polish_never_ends_worse_than_it_started() -> Result<(), String> {
        for seed in 0..8 {
            let mut model = ring_board();
            let s = with_moves(seed, 500);
            let out = anneal(&mut model, &s)?;
            if out.cost_after > out.cost_before + 1e-9 {
                return Err(format!(
                    "seed {seed}: cost rose from {} to {}",
                    out.cost_before, out.cost_after
                ));
            }
            // The returned model is the best state, so its own cost is the reported one.
            near(cost(&model, &s)?, out.cost_after)?;
            if out.hpwl_after > out.hpwl_before + 1e-9 && out.cost_after >= out.cost_before {
                return Err(format!("seed {seed}: wire rose with no cost gain"));
            }
        }
        Ok(())
    }

    #[test]
    fn polish_shortens_wire_on_a_loose_fixture() -> Result<(), String> {
        let mut model = ring_board();
        let s = with_moves(7, 2000);
        let out = anneal(&mut model, &s)?;
        if out.hpwl_after >= out.hpwl_before {
            return Err(format!(
                "wire did not shrink: {} to {}",
                out.hpwl_before, out.hpwl_after
            ));
        }
        near(model.hpwl_of(), out.hpwl_after)
    }

    #[test]
    fn polish_stays_within_its_move_budget() -> Result<(), String> {
        let mut model = ring_board();
        let s = with_moves(4, 137);
        let out = anneal(&mut model, &s)?;
        if out.moves_used > s.moves_for(4) {
            return Err(format!("{} moves passed the budget", out.moves_used));
        }
        let auto = PlacementSettings::default();
        let mut again = ring_board();
        let out = anneal(&mut again, &auto)?;
        if out.moves_used > auto.moves_for(4) {
            return Err(format!(
                "{} moves passed the automatic budget",
                out.moves_used
            ));
        }
        let counted = out.accepted + out.rejected_by_rule + out.rejected_by_cost;
        if counted != out.moves_used {
            return Err(format!("{counted} decisions for {} moves", out.moves_used));
        }
        Ok(())
    }

    #[test]
    fn polish_with_nothing_to_move_stops_at_once() -> Result<(), String> {
        let mut model = pair(15.0, true);
        let before = model.clone();
        let out = anneal(&mut model, &PlacementSettings::default())?;
        if out.stop != Stop::NothingToMove {
            return Err(format!("stopped with {:?}", out.stop));
        }
        if out.moves_used != 0 || !out.moved.is_empty() || model != before {
            return Err("a run with nothing to move changed something".into());
        }
        Ok(())
    }

    #[test]
    fn polish_refuses_too_many_parts() -> Result<(), String> {
        let mut model = ring_board();
        let s = PlacementSettings {
            max_polish_cells: 3,
            ..PlacementSettings::default()
        };
        let before = model.clone();
        match anneal(&mut model, &s) {
            Ok(_) => Err("four parts passed a cap of three".into()),
            Err(text) => {
                if text != "too many movable parts: 4 exceeds 3" {
                    return Err(format!("unexpected error text: {text}"));
                }
                if model != before {
                    return Err("a refused run changed the model".into());
                }
                Ok(())
            }
        }
    }

    #[test]
    fn a_newcomer_polishes_a_layout_without_breaking_any_rule() -> Result<(), String> {
        let mut model = ring_board();
        let s = with_moves(21, 1500);
        let start = model.clone();
        let out = anneal(&mut model, &s)?;
        if movable_violations(&model, &s)? != 0 {
            return Err("the polished layout breaks a rule".into());
        }
        if out.cost_after > out.cost_before {
            return Err("the polish made the layout worse".into());
        }
        for m in &out.moved {
            let found = model
                .cells
                .iter()
                .any(|c| c.id == m.id && c.x == m.x && c.y == m.y);
            if !found {
                return Err(format!("moved part {} is not where it is reported", m.id));
            }
        }
        if model.cells[4] != start.cells[4] {
            return Err("the fixed part moved".into());
        }
        Ok(())
    }
}
