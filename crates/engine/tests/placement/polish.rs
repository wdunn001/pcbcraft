//! Tests for the annealing polish: slices 1122-a to 1122-e.
use super::fixture::{Fixture, near};
use pcbcraft_engine::placement::anneal::{
    anneal, apply_move, cost, evaluate, judge, next_move, start_temperature, undo_move, violations,
};
use pcbcraft_engine::placement::{
    Decision, Move, PlacementSettings, Rng, Stop, Verdict, run_polish,
};
use pcbcraft_engine::scope::Scope;
use pcbcraft_engine::{Command, Error, Session};
use serde_json::json;
use std::collections::BTreeSet;

fn movable(fx: &Fixture) -> Vec<usize> {
    (0..fx.model.cells.len())
        .filter(|&i| !fx.model.cells[i].fixed)
        .collect()
}

// ---------------------------------------------------------------- slice 1122-a

#[test]
#[ignore = "built in slice-1122-a"]
fn the_first_ten_moves_for_seed_zero_are_pinned() -> Result<(), String> {
    let fx = Fixture::load("polish")?;
    let (mov, s) = (movable(&fx), PlacementSettings::default());
    assert_eq!(mov, vec![2, 3, 4, 5], "the four resistors are movable");
    let mut rng = Rng::new(0);
    let want = [
        Move::Swap { a: 2, b: 4 },
        Move::Swap { a: 5, b: 2 },
        Move::Translate {
            cell: 2,
            dx: -2.0,
            dy: 2.0,
        },
        Move::Translate {
            cell: 4,
            dx: -0.5,
            dy: 0.5,
        },
        Move::Rotate { cell: 5 },
        Move::Translate {
            cell: 4,
            dx: -1.0,
            dy: 0.0,
        },
        Move::Swap { a: 3, b: 2 },
        Move::Translate {
            cell: 3,
            dx: -2.0,
            dy: 2.0,
        },
        Move::Translate {
            cell: 4,
            dx: 0.0,
            dy: 1.0,
        },
        Move::Translate {
            cell: 2,
            dx: 1.0,
            dy: 1.0,
        },
    ];
    for (i, w) in want.iter().enumerate() {
        assert_eq!(
            next_move(&mut rng, &mov, &fx.model, &s)?.as_ref(),
            Some(w),
            "move {i}"
        );
    }
    Ok(())
}

#[test]
#[ignore = "built in slice-1122-a"]
fn moves_never_name_a_fixed_cell_swaps_pair_equal_sizes_and_an_empty_list_gives_none()
-> Result<(), String> {
    let mut fx = Fixture::load("polish")?;
    fx.model.cells[3].half_x = 2.0; // R2 is now a different size: no swap may involve it
    let (mov, s) = (movable(&fx), PlacementSettings::default());
    let mut rng = Rng::new(5);
    for _ in 0..500 {
        let m = next_move(&mut rng, &mov, &fx.model, &s)?.ok_or("a move")?;
        let cells = match m {
            Move::Translate { cell, .. } | Move::Rotate { cell } => vec![cell],
            Move::Swap { a, b } => {
                assert!(
                    a != 3 && b != 3 && a != b,
                    "swap pairs two different equal-size parts"
                );
                vec![a, b]
            }
        };
        assert!(
            cells.iter().all(|c| mov.contains(c)),
            "only movable cells appear"
        );
    }
    assert_eq!(next_move(&mut rng, &[], &fx.model, &s)?, None);
    Ok(())
}

#[test]
#[ignore = "built in slice-1122-a"]
fn apply_then_undo_restores_the_model_and_a_fixed_cell_is_refused() -> Result<(), String> {
    let mut fx = Fixture::load("polish")?;
    let (mov, s) = (movable(&fx), PlacementSettings::default());
    let start = fx.model.clone();
    let mut rng = Rng::new(9);
    for _ in 0..200 {
        let m = next_move(&mut rng, &mov, &fx.model, &s)?.ok_or("a move")?;
        apply_move(&mut fx.model, &m)?;
        undo_move(&mut fx.model, &m)?;
        assert_eq!(fx.model, start, "undo restores the model exactly: {m:?}");
    }
    assert!(
        apply_move(&mut fx.model, &Move::Rotate { cell: 0 }).is_err(),
        "cell 0 (J) is fixed"
    );
    apply_move(&mut fx.model, &Move::Rotate { cell: 2 })?;
    assert_eq!(
        (fx.model.cells[2].turns, fx.model.cells[2].half()),
        (1, (0.5, 1.0))
    );
    Ok(())
}

// ---------------------------------------------------------------- slice 1122-b

