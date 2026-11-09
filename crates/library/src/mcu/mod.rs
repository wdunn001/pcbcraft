//! MCU pin-mux library records (schema 1) with import and validation.
mod builtins;
mod parse;
mod schema;
mod validate;

pub use builtins::{BUILT_INS, BuiltinMcu, builtin_ids, load_builtin};
pub use parse::{MCU_FILE_SUFFIX, import_mcu_bundle_json, parse_and_validate_mcu};
pub use schema::{
    BootStrap, DmaRequest, MCU_SCHEMA, McuDocument, McuIdentity, McuImportBundle, McuPackage,
    McuPeripheral, McuPin, McuProvenance, PinAlternate,
};
pub use validate::validate_mcu;
