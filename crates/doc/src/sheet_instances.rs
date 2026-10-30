//! Reusable schematic page instances: materialized copies on a parent page.

use crate::{
    Component, ComponentField, DrawnWire, Id, Label, MAX_ITEMS, Net, Pin, PinRef, Point, Project,
    Wire,
};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_SHEET_INSTANCES: usize = 64;
pub const MAX_INSTANCE_NAME: usize = 64;
pub const MAX_REFERENCE_OFFSET: u64 = 999_000;

/// One placed copy of a template schematic page on a parent page.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SheetInstance {
    pub id: Id,
    /// Template page that defines components and local connectivity.
    pub sheet: Id,
    pub name: String,
    pub reference_offset: u64,
    /// Parent schematic page where this copy is materialized.
    pub parent: Id,
    pub entry: Point,
}

pub fn reference_with_offset(reference: &str, offset: u64) -> Result<String, String> {
    let letters: String = reference
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    let digits: String = reference.chars().skip(letters.len()).collect();
    if letters.is_empty() || digits.is_empty() {
        return Err("reference needs a letter prefix and numeric suffix".into());
    }
    let base: u64 = digits
        .parse()
        .map_err(|_: std::num::ParseIntError| "reference numeric suffix is invalid".to_string())?;
    let sum = base
        .checked_add(offset)
        .ok_or_else(|| "reference offset would exceed the numeric suffix limit".to_string())?;
    let out = format!("{letters}{sum}");
    if out.len() > 64 {
        return Err("reference would exceed 64 bytes".into());
    }
    Ok(out)
}

pub fn allocate_id(project: &Project, next: &mut Id) -> Result<Id, String> {
    let id = *next;
    if id == 0 || id > u32::MAX as u64 {
        return Err("object ID space exhausted".into());
    }
    *next = next.saturating_add(1);
    let _ = project;
    Ok(id)
}

fn template_port_nets(project: &Project, sheet: Id) -> BTreeSet<Id> {
    project
        .ports
        .iter()
        .filter(|p| p.sheet == sheet)
        .map(|p| p.net)
        .collect()
}

fn net_used_on_sheet(project: &Project, sheet: Id, net: Id) -> bool {
    project
        .components
        .iter()
        .any(|c| c.sheet == sheet && c.pins.iter().any(|p| p.net == Some(net)))
        || project
            .ports
            .iter()
            .any(|p| p.sheet == sheet && p.net == net)
}

fn parent_net_for_port(project: &Project, parent: Id, port_name: &str) -> Result<Id, String> {
    project
        .ports
        .iter()
        .find(|p| p.sheet == parent && p.name == port_name)
        .map(|p| p.net)
        .ok_or_else(|| format!("parent page has no port {port_name:?} for this instance"))
}

