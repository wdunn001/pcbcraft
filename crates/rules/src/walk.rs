//! Bounded netlist walks for companion-part checks (decoupling, pull-ups, power).
use pcbcraft_doc::{Id, PinRef, Project};
use std::collections::{BTreeSet, VecDeque};

/// Passive schematic kinds a hop may cross. ICs and other parts stop the walk.
pub const PASSIVE_KINDS: &[&str] = &["resistor", "capacitor", "inductor"];

const HOP_CAP: u8 = 8;

/// Search from a pin for a component kind (and optional value) within passive hops.
///
/// `max_hops` above [`HOP_CAP`] is clamped to [`HOP_CAP`] before searching.
pub struct Query<'a> {
    pub start: PinRef,
    pub target_kind: &'a str,
    pub value: Option<&'a str>,
    pub max_hops: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub part: Id,
    /// Passive parts traversed, then the matching part id.
    pub path: Vec<Id>,
}

impl Query<'_> {
    fn hop_limit(&self) -> u8 {
        self.max_hops.min(HOP_CAP)
    }
}

fn pin_net(project: &Project, start: &PinRef) -> Option<Id> {
    project
        .component(start.component)
        .and_then(|c| project.pin(start))
        .and_then(|p| p.net)
}

fn component_on_net(project: &Project, component: Id, net: Id) -> bool {
    project
        .component(component)
        .is_some_and(|c| c.pins.iter().any(|p| p.net == Some(net)))
}

fn matches_target(
    project: &Project,
    component: Id,
    target_kind: &str,
    value: Option<&str>,
) -> bool {
    let Some(c) = project.component(component) else {
        return false;
    };
    if c.kind != target_kind {
        return false;
    }
    match value {
        Some(v) => c.value == v,
        None => true,
    }
}

fn hits_on_net(
    project: &Project,
    net: Id,
    target_kind: &str,
    value: Option<&str>,
    prefix: &[Id],
) -> Vec<Hit> {
    let mut out = Vec::new();
    for c in &project.components {
        if !component_on_net(project, c.id, net) {
            continue;
        }
        if matches_target(project, c.id, target_kind, value) {
            let mut path = prefix.to_vec();
            path.push(c.id);
            out.push(Hit { part: c.id, path });
        }
    }
    out
}

fn passive_bridges(project: &Project, component: Id, from_net: Id) -> Vec<(Id, Id)> {
    let Some(c) = project.component(component) else {
        return vec![];
    };
    if !PASSIVE_KINDS.contains(&c.kind.as_str()) {
        return vec![];
    }
    if !component_on_net(project, component, from_net) {
        return vec![];
    }
    c.pins
        .iter()
        .filter_map(|p| p.net.filter(|n| *n != from_net).map(|n| (n, component)))
        .collect()
}

/// Breadth-first search from `q.start` for matching parts, crossing only [`PASSIVE_KINDS`].
pub fn find(project: &Project, q: &Query<'_>) -> Vec<Hit> {
    let Some(start_net) = pin_net(project, &q.start) else {
        return vec![];
    };
    let limit = q.hop_limit();
    let mut hits = Vec::new();
    let mut seen_passives = BTreeSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((start_net, 0u8, Vec::<Id>::new()));

    while let Some((net, hops, prefix)) = queue.pop_front() {
        hits.extend(hits_on_net(project, net, q.target_kind, q.value, &prefix));
        if hops >= limit {
            continue;
        }
        for c in &project.components {
            if !PASSIVE_KINDS.contains(&c.kind.as_str()) {
                continue;
            }
            if !component_on_net(project, c.id, net) {
                continue;
            }
            if seen_passives.contains(&c.id) {
                continue;
            }
            seen_passives.insert(c.id);
            let mut next_prefix = prefix.clone();
            next_prefix.push(c.id);
            for (other_net, _) in passive_bridges(project, c.id, net) {
                queue.push_back((other_net, hops + 1, next_prefix.clone()));
            }
        }
    }

    hits.sort_by_key(|h| h.part);
    hits.dedup_by(|a, b| a.part == b.part && a.path == b.path);
    hits
}

