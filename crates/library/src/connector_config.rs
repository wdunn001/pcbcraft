//! Map part-number field selections to generator options.
//!
//! # Rules
//! - Field tokens are validated against series part-number tables before mapping.
//! - Generator options are validated against series generator tables before returning.
//!
//! # Dependencies
//! - crates/library/src/connector_partnumber.rs
//! - crates/library/src/connector_series_generate.rs
//! - crates/library/src/connector_series_data.rs
use crate::connector_partnumber::FieldError;
use crate::connector_series_data::series_spec;
use crate::connector_series_generate::ConnectorSeriesOptions;
use std::collections::BTreeMap;

/// Convert part-number field values to generator options.
///
/// # Rules
/// - TSW/SSW: pitch_positions token encodes pitch and positions; row codes S and D are supported.
/// - BTH: positions_per_row is an explicit code; lead codes 01 and 02 map to stack codes.
/// - Returns a list of [`FieldError`] values when any field is invalid for the series.
pub fn options_from_fields(
    series_id: &str,
    fields: &BTreeMap<String, String>,
) -> Result<ConnectorSeriesOptions, Vec<FieldError>> {
    let spec = series_spec(series_id).map_err(|e| {
        vec![FieldError {
            field: "series".into(),
            message: e,
            valid_values: vec!["TSW".into(), "SSW".into(), "BTH".into()],
        }]
    })?;

    match spec.series_id.as_str() {
        "TSW" | "SSW" => map_tsw_ssw(&spec, fields),
        "BTH" => map_bth(&spec, fields),
        _ => Err(vec![FieldError {
            field: "series".into(),
            message: format!("unsupported series {}", spec.series_id),
            valid_values: vec!["TSW".into(), "SSW".into(), "BTH".into()],
        }]),
    }
}

fn map_tsw_ssw(
    spec: &crate::connector_series_data::ConnectorSeriesSpec,
    fields: &BTreeMap<String, String>,
) -> Result<ConnectorSeriesOptions, Vec<FieldError>> {
    let pitch_positions = fields.get("pitch_positions").ok_or_else(|| {
        vec![FieldError {
            field: "pitch_positions".into(),
            message: "pitch and positions are required".into(),
            valid_values: vec!["1NN or 2NN".into()],
        }]
    })?;

    let positions = positions_from_pitch_token(pitch_positions)?;

    let row = fields.get("row").ok_or_else(|| {
        vec![FieldError {
            field: "row".into(),
            message: "row is required".into(),
            valid_values: vec!["S".into(), "D".into()],
        }]
    })?;

    let rows = match row.as_str() {
        "S" => 1,
        "D" => 2,
        _ => {
            return Err(vec![FieldError {
                field: "row".into(),
                message: format!("row {row} is not supported by the generator"),
                valid_values: vec!["S".into(), "D".into()],
            }]);
        }
    };

    let lead_code = fields.get("lead").ok_or_else(|| {
        vec![FieldError {
            field: "lead".into(),
            message: "lead style is required".into(),
            valid_values: spec.lead_codes.iter().map(|l| l.code.clone()).collect(),
        }]
    })?;

    // Validate lead code against generator table.
    if spec.lead_tail_mm(lead_code).is_err() {
        let valid = spec.lead_codes.iter().map(|l| l.code.clone()).collect();
        return Err(vec![FieldError {
            field: "lead".into(),
            message: format!("lead code {lead_code} has no generator table entry"),
            valid_values: valid,
        }]);
    }

    Ok(ConnectorSeriesOptions {
        positions_per_row: positions,
        rows,
        lead_code: lead_code.clone(),
        stack_code: None,
        alignment_posts: false,
    })
}

fn map_bth(
    spec: &crate::connector_series_data::ConnectorSeriesSpec,
    fields: &BTreeMap<String, String>,
) -> Result<ConnectorSeriesOptions, Vec<FieldError>> {
    let positions_code = fields.get("positions_per_row").ok_or_else(|| {
        vec![FieldError {
            field: "positions_per_row".into(),
            message: "positions are required".into(),
            valid_values: vec![
                "030".into(),
                "050".into(),
                "060".into(),
                "090".into(),
                "120".into(),
                "150".into(),
            ],
        }]
    })?;

    let positions = positions_from_bth_code(positions_code)?;

    // BTH is always dual row.
    let rows = 2;

    // BTH lead codes are stack codes in the generator table.
    let stack_code = fields.get("lead").ok_or_else(|| {
        vec![FieldError {
            field: "lead".into(),
            message: "stack height code is required".into(),
            valid_values: spec.stack_codes.iter().map(|s| s.code.clone()).collect(),
        }]
    })?;

    // Validate stack code against generator table.
    if spec.stack_height_mm(stack_code).is_err() {
        let valid = spec.stack_codes.iter().map(|s| s.code.clone()).collect();
        return Err(vec![FieldError {
            field: "lead".into(),
            message: format!("stack code {stack_code} has no generator table entry"),
            valid_values: valid,
        }]);
    }

    Ok(ConnectorSeriesOptions {
        positions_per_row: positions,
        rows,
        lead_code: String::new(),
        stack_code: Some(stack_code.clone()),
        alignment_posts: false,
    })
}

