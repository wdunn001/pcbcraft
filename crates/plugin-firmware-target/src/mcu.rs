//! Pin-mux tables for microcontrollers: parse, validate, and query alternate functions.
//!
//! When the MCU record lands in the library crate (issue #844), this module can thin-wrap it.
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// Provenance for bundled or imported pin-mux data.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuProvenance {
    pub source: String,
    pub revision: String,
    pub license: String,
    pub date: String,
}

/// One physical pin in the package.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuPin {
    pub name: String,
    #[serde(default)]
    pub number: String,
    #[serde(default)]
    pub functions: Vec<String>,
    /// True when the pin tolerates 5 V on the I/O pad.
    #[serde(default)]
    pub ft_5v: bool,
    #[serde(default)]
    pub analog_only: bool,
    /// When set, the pin is a boot or strap pin that must sit at this level at reset.
    #[serde(default)]
    pub boot_level: Option<String>,
    #[serde(default)]
    pub debug: bool,
}

/// One peripheral instance (UART1, SPI2, TIM3, and so on).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuInstance {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub signals: Vec<String>,
    #[serde(default)]
    pub dma: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuChip {
    pub id: String,
    pub package: String,
}

/// Parsed pin-mux record for one MCU part.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuRecord {
    pub provenance: McuProvenance,
    pub chip: McuChip,
    #[serde(default)]
    pub pins: Vec<McuPin>,
    #[serde(default)]
    pub instances: Vec<McuInstance>,
}

impl McuRecord {
    pub fn pin_by_name(&self, name: &str) -> Option<&McuPin> {
        self.pins.iter().find(|p| p.name == name)
    }

    pub fn pin_functions(&self, pin: &str) -> BTreeSet<&str> {
        self.pin_by_name(pin)
            .map(|p| p.functions.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    pub fn instance_by_id(&self, id: &str) -> Option<&McuInstance> {
        self.instances.iter().find(|i| i.id == id)
    }
}

/// Parse and validate a pin-mux table from TOML text.
pub fn parse_mcu_record(text: &str) -> Result<McuRecord, String> {
    let record: McuRecord = toml::from_str(text).map_err(|e| e.to_string())?;
    validate_mcu_record(&record)?;
    Ok(record)
}

pub fn validate_mcu_record(record: &McuRecord) -> Result<(), String> {
    if record.provenance.source.trim().is_empty() {
        return Err("mcu record provenance source is required".into());
    }
    if record.provenance.license.trim().is_empty() {
        return Err("mcu record provenance license is required".into());
    }
    if record.provenance.revision.trim().is_empty() {
        return Err("mcu record provenance revision is required".into());
    }
    if record.provenance.date.trim().is_empty() {
        return Err("mcu record provenance date is required".into());
    }
    if record.chip.id.trim().is_empty() || record.chip.package.trim().is_empty() {
        return Err("mcu record chip id and package are required".into());
    }
    if record.pins.is_empty() {
        return Err("mcu record needs at least one pin".into());
    }
    let mut names = BTreeSet::new();
    for pin in &record.pins {
        if pin.name.trim().is_empty() {
            return Err("mcu pin name is required".into());
        }
        if !names.insert(&pin.name) {
            return Err(format!("duplicate mcu pin name {}", pin.name));
        }
        if let Some(level) = &pin.boot_level {
            if level != "low" && level != "high" {
                return Err(format!("boot level on {} must be low or high", pin.name));
            }
        }
    }
    let mut instance_ids = BTreeSet::new();
    for inst in &record.instances {
        if inst.id.trim().is_empty() || inst.kind.trim().is_empty() {
            return Err("mcu instance id and kind are required".into());
        }
        if !instance_ids.insert(&inst.id) {
            return Err(format!("duplicate mcu instance {}", inst.id));
        }
        for sig in &inst.signals {
            if sig.trim().is_empty() {
                return Err(format!("empty signal on instance {}", inst.id));
            }
        }
    }
    Ok(())
}

/// Map a logical function token to the instance id that owns it, when known.
pub fn function_instance(record: &McuRecord, function: &str) -> Option<String> {
    for inst in &record.instances {
        if inst.signals.iter().any(|s| s == function) {
            return Some(inst.id.clone());
        }
        if function.starts_with(&format!("{}_", inst.id)) {
            return Some(inst.id.clone());
        }
    }
    if let Some((prefix, _)) = function.split_once('_') {
        if record.instance_by_id(prefix).is_some() {
            return Some(prefix.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/stm32f405-fixture.mcu.toml");

    #[test]
    fn fixture_mcu_record_loads_and_validates() -> Result<(), String> {
        let record = parse_mcu_record(FIXTURE)?;
        assert_eq!(record.chip.id, "STM32F405");
        assert!(record.pin_by_name("PA4").is_some());
        Ok(())
    }

    #[test]
    fn duplicate_pin_name_is_refused() {
        let mut text = FIXTURE.to_string();
        text.push_str("\n[[pins]]\nname = \"PA4\"\nfunctions = [\"GPIO\"]\n");
        assert!(parse_mcu_record(&text).is_err());
    }

    #[test]
    fn empty_provenance_source_is_refused() {
        let text = FIXTURE.replace("source = \"test-fixture\"", "source = \"\"");
        assert!(parse_mcu_record(&text).is_err());
    }
}