pub(crate) fn any_pin_on_net(project: &Project, net: Id) -> Option<PinRef> {
    for c in &project.components {
        for p in &c.pins {
            if p.net == Some(net) {
                return Some(PinRef {
                    component: c.id,
                    pin: p.number.clone(),
                });
            }
        }
    }
    None
}

/// True when the net is driven by a power label, connector pin, or power output.
pub fn net_is_power_driven(project: &Project, net: Id) -> bool {
    if project
        .labels
        .iter()
        .any(|l| l.net == net && l.kind == "power")
    {
        return true;
    }
    let Some(start) = any_pin_on_net(project, net) else {
        return false;
    };
    if !find(
        project,
        &Query {
            start,
            target_kind: "connector",
            value: None,
            max_hops: 0,
        },
    )
    .is_empty()
    {
        return true;
    }
    project.components.iter().any(|c| {
        c.pins
            .iter()
            .any(|p| p.net == Some(net) && p.electrical_type == "power_out")
    })
}

/// True when a passive on `net` reaches a power-driven rail (pull-up pattern).
pub fn pullup_reaches_power(project: &Project, net: Id) -> bool {
    let Some(start) = any_pin_on_net(project, net) else {
        return false;
    };
    for hit in find(
        project,
        &Query {
            start: start.clone(),
            target_kind: "resistor",
            value: None,
            max_hops: 0,
        },
    ) {
        if resistor_other_pin_reaches_power(project, hit.part, net) {
            return true;
        }
    }
    project.components.iter().any(|c| {
        let acts_as_passive =
            c.kind == "resistor" || c.pins.iter().any(|p| p.electrical_type == "passive");
        if !acts_as_passive || c.kind == "resistor" {
            return false;
        }
        let touches_bus = c.pins.iter().any(|p| p.net == Some(net));
        if !touches_bus {
            return false;
        }
        c.pins.iter().any(|p| {
            p.net
                .filter(|n| *n != net)
                .is_some_and(|other| net_is_power_driven(project, other))
        })
    })
}

