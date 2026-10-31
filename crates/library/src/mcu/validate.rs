//! Deterministic validation for MCU pin-mux documents.
use super::schema::{
    BootStrap, McuDocument, McuPackage, McuPeripheral, McuProvenance, DEFAULT_MAX_AF, MAX_AF_PER_PIN,
    MAX_DMA_REQUESTS, MAX_ID_BYTES, MAX_NAME_BYTES, MAX_PACKAGE_PINS, MAX_PACKAGES, MAX_PERIPHERALS,
    MAX_PERIPHERAL_SIGNALS, MAX_PINS, MAX_TEXT_BYTES, MAX_TIMER_CHANNELS, MCU_SCHEMA,
};
use std::collections::{BTreeMap, BTreeSet};

fn err(message: impl Into<String>) -> String {
    message.into()
}

fn bounded_id(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_ID_BYTES || value.contains('\0') {
        return Err(err(format!("{label} must be 1 to {MAX_ID_BYTES} bytes")));
    }
    Ok(())
}

fn bounded_name(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_NAME_BYTES || value.contains('\0') {
        return Err(err(format!("{label} must be 1 to {MAX_NAME_BYTES} bytes")));
    }
    Ok(())
}

fn bounded_text(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_TEXT_BYTES || value.contains('\0') {
        return Err(err(format!("{label} must be 1 to {MAX_TEXT_BYTES} bytes")));
    }
    Ok(())
}

fn validate_provenance(p: &McuProvenance) -> Result<(), String> {
    bounded_text("provenance.source_repository", &p.source_repository)?;
    bounded_text("provenance.revision", &p.revision)?;
    bounded_text("provenance.licence", &p.licence)?;
    bounded_text("provenance.fetched", &p.fetched)?;
    if p.fetched.len() != 10 || !p.fetched.as_bytes().get(4).is_some_and(|b| *b == b'-') {
        return Err(err("provenance.fetched must be an ISO date YYYY-MM-DD"));
    }
    Ok(())
}

fn validate_boot_strap(b: &BootStrap) -> Result<(), String> {
    match b.required_level.as_str() {
        "high" | "low" | "float" => Ok(()),
        other => Err(err(format!(
            "boot_strap.required_level must be high, low or float, not {other}"
        ))),
    }
}

struct PeripheralIndex<'a> {
    by_instance: BTreeMap<&'a str, &'a McuPeripheral>,
}

fn peripheral_index<'a>(peripherals: &'a [McuPeripheral]) -> Result<PeripheralIndex<'a>, String> {
    if peripherals.len() > MAX_PERIPHERALS {
        return Err(err(format!("peripheral limit is {MAX_PERIPHERALS}")));
    }
    let mut by_instance = BTreeMap::new();
    for p in peripherals {
        bounded_id("peripheral.instance", &p.instance)?;
        bounded_name("peripheral.kind", &p.kind)?;
        if p.signals.len() > MAX_PERIPHERAL_SIGNALS {
            return Err(err(format!(
                "peripheral {} signals exceed {MAX_PERIPHERAL_SIGNALS}",
                p.instance
            )));
        }
        if p.channels.len() > MAX_TIMER_CHANNELS {
            return Err(err(format!(
                "peripheral {} channels exceed {MAX_TIMER_CHANNELS}",
                p.instance
            )));
        }
        for s in &p.signals {
            bounded_name("peripheral.signal", s)?;
        }
        for c in &p.channels {
            bounded_name("peripheral.channel", c)?;
        }
        if by_instance.insert(p.instance.as_str(), p).is_some() {
            return Err(err(format!("duplicate peripheral instance {}", p.instance)));
        }
    }
    Ok(PeripheralIndex { by_instance })
}

fn gpio_signal_allowed(signal: &str) -> bool {
    let Some(rest) = signal.strip_prefix("GPIO") else {
        return false;
    };
    let (port, pin) = rest.split_once('.')?;
    port.len() == 1
        && port.chars().all(|c| c.is_ascii_uppercase())
        && !pin.is_empty()
        && pin.chars().all(|c| c.is_ascii_digit())
}

fn signal_belongs(peripheral: &McuPeripheral, signal_part: &str) -> bool {
    if peripheral.signals.iter().any(|s| s == signal_part) {
        return true;
    }
    peripheral.channels.iter().any(|c| c == signal_part)
}

fn validate_pin_signal(
    pin_name: &str,
    af: u8,
    signal: &str,
    max_af: u8,
    index: &PeripheralIndex<'_>,
) -> Result<(), String> {
    bounded_name("pin alternate signal", signal)?;
    if af > max_af {
        return Err(err(format!(
            "pin {pin_name} alternate function {af} exceeds max {max_af}"
        )));
    }
    let Some((instance, signal_part)) = signal.split_once('.') else {
        return Err(err(format!(
            "pin {pin_name} signal {signal} must be INSTANCE.SIGNAL"
        )));
    };
    if gpio_signal_allowed(signal) {
        if af != 0 {
            return Err(err(format!(
                "pin {pin_name} GPIO signal {signal} must use alternate function 0"
            )));
        }
        return Ok(());
    }
    let Some(peripheral) = index.by_instance.get(instance) else {
        return Err(err(format!(
            "pin {pin_name} signal {signal} references unknown instance {instance}"
        )));
    };
    if !signal_belongs(peripheral, signal_part) {
        return Err(err(format!(
            "pin {pin_name} signal {signal} is not declared on peripheral {instance}"
        )));
    }
    Ok(())
}

