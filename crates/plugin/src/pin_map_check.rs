//! Reverse check of an imported pin map against the design.
//!
//! Parses a CSV pin map (the same format exported by `render_pin_map_csv`)
//! and compares it to the design's pin assignments. Reports pins given a
//! function they do not have, swapped pins, and pins missing from the import.
//!
//! # Rules
//! - Refuses text over 1 MiB or more than 4096 lines.
//! - Lines starting with `#` are skipped.
//! - Pin and function columns are located by header name.
//! - Swapped pins are detected when two pins have each other's functions.
//!
//! # Dependencies
//! - crates/plugin/src/pin_map_detail.rs `PinDetail`
//!
//! # Do not
//! - Mutate the input.
use crate::pin_map_detail::PinDetail;

/// A mismatch between an imported pin map and the design.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinMapMismatch {
    /// The pin name.
    pub pin: String,
    /// The function the design expects on this pin.
    pub expected: String,
    /// The function the import gives this pin, or empty when missing.
    pub found: String,
    /// When this pin is swapped with another, that pin's name.
    pub swapped_with: Option<String>,
}

const MAX_TEXT_BYTES: usize = 1_048_576;
const MAX_LINES: usize = 4096;

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in line.chars() {
        if in_quotes {
            if ch == '"' {
                in_quotes = false;
            } else {
                current.push(ch);
            }
        } else {
            match ch {
                '"' => in_quotes = true,
                ',' => {
                    fields.push(current);
                    current = String::new();
                }
                _ => current.push(ch),
            }
        }
    }
    fields.push(current);
    fields
}

/// Check an imported pin map CSV against the design's pin assignments.
///
/// Returns a list of mismatches sorted by pin name. A mismatch is emitted when
/// a pin in the import has a different function than the design, or when a
/// design pin is missing from the import. Swapped pins are detected when two
/// pins have each other's functions.
///
/// # Rules
/// - The CSV header must contain `pin` and `function` columns.
/// - Lines starting with `#` are skipped.
/// - Text over 1 MiB or more than 4096 lines is rejected.
///
/// # Dependencies
/// - crates/plugin/src/pin_map_detail.rs `PinDetail`
///
/// # Do not
/// - Mutate the inputs.
pub fn check_pin_map_text(d: &PinDetail, csv: &str) -> Result<Vec<PinMapMismatch>, String> {
    if csv.len() > MAX_TEXT_BYTES {
        return Err("imported pin map is too large".into());
    }
    let lines: Vec<&str> = csv.lines().collect();
    if lines.len() > MAX_LINES {
        return Err("imported pin map has too many lines".into());
    }
    let Some(header_line) = lines.first() else {
        return Err("imported pin map is empty".into());
    };
    let header = parse_csv_line(header_line);
    let pin_col = header
        .iter()
        .position(|h| h.trim() == "pin")
        .ok_or("imported pin map has no pin column")?;
    let func_col = header
        .iter()
        .position(|h| h.trim() == "function")
        .ok_or("imported pin map has no function column")?;
    let mut imported = std::collections::BTreeMap::new();
    for line in &lines[1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let fields = parse_csv_line(line);
        if fields.len() <= pin_col || fields.len() <= func_col {
            continue;
        }
        let pin = fields[pin_col].trim().to_string();
        let func = fields[func_col].trim().to_string();
        if !pin.is_empty() {
            imported.insert(pin, func);
        }
    }
    let mut mismatches = Vec::new();
    let mut design_map = std::collections::BTreeMap::new();
    for row in &d.rows {
        design_map.insert(row.pin.clone(), row.function.clone());
    }
    for (pin, expected) in &design_map {
        match imported.get(pin.as_str()) {
            Some(found) if found == expected => {}
            Some(found) => {
                mismatches.push(PinMapMismatch {
                    pin: pin.clone(),
                    expected: expected.clone(),
                    found: found.clone(),
                    swapped_with: None,
                });
            }
            None => {
                mismatches.push(PinMapMismatch {
                    pin: pin.clone(),
                    expected: expected.clone(),
                    found: String::new(),
                    swapped_with: None,
                });
            }
        }
    }
    for (pin, func) in &imported {
        if !design_map.contains_key(pin.as_str()) {
            mismatches.push(PinMapMismatch {
                pin: pin.clone(),
                expected: String::new(),
                found: func.clone(),
                swapped_with: None,
            });
        }
    }
    // Detect swapped pins: two pins where each has the other's function.
    let mut swap_pairs = std::collections::BTreeMap::new();
    for m in &mismatches {
        if m.found.is_empty() {
            continue;
        }
        for other in &mismatches {
            if other.pin == m.pin {
                continue;
            }
            if other.found == m.expected && m.found == other.expected {
                swap_pairs.insert(m.pin.clone(), other.pin.clone());
            }
        }
    }
    for m in &mut mismatches {
        if let Some(swapped) = swap_pairs.get(&m.pin) {
            m.swapped_with = Some(swapped.clone());
        }
    }
    mismatches.sort_by(|a, b| a.pin.cmp(&b.pin));
    Ok(mismatches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pin_map_detail::{PinDetail, PinDetailRow, pin_map_detail};
    use crate::pin_map_export::render_pin_map_csv;
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
    fn reverse_check_accepts_the_exported_file() -> Result<(), String> {
        let project = fixture_project();
        let detail = pin_map_detail(&project)?;
        let csv = render_pin_map_csv(&detail);
        let mismatches = check_pin_map_text(&detail, &csv)?;
        assert!(
            mismatches.is_empty(),
            "exported file should match design: {:?}",
            mismatches
        );
        Ok(())
    }

    #[test]
    fn reverse_check_reports_swapped_pin() -> Result<(), String> {
        let project = fixture_project();
        let detail = pin_map_detail(&project)?;
        let csv = render_pin_map_csv(&detail);
        let swapped = csv
            .replace("PA5,SPI1_SCK", "PA5,SPI1_MISO")
            .replace("PA6,SPI1_MISO", "PA6,SPI1_SCK");
        let mismatches = check_pin_map_text(&detail, &swapped)?;
        let pa5 = mismatches
            .iter()
            .find(|m| m.pin == "PA5")
            .ok_or("PA5 mismatch missing")?;
        assert_eq!(pa5.expected, "SPI1_SCK");
        assert_eq!(pa5.found, "SPI1_MISO");
        assert_eq!(pa5.swapped_with, Some("PA6".to_string()));
        let pa6 = mismatches
            .iter()
            .find(|m| m.pin == "PA6")
            .ok_or("PA6 mismatch missing")?;
        assert_eq!(pa6.expected, "SPI1_MISO");
        assert_eq!(pa6.found, "SPI1_SCK");
        assert_eq!(pa6.swapped_with, Some("PA5".to_string()));
        Ok(())
    }

    #[test]
    fn reverse_check_refuses_a_file_without_pin_column() -> Result<(), String> {
        let detail = PinDetail {
            rows: vec![PinDetailRow {
                pin: "PA1".into(),
                function: "SPI1_SCK".into(),
                net: String::new(),
                instance: String::new(),
                bus: String::new(),
                select: String::new(),
                interrupt: String::new(),
            }],
            unmapped: vec![],
        };
        let csv = "col1,col2\nval1,val2\n";
        let result = check_pin_map_text(&detail, csv);
        assert!(result.is_err(), "expected error for missing pin column");
        let err = result.err().ok_or("expected error")?;
        assert!(
            err.contains("pin column"),
            "error should mention pin column"
        );
        Ok(())
    }
}
