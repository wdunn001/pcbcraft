//! First-slice Samtec series generators: footprint, symbol, and parametric body.
use crate::connector_parametric::{
    build_parametric_spec, housing_length_mm, ConnectorParametricSpec,
};
use crate::connector_series_data::{series_spec, ConnectorSeriesSpec};
use crate::footprint_generate::{generate_footprint, LandPatternDensity, LandPatternParams};
use crate::online_import::generic_symbol_for;
use crate::{FootprintDefinition, Model3D, SymbolDefinition};
use std::fmt::Write as _;

/// Bounded configuration for one generated connector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectorSeriesOptions {
    pub positions_per_row: u32,
    pub rows: u32,
    pub lead_code: String,
    pub stack_code: Option<String>,
    pub alignment_posts: bool,
}

/// One field error with valid alternatives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectorFieldError {
    pub field: String,
    pub message: String,
    pub valid_values: Vec<String>,
}

impl ConnectorFieldError {
    fn new(field: &str, message: impl Into<String>, valid: Vec<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            valid_values: valid,
        }
    }
}

/// Footprint, symbol, model, and suggested part number for a valid configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedConnectorPart {
    pub part_number: String,
    pub footprint: FootprintDefinition,
    pub symbol: SymbolDefinition,
    pub model: Model3D,
    pub parametric: ConnectorParametricSpec,
    pub mated_stack_height_mm: Option<f64>,
}

