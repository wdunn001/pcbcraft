//! Optional vendor quote handoff and marketplace settings (schema only).
//!
//! Separate from the sharing portal. Nothing here contacts a vendor host; callers
//! build quote manifests from manufacturing outputs after explicit user approval.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const VENDOR_HANDOFF_SCHEMA_VERSION: u32 = 1;
const MAX_ADAPTER_ID: usize = 64;
const MAX_LABEL: usize = 128;
const MAX_URL: usize = 512;
const MAX_TAG: usize = 64;
const MAX_TAGS: usize = 32;
const MAX_FILES: usize = 256;
const MAX_PATH: usize = 512;
const MAX_NOTES: usize = 64;
const MAX_NOTE_LEN: usize = 256;
const SHA256_HEX: usize = 64;

fn bounded(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max
}

fn sha256_hex(s: &str) -> bool {
    s.len() == SHA256_HEX && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Per-instance module switch and sub-settings. Everything is off by default.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VendorHandoffModuleSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub vendor_quotes: VendorQuoteSettings,
    #[serde(default)]
    pub marketplace: MarketplaceSettings,
}

/// Vendor quote handoff sub-switch.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VendorQuoteSettings {
    #[serde(default)]
    pub enabled: bool,
    /// App preference: show vendor quote controls when the instance also enables quotes.
    #[serde(default)]
    pub show_in_app: bool,
}

/// Marketplace sub-switch (not implemented; schema reserves the flag).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MarketplaceSettings {
    #[serde(default)]
    pub enabled: bool,
}

/// How an adapter delivers a quote or order.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VendorAdapterMode {
    DocumentedApi,
    UploadFlow,
    LinkOnly,
}

/// One registered adapter (metadata only).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VendorAdapterDescriptor {
    pub id: String,
    pub label: String,
    pub mode: VendorAdapterMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<String>,
}

/// One manufacturing output listed in a quote handoff dialog.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VendorQuoteFileEntry {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Manifest shown before anything leaves the machine.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VendorQuoteHandoffManifest {
    pub schema_version: u32,
    pub adapter_id: String,
    pub adapter_mode: VendorAdapterMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_label: Option<String>,
    pub files: Vec<VendorQuoteFileEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stackup_notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub finish_options: Vec<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Instance capability fragment for the app and portal service.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VendorHandoffInstanceCapabilities {
    pub schema_version: u32,
    pub compiled: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub module_enabled: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub vendor_quotes: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub vendor_quotes_ui: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub marketplace: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapter_modes: Vec<VendorAdapterMode>,
}

impl VendorHandoffModuleSettings {
    pub fn valid(&self) -> bool {
        if self.marketplace.enabled && !self.enabled {
            return false;
        }
        if self.vendor_quotes.enabled && !self.enabled {
            return false;
        }
        true
    }

    pub fn vendor_quotes_ui(&self) -> bool {
        self.enabled && self.vendor_quotes.enabled && self.vendor_quotes.show_in_app
    }

    pub fn instance_capabilities(&self, compiled: bool) -> VendorHandoffInstanceCapabilities {
        let module_enabled = compiled && self.enabled;
        let vendor_quotes = module_enabled && self.vendor_quotes.enabled;
        let marketplace = module_enabled && self.marketplace.enabled;
        let mut adapter_modes = Vec::new();
        if vendor_quotes {
            adapter_modes.push(VendorAdapterMode::UploadFlow);
            adapter_modes.push(VendorAdapterMode::LinkOnly);
        }
        VendorHandoffInstanceCapabilities {
            schema_version: VENDOR_HANDOFF_SCHEMA_VERSION,
            compiled,
            module_enabled,
            vendor_quotes,
            vendor_quotes_ui: vendor_quotes && self.vendor_quotes.show_in_app,
            marketplace,
            adapter_modes,
        }
    }

    pub fn capability_json(&self, compiled: bool) -> Value {
        serde_json::to_value(self.instance_capabilities(compiled))
            .unwrap_or_else(|_| json!({"schema_version": VENDOR_HANDOFF_SCHEMA_VERSION, "compiled": compiled}))
    }
}

