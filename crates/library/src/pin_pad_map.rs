//! Symbol pin to footprint pad mapping for part verification and replacement checks.
//!
//! # Rules
//! - Auto fill links pads to pins when numbers match; one pin may claim several pads.
//! - Pads with no pin stay unclaimed until marked mechanical, thermal or no-connect.
//! - Empty pad numbers are mechanical by default when the pad is unplated.
//!
//! # Dependencies
//! - crates/library/src/lib.rs (`SymbolDefinition`, `FootprintDefinition`)
//!
//! # Do not
//! - Guess signal names; only numbers and explicit roles drive the map.

use crate::{FootprintDefinition, FootprintPad, SymbolDefinition, SymbolPin};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// How a footprint pad participates in electrical pinout.
///
/// # Rules
/// - Electrical pads must appear in the pin map or validation fails.
/// - Mechanical, thermal and no-connect pads may stay unmapped when their role is set.
///
/// # Dependencies
/// - crates/library/src/pin_pad_map.rs `PinPadMap::set_pad_role`
///
/// # Do not
/// - Treat plated signal pads as mechanical without an explicit role change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadRole {
    #[default]
    Electrical,
    Mechanical,
    Thermal,
    NoConnect,
}

/// One schematic pin and the footprint pads it drives.
///
/// # Rules
/// - `pin` is the symbol pin number as stored on `SymbolPin`.
/// - `pads` lists stable pad labels from `pad_label` / `footprint_pad_labels`.
///
/// # Dependencies
/// - crates/library/src/lib.rs `SymbolPin`
///
/// # Do not
/// - Store duplicate pad labels in one row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinPadRow {
    pub pin: String,
    #[serde(default)]
    pub pads: Vec<String>,
}

/// Full pin-to-pad table plus per-pad roles.
///
/// # Rules
/// - Rows follow symbol pin order from `auto_fill` then sort by pin when remapped.
/// - `pad_roles` keys use the same labels as `pads` entries.
///
/// # Dependencies
/// - crates/library/src/pin_pad_map.rs `auto_fill`, `validate_pin_pad_map`
///
/// # Do not
/// - Serialize pad roles without labels; keys must match the footprint instance.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinPadMap {
    #[serde(default)]
    pub rows: Vec<PinPadRow>,
    #[serde(default)]
    pub pad_roles: BTreeMap<String, PadRole>,
}

fn numbered_pad_counts(pads: &[FootprintPad]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for pad in pads {
        let n = pad.number.trim();
        if !n.is_empty() {
            *counts.entry(n.to_owned()).or_insert(0) += 1;
        }
    }
    counts
}

/// Stable label for a pad in messages and maps. Repeated numbers get `#index`.
pub fn pad_label(pad: &FootprintPad, index: usize, counts: &BTreeMap<String, usize>) -> String {
    let n = pad.number.trim();
    if n.is_empty() {
        format!("pad {index}")
    } else if counts.get(n).copied().unwrap_or(0) > 1 {
        format!("{n}#{index}")
    } else {
        n.to_owned()
    }
}

/// One label per footprint pad, in pad order.
pub fn footprint_pad_labels(footprint: &FootprintDefinition) -> Vec<String> {
    let counts = numbered_pad_counts(&footprint.pads);
    footprint
        .pads
        .iter()
        .enumerate()
        .map(|(i, p)| pad_label(p, i, &counts))
        .collect()
}

fn default_pad_role(pad: &FootprintPad) -> PadRole {
    if pad.pad_type == "unplated" || pad.number.trim().is_empty() {
        PadRole::Mechanical
    } else {
        PadRole::Electrical
    }
}

fn pin_numbers(symbol: &SymbolDefinition) -> BTreeSet<String> {
    symbol
        .pins
        .iter()
        .map(|p| p.number.clone())
        .collect()
}

fn pads_by_number(footprint: &FootprintDefinition) -> BTreeMap<String, Vec<String>> {
    let counts = numbered_pad_counts(&footprint.pads);
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (i, pad) in footprint.pads.iter().enumerate() {
        let label = pad_label(pad, i, &counts);
        let key = pad.number.trim();
        if key.is_empty() {
            continue;
        }
        out.entry(key.to_owned()).or_default().push(label);
    }
    out
}

/// Build a map by matching pin numbers to pad numbers. Repeated pad numbers attach to one pin.
pub fn auto_fill(symbol: &SymbolDefinition, footprint: &FootprintDefinition) -> PinPadMap {
    let grouped = pads_by_number(footprint);
    let mut rows = Vec::new();
    for pin in &symbol.pins {
        let mut pads = grouped.get(&pin.number).cloned().unwrap_or_default();
        pads.sort();
        rows.push(PinPadRow {
            pin: pin.number.clone(),
            pads,
        });
    }
    let counts = numbered_pad_counts(&footprint.pads);
    let mut pad_roles = BTreeMap::new();
    for (i, pad) in footprint.pads.iter().enumerate() {
        pad_roles.insert(pad_label(pad, i, &counts), default_pad_role(pad));
    }
    PinPadMap { rows, pad_roles }
}