fn validate_options(
    spec: &ConnectorSeriesSpec,
    options: &ConnectorSeriesOptions,
) -> Result<(), Vec<ConnectorFieldError>> {
    let mut errors = Vec::new();
    let lim = &spec.limits;
    if options.positions_per_row < lim.positions_per_row_min
        || options.positions_per_row > lim.positions_per_row_max
    {
        errors.push(ConnectorFieldError::new(
            "positions_per_row",
            format!(
                "must be between {} and {}",
                lim.positions_per_row_min, lim.positions_per_row_max
            ),
            vec![],
        ));
    }
    if options.rows < lim.rows_min || options.rows > lim.rows_max {
        errors.push(ConnectorFieldError::new(
            "rows",
            format!("must be between {} and {}", lim.rows_min, lim.rows_max),
            vec!["1".into(), "2".into()],
        ));
    }
    if spec.geometry.land_pattern_family == "smt_dual_row" && options.rows != 2 {
        errors.push(ConnectorFieldError::new(
            "rows",
            "BTH is always dual row".into(),
            vec!["2".into()],
        ));
    }
    if spec.series_id == "BTH" {
        if options.stack_code.as_deref().unwrap_or("").is_empty() {
            errors.push(ConnectorFieldError::new(
                "stack_code",
                "is required for BTH".into(),
                spec.stack_codes
                    .iter()
                    .map(|s| s.code.clone())
                    .collect(),
            ));
        } else if spec
            .stack_height_mm(options.stack_code.as_deref().unwrap_or(""))
            .is_err()
        {
            errors.push(ConnectorFieldError::new(
                "stack_code",
                "is not a valid stack code".into(),
                spec.stack_codes
                    .iter()
                    .map(|s| s.code.clone())
                    .collect(),
            ));
        }
    } else if spec.lead_tail_mm(&options.lead_code).is_err() {
        errors.push(ConnectorFieldError::new(
            "lead_code",
            "is not a valid lead code".into(),
            spec.lead_codes.iter().map(|l| l.code.clone()).collect(),
        ));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(())
}

fn format_part_number(spec: &ConnectorSeriesSpec, options: &ConnectorSeriesOptions) -> String {
    let tmpl = if options.rows <= 1 {
        &spec.part_number_template.single_row
    } else {
        &spec.part_number_template.dual_row
    };
    let positions_total = options.positions_per_row * options.rows;
    let mut out = tmpl.clone();
    out = out.replace("{positions:03}", &format!("{:03}", options.positions_per_row));
    out = out.replace("{positions_total:03}", &format!("{:03}", positions_total));
    out = out.replace("{lead}", &options.lead_code);
    if let Some(stack) = &options.stack_code {
        out = out.replace("{stack}", stack);
    }
    out
}

fn footprint_params(
    spec: &ConnectorSeriesSpec,
    options: &ConnectorSeriesOptions,
    part_number: &str,
) -> LandPatternParams {
    let g = &spec.geometry;
    let pin_count = options.positions_per_row * options.rows;
    let mut params = LandPatternParams {
        name: part_number.to_owned(),
        description: String::new(),
        keywords: format!(
            "connector {} {}",
            spec.manufacturer.to_ascii_lowercase(),
            spec.series_id
        ),
        pitch: g.pitch_mm,
        pin_count,
        body_length: g.smt_pad_length_mm,
        body_width: g.smt_pad_width_mm,
        row_spacing: g.row_spacing_mm,
        lead_width: g.th_pad_mm.max(g.smt_pad_width_mm),
        lead_diameter: g.th_drill_mm,
        ..LandPatternParams::default()
    };
    if g.land_pattern_family == "smt_dual_row" {
        params.body_length = g.smt_pad_length_mm;
        params.lead_width = g.smt_pad_width_mm;
    }
    params
}

fn land_pattern_family(spec: &ConnectorSeriesSpec, rows: u32) -> &str {
    if rows <= 1 {
        spec.geometry.land_pattern_family.as_str()
    } else if spec.geometry.land_pattern_family == "smt_dual_row" {
        "smt_dual_row"
    } else {
        spec.geometry
            .dual_row_land_pattern_family
            .as_str()
    }
}

fn append_alignment_posts(
    fp: &mut FootprintDefinition,
    spec: &ConnectorSeriesSpec,
    options: &ConnectorSeriesOptions,
) {
    if !options.alignment_posts || spec.series_id != "BTH" {
        return;
    }
    let g = &spec.geometry;
    let span = housing_length_mm(options.positions_per_row, g.pitch_mm);
    let half = span / 2.0 + g.pitch_mm;
    let y = 0.0;
    let pad = g.pitch_mm * 2.4;
    let drill = g.pitch_mm * 1.6;
    for (n, x) in [("AP1", -half), ("AP2", half)] {
        fp.pads.push(crate::FootprintPad {
            number: n.into(),
            pad_type: "unplated".into(),
            shape: "circle".into(),
            position: pcbcraft_geom::Point::new(x, y),
            rotation: 0.0,
            size: pcbcraft_geom::Point::default(),
            layers: vec!["F.Mask".into()],
            drill: Some(crate::Drill {
                size: pcbcraft_geom::Point::new(drill, drill),
                offset: pcbcraft_geom::Point::default(),
            }),
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        });
    }
}

fn description_for(spec: &ConnectorSeriesSpec) -> String {
    let mut text = String::new();
    let _ = writeln!(&mut text, "{}", spec.verify_note);
    let _ = writeln!(&mut text, "Drawing: {}", spec.drawing_url);
    text
}

/// Build footprint, symbol, and parametric model for a first-slice series.
pub fn generate_connector_part(
    series_id: &str,
    options: &ConnectorSeriesOptions,
) -> Result<GeneratedConnectorPart, Vec<ConnectorFieldError>> {
    let spec = series_spec(series_id).map_err(|e| {
        vec![ConnectorFieldError::new("series_id", e, first_slice_ids())]
    })?;
    validate_options(&spec, options)?;
    let part_number = format_part_number(&spec, options);
    let family = land_pattern_family(&spec, options.rows);
    let params = footprint_params(&spec, options, &part_number);
    let mut footprint = generate_footprint(family, LandPatternDensity::Nominal, &params)
        .map_err(|e| vec![ConnectorFieldError::new("geometry", e, vec![])])?;
    append_alignment_posts(&mut footprint, &spec, options);
    footprint.description = description_for(&spec);
    footprint.validate().map_err(|e| {
        vec![ConnectorFieldError::new("footprint", e.to_string(), vec![])]
    })?;

    let (tail_mm, mated_stack) = if spec.series_id == "BTH" {
        let code = options.stack_code.as_deref().unwrap_or("");
        let (stack, gap) = spec.stack_height_mm(code).map_err(|e| {
            vec![ConnectorFieldError::new("stack_code", e, vec![])]
        })?;
        (gap, Some(stack))
    } else {
        let tail = spec.lead_tail_mm(&options.lead_code).map_err(|e| {
            vec![ConnectorFieldError::new("lead_code", e, vec![])]
        })?;
        (tail, None)
    };

    let parametric = build_parametric_spec(
        &spec,
        options.rows,
        options.positions_per_row,
        tail_mm,
        mated_stack.unwrap_or(0.0),
    );
    let model = ConnectorParametricSpec::model_attachment(&parametric).map_err(|e| {
        vec![ConnectorFieldError::new("model", e, vec![])]
    })?;
    footprint.model = Some(model.clone());

    let symbol = generic_symbol_for(&footprint).map_err(|e| {
        vec![ConnectorFieldError::new("symbol", e.to_string(), vec![])]
    })?;

    Ok(GeneratedConnectorPart {
        part_number,
        footprint,
        symbol,
        model,
        parametric,
        mated_stack_height_mm: mated_stack,
    })
}

fn first_slice_ids() -> Vec<String> {
    crate::connector_series_data::first_slice_series_ids()
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// Pin 1 centre in footprint coordinates for the documented corner (left row, first position).
pub fn pin1_position(fp: &FootprintDefinition) -> Option<(f64, f64)> {
    let pad = fp.pads.iter().find(|p| p.number == "1")?;
    Some((pad.position.x, pad.position.y))
}
