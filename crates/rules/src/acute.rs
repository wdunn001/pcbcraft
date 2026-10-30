//! Acute copper corner check: track joints sharper than a right angle.
//!
//! Context: called once from `drc` in `lib.rs` after stub length. Flags joints
//! where two track segments on the same net and layer meet with an internal
//! angle below ninety degrees, which can trap etchant.
//!
//! Local rules:
//! - Only straight `Track` segments; arc tracks are skipped until arc tangents
//!   are defined here.
//! - Endpoints closer than `NODE_MERGE_MM` are one junction.
//! - Collinear continuation (zero turn) never reports.
//! - One finding per unordered track pair at a junction.
//!
//! Dependencies: `Track` in `crates/doc/src/lib.rs`, `Point` in geom.
use super::Finding;
use pcbcraft_doc::{Id, Layer, Project, Track};
use pcbcraft_geom::Point;
use std::collections::BTreeSet;

/// Track ends closer than this are the same junction, in millimetres.
const NODE_MERGE_MM: f64 = 1e-6;
/// Corners at or above this internal angle pass, in degrees.
const MIN_INTERNAL_ANGLE_DEG: f64 = 90.0;

struct Node {
    at: Point,
    /// Indices into the layer track slice.
    tracks: Vec<(usize, bool)>,
}

fn node_index(nodes: &mut Vec<Node>, at: Point) -> usize {
    if let Some(i) = nodes
        .iter()
        .position(|n| n.at.distance(at) <= NODE_MERGE_MM)
    {
        return i;
    }
    nodes.push(Node { at, tracks: vec![] });
    nodes.len() - 1
}

fn direction_from(node_at: Point, other: Point) -> (f64, f64, f64) {
    let dx = other.x - node_at.x;
    let dy = other.y - node_at.y;
    let len = dx.hypot(dy);
    if len <= NODE_MERGE_MM {
        return (0.0, 0.0, 0.0);
    }
    (dx / len, dy / len, len)
}

fn internal_angle_deg(ax: f64, ay: f64, bx: f64, by: f64) -> Option<f64> {
    let cross = ax * by - ay * bx;
    if cross.abs() <= 1e-12 && ax * bx + ay * by < 0.0 {
        return None;
    }
    let between = (ax * bx + ay * by).clamp(-1.0, 1.0).acos().to_degrees();
    Some(between.min(180.0 - between))
}

/// `acute_angle` findings for every track joint sharper than ninety degrees.
///
/// Severity is `project.rule_severity("acute_angle", "warning")`.
pub(crate) fn acute_angle_findings(project: &Project) -> Vec<Finding> {
    let mut result = vec![];
    let mut seen: BTreeSet<(Id, Id)> = BTreeSet::new();
    for layer in [Layer::Front, Layer::Back] {
        let tracks: Vec<&Track> = project
            .board
            .tracks
            .iter()
            .filter(|t| t.layer == layer && t.arc.is_none())
            .collect();
        let mut by_net: BTreeSet<Id> = BTreeSet::new();
        for t in &tracks {
            by_net.insert(t.net);
        }
        for net in by_net {
            let net_tracks: Vec<&Track> = tracks.iter().copied().filter(|t| t.net == net).collect();
            let mut nodes: Vec<Node> = vec![];
            for (i, track) in net_tracks.iter().enumerate() {
                let a = node_index(&mut nodes, track.start);
                let b = node_index(&mut nodes, track.end);
                nodes[a].tracks.push((i, true));
                if a != b {
                    nodes[b].tracks.push((i, false));
                }
            }
            for node in &nodes {
                if node.tracks.len() < 2 {
                    continue;
                }
                let dirs: Vec<(usize, f64, f64)> = node
                    .tracks
                    .iter()
                    .filter_map(|(i, at_start)| {
                        let track = net_tracks[*i];
                        let other = if *at_start { track.end } else { track.start };
                        let (dx, dy, len) = direction_from(node.at, other);
                        (len > NODE_MERGE_MM).then_some((*i, dx, dy))
                    })
                    .collect();
                for a in 0..dirs.len() {
                    for b in a + 1..dirs.len() {
                        let (ia, ax, ay) = dirs[a];
                        let (ib, bx, by) = dirs[b];
                        let Some(angle) = internal_angle_deg(ax, ay, bx, by) else {
                            continue;
                        };
                        if angle + 1e-6 >= MIN_INTERNAL_ANGLE_DEG {
                            continue;
                        }
                        let id_a = net_tracks[ia].id;
                        let id_b = net_tracks[ib].id;
                        let key = if id_a <= id_b {
                            (id_a, id_b)
                        } else {
                            (id_b, id_a)
                        };
                        if !seen.insert(key) {
                            continue;
                        }
                        result.push(Finding {
                            code: "acute_angle",
                            severity: project.rule_severity("acute_angle", "warning"),
                            message: format!(
                                "{}: tracks {} and {} meet at ({:.3}, {:.3}) with a {:.1}° corner; at least {:.0}° is easier to etch",
                                project.net_name(net),
                                id_a,
                                id_b,
                                node.at.x,
                                node.at.y,
                                angle,
                                MIN_INTERNAL_ANGLE_DEG,
                            ),
                            objects: vec![id_a, id_b],
                        });
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::Finding;
    use super::super::drc;
    use pcbcraft_doc::{Layer, Net, Track};

    fn track(p: &mut pcbcraft_doc::Project, id: Id, a: Point, b: Point) {
        p.board.tracks.push(Track {
            id,
            net: 1,
            start: a,
            end: b,
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
    }

    fn acute(p: &pcbcraft_doc::Project) -> Vec<Finding> {
        drc(p)
            .into_iter()
            .filter(|f| f.code == "acute_angle")
            .collect()
    }

    #[test]
    fn ninety_degree_corner_does_not_report_acute_angle() -> Result<(), String> {
        let mut p = pcbcraft_doc::Project::default();
        p.nets.push(Net {
            id: 1,
            name: "sig".into(),
            ..Default::default()
        });
        track(&mut p, 10, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
        track(&mut p, 11, Point::new(10.0, 0.0), Point::new(10.0, 10.0));
        p.validate()?;
        assert!(acute(&p).is_empty());
        Ok(())
    }

    #[test]
    fn sharp_corner_reports_acute_angle_with_both_track_ids() -> Result<(), String> {
        let mut p = pcbcraft_doc::Project::default();
        p.nets.push(Net {
            id: 1,
            name: "sig".into(),
            ..Default::default()
        });
        track(&mut p, 20, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
        track(&mut p, 21, Point::new(10.0, 0.0), Point::new(15.0, 5.0));
        p.validate()?;
        let found = acute(&p);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].objects, vec![20, 21]);
        assert!(found[0].message.contains("45.0°") || found[0].message.contains("45.0"));
        Ok(())
    }

    #[test]
    fn collinear_joint_does_not_report_acute_angle() -> Result<(), String> {
        let mut p = pcbcraft_doc::Project::default();
        p.nets.push(Net {
            id: 1,
            name: "sig".into(),
            ..Default::default()
        });
        track(&mut p, 30, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
        track(&mut p, 31, Point::new(10.0, 0.0), Point::new(20.0, 0.0));
        p.validate()?;
        assert!(acute(&p).is_empty());
        Ok(())
    }
}
