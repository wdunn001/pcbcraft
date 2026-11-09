//! Typed MCU pin-mux records (schema version 1).
use serde::{Deserialize, Serialize};

pub const MCU_SCHEMA: u32 = 1;
pub const MAX_ID_BYTES: usize = 64;
pub const MAX_NAME_BYTES: usize = 128;
pub const MAX_TEXT_BYTES: usize = 4096;
pub const MAX_PINS: usize = 512;
pub const MAX_AF_PER_PIN: usize = 16;
pub const MAX_PACKAGES: usize = 16;
pub const MAX_PACKAGE_PINS: usize = 512;
pub const MAX_PERIPHERALS: usize = 256;
pub const MAX_PERIPHERAL_SIGNALS: usize = 64;
pub const MAX_TIMER_CHANNELS: usize = 16;
pub const MAX_DMA_REQUESTS: usize = 256;
pub const DEFAULT_MAX_AF: u8 = 15;

/// Required provenance for bundled or imported MCU tables.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuProvenance {
    pub source_repository: String,
    pub revision: String,
    pub licence: String,
    pub fetched: String,
}

/// Chip identity and mux limits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuIdentity {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub family: String,
    #[serde(default = "default_max_af")]
    pub max_alternate_function: u8,
}

fn default_max_af() -> u8 {
    DEFAULT_MAX_AF
}

/// One package variant: the ordered pin or ball names exposed on that package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuPackage {
    pub id: String,
    pub name: String,
    pub pins: Vec<String>,
}

/// Required boot or strap level on a pin at reset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootStrap {
    pub required_level: String,
}

/// Per-pin electrical and mux limits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuPin {
    pub name: String,
    #[serde(default)]
    pub five_volt_tolerance: bool,
    #[serde(default)]
    pub analog_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_strap: Option<BootStrap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_domain: Option<String>,
    #[serde(default)]
    pub alternate: Vec<PinAlternate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinAlternate {
    pub af: u8,
    pub signal: String,
}

/// Declared peripheral instance and the signal names it exposes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuPeripheral {
    pub kind: String,
    pub instance: String,
    #[serde(default)]
    pub signals: Vec<String>,
    #[serde(default)]
    pub channels: Vec<String>,
}

/// Maps a DMA request line to a peripheral signal name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DmaRequest {
    pub instance: String,
    pub signal: String,
    pub request: u32,
}

/// One MCU pin-mux file, TOML or JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuDocument {
    pub schema: u32,
    pub provenance: McuProvenance,
    pub mcu: McuIdentity,
    #[serde(default)]
    pub package: Vec<McuPackage>,
    #[serde(default)]
    pub pin: Vec<McuPin>,
    #[serde(default)]
    pub peripheral: Vec<McuPeripheral>,
    #[serde(default)]
    pub dma: Vec<DmaRequest>,
}

/// Importer input: permissively licensed bundle before normalisation to [`McuDocument`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McuImportBundle {
    pub provenance: McuProvenance,
    pub mcu: McuIdentity,
    pub package: Vec<McuPackage>,
    pub pin: Vec<McuPin>,
    pub peripheral: Vec<McuPeripheral>,
    #[serde(default)]
    pub dma: Vec<DmaRequest>,
}

impl McuImportBundle {
    pub fn into_document(self) -> McuDocument {
        McuDocument {
            schema: MCU_SCHEMA,
            provenance: self.provenance,
            mcu: self.mcu,
            package: self.package,
            pin: self.pin,
            peripheral: self.peripheral,
            dma: self.dma,
        }
    }
}
