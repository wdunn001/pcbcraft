use pcbcraft_publication::{check_datasheet_refs, check_embedded_datasheet_bytes, DatasheetRef, ScanOutcome};

#[test]
fn datasheet_refs_require_https_and_hash() {
    let bad = check_datasheet_refs(&[DatasheetRef {
        url: "http://example.com/a.pdf".into(),
        title: "Part A".into(),
        sha256: "short".into(),
    }]);
    assert!(bad.iter().any(|o| matches!(o, ScanOutcome::HardFail { rule: "datasheet_url", .. })));
}

#[test]
fn pdf_bytes_in_bundle_are_refused() {
    let hits = check_embedded_datasheet_bytes("docs/part.pdf", b"%PDF-1.4", "attachment");
    assert!(hits.iter().any(|o| matches!(o, ScanOutcome::HardFail { rule: "datasheet_pdf_bytes", .. })));
}
