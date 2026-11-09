//! Checks that mask bridges between plain pads meet the maker's minimum.
//!
//! Context: issue 110. Fabrication houses require a minimum amount of
//! soldermask between two mask openings. This check measures the copper
//! gap between all pairs of plain (non-exposed, non-notched) pads on each
//! side, subtracts the mask margins, and reports when the remaining mask
//! bridge is too thin.
//!
//! # Rules
//! - Only plain pads are checked here; special pairs stay with mask_web.
//! - The limit comes from `board.rules.mask_dam`; when unset, the check
//!   produces no findings.
//! - One finding per pair of openings that violates the limit.
//!
//! # Dependencies
//! - crates/rules/src/lib.rs (`Finding`, `drc`)
//! - crates/rules/src/pad_kinds.rs (`openings`, `ring_gap`, `Opening`)
//! - crates/geom/src/lib.rs (via pad_kinds)
//!
//! # Do not
//! - Flag pairs where one pad is exposed or notched; mask_web owns them.
use super::Finding;
use super::pad_kinds::{openings, ring_gap};
use pcbcraft_doc::{Layer, Project};

/// `mask_dam` findings for plain pad pairs with too-thin mask bridges.
///
/// # Rules
/// - Severity is set by `project.rule_severity("mask_dam", "warning")`.
/// - Returns empty when `board.rules` is `None` or `mask_dam` is `None`.
///
/// # Dependencies
/// - crates/rules/src/lib.rs (`drc` calls this once)
///
/// # Do not
/// - Report pairs where the copper gap is zero or less; that is a clearance
///   matter, not a mask dam issue.
pub(crate) fn mask_dam_findings(project: &Project) -> Vec<Finding> {
    let Some(rules) = project.board.rules.as_ref() else {
        return vec![];
    };
    let Some(limit) = rules.mask_dam else {
        return vec![];
    };
    let mut result = vec![];
    for side in [Layer::Front, Layer::Back] {
        let list = openings(project, side);
        for (i, a) in list.iter().enumerate() {
            for b in list.iter().skip(i + 1) {
                if a.special || b.special {
                    continue;
                }
                if a.footprint == b.footprint && a.number == b.number {
                    continue;
                }
                let copper = ring_gap(&a.ring, &b.ring);
                if copper <= 0.0 {
                    continue;
                }
                let web = copper - a.margin - b.margin;
                if web + 1e-6 < limit {
                    result.push(Finding {
                        code: "mask_dam",
                        severity: project.rule_severity("mask_dam", "warning"),
                        message: format!(
                            "Mask between pad {} and pad {} is {:.3} mm; the maker needs at least {:.3} mm",
                            a.number, b.number, web.max(0.0), limit
                        ),
                        objects: vec![a.footprint, b.footprint],
                        hint: Some("Move the pads apart, or ask the maker for a thinner mask bridge.".into()),
                    });
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Footprint, Id, Pad};
    use pcbcraft_geom::Point;
    use pcbcraft_library::FootprintPad;

    fn pad_at(id: Id, at: Point, layers: &[&str], polygon: Vec<Point>) -> Footprint {
        Footprint {
            id,
            component: id,
            position: at,
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![Pad {
                number: id.to_string(),
                offset: Point::default(),
                diameter: 1.0,
                net: None,
                castellated: None,
                geometry: Some(FootprintPad {
                    number: id.to_string(),
                    pad_type: "smd".into(),
                    shape: if polygon.is_empty() { "rect" } else { "custom" }.into(),
                    size: Point::new(1.0, 1.0),
                    layers: layers.iter().map(|l| (*l).to_owned()).collect(),
                    polygon,
                    ..FootprintPad::default()
                }),
            }],
            preserved: pcbcraft_doc::PreservedSource::default(),
        }
    }

    const PLAIN: [&str; 3] = ["F.Cu", "F.Paste", "F.Mask"];
    const EXPOSED: [&str; 2] = ["F.Cu", "F.Mask"];

    #[test]
    fn a_mask_bridge_thinner_than_the_limit_fires_and_a_wide_one_stays_quiet() {
        let mut p = Project::default();
        p.board.rules = Some(pcbcraft_doc::FabRules {
            mask_dam: Some(0.55),
            ..Default::default()
        });
        p.board
            .footprints
            .push(pad_at(1, Point::new(10.0, 10.0), &PLAIN, vec![]));
        p.board
            .footprints
            .push(pad_at(2, Point::new(11.6, 10.0), &PLAIN, vec![]));
        let findings = mask_dam_findings(&p);
        assert_eq!(findings.len(), 1, "web = 0.5 mm is less than 0.55 mm limit");
        assert_eq!(findings[0].code, "mask_dam");

        if let Some(r) = p.board.rules.as_mut() {
            r.mask_dam = Some(0.5);
        }
        let findings = mask_dam_findings(&p);
        assert!(findings.is_empty(), "web = 0.5 mm meets the 0.5 mm limit");

        p.board.rules = None;
        let findings = mask_dam_findings(&p);
        assert!(findings.is_empty(), "no rules means no findings");
    }

    #[test]
    fn a_pair_with_an_exposed_pad_is_left_to_mask_web() {
        let mut p = Project::default();
        p.board.rules = Some(pcbcraft_doc::FabRules {
            mask_dam: Some(0.55),
            ..Default::default()
        });
        p.board
            .footprints
            .push(pad_at(1, Point::new(10.0, 10.0), &PLAIN, vec![]));
        p.board
            .footprints
            .push(pad_at(2, Point::new(11.6, 10.0), &EXPOSED, vec![]));
        let findings = mask_dam_findings(&p);
        assert!(
            findings.is_empty(),
            "mask_dam should not fire for exposed pads"
        );
    }
}