fn resistor_other_pin_reaches_power(project: &Project, resistor: Id, net: Id) -> bool {
    let Some(c) = project.component(resistor) else {
        return false;
    };
    c.pins.iter().any(|p| {
        p.net
            .filter(|n| *n != net)
            .is_some_and(|other| net_is_power_driven(project, other))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Component, Net, Pin, PinRef, Project, component_template};
    use pcbcraft_geom::Point;
    use pcbcraft_library::{SymbolDefinition, SymbolPin};

    fn library_ic(id: Id, reference: &str, pins: Vec<Pin>) -> Component {
        let sym_pins: Vec<SymbolPin> = pins
            .iter()
            .map(|p| SymbolPin {
                number: p.number.clone(),
                name: p.name.clone(),
                position: p.offset,
                angle: p.direction.unwrap_or(0.0),
                length: 2.5,
                electrical_type: p.electrical_type.clone(),
                unit: p.unit,
                body_style: 0,
            })
            .collect();
        Component {
            id,
            reference: reference.into(),
            value: "MCU".into(),
            kind: "library".into(),
            position: Point::new(20.0, 20.0),
            rotation: 0.0,
            pins,
            units: vec![],
            library_symbol: Some(pcbcraft_doc::LibrarySymbol {
                library: "test".into(),
                unit: 1,
                definition: SymbolDefinition {
                    name: "TestIC".into(),
                    reference: "U".into(),
                    value: "MCU".into(),
                    description: String::new(),
                    keywords: String::new(),
                    footprint: String::new(),
                    set: String::new(),
                    pins: sym_pins,
                    graphics: vec![],
                    extends: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                },
            }),
            library_footprint: None,
            fields: vec![],
            sheet: 0,
        }
    }

    fn wire_resistor(id: Id, a: Id, b: Id) -> Result<Component, String> {
        let mut r = component_template("resistor", id, format!("R{id}"), Point::default())
            .ok_or("resistor template missing")?;
        r.pins[0].net = Some(a);
        r.pins[1].net = Some(b);
        Ok(r)
    }

    #[test]
    fn walk_finds_a_direct_hit() -> Result<(), String> {
        let mut p = Project::default();
        let vdd = 1;
        p.nets.push(Net {
            id: vdd,
            name: "VDD".into(),
            ..Default::default()
        });
        p.components.push(library_ic(
            2,
            "U1",
            vec![Pin {
                number: "1".into(),
                name: "VDD".into(),
                offset: Point::default(),
                net: Some(vdd),
                electrical_type: "power_in".into(),
                direction: Some(0.0),
                unit: 1,
            }],
        ));
        let mut cap =
            component_template("capacitor", 3, "C1".into(), Point::default()).ok_or("cap")?;
        for pin in &mut cap.pins {
            pin.net = Some(vdd);
        }
        p.components.push(cap);
        p.validate()?;
        let hits = find(
            &p,
            &Query {
                start: PinRef {
                    component: 2,
                    pin: "1".into(),
                },
                target_kind: "capacitor",
                value: None,
                max_hops: 0,
            },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].part, 3);
        Ok(())
    }

    #[test]
    fn walk_finds_a_part_one_resistor_away() -> Result<(), String> {
        let mut p = Project::default();
        let a = 10;
        let b = 11;
        p.nets.push(Net {
            id: a,
            name: "A".into(),
            ..Default::default()
        });
        p.nets.push(Net {
            id: b,
            name: "B".into(),
            ..Default::default()
        });
        p.components.push(library_ic(
            2,
            "U1",
            vec![Pin {
                number: "1".into(),
                name: "IO".into(),
                offset: Point::default(),
                net: Some(a),
                electrical_type: "input".into(),
                direction: Some(0.0),
                unit: 1,
            }],
        ));
        p.components.push(wire_resistor(20, a, b)?);
        let mut cap =
            component_template("capacitor", 30, "C1".into(), Point::default()).ok_or("cap")?;
        for pin in &mut cap.pins {
            pin.net = Some(b);
        }
        p.components.push(cap);
        p.validate()?;
        let hits = find(
            &p,
            &Query {
                start: PinRef {
                    component: 2,
                    pin: "1".into(),
                },
                target_kind: "capacitor",
                value: None,
                max_hops: 1,
            },
        );
        assert!(hits.iter().any(|h| h.part == 30));
        Ok(())
    }

    #[test]
    fn walk_terminates_on_a_netlist_loop() -> Result<(), String> {
        let mut p = Project::default();
        let a = 10;
        let b = 11;
        p.nets.push(Net {
            id: a,
            name: "A".into(),
            ..Default::default()
        });
        p.nets.push(Net {
            id: b,
            name: "B".into(),
            ..Default::default()
        });
        p.components.push(library_ic(
            2,
            "U1",
            vec![Pin {
                number: "1".into(),
                name: "IO".into(),
                offset: Point::default(),
                net: Some(a),
                electrical_type: "input".into(),
                direction: Some(0.0),
                unit: 1,
            }],
        ));
        p.components.push(wire_resistor(20, a, b)?);
        p.components.push(wire_resistor(21, b, a)?);
        p.validate()?;
        let hits = find(
            &p,
            &Query {
                start: PinRef {
                    component: 2,
                    pin: "1".into(),
                },
                target_kind: "capacitor",
                value: None,
                max_hops: 8,
            },
        );
        assert!(hits.is_empty());
        Ok(())
    }

    #[test]
    fn walk_respects_the_hop_limit() -> Result<(), String> {
        let mut p = Project::default();
        let nets: Vec<Id> = (1..=5).collect();
        for (i, id) in nets.iter().enumerate() {
            p.nets.push(Net {
                id: *id,
                name: format!("N{i}"),
                ..Default::default()
            });
        }
        p.components.push(library_ic(
            100,
            "U1",
            vec![Pin {
                number: "1".into(),
                name: "IO".into(),
                offset: Point::default(),
                net: Some(nets[0]),
                electrical_type: "input".into(),
                direction: Some(0.0),
                unit: 1,
            }],
        ));
        for i in 0..4 {
            p.components
                .push(wire_resistor(200 + i as Id, nets[i], nets[i + 1])?);
        }
        let mut cap =
            component_template("capacitor", 300, "C1".into(), Point::default()).ok_or("cap")?;
        for pin in &mut cap.pins {
            pin.net = Some(nets[4]);
        }
        p.components.push(cap);
        p.validate()?;
        let short = find(
            &p,
            &Query {
                start: PinRef {
                    component: 100,
                    pin: "1".into(),
                },
                target_kind: "capacitor",
                value: None,
                max_hops: 2,
            },
        );
        assert!(short.is_empty());
        let long = find(
            &p,
            &Query {
                start: PinRef {
                    component: 100,
                    pin: "1".into(),
                },
                target_kind: "capacitor",
                value: None,
                max_hops: 4,
            },
        );
        assert!(long.iter().any(|h| h.part == 300));
        Ok(())
    }

    #[test]
    fn walk_does_not_cross_an_ic() -> Result<(), String> {
        let mut p = Project::default();
        let a = 10;
        let b = 11;
        p.nets.push(Net {
            id: a,
            name: "A".into(),
            ..Default::default()
        });
        p.nets.push(Net {
            id: b,
            name: "B".into(),
            ..Default::default()
        });
        p.components.push(library_ic(
            2,
            "U1",
            vec![Pin {
                number: "1".into(),
                name: "A".into(),
                offset: Point::default(),
                net: Some(a),
                electrical_type: "input".into(),
                direction: Some(0.0),
                unit: 1,
            }],
        ));
        p.components.push(library_ic(
            3,
            "U2",
            vec![
                Pin {
                    number: "1".into(),
                    name: "A".into(),
                    offset: Point::default(),
                    net: Some(a),
                    electrical_type: "input".into(),
                    direction: Some(0.0),
                    unit: 1,
                },
                Pin {
                    number: "2".into(),
                    name: "B".into(),
                    offset: Point::default(),
                    net: Some(b),
                    electrical_type: "input".into(),
                    direction: Some(0.0),
                    unit: 1,
                },
            ],
        ));
        let mut cap =
            component_template("capacitor", 30, "C1".into(), Point::default()).ok_or("cap")?;
        for pin in &mut cap.pins {
            pin.net = Some(b);
        }
        p.components.push(cap);
        p.validate()?;
        let hits = find(
            &p,
            &Query {
                start: PinRef {
                    component: 2,
                    pin: "1".into(),
                },
                target_kind: "capacitor",
                value: None,
                max_hops: 2,
            },
        );
        assert!(hits.is_empty());
        Ok(())
    }

    #[test]
    fn walk_returns_the_path() -> Result<(), String> {
        let mut p = Project::default();
        let a = 10;
        let b = 11;
        p.nets.push(Net {
            id: a,
            name: "A".into(),
            ..Default::default()
        });
        p.nets.push(Net {
            id: b,
            name: "B".into(),
            ..Default::default()
        });
        p.components.push(library_ic(
            2,
            "U1",
            vec![Pin {
                number: "1".into(),
                name: "IO".into(),
                offset: Point::default(),
                net: Some(a),
                electrical_type: "input".into(),
                direction: Some(0.0),
                unit: 1,
            }],
        ));
        p.components.push(wire_resistor(20, a, b)?);
        let mut cap =
            component_template("capacitor", 30, "C1".into(), Point::default()).ok_or("cap")?;
        for pin in &mut cap.pins {
            pin.net = Some(b);
        }
        p.components.push(cap);
        p.validate()?;
        let hits = find(
            &p,
            &Query {
                start: PinRef {
                    component: 2,
                    pin: "1".into(),
                },
                target_kind: "capacitor",
                value: None,
                max_hops: 1,
            },
        );
        let hit = hits.iter().find(|h| h.part == 30).ok_or("missing hit")?;
        assert_eq!(hit.path, vec![20, 30]);
        Ok(())
    }

    #[test]
    fn walk_clamps_hop_limit_at_cap() {
        let q = Query {
            start: PinRef {
                component: 1,
                pin: "1".into(),
            },
            target_kind: "capacitor",
            value: None,
            max_hops: 255,
        };
        assert_eq!(q.hop_limit(), HOP_CAP);
    }
}
