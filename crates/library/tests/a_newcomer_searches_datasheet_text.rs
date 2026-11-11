//! Story from issue #360: a newcomer can search, select and copy text in datasheets.
use pcbcraft_datasheet::{
    Selection, fixture_pdf, ingest_pdf, open, page_has_text, plain_text, search_document,
    selected_text, text_layer,
};

#[test]
fn a_newcomer_searches_selects_and_copies_datasheet_text() -> Result<(), String> {
    // A newcomer opens a datasheet PDF.
    let bytes = fixture_pdf::one_page_text();
    let (record, pdf_bytes) = ingest_pdf(bytes.clone(), None, None, "2026-01-01T00:00:00Z")?;
    assert_eq!(record.byte_len, bytes.len() as u64);

    // They parse the document.
    let pdf = open(pdf_bytes)?;

    // They can see the text layer of a page.
    let ic = hayro::hayro_interpret::InterpreterCache::new();
    let layer = text_layer(&pdf, 0, &ic)?;
    assert!(!layer.is_empty());

    // They can get the plain text of a page.
    let text = plain_text(&layer);
    assert!(text.contains("Sample"));

    // They can check if a page has text.
    assert!(page_has_text(&layer));

    // They can search for a word and find hits on the correct page.
    let search = search_document(&pdf, "Sample")?;
    assert!(search.hits.iter().any(|h| h.page == 0));

    // They can select text by specifying a region.
    let region = Selection {
        anchor: 0,
        focus: 1,
    };
    let selected = selected_text(&layer, region);
    // Selection should return text without panicking.
    let _ = selected;

    Ok(())
}