#[test]
fn the_gate_names_the_rule_a_move_would_break() -> Result<(), String> {
    let mut fx = Fixture::load("gate")?;
    let s = PlacementSettings::default();
    let (r1, r3) = (fx.index("R1")?, fx.index("R3")?);
    let start = fx.model.clone();
    let cases = [
        (
            Move::Translate {
                cell: r1,
                dx: 1.5,
                dy: 0.0,
            },
            Verdict::Overlap,
            "R1 into R2",
        ),
        (
            Move::Translate {
                cell: r3,
                dx: 0.0,
                dy: 1.0,
            },
            Verdict::Keepout,
            "R3 into the keepout",
        ),
        (
            Move::Translate {
                cell: r3,
                dx: 0.0,
                dy: -9.5,
            },
            Verdict::Edge,
            "R3 inside the edge clearance",
        ),
        (
            Move::Translate {
                cell: r1,
                dx: 0.0,
                dy: 0.5,
            },
            Verdict::Allowed,
            "R1 up by half a millimetre",
        ),
        (
            Move::Swap { a: r1, b: r3 },
            Verdict::Allowed,
            "equal parts swap into free space",
        ),
        (
            Move::Translate {
                cell: r3,
                dx: 0.0,
                dy: -9.0,
            },
            Verdict::Allowed,
            "R3 down to the clearance line",
        ),
    ];
    for (mv, want, why) in cases {
        assert_eq!(judge(&mut fx.model, &s, &mv)?, want, "{why}");
        assert_eq!(fx.model, start, "judging leaves the model unchanged");
    }
    assert_eq!(
        judge(&mut fx.model, &s, &Move::Rotate { cell: 0 })?,
        Verdict::Locked,
        "J is fixed"
    );
    Ok(())
}

#[test]
fn violations_count_overlaps_keepout_hits_and_the_edge_per_cell() -> Result<(), String> {
    let mut fx = Fixture::load("gate")?;
    let s = PlacementSettings::default();
    let r1 = fx.index("R1")?;
    assert_eq!(
        violations(&fx.model, &s, r1)?,
        (0, 0, 0),
        "the start is clean"
    );
    fx.model.cells[r1].x = 13.0; // on top of R2
    assert_eq!(violations(&fx.model, &s, r1)?, (1, 0, 0));
    fx.model.cells[r1].x = 20.0;
    fx.model.cells[r1].y = 15.0; // inside the keepout
    assert_eq!(violations(&fx.model, &s, r1)?, (0, 1, 0));
    fx.model.cells[r1].x = 0.5;
    fx.model.cells[r1].y = 15.0; // past the left edge
    assert_eq!(violations(&fx.model, &s, r1)?.2, 1);
    Ok(())
}

// ---------------------------------------------------------------- slice 1122-c

#[test]
fn cost_is_wire_plus_one_hundred_per_square_millimetre_of_overlap() -> Result<(), String> {
    let mut fx = Fixture::load("cost")?;
    let s = PlacementSettings::default();
    near("cost", cost(&fx.model, &s)?, fx.nums("cost")?[0], 1e-9)?;
    let b = fx.index("B")?;
    let c = evaluate(
        &mut fx.model,
        &s,
        Move::Translate {
            cell: b,
            dx: 1.0,
            dy: 0.0,
        },
        111.0,
    )?;
    assert_eq!(
        c.verdict,
        Verdict::Allowed,
        "removing an overlap is allowed"
    );
    near("delta", c.delta_cost, fx.nums("delta_fix")?[0], 1e-9)?;
    near("model unchanged", cost(&fx.model, &s)?, 111.0, 1e-9)?;
    let locked = evaluate(&mut fx.model, &s, Move::Rotate { cell: 0 }, 111.0)?;
    assert_eq!((locked.verdict, locked.delta_cost), (Verdict::Locked, 0.0));
    Ok(())
}

#[test]
fn the_start_temperature_makes_the_mean_uphill_move_eighty_percent_likely() -> Result<(), String> {
    let mut fx = Fixture::load("polish")?;
    let (mov, s) = (movable(&fx), PlacementSettings::default());
    let start = fx.model.clone();
    let t0 = start_temperature(&mut fx.model, &mov, &s)?;
    near("t0", t0, fx.nums("t0")?[0], 1e-9)?;
    assert_eq!(fx.model, start, "the probe leaves the model unchanged");
    let mut none = Fixture::load("no_nets")?;
    let mov = movable(&none);
    near(
        "fallback",
        start_temperature(&mut none.model, &mov, &s)?,
        none.nums("t0")?[0],
        1e-12,
    )
}

