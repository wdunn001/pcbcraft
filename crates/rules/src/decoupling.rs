//! The `missing_decoupling` check: every integrated circuit power pin needs a
//! capacitor on the same net, close to the pin.
//!
//! Context: called once from `erc` in `lib.rs`, so its findings travel with the
//! other design-intent findings and obey `rule_severity`, exclusions and
//! regions like every other code. It reads the schematic pins (which net, which
//! electrical type) and, when both parts are placed, the board pad positions.
//!
//! Local rules:
//! - A power pin is a pin of electrical type `power_in` on a part that is not a
//!   capacitor itself, not a resistor, connector or LED (unless its reference
//!   starts with U), and not flagged no-connect. Pins on a ground-named net (GND, VSS, ground, earth) are
//!   skipped: the return path is not what this check is about.
//! - A capacitor is a part of kind `capacitor`, or a reference starting with C
//!   and a digit. It counts when any of its pins sits on the net.
//! - No capacitor on the net at all is always a `missing_decoupling` finding.
//!   Once a capacitor is on the net, the distance of the copper path decides:
//!   `decoupling_distance` when the path is longer than the limit for that
//!   capacitor value, and `decoupling_ground_via` when the capacitor ground pad
//!   has no via or ground pour within `GROUND_RETURN_MM`. A net with a
//!   capacitor and no copper path to it is left to `unrouted`.
//! - The path walks the same touching graph `airwires` builds: tracks cost
//!   their length, vias the board thickness, pads and poured copper nothing.
//!   The limit comes from `DecouplingRule::distance_for_value`, so a project
//!   limit other than the 3.0 mm default wins outright.
//! - One finding per power pin per code, in component then pin order, so
//!   repeated runs give the same list.
//!
//! Dependencies: `crates/doc/src/lib.rs` (`DecouplingRule`, `Footprint`,
//! `Project`); `crates/rules/src/lib.rs` (`copper`, `touching`, `Grid`,
//! `Copper`, `gap`, `inside`, `polygon_segment`).
use super::{Copper, Finding};
use pcbcraft_doc::{Component, Id, Pin, Project};
use std::collections::{BTreeMap, BTreeSet};

/// True for a net name that is a return path rather than a supply.
fn is_ground_name(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["GND", "VSS", "GROUND", "EARTH"]
        .iter()
        .any(|g| n.contains(g))
}

/// True when the part is a capacitor by kind or by reference.
fn is_capacitor(c: &Component) -> bool {
    if c.kind == "capacitor" {
        return true;
    }
    let mut chars = c.reference.chars();
    chars.next() == Some('C') && chars.next().is_some_and(|d| d.is_ascii_digit())
}

/// One capacitor that decouples a net, with the pad numbers of its pins on it.
struct Cap<'a> {
    part: &'a Component,
    pads: Vec<&'a str>,
}

/// Capacitors with a pin on `net`.
fn caps_on_net(project: &Project, net: Id) -> Vec<Cap<'_>> {
    project
        .components
        .iter()
        .filter(|c| is_capacitor(c))
        .filter_map(|c| {
            let pads: Vec<&str> = c
                .pins
                .iter()
                .filter(|p| p.net == Some(net))
                .map(|p| p.number.as_str())
                .collect();
            (!pads.is_empty()).then_some(Cap { part: c, pads })
        })
        .collect()
}

