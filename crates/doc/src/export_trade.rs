//! Project settings for the export-and-trade compliance rule set: a user ITAR
//! reminder flag and a dated tariff exposure table used for informational
//! estimates only.
use serde::{Deserialize, Serialize};

/// Most rows allowed in the tariff exposure table.
pub const MAX_TARIFF_ROWS: usize = 64;
/// Longest tariff table text field, in bytes.
pub const MAX_TARIFF_TEXT: usize = 256;
/// Longest tariff row note, in bytes.
pub const MAX_TARIFF_NOTE: usize = 512;
/// Longest HTS prefix on a tariff row, in bytes.
pub const MAX_HTS_PREFIX: usize = 16;

fn bounded(text: &str, max: usize, label: &str) -> Result<(), String> {
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(format!(
            "{label} must be at most {max} bytes without control characters"
        ));
    }
    Ok(())
}

fn iso_country(code: &str) -> Result<(), String> {
    if code.len() != 2 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err("origin must be a two-letter ISO 3166-1 alpha-2 code".into());
    }
    Ok(())
}

/// One row of the user-maintained tariff exposure table.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TariffExposureRow {
    /// HTS prefix matched against each part's declared HTS code (longest prefix wins).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hts_prefix: String,
    /// Country of origin the row applies to.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
    /// Estimated duty rate in percent, such as 25 for twenty-five percent.
    #[serde(default)]
    pub rate_percent: f64,
    /// First calendar day this row applies, ISO 8601 `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub effective_from: String,
    /// Last calendar day this row applies; empty means open-ended.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub effective_to: String,
    /// Free-form note printed beside the estimate.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl TariffExposureRow {
    /// Check HTS prefix, origin code, rate and ISO date bounds.
    pub fn validate(&self) -> Result<(), String> {
        bounded(&self.hts_prefix, MAX_HTS_PREFIX, "HTS prefix")?;
        if self.hts_prefix.trim().is_empty() {
            return Err("a tariff row needs an HTS prefix".into());
        }
        iso_country(&self.origin)?;
        if !self.rate_percent.is_finite() || self.rate_percent < 0.0 || self.rate_percent > 1000.0 {
            return Err("tariff rate must be a finite percent between 0 and 1000".into());
        }
        bounded(&self.effective_from, MAX_TARIFF_TEXT, "effective from date")?;
        if self.effective_from.len() != 10 {
            return Err("effective from must be an ISO date YYYY-MM-DD".into());
        }
        if !self.effective_to.is_empty() {
            bounded(&self.effective_to, MAX_TARIFF_TEXT, "effective to date")?;
            if self.effective_to.len() != 10 {
                return Err("effective to must be an ISO date YYYY-MM-DD".into());
            }
            if self.effective_to < self.effective_from {
                return Err("effective to must not be before effective from".into());
            }
        }
        bounded(&self.note, MAX_TARIFF_NOTE, "tariff note")?;
        Ok(())
    }
}

/// Export-and-trade choices stored on the project.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExportTradeSettings {
    /// When true, the report notes that the user flagged ITAR relevance; never a licence claim.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub itar_flag: bool,
    /// Informational tariff estimates keyed by HTS prefix and origin.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tariff_rows: Vec<TariffExposureRow>,
    /// Id of the party screening list snapshot to use; empty uses the bundled fixture list.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub screening_list_id: String,
}

impl ExportTradeSettings {
    /// True when the ITAR flag is off and no tariff rows or list id are stored.
    pub fn is_empty(&self) -> bool {
        !self.itar_flag && self.tariff_rows.is_empty() && self.screening_list_id.is_empty()
    }

    /// Check row count, list id length and each tariff row.
    pub fn validate(&self) -> Result<(), String> {
        if self.tariff_rows.len() > MAX_TARIFF_ROWS {
            return Err(format!(
                "the tariff exposure table holds at most {MAX_TARIFF_ROWS} rows"
            ));
        }
        bounded(
            &self.screening_list_id,
            MAX_TARIFF_TEXT,
            "screening list id",
        )?;
        for row in &self.tariff_rows {
            row.validate()?;
        }
        Ok(())
    }
}
