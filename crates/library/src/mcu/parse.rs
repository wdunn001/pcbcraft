//! Parse MCU pin-mux files from TOML or JSON.
use super::schema::{McuDocument, McuImportBundle, MCU_SCHEMA};
use super::validate::validate_mcu;

pub const MCU_FILE_SUFFIX: &str = ".pcbcraft-mcu.toml";

fn parse_toml(text: &str) -> Result<McuDocument, String> {
    toml::from_str(text).map_err(|e| format!("MCU TOML: {e}"))
}

fn parse_json(text: &str) -> Result<McuDocument, String> {
    let bundle: McuImportBundle =
        serde_json::from_str(text).map_err(|e| format!("MCU import JSON: {e}"))?;
    let mut doc = bundle.into_document();
    doc.schema = MCU_SCHEMA;
    Ok(doc)
}

/// Parse and validate MCU text. Accepts TOML (`.pcbcraft-mcu.toml`) or import JSON.
pub fn parse_and_validate_mcu(text: &str) -> Result<McuDocument, String> {
    let trimmed = text.trim_start();
    let doc = if trimmed.starts_with('{') {
        parse_json(text)?
    } else {
        parse_toml(text)?
    };
    validate_mcu(&doc)?;
    Ok(doc)
}

/// Import a permissively licensed JSON bundle into a validated document.
pub fn import_mcu_bundle_json(text: &str) -> Result<McuDocument, String> {
    parse_and_validate_mcu(text)
}
