//! CSV and JSON export of the detailed pin map.
//!
//! Renders the pin map to generic text formats suitable for firmware
//! handoff. Parts with no known driver are listed as unmapped.
//!
//! # Rules
//! - CSV fields containing commas, quotes or newlines are quoted.
//! - JSON is pretty-printed with `rows` and `unmapped` keys.
//! - Output size is capped at 8 MiB.
//!
//! # Dependencies
//! - crates/plugin/src/pin_map_detail.rs `PinDetail`
//! - `crates/plugin/Cargo.toml` (serde_json for JSON encoding)
//!
//! # Do not
//! - Mutate the project or pin detail.
use crate::pin_map_detail::PinDetail;
use serde_json::{Value, json};

const MAX_OUTPUT_BYTES: usize = 8_388_608;

fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        let escaped = value.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        value.to_string()
    }
}

/// Render a pin map to CSV text with header and unmapped parts.
///
/// # Rules
/// - Header is `pin,function,net,instance,bus,select,interrupt`.
/// - Rows are in the same order as the input.
/// - Unmapped parts are appended as `# unmapped: NAME` lines.
///
/// # Dependencies
/// - crates/plugin/src/pin_map_detail.rs
///
/// # Do not
/// - Mutate the input.
pub fn render_pin_map_csv(d: &PinDetail) -> String {
    let mut out = String::from("pin,function,net,instance,bus,select,interrupt\n");
    for row in &d.rows {
        if out.len() > MAX_OUTPUT_BYTES {
            break;
        }
        out.push_str(&csv_field(&row.pin));
        out.push(',');
        out.push_str(&csv_field(&row.function));
        out.push(',');
        out.push_str(&csv_field(&row.net));
        out.push(',');
        out.push_str(&csv_field(&row.instance));
        out.push(',');
        out.push_str(&csv_field(&row.bus));
        out.push(',');
        out.push_str(&csv_field(&row.select));
        out.push(',');
        out.push_str(&csv_field(&row.interrupt));
        out.push('\n');
    }
    for name in &d.unmapped {
        if out.len() > MAX_OUTPUT_BYTES {
            break;
        }
        out.push_str("# unmapped: ");
        out.push_str(name);
        out.push('\n');
    }
    out
}

/// Render a pin map to pretty-printed JSON with rows and unmapped keys.
///
/// # Rules
/// - Keys are `rows` (array of objects) and `unmapped` (array of strings).
/// - Rows preserve the input order.
///
/// # Dependencies
/// - `crates/plugin/Cargo.toml` (serde_json for JSON encoding)
///
/// # Do not
/// - Mutate the input.
pub fn render_pin_map_json(d: &PinDetail) -> Result<String, String> {
    let rows: Vec<Value> = d
        .rows
        .iter()
        .map(|row| {
            json!({
                "pin": row.pin,
                "function": row.function,
                "net": row.net,
                "instance": row.instance,
                "bus": row.bus,
                "select": row.select,
                "interrupt": row.interrupt,
            })
        })
        .collect();
    let doc = json!({
        "rows": rows,
        "unmapped": d.unmapped,
    });
    let text = serde_json::to_string_pretty(&doc)
        .map_err(|e| format!("json serialization failed: {e}"))?;
    if text.len() > MAX_OUTPUT_BYTES {
        return Err("pin map export output is too large".into());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pin_map_detail::{PinDetail, PinDetailRow, pin_map_detail};
    use pcbcraft_doc::{Component, EmbeddedFile, Net, Pin, Project};

    fn fixture_project() -> Project {
        let design_text = include_str!("../../library/tests/fixtures/firmware-design.toml");
        let mcu_text = include_str!("../../library/tests/fixtures/stm32f405-fixture.mcu.toml");
        let mut project = Project::default();
        project.files.push(EmbeddedFile {
            name: "firmware-design.toml".into(),
            data: design_text.into(),
            encoding: String::new(),
        });
        project.files.push(EmbeddedFile {
            name: "stm32f405-fixture.mcu.toml".into(),
            data: mcu_text.into(),
            encoding: String::new(),
        });
        project.components.push(Component {
            id: 1,
            reference: "U1".into(),
            value: "STM32F405".into(),
            kind: "ic_mcu".into(),
            position: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
            rotation: 0.0,
            pins: vec![
                Pin {
                    number: "PA5".into(),
                    name: "PA5".into(),
                    offset: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
                    net: Some(10),
                    electrical_type: "bidirectional".into(),
                    direction: None,
                    unit: 1,
                    appearances: Default::default(),
                },
                Pin {
                    number: "PA4".into(),
                    name: "PA4".into(),
                    offset: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
                    net: Some(11),
                    electrical_type: "input".into(),
                    direction: None,
                    unit: 1,
                    appearances: Default::default(),
                },
                Pin {
                    number: "PB12".into(),
                    name: "PB12".into(),
                    offset: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
                    net: Some(12),
                    electrical_type: "input".into(),
                    direction: None,
                    unit: 1,
                    appearances: Default::default(),
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
            compliance: Default::default(),
            part: None,
            sourcing: Default::default(),
            instance: Default::default(),
            prototype: Default::default(),
            include_in_bom: true,
            include_in_placement: true,
        });
        project.nets.push(Net {
            id: 10,
            name: "+5V".into(),
            ..Net::default()
        });
        project.nets.push(Net {
            id: 11,
            name: "GYRO_CS".into(),
            ..Net::default()
        });
        project.nets.push(Net {
            id: 12,
            name: "FLASH_CS".into(),
            ..Net::default()
        });
        project
    }

    #[test]
    fn pin_map_export_matches_golden() -> Result<(), String> {
        let project = fixture_project();
        let detail = pin_map_detail(&project)?;
        let csv = render_pin_map_csv(&detail);
        let expected = include_str!("../tests/fixtures/pin-map-detail/expected.csv");
        assert_eq!(csv, expected, "csv export does not match golden file");
        Ok(())
    }

    #[test]
    fn pin_map_json_has_the_same_rows() -> Result<(), String> {
        let project = fixture_project();
        let detail = pin_map_detail(&project)?;
        let json_text = render_pin_map_json(&detail)?;
        let doc: Value =
            serde_json::from_str(&json_text).map_err(|e| format!("json parse failed: {e}"))?;
        let rows = doc
            .get("rows")
            .and_then(Value::as_array)
            .ok_or("rows missing from json")?;
        assert_eq!(rows.len(), 10, "expected 10 rows in json");
        let first = rows
            .first()
            .ok_or("no first row")?
            .get("pin")
            .and_then(Value::as_str)
            .ok_or("pin missing from first row")?;
        assert_eq!(first, "PA10", "first row pin must be PA10");
        Ok(())
    }

    #[test]
    fn csv_quotes_a_comma() {
        let detail = PinDetail {
            rows: vec![PinDetailRow {
                pin: "PA1".into(),
                function: "A,B".into(),
                net: String::new(),
                instance: String::new(),
                bus: String::new(),
                select: String::new(),
                interrupt: String::new(),
            }],
            unmapped: vec![],
        };
        let csv = render_pin_map_csv(&detail);
        assert!(
            csv.contains("\"A,B\""),
            "function with comma must be quoted: {csv}"
        );
    }
}
