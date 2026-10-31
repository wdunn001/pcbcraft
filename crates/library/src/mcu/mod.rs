//! MCU pin-mux library records (schema 1) with import and validation.
mod builtins;
mod parse;
mod schema;
mod validate;

pub use builtins::{builtin_ids, load_builtin, BuiltinMcu, BUILT_INS};
pub use parse::{import_mcu_bundle_json, parse_and_validate_mcu, MCU_FILE_SUFFIX};
pub use schema::{
    BootStrap, DmaRequest, McuDocument, McuIdentity, McuImportBundle, McuPackage, McuPeripheral,
    McuPin, McuProvenance, PinAlternate, MCU_SCHEMA,
};
pub use validate::validate_mcu;