impl VendorAdapterDescriptor {
    pub fn valid(&self) -> bool {
        if !bounded(&self.id, MAX_ADAPTER_ID) || !bounded(&self.label, MAX_LABEL) {
            return false;
        }
        if !self.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return false;
        }
        if self
            .page_url
            .as_ref()
            .is_some_and(|u| !bounded(u, MAX_URL))
        {
            return false;
        }
        if self.capabilities.len() > MAX_TAGS
            || self.requirements.len() > MAX_TAGS
            || self
                .capabilities
                .iter()
                .chain(self.requirements.iter())
                .any(|t| !bounded(t, MAX_TAG))
        {
            return false;
        }
        match self.mode {
            VendorAdapterMode::LinkOnly | VendorAdapterMode::UploadFlow => self.page_url.is_some(),
            VendorAdapterMode::DocumentedApi => false,
        }
    }
}

impl VendorQuoteFileEntry {
    pub fn valid(&self) -> bool {
        bounded(&self.path, MAX_PATH)
            && sha256_hex(&self.sha256)
            && self
                .role
                .as_ref()
                .is_none_or(|r| bounded(r, MAX_TAG))
    }
}

impl VendorQuoteHandoffManifest {
    pub fn valid(&self) -> bool {
        if self.schema_version != VENDOR_HANDOFF_SCHEMA_VERSION {
            return false;
        }
        if !bounded(&self.adapter_id, MAX_ADAPTER_ID) || self.files.is_empty() {
            return false;
        }
        if self.files.len() > MAX_FILES {
            return false;
        }
        if self
            .host_label
            .as_ref()
            .is_some_and(|h| !bounded(h, MAX_LABEL))
        {
            return false;
        }
        if self.stackup_notes.len() > MAX_NOTES
            || self.finish_options.len() > MAX_NOTES
            || self
                .stackup_notes
                .iter()
                .chain(self.finish_options.iter())
                .any(|n| !bounded(n, MAX_NOTE_LEN))
        {
            return false;
        }
        self.files.iter().all(VendorQuoteFileEntry::valid)
    }
}

/// Built-in fixture adapter for upload-flow tests (no network).
pub fn fixture_upload_adapter() -> VendorAdapterDescriptor {
    VendorAdapterDescriptor {
        id: "fixture-upload".into(),
        label: "Fixture upload flow".into(),
        mode: VendorAdapterMode::UploadFlow,
        page_url: Some("https://example.test/quote-upload".into()),
        capabilities: vec!["quote".into()],
        requirements: vec![
            "gerber_set".into(),
            "drill".into(),
            "bom".into(),
            "placement".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_off_and_capabilities_omit_flags() -> Result<(), String> {
        let settings = VendorHandoffModuleSettings::default();
        assert!(settings.valid());
        let caps = settings.instance_capabilities(true);
        assert!(!caps.module_enabled);
        assert!(!caps.vendor_quotes);
        assert!(!caps.vendor_quotes_ui);
        assert!(!caps.marketplace);
        assert!(caps.adapter_modes.is_empty());
        let json = settings.capability_json(true);
        let obj = json
            .as_object()
            .ok_or("capabilities must be an object")?;
        assert!(obj.get("module_enabled").is_none());
        assert!(obj.get("marketplace").is_none());
        Ok(())
    }

    #[test]
    fn sub_switch_without_master_is_invalid() -> Result<(), String> {
        let mut settings = VendorHandoffModuleSettings::default();
        settings.vendor_quotes.enabled = true;
        assert!(!settings.valid());
        Ok(())
    }

    #[test]
    fn enabled_capabilities_expose_quote_flags() -> Result<(), String> {
        let settings = VendorHandoffModuleSettings {
            enabled: true,
            vendor_quotes: VendorQuoteSettings {
                enabled: true,
                show_in_app: true,
            },
            ..Default::default()
        };
        let caps = settings.instance_capabilities(true);
        assert!(caps.vendor_quotes);
        assert!(caps.vendor_quotes_ui);
        assert!(!caps.marketplace);
        Ok(())
    }

    #[test]
    fn fixture_manifest_and_adapter_validate() -> Result<(), String> {
        let adapter = fixture_upload_adapter();
        assert!(adapter.valid());
        let manifest = VendorQuoteHandoffManifest {
            schema_version: VENDOR_HANDOFF_SCHEMA_VERSION,
            adapter_id: adapter.id.clone(),
            adapter_mode: adapter.mode,
            host_label: Some("example.test".into()),
            files: vec![VendorQuoteFileEntry {
                path: "gerber/board.gtl".into(),
                size_bytes: 128,
                sha256: "a".repeat(SHA256_HEX),
                role: Some("gerber".into()),
            }],
            stackup_notes: vec!["1.6 mm FR4".into()],
            finish_options: vec!["HASL".into()],
        };
        assert!(manifest.valid());
        Ok(())
    }
}
