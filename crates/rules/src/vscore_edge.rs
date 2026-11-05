//! Copper clearance to scored edges: tracks and pads must keep a minimum
//! distance from v-score lines, measured from the score line to the copper
//! edge (not the track center).
//!
//! Context: called from `drc` in `lib.rs` when `FabRules.edge_vscore` is set.
//! Uses `pcbcraft_doc::board_vscore_segments` to find all score lines,
//! including user-defined Cut layers and panel score lines.
//!
//! Local rules:
//! - Distance is computed from the copper edge: segment distance minus the
//!   copper radius (or outline_error for complex shapes).
//! - Each copper id is reported only once, with the smallest distance.
//! - No limit means the check is not run.
//!
//! Dependencies: `crates/doc/src/lib.rs` (`board_vscore_segments`),
//! `crates/doc/src/fab.rs` (`FabRules`), `crates/geom/src/lib.rs`
//! (`segment_distance`), `crates/rules/src/lib.rs` (`copper`, `Finding`).

use super::{Finding, copper};
use pcbcraft_doc::{Id, Project, board_vscore_segments};
use pcbcraft_geom::segment_distance;
use std::collections::BTreeMap;

/// One finding per copper item that is too close to a scored edge.
pub(crate) fn vscore_edge_findings(project: &Project) -> Vec<Finding> {
    let Some(rules) = project.board.rules.as_ref() else {
        return vec![];
    };
    let Some(limit) = rules.edge_vscore else {
        return vec![];
    };
    let segments = board_vscore_segments(project);
    if segments.is_empty() {
        return vec![];
    }

    let items = copper(project);
    // Map copper id -> smallest distance to any score line.
    let mut worst: BTreeMap<Id, f64> = BTreeMap::new();

    for item in items {
        for (a, b) in &segments {
            let dist = if let Some(outline) = &item.outline {
                // Minimum over all outline edges minus outline error,
                // same approach as the edge_routed check.
                let min_outline = outline
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let q = outline[(i + 1) % outline.len()];
                        segment_distance(*p, q, *a, *b)
                    })
                    .fold(f64::INFINITY, f64::min);
                min_outline - item.outline_error
            } else {
                segment_distance(item.start, item.end, *a, *b) - item.radius
            };
            if dist < worst.get(&item.id).copied().unwrap_or(f64::INFINITY) {
                worst.insert(item.id, dist);
            }
        }
    }

    let mut findings = vec![];
    for (id, dist) in worst {
        if dist + 1e-6 < limit {
            findings.push(Finding {
                code: "copper_to_vscore",
                severity: project.rule_severity("copper_to_vscore", "error"),
                message: format!(
                    "Copper is {:.3} mm from the score line; the maker needs at least {:.3} mm",
                    dist, limit
                ),
                objects: vec![id],
                hint: Some("Move the copper away from the score line.".into()),
            });
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{ArtKind, ArtLayer, ArtShape, FabRules};
    use pcbcraft_geom::Point;

    fn project_with_track(x: f64) -> Project {
        let mut p = Project::default();
        p.board.tracks.push(pcbcraft_doc::Track {
            id: 1,
            net: 1,
            start: Point::new(x, 0.0),
            end: Point::new(5.0, 0.0),
            width: 0.4,
            layer: pcbcraft_doc::Layer::Front,
            arc: None,
            preserved: Default::default(),
        });
        p
    }

    fn add_cut_layer(p: &mut Project) {
        p.board.user_layers.push(ArtLayer {
            side: Default::default(),
            id: 10,
            name: "Cut".into(),
            kind: ArtKind::Cut,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![ArtShape::Line {
                start: Point::new(0.0, -5.0),
                end: Point::new(0.0, 5.0),
            }],
        });
    }

    fn add_edge_vscore(p: &mut Project, limit: f64) {
        p.board.rules = Some(FabRules {
            edge_vscore: Some(limit),
            ..FabRules::default()
        });
    }

    #[test]
    fn copper_nearer_the_score_line_than_the_limit_fires_and_farther_copper_does_not()
    -> Result<(), String> {
        // Track at x=1.0 with radius 0.2 -> edge at x=0.8, 0.8 mm from score line.
        let mut p = project_with_track(1.0);
        add_cut_layer(&mut p);

        // No edge_vscore: no finding.
        let f = vscore_edge_findings(&p);
        assert_eq!(f.len(), 0, "no finding without edge_vscore");

        // Limit 1.0: track edge is 0.8 mm away, must fire.
        add_edge_vscore(&mut p, 1.0);
        let f = vscore_edge_findings(&p);
        assert_eq!(f.len(), 1, "finding when closer than limit");
        assert_eq!(f[0].code, "copper_to_vscore");
        assert!(
            f[0].message.contains("0.800"),
            "message should report distance: {}",
            f[0].message
        );

        // Limit 0.8: track edge is 0.8 mm away, must stay quiet.
        add_edge_vscore(&mut p, 0.8);
        let f = vscore_edge_findings(&p);
        assert_eq!(f.len(), 0, "no finding when exactly at limit");

        // No Cut layer: no finding.
        add_edge_vscore(&mut p, 1.0);
        p.board.user_layers.clear();
        let f = vscore_edge_findings(&p);
        assert_eq!(f.len(), 0, "no finding without score lines");

        Ok(())
    }
}
