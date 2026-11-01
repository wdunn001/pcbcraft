//! Differential-pair skew and uncoupled-length checks beyond legacy length tolerance.
use pcbcraft_doc::{DiffPair, Id, Layer, Project, Track};
use pcbcraft_geom::impedance as imp;
use pcbcraft_geom::{Point, segment_distance};

use crate::Finding;

const DEFAULT_COPPER_MM: f64 = 0.035;
/// Edge spacing at or below this multiple of the pair gap counts as coupled.
const COUPLING_GAP_FACTOR: f64 = 3.0;
/// Step used when walking a track to find where it stops being coupled.
const UNCOUPLED_STEP_MM: f64 = 0.05;

fn dielectric_for(project: &Project, layer: Layer) -> (f64, f64) {
    project
        .dielectric_beside(&project.layer_name(layer))
        .unwrap_or((project.board.thickness, project.board.er))
}

fn copper_thickness(project: &Project, layer: Layer) -> f64 {
    let name = project.layer_name(layer);
    project
        .board
        .layer_records
        .iter()
        .find(|r| r.name == name && r.kind == "copper" && r.thickness > 0.0)
        .map(|r| r.thickness)
        .unwrap_or(DEFAULT_COPPER_MM)
}

fn track_delay_ps(project: &Project, track: &Track) -> Option<f64> {
    let (height, er) = dielectric_for(project, track.layer);
    let copper = copper_thickness(project, track.layer);
    let model = imp::microstrip(track.width, height, copper, er)?;
    Some(imp::delay_ps(track.length(), model.eeff))
}

fn net_delay_ps(project: &Project, net: Id) -> f64 {
    project
        .board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .filter_map(|t| track_delay_ps(project, t))
        .sum()
}

fn net_routed_length(tracks: &[Track], net: Id) -> f64 {
    tracks
        .iter()
        .filter(|t| t.net == net)
        .map(Track::length)
        .sum()
}

/// Longest stretch of `track` that has no mate copper within coupling range.
/// The track is walked in small steps so a leg that only touches the mate at
/// one end still counts as uncoupled along the rest of its length.
fn track_uncoupled_run(project: &Project, pair: &DiffPair, track: &Track, mate: Id) -> f64 {
    let length = track.length();
    if !length.is_finite() || length <= 0.0 {
        return 0.0;
    }
    let steps = ((length / UNCOUPLED_STEP_MM).ceil() as usize).clamp(1, 4000);
    let step = length / steps as f64;
    let at = |i: usize| {
        let t = i as f64 / steps as f64;
        Point::new(
            track.start.x + (track.end.x - track.start.x) * t,
            track.start.y + (track.end.y - track.start.y) * t,
        )
    };
    let mates: Vec<&Track> = project
        .board
        .tracks
        .iter()
        .filter(|o| o.net == mate && o.layer == track.layer)
        .collect();
    let coupled_at = |p: Point| {
        mates.iter().any(|o| {
            let separation =
                segment_distance(p, p, o.start, o.end) - track.width / 2.0 - o.width / 2.0;
            separation.is_finite() && separation + 1e-6 <= pair.gap * COUPLING_GAP_FACTOR
        })
    };
    let (mut longest, mut run) = (0.0_f64, 0.0_f64);
    for i in 0..steps {
        let mid = Point::new((at(i).x + at(i + 1).x) / 2.0, (at(i).y + at(i + 1).y) / 2.0);
        if coupled_at(mid) {
            run = 0.0;
        } else {
            run += step;
            longest = longest.max(run);
        }
    }
    longest
}

fn max_uncoupled_run(project: &Project, pair: &DiffPair) -> f64 {
    let mut longest = 0.0_f64;
    for track in &project.board.tracks {
        if track.net != pair.net_p && track.net != pair.net_n {
            continue;
        }
        let mate = if track.net == pair.net_p {
            pair.net_n
        } else {
            pair.net_p
        };
        longest = longest.max(track_uncoupled_run(project, pair, track, mate));
    }
    longest
}

pub(crate) fn pair_property_findings(project: &Project, pair: &DiffPair) -> Vec<Finding> {
    let mut out = Vec::new();
    let tracks = &project.board.tracks;
    if let Some(limit) = pair.skew_limit_mm {
        let skew =
            (net_routed_length(tracks, pair.net_p) - net_routed_length(tracks, pair.net_n)).abs();
        if skew > limit + 1e-9 {
            out.push(Finding {
                code: "pair_skew_exceeded",
                severity: project.rule_severity("pair_skew_exceeded", "error"),
                message: format!(
                    "{}: routed lengths differ by {:.3} mm, at most {:.3} mm allowed",
                    pair.name, skew, limit
                ),
                objects: vec![pair.net_p, pair.net_n],
                hint: None,
            });
        }
    }
    if let Some(limit) = pair.skew_limit_ps {
        let skew = (net_delay_ps(project, pair.net_p) - net_delay_ps(project, pair.net_n)).abs();
        if skew > limit + 1e-6 {
            out.push(Finding {
                code: "pair_skew_exceeded",
                severity: project.rule_severity("pair_skew_exceeded", "error"),
                message: format!(
                    "{}: propagation skew is {:.1} ps, at most {:.1} ps allowed",
                    pair.name, skew, limit
                ),
                objects: vec![pair.net_p, pair.net_n],
                hint: None,
            });
        }
    }
    if let Some(limit) = pair.max_uncoupled_length {
        let run = max_uncoupled_run(project, pair);
        if run > limit + 1e-9 {
            out.push(Finding {
                code: "pair_uncoupled_too_long",
                severity: project.rule_severity("pair_uncoupled_too_long", "error"),
                message: format!(
                    "{}: an uncoupled run is {:.3} mm, at most {:.3} mm allowed",
                    pair.name, run, limit
                ),
                objects: vec![pair.net_p, pair.net_n],
                hint: None,
            });
        }
    }
    out
}
