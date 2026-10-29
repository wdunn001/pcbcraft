//! Read-only comparison of two saved designs for review and automation.
use pcbcraft_doc::{Component, Footprint, Id, Project};
use pcbcraft_geom::Point;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const EPS: f64 = 1e-6;
const MAX_JSON_CHANGES: usize = 4096;

/// One added, removed, or changed object when two saved designs are compared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignChange {
    /// `added`, `removed`, or `changed`.
    pub kind: String,
    /// Stable object id in the design that moved.
    pub object: Id,
    /// Short human label, such as `component R1` or `net GND`.
    pub summary: String,
    /// Field name when the change is scalar; absent for whole-object add/remove.
    pub field: Option<String>,
    /// Previous value for a field change, as text.
    pub before: Option<String>,
    /// New value for a field change, as text.
    pub after: Option<String>,
}

/// Sorted list of differences between an old and new [`Project`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesignDiff {
    /// Every change, ordered by kind, object id, then summary.
    pub changes: Vec<DesignChange>,
}

impl DesignDiff {
    /// True when no changes were recorded.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// One line per change, sorted lexicographically for stable logs and diffs.
    pub fn to_text(&self) -> String {
        let mut lines: Vec<String> = self
            .changes
            .iter()
            .map(|c| {
                if let (Some(field), Some(before), Some(after)) = (&c.field, &c.before, &c.after) {
                    format!(
                        "{} {} {} {}: {} -> {}",
                        c.kind, c.object, c.summary, field, before, after
                    )
                } else {
                    format!("{} {} {}", c.kind, c.object, c.summary)
                }
            })
            .collect();
        lines.sort();
        lines.join("\n")
    }

    /// JSON summary with `count`, `truncated`, and a `changes` array for automation.
    pub fn to_json(&self) -> Value {
        let count = self.changes.len();
        let truncated = count > MAX_JSON_CHANGES;
        let changes: Vec<Value> = self
            .changes
            .iter()
            .take(MAX_JSON_CHANGES)
            .map(|c| {
                json!({
                    "kind": c.kind,
                    "object": c.object,
                    "summary": c.summary,
                    "field": c.field,
                    "before": c.before,
                    "after": c.after,
                })
            })
            .collect();
        json!({
            "count": count,
            "truncated": truncated,
            "changes": changes,
        })
    }
}

fn f64_changed(a: f64, b: f64) -> bool {
    (a - b).abs() > EPS
}

fn point_changed(a: Point, b: Point) -> bool {
    f64_changed(a.x, b.x) || f64_changed(a.y, b.y)
}

fn push_change(
    changes: &mut Vec<DesignChange>,
    kind: &str,
    object: Id,
    summary: String,
    field: Option<&str>,
    before: Option<String>,
    after: Option<String>,
) {
    changes.push(DesignChange {
        kind: kind.into(),
        object,
        summary,
        field: field.map(str::to_owned),
        before,
        after,
    });
}

fn ids_by_id<T>(items: &[T], id: impl Fn(&T) -> Id) -> BTreeMap<Id, usize> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| (id(item), i))
        .collect()
}

fn diff_pair_maps(
    changes: &mut Vec<DesignChange>,
    old_ids: &BTreeSet<Id>,
    new_ids: &BTreeSet<Id>,
    summary: impl Fn(Id) -> String,
) {
    for id in new_ids.difference(old_ids) {
        push_change(changes, "added", *id, summary(*id), None, None, None);
    }
    for id in old_ids.difference(new_ids) {
        push_change(changes, "removed", *id, summary(*id), None, None, None);
    }
}

