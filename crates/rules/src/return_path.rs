//! Return-path check for fast nets: reference-plane coverage and stitching at layer changes.
//!
//! Context: called from `drc` in `lib.rs` after pour geometry is available. Uses
//! poured zone polygons on the adjacent reference copper layer, not bare outlines.
//!
//! Local rules:
//! - Scope is nets marked fast on their net class, or either side of a differential pair.
//! - A track must sit over reference pour copper on the layer below or above it in the stack.
//! - A layer change on a fast net needs a ground (stitching) via within 2 mm of the transition.
//! - When a reference pour cannot be computed, the check refuses once with a plain message.
//!
//! Dependencies: `pcbcraft_doc::zone_fill`, `super::inside`, stackup rows on the board.
use super::{Finding, inside};
use pcbcraft_doc::{Id, Layer, Project, Track};
use pcbcraft_geom::Point;
use std::collections::BTreeSet;

/// Default search radius for a stitching via at a layer change, in millimetres.
pub(crate) const STITCH_DISTANCE_MM: f64 = 2.0;
/// Sample spacing along a track centreline, in millimetres.
const SAMPLE_SPACING_MM: f64 = 0.25;

type PourPiece = (Vec<Point>, Vec<Vec<Point>>);

/// Findings for return-path violations on fast nets.
pub(crate) fn return_path_findings(project: &Project) -> Vec<Finding> {
    if project.rule_disabled("return_path") {
        return vec![];
    }
    let fast: BTreeSet<Id> = project
        .nets
        .iter()
        .filter(|n| project.net_is_fast(n.id))
        .map(|n| n.id)
        .collect();
    if fast.is_empty() {
        return vec![];
    }
    let ground: BTreeSet<Id> = project
        .nets
        .iter()
        .filter(|n| reference_net_name(&n.name))
        .map(|n| n.id)
        .collect();
    if ground.is_empty() {
        return vec![];
    }

    let mut ref_layers: BTreeSet<Layer> = BTreeSet::new();
    for track in &project.board.tracks {
        if fast.contains(&track.net) {
            if let Some(layer) = reference_layer_for(project, track.layer) {
                ref_layers.insert(layer);
            }
        }
    }

    let mut pours: BTreeSet<(Layer, Id)> = BTreeSet::new();
    for zone in &project.board.zones {
        if zone.filled && ground.contains(&zone.net) && ref_layers.contains(&zone.layer) {
            pours.insert((zone.layer, zone.id));
        }
    }

    let mut pieces_by_layer: BTreeSet<Layer> = BTreeSet::new();
    let mut findings = vec![];
    for (layer, zone_id) in &pours {
        let zone = match project.board.zones.iter().find(|z| z.id == *zone_id) {
            Some(z) => z,
            None => continue,
        };
        match pour_pieces(project, zone) {
            Ok(pieces) => {
                if !pieces.is_empty() {
                    pieces_by_layer.insert(*layer);
                }
            }
            Err(message) => {
                findings.push(Finding {
                    code: "return_path",
                    severity: project.rule_severity("return_path", "warning"),
                    message: format!(
                        "Return path check skipped: plane pour on {} could not be computed ({message})",
                        project.layer_name(*layer),
                    ),
                    objects: vec![*zone_id],
                });
                return findings;
            }
        }
    }

    if pieces_by_layer.is_empty() {
        return findings;
    }

    let mut reported_gap: BTreeSet<(Id, Id)> = BTreeSet::new();
    for track in &project.board.tracks {
        if !fast.contains(&track.net) {
            continue;
        }
        let Some(ref_layer) = reference_layer_for(project, track.layer) else {
            continue;
        };
        let layer_pieces = match reference_pieces(project, ref_layer, &ground, &pours) {
            Ok(p) => p,
            Err(message) => {
                findings.push(Finding {
                    code: "return_path",
                    severity: project.rule_severity("return_path", "warning"),
                    message: format!(
                        "Return path check skipped: plane pour on {} could not be computed ({message})",
                        project.layer_name(ref_layer),
                    ),
                    objects: vec![track.id],
                });
                return findings;
            }
        };
        if layer_pieces.is_empty() {
            continue;
        }
        let bad = sample_track(track)
            .into_iter()
            .find(|p| !point_on_reference(*p, &layer_pieces));
        if let Some(at) = bad
            && reported_gap.insert((track.net, track.id))
        {
            findings.push(Finding {
                code: "return_path",
                severity: project.rule_severity("return_path", "warning"),
                message: format!(
                    "{}: track {} crosses a gap in the reference plane on {} near ({:.2}, {:.2}) mm",
                    project.net_name(track.net),
                    track.id,
                    project.layer_name(ref_layer),
                    at.x,
                    at.y,
                ),
                objects: vec![track.id],
            });
        }
    }

    let mut reported_stitch: BTreeSet<(Id, Id)> = BTreeSet::new();
    for via in &project.board.vias {
        if !fast.contains(&via.net) {
            continue;
        }
        let span = project.via_span_layers(via);
        if span.len() < 2 {
            continue;
        }
        if has_stitch_near(project, via.position, &ground, STITCH_DISTANCE_MM) {
            continue;
        }
        if reported_stitch.insert((via.net, via.id)) {
            findings.push(Finding {
                code: "return_path",
                severity: project.rule_severity("return_path", "warning"),
                message: format!(
                    "{}: layer change at via {} has no stitching via within {STITCH_DISTANCE_MM:.1} mm",
                    project.net_name(via.net),
                    via.id,
                ),
                objects: vec![via.id],
            });
        }
    }

    findings
}

