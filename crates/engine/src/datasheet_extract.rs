//! Engine commands for datasheet table extraction and review (issue #711).
//!
//! Extract takes a TextDocument payload, extracts pin, rating, ordering and
//! dimension tables, and stores the reviewable bundle as a JSON attachment.
//! Review applies accept, edit or reject decisions to individual rows.
//!
//! # Dependencies
//! - crates/library/src/datasheet_extract/

use super::*;
use pcbcraft_library::datasheet_extract::{
    DatasheetExtractBundle, ExtractedOrderingCode, ExtractedPackageDimension, ExtractedPinRow,
    ExtractedRatingRow, ReviewDecision, TextDocument, bounded_field_text,
    extract_from_text_document, set_decision,
};

/// Register the datasheet.extract and datasheet.review command specs.
///
/// # Rules
/// - part names are bounded to 1-64 bytes of [A-Za-z0-9_-]
/// - the document payload must deserialize as a TextDocument
/// - review index is bounded to 0..255
///
/// # Dependencies
/// - crates/engine/src/datasheet_extract.rs
pub(super) fn commands() -> Vec<CommandSpec> {
    let document_schema = json!({
        "type": "object",
        "properties": {
            "pages": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "page_index": { "type": "integer", "minimum": 0 },
                        "page_label": { "type": "integer", "minimum": 1 },
                        "cells": { "type": "array", "items": { "type": "object" } }
                    },
                    "required": ["page_index", "page_label", "cells"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["pages"],
        "additionalProperties": false
    });
    let review_schema = json!({
        "type": "object",
        "properties": {
            "part": { "type": "string", "minLength": 1, "maxLength": 64, "pattern": "^[A-Za-z0-9_-]+$" },
            "table": { "type": "string", "enum": ["pin_table", "absolute_maximum_ratings", "recommended_operating_conditions", "ordering_codes", "package_dimensions"] },
            "index": { "type": "integer", "minimum": 0, "maximum": 255 },
            "decision": { "type": "string", "enum": ["accept", "reject", "edit"] },
            "edited": { "type": "object" }
        },
        "required": ["part", "table", "index", "decision"],
        "additionalProperties": false
    });
    vec![
        CommandSpec {
            id: "datasheet.extract",
            description: "Extract pin, rating, ordering and dimension tables from a provided text document and store the reviewable bundle as a project attachment",
            input_schema: schema(
                json!({
                    "part": { "type": "string", "minLength": 1, "maxLength": 64, "pattern": "^[A-Za-z0-9_-]+$" },
                    "document": document_schema
                }),
                &["part", "document"],
            ),
        },
        CommandSpec {
            id: "datasheet.review",
            description: "Apply an accept, edit or reject decision to one row in a previously extracted datasheet bundle",
            input_schema: schema(review_schema, &["part", "table", "index", "decision"]),
        },
    ]
}

/// Handle the datasheet.extract and datasheet.review commands on the design project.
///
/// # Rules
/// - runs the library extractor and stores the JSON bundle as an attachment
/// - refuses bundles over 256 KiB and projects with more than 32 files
/// - replaces the attachment when the same part is extracted again
/// - review commands load, mutate and write back the bundle atomically
///
/// # Dependencies
/// - crates/library/src/datasheet_extract/parse.rs
/// - crates/engine/src/lib.rs
pub(super) fn run_design(
    p: &mut Project,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    if command == "datasheet.extract" {
        return Some(extract_and_store(p, params));
    }
    if command == "datasheet.review" {
        return Some(review_row(p, params));
    }
    None
}

fn extract_and_store(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: ExtractParams = parse(params)?;
    if !is_valid_part_name(&a.part) {
        return Err(Error::Invalid(
            "part name must be 1 to 64 bytes of [A-Za-z0-9_-]".into(),
        ));
    }
    let bundle = extract_from_text_document(&a.document).map_err(Error::Invalid)?;
    let json = serde_json::to_string(&bundle).map_err(|e| Error::Invalid(e.to_string()))?;
    if json.len() > 262_144 {
        return Err(Error::Invalid(
            "the extract bundle exceeds the 256 KiB attachment limit".into(),
        ));
    }
    let name = format!("datasheet-extract-{}.json", a.part);
    store_attachment(p, &name, &json)?;
    Ok(json!({
        "part": a.part,
        "pins": bundle.pin_table.len(),
        "ratings": bundle.absolute_maximum_ratings.len() + bundle.recommended_operating_conditions.len(),
        "ordering": bundle.ordering_codes.len(),
        "dimensions": bundle.package_dimensions.len()
    }))
}

fn store_attachment(p: &mut Project, name: &str, json: &str) -> Result<(), Error> {
    let files = &mut p.files;
    if let Some(existing) = files.iter_mut().find(|f| f.name == name) {
        existing.data = json.to_owned();
    } else {
        if files.len() >= 32 {
            return Err(Error::Invalid("a project attaches at most 32 files".into()));
        }
        files.push(pcbcraft_doc::EmbeddedFile {
            name: name.to_owned(),
            data: json.to_owned(),
            encoding: String::new(),
        });
    }
    Ok(())
}

fn load_bundle(p: &Project, part: &str) -> Result<DatasheetExtractBundle, Error> {
    let name = format!("datasheet-extract-{}.json", part);
    let file = p
        .files
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| Error::Invalid("no extraction for this part".to_string()))?;
    let bundle: DatasheetExtractBundle = serde_json::from_str(&file.data)
        .map_err(|e| Error::Invalid(format!("bundle decode failed: {}", e)))?;
    Ok(bundle)
}

