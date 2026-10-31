//! Generic pin map rows built from a validated project.
use pcbcraft_doc::Project;
use std::collections::BTreeMap;

/// One connected schematic pin on a named net.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PinMapRow {
    pub net: String,
    pub reference: String,
    pub pin: String,
}

const MAX_PINS: usize = 50_000;

/// Collect every pin with a net assignment into sorted rows.
///
/// # Rules
/// - Calls `Project::validate` before walking components.
/// - Refuses when the pin count exceeds `MAX_PINS`.
///
/// # Dependencies
/// - crates/doc/src/lib.rs `Project`, component pins and nets.
///
/// # Do not
/// - Include unassigned pins or board-only data; schematic connectivity only.
pub fn pin_map_rows(p: &Project) -> Result<Vec<PinMapRow>, String> {
    p.validate()?;
    let names: BTreeMap<u64, String> = p.nets.iter().map(|n| (n.id, n.name.clone())).collect();
    let mut rows = Vec::new();
    for component in &p.components {
        let reference = component.reference.clone();
        for pin in &component.pins {
            let Some(net_id) = pin.net else {
                continue;
            };
            if rows.len() >= MAX_PINS {
                return Err("pin map exceeds the pin limit".into());
            }
            let net = names
                .get(&net_id)
                .ok_or_else(|| "pin references an unknown net".to_string())?
                .clone();
            rows.push(PinMapRow {
                net,
                reference: reference.clone(),
                pin: pin.number.clone(),
            });
        }
    }
    Ok(rows)
}
