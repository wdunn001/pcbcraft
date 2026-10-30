use pcbcraft_library::datasheet_extract::{
    DatasheetExtractBundle, ReviewDecision, TextDocument, TextTableExtractor,
    apply_patch_to_symbol, build_part_patch, extract_from_text_document, set_decision,
};
use pcbcraft_library::{SymbolDefinition, SymbolPin};
use pcbcraft_geom::Point;

fn fixture_doc() -> Result<TextDocument, String> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/datasheet_extract/sample_text_doc.json");
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[test]
fn fixture_yields_pin_table_with_page_numbers() -> Result<(), String> {
    let doc = fixture_doc()?;
    let bundle = extract_from_text_document(&doc)?;
    assert_eq!(bundle.pin_table.len(), 2);
    assert_eq!(bundle.pin_table_page_labels(), vec![3]);
    assert_eq!(bundle.pin_table[0].value.pin_name, "IN+");
    assert_eq!(bundle.pin_table[0].value.source.page_label, 3);
    Ok(())
}

#[test]
fn trait_extracts_from_text_document() -> Result<(), String> {
    let doc = fixture_doc()?;
    let extractor = TextTableExtractor { document: doc };
    let bundle = pcbcraft_library::datasheet_extract::DatasheetTableExtractor::extract(
        &extractor,
        &[],
    )?;
    assert_eq!(bundle.pin_table.len(), 2);
    Ok(())
}

#[test]
fn rejected_values_stay_off_the_part() -> Result<(), String> {
    let doc = fixture_doc()?;
    let mut bundle = extract_from_text_document(&doc)?;
    set_decision(&mut bundle.pin_table[0], ReviewDecision::Accepted)?;
    set_decision(&mut bundle.pin_table[1], ReviewDecision::Rejected)?;
    let patch = build_part_patch(&bundle)?;
    assert_eq!(patch.pin_names.len(), 1);
    assert_eq!(patch.pin_names.get("1").map(String::as_str), Some("IN+"));

    let mut symbol = SymbolDefinition {
        name: "U".into(),
        reference: "U".into(),
        value: "U".into(),
        description: String::new(),
        keywords: String::new(),
        footprint: String::new(),
        set: String::new(),
        pins: vec![
            SymbolPin {
                number: "1".into(),
                name: "~".into(),
                position: Point::ZERO,
                angle: 0.0,
                length: 2.54,
                electrical_type: "input".into(),
                unit: 1,
                body_style: 1,
                graphical_style: "line".into(),
                visible: true,
            },
            SymbolPin {
                number: "2".into(),
                name: "~".into(),
                position: Point::ZERO,
                angle: 180.0,
                length: 2.54,
                electrical_type: "input".into(),
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
    };
    apply_patch_to_symbol(&mut symbol, &patch);
    assert_eq!(symbol.pins[0].name, "IN+");
    assert_eq!(symbol.pins[1].name, "~");
    let _unused: DatasheetExtractBundle = bundle;
    Ok(())
}
