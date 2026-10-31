//! Engine tests for `layout.report`, the shared scope, and `missing_decoupling`.
use pcbcraft_engine::{Error, Session};
use serde_json::{Value, json};

fn id(v: &Value) -> Result<u64, Error> {
    v["result"]["id"]
        .as_u64()
        .ok_or_else(|| Error::Invalid("no id".into()))
}

/// U1 (a part with one power pin on VCC), C1 on VCC and GND, J1 on both nets.
/// Returns the session and the ids (u1, c1, vcc).
fn board() -> Result<(Session, u64, u64, u64), Error> {
    let mut s = Session::default();
    s.project.auto_sync = false;
    let u1 = id(&s.execute(
        "component.add",
        json!({"kind":"resistor","reference":"U1","value":"MCU","position":{"x":10,"y":10}}),
        None,
    )?)?;
    let c1 = id(&s.execute(
        "component.add",
        json!({"kind":"capacitor","reference":"C1","value":"100n","position":{"x":20,"y":10}}),
        None,
    )?)?;
    let j1 = id(&s.execute(
        "component.add",
        json!({"kind":"connector","reference":"J1","value":"Power","position":{"x":30,"y":10}}),
        None,
    )?)?;
    let vcc = id(&s.execute("net.create", json!({"name":"VCC"}), None)?)?;
    let gnd = id(&s.execute("net.create", json!({"name":"GND"}), None)?)?;
    for (net, a, ap, b, bp) in [
        (vcc, u1, "1", c1, "1"),
        (vcc, c1, "1", j1, "1"),
        (gnd, c1, "2", j1, "2"),
    ] {
        s.execute(
            "wire.connect",
            json!({"net":net,"from":{"component":a,"pin":ap},"to":{"component":b,"pin":bp}}),
            None,
        )?;
    }
    // Make U1 pin 1 a power input, as an integrated circuit would have.
    for c in &mut s.project.components {
        if c.id == u1 {
            for p in &mut c.pins {
                if p.number == "1" {
                    p.electrical_type = "power_in".into();
                    p.name = "VDD".into();
                }
            }
        }
    }
    s.execute("board.sync", json!({}), None)?;
    Ok((s, u1, c1, vcc))
}

fn fp_of(s: &Session, component: u64) -> Result<u64, Error> {
    s.project
        .board
        .footprints
        .iter()
        .find(|f| f.component == component)
        .map(|f| f.id)
        .ok_or_else(|| Error::Invalid("no footprint".into()))
}

fn place(s: &mut Session, component: u64, x: f64, y: f64) -> Result<(), Error> {
    let id = fp_of(s, component)?;
    s.execute(
        "footprint.move",
        json!({"id":id,"position":{"x":x,"y":y}}),
        None,
    )?;
    Ok(())
}

fn decoupling(s: &Session) -> Vec<String> {
    pcbcraft_rules::erc(&s.project)
        .into_iter()
        .filter(|f| f.code == "missing_decoupling")
        .map(|f| f.message)
        .collect()
}

#[test]
fn an_empty_scope_is_refused_and_a_named_one_is_not() -> Result<(), Error> {
    let (mut s, u1, _, _) = board()?;
    for bad in [
        json!({}),
        json!({"scope":{}}),
        json!({"scope":{"layers":[],"nets":[],"components":[]}}),
    ] {
        assert!(s.execute("layout.report", bad, None).is_err());
    }
    assert!(
        s.execute(
            "layout.report",
            json!({"scope":{"components":[999999]}}),
            None
        )
        .is_err()
    );
    assert!(
        s.execute(
            "layout.report",
            json!({"scope":{"layers":["Nope.Cu"]}}),
            None
        )
        .is_err()
    );
    assert!(
        s.execute(
            "layout.report",
            json!({"scope":{"components":[u1],"extra":1}}),
            None
        )
        .is_err()
    );
    let ok = s.execute("layout.report", json!({"scope":{"components":[u1]}}), None)?;
    assert_eq!(ok["result"]["parts"].as_array().map(Vec::len), Some(1));
    Ok(())
}

