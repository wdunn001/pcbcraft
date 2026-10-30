//! IPC-style land pattern generation into [`FootprintDefinition`].
use crate::{Drill, FootprintDefinition, FootprintPad, Graphic};
use pcbcraft_geom::Point;
use serde::{Deserialize, Serialize};

const COURTYARD_LAYER: &str = "pcbcraft.courtyard";
const SILK_LAYER: &str = "F.SilkS";
const FAB_LAYER: &str = "F.Fab";
const CHIP_ROUNDRECT: f64 = 0.244;
const GRID_MM: f64 = 0.01;

/// Land pattern density level (IPC-7351B).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LandPatternDensity {
    Most,
    Nominal,
    Least,
}

impl LandPatternDensity {
    pub fn parse(level: &str) -> Option<Self> {
        match level.trim().to_ascii_lowercase().as_str() {
            "a" | "m" | "most" => Some(Self::Most),
            "b" | "n" | "nominal" => Some(Self::Nominal),
            "c" | "l" | "least" => Some(Self::Least),
            _ => None,
        }
    }

    pub fn suffix(self) -> char {
        match self {
            Self::Most => 'M',
            Self::Nominal => 'N',
            Self::Least => 'L',
        }
    }

    fn courtyard_excess(self) -> f64 {
        match self {
            Self::Most => 0.50,
            Self::Nominal => 0.25,
            Self::Least => 0.10,
        }
    }

    fn silk_stroke(self) -> f64 {
        match self {
            Self::Most => 0.15,
            Self::Nominal => 0.127,
            Self::Least => 0.10,
        }
    }
}

/// Bounded numeric inputs shared across package families.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandPatternParams {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub keywords: String,
    pub pitch: f64,
    pub pin_count: u32,
    pub body_length: f64,
    pub body_width: f64,
    #[serde(default)]
    pub row_spacing: f64,
    #[serde(default)]
    pub lead_width: f64,
    #[serde(default)]
    pub lead_diameter: f64,
    #[serde(default)]
    pub grid_rows: u32,
    #[serde(default)]
    pub grid_cols: u32,
    #[serde(default)]
    pub thermal_size: f64,
}

/// Build a footprint from a family id and parameter object.
/// Generate a footprint from an IPC-7351B land pattern family.
///
/// # Rules
/// - Density governs courtyard clearance, silk stroke, and pad size
/// - All package dimensions are validated before generation
/// - The resulting footprint is validated and must pass
///
/// # Dependencies
/// - crates/library/src/footprint_generate.rs [`LandPatternParams`] for bounded input validation
/// - crates/library/src/footprint_generate.rs [`FootprintDefinition`] for output structure
///
/// # Do not
/// - Change density constants without updating IPC-7351B reference
/// - Add new families without corresponding validation tests
pub fn generate_footprint(
    family: &str,
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    if params.name.is_empty() || params.name.len() > 256 {
        return Err("footprint name must be 1 to 256 characters".into());
    }
    let mut def = match family {
        "chip_2terminal" => chip_two_terminal(density, params)?,
        "sot" => sot(density, params)?,
        "soic" => soic(density, params)?,
        "qfp" => qfp(density, params)?,
        "qfn" => qfn(density, params)?,
        "bga" => bga(density, params)?,
        "dip" => dip(density, params)?,
        "header_single" => header_single(density, params)?,
        "header_double" => header_double(density, params)?,
        "radial" => radial(density, params)?,
        "axial" => axial(density, params)?,
        _ => return Err(format!("unknown land pattern family {family}")),
    };
    def.name = params.name.clone();
    if !params.description.is_empty() {
        def.description = params.description.clone();
    }
    if !params.keywords.is_empty() {
        def.keywords = params.keywords.clone();
    }
    def.validate().map_err(|e| e.to_string())?;
    Ok(def)
}

fn require_finite_positive(value: f64, label: &str) -> Result<f64, String> {
    if !value.is_finite() || value <= 0.0 || value > 500.0 {
        return Err(format!(
            "{label} must be a finite positive number up to 500 mm"
        ));
    }
    Ok(value)
}

