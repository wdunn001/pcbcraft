//! Tests for global placement: slices 1121-a to 1121-d.
use super::fixture::{Fixture, near};
use pcbcraft_engine::placement::legalize::legalize;
use pcbcraft_engine::placement::model::boxes_overlap;
use pcbcraft_engine::placement::quadratic::{build_springs, solve};
use pcbcraft_engine::placement::{
    PlacementSettings, Rng, build_model, place_global, run_place_global,
};
use pcbcraft_engine::scope::Scope;
use pcbcraft_engine::wirelength::{hpwl, net_pad_points};
use pcbcraft_engine::{Command, Error, Session};
use serde_json::json;

/// Solve a fixture and write the free positions into its model.
fn solved(name: &str, settings: &PlacementSettings) -> Result<(Fixture, usize), String> {
    let mut fx = Fixture::load(name)?;
    let sol = solve(&fx.model, settings)?;
    for &(i, x, y) in &sol.positions {
        fx.model.cells[i].x = x;
        fx.model.cells[i].y = y;
    }
    Ok((fx, sol.iterations))
}

fn assert_cell(fx: &Fixture, name: &str, key: &str, tol: f64) -> Result<(), String> {
    let want = fx.nums(key)?;
    let c = &fx.model.cells[fx.index(name)?];
    near(&format!("{name}.x"), c.x, want[0], tol)?;
    near(&format!("{name}.y"), c.y, want[1], tol)
}

#[test]
fn the_fixtures_load_and_start_with_the_pinned_wirelength() -> Result<(), String> {
    let p = Fixture::load("pipeline")?;
    near(
        "pipeline start",
        p.model.hpwl_of(),
        p.nums("hpwl_before")?[0],
        1e-12,
    )?;
    let r = Fixture::load("rotate")?;
    near("turned pin", r.model.hpwl_of(), r.nums("hpwl")?[0], 1e-12)?;
    let mut flat = Fixture::load("rotate")?;
    flat.model.cells[1].turns = 0;
    near(
        "unturned pin",
        flat.model.hpwl_of(),
        r.nums("hpwl_unturned")?[0],
        1e-12,
    )?;
    let c = Fixture::load("cost")?;
    near("cost fixture wire", c.model.hpwl_of(), 11.0, 1e-12)
}

// ---------------------------------------------------------------- slice 1121-a

