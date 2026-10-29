//! Schematic companion-part connectivity checks over net graphs and two-terminal bridges.
//!
//! Profile `[[companion]]` apply in `crates/engine/profiles.rs` (#421) is not wired on this
//! branch; built-in rows mirror profile intent until merge.

use crate::Finding;
use pcbcraft_doc::{parse_companion_value, CompanionRule, Component, Id, Project};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const MAX_BFS_DEPTH: usize = 32;

const BRIDGE_KINDS: &[&str] = &["resistor", "capacitor", "diode", "fuse", "inductor"];

struct Bridge {
    component: Id,
    net_a: Id,
    net_b: Id,
}

struct SchematicGraph {
    bridges: Vec<Bridge>,
    /// Adjacency: net -> (other_net, bridge component).
    adjacency: BTreeMap<Id, Vec<(Id, Id)>>,
}

fn kind_matches(kind: &str, expected: &str) -> bool {
    let k = kind.to_ascii_lowercase();
    let e = expected.to_ascii_lowercase();
    if k == e {
        return true;
    }
    matches!(
        (k.as_str(), e.as_str()),
        ("mcu", "ic") | ("ic", "mcu") | ("tvs", "esd") | ("esd", "tvs")
    )
}

fn net_pattern_matches(name: &str, pattern: &str) -> bool {
    let name = name.as_bytes();
    let pattern = pattern.as_bytes();
    fn rec(name: &[u8], pat: &[u8]) -> bool {
        match pat.first() {
            None => name.is_empty(),
            Some(b'?') => name
                .first()
                .is_some_and(|_| rec(&name[1..], &pat[1..])),
            Some(b'*') => rec(name, &pat[1..])
                || name
                    .first()
                    .is_some_and(|_| rec(&name[1..], pat)),
            Some(ch) => name
                .first()
                .is_some_and(|n| n.eq_ignore_ascii_case(ch) && rec(&name[1..], &pat[1..])),
        }
    }
    rec(name, pattern)
}

fn parse_passive_value(value: &str, kind: &str) -> Option<f64> {
    let _ = kind;
    parse_companion_value(value)
}

fn value_in_range(value: &str, kind: &str, min: f64, max: f64) -> bool {
    parse_passive_value(value, kind).is_some_and(|v| v >= min && v <= max)
}

fn is_bridge_component(c: &Component) -> Option<(Id, Id)> {
    if c.pins.len() != 2 {
        return None;
    }
    let n0 = c.pins[0].net?;
    let n1 = c.pins[1].net?;
    if n0 == n1 {
        return None;
    }
    let passive_pins = c.pins.iter().all(|p| p.electrical_type == "passive");
    let bridge_kind = BRIDGE_KINDS.iter().any(|k| kind_matches(&c.kind, k));
    if bridge_kind || passive_pins {
        Some((n0, n1))
    } else {
        None
    }
}

fn schematic_net_graph(project: &Project) -> SchematicGraph {
    let mut bridges = Vec::new();
    let mut adjacency: BTreeMap<Id, Vec<(Id, Id)>> = BTreeMap::new();
    for c in &project.components {
        if let Some((a, b)) = is_bridge_component(c) {
            bridges.push(Bridge {
                component: c.id,
                net_a: a,
                net_b: b,
            });
            adjacency.entry(a).or_default().push((b, c.id));
            adjacency.entry(b).or_default().push((a, c.id));
        }
    }
    SchematicGraph { bridges, adjacency }
}

fn bfs_from_anchor(
    graph: &SchematicGraph,
    start: Id,
) -> BTreeMap<Id, (usize, Option<Id>)> {
    let mut seen = BTreeMap::new();
    let mut queue = VecDeque::new();
    seen.insert(start, (0usize, None));
    queue.push_back(start);
    while let Some(net) = queue.pop_front() {
        let (depth, _) = seen[&net];
        if depth >= MAX_BFS_DEPTH {
            continue;
        }
        if let Some(edges) = graph.adjacency.get(&net) {
            for (other, via) in edges {
                if !seen.contains_key(other) {
                    seen.insert(*other, (depth + 1, Some(*via)));
                    queue.push_back(*other);
                }
            }
        }
    }
    seen
}

