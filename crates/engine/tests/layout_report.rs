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

/// Joins U1 pad 1 to C1 pad 1 with one straight VCC track, so the copper path
/// between them is as long as the straight run, and moves J1 out of its way.
fn route_supply(s: &mut Session, u1: u64, c1: u64, vcc: u64) -> Result<(), Error> {
    let at = |s: &Session, component: u64| -> Result<(f64, f64), Error> {
        let fp = fp_of(s, component)?;
        let p = s
            .project
            .footprint(fp)
            .and_then(|f| f.pad_position("1"))
            .ok_or_else(|| Error::Invalid("no pad 1".into()))?;
        Ok((p.x, p.y))
    };
    let j1 = s
        .project
        .components
        .iter()
        .find(|c| c.reference == "J1")
        .map(|c| c.id)
        .ok_or_else(|| Error::Invalid("no J1".into()))?;
    place(s, j1, 30.0, 40.0)?;
    let (ax, ay) = at(s, u1)?;
    let (bx, by) = at(s, c1)?;
    s.execute(
        "track.add",
        json!({"net":vcc,"start":{"x":ax,"y":ay},"end":{"x":bx,"y":by},"width":0.3}),
        None,
    )?;
    Ok(())
}

/// Messages of the `decoupling_distance` findings of the board check.
fn far_capacitor(s: &Session) -> Vec<String> {
    pcbcraft_rules::drc(&s.project)
        .into_iter()
        .filter(|f| f.code == "decoupling_distance")
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
    let (mut s, u1, c1, vcc) = board()?;
    s.execute(
        "rules.decoupling_set",
        json!({"max_distance_mm":10.0}),
        None,
    )?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 40.0, 10.0)?;
    route_supply(&mut s, u1, c1, vcc)?;
    // Presence is satisfied; the copper path length is the board check.
    assert!(decoupling(&s).is_empty(), "{:?}", decoupling(&s));
    let found = far_capacitor(&s);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("C1") && found[0].contains("10.0 mm"),
        "{}",
        found[0]
    );
    s.execute(
        "rules.decoupling_set",
        json!({"max_distance_mm":50.0}),
        None,
    )?;
    assert!(far_capacitor(&s).is_empty());
    assert!(
        s.execute("rules.decoupling_set", json!({"max_distance_mm":0.0}), None)
            .is_err()
    );
    assert!(s.execute("rules.decoupling_set", json!({}), None).is_err());
    Ok(())
}

#[test]
fn the_decoupling_finding_obeys_severity_and_off() -> Result<(), Error> {
    let (mut s, u1, c1, vcc) = board()?;
    s.execute("rules.decoupling_set", json!({"max_distance_mm":0.1}), None)?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 40.0, 10.0)?;
    route_supply(&mut s, u1, c1, vcc)?;
    let level = |s: &Session| {
        pcbcraft_rules::drc(&s.project)
            .into_iter()
            .find(|f| f.code == "decoupling_distance")
            .map(|f| f.severity)
    };
    assert_eq!(level(&s).as_deref(), Some("warning"));
    s.execute(
        "rules.severity",
        json!({"code":"decoupling_distance","severity":"error"}),
        None,
    )?;
    assert_eq!(level(&s).as_deref(), Some("error"));
    s.execute(
        "rules.severity",
        json!({"code":"decoupling_distance","severity":"off"}),
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

#[test]
fn report_counts_unrouted_connections_per_net() -> Result<(), Error> {
    let (mut s, u1, c1, _) = board()?;
    let j1 = s
        .project
        .components
        .iter()
        .find(|c| c.reference == "J1")
        .map(|c| c.id)
        .ok_or(Error::Invalid("no J1 component".into()))?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 20.0, 10.0)?;
    place(&mut s, j1, 30.0, 10.0)?;
    let wires = pcbcraft_rules::airwires(&s.project);
    assert_eq!(wires.len(), 3, "the fixture must have 3 airwires");
    let r =
        s.execute("layout.report", json!({"scope":{"layers":["F.Cu"]}}), None)?["result"].clone();
    let unrouted = |net: u64| -> usize {
        r["nets"]
            .as_array()
            .and_then(|a| a.iter().find(|n| n["net"] == json!(net)))
            .and_then(|n| n["unrouted"].as_u64())
            .map(|v| v as usize)
            .unwrap_or(0)
    };
    let vcc = s
        .project
        .nets
        .iter()
        .find(|n| n.name == "VCC")
        .map(|n| n.id)
        .ok_or(Error::Invalid("no VCC net".into()))?;
    let gnd = s
        .project
        .nets
        .iter()
        .find(|n| n.name == "GND")
        .map(|n| n.id)
        .ok_or(Error::Invalid("no GND net".into()))?;
    assert_eq!(unrouted(vcc), 2, "VCC has 2 open airwires");
    assert_eq!(unrouted(gnd), 1, "GND has 1 open airwire");
    let total_unrouted = r["totals"]["unrouted"].as_u64().unwrap_or(0) as usize;
    assert_eq!(
        total_unrouted, 3,
        "totals must report 3 unrouted connections"
    );
    Ok(())
}