// ---------------------------------------------------------------- slice 1122-d

fn run_polish_model(
    seed: u64,
    moves: usize,
    trace: usize,
) -> Result<(Fixture, pcbcraft_engine::placement::AnnealOutcome), String> {
    let mut fx = Fixture::load("polish")?;
    let s = PlacementSettings {
        seed,
        moves: Some(moves),
        trace_len: trace,
        ..PlacementSettings::default()
    };
    let out = anneal(&mut fx.model, &s)?;
    Ok((fx, out))
}

#[test]
#[ignore = "built in slice-1122-d"]
fn seed_zero_with_five_hundred_moves_matches_the_pinned_run() -> Result<(), String> {
    let (fx, out) = run_polish_model(0, 500, 20)?;
    let letters: String = out
        .trace
        .iter()
        .map(|d| match d {
            Decision::Accepted => 'A',
            Decision::RejectedByRule => 'R',
            Decision::RejectedByCost => 'C',
        })
        .collect();
    assert_eq!(letters, fx.words("trace")?[0], "the first twenty decisions");
    let w = fx.words("run_500")?;
    near(
        "best cost",
        out.cost_after,
        w[0].parse().map_err(|_| "num")?,
        1e-9,
    )?;
    near("hpwl after", out.hpwl_after, out.cost_after, 1e-9)?;
    near(
        "start cost",
        out.cost_before,
        fx.nums("start_cost")?[0],
        1e-9,
    )?;
    near("t0", out.start_temperature, fx.nums("t0")?[0], 1e-9)?;
    assert_eq!(
        (
            out.accepted,
            out.rejected_by_rule,
            out.rejected_by_cost,
            out.moves_used
        ),
        (
            w[1].parse().map_err(|_| "n")?,
            w[2].parse().map_err(|_| "n")?,
            w[3].parse().map_err(|_| "n")?,
            w[4].parse().map_err(|_| "n")?
        )
    );
    assert_eq!(out.stop, Stop::Budget);
    for name in ["R1", "R2", "R3", "R4"] {
        let want = fx.nums(name)?;
        let c = &fx.model.cells[fx.index(name)?];
        near(&format!("{name}.x"), c.x, want[0], 1e-9)?;
        near(&format!("{name}.y"), c.y, want[1], 1e-9)?;
        assert_eq!(f64::from(c.turns), want[2], "{name} turns");
    }
    Ok(())
}

#[test]
#[ignore = "built in slice-1122-d"]
fn a_long_run_freezes_early_and_ends_no_worse_than_it_started() -> Result<(), String> {
    let (fx, out) = run_polish_model(0, 3000, 0)?;
    let w = fx.words("run_3000")?;
    near("best", out.cost_after, w[0].parse().map_err(|_| "n")?, 1e-9)?;
    assert_eq!(
        (out.moves_used, out.stop),
        (w[1].parse().map_err(|_| "n")?, Stop::Frozen)
    );
    assert!(out.cost_after <= out.cost_before);
    let (_, other) = run_polish_model(1, 500, 0)?;
    near("seed one", other.cost_after, fx.nums("seed1_500")?[0], 1e-9)
}

#[test]
#[ignore = "built in slice-1122-d"]
fn one_seed_is_deterministic_fixed_parts_stay_and_the_budget_bounds_the_loop() -> Result<(), String>
{
    let (a, out_a) = run_polish_model(3, 400, 5)?;
    let (b, out_b) = run_polish_model(3, 400, 5)?;
    assert_eq!(
        format!("{:?}{:?}", a.model, out_a),
        format!("{:?}{:?}", b.model, out_b),
        "same seed, same bytes"
    );
    let start = Fixture::load("polish")?;
    for name in ["J", "U"] {
        let i = a.index(name)?;
        assert_eq!(a.model.cells[i], start.model.cells[i], "{name} is fixed");
    }
    let (_, short) = run_polish_model(0, 20, 0)?;
    assert_eq!(short.moves_used, 20);
    assert_eq!(
        short.accepted + short.rejected_by_rule + short.rejected_by_cost,
        20,
        "every move is counted once"
    );
    let s = PlacementSettings::default();
    for i in 0..a.model.cells.len() {
        assert_eq!(
            violations(&a.model, &s, i)?,
            (0, 0, 0),
            "the end state breaks no hard rule"
        );
    }
    Ok(())
}