fn review_row(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: ReviewParams = parse(params)?;
    if !is_valid_part_name(&a.part) {
        return Err(Error::Invalid(
            "part name must be 1 to 64 bytes of [A-Za-z0-9_-]".into(),
        ));
    }
    if !is_valid_table_name(&a.table) {
        return Err(Error::Invalid("unknown table name".into()));
    }
    if a.index > 255 {
        return Err(Error::Invalid("index must be 0 to 255".into()));
    }

    let mut bundle = load_bundle(p, &a.part)?;
    let decision = match a.decision.as_str() {
        "accept" => ReviewDecision::Accepted,
        "reject" => ReviewDecision::Rejected,
        "edit" => ReviewDecision::Edited,
        _ => return Err(Error::Invalid("unknown decision".into())),
    };

    apply_review(&mut bundle, &a.table, a.index, decision, a.edited.as_ref())?;

    let json = serde_json::to_string(&bundle)
        .map_err(|e| Error::Invalid(format!("bundle encode failed: {}", e)))?;
    if json.len() > 262_144 {
        return Err(Error::Invalid(
            "the extract bundle exceeds the 256 KiB attachment limit".into(),
        ));
    }
    let name = format!("datasheet-extract-{}.json", a.part);
    store_attachment(p, &name, &json)?;

    Ok(json!({
        "part": a.part,
        "table": a.table,
        "index": a.index,
        "decision": a.decision
    }))
}