fn require_pin_count(count: u32, min: u32, max: u32, label: &str) -> Result<u32, String> {
    if count < min || count > max {
        return Err(format!("{label} must be between {min} and {max}"));
    }
    Ok(count)
}

fn chip_two_terminal(
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    let body_l = require_finite_positive(params.body_length, "body length")?;
    let body_w = require_finite_positive(params.body_width, "body width")?;
    let scale_l = body_l / 2.0;
    let scale_w = body_w / 1.25;
    let pad_l = 1.025 * scale_l;
    let pad_w = 1.4 * scale_w;
    let center_x = 0.9125 * scale_l;
    let excess = density.courtyard_excess();
    let half_x = center_x + pad_l / 2.0 + excess;
    let half_y = pad_w / 2.0 + excess;
    let pads = vec![
        smd_roundrect("1", -center_x, 0.0, pad_l, pad_w, CHIP_ROUNDRECT),
        smd_roundrect("2", center_x, 0.0, pad_l, pad_w, CHIP_ROUNDRECT),
    ];
    Ok(base_footprint(
        density,
        pads,
        body_l,
        body_w,
        half_x,
        half_y,
        format!(
            "RESC{metric}X70{density}",
            metric = metric_code(body_l, body_w),
            density = density.suffix()
        ),
    ))
}

fn sot(
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    let pitch = require_finite_positive(params.pitch, "pitch")?;
    let count = require_pin_count(params.pin_count, 3, 6, "pin count")?;
    let body_l = require_finite_positive(params.body_length, "body length")?;
    let body_w = require_finite_positive(params.body_width, "body width")?;
    let pad_w = pitch * 0.6;
    let pad_l = pitch * 0.45;
    let mut pads = Vec::new();
    let mut number = 1_u32;
    match count {
        3 => {
            for i in 0..3 {
                let x = (i as f64 - 1.0) * pitch;
                pads.push(smd_roundrect(
                    &number.to_string(),
                    x,
                    body_w / 2.0 + pad_l / 2.0,
                    pad_l,
                    pad_w,
                    0.25,
                ));
                number += 1;
            }
        }
        5 | 6 => {
            let left_y = [-pitch / 2.0, pitch / 2.0];
            for y in left_y {
                pads.push(smd_roundrect(
                    &number.to_string(),
                    -body_l / 2.0 - pad_l / 2.0,
                    y,
                    pad_l,
                    pad_w,
                    0.25,
                ));
                number += 1;
            }
            pads.push(smd_roundrect(
                &number.to_string(),
                0.0,
                body_w / 2.0 + pad_l / 2.0,
                pad_l,
                pad_w,
                0.25,
            ));
            number += 1;
            let right_ys = if count == 5 {
                vec![-pitch / 2.0, pitch / 2.0]
            } else {
                vec![-pitch, 0.0, pitch]
            };
            for y in right_ys {
                pads.push(smd_roundrect(
                    &number.to_string(),
                    body_l / 2.0 + pad_l / 2.0,
                    y,
                    pad_l,
                    pad_w,
                    0.25,
                ));
                number += 1;
            }
        }
        _ => return Err("SOT pin count must be 3, 5, or 6".into()),
    }
    let (half_x, half_y) = bounds_from_pads(&pads, density.courtyard_excess());
    Ok(base_footprint(
        density,
        pads,
        body_l,
        body_w,
        half_x,
        half_y,
        format!(
            "SOT{count}P{pitch_mm}{density}",
            pitch_mm = pitch_mils(pitch),
            density = density.suffix()
        ),
    ))
}

