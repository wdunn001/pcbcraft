//! URL and PDF byte checks shared by the desktop cache and the browser reader.

/// Longest accepted http(s) link on a part record.
pub const MAX_URL_LEN: usize = 2048;

/// Largest PDF the cache accepts (25 MiB).
pub const MAX_PDF_BYTES: usize = 25 * 1024 * 1024;

/// Default total cache budget (500 MiB).
pub const DEFAULT_CACHE_CAP_BYTES: u64 = 500 * 1024 * 1024;

/// True when `bytes` begin with the PDF magic header.
pub fn is_pdf(bytes: &[u8]) -> bool {
    bytes.len() >= 5 && &bytes[..5] == b"%PDF-"
}

/// Accept only plain http(s) links with no embedded whitespace.
pub fn http_url(raw: Option<&str>) -> Option<String> {
    let url = raw?.trim();
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return None;
    }
    if url.len() > MAX_URL_LEN || url.chars().any(char::is_whitespace) {
        return None;
    }
    Some(url.to_owned())
}

/// Lower-case hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Reject oversize or non-PDF input before it enters the cache.
pub fn validate_pdf_bytes(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_PDF_BYTES {
        return Err(format!(
            "That PDF is too large (limit {} bytes).",
            MAX_PDF_BYTES
        ));
    }
    if !is_pdf(bytes) {
        return Err("That file is not a valid PDF.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_urls_must_be_plain_web_links() {
        assert_eq!(
            http_url(Some(" https://example.com/a.pdf ")).as_deref(),
            Some("https://example.com/a.pdf")
        );
        assert!(http_url(Some("HTTP://example.com/a")).is_some());
        for bad in [
            "",
            "file:///tmp/x.pdf",
            "javascript:alert(1)",
            "https://a b",
        ] {
            assert!(http_url(Some(bad)).is_none(), "{bad} is refused");
        }
        assert!(http_url(None).is_none());
    }

    #[test]
    fn pdf_magic_is_required() {
        assert!(is_pdf(b"%PDF-1.4\n"));
        assert!(!is_pdf(b"not a pdf"));
    }
}
