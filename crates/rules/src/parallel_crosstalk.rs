//! Parallel-run crosstalk estimate and rise-time electrically-long check.
//!
//! Context: called from `drc` after stub checks. Uses stackup dielectric height
//! beside each copper layer and the crosstalk rule of thumb in `pcbcraft-calc`.
//!
//! Local rules:
//! - Only nets in a class with `rise_time` participate.
//! - Parallel runs closer than three trace widths (edge to edge) with coupled
//!   length above one sixth of the rise-time edge length are reported.
//! - Total routed length above that same edge fraction is reported as
//!   electrically long. Without a rise time, neither check runs for the net.
use pcbcraft_calc::impedance::{self, Crosstalk, PropagationDelay};
use pcbcraft_doc::{Id, Layer, Project, Track};
use pcbcraft_geom::impedance as imp;
use pcbcraft_geom::{Point, segment_distance};

use crate::Finding;

const DEFAULT_COPPER_MM: f64 = 0.035;
const THREE_W: f64 = 3.0;
const EDGE_FRACTION: f64 = 1.0 / 6.0;
/// Sine of the largest angle still treated as parallel.
const PARALLEL_SIN: f64 = 0.05;

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

fn rise_time_for_net(project: &Project, net: Id) -> Option<f64> {
    project.net_class(net).and_then(|c| c.rise_time)
}

fn edge_length_mm(project: &Project, rise_time: f64, width: f64, layer: Layer) -> Option<f64> {
    let (height, er) = dielectric_for(project, layer);
    let copper = copper_thickness(project, layer);
    let model = imp::microstrip(width, height, copper, er)?;
    let out = impedance::propagation_delay(PropagationDelay {
        eeff: model.eeff,
        length: None,
        skew_budget: None,
    })
    .ok()?;
    let tr_ps = rise_time * 1e12;
    if out.delay <= 0.0 {
        return None;
    }
    Some(tr_ps / out.delay)
}

fn electrically_long_limit(edge: f64) -> f64 {
    edge * EDGE_FRACTION
}

fn parallel_overlap(a: Point, b: Point, c: Point, d: Point) -> Option<f64> {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let len_ab = abx.hypot(aby);
    if len_ab < 1e-9 {
        return None;
    }
    let cdx = d.x - c.x;
    let cdy = d.y - c.y;
    let len_cd = cdx.hypot(cdy);
    if len_cd < 1e-9 {
        return None;
    }
    let cross = abx * cdy - aby * cdx;
    if cross.abs() > PARALLEL_SIN * len_ab * len_cd {
        return None;
    }
    let ux = abx / len_ab;
    let uy = aby / len_ab;
    let proj = |p: Point| (p.x - a.x) * ux + (p.y - a.y) * uy;
    let seg1 = (0.0_f64, len_ab);
    let t_c = proj(c);
    let t_d = proj(d);
    let seg2 = (t_c.min(t_d), t_c.max(t_d));
    let lo = seg1.0.max(seg2.0);
    let hi = seg1.1.min(seg2.1);
    if hi - lo > 1e-6 { Some(hi - lo) } else { None }
}

fn coupling_estimate(project: &Project, gap: f64, width: f64, layer: Layer) -> Option<(f64, f64)> {
    let (height, _) = dielectric_for(project, layer);
    let out = impedance::crosstalk(Crosstalk { gap, height, width }).ok()?;
    Some((out.coupling * 100.0, out.coupling_db))
}

fn net_routed_length(tracks: &[Track], net: Id) -> f64 {
    tracks
        .iter()
        .filter(|t| t.net == net)
        .map(Track::length)
        .sum()
}

