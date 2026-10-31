//! Stub length check: dead-end copper branches on classified nets.
//!
//! Context: called once from `drc` in `lib.rs` with the same copper snapshot the
//! clearance and airwire checks use. Works on `Track` records, not on the
//! sampled `Copper` capsules, so a curved track counts once with its true
//! `Track::length()`.
//!
//! Local rules:
//! - Scope is one net on one copper layer, for nets in a class with `max_stub`.
//! - An open stub is a chain of tracks that starts at a track end touching
//!   nothing and runs to the first junction, pad, via or zone.
//! - A test-point branch is a chain that ends on a pad and leaves a junction.
//!   At a junction the two longest pad-ended or continuing arms are the main
//!   run; every other pad-ended arm is a stub. Three equal arms on three pads
//!   therefore report one of them.
//! - A limit of zero allows `ZERO_LIMIT_SLACK_MM` of rounding.
//! - Via barrel stubs are checked in `backdrill_findings` below, only for vias with a back-drill setting; a via without one is not measured.
//!
//! Dependencies: `super::Copper`, `super::touching`, `super::inside` in
//! `crates/rules/src/lib.rs`; `Track::length` in `crates/doc/src/lib.rs`.
use super::{Copper, Finding, inside, touching};
use pcbcraft_doc::{Id, Layer, Project, Track};
use pcbcraft_geom::Point;
use std::collections::BTreeSet;

/// Rounding slack applied when a class allows no stubs at all, in millimetres.
const ZERO_LIMIT_SLACK_MM: f64 = 0.05;
/// Track ends closer than this are the same node, in millimetres.
const NODE_MERGE_MM: f64 = 1e-6;
/// Longest chain walked, a guard against a malformed loop.
const WALK_CAP: usize = 100_000;

/// How a node of the track graph meets other copper.
#[derive(Clone, Copy, PartialEq)]
enum Anchor {
    /// Touches nothing besides the tracks that end here.
    Free,
    /// Touches a pad, via or filled zone of the same net and layer.
    Pad,
    /// Lands on the body of a track that does not end here.
    Body,
}

struct Node {
    at: Point,
    anchor: Anchor,
    /// Indices into the layer's track list that end at this node.
    tracks: Vec<usize>,
}

/// What a chain of tracks leads to, seen from its far end.
enum Arm {
    /// Ends on a pad with nothing else attached.
    PadLeaf(f64, Vec<Id>),
    /// Ends on open copper.
    Open,
    /// Reaches another junction, pad or via with more copper beyond.
    Through,
}

fn exceeds(length: f64, limit: f64) -> bool {
    if limit <= 0.0 {
        length > ZERO_LIMIT_SLACK_MM + 1e-9
    } else {
        length > limit + 1e-9
    }
}

fn node_index(nodes: &mut Vec<Node>, at: Point) -> usize {
    if let Some(i) = nodes
        .iter()
        .position(|n| n.at.distance(at) <= NODE_MERGE_MM)
    {
        return i;
    }
    nodes.push(Node {
        at,
        anchor: Anchor::Free,
        tracks: vec![],
    });
    nodes.len() - 1
}

/// Classify what a node touches, ignoring the tracks that end there.
fn anchor_of(items: &[Copper], net: Id, layer: Layer, at: Point, own: &BTreeSet<Id>) -> Anchor {
    let probe = Copper {
        id: 0,
        net,
        start: at,
        end: at,
        radius: 0.05,
        contact_radius: 0.05,
        conservative: false,
        outline: None,
        outline_error: 0.0,
        layer,
        pad: false,
        clearance: None,
        bridge: None,
        hole: None,
        cutouts: vec![],
    };
    let mut body = false;
    for item in items {
        if item.net != net || item.layer != layer || own.contains(&item.id) {
            continue;
        }
        if item.pad && touching(&probe, item) {
            return Anchor::Pad;
        }
        if let Some(outline) = &item.outline {
            if inside(at, outline) {
                return Anchor::Pad;
            }
        } else if !item.pad && touching(&probe, item) {
            body = true;
        }
    }
    if body { Anchor::Body } else { Anchor::Free }
}

