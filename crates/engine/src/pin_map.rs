//! Engine commands for pin map export and reverse check.
//!
//! Provides `pinmap.export` to render the design's pin map as CSV or JSON
//! and `pinmap.check` to compare an imported pin map against the design.
//! Both commands are read-only and never touch revision or undo state.
//!
//! # Rules
//! - Export format is `csv` (default) or `json`.
//! - Check input text must be 1 to 1048576 bytes.
//! - Errors map to `Error::Invalid`; the project is never mutated.
//!
//! # Dependencies
//! - crates/plugin/src/pin_map_detail.rs (`pin_map_detail`)
//! - crates/plugin/src/pin_map_export.rs (`render_pin_map_csv`, `render_pin_map_json`)
//! - crates/plugin/src/pin_map_check.rs (`check_pin_map_text`)
//! - crates/doc/src/lib.rs (`Project`)
//!
//! # Do not
//! - Mutate the project or its revision.
//! - Guess a driver for an unmapped part.

use pcbcraft_doc::Project;
use serde_json::{Value, json};

const MAX_TEXT_BYTES: usize = 1_048_576;

fn bounded(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.is_empty() || value.len() > max {
        return Err(format!("{label} must be 1 to {max} bytes"));
    }
    Ok(())
}

/// Export the pin map as CSV or JSON text.
///
/// Reads the firmware design record and renders it to a generic pin map
/// file. The `format` parameter controls the output: `csv` (default) or
/// `json`. Returns the text, format, row count and unmapped part names.
///
/// # Rules
/// - Refuses when the firmware design is missing.
/// - JSON output is pretty-printed.
///
/// # Dependencies
/// - crates/plugin/src/pin_map_detail.rs
/// - crates/plugin/src/pin_map_export.rs
///
/// # Do not
/// - Mutate the project.
pub fn pin_map_export(p: &Project, params: &Value) -> Result<Value, String> {
    let format = params
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("csv");
    if format != "csv" && format != "json" {
        return Err("format must be csv or json".to_string());
    }
    let detail = pcbcraft_plugin::pin_map_detail::pin_map_detail(p)?;
    let rows = detail.rows.len();
    let unmapped = detail.unmapped.clone();
    let text = if format == "json" {
        pcbcraft_plugin::pin_map_export::render_pin_map_json(&detail)?
    } else {
        pcbcraft_plugin::pin_map_export::render_pin_map_csv(&detail)
    };
    Ok(json!({
        "format": format,
        "text": text,
        "rows": rows,
        "unmapped": unmapped,
    }))
}

/// Check an imported pin map against the design.
///
/// Parses the imported CSV pin map and compares it to the design's pin
/// assignments. Returns a list of mismatches, each naming the pin, the
/// expected function and the function found. Swapped pins are detected.
///
/// # Rules
/// - Text must be 1 to 1048576 bytes.
/// - Lines starting with `#` are skipped.
///
/// # Dependencies
/// - crates/plugin/src/pin_map_detail.rs
/// - crates/plugin/src/pin_map_check.rs
///
/// # Do not
/// - Mutate the project.
pub fn pin_map_check(p: &Project, params: &Value) -> Result<Value, String> {
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing text".to_string())?;
    bounded("text", text, MAX_TEXT_BYTES)?;
    let detail = pcbcraft_plugin::pin_map_detail::pin_map_detail(p)?;
    let mismatches = pcbcraft_plugin::pin_map_check::check_pin_map_text(&detail, text)?;
    let rows: Vec<Value> = mismatches
        .iter()
        .map(|m| {
            let mut obj = json!({
                "pin": m.pin,
                "expected": m.expected,
                "found": m.found,
            });
            if let Some(swapped) = &m.swapped_with {
                obj["swapped_with"] = json!(swapped);
            }
            obj
        })
        .collect();
    Ok(json!({
        "mismatches": rows,
        "count": rows.len(),
    }))
}
