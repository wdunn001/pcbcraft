//! Engine tests for the physical layer record and dielectric-aware impedance.
use pcbcraft_engine::{Error, Session};
use serde_json::{Value, json};

type T = Result<(), Error>;

fn missing() -> Error {
    Error::Invalid("missing field".into())
}
fn num(v: &Value) -> Result<f64, Error> {
    v.as_f64().ok_or_else(missing)
}
fn rec(s: &mut Session, p: Value) -> Result<(), Error> {
    s.execute("board.set_layer_record", p, None).map(|_| ())
}
/// A board with one routed 0.3 mm track on F.Cu and a two layer stack.
fn routed() -> Result<Session, Error> {
    let mut s = Session::default();
    s.execute("board.resize", json!({"width":100.0,"height":50.0}), None)?;
    let net = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    s.execute(
        "track.add",
        json!({"net":net,"start":{"x":10.0,"y":20.0},"end":{"x":60.0,"y":20.0},"width":0.3}),
        None,
    )?;
    Ok(s)
}
fn ohms(s: &mut Session) -> Result<f64, Error> {
    num(&s.execute("net.impedance", json!({}), None)?["result"]["rows"][0]["impedance_ohm"])
}

#[test]
fn set_layer_record_accepts_material_dk_df_weight_and_tolerance() -> T {
    let mut s = Session::default();
    rec(
        &mut s,
        json!({"name":"Core","kind":"core","thickness":1.0,"material":"FR-4 standard","dk":4.1,"df":0.015,"thickness_tol_pct":10,"copper_weight_oz":1.0,"foil":"rolled"}),
    )?;
    let r = &s.project.board.layer_records[0];
    assert_eq!(r.dk(), Some(4.1));
    assert_eq!(r.thickness_tol_pct, Some(10.0));
    for bad in [
        json!({"dk":0.9}),
        json!({"dk":21}),
        json!({"df":-0.1}),
        json!({"thickness_tol_pct":51}),
        json!({"copper_weight_oz":11}),
        json!({"foil":"x"}),
        json!({"material":"unobtainium"}),
        json!({"linked_copper":"Nope"}),
    ] {
        let mut p = json!({"name":"Core","kind":"core","thickness":1.0});
        if let (Some(o), Some(b)) = (p.as_object_mut(), bad.as_object()) {
            o.extend(b.clone());
        }
        assert!(s.execute("board.set_layer_record", p, None).is_err());
    }
    let list = s.execute("board.materials", json!({}), None)?;
    assert!(
        list["result"]["materials"]
            .as_array()
            .is_some_and(|m| !m.is_empty())
    );
    Ok(())
}

#[test]
fn microstrip_impedance_uses_the_dielectric_row_under_the_layer() -> T {
    let mut s = routed()?;
    rec(
        &mut s,
        json!({"name":"F.Cu","kind":"copper","thickness":0.035}),
    )?;
    rec(
        &mut s,
        json!({"name":"Prepreg","kind":"prepreg","thickness":0.1,"dk":4.2}),
    )?;
    rec(
        &mut s,
        json!({"name":"Core","kind":"core","thickness":1.4,"dk":4.2}),
    )?;
    rec(
        &mut s,
        json!({"name":"B.Cu","kind":"copper","thickness":0.035}),
    )?;
    let thin = ohms(&mut s)?;
    // The old single-value result uses the whole board thickness as the height.
    let mut old = routed()?;
    old.execute("board.set_stackup", json!({"thickness":1.6,"er":4.2}), None)?;
    let single = ohms(&mut old)?;
    assert!(thin < single - 5.0, "thin {thin} vs single {single}");
    Ok(())
}

#[test]
fn the_board_dk_is_the_fallback_when_no_row_carries_one() -> T {
    let mut plain = routed()?;
    let before = ohms(&mut plain)?;
    let mut s = routed()?;
    rec(
        &mut s,
        json!({"name":"F.Cu","kind":"copper","thickness":0.035}),
    )?;
    rec(
        &mut s,
        json!({"name":"Prepreg","kind":"prepreg","thickness":0.1}),
    )?;
    rec(
        &mut s,
        json!({"name":"B.Cu","kind":"copper","thickness":0.035}),
    )?;
    assert!((ohms(&mut s)? - before).abs() < 1e-9);
    Ok(())
}
