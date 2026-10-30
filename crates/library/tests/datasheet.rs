use pcbcraft_library::datasheet::{
    ProjectDatasheetRef, accept_datasheet_url, content_sha256, is_pdf,
};

#[test]
fn url_gate_accepts_plain_http_and_https() {
    assert_eq!(
        accept_datasheet_url(" https://example.com/a.pdf ").as_deref(),
        Some("https://example.com/a.pdf")
    );
    assert_eq!(
        accept_datasheet_url("HTTP://example.com/a").as_deref(),
        Some("HTTP://example.com/a")
    );
}

#[test]
fn url_gate_refuses_bad_addresses() {
    for bad in [
        "",
        "ftp://example.com/x.pdf",
        "https://ex ample.com/x.pdf",
        "https://user@example.com/x.pdf",
        &"https://example.com/".repeat(3000),
    ] {
        assert!(accept_datasheet_url(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn project_ref_serializes_url_hash_and_revision_only() -> Result<(), String> {
    let reference = ProjectDatasheetRef {
        url: "https://example.test/d.pdf".into(),
        content_sha256: content_sha256(b"%PDF-1.4\n"),
        revision: "abc".into(),
    };
    let json = serde_json::to_string(&reference).map_err(|e| e.to_string())?;
    assert!(json.contains("content_sha256"));
    assert!(!json.contains("%PDF"));
    Ok(())
}

#[test]
fn pdf_header_check_is_strict() {
    assert!(is_pdf(b"%PDF-1.7\n"));
    assert!(!is_pdf(b"PDF-1.7\n"));
}