/// Findings for power pins without a nearby capacitor on their net.
///
/// # Rules
/// - Code `missing_decoupling`, default severity `warning` through
///   `Project::rule_severity`. Objects are the part id, plus the nearest
///   capacitor id when the finding is about distance.
/// - The message names the part, the pin number and name, the net, and the
///   suggested value, so a person can act on it without opening the part.
///
/// # Dependencies
/// - crates/doc/src/lib.rs `DecouplingRule` supplies distance and value.
///
/// # Do not
/// - Guess a distance for a part that is not on the board.
/// - Filter by `rule_disabled` here; `erc` does that for every code.
pub(crate) fn missing_decoupling(project: &Project) -> Vec<Finding> {
    let rule = &project.decoupling;
    let mut result = vec![];
    for c in &project.components {
        // An integrated circuit by kind, or by its U reference when the kind says otherwise.
        let ic_like = pcbcraft_doc::is_ic_like_kind(&c.kind) || c.reference.starts_with('U');
        if is_capacitor(c) || !ic_like {
            continue;
        }
        for pin in &c.pins {
            let Some(net) = pin.net.filter(|_| pin.electrical_type == "power_in") else {
                continue;
            };
            if super::pin_flagged_no_connect(project, c.id, &pin.number) {
                continue;
            }
            let net_name = project
                .nets
                .iter()
                .find(|n| n.id == net)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            if is_ground_name(&net_name) {
                continue;
            }
            let label = if pin.name.is_empty() || pin.name == pin.number {
                format!("{} pin {}", c.reference, pin.number)
            } else {
                format!("{} pin {} ({})", c.reference, pin.number, pin.name)
            };
            let caps = caps_on_net(project, net);
            if caps.is_empty() {
                result.push(finding(
                    project,
                    format!(
                        "{label} has no capacitor on {net_name}; add {} next to the pin",
                        rule.suggested_value
                    ),
                    vec![c.id],
                ));
                continue;
            }
            // Presence is enough here. The distance of the copper path needs
            // placed copper, so `decoupling_path_findings` reports it inside
            // `drc`, along with the capacitor ground return check.
        }
    }
    result
}

fn finding(project: &Project, message: String, objects: Vec<Id>) -> Finding {
    Finding {
        code: "missing_decoupling",
        severity: project.rule_severity("missing_decoupling", "warning"),
        message,
        objects,
        hint: None,
    }
}

/// Millimetres a capacitor ground pad must stay within of return copper.
const GROUND_RETURN_MM: f64 = 1.0;

/// The capacitor on a power net nearest by copper path, with the pad the path
/// ends on and the path length in millimetres.
struct CapPath<'a> {
    part: &'a Component,
    ground_pad: String,
    mm: f64,
}

/// Shortest copper path from the pad of `pin` to the nearest capacitor pad on
/// its net, or `None` when no copper connects them.
///
/// # Rules
/// - The walk uses the touching graph `airwires` builds, so a capacitor with no
///   copper between it and the pin is `unrouted` and never a distance.
/// - Tracks cost their length, vias the board thickness, pads and poured copper
///   nothing, and the first capacitor pad reached wins.
///
/// # Dependencies
/// - crates/rules/src/lib.rs `copper`, `touching`, `Grid`, `Copper`.
///
/// # Do not
/// - Fall back to a pad centre to pad centre straight line when nothing touches.
fn nearest_cap_by_copper<'a>(
    project: &Project,
    items: &[Copper],
    neighbours: &[Vec<usize>],
    part: &'a Component,
    pin: &Pin,
    caps: &[Cap<'a>],
) -> Option<CapPath<'a>> {
    let net = pin.net?;
    let power_footprint = footprint_of(project, part.id)?;
    let power_pad = board_pad_number(project, part, &pin.number, net)?;
    let from = copper_index(project, items, power_footprint, &power_pad)?;
    let mut targets: BTreeMap<usize, CapPath<'a>> = BTreeMap::new();
    for cap in caps {
        let Some(footprint) = footprint_of(project, cap.part.id) else {
            continue;
        };
        for pad in &cap.pads {
            let Some(index) = copper_index(project, items, footprint, pad) else {
                continue;
            };
            targets.insert(
                index,
                CapPath {
                    part: cap.part,
                    ground_pad: other_board_pad(project, cap.part, pad, net),
                    mm: 0.0,
                },
            );
        }
    }
    let wanted: BTreeSet<usize> = targets.keys().copied().collect();
    let (index, mm) = copper_path(items, neighbours, from, &wanted, project.board.thickness)?;
    let mut best = targets.remove(&index)?;
    best.mm = mm;
    Some(best)
}
/// Board footprint of the component, when the part is placed.
fn footprint_of(project: &Project, component: Id) -> Option<Id> {
    project
        .board
        .footprints
        .iter()
        .find(|f| f.component == component)
        .map(|f| f.id)
}

