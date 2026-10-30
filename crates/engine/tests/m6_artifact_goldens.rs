//! Integration test entry for M6 artifact goldens (#301).

mod support;

use support::m6_artifact_goldens::{line_diff, run_reference_board_m6_artifact_goldens};

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
