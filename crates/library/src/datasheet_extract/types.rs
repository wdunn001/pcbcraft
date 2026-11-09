use serde::{Deserialize, Serialize};

use super::provenance::DatasheetSourceRecord;
use super::review::ReviewedField;

pub const MAX_FIELD_TEXT: usize = 256;
pub const MAX_EXTRACT_ROWS: usize = 256;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractedPinRow {
    pub pin_number: String,
    pub pin_name: String,
    pub source: DatasheetSourceRecord,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractedRatingRow {
    pub parameter: String,
    pub value: String,
    pub unit: String,
    pub source: DatasheetSourceRecord,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractedOrderingCode {
    pub code: String,
    pub description: String,
    pub source: DatasheetSourceRecord,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractedPackageDimension {
    pub name: String,
    pub value: String,
    pub unit: String,
    pub source: DatasheetSourceRecord,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasheetExtractBundle {
    pub pin_table: Vec<ReviewedField<ExtractedPinRow>>,
    pub absolute_maximum_ratings: Vec<ReviewedField<ExtractedRatingRow>>,
    pub recommended_operating_conditions: Vec<ReviewedField<ExtractedRatingRow>>,
    pub ordering_codes: Vec<ReviewedField<ExtractedOrderingCode>>,
    pub package_dimensions: Vec<ReviewedField<ExtractedPackageDimension>>,
}

impl DatasheetExtractBundle {
    pub fn pin_table_page_labels(&self) -> Vec<u32> {
        let mut pages: Vec<u32> = self
            .pin_table
            .iter()
            .map(|r| r.value.source.page_label)
            .collect();
        pages.sort_unstable();
        pages.dedup();
        pages
    }
}