/// Footprint pad number carrying the net of schematic pin `number`, matched by
/// footprint position first so a multi unit part names the pad the fab plots.
fn board_pad_number(project: &Project, part: &Component, number: &str, net: Id) -> Option<String> {
    let footprint = project
        .board
        .footprints
        .iter()
        .find(|f| f.component == part.id)?;
    let definition = project
        .component(part.id)
        .and_then(|c| c.library_footprint.as_ref())
        .map(|l| l.definition.pads.as_slice())
        .unwrap_or(&[]);
    let by_position = definition
        .iter()
        .find(|def| def.number == number)
        .and_then(|def| {
            footprint.pads.iter().find(|pad| {
                pad.geometry
                    .as_ref()
                    .is_some_and(|g| g.position.distance(def.position) < 1e-9)
            })
        })
        .map(|pad| pad.number.clone());
    by_position.or_else(|| {
        footprint
            .pads
            .iter()
            .find(|pad| pad.number == number && pad.net == Some(net))
            .map(|pad| pad.number.clone())
    })
}

/// Footprint pad number of the capacitor that is not `power_pad` and not on the
/// power net: the pad the return current has to reach.
fn other_board_pad(project: &Project, cap: &Component, power_pad: &str, net: Id) -> String {
    project
        .board
        .footprints
        .iter()
        .find(|f| f.component == cap.id)
        .and_then(|footprint| {
            footprint
                .pads
                .iter()
                .find(|pad| pad.number != power_pad && pad.net != Some(net))
                .or_else(|| footprint.pads.iter().find(|pad| pad.number != power_pad))
                .map(|pad| pad.number.clone())
        })
        .unwrap_or_default()
}

/// Index in `items` of the copper of pad `number` of footprint `footprint`.
fn copper_index(project: &Project, items: &[Copper], footprint: Id, number: &str) -> Option<usize> {
    let at = project
        .board
        .footprints
        .iter()
        .find(|f| f.id == footprint)?
        .pad_position(number)?;
    items
        .iter()
        .position(|item| item.pad && item.id == footprint && item.start.distance(at) < 1e-6)
}

/// Shortest copper path in millimetres from item `from` to the first of
/// `targets`, over the touching graph `airwires` builds.
fn copper_path(
    items: &[Copper],
    neighbours: &[Vec<usize>],
    from: usize,
    targets: &BTreeSet<usize>,
    thickness: f64,
) -> Option<(usize, f64)> {
    let net = items.get(from)?.net;
    let nodes: Vec<usize> = (0..items.len()).filter(|i| items[*i].net == net).collect();
    let mut slot = vec![usize::MAX; items.len()];
    for (k, i) in nodes.iter().enumerate() {
        if let Some(place) = slot.get_mut(*i) {
            *place = k;
        }
    }
    let start = *slot.get(from)?;
    let mut dist = vec![f64::INFINITY; nodes.len()];
    let mut done = vec![false; nodes.len()];
    if let Some(first) = dist.get_mut(start) {
        *first = 0.0;
    }
    loop {
        let mut pick: Option<(f64, usize)> = None;
        for k in 0..nodes.len() {
            if done.get(k).copied().unwrap_or(true) {
                continue;
            }
            let distance = dist.get(k).copied().unwrap_or(f64::INFINITY);
            if !distance.is_finite() || pick.is_some_and(|(best, _)| distance >= best) {
                continue;
            }
            pick = Some((distance, k));
        }
        let (distance, k) = pick?;
        let reached = *nodes.get(k)?;
        if targets.contains(&reached) {
            return Some((reached, distance));
        }
        if let Some(flag) = done.get_mut(k) {
            *flag = true;
        }
        for &next in neighbours.get(reached).map(Vec::as_slice).unwrap_or(&[]) {
            let place = *slot.get(next)?;
            if place == usize::MAX || done.get(place).copied().unwrap_or(true) {
                continue;
            }
            let Some(item) = items.get(next) else {
                continue;
            };
            let cost = distance + step_cost(item, thickness);
            if dist.get(place).is_some_and(|current| *current > cost)
                && let Some(current) = dist.get_mut(place)
            {
                *current = cost;
            }
        }
    }
}

