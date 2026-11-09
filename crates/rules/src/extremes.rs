//! Minimum copper gap and mask bridge measurements for fab reporting.
//!
//! Context: issue 110. Called by the report exporter in the engine to
//! summarize the tightest manufacturable features on the board.
//!
//! # Rules
//! - `smallest_copper_gap` measures the closest approach between copper
//!   items of different nets on the same copper layer; it returns None when
//!   the board has fewer than two such items.
//! - `smallest_mask_web` measures the thinnest solder mask bridge between
//!   two pad openings on the same side; it returns None when no pair exists.
//!
//! # Dependencies
//! - crates/rules/src/lib.rs (`copper`, `gap`)
//! - crates/rules/src/pad_kinds.rs (`openings`, `ring_gap`)
//!
//! # Do not
//! - Treat same-net copper as a gap; that is connectivity, not clearance.
use super::pad_kinds;
use super::{copper, gap};
use pcbcraft_doc::{Layer, Project};

/// Minimum distance between copper items on the same layer with different nets.
///
/// Scans every pair of copper items on each copper layer and returns the
/// smallest gap found between items of different nets. When there are fewer
/// than two items on a layer, or all items share the same net, it returns
/// None for that layer. The final result is the minimum over all layers.
///
/// # Rules
/// - Only same-layer, different-net pairs are measured.
/// - The result is never negative; overlapping copper is a short, not a gap.
///
/// # Dependencies
/// - crates/rules/src/lib.rs (`copper`, `gap`)
///
/// # Do not
/// - Report a gap for same-net copper; that is connectivity, not clearance.
pub fn smallest_copper_gap(project: &Project) -> Option<f64> {
    let items = copper(project);
    let layer_count = project.copper_layer_count();
    let mut best: Option<f64> = None;

    for layer_index in 0..layer_count {
        let layer = match Layer::from_index(layer_index) {
            Ok(l) => l,
            Err(_) => continue,
        };
        let layer_items: Vec<&crate::Copper> = items.iter().filter(|i| i.layer == layer).collect();
        for i in 0..layer_items.len() {
            for j in (i + 1)..layer_items.len() {
                let a = layer_items[i];
                let b = layer_items[j];
                if a.net != b.net {
                    let g = gap(a, b);
                    if g >= 0.0 {
                        best = Some(match best {
                            Some(current) => current.min(g),
                            None => g,
                        });
                    }
                }
            }
        }
    }
    best
}

/// Thinnest solder mask bridge between two pad openings on the same side.
///
/// Measures the distance between solder mask openings (pad outlines grown
/// by their mask margins) on each side of the board. Returns the smallest
/// mask bridge found across both sides, or None when no pair of openings
/// exists on the same side.
///
/// # Rules
/// - Openings are measured by their mask-grown outlines, not pad copper.
/// - Only same-side pairs are compared.
///
/// # Dependencies
/// - crates/rules/src/pad_kinds.rs (`openings`, `ring_gap`)
///
/// # Do not
/// - Measure pad-to-pad copper distance; this is a mask clearance check.
pub fn smallest_mask_web(project: &Project) -> Option<f64> {
    let mut best: Option<f64> = None;
    for side in [Layer::Front, Layer::Back] {
        let openings = pad_kinds::openings(project, side);
        for i in 0..openings.len() {
            for j in (i + 1)..openings.len() {
                let a = &openings[i];
                let b = &openings[j];
                let web = pad_kinds::ring_gap(&a.ring, &b.ring) - a.margin - b.margin;
                best = Some(match best {
                    Some(current) => current.min(web),
                    None => web,
                });
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Footprint, Id, Pad, Project};
    use pcbcraft_geom::Point;
    use pcbcraft_library::FootprintPad;

    fn pad_at(
        id: Id,
        at: Point,
        layers: &[&str],
        polygon: Vec<Point>,
        net: Option<Id>,
    ) -> Footprint {
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
                net,
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

    const FRONT_CU: [&str; 1] = ["F.Cu"];
    const EXPOSED: [&str; 2] = ["F.Cu", "F.Mask"];

    #[test]
    fn measures_the_smallest_copper_gap_between_different_nets() -> Result<(), String> {
        let mut p = Project::default();
        // Two pads on different nets, 2mm apart (gap = 1mm)
        p.board.footprints.push(pad_at(
            1,
            Point::new(10.0, 10.0),
            &FRONT_CU,
            vec![],
            Some(1),
        ));
        p.board.footprints.push(pad_at(
            2,
            Point::new(12.0, 10.0),
            &FRONT_CU,
            vec![],
            Some(2),
        ));
        let gap = smallest_copper_gap(&p).ok_or("a gap between different nets")?;
        assert!((gap - 1.0).abs() < 0.01, "expected 1.0mm gap, got {gap}");
        Ok(())
    }

    #[test]
    fn ignores_same_net_copper() {
        let mut p = Project::default();
        // Two pads on the same net should not be measured
        p.board.footprints.push(pad_at(
            1,
            Point::new(10.0, 10.0),
            &FRONT_CU,
            vec![],
            Some(1),
        ));
        p.board.footprints.push(pad_at(
            2,
            Point::new(12.0, 10.0),
            &FRONT_CU,
            vec![],
            Some(1),
        ));
        let gap = smallest_copper_gap(&p);
        assert!(gap.is_none(), "same-net pads should not produce a gap");
    }

    #[test]
    fn returns_none_for_empty_board() {
        let p = Project::default();
        assert!(smallest_copper_gap(&p).is_none());
    }

    #[test]
    fn measures_mask_web_between_exposed_pads() -> Result<(), String> {
        let mut p = Project::default();
        // Two exposed pads 2mm apart: copper gap = 1mm, web = 1 - 0.05 - 0.05 = 0.9mm
        p.board
            .footprints
            .push(pad_at(1, Point::new(10.0, 10.0), &EXPOSED, vec![], None));
        p.board
            .footprints
            .push(pad_at(2, Point::new(12.0, 10.0), &EXPOSED, vec![], None));
        let web = smallest_mask_web(&p).ok_or("a web between exposed pads")?;
        assert!((web - 0.9).abs() < 0.01, "expected 0.9mm web, got {web}");
        Ok(())
    }

    #[test]
    fn returns_none_for_mask_web_with_no_openings() {
        let p = Project::default();
        assert!(smallest_mask_web(&p).is_none());
    }
}
