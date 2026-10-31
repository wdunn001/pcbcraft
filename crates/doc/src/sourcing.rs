//! Typed sourcing facts of one part: who makes it, what it is called, where to buy it.
//!
//! Context: issue #227 (structured part fields) and #804 (typed sourcing fields). Vendor
//! BOM formats match on manufacturer plus part number, or on a supplier's own number, so
//! those facts cannot live only in free-form `Component::fields`.
//!
//! Local rules: everything here is plain data with bounded text. The old free-form fields
//! stay on the component; [`Component::adopt_free_form_sourcing`] copies matching ones in
//! on load and never removes the originals. A typed value wins over a free-form one.
//!
//! # Dependencies
//! - `crates/doc/src/lib.rs` for `Component`, `Project`.
//! - `crates/library/src/listing.rs` for `Lifecycle`.
//!
//! # Do not
//! - Store a derived fact here; mount type comes from the pads.
//! - Add a second place for a part number; suppliers hold one number each.

use crate::{Component, Project};
use pcbcraft_library::listing::Lifecycle;
use serde::{Deserialize, Serialize};

/// Longest manufacturer, part number, supplier name or description, in bytes.
pub const MAX_SOURCING_TEXT: usize = 256;
/// Longest supplier link, in bytes.
pub const MAX_SUPPLIER_URL: usize = 512;
/// Most suppliers one part lists.
pub const MAX_SUPPLIERS: usize = 16;

/// One supplier's number for a part, with an optional link to its page.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SupplierPart {
    /// Who sells it, such as a distributor or an assembly house. Unique per part, ignoring case.
    pub supplier: String,
    /// The supplier's own number for the part.
    pub part_number: String,
    /// Link to the part on the supplier's site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// The typed sourcing record of a component. Every field is optional; an empty record
/// is not saved.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Sourcing {
    /// Maker of the part.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manufacturer: String,
    /// The maker's part number.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mpn: String,
    /// What the part is, in words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// One number per supplier, in the order they were added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppliers: Vec<SupplierPart>,
    /// Where the part is in its life.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,
    /// Moisture sensitivity level, 1 (unlimited floor life) to 6 (bake before use).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moisture_level: Option<u8>,
    /// True when the part meets RoHS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rohs: Option<bool>,
    /// True when the part has no REACH substance of very high concern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach: Option<bool>,
    /// True when the part is halogen free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub halogen_free: Option<bool>,
}

/// Check one bounded text: at most `max` bytes and no control characters.
fn bounded(name: &str, text: &str, max: usize) -> Result<(), String> {
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(format!(
            "{name} must be at most {max} bytes without control characters"
        ));
    }
    Ok(())
}

impl SupplierPart {
    /// Check the record against the documented bounds.
    ///
    /// # Rules
    /// - Supplier and number are non-blank; text is bounded and has no control characters.
    ///
    /// # Do not
    /// - Fetch the link; it is only stored.
    pub fn validate(&self) -> Result<(), String> {
        if self.supplier.trim().is_empty() || self.part_number.trim().is_empty() {
            return Err("a supplier part needs a supplier and a part number".into());
        }
        bounded("supplier", &self.supplier, MAX_SOURCING_TEXT)?;
        bounded("supplier part number", &self.part_number, MAX_SOURCING_TEXT)?;
        if let Some(url) = &self.url {
            bounded("supplier link", url, MAX_SUPPLIER_URL)?;
        }
        Ok(())
    }
}

impl Sourcing {
    /// True when nothing is recorded.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Check the record against the documented bounds.
    ///
    /// # Rules
    /// - Text fields are bounded; suppliers are unique ignoring case and at most [`MAX_SUPPLIERS`].
    /// - Moisture level is 1 to 6.
    ///
    /// # Dependencies
    /// - `crates/engine/src/lib.rs` `component.set_sourcing` and `component.set_supplier` call it.
    pub fn validate(&self) -> Result<(), String> {
        bounded("manufacturer", &self.manufacturer, MAX_SOURCING_TEXT)?;
        bounded("manufacturer part number", &self.mpn, MAX_SOURCING_TEXT)?;
        bounded("description", &self.description, MAX_SOURCING_TEXT)?;
        if self.suppliers.len() > MAX_SUPPLIERS {
            return Err(format!("a part lists at most {MAX_SUPPLIERS} suppliers"));
        }
        for (i, s) in self.suppliers.iter().enumerate() {
            s.validate()?;
            if self.suppliers[..i]
                .iter()
                .any(|o| o.supplier.eq_ignore_ascii_case(&s.supplier))
            {
                return Err(format!("supplier {} is listed twice", s.supplier));
            }
        }
        if self.moisture_level.is_some_and(|m| !(1..=6).contains(&m)) {
            return Err("moisture level must be 1 to 6".into());
        }
        Ok(())
    }