fn validate_package_pins(
    package: &McuPackage,
    pin_names: &BTreeSet<&str>,
) -> Result<(), String> {
    bounded_id("package.id", &package.id)?;
    bounded_name("package.name", &package.name)?;
    if package.pins.len() > MAX_PACKAGE_PINS {
        return Err(err(format!(
            "package {} exceeds {MAX_PACKAGE_PINS} pins",
            package.id
        )));
    }
    let mut seen = BTreeSet::new();
    for pin in &package.pins {
        bounded_name("package pin", pin)?;
        if !seen.insert(pin.as_str()) {
            return Err(err(format!("package {} lists pin {pin} twice", package.id)));
        }
        if !pin_names.contains(pin.as_str()) {
            return Err(err(format!(
                "package {} pin {pin} is missing from the pin mux table",
                package.id
            )));
        }
    }
    Ok(())
}

fn validate_pin_not_in_package(
    packages: &[McuPackage],
    pin_name: &str,
) -> Result<(), String> {
    let listed = packages
        .iter()
        .any(|pkg| pkg.pins.iter().any(|p| p == pin_name));
    if !listed {
        return Err(err(format!("pin {pin_name} is not listed in any package")));
    }
    Ok(())
}

/// Validate a parsed MCU document. Returns the first deterministic error.
pub fn validate_mcu(doc: &McuDocument) -> Result<(), String> {
    if doc.schema != MCU_SCHEMA {
        return Err(err(format!("unsupported MCU schema {}", doc.schema)));
    }
    validate_provenance(&doc.provenance)?;
    bounded_id("mcu.id", &doc.mcu.id)?;
    bounded_name("mcu.name", &doc.mcu.name)?;
    bounded_name("mcu.vendor", &doc.mcu.vendor)?;
    bounded_name("mcu.family", &doc.mcu.family)?;
    let max_af = if doc.mcu.max_alternate_function == 0 {
        DEFAULT_MAX_AF
    } else {
        doc.mcu.max_alternate_function
    };
    if doc.package.len() > MAX_PACKAGES || doc.package.is_empty() {
        return Err(err(format!(
            "package must list 1 to {MAX_PACKAGES} variants"
        )));
    }
    if doc.pin.len() > MAX_PINS || doc.pin.is_empty() {
        return Err(err(format!("pin table must list 1 to {MAX_PINS} entries")));
    }
    if doc.dma.len() > MAX_DMA_REQUESTS {
        return Err(err(format!("dma table exceeds {MAX_DMA_REQUESTS} entries")));
    }
    let index = peripheral_index(&doc.peripheral)?;
    let mut pin_names = BTreeSet::new();
    for pin in &doc.pin {
        bounded_name("pin.name", &pin.name)?;
        if !pin_names.insert(pin.name.as_str()) {
            return Err(err(format!("duplicate pin name {}", pin.name)));
        }
        if let Some(domain) = &pin.power_domain {
            bounded_name("pin.power_domain", domain)?;
        }
        if let Some(strap) = &pin.boot_strap {
            validate_boot_strap(strap)?;
        }
        if pin.alternate.len() > MAX_AF_PER_PIN {
            return Err(err(format!(
                "pin {} exceeds {MAX_AF_PER_PIN} alternate functions",
                pin.name
            )));
        }
        let mut af_seen = BTreeSet::new();
        for alt in &pin.alternate {
            if !af_seen.insert(alt.af) {
                return Err(err(format!(
                    "pin {} lists alternate function {} twice",
                    pin.name, alt.af
                )));
            }
            validate_pin_signal(&pin.name, alt.af, &alt.signal, max_af, &index)?;
        }
        validate_pin_not_in_package(&doc.package, &pin.name)?;
    }
    for package in &doc.package {
        validate_package_pins(package, &pin_names)?;
    }
    for dma in &doc.dma {
        bounded_id("dma.instance", &dma.instance)?;
        bounded_name("dma.signal", &dma.signal)?;
        let Some(peripheral) = index.by_instance.get(dma.instance.as_str()) else {
            return Err(err(format!(
                "dma entry references unknown instance {}",
                dma.instance
            )));
        };
        if !signal_belongs(peripheral, &dma.signal) {
            return Err(err(format!(
                "dma entry {}.{} is not declared on peripheral {}",
                dma.instance, dma.signal, dma.instance
            )));
        }
    }
    Ok(())
}
