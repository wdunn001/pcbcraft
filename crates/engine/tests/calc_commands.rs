//! Engine wiring for design calculator commands (#174).
use pcbcraft_engine::{Error, Session};
use serde_json::json;

fn width(result: &serde_json::Value) -> f64 {
    result["result"]["width_mm"]
        .as_f64()
        .expect("width_mm")
}

#[test]
fn track_width_matches_ipc2152_table_for_two_amps() -> Result<(), Error> {
    let mut s = Session::default();
    let out = s.execute(
        "calc.track_width",
        json!({"current": 2.0, "temperature_rise": 10.0, "method": "ipc2152"}),
        None,
    )?;
    let w = width(&out);
    assert!((w - 0.762).abs() < 0.001, "expected 0.762 mm, got {w}");
    assert_eq!(
        out["result"]["method_used"].as_str(),
        Some("ipc2152")
    );
    Ok(())
}

#[test]
fn apply_track_width_updates_net_class_default() -> Result<(), Error> {
    let mut s = Session::default();
    let class = s.execute(
        "netclass.add",
        json!({"name": "Power", "clearance": 0.2, "width": 0.25, "nets": []}),
        None,
    )?["result"]["id"]
        .as_u64()
        .expect("class id");
    s.execute(
        "calc.apply",
        json!({"kind": "track_width", "class": class, "current": 2.0, "temperature_rise": 10.0}),
        None,
    )?;
    let w = s
        .project
        .net_classes
        .iter()
        .find(|c| c.id == class)
        .map(|c| c.width)
        .expect("Power class");
    assert!((w - 0.762).abs() < 0.01, "Power class width should reflect 2 A at 10 K");
    Ok(())
}

#[test]
fn divider_and_convert_return_meta() -> Result<(), Error> {
    let mut s = Session::default();
    let div = s.execute(
        "calc.divider",
        json!({"vin": 5.0, "r1": 1000.0, "r2": 1000.0}),
        None,
    )?;
    assert!((div["result"]["vout_v"].as_f64().unwrap() - 2.5).abs() < 0.001);
    assert!(div["result"]["meta"]["formula"].is_string());

    let mil = s.execute(
        "calc.convert",
        json!({"value": 30.0, "from": "mil", "to": "mm"}),
        None,
    )?;
    assert!((mil["result"]["value"].as_f64().unwrap() - 0.762).abs() < 0.001);
    Ok(())
}

#[test]
fn calc_list_exposes_every_calculator() -> Result<(), Error> {
    let mut s = Session::default();
    let out = s.execute("calc.list", json!({}), None)?;
    assert_eq!(out["result"]["count"].as_u64(), Some(32));
    assert!(
        out["result"]["calculators"]
            .as_array()
            .is_some_and(|a| a.iter().any(|c| c["id"] == "trace_width"))
    );
    Ok(())
}
