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
    let rect = bundle.pin_table[0].value.source.rect;
    assert_eq!(rect.x0, 10.0);
    assert_eq!(rect.y0, 70.0);
    assert_eq!(rect.x1, 60.0);
    assert_eq!(rect.y1, 80.0);
    Ok(())
}

#[test]
fn fixture_yields_ratings_and_ordering_with_provenance() -> Result<(), String> {
    let doc = fixture_doc()?;
    let bundle = extract_from_text_document(&doc)?;
    assert_eq!(bundle.absolute_maximum_ratings.len(), 1);
    let rating = &bundle.absolute_maximum_ratings[0].value;
    assert_eq!(rating.parameter, "VDD");
    assert_eq!(rating.value, "6.0");
    assert_eq!(rating.unit, "V");
    assert_eq!(rating.source.page_label, 7);
    assert!(rating.source.rect.x1 > rating.source.rect.x0);

    assert_eq!(bundle.ordering_codes.len(), 1);
    let order = &bundle.ordering_codes[0].value;
    assert_eq!(order.code, "LM358DR");
    assert_eq!(order.description, "SOIC-8");
    assert_eq!(order.source.page_label, 7);
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
fn rejected_ordering_stays_off_the_part() -> Result<(), String> {
    let doc = fixture_doc()?;
    let mut bundle = extract_from_text_document(&doc)?;
    for row in &mut bundle.pin_table {
        set_decision(row, ReviewDecision::Accepted)?;
    }
    set_decision(&mut bundle.ordering_codes[0], ReviewDecision::Rejected)?;
    let patch = build_part_patch(&bundle)?;
    assert!(!patch.properties.contains_key("Ordering code"));
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