fn diff_component_fields(
    changes: &mut Vec<DesignChange>,
    reference: &str,
    old: &Component,
    new: &Component,
) {
    let id = old.id;
    if old.reference != new.reference {
        push_change(
            changes,
            "changed",
            id,
            format!("component {reference}"),
            Some("reference"),
            Some(old.reference.clone()),
            Some(new.reference.clone()),
        );
    }
    if old.value != new.value {
        push_change(
            changes,
            "changed",
            id,
            format!("component {reference}"),
            Some("value"),
            Some(old.value.clone()),
            Some(new.value.clone()),
        );
    }
    if point_changed(old.position, new.position) {
        push_change(
            changes,
            "changed",
            id,
            format!("component {reference}"),
            Some("position"),
            Some(format!("{:.6},{:.6}", old.position.x, old.position.y)),
            Some(format!("{:.6},{:.6}", new.position.x, new.position.y)),
        );
    }
    if f64_changed(old.rotation, new.rotation) {
        push_change(
            changes,
            "changed",
            id,
            format!("component {reference}"),
            Some("rotation"),
            Some(format!("{:.6}", old.rotation)),
            Some(format!("{:.6}", new.rotation)),
        );
    }
    let old_sym = old
        .library_symbol
        .as_ref()
        .map(|s| format!("{}/{}", s.library, s.definition.name));
    let new_sym = new
        .library_symbol
        .as_ref()
        .map(|s| format!("{}/{}", s.library, s.definition.name));
    if old_sym != new_sym {
        push_change(
            changes,
            "changed",
            id,
            format!("component {reference}"),
            Some("library_symbol"),
            old_sym,
            new_sym,
        );
    }
    let old_fp = old
        .library_footprint
        .as_ref()
        .map(|s| format!("{}/{}", s.library, s.definition.name));
    let new_fp = new
        .library_footprint
        .as_ref()
        .map(|s| format!("{}/{}", s.library, s.definition.name));
    if old_fp != new_fp {
        push_change(
            changes,
            "changed",
            id,
            format!("component {reference}"),
            Some("library_footprint"),
            old_fp,
            new_fp,
        );
    }
    let mut old_fields: BTreeMap<String, String> = old
        .fields
        .iter()
        .map(|f| (f.key.clone(), f.value.clone()))
        .collect();
    let mut new_fields: BTreeMap<String, String> = new
        .fields
        .iter()
        .map(|f| (f.key.clone(), f.value.clone()))
        .collect();
    let field_keys: BTreeSet<String> = old_fields
        .keys()
        .chain(new_fields.keys())
        .cloned()
        .collect();
    for key in field_keys {
        let ob = old_fields.remove(&key);
        let nb = new_fields.remove(&key);
        if ob != nb {
            push_change(
                changes,
                "changed",
                id,
                format!("component {reference}"),
                Some(&format!("field:{key}")),
                ob,
                nb,
            );
        }
    }
    let old_pins: BTreeMap<String, Option<Id>> =
        old.pins.iter().map(|p| (p.number.clone(), p.net)).collect();
    let new_pins: BTreeMap<String, Option<Id>> =
        new.pins.iter().map(|p| (p.number.clone(), p.net)).collect();
    for number in old_pins
        .keys()
        .chain(new_pins.keys())
        .collect::<BTreeSet<_>>()
    {
        let on = old_pins.get(number.as_str()).copied().flatten();
        let nn = new_pins.get(number.as_str()).copied().flatten();
        if on != nn {
            push_change(
                changes,
                "changed",
                id,
                format!("component {reference} pin {number} re-pinned"),
                Some("pin_net"),
                on.map(|n| n.to_string()),
                nn.map(|n| n.to_string()),
            );
        }
    }
}

fn diff_components(changes: &mut Vec<DesignChange>, old: &Project, new: &Project) {
    let old_map = ids_by_id(&old.components, |c| c.id);
    let new_map = ids_by_id(&new.components, |c| c.id);
    let old_ids: BTreeSet<Id> = old_map.keys().copied().collect();
    let new_ids: BTreeSet<Id> = new_map.keys().copied().collect();
    diff_pair_maps(changes, &old_ids, &new_ids, |id| {
        new.components
            .iter()
            .find(|c| c.id == id)
            .or_else(|| old.components.iter().find(|c| c.id == id))
            .map(|c| format!("component {}", c.reference))
            .unwrap_or_else(|| format!("component id {id}"))
    });
    for id in old_ids.intersection(&new_ids) {
        let o = &old.components[old_map[id]];
        let n = &new.components[new_map[id]];
        diff_component_fields(changes, &o.reference, o, n);
    }
}

fn diff_nets(changes: &mut Vec<DesignChange>, old: &Project, new: &Project) {
    let old_map = ids_by_id(&old.nets, |n| n.id);
    let new_map = ids_by_id(&new.nets, |n| n.id);
    let old_ids: BTreeSet<Id> = old_map.keys().copied().collect();
    let new_ids: BTreeSet<Id> = new_map.keys().copied().collect();
    diff_pair_maps(changes, &old_ids, &new_ids, |id| {
        new.nets
            .iter()
            .find(|n| n.id == id)
            .or_else(|| old.nets.iter().find(|n| n.id == id))
            .map(|n| format!("net {}", n.name))
            .unwrap_or_else(|| format!("net id {id}"))
    });
    for id in old_ids.intersection(&new_ids) {
        let o = &old.nets[old_map[id]];
        let n = &new.nets[new_map[id]];
        if o.name != n.name {
            push_change(
                changes,
                "changed",
                *id,
                format!("net {}", o.name),
                Some("name"),
                Some(o.name.clone()),
                Some(n.name.clone()),
            );
        }
    }
}

