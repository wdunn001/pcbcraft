//! Embedded connector series tables used by the first-slice generators.
//!
//! # Rules
//! - Values are hand-entered from public manufacturer series pages; no manufacturer files are stored.
//! - Only TSW, SSW, and BTH are embedded in this slice.
//!
//! # Dependencies
//! - crates/library/data/connectors/*.toml
use serde::Deserialize;

const TSW_TOML: &str = include_str!("../data/connectors/tsw.toml");
const SSW_TOML: &str = include_str!("../data/connectors/ssw.toml");
const BTH_TOML: &str = include_str!("../data/connectors/bth.toml");

/// One lead-length code and its tail below the board.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct LeadCodeSpec {
    pub code: String,
    pub tail_below_board_mm: f64,
}

/// Board-to-board stack height code.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct StackCodeSpec {
    pub code: String,
    pub mated_stack_height_mm: f64,
    pub board_gap_mm: f64,
}

/// Land pattern and body dimensions for a series.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SeriesGeometry {
    pub land_pattern_family: String,
    #[serde(default)]
    pub dual_row_land_pattern_family: String,
    pub pitch_mm: f64,
    #[serde(default)]
    pub th_pad_mm: f64,
    #[serde(default)]
    pub th_drill_mm: f64,
    #[serde(default)]
    pub row_spacing_mm: f64,
    #[serde(default)]
    pub smt_pad_width_mm: f64,
    #[serde(default)]
    pub smt_pad_length_mm: f64,
    #[serde(default)]
    pub post_size_mm: f64,
    pub housing_height_mm: f64,
    #[serde(default)]
    pub pin_above_board_mm: f64,
    #[serde(default)]
    pub board_gap_mm: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SeriesLimits {
    pub positions_per_row_min: u32,
    pub positions_per_row_max: u32,
    pub rows_min: u32,
    pub rows_max: u32,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PartNumberTemplate {
    #[serde(default)]
    pub single_row: String,
    #[serde(default)]
    pub dual_row: String,
}

/// Full series table for one connector family.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ConnectorSeriesSpec {
    pub series_id: String,
    pub manufacturer: String,
    pub drawing_url: String,
    pub verify_note: String,
    pub geometry: SeriesGeometry,
    pub limits: SeriesLimits,
    #[serde(default)]
    pub lead_codes: Vec<LeadCodeSpec>,
    #[serde(default)]
    pub stack_codes: Vec<StackCodeSpec>,
    pub part_number_template: PartNumberTemplate,
}

impl ConnectorSeriesSpec {
    fn parse(toml_text: &str) -> Result<Self, String> {
        toml::from_str(toml_text).map_err(|e| format!("series table parse error: {e}"))
    }

    pub fn lead_tail_mm(&self, code: &str) -> Result<f64, String> {
        self.lead_codes
            .iter()
            .find(|entry| entry.code.eq_ignore_ascii_case(code))
            .map(|entry| entry.tail_below_board_mm)
            .ok_or_else(|| {
                let valid: Vec<_> = self.lead_codes.iter().map(|e| e.code.as_str()).collect();
                format!(
                    "lead code {code} is invalid for {}; valid values: {}",
                    self.series_id,
                    valid.join(", ")
                )
            })
    }

    pub fn stack_height_mm(&self, code: &str) -> Result<(f64, f64), String> {
        self.stack_codes
            .iter()
            .find(|entry| entry.code.eq_ignore_ascii_case(code))
            .map(|entry| (entry.mated_stack_height_mm, entry.board_gap_mm))
            .ok_or_else(|| {
                let valid: Vec<_> = self.stack_codes.iter().map(|e| e.code.as_str()).collect();
                format!(
                    "stack code {code} is invalid for {}; valid values: {}",
                    self.series_id,
                    valid.join(", ")
                )
            })
    }
}

/// Load a first-slice series by id (`TSW`, `SSW`, or `BTH`).
pub fn series_spec(series_id: &str) -> Result<ConnectorSeriesSpec, String> {
    let id = series_id.trim().to_ascii_uppercase();
    let text = match id.as_str() {
        "TSW" => TSW_TOML,
        "SSW" => SSW_TOML,
        "BTH" => BTH_TOML,
        _ => return Err(format!("unknown connector series {series_id}")),
    };
    let mut spec = ConnectorSeriesSpec::parse(text)?;
    if spec.series_id.to_ascii_uppercase() != id {
        return Err(format!("series table id mismatch for {series_id}"));
    }
    Ok(spec)
}

pub fn first_slice_series_ids() -> &'static [&'static str] {
    &["TSW", "SSW", "BTH"]
}