/// A node where a branch can end: more than two tracks, or any anchor.
fn is_junction(node: &Node) -> bool {
    node.tracks.len() >= 3 || node.anchor != Anchor::Free
}

/// Follow the chain that starts with `first` leaving `from`. Returns the
/// tracks walked, their summed length and the node where the walk stopped.
fn walk(
    tracks: &[&Track],
    nodes: &[Node],
    ends: &[(usize, usize)],
    from: usize,
    first: usize,
) -> (Vec<Id>, f64, usize) {
    let mut ids = vec![];
    let mut length = 0.0;
    let mut at = from;
    let mut via = first;
    for _ in 0..WALK_CAP {
        let (Some(track), Some((a, b))) = (tracks.get(via), ends.get(via).copied()) else {
            break;
        };
        ids.push(track.id);
        length += track.length();
        let next = if at == a { b } else { a };
        at = next;
        let Some(node) = nodes.get(next) else {
            break;
        };
        if is_junction(node) || node.tracks.len() != 2 {
            break;
        }
        match node.tracks.iter().copied().find(|t| *t != via) {
            Some(t) => via = t,
            None => break,
        }
    }
    (ids, length, at)
}

/// Stub findings for every net in a class that sets `max_stub`.
///
/// Runs per net and per copper layer. Findings carry the stub's track ids and
/// the message "N mm stub on NET, at most M mm allowed".
///
/// # Rules
/// Severity is `project.rule_severity("stub_length", "error")`. Nets with no
/// class, or a class without `max_stub`, are never reported.
///
/// # Dependencies
/// `crates/rules/src/lib.rs` (`Copper`, `touching`), `crates/doc/src/lib.rs`
/// (`NetClass::max_stub`, `Track::length`).
///
/// # Do not
/// Do not reuse the code `track_length`; it reports total routed length.
pub(crate) fn stub_length_findings(project: &Project, items: &[Copper]) -> Vec<Finding> {
    let mut result = vec![];
    let mut reported: BTreeSet<Vec<Id>> = BTreeSet::new();
    for class in &project.net_classes {
        let Some(limit) = class.max_stub else {
            continue;
        };
        for net in &class.nets {
            for layer in project.copper_layers() {
                let tracks: Vec<&Track> = project
                    .board
                    .tracks
                    .iter()
                    .filter(|t| t.net == *net && t.layer == layer)
                    .collect();
                for (ids, length) in layer_stubs(items, *net, layer, &tracks) {
                    if !exceeds(length, limit) || !reported.insert(ids.clone()) {
                        continue;
                    }
                    result.push(Finding {
                        code: "stub_length",
                        severity: project.rule_severity("stub_length", "error"),
                        message: format!(
                            "{length:.3} mm stub on {}, at most {limit:.3} mm allowed",
                            project.net_name(*net),
                        ),
                        objects: ids,
                        hint: None,
                    });
                }
            }
        }
    }
    result
}