#[test]
#[ignore = "built in slice-1122-d"]
fn a_model_with_nothing_movable_stops_at_once_and_too_many_parts_are_refused() -> Result<(), String>
{
    let mut fixed = Fixture::parse("cell F 5 5 1 1 fixed\n")?;
    let out = anneal(&mut fixed.model, &PlacementSettings::default())?;
    assert_eq!((out.stop, out.moves_used), (Stop::NothingToMove, 0));
    let mut fx = Fixture::load("polish")?;
    let small = PlacementSettings {
        max_polish_cells: 3,
        ..PlacementSettings::default()
    };
    assert!(
        anneal(&mut fx.model, &small).is_err(),
        "four movable parts is above the cap of 3"
    );
    Ok(())
}

// ---------------------------------------------------------------- slice 1122-e

fn error_set(s: &Session) -> BTreeSet<(String, Vec<u64>)> {
    pcbcraft_rules::drc(&s.project)
        .into_iter()
        .filter(|f| f.severity == "error")
        .map(|f| {
            let mut o = f.objects;
            o.sort_unstable();
            (f.code.to_string(), o)
        })
        .collect()
}

fn board(s: &mut Session) -> Result<Vec<u64>, Error> {
    s.project.auto_sync = false;
    let mut ids = Vec::new();
    for (i, (kind, r)) in [("connector", "J1"), ("resistor", "R1"), ("resistor", "R2")]
        .into_iter()
        .enumerate()
    {
        let v = s.execute("component.add", json!({"kind":kind,"reference":r,"value":"v","position":{"x":10.0 + 10.0 * i as f64,"y":10}}), None)?;
        ids.push(
            v["result"]["id"]
                .as_u64()
                .ok_or_else(|| Error::Invalid("no id".into()))?,
        );
    }
    let n = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(|| Error::Invalid("no id".into()))?;
    for (a, b) in [(0, 1), (1, 2)] {
        s.execute("wire.connect", json!({"net":n,"from":{"component":ids[a],"pin":"1"},"to":{"component":ids[b],"pin":"2"}}), None)?;
    }
    s.execute("board.sync", json!({}), None)?;
    let fps: Vec<u64> = ids
        .iter()
        .filter_map(|c| {
            s.project
                .board
                .footprints
                .iter()
                .find(|f| f.component == *c)
                .map(|f| f.id)
        })
        .collect();
    for (id, x, y) in [
        (fps[0], 5.0, 20.0),
        (fps[1], 50.0, 5.0),
        (fps[2], 50.0, 35.0),
    ] {
        s.execute(
            "footprint.move",
            json!({"id":id,"position":{"x":x,"y":y}}),
            None,
        )?;
    }
    s.execute(
        "object.set_locked",
        json!({"id":fps[0],"locked":true}),
        None,
    )?;
    Ok(fps)
}

#[test]
#[ignore = "built in slice-1122-e"]
fn the_polish_pass_adds_no_error_keeps_locked_parts_and_is_one_undo_step() -> Result<(), Error> {
    let mut s = Session::default();
    let fps = board(&mut s)?;
    let pos = |s: &Session| -> Vec<(u64, f64, f64, f64)> {
        s.project
            .board
            .footprints
            .iter()
            .map(|f| (f.id, f.position.x, f.position.y, f.rotation))
            .collect()
    };
    let (before, errors) = (pos(&s), error_set(&s));
    let scope = Scope::from_value(&s.project, &json!({"layers":["F.Cu"]}))?;
    let out = run_polish(
        &mut s.project.clone(),
        &scope,
        &PlacementSettings::default(),
    )
    .map_err(Error::Invalid)?;
    assert!(
        out.cost_after <= out.cost_before,
        "the best state is restored"
    );
    let params = json!({"scope":{"layers":["F.Cu"]},"seed":0,"moves":500});
    let dry = s.dry_run(
        &[Command {
            command: "layout.polish".into(),
            params: params.clone(),
        }],
        None,
    )?;
    assert_eq!(dry.results.len(), 1);
    assert_eq!(pos(&s), before, "a preview changes nothing");
    s.execute("layout.polish", params, None)?;
    let after = pos(&s);
    let (j_before, j_after) = (
        before.iter().find(|p| p.0 == fps[0]),
        after.iter().find(|p| p.0 == fps[0]),
    );
    assert_eq!(
        j_before, j_after,
        "the locked connector keeps its exact position and rotation"
    );
    assert!(
        error_set(&s).is_subset(&errors),
        "no new error code or object"
    );
    s.undo()?;
    assert_eq!(
        pos(&s),
        before,
        "one undo restores every position and rotation"
    );
    assert!(
        s.execute("layout.polish", json!({"scope":{}}), None)
            .is_err(),
        "an empty scope is refused"
    );
    Ok(())
}
