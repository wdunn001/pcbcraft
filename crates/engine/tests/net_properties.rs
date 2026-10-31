//! Engine commands and DRC for net electrical properties.
use pcbcraft_engine::{Error, Session};
use serde_json::json;

type T = Result<(), Error>;

fn drc_codes(s: &Session) -> Vec<String> {
    s.checks()["drc"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|f| f["code"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn set_properties_rejects_an_impedance_outside_the_bounds() -> T {
    let mut s = Session::default();
    let net = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(|| Error::Invalid("missing net id".into()))?;
    for bad in [9.9, 200.1, f64::NAN] {
        assert!(
            s.execute(
                "net.set_properties",
                json!({"id": net, "impedance_target": bad}),
                None
            )
            .is_err(),
            "expected rejection for {bad}"
        );
    }
    s.execute(
        "net.set_properties",
        json!({"id": net, "impedance_target": 50.0}),
        None,
    )?;
    Ok(())
}

#[test]
fn a_reference_layer_must_be_a_copper_row() -> T {
    let mut s = Session::default();
    let net = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(|| Error::Invalid("missing net id".into()))?;
    s.execute(
        "board.set_layer_record",
        json!({"name":"F.Cu","kind":"copper","thickness":0.035}),
        None,
    )?;
    s.execute(
        "board.set_layer_record",
        json!({"name":"Core","kind":"core","thickness":1.0}),
        None,
    )?;
    assert!(
        s.execute(
            "net.set_properties",
            json!({"id": net, "reference_layers": ["Core"]}),
            None
        )
        .is_err()
    );
    assert!(
        s.execute(
            "net.set_properties",
            json!({"id": net, "reference_layers": ["Missing"]}),
            None
        )
        .is_err()
    );
    s.execute(
        "net.set_properties",
        json!({"id": net, "reference_layers": ["F.Cu"]}),
        None,
    )?;
    Ok(())
}

#[test]
fn a_target_without_a_reference_layer_is_reported_incomplete() -> T {
    let mut s = Session::default();
    let net = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(|| Error::Invalid("missing net id".into()))?;
    s.execute(
        "net.set_properties",
        json!({"id": net, "impedance_target": 50.0}),
        None,
    )?;
    assert!(
        drc_codes(&s).contains(&"impedance_target_incomplete".to_string()),
        "expected incomplete finding before reference layers are named"
    );
    s.execute(
        "board.set_layer_record",
        json!({"name":"F.Cu","kind":"copper","thickness":0.035}),
        None,
    )?;
    s.execute(
        "net.set_properties",
        json!({"id": net, "reference_layers": ["F.Cu"]}),
        None,
    )?;
    assert!(
        !drc_codes(&s).contains(&"impedance_target_incomplete".to_string()),
        "reference layer should clear the incomplete finding"
    );
    Ok(())
}