fn apply_review(
    bundle: &mut DatasheetExtractBundle,
    table: &str,
    index: usize,
    decision: ReviewDecision,
    edited: Option<&Value>,
) -> Result<(), Error> {
    match table {
        "pin_table" => {
            let row = bundle
                .pin_table
                .get_mut(index)
                .ok_or_else(|| Error::Invalid(format!("pin_table index {} out of range", index)))?;
            if decision == ReviewDecision::Edited {
                let mut pin = deserialize_edited_pin(edited)?;
                pin.source = row.value.source.clone();
                row.edited = Some(pin);
            }
            set_decision(row, decision).map_err(Error::Invalid)?;
        }
        "absolute_maximum_ratings" | "recommended_operating_conditions" => {
            let rows = if table == "absolute_maximum_ratings" {
                &mut bundle.absolute_maximum_ratings
            } else {
                &mut bundle.recommended_operating_conditions
            };
            let row = rows
                .get_mut(index)
                .ok_or_else(|| Error::Invalid(format!("{} index {} out of range", table, index)))?;
            if decision == ReviewDecision::Edited {
                let mut rating = deserialize_edited_rating(edited)?;
                rating.source = row.value.source.clone();
                row.edited = Some(rating);
            }
            set_decision(row, decision).map_err(Error::Invalid)?;
        }
        "ordering_codes" => {
            let row = bundle.ordering_codes.get_mut(index).ok_or_else(|| {
                Error::Invalid(format!("ordering_codes index {} out of range", index))
            })?;
            if decision == ReviewDecision::Edited {
                let mut code = deserialize_edited_ordering(edited)?;
                code.source = row.value.source.clone();
                row.edited = Some(code);
            }
            set_decision(row, decision).map_err(Error::Invalid)?;
        }
        "package_dimensions" => {
            let row = bundle.package_dimensions.get_mut(index).ok_or_else(|| {
                Error::Invalid(format!("package_dimensions index {} out of range", index))
            })?;
            if decision == ReviewDecision::Edited {
                let mut dim = deserialize_edited_dimension(edited)?;
                dim.source = row.value.source.clone();
                row.edited = Some(dim);
            }
            set_decision(row, decision).map_err(Error::Invalid)?;
        }
        _ => return Err(Error::Invalid("unknown table name".into())),
    }
    Ok(())
}

fn deserialize_edited_pin(edited: Option<&Value>) -> Result<ExtractedPinRow, Error> {
    let v = edited
        .ok_or_else(|| Error::Invalid("edited value required for edit decision".to_string()))?;
    let row: ExtractedPinRow = serde_json::from_value(v.clone())
        .map_err(|e| Error::Invalid(format!("pin edit decode failed: {}", e)))?;
    bounded_field_text(&row.pin_number)?;
    bounded_field_text(&row.pin_name)?;
    Ok(row)
}

fn deserialize_edited_rating(edited: Option<&Value>) -> Result<ExtractedRatingRow, Error> {
    let v = edited
        .ok_or_else(|| Error::Invalid("edited value required for edit decision".to_string()))?;
    let row: ExtractedRatingRow = serde_json::from_value(v.clone())
        .map_err(|e| Error::Invalid(format!("rating edit decode failed: {}", e)))?;
    bounded_field_text(&row.parameter)?;
    bounded_field_text(&row.value)?;
    bounded_field_text(&row.unit)?;
    Ok(row)
}

fn deserialize_edited_ordering(edited: Option<&Value>) -> Result<ExtractedOrderingCode, Error> {
    let v = edited
        .ok_or_else(|| Error::Invalid("edited value required for edit decision".to_string()))?;
    let row: ExtractedOrderingCode = serde_json::from_value(v.clone())
        .map_err(|e| Error::Invalid(format!("ordering edit decode failed: {}", e)))?;
    bounded_field_text(&row.code)?;
    bounded_field_text(&row.description)?;
    Ok(row)
}

fn deserialize_edited_dimension(
    edited: Option<&Value>,
) -> Result<ExtractedPackageDimension, Error> {
    let v = edited
        .ok_or_else(|| Error::Invalid("edited value required for edit decision".to_string()))?;
    let row: ExtractedPackageDimension = serde_json::from_value(v.clone())
        .map_err(|e| Error::Invalid(format!("dimension edit decode failed: {}", e)))?;
    bounded_field_text(&row.name)?;
    bounded_field_text(&row.value)?;
    bounded_field_text(&row.unit)?;
    Ok(row)
}

fn is_valid_part_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    bytes
        .iter()
        .all(|b| matches!(*b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
}

fn is_valid_table_name(table: &str) -> bool {
    matches!(
        table,
        "pin_table"
            | "absolute_maximum_ratings"
            | "recommended_operating_conditions"
            | "ordering_codes"
            | "package_dimensions"
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtractParams {
    part: String,
    document: TextDocument,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewParams {
    part: String,
    table: String,
    index: usize,
    decision: String,
    edited: Option<Value>,
}