    /// The supplier's number, matching the supplier name ignoring case.
    pub fn supplier_number(&self, supplier: &str) -> Option<&str> {
        self.suppliers
            .iter()
            .find(|s| s.supplier.eq_ignore_ascii_case(supplier))
            .map(|s| s.part_number.as_str())
    }

    /// Set a supplier's number and link, replacing the entry of the same supplier or
    /// adding one. `None` for the number removes the supplier.
    ///
    /// # Rules
    /// - Order of the other suppliers does not change.
    /// - On error the record is unchanged.
    ///
    /// # Do not
    /// - Call it with a blank supplier name; it is refused.
    pub fn set_supplier(
        &mut self,
        supplier: &str,
        part_number: Option<&str>,
        url: Option<&str>,
    ) -> Result<(), String> {
        let at = self
            .suppliers
            .iter()
            .position(|s| s.supplier.eq_ignore_ascii_case(supplier.trim()));
        let Some(number) = part_number else {
            return match at {
                Some(i) => {
                    self.suppliers.remove(i);
                    Ok(())
                }
                None => Err("the part lists no such supplier".into()),
            };
        };
        let url = url.map(str::trim).filter(|u| !u.is_empty());
        let entry = SupplierPart {
            supplier: at
                .map(|i| self.suppliers[i].supplier.clone())
                .unwrap_or_else(|| supplier.trim().to_owned()),
            part_number: number.trim().to_owned(),
            url: url.map(str::to_owned),
        };
        entry.validate()?;
        match at {
            Some(i) => self.suppliers[i] = entry,
            None => {
                if self.suppliers.len() >= MAX_SUPPLIERS {
                    return Err(format!("a part lists at most {MAX_SUPPLIERS} suppliers"));
                }
                self.suppliers.push(entry);
            }
        }
        Ok(())
    }
}

/// A key with spaces, dashes, dots and underscores dropped and ASCII lowered.
fn plain(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_alphanumeric() || *c == '#')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The supplier a free-form key names, when it reads like "Digi-Key PN" or "Mouser SKU".
fn supplier_of_key(key: &str) -> Option<String> {
    let key = key.trim();
    let lower = key.to_ascii_lowercase();
    for tail in [" part number", " part no", " part #", " pn", " sku"] {
        if let Some(head) = lower.strip_suffix(tail) {
            let name = key[..head.len()].trim();
            let p = plain(name);
            let maker = [
                "manufacturer",
                "mfr",
                "mfg",
                "maker",
                "customer",
                "internal",
            ];
            if !p.is_empty() && !maker.contains(&p.as_str()) {
                return Some(name.to_owned());
            }
        }
    }
    None
}

/// The standard key a typed value answers to in BOM columns: `Supplier:<name>` reads a
/// supplier's number.
pub const SUPPLIER_KEY_PREFIX: &str = "Supplier:";

impl Component {
    /// Copy matching free-form fields into the typed record, leaving the originals alone.
    ///
    /// Why: files written before the typed fields hold `MPN`, `Manufacturer`,
    /// `Description` and keys like `Digi-Key PN` as free-form text; tools read the typed
    /// record.
    ///
    /// # Rules
    /// - Keys match ignoring case, spaces, dashes and dots (`Mfr Part Number` is an MPN).
    /// - A typed value that is already set is kept; so is an existing supplier of that name.
    /// - A value that fails the bounds is left in the free-form field only.
    /// - Safe to run twice; the second run changes nothing.
    ///
    /// # Dependencies
    /// - `crates/io/src/generate.rs` `migrate_loaded_project` and `crates/engine/src/lib.rs`
    ///   `Session::replace` call it through [`Project::adopt_free_form_sourcing`].
    ///
    /// # Do not
    /// - Delete or edit `fields`.
    pub fn adopt_free_form_sourcing(&mut self) {
        let mut next = self.sourcing.clone();
        for f in &self.fields {
            let value = f.value.trim();
            if value.is_empty() {
                continue;
            }
            let slot = match plain(&f.key).as_str() {
                "manufacturer" | "mfr" | "mfg" | "manufacturername" | "mfrname" => {
                    Some(&mut next.manufacturer)
                }
                "mpn"
                | "manufacturerpartnumber"
                | "mfrpartnumber"
                | "mfgpartnumber"
                | "manufacturerpn"
                | "mfrpn"
                | "mfgpn"
                | "manufacturerpart" => Some(&mut next.mpn),
                "description" => Some(&mut next.description),
                _ => None,
            };
            if let Some(slot) = slot {
                if slot.is_empty() && bounded("field", value, MAX_SOURCING_TEXT).is_ok() {
                    *slot = value.to_owned();
                }
                continue;
            }
            if let Some(name) = supplier_of_key(&f.key)
                && next.supplier_number(&name).is_none()
            {
                let _ = next.set_supplier(&name, Some(value), None);
            }
        }
        if next.validate().is_ok() {
            self.sourcing = next;
        }
    }

