//! Engine command for datasheet table extraction (issue #711).
//!
//! Takes a TextDocument payload, extracts pin, rating, ordering and dimension
//! tables, and stores the reviewable bundle as a JSON attachment on the project.
//!
//! # Dependencies
//! - crates/library/src/datasheet_extract/

use super::*;
use pcbcraft_library::datasheet_extract::{TextDocument, extract_from_text_document};

/// Register the datasheet.extract command spec.
///
/// # Rules
/// - part names are bounded to 1-64 bytes of [A-Za-z0-9_-]
/// - the document payload must deserialize as a TextDocument
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
    vec![CommandSpec {
        id: "datasheet.extract",
        description: "Extract pin, rating, ordering and dimension tables from a provided text document and store the reviewable bundle as a project attachment",
        input_schema: schema(
            json!({
                "part": { "type": "string", "minLength": 1, "maxLength": 64, "pattern": "^[A-Za-z0-9_-]+$" },
                "document": document_schema
            }),
            &["part", "document"],
        ),
    }]
}

/// Handle the datasheet.extract command on the design project.
///
/// # Rules
/// - runs the library extractor and stores the JSON bundle as an attachment
/// - refuses bundles over 256 KiB and projects with more than 32 files
/// - replaces the attachment when the same part is extracted again
///
/// # Dependencies
/// - crates/library/src/datasheet_extract/parse.rs
/// - crates/engine/src/lib.rs
pub(super) fn run_design(
    p: &mut Project,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    if command != "datasheet.extract" {
        return None;
    }
    Some(extract_and_store(p, params))
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
    let files = &mut p.files;
    if let Some(existing) = files.iter_mut().find(|f| f.name == name) {
        existing.data = json;
    } else {
        if files.len() >= 32 {
            return Err(Error::Invalid("a project attaches at most 32 files".into()));
        }
        files.push(pcbcraft_doc::EmbeddedFile {
            name: name.clone(),
            data: json,
            encoding: String::new(),
        });
    }
    Ok(json!({
        "part": a.part,
        "pins": bundle.pin_table.len(),
        "ratings": bundle.absolute_maximum_ratings.len() + bundle.recommended_operating_conditions.len(),
        "ordering": bundle.ordering_codes.len(),
        "dimensions": bundle.package_dimensions.len()
    }))
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtractParams {
    part: String,
    document: TextDocument,
}
