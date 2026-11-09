//! Integration test entry for M6 artifact goldens (#301).

mod support;

use support::m6_artifact_goldens::{
    compare_golden_in, line_diff, run_reference_board_m6_artifact_goldens,
};

#[test]
fn golden_diff_shows_line_and_path() -> Result<(), String> {
    let diff = line_diff("one\ntwo\nthree\n", "one\nTWO\nthree\n");
    if !diff.contains("line 2") {
        return Err(format!("expected line number in diff, got: {diff}"));
    }
    if !diff.contains("golden: two") || !diff.contains("actual: TWO") {
        return Err(format!("expected line bodies in diff, got: {diff}"));
    }
    Ok(())
}

#[test]
fn reference_board_generates_all_m6_artifacts() -> Result<(), String> {
    run_reference_board_m6_artifact_goldens()
}

#[test]
fn golden_harness_fails_on_a_wrong_blessed_file() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    std::fs::write(dir.path().join("bom.golden"), "one\ntwo\n")
        .map_err(|e| format!("write golden: {e}"))?;
    let mut failures = Vec::new();
    let mut skip_notes = Vec::new();
    compare_golden_in(
        dir.path(),
        false,
        "bom",
        "one\nTWO\n",
        &mut failures,
        &mut skip_notes,
    )?;
    if failures.len() != 1 {
        return Err(format!("expected one failure, got: {failures:?}"));
    }
    if !failures[0].contains("tests/fixtures/artifacts/bom.golden differs") {
        return Err(format!("unexpected failure text: {}", failures[0]));
    }
    if !failures[0].contains("line 2") {
        return Err(format!("expected line diff in failure: {}", failures[0]));
    }
    Ok(())
}

#[test]
fn golden_harness_skips_when_blessed_file_is_missing() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let mut failures = Vec::new();
    let mut skip_notes = Vec::new();
    compare_golden_in(
        dir.path(),
        false,
        "bom",
        "csv\n",
        &mut failures,
        &mut skip_notes,
    )?;
    if !failures.is_empty() {
        return Err(format!("missing golden should not fail: {failures:?}"));
    }
    if skip_notes.len() != 1 || !skip_notes[0].contains("bom.golden not blessed yet") {
        return Err(format!("expected skip note, got: {skip_notes:?}"));
    }
    Ok(())
}

#[test]
fn golden_harness_writes_under_bless() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let mut failures = Vec::new();
    let mut skip_notes = Vec::new();
    compare_golden_in(
        dir.path(),
        true,
        "bom",
        "csv\n",
        &mut failures,
        &mut skip_notes,
    )?;
    if !failures.is_empty() {
        return Err(format!("bless should not fail: {failures:?}"));
    }
    let path = dir.path().join("bom.golden");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read blessed: {e}"))?;
    if text != "csv\n" {
        return Err(format!("blessed content mismatch: {text:?}"));
    }
    Ok(())
}
