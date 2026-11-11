//! Story: A newcomer reviews a datasheet pin table.
//!
//! They extract the pin table from a fixture datasheet and check that every
//! row carries its page provenance (issue 365).
//!
//! # Dependencies
//!
//! - crates/library/src/datasheet_extract/mod.rs
//! - crates/library/tests/fixtures/datasheet_extract/sample_text_doc.json

use std::fs;
use std::path::Path;

use pcbcraft_library::datasheet_extract::{TextDocument, extract_from_text_document};

fn fixture_doc() -> Result<TextDocument, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/datasheet_extract/sample_text_doc.json");
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

#[test]
fn a_newcomer_reviews_a_datasheet_pin_table() -> Result<(), String> {
    let doc = fixture_doc()?;
    let bundle = extract_from_text_document(&doc)?;

    assert_eq!(bundle.pin_table.len(), 2);
    assert_eq!(bundle.pin_table_page_labels(), vec![3]);

    for row in bundle.pin_table.iter() {
        assert_eq!(row.value.source.page_label, 3);
    }

    let json = serde_json::to_value(&bundle).map_err(|e| e.to_string())?;
    let page = json["pin_table"][0]["value"]["source"]["page_label"]
        .as_i64()
        .ok_or("page label missing")?;
    assert_eq!(page, 3);

    Ok(())
}