fn bridge_between(graph: &SchematicGraph, a: Id, b: Id) -> Option<Id> {
    graph.bridges.iter().find_map(|br| {
        if (br.net_a == a && br.net_b == b) || (br.net_a == b && br.net_b == a) {
            Some(br.component)
        } else {
            None
        }
    })
}

fn is_ground_net(project: &Project, net: Id) -> bool {
    let upper = project.net_name(net).to_ascii_uppercase();
    if matches!(upper.as_str(), "GND" | "VSS" | "AGND" | "DGND") {
        return true;
    }
    project.labels.iter().any(|l| {
        l.net == net
            && l.kind == "power"
            && l.text.to_ascii_uppercase().contains("GND")
    })
}

fn is_power_net(project: &Project, net: Id) -> bool {
    if is_ground_net(project, net) {
        return false;
    }
    let upper = project.net_name(net).to_ascii_uppercase();
    if upper.starts_with("VCC")
        || upper.starts_with('+')
        || upper.contains("3V3")
        || upper.contains("5V")
        || upper.contains("VDD")
    {
        return true;
    }
    if project.labels.iter().any(|l| l.net == net && l.kind == "power") {
        return !is_ground_net(project, net);
    }
    project.components.iter().any(|c| {
        c.pins.iter().any(|p| {
            p.net == Some(net)
                && (p.electrical_type == "power_out" || p.electrical_type == "power_in")
                && !is_ground_net(project, net)
        })
    })
}

fn net_matches_usb_data(name: &str) -> bool {
    let u = name.to_ascii_uppercase();
    u.contains("USB") && (u.contains("DP") || u.contains("DM") || u.contains("D+") || u.contains("D-"))
        || net_pattern_matches(name, "USB_*")
}

fn finding(
    project: &Project,
    code: &'static str,
    message: String,
    objects: Vec<Id>,
) -> Finding {
    Finding {
        code,
        severity: project.rule_severity(code, "warning"),
        message,
        objects,
    }
}

fn path_has_resistor_in_range(
    project: &Project,
    graph: &SchematicGraph,
    from: Id,
    to: Id,
    min: f64,
    max: f64,
) -> bool {
    let reached = bfs_from_anchor(graph, from);
    if !reached.contains_key(&to) {
        return false;
    }
    graph.bridges.iter().any(|br| {
        if !reached.contains_key(&br.net_a) || !reached.contains_key(&br.net_b) {
            return false;
        }
        project.component(br.component).is_some_and(|c| {
            kind_matches(&c.kind, "resistor") && value_in_range(&c.value, &c.kind, min, max)
        })
    })
}

fn builtin_usb_series(project: &Project, graph: &SchematicGraph) -> Vec<Finding> {
    let mut out = Vec::new();
    for net in &project.nets {
        if !net_matches_usb_data(&net.name) {
            continue;
        }
        let on_net: Vec<_> = project
            .components
            .iter()
            .filter(|c| c.pins.iter().any(|p| p.net == Some(net.id)))
            .collect();
        let Some(connector) = on_net.iter().find(|c| kind_matches(&c.kind, "connector"))
        else {
            continue;
        };
        let mcu_nets: BTreeSet<Id> = project
            .components
            .iter()
            .filter(|c| kind_matches(&c.kind, "mcu") || kind_matches(&c.kind, "ic"))
            .flat_map(|c| c.pins.iter().filter_map(|p| p.net))
            .collect();
        let reached = bfs_from_anchor(graph, net.id);
        let targets: Vec<Id> = mcu_nets
            .into_iter()
            .filter(|n| reached.contains_key(n))
            .collect();
        if targets.is_empty() {
            continue;
        }
        let ok = targets.iter().any(|target| {
            path_has_resistor_in_range(project, graph, net.id, *target, 22.0, 33.0)
        });
        if !ok {
            out.push(finding(
                project,
                "missing_series_resistor",
                format!(
                    "Add a 22 to 33 ohm resistor between the USB connector and the transceiver on net {}",
                    net.name
                ),
                vec![connector.id, net.id],
            ));
        }
    }
    out
}

