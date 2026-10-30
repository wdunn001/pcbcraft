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

#[test]
fn every_built_in_profile_toml_parses() {
    for builtin in pcbcraft_engine::profiles::BUILT_INS {
        let parsed = toml::from_str::<toml::Value>(builtin.toml);
        assert!(
            parsed.is_ok(),
            "{} must parse: {:?}",
            builtin.id,
            parsed.err()
        );
        let doc = parsed.unwrap_or_else(|_| toml::Value::Table(toml::map::Map::new()));
        assert_eq!(doc["schema"].as_integer(), Some(1));
        assert_eq!(doc["profile"]["id"].as_str(), Some(builtin.id));
    }
}

#[test]
fn value_source_fields_name_known_schema_keys() {
    let known: std::collections::BTreeSet<_> =
        pcbcraft_engine::profiles::known_value_source_fields()
            .iter()
            .copied()
            .collect();
    for builtin in pcbcraft_engine::profiles::BUILT_INS {
        let parsed = toml::from_str::<toml::Value>(builtin.toml);
        assert!(parsed.is_ok(), "{}", builtin.id);
        let doc = parsed.unwrap_or_else(|_| toml::Value::Table(toml::map::Map::new()));
        if let Some(rows) = doc.get("value_source").and_then(|v| v.as_array()) {
            for row in rows {
                let field = row["field"].as_str().unwrap_or("");
                assert!(
                    known.contains(field),
                    "{} value_source field {field}",
                    builtin.id
                );
            }
        }
    }
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
fn profile_list_includes_usb2() -> Result<(), Error> {
    let mut s = Session::default();
    let out = s.execute("rules.profile_list", json!({}), None)?;
    let profiles = out["result"]["profiles"].as_array().ok_or_else(missing)?;
    assert!(profiles.iter().any(|p| p["id"] == "usb2"));
    Ok(())
}

#[test]
fn manifest_slugs_match_built_ins_and_manifest_jsonl() -> Result<(), String> {
    use pcbcraft_engine::profiles::{manifest_slugs, parse_builtin, BUILT_INS};
    use std::collections::BTreeSet;

    let slugs = manifest_slugs();
    assert_eq!(slugs.len(), BUILT_INS.len());
    for (slug, builtin) in slugs.iter().zip(BUILT_INS.iter()) {
        assert_eq!(*slug, builtin.id);
    }
    let mut seen = BTreeSet::new();
    for slug in slugs {
        if !seen.insert(*slug) {
            return Err(format!("duplicate slug {slug}"));
        }
        let body = parse_builtin(slug)?;
        if body.trim().is_empty() {
            return Err(format!("empty embedded profile for {slug}"));
        }
    }
    let manifest = include_str!("../profiles/manifest.jsonl");
    let manifest_slugs: BTreeSet<String> = manifest
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            if v["topic"].as_str()? != "netrules" {
                return None;
            }
            Some(v["slug"].as_str()?.to_string())
        })
        .collect();
    for slug in slugs {
        if !manifest_slugs.contains(*slug) {
            return Err(format!("manifest.jsonl missing {slug}"));
        }
    }
    Ok(())
}

#[test]
fn class_pattern_matches_glob_oracle() -> Result<(), String> {
    use pcbcraft_engine::profiles::class_pattern_matches;
    let cases: &[(&str, &str, bool)] = &[
        ("USB_*", "USB_D+", true),
        ("USB_*", "I2C_SDA", false),
        ("?", "A", true),
        ("?", "AB", false),
        ("N??", "N12", true),
        ("N??", "N123", false),
        ("*GND*", "VCC_GND", true),
        ("*", "", true),
        ("", "", true),
        ("exact", "exact", true),
        ("exact", "exactx", false),
    ];
    for (pattern, name, want) in cases {
        assert_eq!(
            class_pattern_matches(pattern, name),
            *want,
            "pattern {pattern:?} name {name:?}"
        );
    }
    Ok(())
}

#[test]
fn parse_builtin_returns_embedded_toml_or_unknown_error() -> Result<(), String> {
    use pcbcraft_engine::profiles::parse_builtin;
    let usb = parse_builtin("usb2")?;
    let doc: toml::Value = toml::from_str(usb).map_err(|e| e.to_string())?;
    assert_eq!(doc["schema"].as_integer(), Some(1));
    assert_eq!(doc["profile"]["id"].as_str(), Some("usb2"));
    match parse_builtin("not-a-profile") {
        Err(err) if err.contains("unknown profile") => Ok(()),
        Err(err) => Err(format!("unexpected error for unknown profile: {err}")),
        Ok(_) => Err("not-a-profile should not parse".into()),
    }
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