fn soic(
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    let pitch = require_finite_positive(params.pitch, "pitch")?;
    let count = require_pin_count(params.pin_count, 4, 32, "pin count")?;
    if !count.is_multiple_of(2) {
        return Err("SOIC pin count must be even".into());
    }
    let per_side = count / 2;
    let body_w = require_finite_positive(params.body_width, "body width")?;
    let span = (per_side as f64 - 1.0) * pitch;
    let pad_l = pitch * 0.55;
    let pad_w = pitch * 0.35;
    let row_y = body_w / 2.0 + pad_w / 2.0;
    let mut pads = Vec::new();
    for i in 0..per_side {
        let x = -span / 2.0 + i as f64 * pitch;
        let num = (i + 1).to_string();
        pads.push(smd_roundrect(&num, x, -row_y, pad_l, pad_w, 0.25));
    }
    for i in 0..per_side {
        let x = span / 2.0 - i as f64 * pitch;
        let num = (per_side + i + 1).to_string();
        pads.push(smd_roundrect(&num, x, row_y, pad_l, pad_w, 0.25));
    }
    let body_l = span + pitch;
    let (half_x, half_y) = bounds_from_pads(&pads, density.courtyard_excess());
    Ok(base_footprint(
        density,
        pads,
        body_l,
        body_w,
        half_x,
        half_y,
        format!(
            "SOIC{pitch_mm}P{body}-{count}{density}",
            pitch_mm = pitch_mils(pitch),
            body = (body_w * 100.0).round() as u32,
            count = count,
            density = density.suffix()
        ),
    ))
}

fn qfp(
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    let pitch = require_finite_positive(params.pitch, "pitch")?;
    let count = require_pin_count(params.pin_count, 16, 208, "pin count")?;
    if !count.is_multiple_of(4) {
        return Err("QFP pin count must be a multiple of four".into());
    }
    let per_side = count / 4;
    let body_l = require_finite_positive(params.body_length, "body length")?;
    let body_w = require_finite_positive(params.body_width, "body width")?;
    let pad_l = pitch * 0.55;
    let pad_w = pitch * 0.30;
    let pads = gullwing_pads(per_side, pitch, body_l, body_w, pad_l, pad_w);
    let (half_x, half_y) = bounds_from_pads(&pads, density.courtyard_excess());
    Ok(base_footprint(
        density,
        pads,
        body_l,
        body_w,
        half_x,
        half_y,
        format!(
            "QFP{pitch_mm}-{count}{density}",
            pitch_mm = pitch_mils(pitch),
            count = count,
            density = density.suffix()
        ),
    ))
}

fn qfn(
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    let pitch = require_finite_positive(params.pitch, "pitch")?;
    let count = require_pin_count(params.pin_count, 8, 128, "pin count")?;
    if !count.is_multiple_of(4) {
        return Err("QFN pin count must be a multiple of four".into());
    }
    let per_side = count / 4;
    let body_l = require_finite_positive(params.body_length, "body length")?;
    let body_w = require_finite_positive(params.body_width, "body width")?;
    let pad_l = pitch * 0.45;
    let pad_w = pitch * 0.25;
    let mut pads = gullwing_pads(per_side, pitch, body_l, body_w, pad_l, pad_w);
    let thermal = if params.thermal_size > 0.0 {
        require_finite_positive(params.thermal_size, "thermal pad size")?
    } else {
        body_l.min(body_w) * 0.5
    };
    pads.push(smd_rect("EP", 0.0, 0.0, thermal, thermal));
    let (half_x, half_y) = bounds_from_pads(&pads, density.courtyard_excess());
    Ok(base_footprint(
        density,
        pads,
        body_l,
        body_w,
        half_x,
        half_y,
        format!(
            "QFN{pitch_mm}X{body_l}X{body_w}-{count}{density}",
            pitch_mm = pitch_mils(pitch),
            body_l = (body_l * 100.0).round() as u32,
            body_w = (body_w * 100.0).round() as u32,
            count = count,
            density = density.suffix()
        ),
    ))
}

