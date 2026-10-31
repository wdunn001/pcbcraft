//! Direct-current simulation: build a netlist from the schematic and solve it.
//!
//! # Rules
//! - Read-only: `sim.dc` never changes the project or revision.
//! - Components need a `sim.model` field (`R`, `V`, `I`, `ignore`; `C` and `L` are open at DC).
//! - Values come from `sim.value` when set, otherwise from `value`.
//!
//! # Dependencies
//! - docs/adr-0001-simulation.md
//! - crates/sim
//!
//! # Do not
//! - Do not run an external program or touch the UI.
use super::*;
use pcbcraft_sim::{Element, Netlist, parse_value, solve_dc};
use std::collections::BTreeMap;

const MAX_NETS: usize = pcbcraft_sim::MAX_NODES;
const MAX_ELEMENTS: usize = pcbcraft_sim::MAX_ELEMENTS;

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "sim.dc",
        description: "Run a direct-current solve on the schematic using each part's sim.model field: R for resistors, V for voltage sources with pin 1 at the positive node and pin 2 at the negative node, I for current sources with pin 1 where current leaves and pin 2 where it enters; C and L are open at DC; D and ignore are skipped. Ground is the net named GND or 0, or the net named in sim.ground on any part. Returns node voltages by net name and current through each stamped element. Read only",
        input_schema: schema(json!({}), &[]),
    }]
}

pub(super) fn run_read(p: &Project, params: &Value) -> Result<Value, Error> {
    sim_dc(p, params)
}

fn sim_dc(p: &Project, params: &Value) -> Result<Value, Error> {
    if params.as_object().is_some_and(|o| !o.is_empty()) {
        return Err(Error::Invalid(
            "sim.dc takes no parameters; set sim.model and sim.value on the parts instead".into(),
        ));
    }
    let netlist = build_netlist(p).map_err(Error::Invalid)?;
    let ground = netlist.ground;
    let solution = solve_dc(&netlist.net).map_err(Error::Invalid)?;
    let mut nets: Vec<Value> = vec![];
    for (id, node) in &netlist.node_by_net {
        if *id == ground {
            continue;
        }
        let volts = solution.node_volts[node.index];
        if !volts.is_finite() {
            return Err(Error::Invalid(format!(
                "net {} has a voltage that is not a finite number",
                node.name
            )));
        }
        nets.push(json!({
            "id": id,
            "name": node.name,
            "volts": r9(volts),
        }));
    }
    nets.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["name"].as_str().unwrap_or_default())
    });
    let mut currents: BTreeMap<String, f64> = BTreeMap::new();
    for sc in &solution.source_currents {
        currents.insert(sc.name.clone(), sc.amps);
    }
    let mut elements: Vec<Value> = vec![];
    for element in netlist.net.elements() {
        let name = element.name().to_string();
        let amps = match element {
            Element::VoltageSource { .. } => currents.get(&name).copied(),
            Element::CurrentSource { amps, .. } => Some(*amps),
            Element::Resistor { a, b, ohms, .. } => {
                let va = solution.node_volts[*a];
                let vb = solution.node_volts[*b];
                Some((va - vb) / ohms)
            }
        };
        let Some(amps) = amps else {
            return Err(Error::Invalid(format!(
                "could not report current for {name}"
            )));
        };
        if !amps.is_finite() {
            return Err(Error::Invalid(format!(
                "{name} has a current that is not a finite number"
            )));
        }
        elements.push(json!({"name": name, "amps": r9(amps)}));
    }
    Ok(json!({
        "ground": {
            "id": ground,
            "name": netlist.ground_name,
        },
        "nets": nets,
        "elements": elements,
    }))
}

fn r9(v: f64) -> f64 {
    (v * 1e9).round() / 1e9
}

struct NodeInfo {
    index: usize,
    name: String,
}

struct BuiltNetlist {
    net: Netlist,
    ground: Id,
    ground_name: String,
    node_by_net: BTreeMap<Id, NodeInfo>,
}

struct GroundChoice {
    id: Id,
    name: String,
}

fn build_netlist(p: &Project) -> Result<BuiltNetlist, String> {
    let ground = pick_ground(p)?;
    let mut node_by_net: BTreeMap<Id, NodeInfo> = BTreeMap::new();
    node_by_net.insert(
        ground.id,
        NodeInfo {
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
        if model.is_empty() || model.eq_ignore_ascii_case("ignore") || model.eq_ignore_ascii_case("D")
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
                    .ok_or_else(|| format!("{} pin {} is on an unknown net", comp.reference, pin.number))?;
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
        node_by_net.insert(net, NodeInfo { index, name });
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
        let value = parse_value(value_text).map_err(|e| {
            format!("{}: {e}", comp.reference)
        })?;
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
            "no parts have sim.model set to R, V or I; mark the divider or source parts first"
                .into(),
        );
    }
    Ok(BuiltNetlist {
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
            return Err(format!(
                "{} has an empty sim.ground field",
                comp.reference
            ));
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
    map: &BTreeMap<Id, NodeInfo>,
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
                        "net {} is not in the solved circuit; connect it to a stamped part",
                        n.name
                    )
                })
                .unwrap_or_else(|| format!("{} pin {pin} is on an unknown net", comp.reference))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    fn missing() -> Error {
        Error::Invalid("missing field".into())
    }

    #[test]
    fn divider_fixture_returns_five_volts_and_leaves_revision_alone() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute("net.create", json!({"name":"GND"}), None)?;
        s.execute("net.create", json!({"name":"VCC"}), None)?;
        s.execute("net.create", json!({"name":"MID"}), None)?;
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"V1","position":{"x":0.0,"y":0.0}}),
            None,
        )?;
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","value":"1k","position":{"x":5.0,"y":0.0}}),
            None,
        )?;
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","value":"1k","position":{"x":10.0,"y":0.0}}),
            None,
        )?;
        s.execute(
            "netlist.import",
            json!({"nets":[
                {"name":"GND","pins":[{"reference":"V1","pin":"2"},{"reference":"R2","pin":"2"}]},
                {"name":"VCC","pins":[{"reference":"V1","pin":"1"},{"reference":"R1","pin":"1"}]},
                {"name":"MID","pins":[{"reference":"R1","pin":"2"},{"reference":"R2","pin":"1"}]},
            ]}),
            None,
        )?;
        for (reference, model, value) in [
            ("V1", "V", "10"),
            ("R1", "R", "1k"),
            ("R2", "R", "1k"),
        ] {
            s.execute(
                "component.field",
                json!({"reference":reference,"key":"sim.model","value":model}),
                None,
            )?;
            if model == "V" {
                s.execute(
                    "component.field",
                    json!({"reference":reference,"key":"sim.value","value":value}),
                    None,
                )?;
            }
        }
        let revision = s.revision;
        let out = s.execute("sim.dc", json!({}), None)?;
        assert_eq!(out["revision"], revision);
        let nets = out["result"]["nets"].as_array().ok_or_else(missing)?;
        let mid = nets
            .iter()
            .find(|n| n["name"] == "MID")
            .ok_or_else(missing)?;
        let volts = mid["volts"].as_f64().ok_or_else(missing)?;
        if (volts - 5.0).abs() > 1e-9 {
            return Err(Error::Invalid(format!("MID was {volts} V, wanted 5 V")));
        }
        Ok(())
    }
}