#[test]
fn report_routed_length_is_the_track_sum() -> Result<(), Error> {
    let (mut s, u1, c1, _) = board()?;
    let j1 = s
        .project
        .components
        .iter()
        .find(|c| c.reference == "J1")
        .map(|c| c.id)
        .ok_or(Error::Invalid("no J1 component".into()))?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 20.0, 10.0)?;
    place(&mut s, j1, 30.0, 10.0)?;
    let c1_fp = fp_of(&s, c1)?;
    let j1_fp = fp_of(&s, j1)?;
    // Read pad world positions to compute expected Manhattan distance.
    let c1_pad = s
        .project
        .footprint(c1_fp)
        .ok_or(Error::Invalid("no C1 footprint".into()))?
        .pad_position("2")
        .ok_or(Error::Invalid("no C1 pad 2".into()))?;
    let j1_pad = s
        .project
        .footprint(j1_fp)
        .ok_or(Error::Invalid("no J1 footprint".into()))?
        .pad_position("2")
        .ok_or(Error::Invalid("no J1 pad 2".into()))?;
    let expected = (j1_pad.x - c1_pad.x).abs() + (j1_pad.y - c1_pad.y).abs();
    // Route the GND net between C1 pad 2 and J1 pad 2.
    s.execute(
        "track.route",
        json!({"from":{"footprint":c1_fp,"pad":"2"},"to":{"footprint":j1_fp,"pad":"2"},"layer":"F.Cu"}),
        None,
    )?;
    let r =
        s.execute("layout.report", json!({"scope":{"layers":["F.Cu"]}}), None)?["result"].clone();
    let gnd = s
        .project
        .nets
        .iter()
        .find(|n| n.name == "GND")
        .map(|n| n.id)
        .ok_or(Error::Invalid("no GND net".into()))?;
    let vcc = s
        .project
        .nets
        .iter()
        .find(|n| n.name == "VCC")
        .map(|n| n.id)
        .ok_or(Error::Invalid("no VCC net".into()))?;
    let net_row = |net: u64| -> Option<&Value> {
        r["nets"]
            .as_array()
            .and_then(|a| a.iter().find(|n| n["net"] == json!(net)))
    };
    let gnd_row = net_row(gnd).ok_or(Error::Invalid("no GND row".into()))?;
    let vcc_row = net_row(vcc).ok_or(Error::Invalid("no VCC row".into()))?;
    // The routed length is the sum of the two orthogonal segments, which
    // equals the Manhattan distance between the two pad positions.
    let gnd_routed = gnd_row["routed_mm"].as_f64().unwrap_or(-1.0);
    assert!(
        (gnd_routed - expected).abs() < 0.001,
        "GND routed {gnd_routed} expected {expected}"
    );
    // The GND airwire is closed by the track.
    assert_eq!(gnd_row["unrouted"], json!(0), "GND is fully routed");
    // VCC has no tracks.
    let vcc_routed = vcc_row["routed_mm"].as_f64().unwrap_or(-1.0);
    assert!(
        vcc_routed.abs() < 0.001,
        "VCC is not routed, got {vcc_routed}"
    );
    // The total routed length equals the single net with copper.
    let total_routed = r["totals"]["routed_mm"].as_f64().unwrap_or(-1.0);
    assert!(
        (total_routed - expected).abs() < 0.001,
        "total routed {total_routed} expected {expected}"
    );
    Ok(())
}

