//! Track endpoints landing on a pad their layer cannot reach.
//!
//! Context: issue 1882. Called once from `drc` in `lib.rs`. The connectivity
//! model already refuses to join copper on two different layers, so a case
//! like this does surface, as `unrouted` on the whole net. That finding names
//! the two pads it could not join. It names neither the track that was meant
//! to join them nor the layer that stopped it, and a net drawn end to end on
//! screen reads as finished. This file adds the finding that says which track
//! and which layer.
//!
//! # Rules
//! - One finding per track endpoint sitting inside a pad of the same net that
//!   has no copper on the track's layer.
//! - A plated pad reaches every copper layer, so it is never reported.
//! - A via of the same net covering the same endpoint silences the finding.
//!   That is the pattern of leaving a pad on its own layer and changing layer
//!   right there, which is sound.
//!
//! # Dependencies
//! - crates/rules/src/lib.rs (`Finding`, `drc` calls this once)
//! - crates/doc/src/lib.rs (`Footprint::pad_on_copper`,
//!   `Footprint::pad_world_position`, `Pad::contact_radius`)
//! - crates/geom/src/lib.rs (`point_in_polygon`)
//! - crates/doc/src/lib.rs (`Project::layer_name`)
//!
//! # Do not
//! - Report a track that merely passes near a pad. Only an endpoint counts,
//!   because only an endpoint claims a connection.
use super::Finding;
use pcbcraft_doc::{Footprint, Pad, Project};
use pcbcraft_geom::{Point, point_in_polygon};

/// Slack on the endpoint test, in millimetres.
const TOUCH_MM: f64 = 1e-6;

/// Whether `at` lands on the copper of `pad` as placed.
fn on_pad(f: &Footprint, pad: &Pad, at: Point) -> bool {
    let ring: Vec<Point> = pad
        .outline_points()
        .into_iter()
        .map(|p| f.position.offset(p.rotate(-f.rotation)))
        .collect();
    if ring.len() >= 3 {
        return point_in_polygon(at, &ring);
    }
    let centre = f.pad_world_position(pad);
    let reach = pad.contact_radius();
    reach > 0.0 && centre.distance(at) <= reach + TOUCH_MM
}

/// Whether a via of `net` covers `at` and carries copper on both layers.
fn via_covers(project: &Project, net: pcbcraft_doc::Id, at: Point) -> bool {
    project.board.vias.iter().any(|via| {
        via.net == net && via.diameter > 0.0 && via.position.distance(at) <= via.diameter / 2.0
    })
}