/// Every stub candidate on one net and layer as (track ids, length).
fn layer_stubs(items: &[Copper], net: Id, layer: Layer, tracks: &[&Track]) -> Vec<(Vec<Id>, f64)> {
    let mut nodes: Vec<Node> = vec![];
    let mut ends = vec![];
    for (i, track) in tracks.iter().enumerate() {
        let a = node_index(&mut nodes, track.start);
        let b = node_index(&mut nodes, track.end);
        ends.push((a, b));
        if let Some(n) = nodes.get_mut(a) {
            n.tracks.push(i);
        }
        if let (true, Some(n)) = (a != b, nodes.get_mut(b)) {
            n.tracks.push(i);
        }
    }
    for node in &mut nodes {
        let own: BTreeSet<Id> = node
            .tracks
            .iter()
            .filter_map(|i| tracks.get(*i).map(|t| t.id))
            .collect();
        node.anchor = anchor_of(items, net, layer, node.at, &own);
    }
    let arm_from = |from: usize, first: usize| -> Arm {
        let (ids, length, end) = walk(tracks, &nodes, &ends, from, first);
        match nodes.get(end) {
            Some(n) if n.tracks.len() == 1 && n.anchor == Anchor::Free => Arm::Open,
            Some(n) if n.tracks.len() == 1 && n.anchor == Anchor::Pad => Arm::PadLeaf(length, ids),
            _ => Arm::Through,
        }
    };
    let mut found = vec![];
    // Open stubs: start at every free dead end and walk toward the copper.
    for (idx, node) in nodes.iter().enumerate() {
        if let (1, Anchor::Free, Some(first)) =
            (node.tracks.len(), node.anchor, node.tracks.first().copied())
        {
            let (mut ids, length, _) = walk(tracks, &nodes, &ends, idx, first);
            ids.sort_unstable();
            found.push((ids, length));
        }
    }
    // Test-point branches: at each free junction, all but the two longest
    // non-open arms that end on a pad.
    for (idx, node) in nodes.iter().enumerate() {
        if node.tracks.len() < 3 || node.anchor != Anchor::Free {
            continue;
        }
        let mut pad_arms: Vec<(f64, Vec<Id>)> = vec![];
        let mut through = 0usize;
        for first in &node.tracks {
            match arm_from(idx, *first) {
                Arm::PadLeaf(length, ids) => pad_arms.push((length, ids)),
                Arm::Through => through += 1,
                Arm::Open => {}
            }
        }
        pad_arms.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let main_slots = 2usize.saturating_sub(through);
        for (length, mut ids) in pad_arms.into_iter().skip(main_slots) {
            ids.sort_unstable();
            found.push((ids, length));
        }
    }
    found
}

/// Slack when comparing a barrel stub with its limit, in millimetres.
const STUB_SLACK_MM: f64 = 1e-9;

