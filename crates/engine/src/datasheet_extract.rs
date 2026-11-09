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
    ExtractedRatingRow, ReviewDecision, TextDocument, apply_patch_to_symbol, bounded_field_text,
    build_part_patch, extract_from_text_document, set_decision,
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
    let review_properties = json!({
        "part": { "type": "string", "minLength": 1, "maxLength": 64, "pattern": "^[A-Za-z0-9_-]+$" },
            "table": { "type": "string", "enum": ["pin_table", "absolute_maximum_ratings", "recommended_operating_conditions", "ordering_codes", "package_dimensions"] },
            "index": { "type": "integer", "minimum": 0, "maximum": 255 },
            "decision": { "type": "string", "enum": ["accept", "reject", "edit"] },
            "edited": { "type": "object" }
    });
    let apply_properties = json!({
        "part": { "type": "string", "minLength": 1, "maxLength": 64, "pattern": "^[A-Za-z0-9_-]+$" },
        "symbol": { "type": "string", "minLength": 1, "maxLength": 128 }
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
            input_schema: schema(review_properties, &["part", "table", "index", "decision"]),
        },
        CommandSpec {
            id: "datasheet.apply_review",
            description: "Apply accepted and edited datasheet values to a library symbol, renaming pins and setting the ordering code property",
            input_schema: schema(apply_properties, &["part", "symbol"]),
        },
        CommandSpec {
            id: "datasheet.review_list",
            description: "List all extracted datasheet values for a part with their review decisions and provenance",
            input_schema: schema(
                json!({
                    "part": { "type": "string", "minLength": 1, "maxLength": 64, "pattern": "^[A-Za-z0-9_-]+$" }
                }),
                &["part"],
            ),
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
    if command == "datasheet.apply_review" {
        return Some(apply_review_command(p, params));
    }
    if command == "datasheet.review_list" {
        return Some(review_list(p, params));
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

/// Apply the reviewed datasheet bundle to a library symbol.
///
/// # Rules
/// - loads the bundle for the given part name
/// - builds a patch from accepted and edited values only
/// - finds the symbol in the project library by name
/// - applies the patch and returns the count of renamed pins
///
/// # Dependencies
/// - crates/library/src/datasheet_extract/apply.rs
fn review_list(p: &Project, params: &Value) -> Result<Value, Error> {
    let a: ListParams = parse(params)?;
    if !is_valid_part_name(&a.part) {
        return Err(Error::Invalid(
            "part name must be 1 to 64 bytes of [A-Za-z0-9_-]".into(),
        ));
    }
    let bundle = load_bundle(p, &a.part)?;
    let mut rows = Vec::new();
    for (i, row) in bundle.pin_table.iter().enumerate() {
        let number = if row.decision == ReviewDecision::Edited {
            row.edited
                .as_ref()
                .map(|e| e.pin_number.clone())
                .unwrap_or(row.value.pin_number.clone())
        } else {
            row.value.pin_number.clone()
        };
        let name = if row.decision == ReviewDecision::Edited {
            row.edited
                .as_ref()
                .map(|e| e.pin_name.clone())
                .unwrap_or(row.value.pin_name.clone())
        } else {
            row.value.pin_name.clone()
        };
        rows.push(json!({
            "table": "pin_table",
            "index": i,
            "label": format!("Pin {} {}", number, name),
            "page_index": row.value.source.page_index,
            "page_label": row.value.source.page_label,
            "rect": [
                row.value.source.rect.x0,
                row.value.source.rect.y0,
                row.value.source.rect.x1,
                row.value.source.rect.y1
            ],
            "decision": serde_json::to_value(row.decision).map_err(|e| Error::Invalid(e.to_string()))?
        }));
    }
    for table in [
        ("absolute_maximum_ratings", &bundle.absolute_maximum_ratings),
        (
            "recommended_operating_conditions",
            &bundle.recommended_operating_conditions,
        ),
    ] {
        for (i, row) in table.1.iter().enumerate() {
            let parameter = if row.decision == ReviewDecision::Edited {
                row.edited
                    .as_ref()
                    .map(|e| e.parameter.clone())
                    .unwrap_or(row.value.parameter.clone())
            } else {
                row.value.parameter.clone()
            };
            let value = if row.decision == ReviewDecision::Edited {
                row.edited
                    .as_ref()
                    .map(|e| e.value.clone())
                    .unwrap_or(row.value.value.clone())
            } else {
                row.value.value.clone()
            };
            let unit = if row.decision == ReviewDecision::Edited {
                row.edited
                    .as_ref()
                    .map(|e| e.unit.clone())
                    .unwrap_or(row.value.unit.clone())
            } else {
                row.value.unit.clone()
            };
            rows.push(json!({
                "table": table.0,
                "index": i,
                "label": format!("{} {} {}", parameter, value, unit),
                "page_index": row.value.source.page_index,
                "page_label": row.value.source.page_label,
                "rect": [
                    row.value.source.rect.x0,
                    row.value.source.rect.y0,
                    row.value.source.rect.x1,
                    row.value.source.rect.y1
                ],
                "decision": serde_json::to_value(row.decision).map_err(|e| Error::Invalid(e.to_string()))?
            }));
        }
    }
    for (i, row) in bundle.ordering_codes.iter().enumerate() {
        let code = if row.decision == ReviewDecision::Edited {
            row.edited
                .as_ref()
                .map(|e| e.code.clone())
                .unwrap_or(row.value.code.clone())
        } else {
            row.value.code.clone()
        };
        rows.push(json!({
            "table": "ordering_codes",
            "index": i,
            "label": code,
            "page_index": row.value.source.page_index,
            "page_label": row.value.source.page_label,
            "rect": [
                row.value.source.rect.x0,
                row.value.source.rect.y0,
                row.value.source.rect.x1,
                row.value.source.rect.y1
            ],
            "decision": serde_json::to_value(row.decision).map_err(|e| Error::Invalid(e.to_string()))?
        }));
    }
    for (i, row) in bundle.package_dimensions.iter().enumerate() {
        let name = if row.decision == ReviewDecision::Edited {
            row.edited
                .as_ref()
                .map(|e| e.name.clone())
                .unwrap_or(row.value.name.clone())
        } else {
            row.value.name.clone()
        };
        let value = if row.decision == ReviewDecision::Edited {
            row.edited
                .as_ref()
                .map(|e| e.value.clone())
                .unwrap_or(row.value.value.clone())
        } else {
            row.value.value.clone()
        };
        let unit = if row.decision == ReviewDecision::Edited {
            row.edited
                .as_ref()
                .map(|e| e.unit.clone())
                .unwrap_or(row.value.unit.clone())
        } else {
            row.value.unit.clone()
        };
        rows.push(json!({
            "table": "package_dimensions",
            "index": i,
            "label": format!("{} {} {}", name, value, unit),
            "page_index": row.value.source.page_index,
            "page_label": row.value.source.page_label,
            "rect": [
                row.value.source.rect.x0,
                row.value.source.rect.y0,
                row.value.source.rect.x1,
                row.value.source.rect.y1
            ],
            "decision": serde_json::to_value(row.decision).map_err(|e| Error::Invalid(e.to_string()))?
        }));
    }
    Ok(json!({ "part": a.part, "rows": rows }))
}
fn apply_review_command(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: ApplyParams = parse(params)?;
    if !is_valid_part_name(&a.part) {
        return Err(Error::Invalid(
            "part name must be 1 to 64 bytes of [A-Za-z0-9_-]".into(),
        ));
    }
    if a.symbol.is_empty() || a.symbol.len() > 128 {
        return Err(Error::Invalid("symbol name must be 1 to 128 bytes".into()));
    }

    let bundle = load_bundle(p, &a.part)?;
    let patch = build_part_patch(&bundle).map_err(Error::Invalid)?;

    let symbol = p
        .library
        .symbols
        .iter_mut()
        .find(|s| s.name == a.symbol)
        .ok_or_else(|| Error::Invalid(format!("no such library symbol {}", a.symbol)))?;

    let renamed = apply_patch_to_symbol(symbol, &patch);
    let ordering_code = patch.properties.get("Ordering code").cloned();

    Ok(json!({
        "symbol": a.symbol,
        "renamed_pins": renamed,
        "ordering_code": ordering_code
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyParams {
    part: String,
    symbol: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    part: String,
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use pcbcraft_geom::Point;
    use pcbcraft_library::datasheet_extract::{
        DatasheetExtractBundle, DatasheetRect, DatasheetSourceRecord, ExtractedPinRow,
        ReviewedField,
    };
    use pcbcraft_library::{SymbolDefinition, SymbolPin};

    fn make_symbol(name: &str) -> SymbolDefinition {
        SymbolDefinition {
            name: name.into(),
            reference: name.into(),
            value: name.into(),
            description: String::new(),
            keywords: String::new(),
            footprint: String::new(),
            set: String::new(),
            pins: vec![
                SymbolPin {
                    number: "1".into(),
                    name: "A".into(),
                    position: Point::new(0.0, 0.0),
                    angle: 0.0,
                    length: 2.54,
                    electrical_type: "passive".into(),
                    unit: 1,
                    body_style: 1,
                    graphical_style: "line".into(),
                    visible: true,
                },
                SymbolPin {
                    number: "2".into(),
                    name: "B".into(),
                    position: Point::new(0.0, 0.0),
                    angle: 0.0,
                    length: 2.54,
                    electrical_type: "passive".into(),
                    unit: 1,
                    body_style: 1,
                    graphical_style: "line".into(),
                    visible: true,
                },
            ],
            graphics: Vec::new(),
            extends: None,
            raw_source: String::new(),
            unsupported: Vec::new(),
            retained: vec![],
        }
    }

    fn make_pin_row(
        pin_number: &str,
        pin_name: &str,
        decision: ReviewDecision,
    ) -> Result<ReviewedField<ExtractedPinRow>, String> {
        let source = DatasheetSourceRecord::new(
            0,
            3,
            DatasheetRect {
                x0: 10.0,
                y0: 20.0,
                x1: 100.0,
                y1: 80.0,
            },
            format!("{} {}", pin_number, pin_name).as_str(),
            None,
        )?;
        let mut row = ReviewedField::new(ExtractedPinRow {
            pin_number: pin_number.into(),
            pin_name: pin_name.into(),
            source,
        });
        row.decision = decision;
        Ok(row)
    }

    fn make_bundle_with_pins(
        pins: Vec<(String, String, ReviewDecision)>,
    ) -> Result<DatasheetExtractBundle, String> {
        let mut bundle = DatasheetExtractBundle::default();
        for (num, name, decision) in pins {
            bundle.pin_table.push(make_pin_row(&num, &name, decision)?);
        }
        Ok(bundle)
    }

    #[test]
    fn rejected_values_are_never_written_to_the_part() -> Result<(), String> {
        let mut s = Session::default();
        s.project.library.symbols.push(make_symbol("IC"));

        let bundle = make_bundle_with_pins(vec![
            ("1".to_string(), "IN+".to_string(), ReviewDecision::Rejected),
            ("2".to_string(), "GND".to_string(), ReviewDecision::Accepted),
        ])?;
        let json = serde_json::to_string(&bundle).map_err(|e| e.to_string())?;
        s.project.files.push(pcbcraft_doc::EmbeddedFile {
            name: "datasheet-extract-TESTIC.json".to_string(),
            data: json,
            encoding: String::new(),
        });

        let result = s
            .execute(
                "datasheet.apply_review",
                json!({
                    "part": "TESTIC",
                    "symbol": "IC"
                }),
                None,
            )
            .map_err(|e| e.to_string())?;
        assert_eq!(result["result"]["renamed_pins"], 1);

        let symbol = s
            .project
            .library
            .symbols
            .iter()
            .find(|sym| sym.name == "IC")
            .ok_or("symbol not found")?;
        assert_eq!(symbol.pins[0].name, "A");
        assert_eq!(symbol.pins[1].name, "GND");
        Ok(())
    }

    #[test]
    fn accepted_values_store_page_and_rectangle() -> Result<(), String> {
        let mut s = Session::default();
        s.project.library.symbols.push(make_symbol("IC"));

        let bundle = make_bundle_with_pins(vec![(
            "1".to_string(),
            "VCC".to_string(),
            ReviewDecision::Accepted,
        )])?;
        let json = serde_json::to_string(&bundle).map_err(|e| e.to_string())?;
        s.project.files.push(pcbcraft_doc::EmbeddedFile {
            name: "datasheet-extract-TESTIC.json".to_string(),
            data: json,
            encoding: String::new(),
        });

        let result = s
            .execute(
                "datasheet.apply_review",
                json!({
                    "part": "TESTIC",
                    "symbol": "IC"
                }),
                None,
            )
            .map_err(|e| e.to_string())?;
        assert_eq!(result["result"]["renamed_pins"], 1);

        let attachment = s
            .project
            .files
            .iter()
            .find(|f| f.name == "datasheet-extract-TESTIC.json")
            .ok_or("attachment not found")?;
        let bundle: DatasheetExtractBundle =
            serde_json::from_str(&attachment.data).map_err(|e| e.to_string())?;
        let row = bundle.pin_table.first().ok_or("no pin rows")?;
        assert_eq!(row.value.source.page_label, 3);
        assert_eq!(row.value.source.rect.x0, 10.0);
        assert_eq!(row.value.source.rect.y1, 80.0);
        Ok(())
    }

    #[test]
    fn datasheet_apply_is_one_undo_step() -> Result<(), String> {
        let mut s = Session::default();
        s.project.library.symbols.push(make_symbol("IC"));

        let bundle = make_bundle_with_pins(vec![(
            "1".to_string(),
            "VCC".to_string(),
            ReviewDecision::Accepted,
        )])?;
        let json = serde_json::to_string(&bundle).map_err(|e| e.to_string())?;
        s.project.files.push(pcbcraft_doc::EmbeddedFile {
            name: "datasheet-extract-TESTIC.json".to_string(),
            data: json,
            encoding: String::new(),
        });

        let result = s
            .execute(
                "datasheet.apply_review",
                json!({
                    "part": "TESTIC",
                    "symbol": "IC"
                }),
                None,
            )
            .map_err(|e| e.to_string())?;
        assert_eq!(result["result"]["renamed_pins"], 1);

        let symbol = s
            .project
            .library
            .symbols
            .iter()
            .find(|sym| sym.name == "IC")
            .ok_or("symbol not found")?;
        assert_eq!(symbol.pins[0].name, "VCC");

        s.undo().map_err(|e| e.to_string())?;

        let symbol = s
            .project
            .library
            .symbols
            .iter()
            .find(|sym| sym.name == "IC")
            .ok_or("symbol not found")?;
        assert_eq!(symbol.pins[0].name, "A");
        Ok(())
    }

    #[test]
    fn review_list_flattens_every_table_with_its_page() -> Result<(), String> {
        let mut s = Session::default();
        let bundle = make_bundle_with_pins(vec![
            ("1".to_string(), "IN+".to_string(), ReviewDecision::Pending),
            ("2".to_string(), "GND".to_string(), ReviewDecision::Pending),
        ])?;
        let json = serde_json::to_string(&bundle).map_err(|e| e.to_string())?;
        s.project.files.push(pcbcraft_doc::EmbeddedFile {
            name: "datasheet-extract-TESTIC.json".to_string(),
            data: json,
            encoding: String::new(),
        });
        let result = s
            .execute("datasheet.review_list", json!({ "part": "TESTIC" }), None)
            .map_err(|e| e.to_string())?;
        let rows = result["result"]["rows"].as_array().ok_or("no rows")?;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["table"], "pin_table");
        assert_eq!(rows[0]["index"], 0);
        assert_eq!(rows[0]["label"], "Pin 1 IN+");
        assert_eq!(rows[0]["page_label"], 3);
        let rect = rows[0]["rect"].as_array().ok_or("no rect")?;
        assert_eq!(rect[0], 10.0);
        assert_eq!(rect[1], 20.0);
        assert_eq!(rect[2], 100.0);
        assert_eq!(rect[3], 80.0);
        assert_eq!(rows[0]["decision"], "pending");
        assert_eq!(rows[1]["label"], "Pin 2 GND");
        Ok(())
    }

    #[test]
    fn review_list_without_an_extraction_is_an_error() -> Result<(), String> {
        let mut s = Session::default();
        let message = s
            .execute("datasheet.review_list", json!({ "part": "MISSING" }), None)
            .err()
            .map(|e| e.to_string())
            .ok_or("a part with no extraction must be refused")?;
        assert!(
            message.contains("no extraction for this part"),
            "the refusal names the missing extraction: {message}"
        );
        Ok(())
    }
}