pub(crate) fn parallel_crosstalk_findings(project: &Project) -> Vec<Finding> {
    let mut result = vec![];
    let tracks = &project.board.tracks;
    let mut reported: std::collections::BTreeSet<(Id, Id, Id, Id)> =
        std::collections::BTreeSet::new();

    for layer in [Layer::Front, Layer::Back] {
        let layer_tracks: Vec<&Track> = tracks.iter().filter(|t| t.layer == layer).collect();
        for i in 0..layer_tracks.len() {
            for j in (i + 1)..layer_tracks.len() {
                let a = layer_tracks[i];
                let b = layer_tracks[j];
                if a.net == b.net {
                    continue;
                }
                let tr_a = rise_time_for_net(project, a.net);
                let tr_b = rise_time_for_net(project, b.net);
                let rise_time = match (tr_a, tr_b) {
                    (Some(ta), Some(tb)) => ta.min(tb),
                    (Some(t), None) | (None, Some(t)) => t,
                    (None, None) => continue,
                };
                let Some(overlap) = parallel_overlap(a.start, a.end, b.start, b.end) else {
                    continue;
                };
                let gap = segment_distance(a.start, a.end, b.start, b.end)
                    - a.width / 2.0
                    - b.width / 2.0;
                if !gap.is_finite() {
                    continue;
                }
                let ref_width = a.width.max(b.width);
                if gap + 1e-6 >= THREE_W * ref_width {
                    continue;
                }
                let width_for_edge = ref_width;
                let Some(edge) = edge_length_mm(project, rise_time, width_for_edge, layer) else {
                    continue;
                };
                let coupled_limit = electrically_long_limit(edge);
                if overlap <= coupled_limit + 1e-9 {
                    continue;
                }
                let key = (
                    a.net.min(b.net),
                    a.net.max(b.net),
                    a.id.min(b.id),
                    a.id.max(b.id),
                );
                if !reported.insert(key) {
                    continue;
                }
                let coupling = coupling_estimate(project, gap.max(0.01), ref_width, layer);
                let names = format!("{} / {}", project.net_name(a.net), project.net_name(b.net));
                let message = match coupling {
                    Some((pct, db)) => format!(
                        "{names}: parallel run estimate {:.3} mm side by side at {:.3} mm gap, coupling estimate {:.2} % ({:.1} dB)",
                        overlap, gap, pct, db
                    ),
                    None => format!(
                        "{names}: parallel run estimate {:.3} mm side by side at {:.3} mm gap",
                        overlap, gap
                    ),
                };
                result.push(Finding {
                    code: "parallel_crosstalk",
                    severity: project.rule_severity("parallel_crosstalk", "warning"),
                    message,
                    objects: vec![a.id, b.id],
                });
            }
        }
    }

    let mut long_reported: std::collections::BTreeSet<Id> = std::collections::BTreeSet::new();
    for net in project.nets.iter().map(|n| n.id) {
        let Some(rise_time) = rise_time_for_net(project, net) else {
            continue;
        };
        let routed = net_routed_length(tracks, net);
        let sample = tracks.iter().find(|t| t.net == net);
        let width = sample.map(|t| t.width).or_else(|| {
            project
                .net_class(net)
                .map(|c| c.width)
                .filter(|w| w.is_finite() && *w > 0.0)
        });
        let Some(width) = width else {
            continue;
        };
        let layer = sample.map(|t| t.layer).unwrap_or(Layer::Front);
        let Some(edge) = edge_length_mm(project, rise_time, width, layer) else {
            continue;
        };
        let limit = electrically_long_limit(edge);
        if routed <= limit + 1e-9 {
            continue;
        }
        if !long_reported.insert(net) {
            continue;
        }
        result.push(Finding {
            code: "net_electrically_long",
            severity: project.rule_severity("net_electrically_long", "warning"),
            message: format!(
                "{} routed {:.3} mm; estimate treats tracks longer than {:.3} mm as electrically long for this rise time",
                project.net_name(net),
                routed,
                limit
            ),
            objects: vec![net],
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drc;
    use pcbcraft_doc::{Net, NetClass, Project, Track};

    fn clock_pair(gap_y: f64) -> Project {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "CLK".into(),
            ..Default::default()
        });
        p.nets.push(Net {
            id: 2,
            name: "DATA".into(),
            ..Default::default()
        });
        let width = 0.2;
        p.board.tracks.push(Track {
            id: 10,
            net: 1,
            start: Point::new(0.0, 0.0),
            end: Point::new(10.0, 0.0),
            width,
            layer: Layer::Front,
            arc: None,
        });
        p.board.tracks.push(Track {
            id: 11,
            net: 2,
            start: Point::new(0.0, width + gap_y),
            end: Point::new(10.0, width + gap_y),
            width,
            layer: Layer::Front,
            arc: None,
        });
        p.net_classes.push(NetClass {
            id: 50,
            name: "CLK".into(),
            clearance: 0.2,
            width,
            min_width: None,
            max_width: None,
            target_length: None,
            length_tolerance: None,
            max_stub: None,
            via_diameter: None,
            via_drill: None,
            pair_width: None,
            pair_gap: None,
            rise_time: Some(1e-9),
            nets: vec![1, 2],
            profile: None,
        });
        p.validate().expect("valid fixture");
        p
    }

    fn parallel_findings(p: &Project) -> Vec<Finding> {
        drc(p)
            .into_iter()
            .filter(|f| f.code == "parallel_crosstalk")
            .collect()
    }

    fn long_findings(p: &Project) -> Vec<Finding> {
        drc(p)
            .into_iter()
            .filter(|f| f.code == "net_electrically_long")
            .collect()
    }

    #[test]
    fn one_width_gap_parallel_run_is_flagged() {
        let p = clock_pair(0.2);
        assert_eq!(parallel_findings(&p).len(), 1);
        assert!(parallel_findings(&p)[0].message.contains("estimate"));
    }

    #[test]
    fn three_width_gap_parallel_run_passes() {
        let p = clock_pair(0.6);
        assert!(parallel_findings(&p).is_empty());
    }

    #[test]
    fn electrically_long_net_is_flagged_with_rise_time() {
        let mut p = clock_pair(2.0);
        p.board.tracks[0].end = Point::new(80.0, 0.0);
        p.board.tracks[1].end = Point::new(80.0, p.board.tracks[1].width + 2.0);
        p.validate().expect("valid");
        assert!(!long_findings(&p).is_empty());
        assert!(long_findings(&p)[0].message.contains("electrically long"));
    }

    #[test]
    fn without_rise_time_electrically_long_is_quiet() {
        let mut p = clock_pair(2.0);
        p.net_classes[0].rise_time = None;
        p.board.tracks[0].end = Point::new(80.0, 0.0);
        p.validate().expect("valid");
        assert!(long_findings(&p).is_empty());
        assert!(parallel_findings(&p).is_empty());
    }
}