fn diff_footprint(
    changes: &mut Vec<DesignChange>,
    old: &Footprint,
    new: &Footprint,
    reference: &str,
) {
    let id = old.id;
    if point_changed(old.position, new.position) {
        push_change(
            changes,
            "changed",
            id,
            format!("footprint {reference}"),
            Some("position"),
            Some(format!("{:.6},{:.6}", old.position.x, old.position.y)),
            Some(format!("{:.6},{:.6}", new.position.x, new.position.y)),
        );
    }
    if f64_changed(old.rotation, new.rotation) {
        push_change(
            changes,
            "changed",
            id,
            format!("footprint {reference}"),
            Some("rotation"),
            Some(format!("{:.6}", old.rotation)),
            Some(format!("{:.6}", new.rotation)),
        );
    }
    if old.locked != new.locked {
        push_change(
            changes,
            "changed",
            id,
            format!("footprint {reference}"),
            Some("locked"),
            old.locked.map(|v| v.to_string()),
            new.locked.map(|v| v.to_string()),
        );
    }
    let old_pads: BTreeMap<String, Option<Id>> =
        old.pads.iter().map(|p| (p.number.clone(), p.net)).collect();
    let new_pads: BTreeMap<String, Option<Id>> =
        new.pads.iter().map(|p| (p.number.clone(), p.net)).collect();
    for number in old_pads
        .keys()
        .chain(new_pads.keys())
        .collect::<BTreeSet<_>>()
    {
        let on = old_pads.get(number.as_str()).copied().flatten();
        let nn = new_pads.get(number.as_str()).copied().flatten();
        if on != nn {
            push_change(
                changes,
                "changed",
                id,
                format!("footprint {reference} pad {number}"),
                Some("pad_net"),
                on.map(|n| n.to_string()),
                nn.map(|n| n.to_string()),
            );
        }
    }
}

fn diff_footprints(changes: &mut Vec<DesignChange>, old: &Project, new: &Project) {
    let old_map = ids_by_id(&old.board.footprints, |f| f.id);
    let new_map = ids_by_id(&new.board.footprints, |f| f.id);
    let old_ids: BTreeSet<Id> = old_map.keys().copied().collect();
    let new_ids: BTreeSet<Id> = new_map.keys().copied().collect();
    diff_pair_maps(changes, &old_ids, &new_ids, |id| {
        let fp = new
            .board
            .footprints
            .iter()
            .find(|f| f.id == id)
            .or_else(|| old.board.footprints.iter().find(|f| f.id == id));
        fp.and_then(|f| {
            old.component(f.component)
                .or_else(|| new.component(f.component))
                .map(|c| format!("footprint {}", c.reference))
        })
        .unwrap_or_else(|| format!("footprint id {id}"))
    });
    for id in old_ids.intersection(&new_ids) {
        let o = &old.board.footprints[old_map[id]];
        let n = &new.board.footprints[new_map[id]];
        let reference = old
            .component(o.component)
            .or_else(|| new.component(n.component))
            .map(|c| c.reference.as_str())
            .unwrap_or("?");
        diff_footprint(changes, o, n, reference);
    }
}

fn diff_tracks(changes: &mut Vec<DesignChange>, old: &Project, new: &Project) {
    diff_id_entities(
        changes,
        &old.board.tracks,
        &new.board.tracks,
        |t| t.id,
        |o, n| {
            let mut local = Vec::new();
            if point_changed(o.start, n.start) || point_changed(o.end, n.end) {
                push_change(
                    &mut local,
                    "changed",
                    o.id,
                    "track geometry".into(),
                    Some("segment"),
                    Some(format!(
                        "{:.6},{:.6} -> {:.6},{:.6}",
                        o.start.x, o.start.y, o.end.x, o.end.y
                    )),
                    Some(format!(
                        "{:.6},{:.6} -> {:.6},{:.6}",
                        n.start.x, n.start.y, n.end.x, n.end.y
                    )),
                );
            }
            if o.net != n.net {
                push_change(
                    &mut local,
                    "changed",
                    o.id,
                    "track net".into(),
                    Some("net"),
                    Some(o.net.to_string()),
                    Some(n.net.to_string()),
                );
            }
            if f64_changed(o.width, n.width) {
                push_change(
                    &mut local,
                    "changed",
                    o.id,
                    "track width".into(),
                    Some("width"),
                    Some(format!("{:.6}", o.width)),
                    Some(format!("{:.6}", n.width)),
                );
            }
            if o.layer != n.layer {
                push_change(
                    &mut local,
                    "changed",
                    o.id,
                    "track layer".into(),
                    Some("layer"),
                    Some(format!("{:?}", o.layer)),
                    Some(format!("{:?}", n.layer)),
                );
            }
            local
        },
    );
}

