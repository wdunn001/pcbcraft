//! Built-in MCU pin-mux tables shipped with the app.
use super::parse::parse_and_validate_mcu;
use super::schema::McuDocument;

pub struct BuiltinMcu {
    pub id: &'static str,
    pub toml: &'static str,
}

macro_rules! builtin {
    ($id:literal, $file:literal) => {
        BuiltinMcu {
            id: $id,
            toml: include_str!(concat!("../../data/mcu/", $file)),
        }
    };
}

pub static BUILT_INS: &[BuiltinMcu] = &[
    builtin!("stm32f405rg", "stm32f405rg.toml"),
    builtin!("stm32g431cb", "stm32g431cb.toml"),
];

pub fn builtin_ids() -> &'static [&'static str] {
    &["stm32f405rg", "stm32g431cb"]
}

pub fn load_builtin(id: &str) -> Result<McuDocument, String> {
    let entry = BUILT_INS
        .iter()
        .find(|b| b.id == id)
        .ok_or_else(|| format!("unknown built-in MCU {id}"))?;
    parse_and_validate_mcu(entry.toml)
}