fn builtin_cc_pulldown(project: &Project, graph: &SchematicGraph) -> Vec<Finding> {
    let mut out = Vec::new();
    for net in &project.nets {
        let u = net.name.to_ascii_uppercase();
        if u != "CC1" && u != "CC2" {
            continue;
        }
        let mut ok = false;
        for br in &graph.bridges {
            if br.net_a != net.id && br.net_b != net.id {
                continue;
            }
            let other = if br.net_a == net.id {
                br.net_b
            } else {
                br.net_a
            };
            if !is_ground_net(project, other) {
                continue;
            }
            if let Some(c) = project.component(br.component) {
                if kind_matches(&c.kind, "resistor")
                    && value_in_range(&c.value, &c.kind, 5_000.0, 5_600.0)
                {
                    ok = true;
                    break;
                }
            }
        }
        if !ok {
            out.push(finding(
                project,
                "missing_cc_pulldown",
                format!(
                    "Add a 5.0 to 5.6 kohm resistor from {} to ground",
                    net.name
                ),
                vec![net.id],
            ));
        }
    }
    out
}

fn open_drain_on_net(project: &Project, net: Id) -> bool {
    project.components.iter().any(|c| {
        c.pins.iter().any(|p| {
            p.net == Some(net)
                && (p.electrical_type == "open_collector"
                    || p.electrical_type == "open_emitter")
        })
    })
}

fn count_pullups_to_power(project: &Project, graph: &SchematicGraph, net: Id) -> u32 {
    let mut count = 0u32;
    for br in &graph.bridges {
        if br.net_a != net && br.net_b != net {
            continue;
        }
        let other = if br.net_a == net { br.net_b } else { br.net_a };
        if !is_power_net(project, other) {
            continue;
        }
        if let Some(c) = project.component(br.component) {
            if kind_matches(&c.kind, "resistor") {
                count += 1;
            }
        }
    }
    count
}

fn builtin_i2c_pullup(project: &Project, graph: &SchematicGraph) -> Vec<Finding> {
    let mut out = Vec::new();
    for net in &project.nets {
        let u = net.name.to_ascii_uppercase();
        if u != "SDA" && u != "SCL" {
            continue;
        }
        if !open_drain_on_net(project, net.id) {
            continue;
        }
        let pullups = count_pullups_to_power(project, graph, net.id);
        if pullups != 1 {
            out.push(finding(
                project,
                "missing_pullup",
                format!(
                    "Add exactly one pull-up resistor from {} to a power net",
                    net.name
                ),
                vec![net.id],
            ));
        }
    }
    out
}

fn bus_end_nets(project: &Project, graph: &SchematicGraph, pattern: &str) -> Vec<Id> {
    let bus_nets: BTreeSet<Id> = project
        .nets
        .iter()
        .filter(|n| net_pattern_matches(&n.name, pattern))
        .map(|n| n.id)
        .collect();
    let mut ends = Vec::new();
    for net in &bus_nets {
        let degree = graph
            .adjacency
            .get(net)
            .map(|e| e.iter().filter(|(o, _)| bus_nets.contains(o)).count())
            .unwrap_or(0);
        if degree <= 1 {
            ends.push(*net);
        }
    }
    ends
}

fn termination_on_can_pair(
    project: &Project,
    graph: &SchematicGraph,
    net_h: Id,
    net_l: Id,
) -> bool {
    graph.bridges.iter().any(|br| {
        let pair = (br.net_a, br.net_b);
        if pair != (net_h, net_l) && pair != (net_l, net_h) {
            return false;
        }
        project
            .component(br.component)
            .is_some_and(|c| {
                kind_matches(&c.kind, "resistor")
                    && value_in_range(&c.value, &c.kind, 110.0, 130.0)
            })
    })
}

