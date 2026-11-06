//! Vendor quote handoff and marketplace integration types.
//!
//! This module provides the adapter trait for vendor integrations and
//! implementations for upload-flow and link-only modes. API adapters
//! require vendor keys and are built only when terms are documented.
//!
//! # Rules
//! - One adapter per vendor, no shared state between adapters.
//! - No keys in bundles, manifests or logs; tokens live in the credential store.
//! - Off by default; the app setting and instance capability document control visibility.
//!
//! # Dependencies
//! - crates/library/src/part_types.rs (Part, PartSidecar)
//! - crates/library/src/sources.rs (PartSource)
//!
//! # Do not
//! - Store card data or payment details in this module.
//! - Add marketplace logic until the owner approves it.

use serde::{Deserialize, Serialize};

/// Capabilities a vendor integration may support.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VendorCapability {
    /// The vendor accepts a quote request with manufacturing files.
    Quote,
    /// The vendor accepts an order through the app.
    Order,
    /// The vendor page can be opened with a prepared archive.
    UploadFlow,
    /// The vendor page can be linked from a publication.
    LinkOnly,
}

/// Requirements a design must satisfy before sending to a vendor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorRequirements {
    /// Gerber set required.
    pub gerber: bool,
    /// Drill files required.
    pub drill: bool,
    /// Board outline required.
    pub outline: bool,
    /// Stackup and finish options required.
    pub stackup: bool,
    /// Bill of materials with manufacturer part numbers required.
    pub bom: bool,
    /// Pick and place file required.
    pub pick_and_place: bool,
    /// Assembly drawing required.
    pub assembly_drawing: bool,
}

impl Default for VendorRequirements {
    fn default() -> Self {
        Self {
            gerber: true,
            drill: true,
            outline: true,
            stackup: true,
            bom: true,
            pick_and_place: true,
            assembly_drawing: true,
        }
    }
}

/// A file to include in a quote request or order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteFile {
    /// Display name for the file.
    pub name: String,
    /// File size in bytes.
    pub size: u64,
    /// SHA-256 hash of the file content.
    pub sha256: String,
}

/// Options for requesting a quote.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuoteOptions {
    /// Number of board copies requested.
    pub quantity: u32,
    /// Board thickness in millimetres.
    pub thickness_mm: f64,
    /// Copper weight in ounces.
    pub copper_oz: f64,
    /// Surface finish (e.g., "HASL", "ENIG", "Immersion Tin").
    pub finish: String,
    /// Board colour.
    pub color: String,
}

impl Default for QuoteOptions {
    fn default() -> Self {
        Self {
            quantity: 1,
            thickness_mm: 1.6,
            copper_oz: 1.0,
            finish: "HASL".to_string(),
            color: "Green".to_string(),
        }
    }
}

/// The result of preparing a quote request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuotePrepared {
    /// Files to send with the quote request.
    pub files: Vec<QuoteFile>,
    /// The vendor's name.
    pub vendor: String,
    /// The vendor's host (for display).
    pub host: String,
    /// The quote options that will be sent.
    pub options: QuoteOptions,
}

/// The result of preparing an order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderPrepared {
    /// Files to send with the order.
    pub files: Vec<QuoteFile>,
    /// The vendor's name.
    pub vendor: String,
    /// The vendor's host (for display).
    pub host: String,
    /// The order options that will be sent.
    pub options: QuoteOptions,
}

/// Adapter trait for vendor integrations.
///
/// One implementation per vendor, supporting one or more capabilities.
///
/// # Implementations
/// - crates/library/src/vendor_integrations.rs (`JlcpcbUploadAdapter`)
pub trait VendorAdapter {
    /// The vendor's display name.
    fn name(&self) -> &str;

    /// The vendor's host (for display and link building).
    fn host(&self) -> &str;

    /// Capabilities this adapter supports.
    fn capabilities(&self) -> &[VendorCapability];