fn bga(
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    let pitch = require_finite_positive(params.pitch, "pitch")?;
    let rows = require_pin_count(params.grid_rows.max(1), 1, 40, "grid rows")?;
    let cols = require_pin_count(params.grid_cols.max(1), 1, 40, "grid columns")?;
    let ball = if params.lead_diameter > 0.0 {
        require_finite_positive(params.lead_diameter, "ball diameter")?
    } else {
        pitch * 0.6
    };
    let span_x = (cols as f64 - 1.0) * pitch;
    let span_y = (rows as f64 - 1.0) * pitch;
    let mut pads = Vec::new();
    let mut number = 1_u32;
    for row in 0..rows {
        for col in 0..cols {
            let x = -span_x / 2.0 + col as f64 * pitch;
            let y = -span_y / 2.0 + row as f64 * pitch;
            pads.push(smd_circle(&number.to_string(), x, y, ball));
            number += 1;
        }
    }
    let body_l = span_x + pitch;
    let body_w = span_y + pitch;
    let excess = density.courtyard_excess().max(1.0);
    let (half_x, half_y) = bounds_from_pads(&pads, excess);
    Ok(base_footprint(
        density,
        pads,
        body_l,
        body_w,
        half_x,
        half_y,
        format!(
            "BGA{pitch_mm}_{cols}x{rows}{density}",
            pitch_mm = pitch_mils(pitch),
            cols = cols,
            rows = rows,
            density = density.suffix()
        ),
    ))
}

fn dip(
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    let pitch = require_finite_positive(params.pitch, "pitch")?;
    let count = require_pin_count(params.pin_count, 4, 64, "pin count")?;
    if !count.is_multiple_of(2) {
        return Err("DIP pin count must be even".into());
    }
    let row_spacing = if params.row_spacing > 0.0 {
        require_finite_positive(params.row_spacing, "row spacing")?
    } else {
        7.62
    };
    let per_row = count / 2;
    let span = (per_row as f64 - 1.0) * pitch;
    let pad = 1.6;
    let drill = if params.lead_diameter > 0.0 {
        require_finite_positive(params.lead_diameter, "lead diameter")?
    } else {
        0.8
    };
    let mut pads = Vec::new();
    for i in 0..per_row {
        let x = -span / 2.0 + i as f64 * pitch;
        pads.push(th_pad(
            &(i + 1).to_string(),
            x,
            -row_spacing / 2.0,
            pad,
            drill,
        ));
    }
    for i in 0..per_row {
        let x = span / 2.0 - i as f64 * pitch;
        pads.push(th_pad(
            &(per_row + i + 1).to_string(),
            x,
            row_spacing / 2.0,
            pad,
            drill,
        ));
    }
    let body_l = span + pitch;
    let body_w = row_spacing;
    let (half_x, half_y) = bounds_from_pads(&pads, density.courtyard_excess());
    Ok(base_footprint(
        density,
        pads,
        body_l,
        body_w,
        half_x,
        half_y,
        format!(
            "DIP-{count}_W{row}{density}",
            count = count,
            row = (row_spacing * 100.0).round() as u32,
            density = density.suffix()
        ),
    ))
}

fn header_single(
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    let pitch = require_finite_positive(params.pitch, "pitch")?;
    let count = require_pin_count(params.pin_count, 1, 40, "pin count")?;
    let span = (count as f64 - 1.0) * pitch;
    let pad = 1.7;
    let drill = if params.lead_diameter > 0.0 {
        require_finite_positive(params.lead_diameter, "lead diameter")?
    } else {
        1.0
    };
    let mut pads = Vec::new();
    for i in 0..count {
        let x = -span / 2.0 + i as f64 * pitch;
        pads.push(th_pad(&(i + 1).to_string(), x, 0.0, pad, drill));
    }
    let body_l = span + pitch;
    let body_w = pitch * 2.0;
    let (half_x, half_y) = bounds_from_pads(&pads, density.courtyard_excess());
    Ok(base_footprint(
        density,
        pads,
        body_l,
        body_w,
        half_x,
        half_y,
        format!(
            "HDR1X{count}{density}",
            count = count,
            density = density.suffix()
        ),
    ))
}

