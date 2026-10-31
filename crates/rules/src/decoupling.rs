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
//!   capacitor itself. Pins on a ground-named net (GND, VSS, ground, earth) are
//!   skipped: the return path is not what this check is about.
//! - A capacitor is a part of kind `capacitor`, or a reference starting with C
//!   and a digit. It counts only if its other pin sits on a different net.
//! - No capacitor on the net at all is always a finding. A capacitor that
//!   exists but is too far is a finding only when the power pad and the
//!   capacitor pad are both placed; unplaced parts are never guessed at.
//! - Distance is pad centre to pad centre in millimetres, from
//!   `Project::decoupling`. One finding per power pin, in component then pin
//!   order, so repeated runs give the same list.
//!
//! Dependencies: `crates/doc/src/lib.rs` (`DecouplingRule`, `Footprint`,
//! `Project`).
use super::Finding;
use pcbcraft_doc::{Component, Id, Project};
use pcbcraft_geom::Point;

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

/// World position of pad `number` of the part, if the part is placed.
fn pad_point(project: &Project, component: Id, number: &str) -> Option<Point> {
    project
        .board
        .footprints
        .iter()
        .find(|f| f.component == component)
        .and_then(|f| f.pad_position(number))
}

/// One capacitor that decouples a net, with the pad numbers of its pins on it.
struct Cap<'a> {
    part: &'a Component,
    pads: Vec<&'a str>,
}

/// Capacitors with a pin on `net` and another pin on a different net.
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
            let returns = c.pins.iter().any(|p| p.net.is_some() && p.net != Some(net));
            (!pads.is_empty() && returns).then_some(Cap { part: c, pads })
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
        if is_capacitor(c) {
            continue;
        }
        for pin in &c.pins {
            let Some(net) = pin.net.filter(|_| pin.electrical_type == "power_in") else {
                continue;
            };
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
            let Some(from) = pad_point(project, c.id, &pin.number) else {
                continue;
            };
            let mut nearest: Option<(f64, &Component)> = None;
            for cap in &caps {
                for pad in &cap.pads {
                    if let Some(at) = pad_point(project, cap.part.id, pad) {
                        let d = from.distance(at);
                        if nearest.is_none_or(|(best, _)| d < best) {
                            nearest = Some((d, cap.part));
                        }
                    }
                }
            }
            let Some((d, cap)) = nearest else {
                continue;
            };
            if d > rule.max_distance_mm + 1e-9 {
                result.push(finding(
                    project,
                    format!(
                        "{label}: the nearest capacitor on {net_name} is {} at {d:.1} mm, over the {:.1} mm limit; \
                         move it closer or add {} next to the pin",
                        cap.reference, rule.max_distance_mm, rule.suggested_value
                    ),
                    vec![c.id, cap.id],
                ));
            }
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
    }
}
