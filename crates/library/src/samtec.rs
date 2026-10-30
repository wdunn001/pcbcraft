//! Samtec manufacturer helpers for issue 785 step 1: build the public product
//! page link and provenance for CAD files the user downloaded and dropped in.
//! There is no `CadSource` fetch here until written permission is recorded on
//! the tracking issue.
//!
//! # Rules
//! - No network access; URLs are derived from the part number only.
//! - No Samtec-owned bytes are stored in the repository or shipped in releases.
//!
//! # Dependencies
//! - crates/library/src/cad.rs
use crate::cad::{CadKind, Provenance, USER_MANUFACTURER_SOURCE_ID};
use crate::sources::SourceError;
use std::path::Path;

const MANUFACTURER: &str = "Samtec";
const MAX_MPN_BYTES: usize = 64;
const PRODUCT_BASE: &str = "https://www.samtec.com/products/";

/// Stable id used in notices for user-downloaded Samtec files.
pub const SOURCE_ID: &str = USER_MANUFACTURER_SOURCE_ID;

fn slug(mpn: &str) -> String {
    mpn.trim().to_ascii_lowercase()
}

/// Validate a Samtec part number before building links or provenance.
pub fn check_mpn(mpn: &str) -> Result<(), SourceError> {
    let mpn = mpn.trim();
    if mpn.is_empty() || mpn.len() > MAX_MPN_BYTES {
        return Err(SourceError::Invalid(format!(
            "the part number must be 1 to {MAX_MPN_BYTES} bytes"
        )));
    }
    if !mpn
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(SourceError::Invalid(
            "the part number may only contain letters, digits, hyphens and underscores".into(),
        ));
    }
    Ok(())
}

/// Public manufacturer product page for one part number.
pub fn manufacturer_page_url(mpn: &str) -> Result<String, SourceError> {
    check_mpn(mpn)?;
    Ok(format!("{}{}", PRODUCT_BASE, slug(mpn)))
}

/// Which CAD kind a dropped file name suggests.
pub fn file_kind(path: &Path) -> Result<CadKind, SourceError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| SourceError::Invalid("the file must have an extension".into()))?;
    match ext.as_str() {
        "kicad_mod" => Ok(CadKind::Footprint),
        "kicad_sym" => Ok(CadKind::Symbol),
        "step" | "stp" => Ok(CadKind::Model),
        _ => Err(SourceError::Invalid(
            "expected a symbol, footprint or STEP file".into(),
        )),
    }
}

fn basename(path: &Path) -> Result<String, SourceError> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned)
        .ok_or_else(|| SourceError::Invalid("the file path must end with a name".into()))
}

/// Provenance for one user-downloaded Samtec file.
pub fn user_download_provenance(
    mpn: &str,
    kind: CadKind,
    path: &Path,
) -> Result<Provenance, SourceError> {
    let page = manufacturer_page_url(mpn)?;
    Provenance::user_manufacturer_download(MANUFACTURER, mpn, kind, &basename(path)?, &page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manufacturer_page_uses_the_public_product_slug() -> Result<(), SourceError> {
        assert_eq!(
            manufacturer_page_url("TSW-103-07-G-S")?,
            "https://www.samtec.com/products/tsw-103-07-g-s"
        );
        Ok(())
    }

    #[test]
    fn user_download_notice_states_not_redistributed() -> Result<(), SourceError> {
        let p = user_download_provenance(
            "TSW-103-07-G-S",
            CadKind::Footprint,
            Path::new("/tmp/TSW-103-07-G-S.kicad_mod"),
        )?;
        let n = p.notice();
        assert!(n.contains("downloaded by the user"));
        assert!(n.contains("not redistributed"));
        assert!(n.contains("TSW-103-07-G-S"));
        Ok(())
    }
}
