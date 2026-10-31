//! Shared helpers to read Gerber X3 component layers in integration tests.
use std::collections::BTreeMap;

/// One component flash read back from a Gerber X3 component layer.
#[derive(Debug, PartialEq)]
pub struct ComponentFlash {
    /// Designator from `%TO.C,...*%`.
    pub reference: String,
    /// Rotation in degrees from `%TO.CRot,...*%`.
    pub rotation: f64,
    /// Manufacturer string when present.
    pub manufacturer: String,
    /// Manufacturer part number when present.
    pub mpn: String,
    /// Mount type token from `%TO.CMnt,...*%`.
    pub mount: String,
    /// Flash X in millimetres.
    pub x_mm: f64,
    /// Flash Y in millimetres.
    pub y_mm: f64,
}

/// Collect component flashes and object attributes from a component layer file.
pub fn component_flashes(text: &str) -> Result<Vec<ComponentFlash>, String> {
    let mut out = Vec::new();
    let mut attrs: BTreeMap<String, String> = BTreeMap::new();
    let inch = text.contains("%MOIN*%");
    let scale = if inch { 25.4 } else { 1.0 };
    for line in text.lines() {
        if let Some(body) = line.strip_prefix("%TO.").and_then(|l| l.strip_suffix("*%")) {
            let (key, value) = body
                .split_once(',')
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .unwrap_or((body.to_string(), String::new()));
            attrs.insert(key, value);
        } else if line == "%TD*%" {
            attrs.clear();
        } else if line.ends_with("D03*") && attrs.contains_key("C") {
            let body = line.trim_end_matches("D03*");
            let (xs, ys) = body
                .strip_prefix('X')
                .and_then(|rest| rest.split_once('Y'))
                .ok_or("component flash without coordinates")?;
            let x: f64 = (xs.parse().map_err(|e| format!("{e}"))?) / 1e6 * scale;
            let y: f64 = (ys.parse().map_err(|e| format!("{e}"))?) / 1e6 * scale;
            let rotation = attrs
                .get("CRot")
                .ok_or("component flash without rotation")?
                .parse::<f64>()
                .map_err(|e| format!("{e}"))?;
            out.push(ComponentFlash {
                reference: attrs.get("C").cloned().unwrap_or_default(),
                rotation,
                manufacturer: attrs.get("CMfr").cloned().unwrap_or_default(),
                mpn: attrs.get("CMPN").cloned().unwrap_or_default(),
                mount: attrs.get("CMnt").cloned().unwrap_or_default(),
                x_mm: x,
                y_mm: y,
            });
            attrs.clear();
        }
    }
    out.sort_by(|a, b| a.reference.cmp(&b.reference));
    Ok(out)
}