#[test]
fn the_report_changes_nothing_and_repeats_byte_for_byte() -> Result<(), Error> {
    let (mut s, _, _, _) = board()?;
    let before = serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;
    let rev = s.revision;
    let scope = json!({"scope":{"layers":["F.Cu"]}});
    let a = s.execute("layout.report", scope.clone(), None)?;
    let b = s.execute("layout.report", scope, None)?;
    assert_eq!(a["result"].to_string(), b["result"].to_string());
    assert_eq!(s.revision, rev, "a read does not bump the revision");
    assert_eq!(
        serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?,
        before
    );
    Ok(())
}

#[test]
fn report_lists_parts_gaps_nets_and_totals() -> Result<(), Error> {
    let (mut s, u1, c1, vcc) = board()?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 20.0, 10.0)?;
    let r =
        s.execute("layout.report", json!({"scope":{"layers":["F.Cu"]}}), None)?["result"].clone();
    let refs: Vec<&str> = r["parts"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p["reference"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(refs, ["C1", "J1", "U1"], "parts sorted by reference");
    assert_eq!(r["gaps"].as_array().map(Vec::len), Some(3));
    let vcc_row = r["nets"]
        .as_array()
        .and_then(|a| a.iter().find(|n| n["net"] == json!(vcc)))
        .cloned()
        .ok_or(Error::Invalid("no VCC".into()))?;
    assert_eq!(vcc_row["pads"], json!(3));
    let h = vcc_row["hpwl_mm"].as_f64().unwrap_or(-1.0);
    let t = vcc_row["ratsnest_mm"].as_f64().unwrap_or(-1.0);
    assert!(h > 0.0 && t > 0.0 && t >= h * 0.5 - 1e-6, "h {h} t {t}");
    let sum: f64 = r["nets"]
        .as_array()
        .map(|a| a.iter().filter_map(|n| n["hpwl_mm"].as_f64()).sum())
        .unwrap_or(-1.0);
    assert!((sum - r["totals"]["hpwl_mm"].as_f64().unwrap_or(-2.0)).abs() < 1e-3);
    Ok(())
}

#[test]
fn moving_a_part_away_makes_the_wirelength_grow() -> Result<(), Error> {
    let (mut s, u1, c1, _) = board()?;
    let total = |s: &mut Session| -> Result<f64, Error> {
        Ok(s.execute("layout.report", json!({"scope":{"layers":["F.Cu"]}}), None)?["result"]["totals"]["ratsnest_mm"].as_f64().unwrap_or(-1.0))
    };
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 14.0, 10.0)?;
    place(&mut s, c1 + 1, 30.0, 10.0)?;
    let near = total(&mut s)?;
    place(&mut s, c1, 50.0, 10.0)?;
    let far = total(&mut s)?;
    assert!(far > near, "near {near} far {far}");
    Ok(())
}

#[test]
fn parts_off_the_board_and_overlapping_parts_are_reported() -> Result<(), Error> {
    let (mut s, u1, c1, _) = board()?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 10.0, 10.0)?;
    let r = s.execute(
        "layout.report",
        json!({"scope":{"components":[u1,c1]}}),
        None,
    )?["result"]
        .clone();
    assert_eq!(r["overlaps"].as_array().map(Vec::len), Some(1), "{r}");
    place(&mut s, c1, 5000.0, 10.0)?;
    let r = s.execute(
        "layout.report",
        json!({"scope":{"components":[u1,c1]}}),
        None,
    )?["result"]
        .clone();
    assert_eq!(r["overlaps"].as_array().map(Vec::len), Some(0));
    assert_eq!(r["off_board"][0]["reference"], json!("C1"), "{r}");
    Ok(())
}

#[test]
fn aligned_parts_form_a_row() -> Result<(), Error> {
    let (mut s, u1, c1, _) = board()?;
    place(&mut s, u1, 10.0, 12.0)?;
    place(&mut s, c1, 25.0, 12.0)?;
    let r = s.execute(
        "layout.report",
        json!({"scope":{"components":[u1,c1]}}),
        None,
    )?["result"]
        .clone();
    let rows: Vec<&Value> = r["alignment_groups"]
        .as_array()
        .map(|a| a.iter().filter(|g| g["axis"] == "y").collect())
        .unwrap_or_default();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["parts"].as_array().map(Vec::len), Some(2));
    Ok(())
}