impl PinPadMap {
    /// Pads currently assigned to `pin`, if that pin has a row.
    pub fn pads_for_pin(&self, pin: &str) -> Option<&[String]> {
        self.rows
            .iter()
            .find(|r| r.pin == pin)
            .map(|r| r.pads.as_slice())
    }

    /// Assign `pad` to `pin`, removing it from any other pin row first.
    pub fn assign_pad_to_pin(&mut self, pin: &str, pad: &str) {
        for row in &mut self.rows {
            row.pads.retain(|p| p != pad);
        }
        if let Some(row) = self.rows.iter_mut().find(|r| r.pin == pin) {
            if !row.pads.iter().any(|p| p == pad) {
                row.pads.push(pad.to_owned());
                row.pads.sort();
            }
        } else {
            self.rows.push(PinPadRow {
                pin: pin.to_owned(),
                pads: vec![pad.to_owned()],
            });
            self.rows.sort_by(|a, b| a.pin.cmp(&b.pin));
        }
    }

    /// Record how an unmapped or auxiliary pad should be treated in validation.
    pub fn set_pad_role(&mut self, pad: &str, role: PadRole) {
        self.pad_roles.insert(pad.to_owned(), role);
    }

    /// Role for `pad`, defaulting to electrical when unset.
    pub fn pad_role(&self, pad: &str) -> PadRole {
        self.pad_roles.get(pad).copied().unwrap_or(PadRole::Electrical)
    }

    /// Every pad label on the footprint.
    pub fn all_pad_labels(footprint: &FootprintDefinition) -> Vec<String> {
        footprint_pad_labels(footprint)
    }
}

fn is_nc_pin(pin: &SymbolPin) -> bool {
    pin.electrical_type == "no_connect"
}

fn pad_claimed(map: &PinPadMap, pad: &str) -> bool {
    map.rows.iter().any(|r| r.pads.iter().any(|p| p == pad))
}

/// One validation finding; `pin` or `pad` names the object when the issue is localised.
///
/// # Rules
/// - `code` is stable (`pin_unmapped`, `pad_unclaimed`, and similar).
/// - At least one of `pin` or `pad` is set for pin/pad specific codes.
///
/// # Dependencies
/// - crates/library/src/pin_pad_map.rs `validate_pin_pad_map`
///
/// # Do not
/// - Reuse codes for unrelated checks; part validation maps these into its own findings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinPadFinding {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad: Option<String>,
}

/// Validate that the map covers symbol pins and footprint pads consistently.
pub fn validate_pin_pad_map(
    symbol: &SymbolDefinition,
    footprint: &FootprintDefinition,
    map: &PinPadMap,
) -> Vec<PinPadFinding> {
    let mut findings = Vec::new();
    let pin_set = pin_numbers(symbol);
    let pad_labels = PinPadMap::all_pad_labels(footprint);
    let pad_set: BTreeSet<_> = pad_labels.iter().cloned().collect();

    for row in &map.rows {
        if !pin_set.contains(&row.pin) {
            findings.push(PinPadFinding {
                code: "pin_unknown".into(),
                message: format!("pin {} is not on symbol {}", row.pin, symbol.name),
                pin: Some(row.pin.clone()),
                pad: None,
            });
        }
        for pad in &row.pads {
            if !pad_set.contains(pad) {
                findings.push(PinPadFinding {
                    code: "pad_unknown".into(),
                    message: format!("pin {} claims {} which is not on footprint {}", row.pin, pad, footprint.name),
                    pin: Some(row.pin.clone()),
                    pad: Some(pad.clone()),
                });
            }
        }
    }

    for pad in &pad_labels {
        let role = map.pad_role(pad);
        let claimed = pad_claimed(map, pad);
        if role == PadRole::Electrical && !claimed {
            findings.push(PinPadFinding {
                code: "pad_unclaimed".into(),
                message: format!("{pad} on footprint {} is not mapped to a pin", footprint.name),
                pin: None,
                pad: Some(pad.clone()),
            });
        }
    }

    for pin in &symbol.pins {
        let mapped = map
            .pads_for_pin(&pin.number)
            .is_some_and(|pads| !pads.is_empty());
        if !mapped && !is_nc_pin(pin) {
            findings.push(PinPadFinding {
                code: "pin_unmapped".into(),
                message: format!(
                    "symbol pin {} has no footprint pad on {}",
                    pin.number, footprint.name
                ),
                pin: Some(pin.number.clone()),
                pad: None,
            });
        }
    }

    let mut pad_to_pin: BTreeMap<String, String> = BTreeMap::new();
    for row in &map.rows {
        for pad in &row.pads {
            if let Some(other) = pad_to_pin.insert(pad.clone(), row.pin.clone()) {
                if other != row.pin {
                    findings.push(PinPadFinding {
                        code: "pad_duplicate".into(),
                        message: format!("{pad} is mapped to both pin {other} and pin {}", row.pin),
                        pin: Some(row.pin.clone()),
                        pad: Some(pad.clone()),
                    });
                }
            }
        }
    }

    findings
}
