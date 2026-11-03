//! Hole to copper and unplated hole size checks.
//!
//! Context: called once from `drc` in `lib.rs`, next to the other fabrication
//! checks. Both checks read `Board.rules` and stay silent when the matching
//! limit is not set, so an unknown fab limit never raises a finding.
//!
//! Local rules:
//! - A plated hole (via or plated pad) is measured wall to foreign copper on
//!   its own copper layer; copper of the same net is the hole's own land and
//!   is skipped, and so is the other copper of the same pad or via.
//! - An unplated hole (mounting hole, slot, unplated pad drill) is measured to
//!   copper of any net on every copper layer, since the drill bit removes
//!   whatever it meets.
//! - A gap below zero is a copper clash and belongs to the clearance check, so
//!   it is not repeated here.
//! - One finding per hole and copper object, at the smallest gap found.
//!
//! Dependencies: `crates/rules/src/lib.rs` (`copper`, `gap`), `crates/doc/src/fab.rs`
//! (`FabRules`), `crates/geom/src/lib.rs`.
use super::{Copper, Finding, copper, gap};
use pcbcraft_doc::{Id, Layer, Project};
use pcbcraft_geom::Point;
use std::collections::BTreeMap;

/// Gaps closer than this to the limit count as meeting it, in millimetres.
const SLACK_MM: f64 = 1e-6;

/// One unplated drill, as a capsule in board space.
struct Unplated {
    owner: Id,
    label: String,
    start: Point,
    end: Point,
    radius: f64,
    /// Centre of the pad this drill belongs to, when it is a pad drill.
    pad_at: Option<Point>,
}

fn unplated_holes(project: &Project) -> Vec<Unplated> {
    let mut all = vec![];
    for h in &project.board.holes {
        all.push(Unplated {
            owner: h.id,
            label: format!("Hole {}", h.id),
            start: h.start(),
            end: h.finish(),
            radius: h.diameter / 2.0,
            pad_at: None,
        });
    }
    for f in &project.board.footprints {
        let reference = project
            .component(f.component)
            .map_or_else(|| format!("footprint {}", f.id), |c| c.reference.clone());
        for pad in &f.pads {
            let Some(d) = pad.drill_capsule().filter(|d| !d.plated) else {
                continue;
            };
            let world = |p: Point| f.position.offset(p.rotate(-f.rotation));
            all.push(Unplated {
                owner: f.id,
                label: format!("Unplated hole of pad {} on {reference}", pad.number),
                start: world(d.start),
                end: world(d.end),
                radius: d.radius,
                pad_at: f.pad_position(&pad.number),
            });
        }
    }
    all
}