#[test]
fn fully_routed_net_reports_zero_unrouted() -> Result<(), Error> {
    // Hand-computed: routing GND between C1 pad 2 and J1 pad 2 closes the
    // single GND airwire, leaving only the two VCC airwires. The GND row
    // must show unrouted 0 and unrouted_mm 0.0, and the totals must count
    // only the two open VCC connections.
    let (mut s, u1, c1, _) = board()?;
    let j1 = s
        .project
        .components
        .iter()
        .find(|c| c.reference == "J1")
        .map(|c| c.id)
        .ok_or(Error::Invalid("no J1 component".into()))?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 20.0, 10.0)?;
    place(&mut s, j1, 30.0, 10.0)?;
    let c1_fp = fp_of(&s, c1)?;
    let j1_fp = fp_of(&s, j1)?;
    s.execute(
        "track.route",
        json!({"from":{"footprint":c1_fp,"pad":"2"},"to":{"footprint":j1_fp,"pad":"2"},"layer":"F.Cu"}),
        None,
    )?;
    let r =
        s.execute("layout.report", json!({"scope":{"layers":["F.Cu"]}}), None)?["result"].clone();
    let gnd = s
        .project
        .nets
        .iter()
        .find(|n| n.name == "GND")
        .map(|n| n.id)
        .ok_or(Error::Invalid("no GND net".into()))?;
    let vcc = s
        .project
        .nets
        .iter()
        .find(|n| n.name == "VCC")
        .map(|n| n.id)
        .ok_or(Error::Invalid("no VCC net".into()))?;
    let row = |net: u64| -> Option<&Value> {
        r["nets"]
            .as_array()
            .and_then(|a| a.iter().find(|n| n["net"] == json!(net)))
    };
    let gnd_row = row(gnd).ok_or(Error::Invalid("no GND row".into()))?;
    let vcc_row = row(vcc).ok_or(Error::Invalid("no VCC row".into()))?;
    assert_eq!(gnd_row["unrouted"], json!(0), "GND is fully routed");
    let gnd_open = gnd_row["unrouted_mm"].as_f64().unwrap_or(-1.0);
    assert!(gnd_open.abs() < 0.001, "GND unrouted_mm is zero");
    assert_eq!(
        vcc_row["unrouted"],
        json!(2),
        "VCC still has two open connections"
    );
    assert_eq!(
        r["totals"]["unrouted"],
        json!(2),
        "totals count only the unrouted VCC connections"
    );
    Ok(())
}

#[test]
fn locked_and_hidden_items_are_counted_like_other_items() -> Result<(), Error> {
    // Hand-computed: locking U1 must not change any wirelength value because
    // the report walks every placed pad. Flipping J1 moves its pads to the
    // opposite side, but the report must still include those pads in every
    // net row; the scope layers F.Cu and B.Cu ensure both sides are reported.
    let (mut s, u1, c1, _) = board()?;
    let j1 = s
        .project
        .components
        .iter()
        .find(|c| c.reference == "J1")
        .map(|c| c.id)
        .ok_or(Error::Invalid("no J1 component".into()))?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 20.0, 10.0)?;
    place(&mut s, j1, 30.0, 10.0)?;
    let baseline =
        s.execute("layout.report", json!({"scope":{"layers":["F.Cu"]}}), None)?["result"].clone();
    let u1_fp = fp_of(&s, u1)?;
    s.execute("object.set_locked", json!({"id":u1_fp,"locked":true}), None)?;
    let after_lock =
        s.execute("layout.report", json!({"scope":{"layers":["F.Cu"]}}), None)?["result"].clone();
    assert_eq!(
        baseline["nets"].to_string(),
        after_lock["nets"].to_string(),
        "locking a footprint changes no wirelength value"
    );
    let j1_fp = fp_of(&s, j1)?;
    s.execute("footprint.flip", json!({"id":j1_fp}), None)?;
    let after_flip = s.execute(
        "layout.report",
        json!({"scope":{"layers":["F.Cu","B.Cu"]}}),
        None,
    )?["result"]
        .clone();
    // Both nets must still appear with their pad counts after the flip.
    let nets = after_flip["nets"]
        .as_array()
        .ok_or(Error::Invalid("no nets".into()))?;
    let gnd = s
        .project
        .nets
        .iter()
        .find(|n| n.name == "GND")
        .map(|n| n.id)
        .ok_or(Error::Invalid("no GND net".into()))?;
    let vcc = s
        .project
        .nets
        .iter()
        .find(|n| n.name == "VCC")
        .map(|n| n.id)
        .ok_or(Error::Invalid("no VCC net".into()))?;
    let gnd_row = nets
        .iter()
        .find(|n| n["net"] == json!(gnd))
        .ok_or(Error::Invalid("GND row missing after flip".into()))?;
    let vcc_row = nets
        .iter()
        .find(|n| n["net"] == json!(vcc))
        .ok_or(Error::Invalid("VCC row missing after flip".into()))?;
    assert_eq!(gnd_row["pads"], json!(2));
    assert_eq!(vcc_row["pads"], json!(3));
    Ok(())
}

