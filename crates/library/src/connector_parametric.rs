//! Parametric connector body metadata attached to generated footprints.
//!
//! # Rules
//! - The model path is always [`PARAMETRIC_MODEL_PATH`]; geometry lives in JSON on the model reference.
//! - Body bounds for checks use z = 0 at the board surface.
//!
//! # Dependencies
//! - crates/library/src/lib.rs [`FootprintDefinition`]
use crate::FootprintDefinition;
use crate::connector_series_data::ConnectorSeriesSpec;
use crate::{Graphic, Model3D};
use serde::{Deserialize, Serialize};

pub const PARAMETRIC_MODEL_PATH: &str = "parametric:connector/v1";

/// Serializable body parameters stored on the footprint model reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorParametricSpec {
    pub version: u32,
    pub series_id: String,
    pub rows: u32,
    pub positions_per_row: u32,
    pub pitch_mm: f64,
    pub row_spacing_mm: f64,
    pub housing_length_mm: f64,
    pub housing_width_mm: f64,
    pub housing_height_mm: f64,
    pub pin_post_mm: f64,
    pub pin_above_board_mm: f64,
    pub tail_below_board_mm: f64,
    #[serde(default)]
    pub mated_stack_height_mm: f64,
}

impl ConnectorParametricSpec {
    pub fn model_attachment(spec: &Self) -> Result<Model3D, String> {
        let raw = serde_json::to_string(spec).map_err(|e| e.to_string())?;
        if raw.len() > 16_384 {
            return Err("parametric connector model JSON is too large".into());
        }
        Ok(Model3D {
            path: PARAMETRIC_MODEL_PATH.into(),
            offset: [0.0; 3],
            scale: [1.0; 3],
            rotate: [0.0; 3],
            raw_source: raw,
        })
    }

    pub fn from_json(raw: &str) -> Result<Self, String> {
        serde_json::from_str(raw).map_err(|e| format!("parametric connector JSON: {e}"))
    }

    /// Full solid bounds including tail below the board.
    pub fn axis_bounds(&self) -> ([f64; 3], [f64; 3]) {
        let half_l = self.housing_length_mm / 2.0;
        let half_w = self.housing_width_mm / 2.0;
        let min_z = -self.tail_below_board_mm;
        let max_z = self.housing_height_mm.max(self.pin_above_board_mm);
        ([-half_l, -half_w, min_z], [half_l, half_w, max_z])
    }

    /// Housing and pins above the board; the lowest face sits at z = 0.
    pub fn body_above_board_bounds(&self) -> ([f64; 3], [f64; 3]) {
        let half_l = self.housing_length_mm / 2.0;
        let half_w = self.housing_width_mm / 2.0;
        (
            [-half_l, -half_w, 0.0],
            [
                half_l,
                half_w,
                self.pin_above_board_mm.max(self.housing_height_mm),
            ],
        )
    }
}

pub fn housing_length_mm(positions_per_row: u32, pitch_mm: f64) -> f64 {
    if positions_per_row <= 1 {
        pitch_mm
    } else {
        (positions_per_row as f64 - 1.0) * pitch_mm + pitch_mm
    }
}

pub fn build_parametric_spec(
    series: &ConnectorSeriesSpec,
    rows: u32,
    positions_per_row: u32,
    tail_below_board_mm: f64,
    mated_stack_height_mm: f64,
) -> ConnectorParametricSpec {
    let g = &series.geometry;
    let housing_length = housing_length_mm(positions_per_row, g.pitch_mm);
    let housing_width = if rows <= 1 {
        g.pitch_mm * 2.0
    } else if g.land_pattern_family == "smt_dual_row" {
        g.row_spacing_mm + g.smt_pad_width_mm
    } else {
        g.row_spacing_mm + g.pitch_mm
    };
    ConnectorParametricSpec {
        version: 1,
        series_id: series.series_id.clone(),
        rows,
        positions_per_row,
        pitch_mm: g.pitch_mm,
        row_spacing_mm: g.row_spacing_mm,
        housing_length_mm: housing_length,
        housing_width_mm: housing_width,
        housing_height_mm: g.housing_height_mm,
        pin_post_mm: g.post_size_mm.max(g.smt_pad_width_mm),
        pin_above_board_mm: g.pin_above_board_mm.max(g.housing_height_mm),
        tail_below_board_mm,
        mated_stack_height_mm,
    }
}

pub fn body_fits_courtyard(spec: &ConnectorParametricSpec, fp: &FootprintDefinition) -> bool {
    let (body_min, body_max) = spec.body_above_board_bounds();
    let Some((cx_min, cy_min, cx_max, cy_max)) = courtyard_rect(fp) else {
        return false;
    };
    body_min[0] >= cx_min && body_max[0] <= cx_max && body_min[1] >= cy_min && body_max[1] <= cy_max
}

fn courtyard_rect(fp: &FootprintDefinition) -> Option<(f64, f64, f64, f64)> {
    for g in &fp.graphics {
        if let Graphic::Rectangle {
            start, end, layer, ..
        } = g
        {
            if layer == "pcbcraft.courtyard" {
                return Some((
                    start.x.min(end.x),
                    start.y.min(end.y),
                    start.x.max(end.x),
                    start.y.max(end.y),
                ));
            }
        }
    }
    None
}
