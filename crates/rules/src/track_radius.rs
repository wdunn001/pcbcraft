//! Minimum arc radius check for classified nets.
//!
//! Context: called once from `drc` in `lib.rs` after width checks. Flags curved
//! tracks whose circular radius falls below the net class `min_radius`.
//!
//! Local rules:
//! - Straight tracks and arcs that fail `Track::curve()` are skipped.
//! - One finding per track id; severity follows `track_radius`.
//!
//! Dependencies: `NetClass::min_radius`, `Track::curve` in `crates/doc`.
use super::Finding;
use pcbcraft_doc::{Id, Project, Track};

fn track_radius_finding(
    project: &Project,
    track: &Track,
    class_name: &str,
    limit: f64,
    measured: f64,
) -> Finding {
    Finding {
        code: "track_radius",
        severity: project.rule_severity("track_radius", "error"),
        message: format!(
            "{} track {} arc radius is {:.3} mm, {} requires at least {:.3} mm; try track.round_corners",
            project.net_name(track.net),
            track.id,
            measured,
            class_name,
            limit,
        ),
        objects: vec![track.id],
    }
}

/// `track_radius` findings for every arc tighter than its net class allows.
pub(crate) fn track_radius_findings(project: &Project) -> Vec<Finding> {
    let mut result = vec![];
    for track in &project.board.tracks {
        let Some(class) = project.net_class(track.net) else {
            continue;
        };
        let Some(limit) = class.min_radius else {
            continue;
        };
        let Some(arc) = track.curve() else {
            continue;
        };
        let measured = arc.radius;
        if measured + 1e-6 >= limit {
            continue;
        }
        result.push(track_radius_finding(
            project,
            track,
            &class.name,
            limit,
            measured,
        ));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::drc;
    use super::super::Finding;
    use pcbcraft_doc::{Id, Layer, Net, NetClass, Track};
    use pcbcraft_geom::Point;

    fn arc_track(p: &mut pcbcraft_doc::Project, id: Id, start: Point, mid: Point, end: Point) {
        p.board.tracks.push(Track {
            id,
            net: 1,
            start,
            end,
            width: 0.25,
            layer: Layer::Front,
            arc: Some(mid),
        });
    }

    fn radius(p: &pcbcraft_doc::Project) -> Vec<Finding> {
        drc(p)
            .into_iter()
            .filter(|f| f.code == "track_radius")
            .collect()
    }

    #[test]
    fn arc_tighter_than_class_min_radius_reports_track_radius() -> Result<(), String> {
        let mut p = pcbcraft_doc::Project::default();
        p.nets.push(Net {
            id: 1,
            name: "sig".into(),
            ..Default::default()
        });
        p.net_classes.push(NetClass {
            id: 50,
            name: "TIGHT".into(),
            clearance: 0.2,
            width: 0.25,
            min_width: None,
            max_width: None,
            target_length: None,
            length_tolerance: None,
            max_stub: None,
            min_radius: Some(2.0),
            via_diameter: None,
            via_drill: None,
            pair_width: None,
            pair_gap: None,
            nets: vec![1],
            profile: None,
        });
        arc_track(
            &mut p,
            10,
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.5),
            Point::new(2.0, 0.0),
        );
        p.validate()?;
        let found = radius(&p);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].objects, vec![10]);
        assert!(found[0].message.contains("track.round_corners"));
        Ok(())
    }

    #[test]
    fn straight_track_never_reports_track_radius() -> Result<(), String> {
        let mut p = pcbcraft_doc::Project::default();
        p.nets.push(Net {
            id: 1,
            name: "sig".into(),
            ..Default::default()
        });
        p.net_classes.push(NetClass {
            id: 50,
            name: "TIGHT".into(),
            clearance: 0.2,
            width: 0.25,
            min_width: None,
            max_width: None,
            target_length: None,
            length_tolerance: None,
            max_stub: None,
            min_radius: Some(2.0),
            via_diameter: None,
            via_drill: None,
            pair_width: None,
            pair_gap: None,
            nets: vec![1],
            profile: None,
        });
        p.board.tracks.push(Track {
            id: 11,
            net: 1,
            start: Point::new(0.0, 0.0),
            end: Point::new(5.0, 0.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.validate()?;
        assert!(radius(&p).is_empty());
        Ok(())
    }
}