    /// Manufacturer: the typed value, else the free-form `Manufacturer` field.
    pub fn manufacturer(&self) -> &str {
        self.typed_or_field(&self.sourcing.manufacturer, "Manufacturer")
    }

    /// Manufacturer part number: the typed value, else the free-form `MPN` field.
    pub fn mpn(&self) -> &str {
        self.typed_or_field(&self.sourcing.mpn, "MPN")
    }

    /// Description: the typed value, else the free-form `Description` field.
    pub fn description(&self) -> &str {
        self.typed_or_field(&self.sourcing.description, "Description")
    }

    fn typed_or_field<'a>(&'a self, typed: &'a str, key: &str) -> &'a str {
        if typed.is_empty() {
            self.field(key).unwrap_or("")
        } else {
            typed
        }
    }

    /// The text a bill of materials column shows for `key`.
    ///
    /// Standard keys read the typed record (`Manufacturer`, `MPN`, `Description`,
    /// `Lifecycle`, `Moisture level`, `RoHS`, `REACH`, `Halogen free`, `Suppliers` for
    /// every supplier as `Name: number; ...`, and `Supplier:<name>` for one number);
    /// any other key reads the free-form field.
    ///
    /// # Rules
    /// - Unset gives an empty string, never a placeholder.
    ///
    /// # Dependencies
    /// - `crates/engine/src/lib.rs` `bom_export` reads every column through it.
    pub fn bom_value(&self, key: &str) -> String {
        let yes_no = |v: Option<bool>| match v {
            Some(true) => "yes".to_owned(),
            Some(false) => "no".to_owned(),
            None => String::new(),
        };
        let s = &self.sourcing;
        match key {
            "Manufacturer" => self.manufacturer().to_owned(),
            "MPN" => self.mpn().to_owned(),
            "Description" => self.description().to_owned(),
            "Suppliers" => s
                .suppliers
                .iter()
                .map(|p| format!("{}: {}", p.supplier, p.part_number))
                .collect::<Vec<_>>()
                .join("; "),
            "Lifecycle" => s
                .lifecycle
                .map(|l| lifecycle_text(l).to_owned())
                .unwrap_or_default(),
            "Moisture level" => s.moisture_level.map(|m| m.to_string()).unwrap_or_default(),
            "RoHS" => yes_no(s.rohs),
            "REACH" => yes_no(s.reach),
            "Halogen free" => yes_no(s.halogen_free),
            _ => match key.strip_prefix(SUPPLIER_KEY_PREFIX) {
                Some(name) => s.supplier_number(name).unwrap_or("").to_owned(),
                None => self.field(key).unwrap_or("").to_owned(),
            },
        }
    }
}

/// The words a lifecycle shows in a column and in the Sourcing section.
pub fn lifecycle_text(l: Lifecycle) -> &'static str {
    match l {
        Lifecycle::Active => "active",
        Lifecycle::Nrnd => "not recommended for new designs",
        Lifecycle::LastTimeBuy => "last time buy",
        Lifecycle::Obsolete => "obsolete",
        Lifecycle::Unknown => "unknown",
    }
}

impl Project {
    /// Run [`Component::adopt_free_form_sourcing`] on every component.
    pub fn adopt_free_form_sourcing(&mut self) {
        for c in &mut self.components {
            c.adopt_free_form_sourcing();
        }
    }
}