fn builtin_can_termination(project: &Project, graph: &SchematicGraph) -> Vec<Finding> {
    let mut out = Vec::new();
    let h_nets: Vec<_> = project
        .nets
        .iter()
        .filter(|n| net_pattern_matches(&n.name, "CAN_H") || n.name.to_ascii_uppercase() == "CANH")
        .collect();
    for h in h_nets {
        let l_name = h.name.replace("_H", "_L").replace("CANH", "CANL");
        let Some(l) = project.nets.iter().find(|n| n.name == l_name) else {
            continue;
        };
        let ends_h = bus_end_nets(project, graph, "CAN*");
        if ends_h.is_empty() {
            continue;
        }
        if !termination_on_can_pair(project, graph, h.id, l.id) {
            out.push(finding(
                project,
                "missing_bus_termination",
                format!(
                    "Add a 110 to 130 ohm termination resistor between {} and {}",
                    h.name, l.name
                ),
                vec![h.id, l.id],
            ));
        }
    }
    out
}

fn builtin_relay_flyback(project: &Project, graph: &SchematicGraph) -> Vec<Finding> {
    let mut out = Vec::new();
    for c in &project.components {
        if !kind_matches(&c.kind, "relay") {
            continue;
        }
        let nets: Vec<Id> = c.pins.iter().filter_map(|p| p.net).collect();
        if nets.len() != 2 || nets[0] == nets[1] {
            continue;
        }
        let a = nets[0];
        let b = nets[1];
        let has_diode = graph.bridges.iter().any(|br| {
            let pair = (br.net_a, br.net_b);
            if pair != (a, b) {
                return false;
            }
            project
                .component(br.component)
                .is_some_and(|d| kind_matches(&d.kind, "diode"))
        });
        if !has_diode {
            out.push(finding(
                project,
                "missing_flyback_diode",
                format!(
                    "Add a flyback diode anti-parallel across the {} coil",
                    c.reference
                ),
                vec![c.id],
            ));
        }
    }
    out
}

fn protection_on_path_to_sink(
    project: &Project,
    graph: &SchematicGraph,
    start: Id,
    sink_kinds: &[&str],
) -> bool {
    let reached = bfs_from_anchor(graph, start);
    let sink = project.components.iter().any(|c| {
        sink_kinds.iter().any(|k| kind_matches(&c.kind, k))
            && c.pins
                .iter()
                .any(|p| p.net.is_some_and(|n| reached.contains_key(&n) && n != start))
    });
    if !sink {
        return true;
    }
    graph.bridges.iter().any(|br| {
        if !reached.contains_key(&br.net_a) || !reached.contains_key(&br.net_b) {
            return false;
        }
        project.component(br.component).is_some_and(|c| {
            kind_matches(&c.kind, "tvs")
                || kind_matches(&c.kind, "esd")
                || kind_matches(&c.kind, "diode")
        })
    })
}

fn builtin_connector_tvs(project: &Project, graph: &SchematicGraph) -> Vec<Finding> {
    let mut out = Vec::new();
    for c in &project.components {
        if !kind_matches(&c.kind, "connector") {
            continue;
        }
        for pin in &c.pins {
            let Some(net) = pin.net else {
                continue;
            };
            if is_power_net(project, net) || is_ground_net(project, net) {
                continue;
            }
            let ok = protection_on_path_to_sink(project, graph, net, &["ic", "mcu"]);
            if !ok {
                out.push(finding(
                    project,
                    "missing_tvs",
                    format!(
                        "Add a TVS or protection diode on the path from {} toward the load on net {}",
                        c.reference,
                        project.net_name(net)
                    ),
                    vec![c.id, net],
                ));
            }
        }
    }
    out
}