pub fn expand_instance(
    project: &mut Project,
    inst: &SheetInstance,
    next: &mut Id,
) -> Result<(usize, usize), String> {
    let template = inst.sheet;
    if !project.sheets.iter().any(|s| s.id == template) {
        return Err("template sheet does not exist".into());
    }
    let template_parts: Vec<Component> = project
        .components
        .iter()
        .filter(|c| c.sheet == template)
        .cloned()
        .collect();
    if template_parts.is_empty() {
        return Err("template sheet has no components".into());
    }
    if !inst.entry.valid() {
        return Err("instance entry position is out of range".into());
    }
    let port_nets = template_port_nets(project, template);
    let mut net_map: BTreeMap<Id, Id> = BTreeMap::new();
    for port in project.ports.iter().filter(|p| p.sheet == template) {
        let parent_net = parent_net_for_port(project, inst.parent, &port.name)?;
        net_map.insert(port.net, parent_net);
    }
    let mut local_nets: BTreeSet<Id> = BTreeSet::new();
    for c in &template_parts {
        for pin in &c.pins {
            if let Some(net) = pin.net {
                if !port_nets.contains(&net) && net_used_on_sheet(project, template, net) {
                    local_nets.insert(net);
                }
            }
        }
    }
    for net in local_nets {
        if net_map.contains_key(&net) {
            continue;
        }
        let Some(old) = project.nets.iter().find(|n| n.id == net) else {
            return Err("template net is missing".into());
        };
        let suffix = format!("__{}", inst.name);
        let mut name = format!("{}{suffix}", old.name);
        if name.len() > 128 {
            name.truncate(128);
        }
        let mut candidate = name.clone();
        let mut n = 1u64;
        while project.nets.iter().any(|x| x.name == candidate) {
            candidate = format!("{name}_{n}");
            n += 1;
            if candidate.len() > 128 {
                return Err("could not allocate a unique local net name".into());
            }
        }
        let new_id = allocate_id(project, next)?;
        project.nets.push(Net {
            id: new_id,
            name: candidate,
            target_length: old.target_length,
            length_tolerance: old.length_tolerance,
            instance: Some(inst.id),
        });
        net_map.insert(net, new_id);
    }
    let mut component_map: BTreeMap<Id, Id> = BTreeMap::new();
    let mut new_components = 0usize;
    for old in &template_parts {
        let new_id = allocate_id(project, next)?;
        component_map.insert(old.id, new_id);
        let reference = reference_with_offset(&old.reference, inst.reference_offset)?;
        if project.components.iter().any(|c| c.reference == reference) {
            return Err(format!("reference {reference} is already in use"));
        }
        let mut pins: Vec<Pin> = Vec::with_capacity(old.pins.len());
        for p in &old.pins {
            let mut pin = p.clone();
            if let Some(net) = pin.net {
                let mapped = *net_map
                    .get(&net)
                    .ok_or_else(|| "template pin net could not be remapped".to_string())?;
                pin.net = Some(mapped);
            }
            pins.push(pin);
        }
        let position = Point::new(old.position.x + inst.entry.x, old.position.y + inst.entry.y);
        if !position.valid() {
            return Err("materialized component position is out of range".into());
        }
        project.components.push(Component {
            id: new_id,
            reference,
            value: old.value.clone(),
            kind: old.kind.clone(),
            position,
            rotation: old.rotation,
            pins,
            units: old.units.clone(),
            library_symbol: old.library_symbol.clone(),
            library_footprint: old.library_footprint.clone(),
            fields: old
                .fields
                .iter()
                .map(|f| ComponentField {
                    key: f.key.clone(),
                    value: f.value.clone(),
                })
                .collect(),
            sheet: inst.parent,
            instance: Some(inst.id),
            prototype: Some(old.id),
        });
        new_components += 1;
    }
    for wire in project
        .wires
        .iter()
        .filter(|w| {
            project
                .component(w.from.component)
                .is_some_and(|c| c.sheet == template)
                && project
                    .component(w.to.component)
                    .is_some_and(|c| c.sheet == template)
        })
        .cloned()
        .collect::<Vec<_>>()
    {
        let from = component_map
            .get(&wire.from.component)
            .ok_or_else(|| "wire endpoint missing after expansion".to_string())?;
        let to = component_map
            .get(&wire.to.component)
            .ok_or_else(|| "wire endpoint missing after expansion".to_string())?;
        let net = *net_map
            .get(&wire.net)
            .ok_or_else(|| "wire net missing after expansion".to_string())?;
        let id = allocate_id(project, next)?;
        project.wires.push(Wire {
            id,
            net,
            from: PinRef {
                component: *from,
                pin: wire.from.pin.clone(),
            },
            to: PinRef {
                component: *to,
                pin: wire.to.pin.clone(),
            },
        });
    }
    for wire in project
        .drawn_wires
        .iter()
        .filter(|w| project.schematic_drawn_wire_page(w) == template)
        .cloned()
        .collect::<Vec<_>>()
    {
        let net = match wire.net {
            Some(n) => Some(
                *net_map
                    .get(&n)
                    .ok_or_else(|| "drawn wire net missing after expansion".to_string())?,
            ),
            None => None,
        };
        let id = allocate_id(project, next)?;
        project.drawn_wires.push(DrawnWire {
            id,
            net,
            points: wire
                .points
                .iter()
                .map(|p| Point::new(p.x + inst.entry.x, p.y + inst.entry.y))
                .collect(),
        });
    }
    for label in project
        .labels
        .iter()
        .filter(|l| project.schematic_label_page(l) == template)
        .cloned()
        .collect::<Vec<_>>()
    {
        let net = *net_map
            .get(&label.net)
            .ok_or_else(|| "label net missing after expansion".to_string())?;
        let position = Point::new(
            label.position.x + inst.entry.x,
            label.position.y + inst.entry.y,
        );
        if !position.valid() {
            return Err("materialized label position is out of range".into());
        }
        let id = allocate_id(project, next)?;
        project.labels.push(Label {
            id,
            kind: label.kind.clone(),
            net,
            position,
            rotation: label.rotation,
            text: label.text.clone(),
        });
    }
    let new_nets = net_map
        .values()
        .filter(|id| {
            project
                .nets
                .iter()
                .any(|n| n.id == **id && n.instance == Some(inst.id))
        })
        .count();
    if project.ids().count() > MAX_ITEMS {
        return Err("project object limit reached (2048)".into());
    }
    Ok((new_components, new_nets))
}

