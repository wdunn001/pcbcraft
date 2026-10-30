//! Typed sourcing facts on a placed component.
//!
//! Free-form `Component::fields` stay on the component; typed values win when
//! both are set. See issue #649.

use crate::Component;
use pcbcraft_library::listing::{Lifecycle, Packaging, PartListing};
use pcbcraft_library::part_fields::{
    ALTERNATES, DATASHEET, HALOGEN_FREE, LIFECYCLE, MANUFACTURER, MODEL3D, MOISTURE_LEVEL, MPN,
    PACKAGING, REACH, ROHS,
};
use serde::{Deserialize, Serialize};

/// Longest manufacturer, part number or alternate text, in bytes.
pub const MAX_SOURCING_TEXT: usize = 256;
/// Longest link field, in bytes.
pub const MAX_SOURCING_URL: usize = 512;
/// Most alternate parts one component lists.
pub const MAX_ALTERNATES: usize = 8;

/// One alternate source: another maker and part number pair.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PartAlternate {
    pub manufacturer: String,
    pub mpn: String,
}

/// Typed sourcing record. Empty records are not saved on the component.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Sourcing {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manufacturer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mpn: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moisture_level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rohs: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub halogen_free: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packaging: Option<Packaging>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternates: Vec<PartAlternate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datasheet_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model3d_url: Option<String>,
}

fn bounded(name: &str, text: &str, max: usize) -> Result<(), String> {
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(format!(
            "{name} must be at most {max} bytes without control characters"
        ));
    }
    Ok(())
}

impl PartAlternate {
    pub fn validate(&self) -> Result<(), String> {
        if self.manufacturer.trim().is_empty() || self.mpn.trim().is_empty() {
            return Err("an alternate needs a manufacturer and a part number".into());
        }
        bounded("alternate manufacturer", &self.manufacturer, MAX_SOURCING_TEXT)?;
        bounded("alternate part number", &self.mpn, MAX_SOURCING_TEXT)?;
        Ok(())
    }
}

impl Sourcing {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), String> {
        bounded("manufacturer", &self.manufacturer, MAX_SOURCING_TEXT)?;
        bounded("manufacturer part number", &self.mpn, MAX_SOURCING_TEXT)?;
        if self.alternates.len() > MAX_ALTERNATES {
            return Err(format!("a component lists at most {MAX_ALTERNATES} alternates"));
        }
        for alt in &self.alternates {
            alt.validate()?;
        }
        if self.moisture_level.is_some_and(|m| !(1..=6).contains(&m)) {
            return Err("moisture level must be 1 to 6".into());
        }
        if let Some(url) = &self.datasheet_url {
            bounded("datasheet link", url, MAX_SOURCING_URL)?;
        }
        if let Some(url) = &self.model3d_url {
            bounded("3D model link", url, MAX_SOURCING_URL)?;
        }
        Ok(())
    }

    /// Copy matching fields from a listing. When `overwrite` is false, typed
    /// values already set on the component are kept.
    pub fn apply_listing(&mut self, listing: &PartListing, overwrite: bool) {
        let text = |slot: &mut String, src: &str| {
            if !overwrite && !slot.is_empty() {
                return;
            }
            let src = src.trim();
            if src.is_empty() {
                return;
            }
            if bounded("field", src, MAX_SOURCING_TEXT).is_ok() {
                *slot = src.to_owned();
            }
        };
        text(&mut self.manufacturer, &listing.manufacturer);
        text(&mut self.mpn, &listing.mpn);
        let opt = |slot: &mut Option<_>, src: Option<_>, overwrite: bool| {
            if !overwrite && slot.is_some() {
                return;
            }
            if src.is_some() {
                *slot = src;
            }
        };
        opt(&mut self.lifecycle, Some(listing.lifecycle), overwrite);
        opt(&mut self.moisture_level, listing.moisture_level, overwrite);
        opt(&mut self.rohs, listing.rohs, overwrite);
        opt(&mut self.reach, listing.reach, overwrite);
        opt(&mut self.halogen_free, listing.halogen_free, overwrite);
        opt(&mut self.packaging, listing.packaging, overwrite);
        if overwrite || self.datasheet_url.is_none() {
            if let Some(url) = listing.datasheet_url.as_deref() {
                if bounded("datasheet link", url, MAX_SOURCING_URL).is_ok() {
                    self.datasheet_url = Some(url.to_owned());
                }
            }
        }
        if overwrite || self.model3d_url.is_none() {
            if let Some(url) = listing.model3d.as_deref() {
                if bounded("3D model link", url, MAX_SOURCING_URL).is_ok() {
                    self.model3d_url = Some(url.to_owned());
                }
            }
        }
    }
}