/// Back-drill checks for via barrels: a remaining stub longer than the
/// setting allows, and a back-drill that cuts something it must not.
///
/// # Rules
/// - `backdrill_stub_too_long`: the stub from the stackup records exceeds `max_stub`; the message gives the stub and the limit in millimetres. A stackup with no rows for the span reports nothing.
/// - `backdrill_must_not_cut`: the setting does not fit its via (must-not-cut layer outside the span or not below the start layer, drill not larger than the hole, no hole), or a track of the via's net ends on the via on a layer the drill removes.
///
/// # Dependencies
/// - crates/doc/src/backdrill.rs `Project::effective_backdrill`, `validate_backdrill`, `backdrilled_layers`, `backdrill_stub`.
///
/// # Do not
/// - Report a via without a back-drill setting.
/// - Reuse the code `stub_too_long`; that one is for track branches.
pub(crate) fn backdrill_findings(project: &Project) -> Vec<Finding> {
    let mut result = vec![];
    for via in &project.board.vias {
        let Some(spec) = project.effective_backdrill(via) else {
            continue;
        };
        let at = format!("Via at {:.2}, {:.2}", via.position.x, via.position.y);
        if let Err(why) = project.validate_backdrill(via, &spec) {
            result.push(Finding {
                code: "backdrill_must_not_cut",
                severity: project.rule_severity("backdrill_must_not_cut", "error"),
                message: format!("{at} has a back-drill that does not fit it: {why}"),
                objects: vec![via.id],
            });
            continue;
        }
        let gone = project.backdrilled_layers(via, &spec);
        let used: Vec<Id> = project
            .board
            .tracks
            .iter()
            .filter(|t| t.net == via.net && gone.contains(&t.layer))
            .filter(|t| {
                [t.start, t.end]
                    .iter()
                    .any(|e| e.distance(via.position) <= via.diameter / 2.0 + 1e-6)
            })
            .map(|t| t.id)
            .collect();
        if !used.is_empty() {
            let mut objects = vec![via.id];
            objects.extend(used);
            result.push(Finding {
                code: "backdrill_must_not_cut",
                severity: project.rule_severity("backdrill_must_not_cut", "error"),
                message: format!(
                    "{at} is back-drilled through a layer that a track of its own net connects on"
                ),
                objects,
            });
        }
        if let Some(stub) = project.backdrill_stub(via, &spec)
            && stub > spec.max_stub + STUB_SLACK_MM
        {
            result.push(Finding {
                code: "backdrill_stub_too_long",
                severity: project.rule_severity("backdrill_stub_too_long", "warning"),
                message: format!(
                    "{at} leaves a {stub:.3} mm stub after back-drilling; the limit is {:.3} mm",
                    spec.max_stub
                ),
                objects: vec![via.id],
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::exceeds;
    use pcbcraft_doc::{Footprint, Net, NetClass, Pad, Track, component_template};

    fn pad_at(p: &mut Project, id: Id, at: Point) {
        if let Some(mut c) = component_template("resistor", id, format!("R{id}"), at) {
            for pin in &mut c.pins {
                pin.net = Some(1);
            }
            p.components.push(c);
        }
        p.board.footprints.push(Footprint {
            preserved: pcbcraft_doc::PreservedSource::default(),
            id: id + 10,
            component: id,
            position: at,
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 1.0,
                net: Some(1),
                castellated: None,
                geometry: None,
            }],
            preserved: pcbcraft_doc::PreservedSource::default(),
        });
    }
    fn track(p: &mut Project, id: Id, a: Point, b: Point) {
        p.board.tracks.push(Track {
            preserved: pcbcraft_doc::PreservedSource::default(),
            id,
            net: 1,
            start: a,
            end: b,
            width: 0.25,
            layer: Layer::Front,
            arc: None,
            preserved: pcbcraft_doc::PreservedSource::default(),
        });
    }
    /// A 20 mm run from a pad at x=10 to a pad at x=30 with a junction at x=20.
    fn run() -> Project {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "signal".into(),
            ..Default::default()
        });
        pad_at(&mut p, 2, Point::new(10.0, 10.0));
        pad_at(&mut p, 3, Point::new(30.0, 10.0));
        track(&mut p, 100, Point::new(10.0, 10.0), Point::new(20.0, 10.0));
        track(&mut p, 101, Point::new(20.0, 10.0), Point::new(30.0, 10.0));
        p
    }
    fn classify(p: &mut Project, max_stub: Option<f64>) -> Result<(), String> {
        p.net_classes.push(NetClass {
            id: 50,
            name: "STRICT".into(),
            clearance: 0.2,
            width: 0.25,
            min_width: None,
            max_width: None,
            target_length: None,
            length_tolerance: None,
            max_stub,
            max_vias: None,
            via_diameter: None,
            via_drill: None,
            pair_width: None,
            pair_gap: None,
            target_impedance: None,
            nets: vec![1],
            profile: None,
            impedance_target: None,
            impedance_tolerance_pct: None,
            line_structure: None,
            reference_layers: vec![],
            current: None,
            temperature_rise: None,
            working_voltage: None,
            insulation: None,
            return_path_rule: None,
        });
        p.validate()
    }
    fn stubs(p: &Project) -> Vec<Finding> {
        drc(p)
            .into_iter()
            .filter(|f| f.code == "stub_length")
            .collect()
    }

    #[test]
    fn open_stub_longer_than_class_max_reports_stub_length_and_names_the_track()
    -> Result<(), String> {
        let mut p = run();
        track(&mut p, 102, Point::new(20.0, 10.0), Point::new(20.0, 13.0));
        classify(&mut p, Some(1.0))?;
        let found = stubs(&p);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].objects, vec![102]);
        assert_eq!(
            found[0].message,
            "3.000 mm stub on signal, at most 1.000 mm allowed"
        );
        Ok(())
    }
    #[test]
    fn removing_stub_track_clears_stub_length_finding() -> Result<(), String> {
        let mut p = run();
        track(&mut p, 102, Point::new(20.0, 10.0), Point::new(20.0, 13.0));
        classify(&mut p, Some(1.0))?;
        assert_eq!(stubs(&p).len(), 1);
        p.board.tracks.retain(|t| t.id != 102);
        p.validate()?;
        assert!(stubs(&p).is_empty());
        Ok(())
    }
    #[test]
    fn test_point_branch_over_limit_reports_stub_length() -> Result<(), String> {
        let mut p = run();
        pad_at(&mut p, 4, Point::new(20.0, 13.0));
        track(&mut p, 102, Point::new(20.0, 10.0), Point::new(20.0, 13.0));
        classify(&mut p, Some(1.0))?;
        let found = stubs(&p);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].objects, vec![102]);
        assert!(found[0].message.starts_with("3.000 mm stub"));
        Ok(())
    }
    #[test]
    fn test_point_branch_under_limit_is_quiet() -> Result<(), String> {
        let mut p = run();
        pad_at(&mut p, 4, Point::new(20.0, 10.5));
        track(&mut p, 102, Point::new(20.0, 10.0), Point::new(20.0, 10.5));
        classify(&mut p, Some(1.0))?;
        assert!(stubs(&p).is_empty());
        Ok(())
    }
    #[test]
    fn max_stub_zero_flags_two_tenth_mm_stub() -> Result<(), String> {
        let mut p = run();
        track(&mut p, 102, Point::new(20.0, 10.0), Point::new(20.0, 10.2));
        classify(&mut p, Some(0.0))?;
        let found = stubs(&p);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].objects, vec![102]);
        Ok(())
    }
    #[test]
    fn max_stub_zero_quiet_when_every_track_end_meets_pad_or_via() -> Result<(), String> {
        let mut p = run();
        classify(&mut p, Some(0.0))?;
        assert!(stubs(&p).is_empty());
        Ok(())
    }
    #[test]
    fn zero_limit_slack_absorbs_rounding_only() {
        assert!(!exceeds(0.04, 0.0));
        assert!(exceeds(0.06, 0.0));
        assert!(!exceeds(1.0, 1.0));
        assert!(exceeds(1.001, 1.0));
    }
    #[test]
    fn unclassified_net_never_reports_stub_length() -> Result<(), String> {
        let mut p = run();
        track(&mut p, 102, Point::new(20.0, 10.0), Point::new(20.0, 13.0));
        p.validate()?;
        assert!(stubs(&p).is_empty());
        Ok(())
    }
    #[test]
    fn class_without_max_stub_never_reports_stub_length() -> Result<(), String> {
        let mut p = run();
        track(&mut p, 102, Point::new(20.0, 10.0), Point::new(20.0, 13.0));
        classify(&mut p, None)?;
        assert!(stubs(&p).is_empty());
        Ok(())
    }
    #[test]
    fn stub_hanging_off_a_track_body_is_measured_from_the_body() -> Result<(), String> {
        let mut p = run();
        // Branch starts mid-way along track 100, at x=15, with no node there.
        track(&mut p, 102, Point::new(15.0, 10.0), Point::new(15.0, 12.0));
        classify(&mut p, Some(1.0))?;
        let found = stubs(&p);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].objects, vec![102]);
        assert!(found[0].message.starts_with("2.000 mm stub"));
        Ok(())
    }
    #[test]
    fn two_track_stub_sums_both_lengths() -> Result<(), String> {
        let mut p = run();
        track(&mut p, 102, Point::new(20.0, 10.0), Point::new(20.0, 12.0));
        track(&mut p, 103, Point::new(20.0, 12.0), Point::new(21.5, 12.0));
        classify(&mut p, Some(4.0))?;
        assert!(stubs(&p).is_empty());
        p.net_classes[0].max_stub = Some(3.4);
        let found = stubs(&p);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].objects, vec![102, 103]);
        assert!(found[0].message.starts_with("3.500 mm stub"));
        Ok(())
    }
    #[test]
    fn shared_copper_snapshot_keeps_airwire_results_unchanged() -> Result<(), String> {
        let mut p = run();
        p.board.tracks.retain(|t| t.id != 101);
        let direct = airwires(&p);
        let shared = airwires_from(&copper(&p));
        assert_eq!(direct.len(), 1);
        assert_eq!(direct.len(), shared.len());
        assert!((direct[0].from.distance(shared[0].from)).abs() < 1e-9);
        assert!((direct[0].to.distance(shared[0].to)).abs() < 1e-9);
        Ok(())
    }
}
