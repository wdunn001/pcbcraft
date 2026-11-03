//! Checks for exposed and notched pads: mask webs between openings and notches
//! too fine to etch.
//!
//! Context: issue 1209. Called once from `drc` in `lib.rs`. The board edge
//! check already measures every pad by its real outline, so a notched pad is
//! held to the edge by its true shape; this file adds the two rules that only
//! the new kinds need.
//!
//! # Rules
//! - `mask_web` looks only at pairs where one pad is exposed or notched, so a
//!   fine-pitch part that was fine before stays quiet. The gap is measured
//!   between the mask openings (outline grown by the mask margin, 0.05 mm when
//!   the footprint states none) on the same side.
//! - `notch_feature` measures the narrowest part of a notched pad: the gap
//!   between two edges of its polygon that do not touch. The limit is the
//!   board's `min_track`, or `DEFAULT_MIN_FEATURE_MM` without fabrication rules.
//!
//! # Dependencies
//! - crates/rules/src/lib.rs (`Finding`, `drc`)
//! - crates/library/src/pad_kind.rs (`FootprintPad::kind`)
//! - crates/geom/src/lib.rs (`segment_distance`, `point_in_polygon`)
//!
//! # Do not
//! - Flag plain pads here; the existing clearance checks own them.
use super::Finding;
use pcbcraft_doc::{Footprint, Id, Layer, Pad, Project};
use pcbcraft_geom::{Point, point_in_polygon, segment_distance};
use pcbcraft_library::PadKind;

/// Thinnest mask web a fabricator holds between two openings, in millimetres.
const MASK_WEB_MM: f64 = 0.1;
/// Mask opening growth when the footprint states no margin, in millimetres.
const DEFAULT_MARGIN_MM: f64 = 0.05;
/// Narrowest notch or arm without fabrication rules, in millimetres.
const DEFAULT_MIN_FEATURE_MM: f64 = 0.1;

pub(super) struct Opening {
    pub(super) footprint: Id,
    pub(super) number: String,
    pub(super) special: bool,
    pub(super) margin: f64,
    pub(super) ring: Vec<Point>,
}

pub(super) fn margin_of(project: &Project, f: &Footprint) -> f64 {
    project
        .component(f.component)
        .and_then(|c| c.library_footprint.as_ref())
        .and_then(|l| l.definition.rules.as_ref())
        .and_then(|r| r.mask_margin)
        .filter(|m| m.is_finite() && m.abs() <= 1.0)
        .unwrap_or(DEFAULT_MARGIN_MM)
}

fn open_on(f: &Footprint, pad: &Pad, side: Layer) -> bool {
    if pad.geometry.is_none() {
        return side == Layer::Front;
    }
    let layers = f.pad_geometry_layers(pad);
    let wanted = if side == Layer::Back {
        "B.Mask"
    } else {
        "F.Mask"
    };
    layers.iter().any(|l| l == wanted || l == "*.Mask")
}

pub(super) fn openings(project: &Project, side: Layer) -> Vec<Opening> {
    let mut out = vec![];
    for f in &project.board.footprints {
        let margin = margin_of(project, f);
        for pad in f.pads.iter().filter(|p| open_on(f, p, side)) {
            let mut ring: Vec<Point> = pad
                .outline_points()
                .into_iter()
                .map(|v| f.position.offset(v.rotate(-f.rotation)))
                .collect();
            if ring.len() > 1 && ring.first() == ring.last() {
                ring.pop();
            }
            if ring.len() < 3 {
                continue;
            }
            out.push(Opening {
                footprint: f.id,
                number: pad.number.clone(),
                special: pad
                    .geometry
                    .as_ref()
                    .is_some_and(|g| g.kind() != PadKind::Plain),
                margin,
                ring,
            });
        }
    }
    out
}

pub(super) fn ring_gap(a: &[Point], b: &[Point]) -> f64 {
    if a.iter().any(|p| point_in_polygon(*p, b)) || b.iter().any(|p| point_in_polygon(*p, a)) {
        return 0.0;
    }
    let mut best = f64::INFINITY;
    for i in 0..a.len() {
        let (p, q) = (a[i], a[(i + 1) % a.len()]);
        for j in 0..b.len() {
            best = best.min(segment_distance(p, q, b[j], b[(j + 1) % b.len()]));
        }
    }
    best
}

