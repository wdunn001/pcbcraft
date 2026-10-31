//! Drill overlap check: two holes that touch or cut into each other.
//!
//! Context: called once from `drc` in `lib.rs`, next to the fabrication
//! checks, and not gated on `Board.rules`: overlapping drills are never
//! manufacturable. Spacing against the preset limits (`hole_to_hole_via`,
//! `hole_to_hole_pad`) is a separate check and is not repeated here.
//!
//! Local rules:
//! - Every drill is a capsule: vias, pad drills (round or oval, plated or
//!   not) and mounting holes or slots.
//! - A pad is walked once, so its front and back copper never count as two
//!   holes; two pads of one footprint are compared with each other.
//! - A via with no drill is an unplated blind hole and is skipped.
//!
//! Dependencies: `crates/doc/src/lib.rs` (`Pad::drill_capsule`, `Hole`, `Via`),
//! `crates/geom/src/lib.rs` (`segment_distance`).
use super::Finding;
use pcbcraft_doc::{FabRules, Id, Project};
use pcbcraft_geom::{Point, segment_distance};

/// A gap below this many millimetres counts as touching or overlapping.
const OVERLAP_MM: f64 = -1e-6;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrillKind {
    Via,
    Pad,
    MountingHole,
}

struct Drill {
    kind: DrillKind,
    owner: Id,
    label: String,
    start: Point,
    end: Point,
    radius: f64,
}

fn drills(project: &Project) -> Vec<Drill> {
    let mut all = vec![];
    for v in project.board.vias.iter().filter(|v| v.drill > 0.0) {
        all.push(Drill {
            kind: DrillKind::Via,
            owner: v.id,
            label: format!("via {}", v.id),
            start: v.position,
            end: v.position,
            radius: v.drill / 2.0,
        });
    }
    for h in &project.board.holes {
        all.push(Drill {
            kind: DrillKind::MountingHole,
            owner: h.id,
            label: format!("hole {}", h.id),
            start: h.start(),
            end: h.finish(),
            radius: h.diameter / 2.0,
        });
    }
    for f in &project.board.footprints {
        let reference = project
            .component(f.component)
            .map_or_else(|| format!("footprint {}", f.id), |c| c.reference.clone());
        for pad in &f.pads {
            let Some(d) = pad.drill_capsule() else {
                continue;
            };
            let world = |p: Point| f.position.offset(p.rotate(-f.rotation));
            all.push(Drill {
                kind: DrillKind::Pad,
                owner: f.id,
                label: format!("pad {} of {reference}", pad.number),
                start: world(d.start),
                end: world(d.end),
                radius: d.radius,
            });
        }
    }
    all
}

/// One `drill_overlap` error per pair of drills whose walls cut into each
/// other.
///
/// # Rules
/// Severity is `project.rule_severity("drill_overlap", "error")`. The message
/// names both drills and the depth of the overlap in millimetres.
///
/// # Dependencies
/// `crates/doc/src/lib.rs` (`Pad::drill_capsule`).
///
/// # Do not
/// Do not report a gap above zero; that is the spacing check.
pub(crate) fn drill_findings(project: &Project) -> Vec<Finding> {
    let all = drills(project);
    let mut result = vec![];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            let gap = segment_distance(a.start, a.end, b.start, b.end) - a.radius - b.radius;
            if gap >= OVERLAP_MM {
                continue;
            }
            let mut objects = vec![a.owner];
            if b.owner != a.owner {
                objects.push(b.owner);
            }
            result.push(Finding {
                code: "drill_overlap",
                severity: project.rule_severity("drill_overlap", "error"),
                message: format!(
                    "{} and {} overlap by {:.3} mm; the holes cut into each other",
                    a.label, b.label, -gap
                ),
                objects,
                hint: None,
            });
        }
    }
    result
}