fn fuse_before_regulator(
    project: &Project,
    graph: &SchematicGraph,
    start: Id,
    reg_net: Id,
) -> bool {
    if graph.bridges.iter().any(|br| {
        let pair = (br.net_a, br.net_b);
        if pair != (start, reg_net) && pair != (reg_net, start) {
            return false;
        }
        project
            .component(br.component)
            .is_some_and(|c| kind_matches(&c.kind, "fuse"))
    }) {
        return true;
    }
    let reached = bfs_from_anchor(graph, start);
    if !reached.contains_key(&reg_net) {
        return false;
    }
    graph.bridges.iter().any(|br| {
        if !reached.contains_key(&br.net_a) || !reached.contains_key(&br.net_b) {
            return false;
        }
        project
            .component(br.component)
            .is_some_and(|c| kind_matches(&c.kind, "fuse"))
    })
}

fn builtin_power_fuse(project: &Project, graph: &SchematicGraph) -> Vec<Finding> {
    let mut out = Vec::new();
    let reg_nets: Vec<(Id, Id)> = project
        .components
        .iter()
        .filter(|c| kind_matches(&c.kind, "ldo") || kind_matches(&c.kind, "buck"))
        .filter_map(|c| {
            c.pins
                .iter()
                .find(|p| p.electrical_type == "power_in")
                .and_then(|p| p.net.map(|n| (c.id, n)))
        })
        .collect();
    for c in &project.components {
        let power_entry = kind_matches(&c.kind, "connector_power")
            || (kind_matches(&c.kind, "connector")
                && c.pins.iter().any(|p| {
                    p.electrical_type == "power_in" || p.electrical_type == "power_out"
                }));
        if !power_entry {
            continue;
        }
        for pin in &c.pins {
            let Some(net) = pin.net else {
                continue;
            };
            if !is_power_net(project, net) {
                continue;
            }
            for (reg_id, reg_net) in &reg_nets {
                if *reg_net == net {
                    continue;
                }
                if !fuse_before_regulator(project, graph, net, *reg_net) {
                    out.push(finding(
                        project,
                        "missing_fuse",
                        format!(
                            "Add a fuse on the path from {} before the regulator on net {}",
                            c.reference,
                            project.net_name(*reg_net)
                        ),
                        vec![c.id, *reg_id, net],
                    ));
                }
            }
        }
    }
    out
}

fn builtin_decoupling(project: &Project, graph: &SchematicGraph) -> Vec<Finding> {
    let mut out = Vec::new();
    for c in &project.components {
        if !kind_matches(&c.kind, "mcu") && !kind_matches(&c.kind, "ic") {
            continue;
        }
        for pin in &c.pins {
            if pin.electrical_type != "power_in" {
                continue;
            }
            let Some(net) = pin.net else {
                continue;
            };
            let has_cap = graph.bridges.iter().any(|br| {
                if br.net_a != net && br.net_b != net {
                    return false;
                }
                let other = if br.net_a == net { br.net_b } else { br.net_a };
                if !is_ground_net(project, other) {
                    return false;
                }
                project
                    .component(br.component)
                    .is_some_and(|cap| kind_matches(&cap.kind, "capacitor"))
            });
            if !has_cap {
                out.push(finding(
                    project,
                    "missing_decoupling",
                    format!(
                        "Add a decoupling capacitor from {} power pin to ground on net {}",
                        c.reference,
                        project.net_name(net)
                    ),
                    vec![c.id, net],
                ));
            }
        }
    }
    out
}

fn builtin_power_driven(project: &Project) -> Vec<Finding> {
    let mut out = Vec::new();
    for net in &project.nets {
        if !is_power_net(project, net.id) {
            continue;
        }
        let driven = project.components.iter().any(|c| {
            (kind_matches(&c.kind, "connector")
                || kind_matches(&c.kind, "connector_power")
                || kind_matches(&c.kind, "ldo")
                || kind_matches(&c.kind, "buck"))
                && c.pins.iter().any(|p| {
                    p.net == Some(net.id)
                        && (p.electrical_type == "power_out"
                            || p.electrical_type == "power_in")
                })
        });
        if !driven {
            out.push(finding(
                project,
                "power_not_driven",
                format!("Power net {} has no obvious source", net.name),
                vec![net.id],
            ));
        }
    }
    out
}

