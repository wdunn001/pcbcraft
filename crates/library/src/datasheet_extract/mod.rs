//! Datasheet table extraction with page and rectangle provenance (issue #711).
//!
//! Deterministic parsing from a text layer; optional desktop sidecar uses hayro when the
//! `datasheet-sidecar` feature is enabled. Every proposed value carries a source record
//! aligned with the shared datasheet rules service (#678). Review accept, edit and reject
//! gates what may be written onto a part.

mod apply;
mod parse;
mod provenance;
mod review;
mod text;
mod types;

#[cfg(feature = "datasheet-sidecar")]
mod sidecar_hayro;

pub use apply::{PartFieldPatch, apply_patch_to_symbol, build_part_patch};
pub use parse::{TextTableExtractor, extract_from_text_document};
pub use provenance::{DatasheetRect, DatasheetSourceRecord, bounded_quote};
pub use review::{ReviewDecision, ReviewedField, effective_text, set_decision};
pub use text::{TextCell, TextDocument, TextPage, cells_from_char_boxes};
pub use types::{
    DatasheetExtractBundle, ExtractedOrderingCode, ExtractedPackageDimension, ExtractedPinRow,
    ExtractedRatingRow, MAX_EXTRACT_ROWS, MAX_FIELD_TEXT,
};

/// Extracts pin, rating, ordering and dimension tables from one datasheet file.
pub trait DatasheetTableExtractor {
    fn extract(&self, pdf_bytes: &[u8]) -> Result<DatasheetExtractBundle, String>;
}

/// Default in-process extractor: hayro text layer when built with `datasheet-sidecar`,
/// otherwise returns an error directing callers to supply a [`TextDocument`].
pub struct DefaultDatasheetTableExtractor;

impl DatasheetTableExtractor for DefaultDatasheetTableExtractor {
    fn extract(&self, pdf_bytes: &[u8]) -> Result<DatasheetExtractBundle, String> {
        #[cfg(feature = "datasheet-sidecar")]
        {
            return sidecar_hayro::extract_pdf(pdf_bytes);
        }
        #[cfg(not(feature = "datasheet-sidecar"))]
        {
            let _ = pdf_bytes;
            Err(
                "PDF table extraction needs the datasheet-sidecar feature or a TextDocument"
                    .into(),
            )
        }
    }
}

impl DatasheetTableExtractor for TextTableExtractor {
    fn extract(&self, pdf_bytes: &[u8]) -> Result<DatasheetExtractBundle, String> {
        let _ = pdf_bytes;
        extract_from_text_document(&self.document)
    }
}
