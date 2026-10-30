//! Acute copper corner check: joints sharper than a right angle.
//!
//! Context: called once from `drc` in `lib.rs` after stub length. Flags track
//! joints and track-to-rect-pad joins with an internal angle below ninety
//! degrees, which can trap etchant. Track-to-track logic complements open work
//! on `acute_angle` (#872); pad joins are handled here.
//!
//! Local rules:
//! - Straight `Track` segments only; curved tracks are skipped until tangents
//!   are defined here.
//! - Endpoints closer than `NODE_MERGE_MM` are one junction.
//! - Collinear continuation never reports.
//! - One finding per unordered track pair at a junction, or one per track at a
//!   shallow rect-pad join.
//!
//! Dependencies: `Track`, `Footprint`, `Pad` in `crates/doc`; `Point` in geom.
use super::Finding;
use pcbcraft_doc::{Id, Layer, Project, Track};
use pcbcraft_geom::Point;
use std::collections::BTreeSet;

const NODE_MERGE_MM: f64 = 1e-6;
const MIN_INTERNAL_ANGLE_DEG: f64 = 90.0;

struct Node {
    at: Point,
    tracks: Vec<(usize, bool)>,
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
        tracks: vec![],
    });
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

fn acute_message(
    project: &Project,
    net: Id,
    at: Point,
    angle: f64,
    detail: &str,
) -> String {
    format!(
        "{}: {detail} at ({:.3}, {:.3}) with a {:.1}° corner; at least {:.0}° is easier to etch; try track.round_corners",
        project.net_name(net),
        at.x,
        at.y,
        angle,
        MIN_INTERNAL_ANGLE_DEG,
    )
}

fn track_track_findings(
    project: &Project,
    layer: Layer,
    seen: &mut BTreeSet<(Id, Id)>,
) -> Vec<Finding> {
    let mut result = vec![];
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
                        code: "acute_corner",
                        severity: project.rule_severity("acute_corner", "warning"),
                        message: acute_message(
                            project,
                            net,
                            node.at,
                            angle,
                            &format!("tracks {id_a} and {id_b} meet"),
                        ),
                        objects: vec![id_a, id_b],
                    });
                }
            }
        }
    }
    result
}

fn pad_is_rect(shape: &str) -> bool {
    matches!(shape, "rect" | "roundrect" | "chamfered")
}

/// Shallow entry angle of a track into a rect pad, in degrees, or none when
/// the track is aligned with a pad edge.
fn rect_pad_entry_angle_deg(track_dir: Point, pad_rotation_deg: f64) -> Option<f64> {
    let theta = (-pad_rotation_deg).to_radians();
    let cos = theta.cos();
    let sin = theta.sin();
    let lx = track_dir.x * cos - track_dir.y * sin;
    let ly = track_dir.x * sin + track_dir.y * cos;
    let mut angle = ly.atan2(lx).to_degrees();
    if angle < 0.0 {
        angle += 360.0;
    }
    let quarter = angle % 90.0;
    if quarter <= 1e-6 || (90.0 - quarter) <= 1e-6 {
        return None;
    }
    Some(quarter.min(90.0 - quarter))
}

fn track_pad_findings(project: &Project, layer: Layer) -> Vec<Finding> {
    let mut result = vec![];
    for f in &project.board.footprints {
        for pad in &f.pads {
            let Some(net) = pad.net else {
                continue;
            };
            let Some(geometry) = pad.geometry.as_ref() else {
                continue;
            };
            if !pad_is_rect(&geometry.shape) {
                continue;
            }
            if !geometry
                .layers
                .iter()
                .any(|l| layer.name() == l.as_str())
            {
                continue;
            }
            let at = f.pad_world_position(pad);
            for track in project.board.tracks.iter().filter(|t| {
                t.net == net && t.layer == layer && t.arc.is_none()
            }) {
                let (into_pad, outward) = if track.start.distance(at) <= NODE_MERGE_MM {
                    (track.end, track.start)
                } else if track.end.distance(at) <= NODE_MERGE_MM {
                    (track.start, track.end)
                } else {
                    continue;
                };
                let (dx, dy, len) = direction_from(at, into_pad);
                if len <= NODE_MERGE_MM {
                    continue;
                }
                let Some(angle) =
                    rect_pad_entry_angle_deg(Point::new(dx, dy), f.rotation + geometry.rotation)
                else {
                    continue;
                };
                if angle + 1e-6 >= MIN_INTERNAL_ANGLE_DEG {
                    continue;
                }
                result.push(Finding {
                    code: "acute_corner",
                    severity: project.rule_severity("acute_corner", "warning"),
                    message: acute_message(
                        project,
                        net,
                        outward,
                        angle,
                        &format!("track {} meets pad {}", track.id, pad.number),
                    ),
                    objects: vec![track.id],
                });
            }
        }
    }
    result
}