fn diff_vias(changes: &mut Vec<DesignChange>, old: &Project, new: &Project) {
    diff_id_entities(
        changes,
        &old.board.vias,
        &new.board.vias,
        |v| v.id,
        |o, n| {
            let mut local = Vec::new();
            if point_changed(o.position, n.position) {
                push_change(
                    &mut local,
                    "changed",
                    o.id,
                    "via position".into(),
                    Some("position"),
                    Some(format!("{:.6},{:.6}", o.position.x, o.position.y)),
                    Some(format!("{:.6},{:.6}", n.position.x, n.position.y)),
                );
            }
            if o.net != n.net {
                push_change(
                    &mut local,
                    "changed",
                    o.id,
                    "via net".into(),
                    Some("net"),
                    Some(o.net.to_string()),
                    Some(n.net.to_string()),
                );
            }
            local
        },
    );
}

fn diff_id_entities<T>(
    changes: &mut Vec<DesignChange>,
    old_items: &[T],
    new_items: &[T],
    id: impl Fn(&T) -> Id,
    compare: impl Fn(&T, &T) -> Vec<DesignChange>,
) {
    let old_map = ids_by_id(old_items, &id);
    let new_map = ids_by_id(new_items, &id);
    let old_ids: BTreeSet<Id> = old_map.keys().copied().collect();
    let new_ids: BTreeSet<Id> = new_map.keys().copied().collect();
    diff_pair_maps(changes, &old_ids, &new_ids, |oid| format!("id {oid}"));
    for oid in old_ids.intersection(&new_ids) {
        changes.extend(compare(&old_items[old_map[oid]], &new_items[new_map[oid]]));
    }
}

fn diff_board_scalar(changes: &mut Vec<DesignChange>, old: &Project, new: &Project) {
    if f64_changed(old.board.clearance, new.board.clearance) {
        push_change(
            changes,
            "changed",
            0,
            "board clearance".into(),
            Some("clearance"),
            Some(format!("{:.6}", old.board.clearance)),
            Some(format!("{:.6}", new.board.clearance)),
        );
    }
    if f64_changed(old.board.width, new.board.width)
        || f64_changed(old.board.height, new.board.height)
    {
        push_change(
            changes,
            "changed",
            0,
            "board size".into(),
            Some("outline"),
            Some(format!("{:.6}x{:.6}", old.board.width, old.board.height)),
            Some(format!("{:.6}x{:.6}", new.board.width, new.board.height)),
        );
    }
}

fn diff_net_classes(changes: &mut Vec<DesignChange>, old: &Project, new: &Project) {
    diff_id_entities(
        changes,
        &old.net_classes,
        &new.net_classes,
        |c| c.id,
        |o, n| {
            let mut local = Vec::new();
            if o.name != n.name {
                push_change(
                    &mut local,
                    "changed",
                    o.id,
                    format!("net class {}", o.name),
                    Some("name"),
                    Some(o.name.clone()),
                    Some(n.name.clone()),
                );
            }
            if f64_changed(o.clearance, n.clearance) {
                push_change(
                    &mut local,
                    "changed",
                    o.id,
                    format!("net class {}", o.name),
                    Some("clearance"),
                    Some(format!("{:.6}", o.clearance)),
                    Some(format!("{:.6}", n.clearance)),
                );
            }
            local
        },
    );
}

fn diff_custom_checks(changes: &mut Vec<DesignChange>, old: &Project, new: &Project) {
    diff_id_entities(
        changes,
        &old.custom_checks,
        &new.custom_checks,
        |c| c.id,
        |o, n| {
            let mut local = Vec::new();
            if o.name != n.name || o.measure != n.measure || f64_changed(o.limit, n.limit) {
                push_change(
                    &mut local,
                    "changed",
                    o.id,
                    format!("custom check {}", o.name),
                    Some("definition"),
                    Some(format!("{} {} {}", o.name, o.measure, o.limit)),
                    Some(format!("{} {} {}", n.name, n.measure, n.limit)),
                );
            }
            local
        },
    );
}