fn positions_from_pitch_token(token: &str) -> Result<u32, Vec<FieldError>> {
    if token.len() != 3 || !token.chars().all(|c| c.is_ascii_digit()) {
        return Err(vec![FieldError {
            field: "pitch_positions".into(),
            message: "must be a three-digit code".into(),
            valid_values: vec!["1NN or 2NN".into()],
        }]);
    }
    let positions: u32 = token[1..].parse().map_err(|_| {
        vec![FieldError {
            field: "pitch_positions".into(),
            message: "invalid positions digits".into(),
            valid_values: vec!["1NN or 2NN".into()],
        }]
    })?;
    Ok(positions)
}

fn positions_from_bth_code(code: &str) -> Result<u32, Vec<FieldError>> {
    // BTH part-number codes like "030" mean 30 positions total (dual row).
    // The generator expects positions_per_row, so divide by 2.
    let total: u32 = code.parse().map_err(|_| {
        vec![FieldError {
            field: "positions_per_row".into(),
            message: "invalid positions code".into(),
            valid_values: vec![
                "030".into(),
                "050".into(),
                "060".into(),
                "090".into(),
                "120".into(),
                "150".into(),
            ],
        }]
    })?;
    if !total.is_multiple_of(2) {
        return Err(vec![FieldError {
            field: "positions_per_row".into(),
            message: "BTH positions must be even for dual row".into(),
            valid_values: vec![
                "030".into(),
                "050".into(),
                "060".into(),
                "090".into(),
                "120".into(),
                "150".into(),
            ],
        }]);
    }
    Ok(total / 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector_series_generate::generate_connector_part;

    fn fields_tsw() -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert("pitch_positions".into(), "110".into());
        m.insert("lead".into(), "07".into());
        m.insert("row".into(), "S".into());
        m
    }

    fn fields_tsw_double() -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert("pitch_positions".into(), "110".into());
        m.insert("lead".into(), "07".into());
        m.insert("row".into(), "D".into());
        m
    }

    #[test]
    fn samtec_fields_map_to_generator_options() -> Result<(), String> {
        let opts = options_from_fields("TSW", &fields_tsw()).map_err(|e| format!("{e:?}"))?;
        assert_eq!(opts.positions_per_row, 10);
        assert_eq!(opts.rows, 1);
        assert_eq!(opts.lead_code, "07");
        assert!(opts.stack_code.is_none());

        let generated = generate_connector_part("TSW", &opts).map_err(|e| format!("{e:?}"))?;
        assert!(generated.part_number.starts_with("TSW-"));
        Ok(())
    }

    #[test]
    fn tsw_double_row_maps() -> Result<(), String> {
        let opts =
            options_from_fields("TSW", &fields_tsw_double()).map_err(|e| format!("{e:?}"))?;
        assert_eq!(opts.rows, 2);
        Ok(())
    }

    #[test]
    fn tsw_triple_row_errors() -> Result<(), String> {
        let mut m = fields_tsw();
        m.insert("row".into(), "T".into());
        let Err(err) = options_from_fields("TSW", &m) else {
            return Err("a triple row is refused".into());
        };
        assert_eq!(err[0].field, "row");
        assert!(err[0].valid_values.contains(&"S".into()));
        assert!(err[0].valid_values.contains(&"D".into()));
        Ok(())
    }

    #[test]
    fn tsw_bad_lead_errors() -> Result<(), String> {
        let mut m = fields_tsw();
        m.insert("lead".into(), "05".into());
        let Err(err) = options_from_fields("TSW", &m) else {
            return Err("a bad lead code is refused".into());
        };
        assert_eq!(err[0].field, "lead");
        assert!(err[0].valid_values.contains(&"07".into()));
        assert!(err[0].valid_values.contains(&"14".into()));
        Ok(())
    }

    #[test]
    fn bth_fields_map() -> Result<(), String> {
        let mut m = BTreeMap::new();
        m.insert("positions_per_row".into(), "060".into());
        m.insert("lead".into(), "01".into());
        m.insert("row".into(), "D".into());
        let opts = options_from_fields("BTH", &m).map_err(|e| format!("{e:?}"))?;
        assert_eq!(opts.positions_per_row, 30);
        assert_eq!(opts.rows, 2);
        assert_eq!(opts.stack_code, Some("01".into()));

        let generated = generate_connector_part("BTH", &opts).map_err(|e| format!("{e:?}"))?;
        assert!(generated.part_number.starts_with("BTH-"));
        Ok(())
    }
}