fn header_double(
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    let pitch = require_finite_positive(params.pitch, "pitch")?;
    let count = require_pin_count(params.pin_count, 2, 80, "pin count")?;
    if !count.is_multiple_of(2) {
        return Err("double-row header pin count must be even".into());
    }
    let per_row = count / 2;
    let row_spacing = if params.row_spacing > 0.0 {
        require_finite_positive(params.row_spacing, "row spacing")?
    } else {
        2.54
    };
    let span = (per_row as f64 - 1.0) * pitch;
    let pad = 1.7;
    let drill = if params.lead_diameter > 0.0 {
        require_finite_positive(params.lead_diameter, "lead diameter")?
    } else {
        1.0
    };
    let mut pads = Vec::new();
    for i in 0..per_row {
        let x = -span / 2.0 + i as f64 * pitch;
        pads.push(th_pad(
            &(i + 1).to_string(),
            x,
            -row_spacing / 2.0,
            pad,
            drill,
        ));
    }
    for i in 0..per_row {
        let x = -span / 2.0 + i as f64 * pitch + pitch / 2.0;
        pads.push(th_pad(
            &(per_row + i + 1).to_string(),
            x,
            row_spacing / 2.0,
            pad,
            drill,
        ));
    }
    let body_l = span + pitch;
    let body_w = row_spacing + pitch;
    let (half_x, half_y) = bounds_from_pads(&pads, density.courtyard_excess());
    Ok(base_footprint(
        density,
        pads,
        body_l,
        body_w,
        half_x,
        half_y,
        format!(
            "HDR2X{count}{density}",
            count = count,
            density = density.suffix()
        ),
    ))
}

fn radial(
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    let pitch = require_finite_positive(params.pitch, "pitch")?;
    let body_l = require_finite_positive(params.body_length, "body length")?;
    let body_w = require_finite_positive(params.body_width, "body width")?;
    let pad = 1.8;
    let drill = if params.lead_diameter > 0.0 {
        require_finite_positive(params.lead_diameter, "lead diameter")?
    } else {
        0.8
    };
    let half = pitch / 2.0;
    let pads = vec![
        th_pad("1", -half, 0.0, pad, drill),
        th_pad("2", half, 0.0, pad, drill),
    ];
    let (half_x, half_y) = bounds_from_pads(&pads, density.courtyard_excess());
    Ok(base_footprint(
        density,
        pads,
        body_l,
        body_w,
        half_x,
        half_y,
        format!(
            "RAD{pitch_mm}{density}",
            pitch_mm = pitch_mils(pitch),
            density = density.suffix()
        ),
    ))
}

fn axial(
    density: LandPatternDensity,
    params: &LandPatternParams,
) -> Result<FootprintDefinition, String> {
    let pitch = require_finite_positive(params.pitch, "pitch")?;
    let body_l = require_finite_positive(params.body_length, "body length")?;
    let body_w = require_finite_positive(params.body_width, "body width")?;
    let pad = 1.8;
    let drill = if params.lead_diameter > 0.0 {
        require_finite_positive(params.lead_diameter, "lead diameter")?
    } else {
        0.8
    };
    let half = pitch / 2.0;
    let pads = vec![
        th_pad("1", -half, 0.0, pad, drill),
        th_pad("2", half, 0.0, pad, drill),
    ];
    let (half_x, half_y) = bounds_from_pads(&pads, density.courtyard_excess());
    Ok(base_footprint(
        density,
        pads,
        body_l,
        body_w,
        half_x,
        half_y,
        format!(
            "AXL{pitch_mm}{density}",
            pitch_mm = pitch_mils(pitch),
            density = density.suffix()
        ),
    ))
}

