//! Differential-pair impedance targets and DRC (issue 316).
use pcbcraft_engine::{Error, Session};
use serde_json::json;

fn missing() -> Error {
    Error::Invalid("missing field".into())
}

fn drc_codes(s: &mut Session) -> Result<Vec<String>, Error> {
    let drc = s.checks()["drc"].as_array().ok_or_else(missing)?;
    Ok(drc
        .iter()
        .filter_map(|f| f["code"].as_str().map(str::to_string))
        .collect())
}

#[test]
fn usb_profile_apply_records_impedance_targets() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("net.create", json!({"name":"USB_D+"}), None)?;
    s.execute("net.create", json!({"name":"USB_D-"}), None)?;
    s.execute("rules.profile_apply", json!({"id":"usb2"}), None)?;
    let pair = &s.project.board.diff_pairs[0];
    assert_eq!(pair.target_impedance, Some(90.0));
    assert_eq!(pair.impedance_tolerance, Some(0.1));
    Ok(())
}

#[test]
fn profile_dry_run_does_not_list_pair_impedance_as_unsupported() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("net.create", json!({"name":"USB_D+"}), None)?;
    s.execute("net.create", json!({"name":"USB_D-"}), None)?;
    let out = s.execute(
        "rules.profile_apply",
        json!({"id":"usb2","dry_run":true}),
        None,
    )?;
    let unsupported = out["result"]["plan"]["unsupported"]
        .as_array()
        .ok_or_else(missing)?;
    let joined: Vec<String> = unsupported
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    assert!(
        !joined.iter().any(|row| row.contains("impedance")),
        "pair impedance should apply, not stay unsupported: {joined:?}"
    );
    Ok(())
}

#[test]
fn impedance_target_finding_clears_when_width_matches_stackup() -> Result<(), Error> {
    let mut s = Session::default();
    let p = s.execute("net.create", json!({"name":"USB_D+"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    let n = s.execute("net.create", json!({"name":"USB_D-"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    s.execute(
        "board.set_diffpair",
        json!({
            "name":"USB",
            "net_p":p,
            "net_n":n,
            "gap":0.4,
            "target_impedance":90.0,
            "impedance_tolerance":0.1
        }),
        None,
    )?;
    s.execute(
        "track.add",
        json!({"net":p,"layer":"F.Cu","width":2.0,"start":{"x":0,"y":0},"end":{"x":20,"y":0}}),
        None,
    )?;
    s.execute(
        "track.add",
        json!({"net":n,"layer":"F.Cu","width":2.0,"start":{"x":0,"y":3},"end":{"x":20,"y":3}}),
        None,
    )?;
    assert!(
        drc_codes(&mut s)?.contains(&"diffpair_impedance".to_string()),
        "wide traces should miss a 90 ohm target"
    );
    let id_p = s.project.board.tracks[0].id;
    let id_n = s.project.board.tracks[1].id;
    s.execute("track.set_width", json!({"id":id_p,"width":1.6}), None)?;
    s.execute("track.set_width", json!({"id":id_n,"width":1.6}), None)?;
    assert!(
        !drc_codes(&mut s)?.contains(&"diffpair_impedance".to_string()),
        "traces sized for the stackup should meet the target"
    );
    Ok(())
}

#[test]
fn pair_without_target_stays_silent() -> Result<(), Error> {
    let mut s = Session::default();
    let p = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    let n = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    s.execute(
        "board.set_diffpair",
        json!({"name":"P","net_p":p,"net_n":n,"gap":0.3}),
        None,
    )?;
    s.execute(
        "track.add",
        json!({"net":p,"layer":"F.Cu","width":2.0,"start":{"x":0,"y":0},"end":{"x":5,"y":0}}),
        None,
    )?;
    assert!(
        !drc_codes(&mut s)?
            .iter()
            .any(|c| c == "diffpair_impedance")
    );
    Ok(())
}

#[test]
fn impedance_fields_round_trip_through_save() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("net.create", json!({"name":"USB_D+"}), None)?;
    s.execute("net.create", json!({"name":"USB_D-"}), None)?;
    s.execute("rules.profile_apply", json!({"id":"usb2"}), None)?;
    let dir = tempfile::tempdir().map_err(|e| Error::Invalid(e.to_string()))?;
    let path = dir.path().join("imp.pcbcraft");
    pcbcraft_io::save(&s.project, &path).map_err(Error::Invalid)?;
    let loaded = pcbcraft_io::load(&path).map_err(Error::Invalid)?;
    assert_eq!(
        loaded.board.diff_pairs[0].target_impedance,
        Some(90.0)
    );
    Ok(())
}