#[test]
fn place_suggest_returns_candidates_for_decoupling() -> Result<(), Error> {
    let (mut s, u1, c1, _) = board()?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 20.0, 10.0)?;
    s.execute("rules.decoupling_set", json!({"max_distance_mm":5.0}), None)?;
    let result = s.execute(
        "layout.place_suggest",
        json!({"satellite": c1, "anchor": u1, "rule": "decoupling"}),
        None,
    )?;
    let r = result["result"].clone();
    assert_eq!(r["rule"], json!("decoupling"));
    assert_eq!(r["max_distance_mm"], json!(5.0));
    let candidates = r["candidates"]
        .as_array()
        .ok_or(Error::Invalid("no candidates".into()))?;
    assert!(!candidates.is_empty(), "should have at least one candidate");
    for cand in candidates {
        let dist = cand["distance_mm"]
            .as_f64()
            .ok_or(Error::Invalid("no distance".into()))?;
        assert!(dist <= 5.0001, "candidate too far: {dist}");
    }
    Ok(())
}

#[test]
fn place_suggest_does_not_move_the_part() -> Result<(), Error> {
    let (mut s, u1, c1, _) = board()?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 20.0, 10.0)?;
    let before = s.project.clone();
    s.execute(
        "layout.place_suggest",
        json!({"satellite": c1, "anchor": u1, "rule": "decoupling"}),
        None,
    )?;
    let after = s.project.clone();
    assert_eq!(
        serde_json::to_string(&before).map_err(|e| Error::Invalid(e.to_string()))?,
        serde_json::to_string(&after).map_err(|e| Error::Invalid(e.to_string()))?,
        "suggest must not change the project"
    );
    Ok(())
}

#[test]
fn place_apply_moves_the_part() -> Result<(), Error> {
    let (mut s, u1, c1, _) = board()?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 20.0, 10.0)?;
    let result = s.execute(
        "layout.place_apply",
        json!({"component": c1, "position": {"x": 12.0, "y": 10.0}}),
        None,
    )?;
    let r = result["result"].clone();
    assert_eq!(r["component"], json!(c1));
    assert_eq!(r["position"]["x"], json!(12.0));
    assert_eq!(r["position"]["y"], json!(10.0));
    let fp = fp_of(&s, c1)?;
    let pos = s
        .project
        .footprint(fp)
        .ok_or(Error::Invalid("no footprint".into()))?
        .position;
    assert!((pos.x - 12.0).abs() < 0.001, "x not updated");
    assert!((pos.y - 10.0).abs() < 0.001, "y not updated");
    Ok(())
}

#[test]
fn place_apply_fails_for_locked_footprint() -> Result<(), Error> {
    let (mut s, u1, c1, _) = board()?;
    place(&mut s, u1, 10.0, 10.0)?;
    place(&mut s, c1, 20.0, 10.0)?;
    let fp = fp_of(&s, c1)?;
    s.execute("footprint.set_locked", json!({"id":fp,"locked":true}), None)?;
    let result = s.execute(
        "layout.place_apply",
        json!({"component": c1, "position": {"x": 12.0, "y": 10.0}}),
        None,
    );
    assert!(result.is_err(), "should fail for locked footprint");
    Ok(())
}