fn reference_net_name(name: &str) -> bool {
    matches!(
        name.trim().to_uppercase().as_str(),
        "GND" | "VSS" | "AGND" | "DGND" | "PGND" | "0V" | "GROUND"
    )
}

fn reference_layer_for(project: &Project, signal: Layer) -> Option<Layer> {
    let idx = signal.index();
    let count = project.copper_layer_count();
    if count <= 1 {
        return None;
    }
    let copper: Vec<_> = project.copper_layer_records().collect();
    if copper.is_empty() {
        if idx == 0 {
            return Layer::from_index(count - 1).ok();
        }
        if idx + 1 == count {
            return Layer::from_index(0).ok();
        }
        return None;
    }
    let plane_index = |i: usize| -> bool {
        copper
            .get(i)
            .is_some_and(|r| r.function() == "plane" || r.copper_role.as_deref() == Some("plane"))
    };
    if idx == 0 {
        return (1..count as usize)
            .find(|&i| plane_index(i))
            .and_then(|i| Layer::from_index(i as u8).ok())
            .or_else(|| Layer::from_index(1).ok());
    }
    if idx + 1 == count {
        return (0..count as usize - 1)
            .rfind(|&i| plane_index(i))
            .and_then(|i| Layer::from_index(i as u8).ok())
            .or_else(|| Layer::from_index(count - 2).ok());
    }
    let below = (0..idx as usize).rfind(|&i| plane_index(i));
    let above = ((idx as usize + 1)..count as usize).find(|&i| plane_index(i));
    match (below, above) {
        (Some(b), Some(a)) => {
            let db = idx as usize - b;
            let da = a - idx as usize;
            if db <= da {
                Layer::from_index(b as u8).ok()
            } else {
                Layer::from_index(a as u8).ok()
            }
        }
        (Some(b), None) => Layer::from_index(b as u8).ok(),
        (None, Some(a)) => Layer::from_index(a as u8).ok(),
        (None, None) => {
            if idx > 0 {
                Layer::from_index(idx - 1).ok()
            } else {
                Layer::from_index(idx + 1).ok()
            }
        }
    }
}

fn pour_pieces(
    project: &Project,
    zone: &pcbcraft_doc::CopperZone,
) -> Result<Vec<PourPiece>, String> {
    let fill = pcbcraft_doc::zone_fill(project, zone)?;
    Ok(fill
        .pieces
        .iter()
        .filter_map(|piece| {
            let mut rings = piece.iter();
            let outer = rings.next()?.clone();
            Some((outer, rings.cloned().collect()))
        })
        .collect())
}

fn reference_pieces(
    project: &Project,
    layer: Layer,
    ground: &BTreeSet<Id>,
    pours: &BTreeSet<(Layer, Id)>,
) -> Result<Vec<PourPiece>, String> {
    let mut out = vec![];
    for (pour_layer, zone_id) in pours {
        if *pour_layer != layer {
            continue;
        }
        let zone = project
            .board
            .zones
            .iter()
            .find(|z| z.id == *zone_id)
            .ok_or_else(|| format!("zone {zone_id} missing"))?;
        if !ground.contains(&zone.net) {
            continue;
        }
        out.extend(pour_pieces(project, zone)?);
    }
    Ok(out)
}

fn point_on_reference(point: Point, pieces: &[PourPiece]) -> bool {
    pieces
        .iter()
        .any(|(outer, holes)| inside(point, outer) && !holes.iter().any(|hole| inside(point, hole)))
}

fn sample_track(track: &Track) -> Vec<Point> {
    if let Some(arc) = track.curve() {
        let len = arc.length();
        let steps = ((len / SAMPLE_SPACING_MM).ceil() as usize).clamp(1, 4096);
        (0..=steps)
            .map(|i| arc.point_at(i as f64 / steps as f64))
            .collect()
    } else {
        let len = track.start.distance(track.end);
        let steps = ((len / SAMPLE_SPACING_MM).ceil() as usize).clamp(1, 4096);
        (0..=steps)
            .map(|i| {
                let t = i as f64 / steps as f64;
                Point::new(
                    track.start.x + (track.end.x - track.start.x) * t,
                    track.start.y + (track.end.y - track.start.y) * t,
                )
            })
            .collect()
    }
}