fn plain(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_alphanumeric() || *c == '#')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl Component {
    /// Copy free-form manufacturer, MPN and datasheet keys into typed fields
    /// when the typed slot is still empty.
    pub fn adopt_free_form_sourcing(&mut self) {
        let mut next = self.sourcing.clone();
        for f in &self.fields {
            let value = f.value.trim();
            if value.is_empty() {
                continue;
            }
            match plain(&f.key).as_str() {
                "manufacturer" | "mfr" | "mfg" | "manufacturername" => {
                    if next.manufacturer.is_empty()
                        && bounded("manufacturer", value, MAX_SOURCING_TEXT).is_ok()
                    {
                        next.manufacturer = value.to_owned();
                    }
                }
                "mpn" | "manufacturerpartnumber" | "mfrpartnumber" | "mfgpartnumber" => {
                    if next.mpn.is_empty() && bounded("mpn", value, MAX_SOURCING_TEXT).is_ok() {
                        next.mpn = value.to_owned();
                    }
                }
                "datasheet" | "datasheeturl" | "datasheetlink" => {
                    if next.datasheet_url.is_none()
                        && bounded("datasheet link", value, MAX_SOURCING_URL).is_ok()
                    {
                        next.datasheet_url = Some(value.to_owned());
                    }
                }
                "3dmodel" | "model3d" | "3dmodelurl" => {
                    if next.model3d_url.is_none()
                        && bounded("3D model link", value, MAX_SOURCING_URL).is_ok()
                    {
                        next.model3d_url = Some(value.to_owned());
                    }
                }
                _ => {}
            }
        }
        if next.validate().is_ok() {
            self.sourcing = next;
        }
    }

    fn typed_or_field<'a>(&'a self, typed: &'a str, key: &str) -> &'a str {
        if typed.is_empty() {
            self.field(key).unwrap_or("")
        } else {
            typed
        }
    }

    pub fn manufacturer(&self) -> &str {
        self.typed_or_field(&self.sourcing.manufacturer, MANUFACTURER)
    }

    pub fn mpn(&self) -> &str {
        self.typed_or_field(&self.sourcing.mpn, MPN)
    }

    /// Text for a BOM column key: typed sourcing first, then free-form fields.
    pub fn bom_value(&self, key: &str) -> String {
        let yes_no = |v: Option<bool>| match v {
            Some(true) => "yes".to_owned(),
            Some(false) => "no".to_owned(),
            None => String::new(),
        };
        let s = &self.sourcing;
        match key {
            k if k == MANUFACTURER => self.manufacturer().to_owned(),
            k if k == MPN => self.mpn().to_owned(),
            k if k == LIFECYCLE => s
                .lifecycle
                .map(lifecycle_text)
                .unwrap_or_default(),
            k if k == MOISTURE_LEVEL => s.moisture_level.map(|m| m.to_string()).unwrap_or_default(),
            k if k == ROHS => yes_no(s.rohs),
            k if k == REACH => yes_no(s.reach),
            k if k == HALOGEN_FREE => yes_no(s.halogen_free),
            k if k == PACKAGING => s
                .packaging
                .map(packaging_text)
                .unwrap_or_default(),
            k if k == DATASHEET => s
                .datasheet_url
                .clone()
                .or_else(|| self.field(DATASHEET).map(str::to_owned))
                .unwrap_or_default(),
            k if k == MODEL3D => s.model3d_url.clone().unwrap_or_default(),
            k if k == ALTERNATES => s
                .alternates
                .iter()
                .map(|a| format!("{}: {}", a.manufacturer, a.mpn))
                .collect::<Vec<_>>()
                .join("; "),
            _ => self.field(key).unwrap_or("").to_owned(),
        }
    }
}

pub fn lifecycle_text(l: Lifecycle) -> String {
    match l {
        Lifecycle::Active => "active".into(),
        Lifecycle::Nrnd => "not recommended for new designs".into(),
        Lifecycle::LastTimeBuy => "last time buy".into(),
        Lifecycle::Obsolete => "obsolete".into(),
        Lifecycle::Unknown => "unknown".into(),
    }
}

fn packaging_text(p: Packaging) -> String {
    match p {
        Packaging::Reel => "reel".into(),
        Packaging::CutTape => "cut tape".into(),
        Packaging::Tray => "tray".into(),
        Packaging::Tube => "tube".into(),
        Packaging::Bulk => "bulk".into(),
        Packaging::Other => "other".into(),
    }
}

impl crate::Project {
    pub fn adopt_free_form_sourcing(&mut self) {
        for c in &mut self.components {
            c.adopt_free_form_sourcing();
        }
    }
}
