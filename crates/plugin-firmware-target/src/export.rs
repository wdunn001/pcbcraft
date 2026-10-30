//! Flight-controller firmware target export and reverse pin-map check.
use pcbcraft_doc::Project;
use std::collections::BTreeMap;

use crate::firmware_design::{FirmwareContext, load_firmware_context};

pub const EXPORT_KIND: &str = "firmware_target";

/// Files written for export kind [`EXPORT_KIND`].
pub fn firmware_target_files(project: &Project) -> Result<Vec<(String, Vec<u8>)>, String> {
    let ctx = load_firmware_context(project)?;
    let config = betaflight_config_h(&ctx)?;
    let manifest = board_manifest_json(&ctx)?;
    Ok(vec![
        (
            format!(
                "configs/{}/{}/config.h",
                ctx.design.board.manufacturer_id.to_ascii_lowercase(),
                ctx.design.board.name.to_ascii_lowercase()
            ),
            config.into_bytes(),
        ),
        ("board-manifest.json".into(), manifest.into_bytes()),
    ])
}

fn betaflight_config_h(ctx: &FirmwareContext) -> Result<String, String> {
    let mut lines = vec![
        format!("#define TARGET {}", ctx.design.board.name),
        format!("#define BOARD_NAME \"{}\"", ctx.design.board.name),
        format!(
            "#define MANUFACTURER_ID {}",
            ctx.design.board.manufacturer_id
        ),
        format!("#define MCU {}", ctx.mcu.chip.id),
        String::new(),
    ];
    if let Some(gyro) = ctx.design.gyro.first() {
        lines.push("#define USE_GYRO".into());
        lines.push(format!("#define USE_GYRO_SPI_{}", gyro.driver));
        lines.push(format!("#define GYRO_1_CS_PIN {}", gyro.cs));
        lines.push(format!("#define GYRO_1_SPI_INSTANCE {}", gyro.spi));
        lines.push(String::new());
    }
    if let Some(flash) = ctx.design.flash.first() {
        lines.push("#define USE_FLASH".into());
        lines.push(format!("#define FLASH_CS_PIN {}", flash.cs));
        lines.push(format!("#define FLASH_SPI_INSTANCE {}", flash.spi));
        lines.push(String::new());
    }
    for serial in &ctx.design.serial {
        lines.push(format!(
            "// SERIAL{} {} TX {} RX {}",
            serial.port, serial.function, serial.tx, serial.rx
        ));
    }
    lines.push(String::new());
    lines.push(
        "// Timer and DMA map: fill using the firmware timer tool; indices are not guessed here."
            .into(),
    );
    lines.push("// TIMER_PIN_MAP".into());
    if !ctx.design.unmapped.is_empty() {
        lines.push("// Unmapped parts (no driver assumed):".into());
        for part in &ctx.design.unmapped {
            lines.push(format!("//   {part}"));
        }
    }
    lines.push(String::new());
    Ok(lines.join("\n"))
}

fn board_manifest_json(ctx: &FirmwareContext) -> Result<String, String> {
    let mut sensors = vec![];
    for gyro in &ctx.design.gyro {
        sensors.push(serde_json::json!({
            "kind": "gyro",
            "driver": gyro.driver,
            "bus": "spi",
            "instance": gyro.spi,
            "chip_select": gyro.cs,
        }));
    }
    for flash in &ctx.design.flash {
        sensors.push(serde_json::json!({
            "kind": "flash",
            "bus": "spi",
            "instance": flash.spi,
            "chip_select": flash.cs,
        }));
    }
    let manifest = serde_json::json!({
        "board": ctx.design.board.name,
        "manufacturer_id": ctx.design.board.manufacturer_id,
        "mcu": ctx.mcu.chip.id,
        "sensors": sensors,
        "serials": ctx.design.serial.iter().map(|s| serde_json::json!({
            "port": s.port,
            "function": s.function,
            "tx": s.tx,
            "rx": s.rx,
        })).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())
}

/// Compare an existing config.h pin map with the schematic firmware design.
pub fn firmware_config_compare(
    project: &Project,
    config_text: &str,
) -> Result<Vec<String>, String> {
    let ctx = load_firmware_context(project)?;
    let parsed = parse_defines(config_text);
    let mut mismatches = vec![];
    if let Some(gyro) = ctx.design.gyro.first() {
        if let Some(cs) = parsed.get("GYRO_1_CS_PIN") {
            if cs != &gyro.cs {
                mismatches.push(format!(
                    "GYRO_1_CS_PIN is {cs} in the file but the design uses {}",
                    gyro.cs
                ));
            }
        }
    }
    for assign in &ctx.design.assignment {
        let key = format!("PIN_MAP_{}", assign.function);
        if let Some(pin) = parsed.get(&key) {
            if pin != &assign.pin {
                mismatches.push(format!(
                    "{key} is {pin} in the file but the design uses {}",
                    assign.pin
                ));
            }
        }
    }
    Ok(mismatches)
}

fn parse_defines(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("#define ") {
            continue;
        }
        let rest = line.trim_start_matches("#define ").trim();
        let Some((name, value)) = rest.split_once(char::is_whitespace) else {
            if !rest.is_empty() {
                out.insert(rest.to_string(), String::new());
            }
            continue;
        };
        out.insert(name.to_string(), value.trim_matches('"').trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Component, EmbeddedFile, Project};
    use pcbcraft_geom::Point;

    fn fixture_project() -> Project {
        let design_text = include_str!("../tests/fixtures/firmware-design.toml");
        let mcu_text = include_str!("../tests/fixtures/stm32f405-fixture.mcu.toml");
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
            position: Point { x: 0.0, y: 0.0 },
            rotation: 0.0,
            pins: vec![],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
        });
        project
    }

    #[test]
    fn export_matches_golden_config() -> Result<(), String> {
        let files = firmware_target_files(&fixture_project())?;
        let config = files
            .iter()
            .find(|(n, _)| n.ends_with("config.h"))
            .ok_or("config.h missing")?;
        let golden = include_str!("../tests/fixtures/config.h.golden");
        if config.1 != golden.as_bytes() {
            return Err(format!(
                "config.h mismatch:\n{}",
                String::from_utf8_lossy(&config.1)
            ));
        }
        Ok(())
    }

    #[test]
    fn reverse_check_reports_swapped_gyro_cs() -> Result<(), String> {
        let config = "#define GYRO_1_CS_PIN PA5\n";
        let mismatches = firmware_config_compare(&fixture_project(), config)?;
        assert!(
            mismatches.iter().any(|m| m.contains("GYRO_1_CS_PIN")),
            "expected gyro cs mismatch: {mismatches:?}"
        );
        Ok(())
    }
}