pub fn companion_findings_for_rules(
    project: &Project,
    rules: &[CompanionRule],
    graph: &SchematicGraph,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for rule in rules {
        let code = rule
            .finding_code
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("missing_companion");
        let code_static: &'static str = match code {
            "missing_series_resistor" => "missing_series_resistor",
            "missing_cc_pulldown" => "missing_cc_pulldown",
            "missing_pullup" => "missing_pullup",
            "missing_bus_termination" => "missing_bus_termination",
            "missing_flyback_diode" => "missing_flyback_diode",
            "missing_tvs" => "missing_tvs",
            "missing_fuse" => "missing_fuse",
            "missing_decoupling" => "missing_decoupling",
            "power_not_driven" => "power_not_driven",
            _ => "missing_companion",
        };
        for net in &project.nets {
            if !rule
                .net_patterns
                .iter()
                .any(|p| net_pattern_matches(&net.name, p))
            {
                continue;
            }
            let min = rule
                .value_min
                .as_ref()
                .and_then(|s| parse_passive_value(s, &rule.required_kind));
            let max = rule
                .value_max
                .as_ref()
                .and_then(|s| parse_passive_value(s, &rule.required_kind));
            let mut satisfied = false;
            for br in &graph.bridges {
                if br.net_a != net.id && br.net_b != net.id {
                    continue;
                }
                if let Some(c) = project.component(br.component) {
                    if !kind_matches(&c.kind, &rule.required_kind) {
                        continue;
                    }
                    if let (Some(lo), Some(hi)) = (min, max) {
                        if !value_in_range(&c.value, &c.kind, lo, hi) {
                            continue;
                        }
                    }
                    satisfied = true;
                    break;
                }
            }
            if !satisfied {
                out.push(finding(
                    project,
                    code_static,
                    format!("Companion rule {} not satisfied on net {}", rule.name, net.name),
                    vec![net.id],
                ));
            }
        }
    }
    out
}