/// Which hole and which copper object a plated finding is about.
type PlatedKey = (Id, Id, u64, u64);
/// Smallest gap found, the hole's label, the finding code and the limit.
type PlatedHit = (f64, String, &'static str, f64);

fn capsule(start: Point, end: Point, radius: f64, layer: Layer) -> Copper {
    Copper {
        id: 0,
        net: 0,
        start,
        end,
        radius,
        contact_radius: radius,
        conservative: false,
        outline: None,
        outline_error: 0.0,
        layer,
        pad: false,
        clearance: None,
        bridge: None,
        hole: None,
        cutouts: vec![],
    }
}

fn same_point(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

/// True when `item` is the copper-layer stand-in of an unplated drill.
fn is_drill_shape(item: &Copper, drills: &[Unplated]) -> bool {
    !item.pad
        && item.net == 0
        && item.outline.is_none()
        && item.hole.is_none()
        && drills.iter().any(|d| {
            (item.radius - d.radius).abs() < 1e-9
                && same_point(item.start, d.start)
                && same_point(item.end, d.end)
        })
}

/// `hole_to_copper_via`, `hole_to_copper_pth` and `hole_to_copper_npth`
/// findings: a drilled wall closer to copper than the fab allows.
///
/// # Rules
/// - Each limit comes from `Board.rules`; a limit left unset skips its kind.
/// - Severity is `project.rule_severity(code, "error")`.
/// - A gap below zero is left to the clearance check.
///
/// # Dependencies
/// - crates/rules/src/lib.rs (`copper` and `gap` give the measured shapes)
///
/// # Do not
/// - Compare a plated hole with copper of its own net; that is its land.
pub(crate) fn hole_to_copper_findings(project: &Project) -> Vec<Finding> {
    let Some(rules) = project.board.rules.as_ref() else {
        return vec![];
    };
    if rules.hole_to_copper_via.is_none()
        && rules.hole_to_copper_pth.is_none()
        && rules.hole_to_copper_npth.is_none()
    {
        return vec![];
    }
    let items = copper(project);
    let drills = unplated_holes(project);
    let mut result = vec![];
    // Plated holes: one probe per via or pad and layer, deduplicated below.
    let mut plated: BTreeMap<PlatedKey, PlatedHit> = BTreeMap::new();
    for hole_item in items.iter().filter(|i| i.pad) {
        let (Some(hole), Some((owner, name))) = (&hole_item.hole, &hole_item.bridge) else {
            continue;
        };
        let (code, min, what) = if name == "via" {
            ("hole_to_copper_via", rules.hole_to_copper_via, "Via hole")
        } else {
            (
                "hole_to_copper_pth",
                rules.hole_to_copper_pth,
                "Plated hole",
            )
        };
        let Some(min) = min else { continue };
        let probe = capsule(hole.start, hole.end, hole.radius, hole_item.layer);
        for other in &items {
            if other.layer != hole_item.layer
                || other.bridge == hole_item.bridge
                || (other.net != 0 && other.net == hole_item.net)
                || is_drill_shape(other, &drills)
                || (other.id == *owner && same_point(other.start, hole_item.start))
            {
                continue;
            }
            let g = gap(&probe, other);
            if g < -SLACK_MM || g + SLACK_MM >= min {
                continue;
            }
            let key = (
                *owner,
                other.id,
                other.start.x.to_bits(),
                other.start.y.to_bits(),
            );
            let label = format!("{what} at {:.2}, {:.2}", hole.start.x, hole.start.y);
            let entry = plated.entry(key).or_insert((g, label, code, min));
            if g < entry.0 {
                entry.0 = g;
            }
        }
    }
    for ((owner, other, _, _), (g, label, code, min)) in plated {
        result.push(Finding {
            code,
            severity: project.rule_severity(code, "error"),
            message: format!(
                "{label} is {:.3} mm from copper of another net; fabrication needs at least {:.3} mm",
                g.max(0.0),
                min
            ),
            objects: vec![owner, other],
            hint: Some("Move the copper away from the hole or use a smaller land.".into()),
        });
    }
    // Unplated holes: any copper, any layer.
    if let Some(min) = rules.hole_to_copper_npth {
        for d in &drills {
            let mut best: BTreeMap<(Id, u64, u64), f64> = BTreeMap::new();
            for layer in project.copper_layers() {
                let probe = capsule(d.start, d.end, d.radius, layer);
                for other in items.iter().filter(|o| o.layer == layer) {
                    if is_drill_shape(other, &drills)
                        || (other.id == d.owner
                            && d.pad_at.is_some_and(|at| same_point(other.start, at)))
                    {
                        continue;
                    }
                    let g = gap(&probe, other);
                    if g < -SLACK_MM || g + SLACK_MM >= min {
                        continue;
                    }
                    let key = (other.id, other.start.x.to_bits(), other.start.y.to_bits());
                    let e = best.entry(key).or_insert(g);
                    if g < *e {
                        *e = g;
                    }
                }
            }
            for ((other, _, _), g) in best {
                result.push(Finding {
                    code: "hole_to_copper_npth",
                    severity: project.rule_severity("hole_to_copper_npth", "error"),
                    message: format!(
                        "{} is {:.3} mm from copper; fabrication needs at least {:.3} mm",
                        d.label,
                        g.max(0.0),
                        min
                    ),
                    objects: vec![d.owner, other],
                    hint: Some("Move the copper away from the hole.".into()),
                });
            }
        }
    }
    result
}

/// `min_npth` findings: an unplated hole drilled smaller than the fab's
/// smallest unplated hole.
///
/// # Rules
/// - Needs `FabRules.min_npth`; without it nothing is reported.
/// - The size is the drill diameter, which is the width of a slot.
///
/// # Dependencies
/// - crates/doc/src/lib.rs (`Hole`, `Pad::drill_capsule`)
///
/// # Do not
/// - Report plated holes; `min_drill` covers those.
pub(crate) fn npth_size_findings(project: &Project) -> Vec<Finding> {
    let Some(min) = project.board.rules.as_ref().and_then(|r| r.min_npth) else {
        return vec![];
    };
    unplated_holes(project)
        .into_iter()
        .filter(|d| d.radius * 2.0 + SLACK_MM < min)
        .map(|d| Finding {
            code: "min_npth",
            severity: project.rule_severity("min_npth", "error"),
            message: format!(
                "{} is {:.3} mm across; the smallest unplated hole the fab drills is {:.3} mm",
                d.label,
                d.radius * 2.0,
                min
            ),
            objects: vec![d.owner],
            hint: Some("Drill a larger hole or move to a fab that drills smaller ones.".into()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use pcbcraft_doc::{FabRules, Hole, Net, Track, Via};

    fn project(rules: FabRules) -> Project {
        let mut p = Project::default();
        for id in 1..=2 {
            p.nets.push(Net {
                id,
                name: format!("n{id}"),
                ..Default::default()
            });
        }
        p.board.rules = Some(rules);
        p
    }

    fn via(id: Id, net: Id, at: Point) -> Via {
        Via {
            id,
            net,
            position: at,
            diameter: 0.8,
            drill: 0.4,
            preserved: pcbcraft_doc::PreservedSource::default(),
            from_layer: None,
            to_layer: None,
            covering: None,
            covering_bottom: None,
            backdrill: None,
        }
    }

    fn track(id: Id, net: Id, y: f64) -> Track {
        Track {
            id,
            net,
            start: Point::new(-5.0, y),
            end: Point::new(5.0, y),
            width: 0.2,
            layer: Layer::Front,
            arc: None,
            preserved: pcbcraft_doc::PreservedSource::default(),
        }
    }

    fn codes(p: &Project, code: &str) -> usize {
        drc(p).iter().filter(|f| f.code == code).count()
    }

    // Via hole wall at radius 0.2. A 0.2 mm wide track at y = 1.0 spans 0.9 to
    // 1.1, so the wall to copper gap is 0.9 - 0.2 = 0.7 mm. Hand computed.
    #[test]
    fn a_via_hole_closer_than_the_limit_to_foreign_copper_fires() {
        let mut p = project(FabRules {
            hole_to_copper_via: Some(0.8),
            ..Default::default()
        });
        p.board.vias.push(via(10, 1, Point::new(0.0, 0.0)));
        p.board.tracks.push(track(11, 2, 1.0));
        assert_eq!(codes(&p, "hole_to_copper_via"), 1);
    }

    #[test]
    fn a_via_hole_at_the_limit_or_beside_its_own_net_is_quiet() {
        let mut p = project(FabRules {
            hole_to_copper_via: Some(0.7),
            ..Default::default()
        });
        p.board.vias.push(via(10, 1, Point::new(0.0, 0.0)));
        p.board.tracks.push(track(11, 2, 1.0));
        assert_eq!(codes(&p, "hole_to_copper_via"), 0, "0.7 mm gap meets 0.7");
        p.board.rules = Some(FabRules {
            hole_to_copper_via: Some(0.8),
            ..Default::default()
        });
        p.board.tracks[0].net = 1;
        assert_eq!(codes(&p, "hole_to_copper_via"), 0, "own net is the land");
    }

    #[test]
    fn without_a_limit_no_hole_to_copper_finding_appears() {
        let mut p = project(FabRules::default());
        p.board.vias.push(via(10, 1, Point::new(0.0, 0.0)));
        p.board.tracks.push(track(11, 2, 0.6));
        assert_eq!(codes(&p, "hole_to_copper_via"), 0);
    }

    // Mounting hole 3.2 mm across (wall at 1.6). Track edge at 2.0 - 0.1 = 1.9,
    // so the gap is 0.3 mm on every copper layer it sits on.
    #[test]
    fn a_mounting_hole_near_any_copper_fires_and_far_copper_does_not() {
        let mut p = project(FabRules {
            hole_to_copper_npth: Some(0.4),
            ..Default::default()
        });
        p.board.holes.push(Hole {
            id: 20,
            position: Point::new(0.0, 0.0),
            diameter: 3.2,
            end: None,
        });
        p.board.tracks.push(track(21, 1, 2.0));
        assert_eq!(codes(&p, "hole_to_copper_npth"), 1);
        p.board.tracks[0].start = Point::new(-5.0, 2.5);
        p.board.tracks[0].end = Point::new(5.0, 2.5);
        assert_eq!(codes(&p, "hole_to_copper_npth"), 0, "0.8 mm gap is enough");
    }

    #[test]
    fn a_small_unplated_hole_fires_and_a_big_one_does_not() {
        let mut p = project(FabRules {
            min_npth: Some(1.0),
            ..Default::default()
        });
        p.board.holes.push(Hole {
            id: 30,
            position: Point::new(0.0, 0.0),
            diameter: 0.8,
            end: None,
        });
        assert_eq!(codes(&p, "min_npth"), 1);
        p.board.holes[0].diameter = 1.0;
        assert_eq!(codes(&p, "min_npth"), 0, "1.0 mm meets the 1.0 mm limit");
    }
}
