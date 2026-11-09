//! A newcomer changes how close a ground via must be to a layer change, watches the return path
//! finding come and go, and undoes the change.
//!
//! Story: "As a designer I want to say how close a ground stitching via must sit to a layer
//! change, so the return path check matches how I stitch my boards."
//!
//! # Dependencies
//! - crates/engine/src/lib.rs (`rules.stitch_distance_mm`)
//! - crates/rules/src/return_path.rs (reads the setting)
//!
//! # Do not
//! - Build the board by editing the project directly; every step goes through a command.

use pcbcraft_engine::Session;
use serde_json::{Value, json};

fn run(session: &mut Session, command: &str, params: Value) -> Result<Value, String> {
    session
        .execute(command, params, None)
        .map_err(|e| e.to_string())
}

fn id(answer: &Value) -> Result<u64, String> {
    answer["result"]["id"]
        .as_u64()
        .ok_or_else(|| format!("the command answer has no id: {answer}"))
}

/// The return path findings that complain about a missing stitching via.
fn missing_stitch(session: &Session) -> Vec<String> {
    pcbcraft_rules::drc(&session.project)
        .into_iter()
        .filter(|f| f.code == "return_path" && f.message.contains("stitching via"))
        .map(|f| f.message)
        .collect()
}

/// A fast signal that changes layer at x = 20, a ground pour behind it and a ground via 1 mm
/// from the layer change.
fn board_with_one_layer_change() -> Result<Session, String> {
    let mut s = Session::default();
    let gnd = id(&run(&mut s, "net.create", json!({"name": "GND"}))?)?;
    let sig = id(&run(&mut s, "net.create", json!({"name": "SIG"}))?)?;
    let class = id(&run(
        &mut s,
        "netclass.add",
        json!({"name": "Fast", "clearance": 0.2, "width": 0.25, "nets": [sig]}),
    )?)?;
    run(
        &mut s,
        "netclass.fast_signal",
        json!({"class": class, "fast": true}),
    )?;
    let corners = [
        (0.0, 0.0),
        (40.0, 0.0),
        (40.0, 40.0),
        (0.0, 40.0),
        (0.0, 0.0),
    ];
    let outline: Vec<Value> = corners
        .iter()
        .map(|(x, y)| json!({"x": x, "y": y}))
        .collect();
    run(
        &mut s,
        "zone.place",
        json!({"net": gnd, "layer": "B.Cu", "outline": outline}),
    )?;
    run(
        &mut s,
        "track.add",
        json!({"net": sig, "start": {"x": 10.0, "y": 10.0}, "end": {"x": 20.0, "y": 10.0},
               "width": 0.25, "layer": "F.Cu"}),
    )?;
    run(
        &mut s,
        "track.add",
        json!({"net": sig, "start": {"x": 20.0, "y": 10.0}, "end": {"x": 30.0, "y": 10.0},
               "width": 0.25, "layer": "B.Cu"}),
    )?;
    run(
        &mut s,
        "via.place",
        json!({"net": sig, "position": {"x": 20.0, "y": 10.0}, "diameter": 0.8, "drill": 0.4}),
    )?;
    run(
        &mut s,
        "via.place",
        json!({"net": gnd, "position": {"x": 21.0, "y": 10.0}, "diameter": 0.8, "drill": 0.4}),
    )?;
    Ok(s)
}

#[test]
fn a_newcomer_changes_the_stitching_distance() -> Result<(), String> {
    let mut s = board_with_one_layer_change()?;
    assert!(
        (s.project.stitch_distance_mm - 2.0).abs() < 1e-9,
        "a new design starts at 2 mm"
    );
    assert!(
        missing_stitch(&s).is_empty(),
        "a ground via 1 mm away satisfies the default: {:?}",
        missing_stitch(&s)
    );

    // Asking for a tighter limit than the ground via gives makes the finding appear.
    run(
        &mut s,
        "rules.stitch_distance_mm",
        json!({"distance_mm": 0.5}),
    )?;
    let found = missing_stitch(&s);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("within 0.5 mm"), "{found:?}");

    // A wider limit lets it go again.
    run(
        &mut s,
        "rules.stitch_distance_mm",
        json!({"distance_mm": 3.0}),
    )?;
    assert!(missing_stitch(&s).is_empty());

    // The change is one undo step, and undoing it brings the previous limit and its finding back.
    run(
        &mut s,
        "rules.stitch_distance_mm",
        json!({"distance_mm": 0.5}),
    )?;
    assert_eq!(missing_stitch(&s).len(), 1);
    s.undo().map_err(|e| e.to_string())?;
    assert!((s.project.stitch_distance_mm - 3.0).abs() < 1e-9);
    assert!(missing_stitch(&s).is_empty());

    // A distance outside 0.1 to 50 mm is refused and changes nothing.
    for bad in [json!({"distance_mm": 0.0}), json!({"distance_mm": 50.5})] {
        assert!(
            s.execute("rules.stitch_distance_mm", bad, None).is_err(),
            "an out of range distance is refused"
        );
        assert!((s.project.stitch_distance_mm - 3.0).abs() < 1e-9);
    }
    Ok(())
}