/// Neighbour lists of the touching graph `airwires` builds, both directions.
fn adjacency(items: &[Copper]) -> Vec<Vec<usize>> {
    let grid = super::Grid::new(items, 0.0);
    let mut result: Vec<Vec<usize>> = vec![vec![]; items.len()];
    for (i, candidates) in grid.near.iter().enumerate() {
        let Some(a) = items.get(i) else {
            continue;
        };
        for &j in candidates.iter() {
            let Some(b) = items.get(j) else {
                continue;
            };
            if i == j || !super::touching(a, b) {
                continue;
            }
            if let Some(list) = result.get_mut(i) {
                list.push(j);
            }
            if let Some(list) = result.get_mut(j) {
                list.push(i);
            }
        }
    }
    result
}

/// Millimetres a step into `item` costs: a track its length, a via the board
/// thickness, and a pad or poured copper nothing.
fn step_cost(item: &Copper, thickness: f64) -> f64 {
    if item.bridge.as_ref().is_some_and(|(_, kind)| kind == "via") {
        return thickness.max(0.0);
    }
    if item.pad || item.outline.is_some() {
        0.0
    } else {
        item.start.distance(item.end)
    }
}

/// Millimetres from a capacitor ground pad to the nearest return copper, or
/// `None` when the pad is off the board or its net is not a return path.
fn ground_return_gap(project: &Project, items: &[Copper], cap: &CapPath<'_>) -> Option<f64> {
    let pin = cap.part.pins.iter().find(|p| p.number == cap.ground_pad)?;
    let ground = pin.net?;
    if !is_return_net(project, ground) {
        return None;
    }
    let footprint = footprint_of(project, cap.part.id)?;
    let index = copper_index(project, items, footprint, &cap.ground_pad)?;
    let pad = items.get(index)?;
    let mut best: Option<f64> = None;
    for item in items
        .iter()
        .filter(|item| item.net == ground && item.id != footprint)
    {
        let plated_via = item.bridge.as_ref().is_some_and(|(_, kind)| kind == "via");
        let ring = item.outline.as_ref().filter(|ring| ring.len() > 2);
        if !plated_via && ring.is_none() {
            continue;
        }
        let distance = if let Some(ring) = ring {
            if super::inside(pad.start, ring) {
                0.0
            } else {
                super::polygon_segment(ring, pad.start, pad.start) - pad.radius
            }
        } else {
            super::gap(pad, item)
        };
        if best.is_none_or(|current| distance < current) {
            best = Some(distance.max(0.0));
        }
    }
    best
}

/// Name of the net the capacitor ground pad sits on, empty when it has none.
fn ground_net_name(project: &Project, cap: &CapPath<'_>) -> String {
    cap.part
        .pins
        .iter()
        .find(|p| p.number == cap.ground_pad)
        .and_then(|p| p.net)
        .map(|net| project.net_name(net).to_string())
        .unwrap_or_default()
}

/// A decoupling distance finding at the severity the project set for `code`.
fn path_finding(
    project: &Project,
    code: &'static str,
    message: String,
    hint: Option<String>,
    objects: Vec<Id>,
) -> Finding {
    Finding {
        code,
        severity: project.rule_severity(code, "warning"),
        message,
        objects,
        hint,
    }
}

/// True when the net is a return path: named like one, or a power net driven by
/// a power output pin or a power symbol.
fn is_return_net(project: &Project, net: Id) -> bool {
    let name = project.net_name(net).to_string();
    if is_ground_name(&name) {
        return true;
    }
    project
        .labels
        .iter()
        .any(|label| label.net == net && label.kind == "power")
        || project.components.iter().any(|c| {
            c.pins
                .iter()
                .any(|p| p.net == Some(net) && p.electrical_type == "power_out")
        })
}