pub fn companion_findings(project: &Project) -> Vec<Finding> {
    let graph = schematic_net_graph(project);
    let mut out = Vec::new();
    out.extend(builtin_usb_series(project, &graph));
    out.extend(builtin_cc_pulldown(project, &graph));
    out.extend(builtin_i2c_pullup(project, &graph));
    out.extend(builtin_can_termination(project, &graph));
    out.extend(builtin_relay_flyback(project, &graph));
    out.extend(builtin_connector_tvs(project, &graph));
    out.extend(builtin_power_fuse(project, &graph));
    out.extend(builtin_decoupling(project, &graph));
    out.extend(builtin_power_driven(project));
    out.extend(companion_findings_for_rules(
        project,
        &project.companion_rules,
        &graph,
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Net, Pin, Point};

    fn pin(num: &str, net: Option<Id>, electrical_type: &str) -> Pin {
        Pin {
            number: num.into(),
            name: num.into(),
            offset: Point::default(),
            net,
            electrical_type: electrical_type.into(),
            direction: None,
            unit: 1,
        }
    }

    fn part(
        id: Id,
        reference: &str,
        kind: &str,
        value: &str,
        pins: Vec<Pin>,
    ) -> Component {
        Component {
            id,
            reference: reference.into(),
            value: value.into(),
            kind: kind.into(),
            position: Point::default(),
            rotation: 0.0,
            pins,
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
        }
    }

    fn net(id: Id, name: &str) -> Net {
        Net {
            id,
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn companion_graph_crosses_two_pin_resistor_between_nets() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.extend([net(1, "A"), net(2, "B")]);
        p.components.push(part(
            10,
            "R1",
            "resistor",
            "27",
            vec![pin("1", Some(1), "passive"), pin("2", Some(2), "passive")],
        ));
        p.validate()?;
        let g = schematic_net_graph(&p);
        assert_eq!(g.bridges.len(), 1);
        assert!(bridge_between(&g, 1, 2).is_some());
        Ok(())
    }

    #[test]
    fn companion_usb_series_resistor_required_between_connector_and_mcu() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.extend([net(1, "USB_DP"), net(2, "USB_DP_MCU")]);
        p.components.push(part(
            10,
            "J1",
            "connector",
            "USB",
            vec![pin("1", Some(1), "passive")],
        ));
        p.components.push(part(
            11,
            "U1",
            "mcu",
            "MCU",
            vec![pin("1", Some(2), "passive")],
        ));
        p.validate()?;
        assert!(companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_series_resistor"));
        p.components.push(part(
            12,
            "R1",
            "resistor",
            "27",
            vec![pin("1", Some(1), "passive"), pin("2", Some(2), "passive")],
        ));
        p.validate()?;
        assert!(!companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_series_resistor"));
        Ok(())
    }

    #[test]
    fn companion_cc_net_requires_5k1_pulldown_to_ground() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.extend([net(1, "CC1"), net(2, "GND")]);
        p.labels.push(pcbcraft_doc::Label {
            id: 50,
            kind: "power".into(),
            net: 2,
            position: Point::default(),
            rotation: 0.0,
            text: "GND".into(),
        });
        p.components.push(part(
            10,
            "J1",
            "connector",
            "USB-C",
            vec![pin("1", Some(1), "passive")],
        ));
        p.validate()?;
        assert!(companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_cc_pulldown"));
        p.components.push(part(
            11,
            "R1",
            "resistor",
            "5.1k",
            vec![pin("1", Some(1), "passive"), pin("2", Some(2), "passive")],
        ));
        p.validate()?;
        assert!(!companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_cc_pulldown"));
        Ok(())
    }

    #[test]
    fn companion_i2c_requires_one_pullup_per_open_drain_net() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.extend([net(1, "SDA"), net(2, "VCC")]);
        p.components.push(part(
            10,
            "U1",
            "ic",
            "Sensor",
            vec![pin("1", Some(1), "open_collector")],
        ));
        p.validate()?;
        assert!(companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_pullup"));
        p.components.push(part(
            11,
            "R1",
            "resistor",
            "4.7k",
            vec![pin("1", Some(1), "passive"), pin("2", Some(2), "passive")],
        ));
        p.validate()?;
        assert!(!companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_pullup"));
        Ok(())
    }

    #[test]
    fn companion_can_bus_end_requires_120_ohm_termination() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.extend([net(1, "CAN_H"), net(2, "CAN_L")]);
        p.components.push(part(
            10,
            "U1",
            "ic",
            "CAN",
            vec![
                pin("1", Some(1), "passive"),
                pin("2", Some(2), "passive"),
            ],
        ));
        p.validate()?;
        assert!(companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_bus_termination"));
        p.components.push(part(
            11,
            "R1",
            "resistor",
            "120",
            vec![pin("1", Some(1), "passive"), pin("2", Some(2), "passive")],
        ));
        p.validate()?;
        assert!(!companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_bus_termination"));
        Ok(())
    }

    #[test]
    fn companion_relay_coil_requires_flyback_diode() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.extend([net(1, "COIL_A"), net(2, "COIL_B")]);
        p.components.push(part(
            10,
            "K1",
            "relay",
            "Relay",
            vec![
                pin("1", Some(1), "passive"),
                pin("2", Some(2), "passive"),
            ],
        ));
        p.validate()?;
        assert!(companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_flyback_diode"));
        p.components.push(part(
            11,
            "D1",
            "diode",
            "1N4148",
            vec![pin("1", Some(1), "passive"), pin("2", Some(2), "passive")],
        ));
        p.validate()?;
        assert!(!companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_flyback_diode"));
        Ok(())
    }

    #[test]
    fn companion_connector_path_includes_tvs_before_ic() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.extend([net(1, "SIG"), net(2, "SIG_IN")]);
        p.components.push(part(
            10,
            "J1",
            "connector",
            "IO",
            vec![pin("1", Some(1), "passive")],
        ));
        p.components.push(part(
            11,
            "R1",
            "resistor",
            "0",
            vec![pin("1", Some(1), "passive"), pin("2", Some(2), "passive")],
        ));
        p.components.push(part(
            12,
            "U1",
            "ic",
            "Load",
            vec![pin("1", Some(2), "passive")],
        ));
        p.validate()?;
        assert!(companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_tvs"));
        p.components.insert(
            1,
            part(
                13,
                "D1",
                "tvs",
                "ESD",
                vec![pin("1", Some(1), "passive"), pin("2", Some(2), "passive")],
            ),
        );
        p.components.retain(|c| c.id != 11);
        p.validate()?;
        assert!(!companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_tvs"));
        Ok(())
    }

    #[test]
    fn companion_power_entry_requires_fuse_before_regulator() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.extend([net(1, "+5V"), net(2, "VREG")]);
        p.components.push(part(
            10,
            "J1",
            "connector_power",
            "Barrel",
            vec![pin("1", Some(1), "power_out")],
        ));
        p.components.push(part(
            11,
            "U1",
            "ldo",
            "3V3",
            vec![pin("1", Some(2), "power_in")],
        ));
        p.validate()?;
        assert!(companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_fuse"));
        p.components.push(part(
            13,
            "F1",
            "fuse",
            "500mA",
            vec![pin("1", Some(1), "passive"), pin("2", Some(2), "passive")],
        ));
        p.validate()?;
        assert!(!companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_fuse"));
        Ok(())
    }

    #[test]
    fn companion_refactors_missing_decoupling_onto_walker() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.extend([net(1, "VDD"), net(2, "GND")]);
        p.labels.push(pcbcraft_doc::Label {
            id: 50,
            kind: "power".into(),
            net: 2,
            position: Point::default(),
            rotation: 0.0,
            text: "GND".into(),
        });
        p.components.push(part(
            10,
            "U1",
            "mcu",
            "MCU",
            vec![pin("1", Some(1), "power_in")],
        ));
        p.validate()?;
        assert!(companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_decoupling"));
        p.components.push(part(
            11,
            "C1",
            "capacitor",
            "100nF",
            vec![pin("1", Some(1), "passive"), pin("2", Some(2), "passive")],
        ));
        p.validate()?;
        assert!(!companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_decoupling"));
        Ok(())
    }

    #[test]
    fn companion_refactors_missing_pullup_onto_walker() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.extend([net(1, "SCL"), net(2, "VCC")]);
        p.components.push(part(
            10,
            "U1",
            "ic",
            "IO",
            vec![pin("1", Some(1), "open_emitter")],
        ));
        p.validate()?;
        let codes: BTreeSet<_> = companion_findings(&p)
            .iter()
            .map(|f| f.code)
            .collect();
        assert!(codes.contains("missing_pullup"));
        Ok(())
    }

    #[test]
    fn companion_rules_from_project_validate_and_run() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.push(net(1, "CUSTOM_BUS"));
        p.companion_rules.push(CompanionRule {
            id: "t1".into(),
            name: "need R".into(),
            net_patterns: vec!["CUSTOM_*".into()],
            required_kind: "resistor".into(),
            value_min: Some("100".into()),
            value_max: Some("150".into()),
            before_kind: None,
            anchor_kind: None,
            sink_kind: None,
            count: None,
            exactly: None,
            role: None,
            finding_code: Some("missing_series_resistor".into()),
        });
        p.validate()?;
        assert!(companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_series_resistor"));
        p.nets.push(net(2, "CUSTOM_GND"));
        p.components.push(part(
            10,
            "R1",
            "resistor",
            "120",
            vec![pin("1", Some(1), "passive"), pin("2", Some(2), "passive")],
        ));
        p.validate()?;
        assert!(!companion_findings(&p)
            .iter()
            .any(|f| f.code == "missing_series_resistor"));
        Ok(())
    }
}
