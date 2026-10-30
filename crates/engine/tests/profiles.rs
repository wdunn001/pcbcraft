use pcbcraft_engine::{Command, Error, Session};
use serde_json::json;
use std::path::PathBuf;

fn missing() -> Error {
    Error::Invalid("missing field".into())
}

#[test]
fn applying_usb2_creates_the_pair_and_class_in_one_undo_step() -> Result<(), Error> {
    let mut s = Session::default();
    let dp = s.execute("net.create", json!({"name":"USB_D+"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    let dn = s.execute("net.create", json!({"name":"USB_D-"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    let _ = s.execute("net.create", json!({"name":"GND"}), None)?;
    let _rev_before = s.revision;
    s.execute("rules.profile_apply", json!({"id":"usb2"}), None)?;
    assert_eq!(s.project.net_classes.len(), 1);
    assert_eq!(s.project.board.diff_pairs.len(), 1);
    assert_eq!(s.project.board.pair_rules.len(), 1);
    assert!(
        s.project.net_classes[0]
            .profile
            .as_deref()
            .is_some_and(|p| p == "usb2")
    );
    assert!(s.project.board.diff_pairs[0].profile.as_deref() == Some("usb2"));
    assert!(s.project.net_classes[0].nets.contains(&dp));
    assert!(s.project.net_classes[0].nets.contains(&dn));
    s.undo()?;
    assert!(s.project.net_classes.is_empty());
    assert!(s.project.board.diff_pairs.is_empty());
    Ok(())
}

#[test]
fn applying_i2c_puts_sda_and_scl_in_one_class_in_one_undo_step() -> Result<(), Error> {
    let mut s = Session::default();
    let sda = s.execute("net.create", json!({"name":"SDA"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    let scl = s.execute("net.create", json!({"name":"SCL"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    s.execute("rules.profile_apply", json!({"id":"i2c"}), None)?;
    assert_eq!(s.project.net_classes.len(), 1);
    assert_eq!(s.project.net_classes[0].name, "I2C");
    let members = &s.project.net_classes[0].nets;
    assert!(members.contains(&sda) && members.contains(&scl));
    Ok(())
}

#[test]
fn dry_run_changes_nothing() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("net.create", json!({"name":"USB_D+"}), None)?;
    s.execute("net.create", json!({"name":"USB_D-"}), None)?;
    let _rev = s.revision;
    let out = s.execute(
        "rules.profile_apply",
        json!({"id":"usb2","dry_run":true}),
        None,
    )?;
    assert_eq!(out["result"]["dry_run"], true);
    assert!(s.project.net_classes.is_empty());
    assert_eq!(s.project.board.diff_pairs.len(), 0);
    Ok(())
}

#[test]
fn unsupported_fields_are_reported_not_dropped() -> Result<(), Error> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unsupported_profile.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| Error::Invalid(e.to_string()))?;
    let doc: toml::Value = toml::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
    assert!(doc.get("length_group").is_some());
    let mut s = Session::default();
    s.execute("net.create", json!({"name":"A"}), None)?;
    s.execute("net.create", json!({"name":"B"}), None)?;
    let out = s.execute(
        "rules.profile_apply",
        json!({"path": path.to_string_lossy(),"dry_run":true}),
        None,
    )?;
    let unsupported = out["result"]["plan"]["unsupported"]
        .as_array()
        .ok_or_else(missing)?;
    assert!(!unsupported.is_empty());
    let joined: Vec<String> = unsupported
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    assert!(
        joined
            .iter()
            .any(|s| s.contains("length_group") || s == "length_group")
    );
    assert!(joined.iter().any(|s| s.contains("companion")));
    Ok(())
}

#[test]
fn profile_provenance_survives_save_and_load() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("net.create", json!({"name":"SDA"}), None)?;
    s.execute("net.create", json!({"name":"SCL"}), None)?;
    s.execute("rules.profile_apply", json!({"id":"i2c"}), None)?;
    let dir = tempfile::tempdir().map_err(|e| Error::Invalid(e.to_string()))?;
    let path = dir.path().join("t.pcbcraft");
    pcbcraft_io::save(&s.project, &path).map_err(Error::Invalid)?;
    let loaded = pcbcraft_io::load(&path).map_err(Error::Invalid)?;
    assert_eq!(
        loaded.net_classes[0].profile,
        s.project.net_classes[0].profile
    );
    Ok(())
}

#[test]
fn applying_twice_updates_values_without_duplicating_classes() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("net.create", json!({"name":"SDA"}), None)?;
    s.execute("net.create", json!({"name":"SCL"}), None)?;
    s.execute("rules.profile_apply", json!({"id":"i2c"}), None)?;
    s.execute("rules.profile_apply", json!({"id":"i2c"}), None)?;
    assert_eq!(s.project.net_classes.len(), 1);
    Ok(())
}

const SCHEMA_V2_BUILTINS: &[&str] = &["usb2", "i2c", "spi", "qspi", "uart", "can"];

#[test]
fn every_built_in_profile_toml_parses_and_validates() {
    for builtin in pcbcraft_engine::profiles::BUILT_INS {
        let doc = pcbcraft_io::interface_profile_from_toml(builtin.toml);
        assert!(doc.is_ok(), "{} must validate: {:?}", builtin.id, doc.err());
        let doc = doc.expect("validated above");
        assert_eq!(doc.profile.id, builtin.id);
        let schema = doc.schema;
        assert!(
            schema == 1 || schema == 2,
            "{} schema must be 1 or 2",
            builtin.id
        );
        if SCHEMA_V2_BUILTINS.contains(&builtin.id) {
            assert_eq!(schema, 2, "{} must be schema 2", builtin.id);
        }
    }
}

#[test]
fn schema_v2_builtins_round_trip_through_toml() -> Result<(), pcbcraft_engine::Error> {
    for id in SCHEMA_V2_BUILTINS {
        let text = pcbcraft_engine::profiles::parse_builtin(id)
            .map_err(|e| pcbcraft_engine::Error::Invalid(e))?;
        let doc = pcbcraft_io::interface_profile_from_toml(text)
            .map_err(pcbcraft_engine::Error::Invalid)?;
        let exported = pcbcraft_io::interface_profile_to_toml(&doc)
            .map_err(pcbcraft_engine::Error::Invalid)?;
        let back = pcbcraft_io::interface_profile_from_toml(&exported)
            .map_err(pcbcraft_engine::Error::Invalid)?;
        if back != doc {
            return Err(pcbcraft_engine::Error::Invalid(format!(
                "{id} toml round trip changed the profile"
            )));
        }
    }
    Ok(())
}

#[test]
fn value_source_fields_name_known_schema_keys() {
    let known: std::collections::BTreeSet<_> =
        pcbcraft_engine::profiles::known_value_source_fields()
            .iter()
            .copied()
            .collect();
    for builtin in pcbcraft_engine::profiles::BUILT_INS {
        let doc = pcbcraft_io::interface_profile_from_toml(builtin.toml);
        assert!(doc.is_ok(), "{}", builtin.id);
        let doc = doc.expect("validated above");
        for row in &doc.value_source {
            assert!(
                known.contains(row.field.as_str()),
                "{} value_source field {}",
                builtin.id,
                row.field
            );
        }
    }
}

#[test]
fn unknown_profile_field_is_refused_with_the_field_name() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/unknown_field_profile.toml");
    let err = pcbcraft_io::interface_profile_from_toml(
        &std::fs::read_to_string(&path).unwrap_or_default(),
    )
    .unwrap_err();
    assert!(
        err.contains("not_a_real_table") || err.contains("unknown field"),
        "expected field name in refusal, got {err}"
    );
}

#[test]
fn out_of_range_impedance_is_refused() {
    let text = r#"
schema = 2
[profile]
id = "bad-z"
name = "Bad Z"
category = "test"
summary = "test"
[bus]
name = "T"
schematic_type = "T"
[[signal]]
name = "A"
net = "A"
electrical_class = "single-ended"
[bus.limits]
impedance_ohm = 5.0
"#;
    let err = pcbcraft_io::interface_profile_from_toml(text).unwrap_err();
    assert!(err.contains("impedance_ohm"), "got {err}");
}

#[test]
fn a_newcomer_picks_usb_2_0_and_gets_the_net_class_and_required_parts() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("net.create", json!({"name":"USB_D+"}), None)?;
    s.execute("net.create", json!({"name":"USB_D-"}), None)?;
    let preview = s.execute("rules.profile_read", json!({"id":"usb2"}), None)?;
    assert_eq!(preview["result"]["bus"]["name"], "USB 2.0");
    let unsupported = preview["result"]["unsupported"]
        .as_array()
        .ok_or_else(missing)?;
    assert!(unsupported.iter().any(|v| v.as_str() == Some("companion")));
    let out = s.execute("rules.profile_apply", json!({"id":"usb2"}), None)?;
    assert_eq!(s.project.net_classes.len(), 1);
    assert_eq!(s.project.board.diff_pairs.len(), 1);
    let companions = out["result"]["plan"]["companions"]
        .as_array()
        .ok_or_else(missing)?;
    assert!(!companions.is_empty());
    Ok(())
}

#[test]
fn profile_read_reports_bus_template_name() -> Result<(), Error> {
    let mut s = Session::default();
    let out = s.execute("rules.profile_read", json!({"id":"i2c"}), None)?;
    assert_eq!(out["result"]["bus"]["name"], "I2C");
    let unsupported = out["result"]["unsupported"].as_array().ok_or_else(missing)?;
    assert!(!unsupported.is_empty());
    Ok(())
}

#[test]
fn manifest_source_slugs_exist() {
    let manifest = include_str!("../profiles/manifest.jsonl");
    let slugs: std::collections::BTreeSet<String> = manifest
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|v| v["slug"].as_str().map(str::to_string))
        })
        .collect();
    for builtin in pcbcraft_engine::profiles::BUILT_INS {
        assert!(
            slugs.contains(builtin.id),
            "manifest missing slug {}",
            builtin.id
        );
    }
}

#[test]
fn schema_v1_built_in_still_loads() {
    let text = pcbcraft_engine::profiles::parse_builtin("rs485").expect("rs485");
    let doc = pcbcraft_io::interface_profile_from_toml(text).expect("rs485 v1");
    assert_eq!(doc.schema, 1);
}

#[test]
fn profile_list_includes_usb2() -> Result<(), Error> {
    let mut s = Session::default();
    let out = s.execute("rules.profile_list", json!({}), None)?;
    let profiles = out["result"]["profiles"].as_array().ok_or_else(missing)?;
    assert!(profiles.iter().any(|p| p["id"] == "usb2"));
    Ok(())
}

#[test]
fn batch_profile_apply_is_one_undo_step() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("net.create", json!({"name":"SDA"}), None)?;
    s.execute("net.create", json!({"name":"SCL"}), None)?;
    let _rev = s.revision;
    s.batch(
        &[Command {
            command: "rules.profile_apply".into(),
            params: json!({"id":"i2c"}),
        }],
        None,
    )?;
    assert!(!s.project.net_classes.is_empty());
    s.undo()?;
    assert!(s.project.net_classes.is_empty());
    Ok(())
}