fn has_stitch_near(project: &Project, at: Point, ground: &BTreeSet<Id>, radius: f64) -> bool {
    project
        .board
        .vias
        .iter()
        .any(|v| ground.contains(&v.net) && v.position.distance(at) <= radius + 1e-9)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{CopperZone, Net, NetClass, Track, Via};

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
            Point::new(x0, y0),
        ]
    }

    fn fast_class(p: &mut Project, net: Id) {
        p.net_classes.push(NetClass {
            id: 50,
            name: "Fast".into(),
            clearance: 0.2,
            width: 0.25,
            min_width: None,
            max_width: None,
            target_length: None,
            length_tolerance: None,
            max_stub: None,
            via_diameter: None,
            via_drill: None,
            pair_width: None,
            pair_gap: None,
            nets: vec![net],
            profile: None,
            fast: true,
        });
    }

    fn two_layer_board_with_ground_pour() -> Project {
        let mut p = Project::default();
        p.board.width = 40.0;
        p.board.height = 40.0;
        p.nets.push(Net {
            id: 1,
            name: "GND".into(),
            ..Default::default()
        });
        p.nets.push(Net {
            id: 2,
            name: "SIG".into(),
            ..Default::default()
        });
        p.board.zones.push(CopperZone {
            id: 10,
            net: 1,
            layer: Layer::Back,
            outline: square(0.0, 0.0, 40.0, 40.0),
            cutouts: vec![],
            filled: true,
            kind: None,
            pour: None,
        });
        fast_class(&mut p, 2);
        p
    }

    #[test]
    fn track_over_reference_slot_is_flagged_and_moving_clears_it() -> Result<(), String> {
        let mut p = two_layer_board_with_ground_pour();
        p.board.zones[0]
            .cutouts
            .push(square(15.0, 18.0, 25.0, 22.0));
        p.board.tracks.push(Track {
            id: 20,
            net: 2,
            start: Point::new(10.0, 20.0),
            end: Point::new(30.0, 20.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.validate()?;
        assert!(
            !return_path_findings(&p).is_empty(),
            "a fast track over a reference slot should report"
        );
        p.board.tracks[0].start.y = 10.0;
        p.board.tracks[0].end.y = 10.0;
        p.validate()?;
        assert!(
            return_path_findings(&p).is_empty(),
            "moving off the slot clears the finding"
        );
        Ok(())
    }

    #[test]
    fn layer_change_without_stitching_via_is_flagged() -> Result<(), String> {
        let mut p = two_layer_board_with_ground_pour();
        p.board.tracks.push(Track {
            id: 21,
            net: 2,
            start: Point::new(10.0, 10.0),
            end: Point::new(20.0, 10.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.board.tracks.push(Track {
            id: 22,
            net: 2,
            start: Point::new(20.0, 10.0),
            end: Point::new(30.0, 10.0),
            width: 0.25,
            layer: Layer::Back,
            arc: None,
        });
        p.board.vias.push(Via {
            id: 30,
            net: 2,
            position: Point::new(20.0, 10.0),
            diameter: 0.8,
            drill: 0.4,
            from_layer: None,
            to_layer: None,
            covering: None,
        });
        p.validate()?;
        assert!(
            return_path_findings(&p)
                .iter()
                .any(|f| f.message.contains("stitching via")),
            "missing ground stitch should report"
        );
        p.board.vias.push(Via {
            id: 31,
            net: 1,
            position: Point::new(20.5, 10.0),
            diameter: 0.8,
            drill: 0.4,
            from_layer: None,
            to_layer: None,
            covering: None,
        });
        p.validate()?;
        assert!(
            !return_path_findings(&p)
                .iter()
                .any(|f| f.message.contains("stitching via")),
            "a nearby ground via satisfies the stitch rule"
        );
        Ok(())
    }

    #[test]
    fn stale_reference_pour_refuses_with_plain_message() -> Result<(), String> {
        let mut p = two_layer_board_with_ground_pour();
        p.board.tracks.push(Track {
            id: 20,
            net: 2,
            start: Point::new(10.0, 10.0),
            end: Point::new(12.0, 10.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        let mut heavy = square(0.0, 0.0, 40.0, 40.0);
        while heavy.len() < 257 {
            heavy.insert(1, Point::new(1.0, 0.0));
        }
        p.board.zones[0].outline = heavy;
        p.validate()?;
        let findings = return_path_findings(&p);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "return_path");
        assert!(
            findings[0].message.contains("skipped"),
            "stale pour produces a refusal message"
        );
        Ok(())
    }

    #[test]
    fn unclassified_slow_net_is_never_checked() -> Result<(), String> {
        let mut p = two_layer_board_with_ground_pour();
        p.net_classes.clear();
        p.board.zones[0]
            .cutouts
            .push(square(15.0, 18.0, 25.0, 22.0));
        p.board.tracks.push(Track {
            id: 20,
            net: 2,
            start: Point::new(10.0, 20.0),
            end: Point::new(30.0, 20.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.validate()?;
        assert!(return_path_findings(&p).is_empty());
        Ok(())
    }
}