#[test]
fn a_locked_part_is_not_movable_under_respect_locked() -> Result<(), Error> {
    let (mut s, u1, _, _) = board()?;
    let f = fp_of(&s, u1)?;
    s.execute("footprint.set_locked", json!({"id":f,"locked":true}), None)?;
    let r =
        s.execute("layout.report", json!({"scope":{"components":[u1]}}), None)?["result"].clone();
    assert_eq!(r["parts"][0]["movable"], json!(false));
    let r = s.execute(
        "layout.report",
        json!({"scope":{"components":[u1],"respect_locked":false}}),
        None,
    )?["result"]
        .clone();
    assert_eq!(r["parts"][0]["movable"], json!(true));
    Ok(())
}

#[test]
fn a_power_pin_with_no_capacitor_is_a_finding_and_adding_one_clears_it() -> Result<(), Error> {
    let (mut s, u1, c1, _) = board()?;
    s.execute(
        "rules.decoupling_set",
        json!({"max_distance_mm":10.0,"suggested_value":"100 nF"}),
        None,
    )?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 13.0, 10.0)?;
    assert!(decoupling(&s).is_empty(), "{:?}", decoupling(&s));
    // Take the capacitor off the supply net.
    let saved = s.project.clone();
    for c in &mut s.project.components {
        if c.id == c1 {
            for p in &mut c.pins {
                p.net = None;
            }
        }
    }
    let found = decoupling(&s);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("U1 pin 1 (VDD)") && found[0].contains("100 nF"),
        "{}",
        found[0]
    );
    s.project = saved;
    assert!(decoupling(&s).is_empty());
    Ok(())
}

#[test]
fn a_capacitor_that_is_too_far_is_a_finding_and_the_distance_is_configurable() -> Result<(), Error>
{
    let (mut s, u1, c1, _) = board()?;
    s.execute(
        "rules.decoupling_set",
        json!({"max_distance_mm":10.0}),
        None,
    )?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 40.0, 10.0)?;
    let found = decoupling(&s);
    assert_eq!(found.len(), 1);
    assert!(
        found[0].contains("C1") && found[0].contains("10.0 mm limit"),
        "{}",
        found[0]
    );
    s.execute(
        "rules.decoupling_set",
        json!({"max_distance_mm":50.0}),
        None,
    )?;
    assert!(decoupling(&s).is_empty());
    assert!(
        s.execute("rules.decoupling_set", json!({"max_distance_mm":0.0}), None)
            .is_err()
    );
    assert!(s.execute("rules.decoupling_set", json!({}), None).is_err());
    Ok(())
}

#[test]
fn the_decoupling_finding_obeys_severity_and_off() -> Result<(), Error> {
    let (mut s, u1, c1, _) = board()?;
    s.execute("rules.decoupling_set", json!({"max_distance_mm":0.1}), None)?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 40.0, 10.0)?;
    let level = |s: &Session| {
        pcbcraft_rules::erc(&s.project)
            .into_iter()
            .find(|f| f.code == "missing_decoupling")
            .map(|f| f.severity)
    };
    assert_eq!(level(&s).as_deref(), Some("warning"));
    s.execute(
        "rules.severity",
        json!({"code":"missing_decoupling","severity":"error"}),
        None,
    )?;
    assert_eq!(level(&s).as_deref(), Some("error"));
    s.execute(
        "rules.severity",
        json!({"code":"missing_decoupling","severity":"off"}),
        None,
    )?;
    assert_eq!(level(&s), None);
    Ok(())
}

#[test]
fn ground_pins_are_not_asked_for_decoupling() -> Result<(), Error> {
    let (mut s, u1, _, _) = board()?;
    for c in &mut s.project.components {
        if c.id == u1 {
            c.pins
                .iter_mut()
                .for_each(|p| p.electrical_type = "power_in".into());
            for p in &mut c.pins {
                if p.number == "2" {
                    p.net = s
                        .project
                        .nets
                        .iter()
                        .find(|n| n.name == "GND")
                        .map(|n| n.id);
                }
            }
        }
    }
    assert!(
        decoupling(&s).iter().all(|m| !m.contains("pin 2")),
        "{:?}",
        decoupling(&s)
    );
    Ok(())
}

#[test]
fn the_commands_are_listed_with_schemas() {
    let all = pcbcraft_engine::commands();
    for id in ["layout.report", "rules.decoupling_set"] {
        let c = all.iter().find(|c| c.id == id);
        assert!(
            c.is_some_and(|c| c.input_schema["type"] == "object"),
            "{id}"
        );
    }
}