/// Findings for power pins whose nearest capacitor is too far away in copper,
/// and for capacitors whose ground pad has no return copper beside it.
///
/// # Rules
/// - Codes `decoupling_distance`, with the part and the capacitor as objects,
///   and `decoupling_ground_via`, with the capacitor as object, both default
///   `warning` through `Project::rule_severity`.
/// - `drc` runs this because both codes measure placed copper, and `drc` also
///   filters `rule_disabled`, per object exclusions and regions.
/// - A pin that has a capacitor on its net but no copper path to it is left to
///   `unrouted`, and an unplaced part is never guessed at.
///
/// # Dependencies
/// - crates/doc/src/lib.rs `DecouplingRule::distance_for_value` gives the limit.
/// - crates/rules/src/lib.rs `copper`, `touching` and `Grid` give the path.
///
/// # Do not
/// - Measure a straight line: a capacitor 1 mm away with no copper between it
///   and the pin is not connected at all.
pub(crate) fn decoupling_path_findings(project: &Project) -> Vec<Finding> {
    let rule = &project.decoupling;
    if project.board.footprints.is_empty() {
        return vec![];
    }
    let mut result = vec![];
    // The copper list and its touching graph cost a pass over the whole board,
    // so they are built once, and only when a power pin has a capacitor.
    let mut graph: Option<(Vec<Copper>, Vec<Vec<usize>>)> = None;
    for c in &project.components {
        let ic_like = pcbcraft_doc::is_ic_like_kind(&c.kind) || c.reference.starts_with('U');
        if is_capacitor(c) || !ic_like {
            continue;
        }
        for pin in &c.pins {
            let Some(net) = pin.net.filter(|_| pin.electrical_type == "power_in") else {
                continue;
            };
            if super::pin_flagged_no_connect(project, c.id, &pin.number) {
                continue;
            }
            let net_name = project.net_name(net).to_string();
            if is_ground_name(&net_name) {
                continue;
            }
            let caps = caps_on_net(project, net);
            if caps.is_empty() {
                continue;
            }
            let label = if pin.name.is_empty() || pin.name == pin.number {
                format!("{} pin {}", c.reference, pin.number)
            } else {
                format!("{} pin {} ({})", c.reference, pin.number, pin.name)
            };
            let (items, neighbours) = graph.get_or_insert_with(|| {
                let items = super::copper(project);
                let neighbours = adjacency(&items);
                (items, neighbours)
            });
            let (items, neighbours) = (items.as_slice(), neighbours.as_slice());
            let Some(cap) = nearest_cap_by_copper(project, items, neighbours, c, pin, &caps) else {
                // No copper path at all, which `unrouted` already reports.
                continue;
            };
            let limit = rule.distance_for_value(&cap.part.value);
            if cap.mm > limit + 1e-9 {
                result.push(path_finding(
                    project,
                    "decoupling_distance",
                    format!(
                        "{label}: nearest capacitor {} is {:.1} mm of copper away, at most \
                         {:.1} mm",
                        cap.part.reference, cap.mm, limit
                    ),
                    Some(format!(
                        "move {} next to the pin or add {}",
                        cap.part.reference, rule.suggested_value
                    )),
                    vec![c.id, cap.part.id],
                ));
            }
            if let Some(gap_mm) = ground_return_gap(project, items, &cap)
                && gap_mm > GROUND_RETURN_MM
            {
                result.push(path_finding(
                    project,
                    "decoupling_ground_via",
                    format!(
                        "{} ground pad on {} has no ground via or ground pour within {:.1} mm; \
                         the nearest return copper is {:.1} mm away",
                        cap.part.reference,
                        ground_net_name(project, &cap),
                        GROUND_RETURN_MM,
                        gap_mm
                    ),
                    Some(format!(
                        "add a via on {} next to the capacitor ground pad",
                        ground_net_name(project, &cap)
                    )),
                    vec![cap.part.id],
                ));
            }
        }
    }
    result
}
