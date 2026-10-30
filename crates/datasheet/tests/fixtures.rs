//! CI: generated fixtures render, search, and document-wide lookup.

use pcbcraft_datasheet::{
    Document, find, fixtures, open, page_size, render_page, search_document, selected_text,
    text_layer, Selection,
};
use hayro::RenderCache;
use hayro::hayro_interpret::InterpreterCache;

#[test]
fn fixtures_render_and_search() {
    for f in fixtures::all() {
        let pdf = open(f.bytes.clone()).unwrap_or_else(|e| panic!("{}: {e}", f.name));
        let rc = RenderCache::new();
        let ic = InterpreterCache::new();
        let bmp = render_page(&pdf, 0, 100.0, &rc).unwrap_or_else(|e| panic!("{}: {e}", f.name));
        let inked = bmp
            .rgba
            .chunks(4)
            .any(|p| p[0] < 200 || p[1] < 200 || p[2] < 200);
        assert_eq!(inked, f.has_ink, "{} ink", f.name);
        for (page, needle) in &f.expect {
            let layer = text_layer(&pdf, *page, &ic).unwrap_or_else(|e| panic!("{}: {e}", f.name));
            let hits = find(&layer, needle);
            assert!(
                !hits.is_empty(),
                "{}: '{needle}' not found on page {page}",
                f.name
            );
            let h = &hits[0];
            let (w, ht) = page_size(&pdf, *page).unwrap_or((0.0, 0.0));
            assert!(
                h.x0 >= -1.0 && h.x1 <= w + 1.0 && h.y0 >= -1.0 && h.y1 <= ht + 1.0,
                "{}: box outside page {:?}",
                f.name,
                h
            );
        }
    }
}

#[test]
fn document_search_finds_every_page() {
    let f = fixtures::all()
        .into_iter()
        .find(|f| f.name == "two-pages.pdf")
        .unwrap_or_else(|| panic!("fixture"));
    let doc = Document::open(f.bytes).unwrap_or_else(|e| panic!("open: {e}"));
    let found = doc.search("LM317").unwrap_or_else(|e| panic!("search: {e}"));
    assert_eq!(found.count(), 1);
    assert_eq!(found.hits[0].page, 0);
    let ratings = doc.search("Absolute").unwrap_or_else(|e| panic!("search: {e}"));
    assert_eq!(ratings.count(), 1);
    assert_eq!(ratings.hits[0].page, 1);
    let many = doc.search("Pin").unwrap_or_else(|e| panic!("search: {e}"));
    assert!(many.count() >= 1);
}

#[test]
fn copy_returns_reading_order() {
    let f = fixtures::all()
        .into_iter()
        .find(|f| f.name == "text-one-page.pdf")
        .unwrap_or_else(|| panic!("fixture"));
    let doc = Document::open(f.bytes).unwrap_or_else(|e| panic!("open: {e}"));
    let layer = doc.text_layer(0).unwrap_or_else(|e| panic!("layer: {e}"));
    let sel = Selection {
        anchor: 0,
        focus: layer.len().saturating_sub(1),
    };
    let copied = selected_text(&layer, sel);
    assert!(
        copied.contains("LM317") && copied.contains("TO-220"),
        "copy should span the title and body: {copied}"
    );
}

#[test]
fn ink_only_page_has_no_text_layer() {
    let f = fixtures::all()
        .into_iter()
        .find(|f| f.name == "ink-only.pdf")
        .unwrap_or_else(|| panic!("fixture"));
    let doc = Document::open(f.bytes).unwrap_or_else(|e| panic!("open: {e}"));
    assert!(
        !doc.page_has_text(0).unwrap_or_else(|e| panic!("layer: {e}")),
        "vector ink alone is not a text layer"
    );
}

#[test]
fn rotated_page_swaps_size() {
    let f = fixtures::all()
        .into_iter()
        .find(|f| f.name == "rotated-90.pdf")
        .unwrap_or_else(|| panic!("fixture"));
    let pdf = open(f.bytes).unwrap_or_else(|e| panic!("open: {e}"));
    let (w, h) = page_size(&pdf, 0).unwrap_or((0.0, 0.0));
    assert!(w > h, "rotated page should be landscape, got {w}x{h}");
}

#[test]
fn search_document_matches_page_helper() {
    let f = fixtures::all()
        .into_iter()
        .find(|f| f.name == "text-one-page.pdf")
        .unwrap_or_else(|| panic!("fixture"));
    let pdf = open(f.bytes).unwrap_or_else(|e| panic!("open: {e}"));
    let doc = search_document(&pdf, "1.5").unwrap_or_else(|e| panic!("search: {e}"));
    assert_eq!(doc.count(), 1);
}
