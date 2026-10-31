//! Schematic netlist for simulation export and optional DC readouts in SI CSV.
//!
//! # Rules
//! - Parts need `sim.model` (`R`, `V`, `I`; `C`, `L`, `D`, and `ignore` are skipped at DC).
//! - Values come from `sim.value` when set, otherwise from `value`.
//! - Ground is the net named in `sim.ground`, or a net named `GND` or `0`.
//!
//! # Dependencies
//! - docs/adr-0001-simulation.md
//! - crates/sim
//!
//! # Do not
//! - Run an external program; the SPICE file is the hand-off.

use pcbcraft_doc::{Component, Id, Project};
use pcbcraft_sim::{check, Element, Netlist, parse_value};
use std::collections::BTreeMap;

const MAX_NETS: usize = pcbcraft_sim::MAX_NODES;
const MAX_ELEMENTS: usize = pcbcraft_sim::MAX_ELEMENTS;

/// One node in the solved schematic, with its SPICE index and net name.
#[derive(Clone, Debug)]
pub struct SimNode {
    /// SPICE node number (0 is ground).
    pub index: usize,
    /// Logical net name for comments and CSV columns.
    pub name: String,
}

/// A schematic netlist ready for SPICE export or a DC solve.
#[derive(Clone, Debug)]
pub struct SchematicNetlist {
    /// The circuit in solver form.
    pub net: Netlist,
    /// Ground net id in the project.
    pub ground: Id,
    /// Ground net name.
    pub ground_name: String,
    /// Every net that appears in the circuit, by project net id.
    pub node_by_net: BTreeMap<Id, SimNode>,
}

struct GroundChoice {
    id: Id,
    name: String,
}

/// Builds the passive schematic netlist stamped by `sim.model` fields.
pub fn build_schematic_netlist(p: &Project) -> Result<SchematicNetlist, String> {
    p.validate()?;
    let ground = pick_ground(p)?;
    let mut node_by_net: BTreeMap<Id, SimNode> = BTreeMap::new();
    node_by_net.insert(
        ground.id,
        SimNode {
            index: 0,
            name: ground.name.clone(),
        },
    );
    let mut pending: Vec<(Id, String)> = vec![];
    for comp in &p.components {
        let Some(model) = comp.field("sim.model") else {
            continue;
        };
        let model = model.trim();
        if model.is_empty()
            || model.eq_ignore_ascii_case("ignore")
            || model.eq_ignore_ascii_case("D")
        {
            continue;
        }
        if model.eq_ignore_ascii_case("C") || model.eq_ignore_ascii_case("L") {
            continue;
        }
        if !matches!(
            model.to_ascii_uppercase().as_str(),
            "R" | "V" | "I"
        ) {
            return Err(format!(
                "{} has sim.model \"{model}\", which must be R, V, I, C, L, D or ignore",
                comp.reference
            ));
        }
        if comp.pins.len() != 2 {
            return Err(format!(
                "{} has sim.model {model} but {} pins; only two-pin parts are supported",
                comp.reference,
                comp.pins.len()
            ));
        }
        for pin in &comp.pins {
            let Some(net) = pin.net else {
                return Err(format!(
                    "{} pin {} is not on a net",
                    comp.reference, pin.number
                ));
            };
            if net == ground.id {
                continue;
            }
            if !node_by_net.contains_key(&net) {
                let name = p
                    .nets
                    .iter()
                    .find(|n| n.id == net)
                    .map(|n| n.name.clone())
                    .ok_or_else(|| {
                        format!("{} pin {} is on an unknown net", comp.reference, pin.number)
                    })?;
                pending.push((net, name));
            }
        }
    }
    pending.sort_by(|a, b| a.0.cmp(&b.0));
    for (net, name) in pending {
        if node_by_net.contains_key(&net) {
            continue;
        }
        let index = node_by_net.len();
        if index >= MAX_NETS {
            return Err(format!(
                "the circuit has more than {MAX_NETS} nodes including ground; simplify the schematic"
            ));
        }
        node_by_net.insert(
            net,
            SimNode {
                index,
                name,
            },
        );
    }
    let node_count = node_by_net.len();
    let mut net = Netlist::new(node_count)?;
    let mut stamped = 0usize;
    for comp in &p.components {
        let Some(model) = comp.field("sim.model") else {
            continue;
        };
        let upper = model.trim().to_ascii_uppercase();
        if upper.is_empty() || upper == "IGNORE" || upper == "D" || upper == "C" || upper == "L" {
            continue;
        }
        let value_text = comp
            .field("sim.value")
            .or_else(|| comp.value.as_deref())
            .ok_or_else(|| format!("{} needs a value for sim.model {upper}", comp.reference))?;
        let value = parse_value(value_text).map_err(|e| format!("{}: {e}", comp.reference))?;
        let node_a = node_index(p, &node_by_net, ground.id, comp, "1")?;
        let node_b = node_index(p, &node_by_net, ground.id, comp, "2")?;
        let element = match upper.as_str() {
            "R" => Element::Resistor {
                name: comp.reference.clone(),
                a: node_a,
                b: node_b,
                ohms: value,
            },
            "V" => Element::VoltageSource {
                name: comp.reference.clone(),
                plus: node_a,
                minus: node_b,
                volts: value,
            },
            "I" => Element::CurrentSource {
                name: comp.reference.clone(),
                from: node_a,
                to: node_b,
                amps: value,
            },
            _ => continue,
        };
        net.add(element)?;
        stamped += 1;
        if stamped > MAX_ELEMENTS {
            return Err(format!(
                "the circuit has more than {MAX_ELEMENTS} simulation elements"
            ));
        }
    }
    if stamped == 0 {
        return Err(
            "no parts have sim.model set to R, V or I; mark the parts you want in the netlist first"
                .into(),
        );
    }
    Ok(SchematicNetlist {
        net,
        ground: ground.id,
        ground_name: ground.name,
        node_by_net,
    })
}