pub fn collapse_instance(project: &mut Project, instance: Id) -> Result<(), String> {
    let inst_index = project
        .sheet_instances
        .iter()
        .position(|i| i.id == instance)
        .ok_or_else(|| "sheet instance does not exist".to_string())?;
    let inst = project.sheet_instances[inst_index].clone();
    let materialized: BTreeSet<Id> = project
        .components
        .iter()
        .filter(|c| c.instance == Some(instance))
        .map(|c| c.id)
        .collect();
    project.components.retain(|c| c.instance != Some(instance));
    let local_nets: BTreeSet<Id> = project
        .nets
        .iter()
        .filter(|n| n.instance == Some(instance))
        .map(|n| n.id)
        .collect();
    project.nets.retain(|n| n.instance != Some(instance));
    project.wires.retain(|w| {
        !materialized.contains(&w.from.component) && !materialized.contains(&w.to.component)
    });
    project
        .drawn_wires
        .retain(|w| w.net.is_none_or(|n| !local_nets.contains(&n)));
    project.labels.retain(|l| !local_nets.contains(&l.net));
    project
        .no_connects
        .retain(|f| !materialized.contains(&f.pin.component));
    project.sheet_instances.remove(inst_index);
    let _ = inst;
    Ok(())
}

pub fn validate_sheet_instances(project: &Project) -> Result<(), String> {
    if project.sheet_instances.len() > MAX_SHEET_INSTANCES {
        return Err("a schematic supports at most 64 sheet instances".into());
    }
    let mut seen: BTreeSet<(Id, &str)> = BTreeSet::new();
    for inst in &project.sheet_instances {
        if inst.name.is_empty() || inst.name.len() > MAX_INSTANCE_NAME {
            return Err("instance name must be 1 to 64 bytes".into());
        }
        if inst.reference_offset > MAX_REFERENCE_OFFSET {
            return Err("reference offset must be 0 to 999000".into());
        }
        if !inst.entry.valid() {
            return Err("instance entry position is out of range".into());
        }
        if !seen.insert((inst.parent, inst.name.as_str())) {
            return Err("duplicate instance name on one parent page".into());
        }
        if inst.parent == inst.sheet {
            return Err("instance parent cannot be the template sheet".into());
        }
        if inst.parent != 0 && !project.sheets.iter().any(|s| s.id == inst.parent) {
            return Err("instance parent page does not exist".into());
        }
        if !project.sheets.iter().any(|s| s.id == inst.sheet) {
            return Err("instance template sheet does not exist".into());
        }
        if !project.components.iter().any(|c| c.sheet == inst.sheet) {
            return Err("instance template sheet has no components".into());
        }
    }
    let port_nets: BTreeSet<Id> = project.ports.iter().map(|p| p.net).collect();
    for net in &project.nets {
        if net.instance.is_some() && port_nets.contains(&net.id) {
            return Err("port nets cannot be tagged with an instance".into());
        }
    }
    for c in &project.components {
        if let Some(instance) = c.instance {
            let Some(inst) = project.sheet_instances.iter().find(|i| i.id == instance) else {
                return Err("component instance does not exist".into());
            };
            let Some(prototype) = c.prototype else {
                return Err("materialized component missing prototype".into());
            };
            if !project
                .components
                .iter()
                .any(|t| t.id == prototype && t.sheet == inst.sheet)
            {
                return Err("component prototype is not on the template sheet".into());
            }
            if c.sheet != inst.parent {
                return Err("materialized component sits on the wrong parent page".into());
            }
        } else if c.prototype.is_some() {
            return Err("prototype without instance".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod sheet_instance_tests {
    use super::*;
    use crate::{Pin, Project, Sheet};

    fn two_resistor_template() -> Result<(Project, Id, Id, Id), String> {
        let mut p = Project::default();
        let mut next = 10u64;
        let sheet = 3;
        p.sheets.push(Sheet {
            id: sheet,
            name: "channel".into(),
            parent: Some(0),
            entry: None,
        });
        let r1 = allocate_id(&p, &mut next)?;
        let r2 = allocate_id(&p, &mut next)?;
        let net = allocate_id(&p, &mut next)?;
        p.nets.push(Net {
            id: net,
            name: "LOCAL".into(),
            target_length: None,
            length_tolerance: None,
            instance: None,
        });
        p.components.push(Component {
            id: r1,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            pins: vec![Pin {
                number: "1".into(),
                name: "1".into(),
                offset: Point::new(-2.0, 0.0),
                net: Some(net),
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet,
            instance: None,
            prototype: None,
        });
        p.components.push(Component {
            id: r2,
            reference: "R2".into(),
            value: "2k".into(),
            kind: "resistor".into(),
            position: Point::new(20.0, 10.0),
            rotation: 0.0,
            pins: vec![Pin {
                number: "1".into(),
                name: "1".into(),
                offset: Point::new(-2.0, 0.0),
                net: Some(net),
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet,
            instance: None,
            prototype: None,
        });
        Ok((p, sheet, r1, r2))
    }

    #[test]
    fn sheet_instances_validate_refuses_unknown_parent() -> Result<(), String> {
        let (mut p, sheet, _, _) = two_resistor_template()?;
        p.sheet_instances.push(SheetInstance {
            id: 50,
            sheet,
            name: "A".into(),
            reference_offset: 0,
            parent: 99,
            entry: Point::new(0.0, 0.0),
        });
        assert!(validate_sheet_instances(&p).is_err());
        Ok(())
    }
}
