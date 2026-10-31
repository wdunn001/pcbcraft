//! `cargo xtask a11y`: the accessibility gate.
//!
//! Context: runs the accessibility tests of `crates/ui-egui` headless (no GPU), then prints
//! the per-screen table the walk wrote to the job summary. Docs: `docs/accessibility.md`.
//! Local rules: this file only selects tests and formats their report; each rule lives in
//! the test that enforces it.
//!
//! # Dependencies
//! - crates/ui-egui/src/e2e/a11y_gate.rs writes `target/a11y/report.json`
//! - crates/ui-egui/src/contrast.rs, target_size.rs, e2e/shell.rs hold the other rules
//!
//! # Do not
//! - Add a rule here; add a test in ui-egui and list its filter in `FILTERS`.

use serde_json::Value;

/// Test-name filters that make up the gate.
pub const FILTERS: [&str; 8] = [
    "e2e::a11y_gate::",
    "every_interactive_control_has_an_accessible_name",
    "the_exemption_file_has_no_entry_for_a_control_that_is_now_named",
    "each_exemption_names_an_issue_number",
    "a_newcomer_never_meets_a_control_too_small_to_hit",
    "contrast::tests::",
    "target_size::tests::",
    "motion::tests::",
];

/// The per-screen table, as Markdown, from the walk's `report.json` text.
pub fn table(report: &str) -> Result<String, String> {
    let rows: Vec<Value> = serde_json::from_str(report).map_err(|e| e.to_string())?;
    let mut out = String::from(
        "| Screen | Controls | No name | Same name twice | Under 24 px | Skipped by Tab | Tab trap |\n\
         | --- | ---: | ---: | ---: | ---: | ---: | --- |\n",
    );
    let n = |row: &Value, key: &str| row.get(key).and_then(Value::as_u64).unwrap_or(0);
    for row in &rows {
        let screen = row.get("screen").and_then(Value::as_str).unwrap_or("?");
        let trap = if row.get("trapped").and_then(Value::as_bool) == Some(true) {
            "yes"
        } else {
            "no"
        };
        out.push_str(&format!(
            "| {screen} | {} | {} | {} | {} | {} | {trap} |\n",
            n(row, "controls"),
            n(row, "unnamed"),
            n(row, "duplicate_names"),
            n(row, "undersized"),
            n(row, "unreached"),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_lists_each_screen() -> Result<(), String> {
        let report = r#"[{"screen":"Pcb","controls":40,"unnamed":0,"duplicate_names":2,
            "undersized":1,"unreached":3,"trapped":true}]"#;
        let t = table(report)?;
        assert!(t.contains("| Pcb | 40 | 0 | 2 | 1 | 3 | yes |"), "{t}");
        assert!(table("not json").is_err());
        Ok(())
    }
}