fn gullwing_pads(
    per_side: u32,
    pitch: f64,
    body_l: f64,
    body_w: f64,
    pad_l: f64,
    pad_w: f64,
) -> Vec<FootprintPad> {
    let mut pads = Vec::new();
    let mut number = 1_u32;
    let span = (per_side as f64 - 1.0) * pitch;
    let off_y = body_w / 2.0 + pad_w / 2.0;
    let off_x = body_l / 2.0 + pad_w / 2.0;
    let along = |i: u32| {
        if per_side <= 1 {
            0.0
        } else {
            -span / 2.0 + i as f64 * pitch
        }
    };
    for i in 0..per_side {
        pads.push(smd_roundrect(
            &number.to_string(),
            along(i),
            -off_y,
            pad_l,
            pad_w,
            0.25,
        ));
        number += 1;
    }
    for i in 0..per_side {
        pads.push(smd_roundrect(
            &number.to_string(),
            off_x,
            along(i),
            pad_w,
            pad_l,
            0.25,
        ));
        number += 1;
    }
    for i in 0..per_side {
        pads.push(smd_roundrect(
            &number.to_string(),
            along(per_side - 1 - i),
            off_y,
            pad_l,
            pad_w,
            0.25,
        ));
        number += 1;
    }
    for i in 0..per_side {
        pads.push(smd_roundrect(
            &number.to_string(),
            -off_x,
            along(per_side - 1 - i),
            pad_w,
            pad_l,
            0.25,
        ));
        number += 1;
    }
    pads
}

fn base_footprint(
    density: LandPatternDensity,
    pads: Vec<FootprintPad>,
    body_l: f64,
    body_w: f64,
    half_x: f64,
    half_y: f64,
    suggested_name: String,
) -> FootprintDefinition {
    let stroke = density.silk_stroke();
    let fab_half_l = body_l / 2.0;
    let fab_half_w = body_w / 2.0;
    let pad_reach = max_pad_reach(&pads);
    let silk_y = pad_reach + 0.25 + stroke;
    let pin1_x = -fab_half_l - stroke * 4.0;
    let pin1_y = -fab_half_w - stroke * 4.0;
    let excess = density.courtyard_excess();
    let art_margin = stroke * 6.0;
    let half_x = snap_grid(
        half_x
            .max(pad_reach + excess)
            .max(fab_half_l + art_margin + excess),
    );
    let half_y = snap_grid(
        half_y
            .max(pad_reach + excess)
            .max(fab_half_w + art_margin + excess)
            .max(silk_y + stroke + excess),
    );
    let mut graphics = vec![
        Graphic::Rectangle {
            start: Point::new(-half_x, -half_y),
            end: Point::new(half_x, half_y),
            width: 0.1,
            fill: String::new(),
            layer: COURTYARD_LAYER.to_owned(),
        },
        Graphic::Rectangle {
            start: Point::new(-fab_half_l, -fab_half_w),
            end: Point::new(fab_half_l, fab_half_w),
            width: 0.1,
            fill: String::new(),
            layer: FAB_LAYER.to_owned(),
        },
        Graphic::Line {
            start: Point::new(-fab_half_l * 0.5, -silk_y),
            end: Point::new(fab_half_l * 0.5, -silk_y),
            width: stroke,
            layer: SILK_LAYER.to_owned(),
        },
        Graphic::Line {
            start: Point::new(-fab_half_l * 0.5, silk_y),
            end: Point::new(fab_half_l * 0.5, silk_y),
            width: stroke,
            layer: SILK_LAYER.to_owned(),
        },
        Graphic::Circle {
            center: Point::new(pin1_x, pin1_y),
            radius: stroke * 1.5,
            width: stroke,
            fill: String::new(),
            layer: SILK_LAYER.to_owned(),
        },
    ];
    if suggested_name.len() <= 256 {
        graphics.push(Graphic::Text {
            position: Point::new(0.0, fab_half_w + stroke * 4.0),
            angle: 0.0,
            text: suggested_name,
            size: Point::new(0.5, 0.5),
            layer: FAB_LAYER.to_owned(),
        });
    }
    FootprintDefinition {
        name: String::new(),
        description: "Generated land pattern".into(),
        keywords: String::new(),
        pads,
        graphics,
        model: None,
        rules: None,
        raw_source: String::new(),
        unsupported: vec![],
    }
}