/// `track_off_pad_layer` findings.
///
/// # Rules
/// - Default severity is an error. The track connects nothing, so the board
///   as drawn does not work.
///
/// # Dependencies
/// - crates/rules/src/lib.rs (`drc` calls this once)
pub(crate) fn track_pad_layer_findings(project: &Project) -> Vec<Finding> {
    if project.rule_disabled("track_off_pad_layer") {
        return vec![];
    }
    let mut result = vec![];
    for track in project.board.tracks.iter().filter(|t| t.net != 0) {
        for at in [track.start, track.end] {
            for f in &project.board.footprints {
                for pad in f
                    .pads
                    .iter()
                    .filter(|pad| pad.net == Some(track.net) && !f.pad_on_copper(pad, track.layer))
                {
                    if !on_pad(f, pad, at) || via_covers(project, track.net, at) {
                        continue;
                    }
                    result.push(Finding {
                        code: "track_off_pad_layer",
                        severity: project.rule_severity("track_off_pad_layer", "error"),
                        message: format!(
                            "{} track on {} ends on pad {}, which has copper only on {}; the two do not connect",
                            project.net_name(track.net),
                            project.layer_name(track.layer),
                            pad.number,
                            project.layer_name(f.board_side()),
                        ),
                        objects: vec![track.id, f.id],
                        hint: Some(
                            "Move the track to the pad's own layer, or leave the pad on its layer and place a via where the layer changes."
                                .into(),
                        ),
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
    use pcbcraft_doc::{Id, Layer, Net, PreservedSource, Track, Via};
    use pcbcraft_library::FootprintPad;

    /// One pad of net 1 at `at`, surface mount on the front unless `plated`.
    fn pad_at(id: Id, at: Point, plated: bool) -> Footprint {
        Footprint {
            id,
            component: id,
            position: at,
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 1.6,
                net: Some(1),
                castellated: None,
                geometry: Some(FootprintPad {
                    number: "1".into(),
                    pad_type: if plated { "thru_hole" } else { "smd" }.into(),
                    shape: "circle".into(),
                    size: Point::new(1.6, 1.6),
                    layers: if plated {
                        vec!["*.Cu".into(), "*.Mask".into()]
                    } else {
                        vec!["F.Cu".into(), "F.Mask".into()]
                    },
                    drill: if plated {
                        Some(pcbcraft_library::Drill {
                            size: Point::new(0.8, 0.8),
                            offset: Point::default(),
                        })
                    } else {
                        None
                    },
                    ..FootprintPad::default()
                }),
            }],
            preserved: PreservedSource::default(),
        }
    }

    /// Two pads of net 1 at x 10 and x 30, joined by one track on `layer`.
    fn two_pads_one_track(plated: bool, layer: Layer) -> Project {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "SIG".into(),
            ..Default::default()
        });
        p.board
            .footprints
            .push(pad_at(1, Point::new(10.0, 10.0), plated));
        p.board
            .footprints
            .push(pad_at(2, Point::new(30.0, 10.0), plated));
        p.board.tracks.push(Track {
            id: 20,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(30.0, 10.0),
            width: 0.25,
            layer,
            arc: None,
            preserved: PreservedSource::default(),
        });
        p
    }

    fn codes(p: &Project) -> Vec<&'static str> {
        track_pad_layer_findings(p).iter().map(|f| f.code).collect()
    }

    #[test]
    fn names_the_track_and_the_layer_when_a_back_track_ends_on_a_front_pad() {
        let p = two_pads_one_track(false, Layer::Back);
        let found = track_pad_layer_findings(&p);
        assert_eq!(found.len(), 2, "one finding per endpoint: {found:?}");
        let first = found.first().map(|f| f.message.clone()).unwrap_or_default();
        assert!(first.contains("SIG"), "the net is named: {first}");
        assert!(first.contains("pad 1"), "the pad is named: {first}");
        assert!(
            found.first().map(|f| f.objects.contains(&20)) == Some(true),
            "the track is one of the objects: {found:?}"
        );
    }

    #[test]
    fn leaves_a_front_track_on_a_front_pad_alone() {
        assert!(codes(&two_pads_one_track(false, Layer::Front)).is_empty());
    }

    #[test]
    fn leaves_a_back_track_on_plated_pads_alone() {
        assert!(
            codes(&two_pads_one_track(true, Layer::Back)).is_empty(),
            "a plated pad reaches every copper layer"
        );
    }

    #[test]
    fn a_via_on_the_pad_silences_the_finding() {
        let mut p = two_pads_one_track(false, Layer::Back);
        for (id, x) in [(30, 10.0), (31, 30.0)] {
            p.board.vias.push(Via {
                id,
                net: 1,
                position: Point::new(x, 10.0),
                diameter: 0.6,
                drill: 0.3,
                preserved: PreservedSource::default(),
                from_layer: None,
                to_layer: None,
                covering: None,
                covering_bottom: None,
                backdrill: None,
            });
        }
        assert!(
            codes(&p).is_empty(),
            "a via where the layer changes is a real connection"
        );
    }

    #[test]
    fn leaves_a_track_that_only_passes_near_a_pad_alone() {
        let mut p = two_pads_one_track(false, Layer::Back);
        p.board.tracks[0].start = Point::new(10.0, 20.0);
        p.board.tracks[0].end = Point::new(30.0, 20.0);
        assert!(codes(&p).is_empty(), "only an endpoint claims a connection");
    }
}