    /// Requirements a design must satisfy before sending.
    fn requirements(&self) -> &VendorRequirements;

    /// Prepare a quote request with the given files and options.
    ///
    /// Returns the prepared request with file hashes and vendor info.
    fn prepare_quote(
        &self,
        files: Vec<QuoteFile>,
        options: QuoteOptions,
    ) -> Result<QuotePrepared, String>;

    /// Prepare an order with the given files and options.
    ///
    /// Returns the prepared order with file hashes and vendor info.
    fn prepare_order(
        &self,
        files: Vec<QuoteFile>,
        options: QuoteOptions,
    ) -> Result<OrderPrepared, String>;
}

/// JLCPCB upload-flow adapter.
///
/// Opens the JLCPCB order page with a prepared archive.
/// No API keys required; the user finishes the order in the browser.
pub struct JlcpcbUploadAdapter;

impl JlcpcbUploadAdapter {
    /// Create a new JLCPCB upload-flow adapter.
    pub fn new() -> Self {
        Self
    }
}

impl Default for JlcpcbUploadAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Contract: crates/library/src/vendor_integrations.rs
impl VendorAdapter for JlcpcbUploadAdapter {
    fn name(&self) -> &str {
        "JLCPCB"
    }

    fn host(&self) -> &str {
        "jlcpcb.com"
    }

    fn capabilities(&self) -> &[VendorCapability] {
        &[VendorCapability::Quote, VendorCapability::UploadFlow]
    }

    fn requirements(&self) -> &VendorRequirements {
        static REQS: std::sync::LazyLock<VendorRequirements> =
            std::sync::LazyLock::new(VendorRequirements::default);
        &REQS
    }

    fn prepare_quote(
        &self,
        files: Vec<QuoteFile>,
        options: QuoteOptions,
    ) -> Result<QuotePrepared, String> {
        if files.is_empty() {
            return Err("no files provided for quote".to_string());
        }
        Ok(QuotePrepared {
            files,
            vendor: self.name().to_string(),
            host: self.host().to_string(),
            options,
        })
    }

    fn prepare_order(
        &self,
        files: Vec<QuoteFile>,
        options: QuoteOptions,
    ) -> Result<OrderPrepared, String> {
        if files.is_empty() {
            return Err("no files provided for order".to_string());
        }
        Ok(OrderPrepared {
            files,
            vendor: self.name().to_string(),
            host: self.host().to_string(),
            options,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{JlcpcbUploadAdapter, QuoteFile, QuoteOptions, VendorAdapter, VendorCapability};

    #[test]
    fn jlcpcb_adapter_has_quote_capability() {
        let adapter = JlcpcbUploadAdapter::new();
        assert!(adapter.capabilities().contains(&VendorCapability::Quote));
        assert!(
            adapter
                .capabilities()
                .contains(&VendorCapability::UploadFlow)
        );
        assert_eq!(adapter.name(), "JLCPCB");
        assert_eq!(adapter.host(), "jlcpcb.com");
    }

    #[test]
    fn jlcpcb_prepare_quote_requires_files() {
        let adapter = JlcpcbUploadAdapter::new();
        let result = adapter.prepare_quote(Vec::new(), QuoteOptions::default());
        assert!(result.is_err());
    }

    #[test]
    fn jlcpcb_prepare_quote_succeeds_with_files() -> Result<(), String> {
        let adapter = JlcpcbUploadAdapter::new();
        let files = vec![QuoteFile {
            name: "gerbers.zip".to_string(),
            size: 1024,
            sha256: "abc123".to_string(),
        }];
        let prepared = adapter
            .prepare_quote(files, QuoteOptions::default())
            .map_err(|e| format!("a quote with files is prepared: {e:?}"))?;
        assert_eq!(prepared.vendor, "JLCPCB");
        assert_eq!(prepared.host, "jlcpcb.com");
        assert_eq!(prepared.files.len(), 1);
        Ok(())
    }
}