/// U1 (power pin 1 on VCC), C1, J1 and R1 on the 60 by 40 default board.
/// J1 is locked; R1 sits on the left edge. Returns the session and the
/// footprint ids (u1, c1, j1, r1).
fn project() -> Result<(Session, [u64; 4]), Error> {
    let mut s = Session::default();
    s.project.auto_sync = false;
    let mut add = |kind: &str, reference: &str, x: f64| -> Result<u64, Error> {
        let v = s.execute(
            "component.add",
            json!({"kind":kind,"reference":reference,"value":"v","position":{"x":x,"y":10}}),
            None,
        )?;
        v["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("no id".into()))
    };
    let (u1, c1, j1, r1) = (
        add("resistor", "U1", 10.0)?,
        add("capacitor", "C1", 20.0)?,
        add("connector", "J1", 30.0)?,
        add("resistor", "R1", 40.0)?,
    );
    let mut net = |name: &str| -> Result<u64, Error> {
        let v = s.execute("net.create", json!({"name":name}), None)?;
        v["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("no id".into()))
    };
    let (vcc, gnd, sig) = (net("VCC")?, net("GND")?, net("SIG")?);
    for (n, a, ap, b, bp) in [
        (vcc, u1, "1", c1, "1"),
        (vcc, c1, "1", j1, "1"),
        (gnd, c1, "2", j1, "2"),
        (sig, u1, "2", r1, "1"),
    ] {
        s.execute(
            "wire.connect",
            json!({"net":n,"from":{"component":a,"pin":ap},"to":{"component":b,"pin":bp}}),
            None,
        )?;
    }
    for c in &mut s.project.components {
        if c.id == u1 {
            for p in &mut c.pins {
                if p.number == "1" {
                    p.electrical_type = "power_in".into();
                }
            }
        }
    }
    s.execute("board.sync", json!({}), None)?;
    let fp = |s: &Session, c: u64| {
        s.project
            .board
            .footprints
            .iter()
            .find(|f| f.component == c)
            .map(|f| f.id)
            .ok_or_else(|| Error::Invalid("no footprint".into()))
    };
    let ids = [fp(&s, u1)?, fp(&s, c1)?, fp(&s, j1)?, fp(&s, r1)?];
    for (id, x, y) in [
        (ids[0], 20.0, 20.0),
        (ids[1], 30.0, 28.0),
        (ids[2], 40.0, 20.0),
        (ids[3], 1.0, 30.0),
    ] {
        s.execute(
            "footprint.move",
            json!({"id":id,"position":{"x":x,"y":y}}),
            None,
        )?;
    }
    s.execute(
        "object.set_locked",
        json!({"id":ids[2],"locked":true}),
        None,
    )?;
    Ok((s, ids))
}

#[test]
fn the_model_marks_locked_and_edge_parts_fixed_and_down_weights_power_nets() -> Result<(), Error> {
    let (s, ids) = project()?;
    let scope = Scope::from_value(&s.project, &json!({"layers":["F.Cu"]}))?;
    let m =
        build_model(&s.project, &scope, &PlacementSettings::default()).map_err(Error::Invalid)?;
    // Cells are sorted by reference: C1, J1, R1, U1.
    let order: Vec<u64> = m.cells.iter().map(|c| c.id).collect();
    assert_eq!(
        order,
        vec![ids[1], ids[2], ids[3], ids[0]],
        "cells sorted by reference then id"
    );
    let fixed: Vec<bool> = m.cells.iter().map(|c| c.fixed).collect();
    assert_eq!(
        fixed,
        vec![false, true, true, false],
        "J1 is locked and R1 is on the edge; C1 and U1 are free"
    );
    // Nets: VCC and GND are power (a typed pin, and the rail name), SIG is a signal net.
    let mut weights: Vec<f64> = m.nets.iter().map(|n| n.weight).collect();
    weights.sort_by(f64::total_cmp);
    assert_eq!(
        weights,
        vec![0.1, 0.1, 1.0],
        "two rails at 0.1, one signal net at 1.0"
    );
    assert!(
        m.nets.iter().all(|n| n.pins.len() >= 2),
        "no single pin net"
    );
    assert_eq!((m.board.x1, m.board.y1), (60.0, 40.0));
    Ok(())
}

#[test]
fn out_of_scope_parts_are_anchors_and_two_builds_are_equal() -> Result<(), Error> {
    let (s, ids) = project()?;
    let scope = Scope::from_value(
        &s.project,
        &json!({"components":[s.project.board.footprints.iter().find(|f| f.id == ids[0]).map(|f| f.component)]}),
    )?;
    let a =
        build_model(&s.project, &scope, &PlacementSettings::default()).map_err(Error::Invalid)?;
    let b =
        build_model(&s.project, &scope, &PlacementSettings::default()).map_err(Error::Invalid)?;
    assert_eq!(a, b, "the model is a pure function of the project");
    assert_eq!(
        a.cells.iter().filter(|c| !c.fixed).count(),
        1,
        "only U1 is in scope and free"
    );
    let too_many = PlacementSettings {
        max_cells: 2,
        ..PlacementSettings::default()
    };
    assert!(
        build_model(&s.project, &scope, &too_many).is_err(),
        "more parts than max_cells is refused"
    );
    Ok(())
}

// ---------------------------------------------------------------- slice 1121-b

#[test]
#[ignore = "built in slice-1121-b"]
fn a_four_pin_net_becomes_six_springs_of_one_third() -> Result<(), String> {
    let fx = Fixture::load("clique")?;
    let springs = build_springs(&fx.model, &PlacementSettings::default())?;
    assert_eq!(springs.len(), 6, "4 pins give 6 pairs");
    for s in &springs {
        near("clique weight", s.weight, 1.0 / 3.0, 1e-12)?;
    }
    let b = fx.index("B")?;
    let last = &springs[5];
    assert_eq!(
        (last.a, last.b),
        (fx.index("D")?, b),
        "pairs are taken i < j in pin order, B is last"
    );
    assert_eq!(
        last.off_b,
        (0.5, 0.0),
        "the pin offset of B travels with the spring"
    );
    Ok(())
}

#[test]
#[ignore = "built in slice-1121-b"]
fn nets_with_too_many_pins_and_pairs_on_one_cell_make_no_spring() -> Result<(), String> {
    let mut text = String::from("cell A 0 0 1 1 fixed\n");
    let mut net = String::from("net big 1 A");
    for i in 0..64 {
        text.push_str(&format!("cell F{i} {i} 5 1 1 free\n"));
        net.push_str(&format!(" F{i}"));
    }
    text.push_str(&net);
    text.push_str("\nnet same 1 A:1,0 A:-1,0\n");
    let fx = Fixture::parse(&text)?;
    let springs = build_springs(&fx.model, &PlacementSettings::default())?;
    assert!(
        springs.is_empty(),
        "65 pins is above max_net_pins 64, and the two pins of one cell are no spring"
    );
    Ok(())
}

#[test]
#[ignore = "built in slice-1121-b"]
fn a_chain_settles_evenly_between_its_fixed_ends() -> Result<(), String> {
    let (fx, steps) = solved("chain", &PlacementSettings::default())?;
    assert_cell(&fx, "P", "P", 1e-4)?;
    assert_cell(&fx, "Q", "Q", 1e-4)?;
    assert!(
        steps as f64 <= fx.nums("cg_steps_at_most")?[0],
        "two free parts need at most two steps, got {steps}"
    );
    let (a, d) = (
        &fx.model.cells[fx.index("A")?],
        &fx.model.cells[fx.index("D")?],
    );
    assert_eq!(
        (a.x, a.y, d.x, d.y),
        (0.0, 0.0, 30.0, 0.0),
        "fixed cells do not move"
    );
    Ok(())
}

#[test]
#[ignore = "built in slice-1121-b"]
fn two_nets_to_one_side_pull_twice_as_hard() -> Result<(), String> {
    let (fx, _) = solved("weighted", &PlacementSettings::default())?;
    assert_cell(&fx, "B", "B", 1e-4)
}

#[test]
#[ignore = "built in slice-1121-b"]
fn a_four_pin_net_pulls_the_pin_to_the_mean_of_the_other_pins() -> Result<(), String> {
    let (fx, _) = solved("clique", &PlacementSettings::default())?;
    assert_cell(&fx, "B", "B", 1e-4)
}

#[test]
#[ignore = "built in slice-1121-b"]
fn a_power_net_pulls_less_than_a_signal_net() -> Result<(), String> {
    let (fx, _) = solved("power", &PlacementSettings::default())?;
    assert_cell(&fx, "B", "B", 1e-4)?;
    let b = &fx.model.cells[fx.index("B")?];
    let (to_a, to_c) = (b.x.hypot(b.y), (40.0 - b.x).hypot(b.y));
    assert!(
        to_a < to_c / 5.0,
        "B ends {to_a} from the signal anchor and {to_c} from the power anchor"
    );
    Ok(())
}

#[test]
#[ignore = "built in slice-1121-b"]
fn the_iteration_bound_stops_the_solve_after_that_many_steps() -> Result<(), String> {
    let one = PlacementSettings {
        iterations: Some(1),
        ..PlacementSettings::default()
    };
    let (fx, steps) = solved("chain", &one)?;
    assert_eq!(steps, 1);
    // One conjugate gradient step from (3, 3) and (25, -4): pinned from an independent solver.
    let p = &fx.model.cells[fx.index("P")?];
    near("P.x after one step", p.x, 9.346349, 1e-5)?;
    near("P.y after one step", p.y, -0.338367, 1e-5)?;
    let q = &fx.model.cells[fx.index("Q")?];
    near("Q.x after one step", q.x, 19.321687, 1e-5)?;
    near("Q.y after one step", q.y, -0.327796, 1e-5)
}

#[test]
#[ignore = "built in slice-1121-b"]
fn a_part_with_no_net_stays_and_a_model_with_no_free_part_is_refused() -> Result<(), String> {
    let fx = Fixture::parse(
        "cell A 5 5 1 1 free\ncell B 9 9 1 1 free\ncell F 0 0 1 1 fixed\nnet n 1 A F\n",
    )?;
    let sol = solve(&fx.model, &PlacementSettings::default())?;
    let b = sol.positions.iter().find(|p| p.0 == 1).ok_or("B is free")?;
    near("B.x", b.1, 9.0, 1e-6)?;
    near("B.y", b.2, 9.0, 1e-6)?;
    let none = Fixture::parse("cell F 0 0 1 1 fixed\n")?;
    assert!(
        solve(&none.model, &PlacementSettings::default()).is_err(),
        "nothing to move is refused"
    );
    Ok(())
}

// ---------------------------------------------------------------- slice 1121-c

fn legal(
    name: &str,
    settings: &PlacementSettings,
    seed: u64,
) -> Result<(Fixture, Vec<usize>, Vec<usize>), String> {
    let mut fx = Fixture::load(name)?;
    let rep = legalize(&mut fx.model, settings, &mut Rng::new(seed))?;
    Ok((fx, rep.moved, rep.unresolved))
}

fn at(fx: &Fixture, name: &str) -> Result<(f64, f64), String> {
    let c = &fx.model.cells[fx.index(name)?];
    Ok((c.x, c.y))
}

#[test]
#[ignore = "built in slice-1121-c"]
fn the_later_of_two_equal_parts_moves_to_the_first_free_lattice_spot() -> Result<(), String> {
    let (fx, moved, unresolved) = legal("legal_pair", &PlacementSettings::default(), 0)?;
    assert_eq!((moved, unresolved), (vec![1], vec![]), "only B moves");
    assert_eq!(at(&fx, "A")?, (10.0, 10.0), "A is untouched");
    let want = fx.nums("B_no_grid")?;
    let (bx, by) = at(&fx, "B")?;
    near("B.x", bx, want[0], 1e-9)?;
    near("B.y", by, want[1], 1e-9)
}

#[test]
#[ignore = "built in slice-1121-c"]
fn a_grid_lands_the_moved_part_on_the_grid() -> Result<(), String> {
    let grid = PlacementSettings {
        grid_mm: Some(1.0),
        ..PlacementSettings::default()
    };
    let (fx, _, _) = legal("legal_pair", &grid, 0)?;
    let want = fx.nums("B_grid_1")?;
    let (bx, by) = at(&fx, "B")?;
    near("B.x", bx, want[0], 1e-9)?;
    near("B.y", by, want[1], 1e-9)
}

#[test]
#[ignore = "built in slice-1121-c"]
fn the_edge_clearance_closes_the_downward_way() -> Result<(), String> {
    let (fx, moved, _) = legal("legal_edge", &PlacementSettings::default(), 0)?;
    assert_eq!(moved, vec![1]);
    let want = fx.nums("B")?;
    let (bx, by) = at(&fx, "B")?;
    near("B.x", bx, want[0], 1e-9)?;
    near("B.y", by, want[1], 1e-9)?;
    assert_eq!(at(&fx, "A")?, (3.0, 2.0), "the fixed part stays");
    Ok(())
}

#[test]
#[ignore = "built in slice-1121-c"]
fn two_fixed_parts_that_overlap_are_reported_and_nothing_moves() -> Result<(), String> {
    let mut fx = Fixture::load("legal_locked")?;
    let rep = legalize(
        &mut fx.model,
        &PlacementSettings::default(),
        &mut Rng::new(0),
    )?;
    assert!(rep.moved.is_empty() && rep.unresolved.is_empty());
    let want = fx.nums("fixed_overlap")?;
    assert_eq!(
        rep.fixed_overlaps,
        vec![(want[0] as usize, want[1] as usize)]
    );
    Ok(())
}

#[test]
#[ignore = "built in slice-1121-c"]
fn six_stacked_parts_spread_with_no_overlap_and_the_seed_breaks_the_ties() -> Result<(), String> {
    let s = PlacementSettings::default();
    for (seed, key) in [(0, "seed0"), (1, "seed1")] {
        let (fx, moved, unresolved) = legal("legal_stack", &s, seed)?;
        assert_eq!(
            (moved.len(), unresolved.len()),
            (5, 0),
            "five parts move, the first stays"
        );
        let want = fx.nums(key)?;
        for (i, c) in fx.model.cells.iter().enumerate() {
            near(&format!("C{} x seed {seed}", i + 1), c.x, want[2 * i], 1e-9)?;
            near(
                &format!("C{} y seed {seed}", i + 1),
                c.y,
                want[2 * i + 1],
                1e-9,
            )?;
            for d in &fx.model.cells[i + 1..] {
                assert!(
                    !boxes_overlap(
                        (c.x, c.y, c.half_x, c.half_y),
                        (d.x, d.y, d.half_x, d.half_y),
                        0.0,
                        1e-9
                    ),
                    "no two parts overlap"
                );
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "built in slice-1121-c"]
fn a_part_inside_a_keepout_moves_out_to_one_of_four_equal_spots_by_seed() -> Result<(), String> {
    for (seed, key) in [(0, "seed0"), (1, "seed1")] {
        let (fx, _, _) = legal("legal_keepout", &PlacementSettings::default(), seed)?;
        let want = fx.nums(key)?;
        let (x, y) = at(&fx, "A")?;
        near("A.x", x, want[0], 1e-9)?;
        near("A.y", y, want[1], 1e-9)?;
    }
    Ok(())
}

#[test]
#[ignore = "built in slice-1121-c"]
fn a_part_with_no_room_is_unresolved_and_stays() -> Result<(), String> {
    let mut fx = Fixture::parse("board 0 0 4 4\ncell A 2 2 1.4 1.4 fixed\ncell B 2 2 1 1 free\n")?;
    let s = PlacementSettings {
        max_rings: 20,
        ..PlacementSettings::default()
    };
    let rep = legalize(&mut fx.model, &s, &mut Rng::new(0))?;
    assert_eq!((rep.moved, rep.unresolved), (vec![], vec![1]));
    assert_eq!((fx.model.cells[1].x, fx.model.cells[1].y), (2.0, 2.0));
    Ok(())
}

// ---------------------------------------------------------------- slice 1121-d

#[test]
#[ignore = "built in slice-1121-d"]
fn the_whole_pass_matches_the_pinned_positions_and_wirelength() -> Result<(), String> {
    let mut fx = Fixture::load("pipeline")?;
    let r = place_global(&mut fx.model, &PlacementSettings::default())?;
    near(
        "hpwl before",
        r.hpwl_before,
        fx.nums("hpwl_before")?[0],
        1e-12,
    )?;
    near("hpwl after", r.hpwl_after, fx.nums("hpwl_after")?[0], 1e-3)?;
    assert_eq!(r.iterations as f64, fx.nums("iterations")?[0]);
    assert!(r.unresolved_cells.is_empty() && r.fixed_overlaps.is_empty());
    for name in ["C1", "C2", "C3", "C4", "C5"] {
        assert_cell(&fx, name, name, 1e-4)?;
    }
    let ids: Vec<u64> = r.moved.iter().map(|m| m.id).collect();
    assert_eq!(
        ids,
        vec![4, 5, 6, 7, 8],
        "only the five free parts are listed"
    );
    for (i, c) in fx.model.cells.iter().enumerate() {
        for d in &fx.model.cells[i + 1..] {
            assert!(
                !boxes_overlap(
                    (c.x, c.y, c.half_x, c.half_y),
                    (d.x, d.y, d.half_x, d.half_y),
                    0.0,
                    1e-9
                ),
                "no overlap after the pass"
            );
        }
    }
    Ok(())
}

#[test]
#[ignore = "built in slice-1121-d"]
fn fixed_parts_do_not_move_and_one_seed_gives_identical_output() -> Result<(), String> {
    let run = |seed: u64| -> Result<(String, Fixture), String> {
        let mut fx = Fixture::load("pipeline")?;
        let r = place_global(
            &mut fx.model,
            &PlacementSettings {
                seed,
                ..PlacementSettings::default()
            },
        )?;
        Ok((format!("{r:?}{:?}", fx.model), fx))
    };
    let (a, fx) = run(7)?;
    let (b, _) = run(7)?;
    assert_eq!(a, b, "same seed, same bytes");
    let start = Fixture::load("pipeline")?;
    for name in ["J", "K", "U"] {
        let i = fx.index(name)?;
        assert_eq!(fx.model.cells[i], start.model.cells[i], "{name} is fixed");
    }
    Ok(())
}

#[test]
#[ignore = "built in slice-1121-d"]
fn a_model_with_no_free_part_or_too_many_is_refused() -> Result<(), String> {
    let mut none = Fixture::parse("cell F 0 0 1 1 fixed\n")?;
    assert!(place_global(&mut none.model, &PlacementSettings::default()).is_err());
    let mut some = Fixture::load("pipeline")?;
    let small = PlacementSettings {
        max_cells: 2,
        ..PlacementSettings::default()
    };
    assert!(
        place_global(&mut some.model, &small).is_err(),
        "five free parts is above max_cells 2"
    );
    Ok(())
}

fn total_hpwl(s: &Session) -> f64 {
    s.project
        .nets
        .iter()
        .map(|n| hpwl(&net_pad_points(&s.project, n.id)))
        .sum()
}

#[test]
#[ignore = "built in slice-1121-d"]
fn the_project_pass_keeps_locked_parts_and_reports_the_wirelength_it_leaves() -> Result<(), Error> {
    let (mut s, ids) = project()?;
    let locked_before = s
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.id == ids[2])
        .map(|f| f.position)
        .ok_or_else(|| Error::Invalid("no J1".into()))?;
    let scope = Scope::from_value(&s.project, &json!({"layers":["F.Cu"]}))?;
    let before = total_hpwl(&s);
    let r = run_place_global(&mut s.project, &scope, &PlacementSettings::default())
        .map_err(Error::Invalid)?;
    let locked_after = s
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.id == ids[2])
        .map(|f| f.position)
        .ok_or_else(|| Error::Invalid("no J1".into()))?;
    assert_eq!(
        (locked_before.x, locked_before.y),
        (locked_after.x, locked_after.y),
        "locked J1 keeps its exact position"
    );
    assert!(
        (r.hpwl_before - before).abs() < 1e-6,
        "the reported start equals the project wirelength"
    );
    assert!(
        (r.hpwl_after - total_hpwl(&s)).abs() < 1e-6,
        "the reported end equals the project wirelength"
    );
    Ok(())
}

#[test]
#[ignore = "built in slice-1121-d"]
fn the_command_previews_applies_in_one_undo_step_and_refuses_an_empty_scope() -> Result<(), Error> {
    let (mut s, _) = project()?;
    let params = json!({"scope":{"layers":["F.Cu"]},"seed":0});
    let before = serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;
    let rev = s.revision;
    let dry = s.dry_run(
        &[Command {
            command: "layout.place_global".into(),
            params: params.clone(),
        }],
        None,
    )?;
    assert_eq!(dry.results.len(), 1, "the preview runs the command once");
    assert_eq!(
        serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?,
        before,
        "a preview changes nothing"
    );
    assert_eq!(s.revision, rev);
    assert!(
        s.execute("layout.place_global", json!({"scope":{}}), None)
            .is_err(),
        "an empty scope is refused"
    );
    s.execute("layout.place_global", params, None)?;
    s.undo()?;
    assert_eq!(
        serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?,
        before,
        "one undo restores every position"
    );
    Ok(())
}