/// `mask_web` and `notch_feature` findings for exposed and notched pads.
///
/// # Rules
/// - `mask_web` is a warning, `notch_feature` an error, both through
///   `project.rule_severity` so a design can retune them.
/// - One finding per pair of openings, and one per notched pad.
///
/// # Dependencies
/// - crates/rules/src/lib.rs (`drc` calls this once)
///
/// # Do not
/// - Report a pair whose openings overlap; copper touching is a clearance matter.
pub(crate) fn pad_kind_findings(project: &Project) -> Vec<Finding> {
    let mut result = vec![];
    for side in [Layer::Front, Layer::Back] {
        let list = openings(project, side);
        for (i, a) in list.iter().enumerate() {
            for b in list.iter().skip(i + 1) {
                if !(a.special || b.special) || (a.footprint == b.footprint && a.number == b.number)
                {
                    continue;
                }
                let copper = ring_gap(&a.ring, &b.ring);
                if copper <= 0.0 {
                    continue;
                }
                let web = copper - a.margin - b.margin;
                if web + 1e-6 < MASK_WEB_MM {
                    result.push(Finding {
                        code: "mask_web",
                        severity: project.rule_severity("mask_web", "warning"),
                        message: format!(
                            "Mask between pad {} and pad {} is {:.3} mm; fabrication holds {:.2} mm and merges thinner webs into one opening",
                            a.number,
                            b.number,
                            web.max(0.0),
                            MASK_WEB_MM
                        ),
                        objects: vec![a.footprint, b.footprint],
                        hint: Some("Move the pads apart or cover the mask between them.".into()),
                    });
                }
            }
        }
    }
    let limit = project
        .board
        .rules
        .as_ref()
        .and_then(|r| r.min_track)
        .unwrap_or(DEFAULT_MIN_FEATURE_MM);
    for f in &project.board.footprints {
        for pad in &f.pads {
            let Some(g) = pad
                .geometry
                .as_ref()
                .filter(|g| g.kind() == PadKind::Notched)
            else {
                continue;
            };
            let ring = &g.polygon;
            let n = ring.len();
            let mut narrowest = f64::INFINITY;
            for i in 0..n {
                for j in (i + 2)..n {
                    if i == 0 && j == n - 1 {
                        continue;
                    }
                    narrowest = narrowest.min(segment_distance(
                        ring[i],
                        ring[(i + 1) % n],
                        ring[j],
                        ring[(j + 1) % n],
                    ));
                }
            }
            if narrowest.is_finite() && narrowest + 1e-6 < limit {
                result.push(Finding {
                    code: "notch_feature",
                    severity: project.rule_severity("notch_feature", "error"),
                    message: format!(
                        "Pad {} has a notch or arm only {:.3} mm wide; fabrication needs at least {:.3} mm",
                        pad.number, narrowest, limit
                    ),
                    objects: vec![f.id],
                    hint: Some("Make the notch wider or shallower.".into()),
                });
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_library::{FootprintPad, NotchPlace, notched_rectangle};

    fn pad_at(id: Id, at: Point, layers: &[&str], polygon: Vec<Point>) -> Footprint {
        let custom = !polygon.is_empty();
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
                    shape: if custom { "custom" } else { "rect" }.into(),
                    size: Point::new(1.0, 1.0),
                    layers: layers.iter().map(|l| (*l).to_owned()).collect(),
                    polygon,
                    ..FootprintPad::default()
                }),
            }],
            preserved: pcbcraft_doc::PreservedSource::default(),
        }
    }

    const EXPOSED: [&str; 2] = ["F.Cu", "F.Mask"];
    const PLAIN: [&str; 3] = ["F.Cu", "F.Paste", "F.Mask"];

    fn codes(p: &Project) -> Vec<&'static str> {
        pad_kind_findings(p).iter().map(|f| f.code).collect()
    }

    #[test]
    fn flags_a_thin_mask_web_beside_an_exposed_pad() {
        let mut p = Project::default();
        p.board
            .footprints
            .push(pad_at(1, Point::new(10.0, 10.0), &EXPOSED, vec![]));
        p.board
            .footprints
            .push(pad_at(2, Point::new(11.1, 10.0), &PLAIN, vec![]));
        assert!(
            codes(&p).contains(&"mask_web"),
            "a 0.1 mm copper gap leaves no web"
        );
        p.board.footprints[1].position = Point::new(12.0, 10.0);
        assert!(!codes(&p).contains(&"mask_web"), "a 1 mm gap leaves a web");
    }

    #[test]
    fn leaves_two_plain_pads_alone() {
        let mut p = Project::default();
        p.board
            .footprints
            .push(pad_at(1, Point::new(10.0, 10.0), &PLAIN, vec![]));
        p.board
            .footprints
            .push(pad_at(2, Point::new(11.1, 10.0), &PLAIN, vec![]));
        assert!(codes(&p).is_empty());
    }

    #[test]
    fn flags_a_notch_finer_than_the_fabrication_limit() -> Result<(), String> {
        let mut p = Project::default();
        let (fine, _) = notched_rectangle(2.0, 1.4, NotchPlace::EdgeN, 0.06, 0.4)?;
        p.board
            .footprints
            .push(pad_at(1, Point::new(10.0, 10.0), &PLAIN, fine));
        assert!(
            codes(&p).contains(&"notch_feature"),
            "a 0.06 mm slot is too fine"
        );
        let (wide, _) = notched_rectangle(2.0, 1.4, NotchPlace::EdgeN, 0.6, 0.4)?;
        p.board.footprints[0].pads[0]
            .geometry
            .as_mut()
            .ok_or("no geometry")?
            .polygon = wide;
        assert!(
            !codes(&p).contains(&"notch_feature"),
            "a 0.6 mm notch is fine"
        );
        Ok(())
    }
}