fn max_pad_reach(pads: &[FootprintPad]) -> f64 {
    let mut reach = 0.0_f64;
    for pad in pads {
        let hx = pad.size.x / 2.0;
        let hy = pad.size.y / 2.0;
        let (c, s) = (
            pad.rotation.to_radians().cos(),
            pad.rotation.to_radians().sin(),
        );
        let reach_x = (hx * c).abs() + (hy * s).abs();
        let reach_y = (hx * s).abs() + (hy * c).abs();
        reach = reach.max(pad.position.y.abs() + reach_y);
        reach = reach.max(pad.position.x.abs() + reach_x);
    }
    reach
}

fn bounds_from_pads(pads: &[FootprintPad], excess: f64) -> (f64, f64) {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for pad in pads {
        let hx = pad.size.x / 2.0;
        let hy = pad.size.y / 2.0;
        let (c, s) = (
            pad.rotation.to_radians().cos(),
            pad.rotation.to_radians().sin(),
        );
        let reach_x = (hx * c).abs() + (hy * s).abs();
        let reach_y = (hx * s).abs() + (hy * c).abs();
        min_x = min_x.min(pad.position.x - reach_x);
        min_y = min_y.min(pad.position.y - reach_y);
        max_x = max_x.max(pad.position.x + reach_x);
        max_y = max_y.max(pad.position.y + reach_y);
    }
    if min_x > max_x {
        return (1.0, 1.0);
    }
    (
        max_x.abs().max(min_x.abs()) + excess,
        max_y.abs().max(min_y.abs()) + excess,
    )
}

fn smd_roundrect(number: &str, x: f64, y: f64, w: f64, h: f64, ratio: f64) -> FootprintPad {
    FootprintPad {
        number: number.to_owned(),
        pad_type: "smd".into(),
        shape: "roundrect".into(),
        position: Point::new(x, y),
        rotation: 0.0,
        size: Point::new(w, h),
        layers: vec!["F.Cu".into(), "F.Mask".into(), "F.Paste".into()],
        drill: None,
        roundrect_ratio: Some(ratio),
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    }
}

fn smd_rect(number: &str, x: f64, y: f64, w: f64, h: f64) -> FootprintPad {
    FootprintPad {
        number: number.to_owned(),
        pad_type: "smd".into(),
        shape: "rect".into(),
        position: Point::new(x, y),
        rotation: 0.0,
        size: Point::new(w, h),
        layers: vec!["F.Cu".into(), "F.Mask".into(), "F.Paste".into()],
        drill: None,
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    }
}

fn smd_circle(number: &str, x: f64, y: f64, diameter: f64) -> FootprintPad {
    FootprintPad {
        number: number.to_owned(),
        pad_type: "smd".into(),
        shape: "circle".into(),
        position: Point::new(x, y),
        rotation: 0.0,
        size: Point::new(diameter, diameter),
        layers: vec!["F.Cu".into(), "F.Mask".into(), "F.Paste".into()],
        drill: None,
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    }
}

fn th_pad(number: &str, x: f64, y: f64, size: f64, drill: f64) -> FootprintPad {
    FootprintPad {
        number: number.to_owned(),
        pad_type: "thru_hole".into(),
        shape: "circle".into(),
        position: Point::new(x, y),
        rotation: 0.0,
        size: Point::new(size, size),
        layers: vec!["*.Cu".into(), "*.Mask".into()],
        drill: Some(Drill {
            size: Point::new(drill, drill),
            offset: Point::default(),
        }),
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    }
}

fn metric_code(body_l: f64, body_w: f64) -> String {
    let l = (body_l * 100.0).round() as u32;
    let w = (body_w * 100.0).round() as u32;
    format!("{l:04}{w:04}")
}

fn pitch_mils(pitch: f64) -> String {
    let mils = (pitch / 0.0254).round() as u32;
    format!("{mils}")
}

fn snap_grid(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    (value / GRID_MM).round() * GRID_MM
}