/// `acute_corner` findings for sharp track corners and shallow pad joins.
pub(crate) fn acute_corner_findings(project: &Project) -> Vec<Finding> {
    let mut result = vec![];
    let mut seen: BTreeSet<(Id, Id)> = BTreeSet::new();
    for layer in [Layer::Front, Layer::Back] {
        result.extend(track_track_findings(project, layer, &mut seen));
        result.extend(track_pad_findings(project, layer));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::drc;
    use super::super::Finding;
    use pcbcraft_doc::{Footprint, Id, Layer, Net, Pad, Track, component_template};
    use pcbcraft_geom::Point;
    use pcbcraft_library::FootprintPad;

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
            .filter(|f| f.code == "acute_corner")
            .collect()
    }

    #[test]
    fn ninety_degree_corner_does_not_report_acute_corner() -> Result<(), String> {
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
    fn sixty_degree_join_reports_acute_corner() -> Result<(), String> {
        let mut p = pcbcraft_doc::Project::default();
        p.nets.push(Net {
            id: 1,
            name: "sig".into(),
            ..Default::default()
        });
        track(&mut p, 20, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
        let tip = Point::new(
            10.0 + 10.0 * (-0.5),
            10.0 * (3.0_f64).sqrt() / 2.0,
        );
        track(&mut p, 21, Point::new(10.0, 0.0), tip);
        p.validate()?;
        let found = acute(&p);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].objects, vec![20, 21]);
        assert!(found[0].message.contains("60.0°") || found[0].message.contains("60.0"));
        Ok(())
    }

    #[test]
    fn mitred_corner_does_not_report_acute_corner() -> Result<(), String> {
        let mut p = pcbcraft_doc::Project::default();
        p.nets.push(Net {
            id: 1,
            name: "sig".into(),
            ..Default::default()
        });
        track(&mut p, 30, Point::new(0.0, 0.0), Point::new(8.0, 0.0));
        track(&mut p, 31, Point::new(8.0, 0.0), Point::new(10.0, 2.0));
        track(&mut p, 32, Point::new(10.0, 2.0), Point::new(10.0, 10.0));
        p.validate()?;
        assert!(acute(&p).is_empty());
        Ok(())
    }

    #[test]
    fn shallow_rect_pad_join_reports_acute_corner() -> Result<(), String> {
        let mut p = pcbcraft_doc::Project::default();
        p.nets.push(Net {
            id: 1,
            name: "sig".into(),
            ..Default::default()
        });
        if let Some(mut c) = component_template("resistor", 2, "R1".into(), Point::new(10.0, 10.0))
        {
            for pin in &mut c.pins {
                pin.net = Some(1);
            }
            p.components.push(c);
        }
        p.board.footprints.push(Footprint {
            id: 12,
            component: 2,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 1.0,
                net: Some(1),
                castellated: None,
                geometry: Some(FootprintPad {
                    number: "1".into(),
                    pad_type: "smd".into(),
                    shape: "rect".into(),
                    position: Point::default(),
                    rotation: 0.0,
                    size: Point::new(2.0, 2.0),
                    layers: vec!["F.Cu".into()],
                    drill: None,
                    roundrect_ratio: None,
                    chamfer: None,
                    polygon: vec![],
                    raw_source: String::new(),
                }),
            }],
        });
        track(
            &mut p,
            40,
            Point::new(14.0, 11.0),
            Point::new(10.0, 10.0),
        );
        p.validate()?;
        let found = acute(&p);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].objects, vec![40]);
        Ok(())
    }
}