fn pick_ground(p: &Project) -> Result<GroundChoice, String> {
    let mut named: Option<GroundChoice> = None;
    for comp in &p.components {
        let Some(net_name) = comp.field("sim.ground") else {
            continue;
        };
        let net_name = net_name.trim();
        if net_name.is_empty() {
            return Err(format!("{} has an empty sim.ground field", comp.reference));
        }
        let Some(net) = p.nets.iter().find(|n| n.name == net_name) else {
            return Err(format!(
                "{} sim.ground names \"{net_name}\", which is not a net in this design",
                comp.reference
            ));
        };
        let choice = GroundChoice {
            id: net.id,
            name: net.name.clone(),
        };
        if let Some(prev) = &named {
            if prev.id != choice.id {
                return Err(format!(
                    "sim.ground points to {} on one part and {} on another; pick one ground net",
                    prev.name, choice.name
                ));
            }
        } else {
            named = Some(choice);
        }
    }
    if let Some(choice) = named {
        return Ok(choice);
    }
    for label in ["GND", "0"] {
        if let Some(net) = p.nets.iter().find(|n| n.name == label) {
            return Ok(GroundChoice {
                id: net.id,
                name: net.name.clone(),
            });
        }
    }
    Err("no ground net: name one net GND or 0, or set sim.ground on a part".into())
}

fn node_index(
    p: &Project,
    map: &BTreeMap<Id, SimNode>,
    ground: Id,
    comp: &Component,
    pin: &str,
) -> Result<usize, String> {
    let pin = comp
        .pins
        .iter()
        .find(|q| q.number == pin)
        .ok_or_else(|| format!("{} has no pin {pin}", comp.reference))?;
    let Some(net) = pin.net else {
        return Err(format!("{} pin {pin} is not on a net", comp.reference));
    };
    if net == ground {
        return Ok(0);
    }
    map.get(&net)
        .map(|n| n.index)
        .ok_or_else(|| {
            p.nets
                .iter()
                .find(|n| n.id == net)
                .map(|n| {
                    format!(
                        "net {} is not in the exported circuit; connect it to a stamped part",
                        n.name
                    )
                })
                .unwrap_or_else(|| format!("{} pin {pin} is on an unknown net", comp.reference))
        })
}

fn spice_value(v: f64) -> String {
    format!("{v:.6e}")
}

/// Writes a SPICE netlist for hand-off to an outside solver.
pub fn spice_netlist(p: &Project) -> Result<String, String> {
    let built = build_schematic_netlist(p)?;
    check(&built.net)?;
    let title = if p.name.trim().is_empty() {
        "untitled design"
    } else {
        p.name.trim()
    };
    let mut out = format!("* SPICE netlist for {title}\n* Generated by PCBCraft; values are parsed, not copied from fields.\n");
    for node in built.node_by_net.values() {
        out.push_str(&format!(
            "* net {} is SPICE node {}\n",
            node.name, node.index
        ));
    }
    for element in built.net.elements() {
        let line = match element {
            Element::Resistor { name, a, b, ohms } => {
                format!("{name} {a} {b} {}\n", spice_value(*ohms))
            }
            Element::VoltageSource {
                name,
                plus,
                minus,
                volts,
            } => format!("{name} {plus} {minus} {}\n", spice_value(*volts)),
            Element::CurrentSource {
                name,
                from,
                to,
                amps,
            } => format!("{name} {from} {to} {}\n", spice_value(*amps)),
        };
        out.push_str(&line);
    }
    out.push_str(".END\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Component, ComponentField, Net, Pin, Project};

    #[test]
    fn spice_netlist_refuses_without_stamped_parts() {
        let p = Project::default();
        let err = spice_netlist(&p).err().unwrap_or_default();
        if !err.contains("sim.model") {
            panic!("unexpected: {err}");
        }
    }

    #[test]
    fn spice_netlist_writes_a_resistor_divider() -> Result<(), String> {
        let mut p = Project::default();
        p.nets = vec![
            Net {
                id: 1,
                name: "GND".into(),
            },
            Net {
                id: 2,
                name: "MID".into(),
            },
            Net {
                id: 3,
                name: "VIN".into(),
            },
        ];
        p.components.push(Component {
            id: 10,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "R".into(),
            position: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
            rotation: 0.0,
            pins: vec![
                Pin {
                    number: "1".into(),
                    name: "1".into(),
                    offset: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
                    net: Some(3),
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
                Pin {
                    number: "2".into(),
                    name: "2".into(),
                    offset: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
                    net: Some(2),
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![ComponentField {
                key: "sim.model".into(),
                value: "R".into(),
            }],
            sheet: 0,
        });
        p.components.push(Component {
            id: 11,
            reference: "R2".into(),
            value: "1k".into(),
            kind: "R".into(),
            position: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
            rotation: 0.0,
            pins: vec![
                Pin {
                    number: "1".into(),
                    name: "1".into(),
                    offset: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
                    net: Some(2),
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
                Pin {
                    number: "2".into(),
                    name: "2".into(),
                    offset: pcbcraft_geom::Point { x: 0.0, y: 0.0 },
                    net: Some(1),
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![ComponentField {
                key: "sim.model".into(),
                value: "R".into(),
            }],
            sheet: 0,
        });
        let text = spice_netlist(&p)?;
        if !text.contains("R1") || !text.contains("R2") || !text.contains(".END") {
            return Err("divider netlist incomplete".into());
        }
        Ok(())
    }
}
