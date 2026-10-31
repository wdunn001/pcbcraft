//! CI test: every generated fixture renders with ink and its known strings are found by search.

use datasheet_spike::{fixtures, open, render_page, search, text_layer};
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
            let hits = search::find(&layer, needle);
            assert!(
                !hits.is_empty(),
                "{}: '{needle}' not found on page {page}",
                f.name
            );
            let h = &hits[0];
            let (w, ht) = datasheet_spike::page_size(&pdf, *page).unwrap_or((0.0, 0.0));
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
fn rotated_page_swaps_size() {
    let all = fixtures::all();
    let f = all
        .iter()
        .find(|f| f.name == "rotated-90.pdf")
        .expect("fixture");
    let pdf = open(f.bytes.clone()).expect("open");
    let (w, h) = datasheet_spike::page_size(&pdf, 0).expect("size");
    assert!(w > h, "rotated page should be landscape, got {w}x{h}");
}