fn tighter_hole_to_hole(
    left: Option<(f64, &'static str)>,
    right: Option<(f64, &'static str)>,
) -> Option<(f64, &'static str)> {
    match (left, right) {
        (None, chosen) | (chosen, None) => chosen,
        (Some(l), Some(r)) => {
            if l.0 <= r.0 {
                Some(l)
            } else {
                Some(r)
            }
        }
    }
}

fn hole_to_hole_limit(rules: &FabRules, a: DrillKind, b: DrillKind) -> Option<(f64, &'static str)> {
    let via = rules.hole_to_hole_via.map(|min| (min, "hole_to_hole_via"));
    let pad = rules.hole_to_hole_pad.map(|min| (min, "hole_to_hole_pad"));
    match (a, b) {
        (DrillKind::Via, DrillKind::Via) => via,
        (DrillKind::Pad, _) | (_, DrillKind::Pad) => pad,
        (DrillKind::Via, DrillKind::MountingHole) | (DrillKind::MountingHole, DrillKind::Via) => {
            tighter_hole_to_hole(via, pad)
        }
        (DrillKind::MountingHole, DrillKind::MountingHole) => pad,
    }
}

/// Wall-to-wall spacing against `FabRules.hole_to_hole_via` and
/// `hole_to_hole_pad` when `Board.rules` is set.
///
/// Overlapping drills are left to `drill_findings`; pairs that already touch
/// or cut in are skipped here.
pub(crate) fn drill_spacing_findings(project: &Project) -> Vec<Finding> {
    let Some(rules) = project.board.rules.as_ref() else {
        return vec![];
    };
    if rules.hole_to_hole_via.is_none() && rules.hole_to_hole_pad.is_none() {
        return vec![];
    }
    let all = drills(project);
    let mut result = vec![];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            let Some((min, code)) = hole_to_hole_limit(rules, a.kind, b.kind) else {
                continue;
            };
            let gap = segment_distance(a.start, a.end, b.start, b.end) - a.radius - b.radius;
            if gap + 1e-6 < OVERLAP_MM {
                continue;
            }
            if gap + 1e-6 >= min {
                continue;
            }
            let mut objects = vec![a.owner];
            if b.owner != a.owner {
                objects.push(b.owner);
            }
            result.push(Finding {
                code,
                severity: project.rule_severity(code, "error"),
                message: format!(
                    "{} and {} are {:.3} mm apart wall to wall; fabrication needs at least {:.3} mm",
                    a.label, b.label, gap.max(0.0), min
                ),
                objects,
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use pcbcraft_doc::{Footprint, Hole, Pad, Via, component_template};
    use pcbcraft_library::{Drill as LibDrill, FootprintPad};

    fn pad(number: &str, offset: Point, size: Point, drill: Point, drill_offset: Point) -> Pad {
        Pad {
            number: number.into(),
            offset,
            diameter: size.x,
            net: None,
            castellated: None,
            geometry: Some(FootprintPad {
                number: number.into(),
                pad_type: "thru_hole".into(),
                shape: "rect".into(),
                position: offset,
                rotation: 0.0,
                size,
                layers: vec!["*.Cu".into()],
                drill: Some(LibDrill {
                    size: drill,
                    offset: drill_offset,
                }),
                roundrect_ratio: None,
                chamfer: None,
                polygon: vec![],
                raw_source: String::new(),
            }),
        }
    }
    fn board_with(pads: Vec<Pad>) -> Project {
        let mut p = Project::default();
        if let Some(c) = component_template("resistor", 2, "R2".into(), Point::new(10.0, 10.0)) {
            p.components.push(c);
        }
        p.board.footprints.push(Footprint {
            preserved: pcbcraft_doc::PreservedSource::default(),
            id: 12,
            component: 2,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads,
        });
        p
    }
    fn via(id: Id, x: f64, y: f64, drill: f64) -> Via {
        Via {
            preserved: pcbcraft_doc::PreservedSource::default(),
            id,
            net: 1,
            position: Point::new(x, y),
            diameter: 0.8,
            drill,
            from_layer: None,
            to_layer: None,
            covering: None,
covering_scope: None,
            covering_bottom: None,
            backdrill: None,
        }
    }
    fn overlaps(p: &Project) -> Vec<Finding> {
        drc(p)
            .into_iter()
            .filter(|f| f.code == "drill_overlap")
            .collect()
    }

    fn spacing(p: &Project, code: &str) -> Vec<Finding> {
        drc(p).into_iter().filter(|f| f.code == code).collect()
    }

    #[test]
    fn via_pair_below_hole_to_hole_via_is_reported() {
        let mut p = Project::default();
        p.board.vias.push(via(5, 1.0, 1.0, 0.4));
        p.board.vias.push(via(6, 1.5, 1.0, 0.4));
        p.board.rules = Some(FabRules {
            hole_to_hole_via: Some(0.3),
            ..FabRules::default()
        });
        let f = spacing(&p, "hole_to_hole_via");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].objects, vec![5, 6]);
        assert!(
            f[0].message.contains("0.100 mm") && f[0].message.contains("0.300 mm"),
            "{}",
            f[0].message
        );
        p.board.vias[1].position = Point::new(1.9, 1.0);
        assert!(spacing(&p, "hole_to_hole_via").is_empty());
    }

    #[test]
    fn pad_drills_use_hole_to_hole_pad_not_via_limit() {
        let mut p = board_with(vec![
            pad(
                "1",
                Point::default(),
                Point::new(1.6, 1.6),
                Point::new(0.8, 0.8),
                Point::default(),
            ),
            pad(
                "2",
                Point::new(1.0, 0.0),
                Point::new(1.6, 1.6),
                Point::new(0.8, 0.8),
                Point::default(),
            ),
        ]);
        p.board.rules = Some(FabRules {
            hole_to_hole_via: Some(0.05),
            hole_to_hole_pad: Some(0.3),
            ..FabRules::default()
        });
        let f = spacing(&p, "hole_to_hole_pad");
        assert_eq!(f.len(), 1, "0.2 mm wall gap is under 0.3 mm pad limit");
        assert!(spacing(&p, "hole_to_hole_via").is_empty());
    }

    #[test]
    fn hole_to_hole_limits_are_skipped_without_board_rules() {
        let mut p = Project::default();
        p.board.vias.push(via(5, 1.0, 1.0, 0.4));
        p.board.vias.push(via(6, 1.5, 1.0, 0.4));
        assert!(spacing(&p, "hole_to_hole_via").is_empty());
    }

    #[test]
    fn overlapping_drills_stay_drill_overlap_when_fab_rules_set() {
        let mut p = Project::default();
        p.board.vias.push(via(5, 1.0, 1.0, 0.4));
        p.board.vias.push(via(6, 1.3, 1.0, 0.4));
        p.board.rules = Some(FabRules {
            hole_to_hole_via: Some(0.3),
            ..FabRules::default()
        });
        let findings = drc(&p);
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.code == "drill_overlap")
                .count(),
            1
        );
        assert!(findings.iter().all(|f| f.code != "hole_to_hole_via"));
    }

    #[test]
    fn overlapping_drills_are_an_error() {
        let mut p = Project::default();
        p.board.vias.push(via(5, 1.0, 1.0, 0.4));
        p.board.vias.push(via(6, 1.3, 1.0, 0.4));
        let f = overlaps(&p);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, "error");
        assert_eq!(f[0].objects, vec![5, 6]);
        assert!(
            f[0].message.contains("overlap by 0.100 mm"),
            "{}",
            f[0].message
        );
        p.board.vias[1].position = Point::new(1.5, 1.0);
        assert!(
            overlaps(&p).is_empty(),
            "0.1 mm of wall is spacing, not overlap"
        );
    }
    #[test]
    fn via_and_mounting_slot_overlap_is_reported() {
        let mut p = Project::default();
        p.board.vias.push(via(5, 3.0, 1.0, 0.4));
        p.board.holes.push(Hole {
            id: 7,
            position: Point::new(1.0, 1.0),
            diameter: 1.0,
            end: Some(Point::new(5.0, 1.0)),
        });
        let f = overlaps(&p);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].objects, vec![5, 7]);
    }
    #[test]
    fn plated_pad_is_not_spaced_against_itself_across_sides() {
        let p = board_with(vec![pad(
            "1",
            Point::default(),
            Point::new(1.6, 1.6),
            Point::new(0.8, 0.8),
            Point::default(),
        )]);
        assert!(overlaps(&p).is_empty());
    }
    #[test]
    fn two_pads_of_one_footprint_with_cutting_drills_report_once() {
        let p = board_with(vec![
            pad(
                "1",
                Point::default(),
                Point::new(1.6, 1.6),
                Point::new(0.8, 0.8),
                Point::default(),
            ),
            pad(
                "2",
                Point::new(0.6, 0.0),
                Point::new(1.6, 1.6),
                Point::new(0.8, 0.8),
                Point::default(),
            ),
        ]);
        let f = overlaps(&p);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].objects, vec![12]);
        assert!(
            f[0].message.contains("overlap by 0.200 mm"),
            "{}",
            f[0].message
        );
    }
    #[test]
    fn via_with_no_drill_is_skipped() {
        let mut p = Project::default();
        p.board.vias.push(via(5, 1.0, 1.0, 0.0));
        p.board.vias.push(via(6, 1.0, 1.0, 0.4));
        assert!(overlaps(&p).is_empty());
    }
    #[test]
    fn centred_drill_ring_is_unchanged() {
        // Round 1.6 mm pad, 0.8 mm drill: the ring is 0.4 mm either way.
        let mut round = pad(
            "1",
            Point::default(),
            Point::new(1.6, 1.6),
            Point::new(0.8, 0.8),
            Point::default(),
        );
        if let Some(g) = round.geometry.as_mut() {
            g.shape = "circle".into();
        }
        assert!((round.min_annular_ring(0.0).unwrap_or(-1.0) - 0.4).abs() < 1e-9);
        assert!((round.min_annular_ring(0.1).unwrap_or(-1.0) - 0.35).abs() < 1e-9);
        let square = pad(
            "1",
            Point::default(),
            Point::new(1.6, 1.6),
            Point::new(0.8, 0.8),
            Point::default(),
        );
        assert!((square.min_annular_ring(0.0).unwrap_or(-1.0) - 0.4).abs() < 1e-9);
    }
    #[test]
    fn offset_drill_annular_ring_measures_the_thin_side() {
        // A 2.0 by 1.0 pad with a 0.4 mm drill shifted 0.3 mm along x: the
        // thin side is the top and bottom edge at 0.5 - 0.2 = 0.3 mm, and the
        // right edge leaves 1.0 - 0.3 - 0.2 = 0.5 mm. The old measure said 0.8.
        let shifted = pad(
            "1",
            Point::default(),
            Point::new(2.0, 1.0),
            Point::new(0.4, 0.4),
            Point::new(0.3, 0.0),
        );
        assert!((shifted.min_annular_ring(0.0).unwrap_or(-1.0) - 0.3).abs() < 1e-9);
        // Moved 0.4 mm along y, the top edge is 0.5 - 0.4 - 0.2 = -0.1 away.
        let high = pad(
            "1",
            Point::default(),
            Point::new(2.0, 1.0),
            Point::new(0.4, 0.4),
            Point::new(0.0, 0.4),
        );
        assert!((high.min_annular_ring(0.0).unwrap_or(1.0) + 0.1).abs() < 1e-9);
    }
    #[test]
    fn offset_drill_ring_below_the_limit_is_reported_by_drc() {
        let mut p = board_with(vec![pad(
            "1",
            Point::default(),
            Point::new(2.0, 1.0),
            Point::new(0.4, 0.4),
            Point::new(0.0, 0.2),
        )]);
        p.board.rules = Some(FabRules {
            min_ring_pth: Some(0.2),
            ..FabRules::default()
        });
        let f: Vec<_> = drc(&p)
            .into_iter()
            .filter(|f| f.code == "annular_ring")
            .collect();
        assert_eq!(f.len(), 1, "ring 0.1 mm is under 0.2 mm");
        assert!(f[0].message.contains("0.100 mm"), "{}", f[0].message);
    }
}
