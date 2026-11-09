//! The YY1 job file for the engine's sample design, written through the same command
//! the window uses.
//!
//! Context: the sample uses through-hole template parts, so the machine lists none of
//! them; the file still carries the header, and the export says why.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs (`placement_yy1.export`, `machine.add`)
//! - crates/io/tests/fixtures/neoden/

use pcbcraft_engine::Session;
use serde_json::json;

#[test]
fn golden_sample_job() -> Result<(), String> {
    let mut s = Session::sample().map_err(|e| e.to_string())?;
    s.execute("machine.add", json!({"preset": "neoden-yy1"}), None)
        .map_err(|e| e.to_string())?;
    let dir = std::env::temp_dir().join(format!("pcbcraft-pnp-sample-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    s.execute(
        "placement_yy1.export",
        json!({"machine": "neoden-yy1", "directory": dir.to_string_lossy(), "side": "top", "layout": "native"}),
        None,
    )
    .map_err(|e| e.to_string())?;
    let fixtures =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../io/tests/fixtures/neoden");
    for name in ["Top.csv", "FeederSheet.csv"] {
        let got = std::fs::read_to_string(dir.join(name)).map_err(|e| e.to_string())?;
        let path = fixtures.join(format!("sample.{name}"));
        if std::env::var("PCBCRAFT_BLESS").is_ok() {
            std::fs::write(&path, &got).map_err(|e| e.to_string())?;
        }
        let want = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
        assert_eq!(got, want, "{name} differs from its golden file");
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A part with no suitable nozzle on the saved machine is a design check finding, and
/// so is a machine with fewer feeders than the board has part types.
#[test]
fn a_part_with_no_nozzle_shows_up_as_a_check_finding() -> Result<(), String> {
    let mut s = Session::default();
    let showcase = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/showcase.pcbcraft");
    s.project = pcbcraft_io::load(&showcase)?;
    s.execute("machine.add", json!({"preset": "neoden-yy1"}), None)
        .map_err(|e| e.to_string())?;
    let findings = pcbcraft_rules::drc(&s.project);
    assert!(
        findings.iter().all(|f| f.code != "placement_nozzle"),
        "the shipped rules give every showcase part a nozzle"
    );
    assert!(
        findings.iter().any(|f| f.code == "placement_nozzle_types"),
        "the showcase needs more nozzle types than the YY1 runs in one go"
    );
    let mut profile = s.project.machine_profiles[0].clone();
    profile.pick_place.placement_rule.clear();
    profile.pick_place.feeders.retain(|b| b.name != "tape");
    let value = serde_json::to_value(&profile).map_err(|e| e.to_string())?;
    s.execute("machine.set", json!({"profile": value}), None)
        .map_err(|e| e.to_string())?;
    let findings = pcbcraft_rules::drc(&s.project);
    assert!(
        findings
            .iter()
            .any(|f| f.code == "placement_nozzle" && f.message.contains("no nozzle")),
        "{findings:?}"
    );
    Ok(())
}

/// Writes the showcase with the YY1 saved in it to the path in `PCBCRAFT_DEMO_OUT`, so a
/// screenshot of the machine view has a design to open. Does nothing without the variable.
#[test]
fn writes_the_showcase_with_the_machine_for_screenshots() -> Result<(), String> {
    let Ok(out) = std::env::var("PCBCRAFT_DEMO_OUT") else {
        return Ok(());
    };
    let mut s = Session::default();
    let showcase = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/showcase.pcbcraft");
    s.project = pcbcraft_io::load(&showcase)?;
    s.execute("machine.add", json!({"preset": "neoden-yy1"}), None)
        .map_err(|e| e.to_string())?;
    pcbcraft_io::save(&s.project, std::path::Path::new(&out))
}
