//! Differential-pair impedance target checks from edge-coupled microstrip estimates.
use crate::Finding;
use pcbcraft_doc::{Id, Layer, Project};
use pcbcraft_geom::impedance as imp;
use std::collections::BTreeMap;

fn dielectric_for(p: &Project, layer: Layer) -> (f64, f64) {
    p.dielectric_beside(&p.layer_name(layer))
        .unwrap_or((p.board.thickness, p.board.er))
}

fn dominant_layer(p: &Project, net_p: Id, net_n: Id) -> Option<Layer> {
    let mut per_layer: BTreeMap<Layer, f64> = BTreeMap::new();
    for track in p
        .board
        .tracks
        .iter()
        .filter(|t| t.net == net_p || t.net == net_n)
    {
        *per_layer.entry(track.layer).or_default() += track.length();
    }
    per_layer
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(layer, _)| layer)
}

fn routed_width_on_layer(p: &Project, net_p: Id, net_n: Id, layer: Layer) -> Option<f64> {
    p.board
        .tracks
        .iter()
        .filter(|t| t.layer == layer && (t.net == net_p || t.net == net_n))
        .map(|t| t.width)
        .fold(None, |best: Option<f64>, w| match best {
            Some(b) if b >= w => Some(b),
            _ => Some(w),
        })
}

fn estimate_diff_ohm(p: &Project, width: f64, gap: f64, layer: Layer) -> Option<f64> {
    let (height, er) = dielectric_for(p, layer);
    let coupled = imp::coupled_microstrip(width, gap, height, er)?;
    let z = coupled.z_diff();
    z.is_finite().then_some(z)
}

fn outside_tolerance(estimated: f64, target: f64, tolerance: f64) -> bool {
    let band = target * tolerance;
    (estimated - target).abs() > band + 1e-6
}

pub fn findings(project: &Project) -> Vec<Finding> {
    let mut result = Vec::new();
    for pair in &project.board.diff_pairs {
        let Some(target) = pair.target_impedance else {
            continue;
        };
        let tolerance = pair.impedance_tolerance.unwrap_or(0.0);
        let Some(layer) = dominant_layer(project, pair.net_p, pair.net_n) else {
            continue;
        };
        let Some(width) = routed_width_on_layer(project, pair.net_p, pair.net_n, layer) else {
            continue;
        };
        let Some(estimated) = estimate_diff_ohm(project, width, pair.gap, layer) else {
            continue;
        };
        if !outside_tolerance(estimated, target, tolerance) {
            continue;
        }
        let layer_name = project.layer_name(layer);
        result.push(Finding {
            code: "diffpair_impedance",
            severity: project.rule_severity("diffpair_impedance", "error"),
            message: format!(
                "{}: differential impedance on {layer_name} is about {estimated:.1} ohm, target {target:.1} ohm within {pct:.0}% tolerance (width {width:.3} mm, gap {gap:.3} mm)",
                pair.name,
                estimated = estimated,
                target = target,
                pct = tolerance * 100.0,
                width = width,
                gap = pair.gap,
            ),
            objects: vec![pair.net_p, pair.net_n],
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{DiffPair, Net, Track};

    fn usb_pair_project(width: f64) -> Project {
        let mut p = Project::default();
        p.board.thickness = 1.6;
        p.board.er = 4.5;
        p.nets.push(Net {
            id: 1,
            name: "USB_D+".into(),
            ..Default::default()
        });
        p.nets.push(Net {
            id: 2,
            name: "USB_D-".into(),
            ..Default::default()
        });
        p.board.diff_pairs.push(DiffPair {
            id: 10,
            name: "USB".into(),
            net_p: 1,
            net_n: 2,
            gap: 0.4,
            length_tolerance: None,
            profile: None,
            target_impedance: Some(90.0),
            impedance_tolerance: Some(0.1),
        });
        for (id, net) in [(100, 1), (101, 2)] {
            p.board.tracks.push(Track {
                id,
                net,
                layer: Layer::Front,
                width,
                start: pcbcraft_geom::Point::new(0.0, 0.0),
                end: pcbcraft_geom::Point::new(10.0, 0.0),
                ..Default::default()
            });
        }
        p
    }

    #[test]
    fn pair_with_target_flags_when_estimate_misses() {
        let p = usb_pair_project(2.0);
        let codes: Vec<_> = findings(&p).into_iter().map(|f| f.code).collect();
        assert!(codes.contains(&"diffpair_impedance"));
    }

    #[test]
    fn pair_at_target_width_passes() {
        let p = usb_pair_project(1.6);
        assert!(findings(&p).is_empty());
    }

    #[test]
    fn pair_without_target_is_silent() {
        let mut p = usb_pair_project(2.0);
        p.board.diff_pairs[0].target_impedance = None;
        p.board.diff_pairs[0].impedance_tolerance = None;
        assert!(findings(&p).is_empty());
    }
}
