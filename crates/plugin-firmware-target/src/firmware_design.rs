//! Firmware target mapping stored beside the schematic: MCU link, pin roles, sensors.
use pcbcraft_doc::{Component, EmbeddedFile, Project};
use serde::Deserialize;

use crate::mcu::{McuRecord, parse_mcu_record};

pub const DEFAULT_DESIGN_FILE: &str = "firmware-design.toml";

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirmwareBoard {
    pub name: String,
    pub manufacturer_id: String,
    pub mcu: String,
    pub mcu_record: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinAssignment {
    pub pin: String,
    pub function: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GyroMapping {
    pub driver: String,
    pub spi: String,
    pub cs: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashMapping {
    pub spi: String,
    pub cs: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerialMapping {
    pub port: u32,
    pub tx: String,
    pub rx: String,
    pub function: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirmwareDesign {
    pub board: FirmwareBoard,
    #[serde(default)]
    pub assignment: Vec<PinAssignment>,
    #[serde(default)]
    pub gyro: Vec<GyroMapping>,
    #[serde(default)]
    pub flash: Vec<FlashMapping>,
    #[serde(default)]
    pub serial: Vec<SerialMapping>,
    #[serde(default)]
    pub unmapped: Vec<String>,
}

/// Loaded firmware design plus its MCU table.
#[derive(Clone, Debug)]
pub struct FirmwareContext {
    pub design: FirmwareDesign,
    pub mcu: McuRecord,
    pub mcu_component: Component,
}

pub fn embedded_file<'a>(project: &Project, name: &str) -> Option<&'a EmbeddedFile> {
    project.files.iter().find(|f| f.name == name)
}

pub fn parse_firmware_design(text: &str) -> Result<FirmwareDesign, String> {
    let design: FirmwareDesign = toml::from_str(text).map_err(|e| e.to_string())?;
    validate_firmware_design(&design)?;
    Ok(design)
}

pub fn validate_firmware_design(design: &FirmwareDesign) -> Result<(), String> {
    if design.board.name.trim().is_empty() {
        return Err("firmware board name is required".into());
    }
    if design.board.manufacturer_id.trim().is_empty() {
        return Err("firmware manufacturer id is required".into());
    }
    if design.board.mcu.trim().is_empty() {
        return Err("firmware mcu reference is required".into());
    }
    if design.board.mcu_record.trim().is_empty() {
        return Err("firmware mcu record file name is required".into());
    }
    for a in &design.assignment {
        if a.pin.trim().is_empty() || a.function.trim().is_empty() {
            return Err("pin assignment needs pin and function".into());
        }
    }
    Ok(())
}

pub fn load_firmware_context(project: &Project) -> Result<FirmwareContext, String> {
    let design_name = project
        .meta
        .get("firmware_design_file")
        .map(String::as_str)
        .unwrap_or(DEFAULT_DESIGN_FILE);
    let design_file = embedded_file(project, design_name)
        .ok_or_else(|| format!("firmware design file {design_name} is missing"))?;
    let design = parse_firmware_design(&design_file.data)?;
    let mcu_file = embedded_file(project, &design.board.mcu_record)
        .ok_or_else(|| format!("mcu record {} is missing", design.board.mcu_record))?;
    let mcu = parse_mcu_record(&mcu_file.data)?;
    let mcu_component = project
        .components
        .iter()
        .find(|c| c.reference == design.board.mcu)
        .ok_or_else(|| format!("mcu reference {} is not on the schematic", design.board.mcu))?
        .clone();
    Ok(FirmwareContext {
        design,
        mcu,
        mcu_component,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::Project;
    use pcbcraft_geom::Point;

    #[test]
    fn design_and_mcu_load_from_embedded_files() -> Result<(), String> {
        let design_text = include_str!("../tests/fixtures/firmware-design.toml");
        let mcu_text = include_str!("../tests/fixtures/stm32f405-fixture.mcu.toml");
        let mut project = Project::default();
        project.files.push(EmbeddedFile {
            name: DEFAULT_DESIGN_FILE.into(),
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
        let ctx = load_firmware_context(&project)?;
        assert_eq!(ctx.design.board.name, "TESTFC");
        assert_eq!(ctx.mcu.chip.id, "STM32F405");
        Ok(())
    }
}