fn diff_schematic_only(changes: &mut Vec<DesignChange>, old: &Project, new: &Project) {
    diff_id_entities(changes, &old.wires, &new.wires, |w| w.id, |_, _| vec![]);
    diff_id_entities(
        changes,
        &old.drawn_wires,
        &new.drawn_wires,
        |w| w.id,
        |_, _| vec![],
    );
    diff_id_entities(changes, &old.labels, &new.labels, |l| l.id, |_, _| vec![]);
    diff_id_entities(
        changes,
        &old.graphics,
        &new.graphics,
        |g| g.id,
        |_, _| vec![],
    );
    diff_id_entities(
        changes,
        &old.no_connects,
        &new.no_connects,
        |f| f.id,
        |_, _| vec![],
    );
}

fn diff_rule_maps(changes: &mut Vec<DesignChange>, old: &Project, new: &Project) {
    if old.rule_severity != new.rule_severity {
        push_change(
            changes,
            "changed",
            0,
            "rule severity overrides".into(),
            Some("rule_severity"),
            Some(format!("{} entries", old.rule_severity.len())),
            Some(format!("{} entries", new.rule_severity.len())),
        );
    }
    if old.rule_exclusions != new.rule_exclusions {
        push_change(
            changes,
            "changed",
            0,
            "rule exclusions".into(),
            Some("rule_exclusions"),
            Some(format!("{} codes", old.rule_exclusions.len())),
            Some(format!("{} codes", new.rule_exclusions.len())),
        );
    }
}

/// Compare two validated designs and return every board and schematic delta found.
pub fn project_diff(old: &Project, new: &Project) -> DesignDiff {
    let mut changes = Vec::new();
    diff_components(&mut changes, old, new);
    diff_nets(&mut changes, old, new);
    diff_footprints(&mut changes, old, new);
    diff_tracks(&mut changes, old, new);
    diff_vias(&mut changes, old, new);
    diff_board_scalar(&mut changes, old, new);
    diff_net_classes(&mut changes, old, new);
    diff_custom_checks(&mut changes, old, new);
    diff_rule_maps(&mut changes, old, new);
    diff_schematic_only(&mut changes, old, new);
    changes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then(a.object.cmp(&b.object))
            .then(a.summary.cmp(&b.summary))
    });
    DesignDiff { changes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Component, Footprint, Net, Pad, Pin, Project};

    fn minimal_project() -> Project {
        let mut p = Project::default();
        p.components.push(Component {
            id: 10,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            pins: vec![Pin {
                number: "1".into(),
                name: "1".into(),
                offset: Point::default(),
                net: Some(1),
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
        });
        p.board.footprints.push(Footprint {
            id: 20,
            component: 10,
            position: Point::new(15.0, 15.0),
            rotation: 0.0,
            locked: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 1.0,
                net: Some(1),
                castellated: None,
                geometry: None,
            }],
        });
        p
    }

    #[test]
    fn moving_one_footprint_and_changing_one_value_yields_exactly_two_changes() {
        let old = minimal_project();
        let mut new = minimal_project();
        new.components[0].value = "2k2".into();
        new.board.footprints[0].position = Point::new(18.0, 15.0);
        let diff = project_diff(&old, &new);
        assert_eq!(diff.changes.len(), 2, "{:?}", diff.changes);
        assert!(
            diff.changes
                .iter()
                .any(|c| c.field.as_deref() == Some("value"))
        );
        assert!(
            diff.changes
                .iter()
                .any(|c| c.field.as_deref() == Some("position"))
        );
    }

    #[test]
    fn identical_projects_yield_empty_diff() {
        let p = minimal_project();
        assert!(project_diff(&p, &p).is_empty());
    }

    #[test]
    fn renamed_net_with_same_id_records_name_change() {
        let mut old = minimal_project();
        let mut new = minimal_project();
        old.nets.push(Net {
            id: 1,
            name: "GND".into(),
            target_length: None,
            length_tolerance: None,
        });
        new.nets.push(Net {
            id: 1,
            name: "VSS".into(),
            target_length: None,
            length_tolerance: None,
        });
        let diff = project_diff(&old, &new);
        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].field.as_deref(), Some("name"));
    }

    #[test]
    fn repinned_component_pin_records_one_change() {
        let mut old = minimal_project();
        let mut new = minimal_project();
        for net in [(1, "A"), (2, "B")] {
            old.nets.push(Net {
                id: net.0,
                name: net.1.into(),
                target_length: None,
                length_tolerance: None,
            });
            new.nets.push(Net {
                id: net.0,
                name: net.1.into(),
                target_length: None,
                length_tolerance: None,
            });
        }
        new.components[0].pins[0].net = Some(2);
        let diff = project_diff(&old, &new);
        assert_eq!(diff.changes.len(), 1);
        assert!(diff.changes[0].summary.contains("re-pinned"));
    }
}
