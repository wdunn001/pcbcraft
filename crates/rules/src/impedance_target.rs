//! Impedance-target check: compare routed geometry to each net's target.
//!
//! Uses closed-form microstrip, stripline and edge-coupled pair models from
//! `pcbcraft_geom::impedance`, with dielectric height and Dk from stackup rows
//! when recorded, otherwise the board defaults.

use super::Finding;
use pcbcraft_doc::{DiffPair, Id, Layer, LayerRecord, Project, Track};
use pcbcraft_geom::impedance as imp;

const DEFAULT_COPPER_MM: f64 = 0.035;
const MODEL_NOTE: &str =
    "Closed-form estimate, typically within a few percent; the fab stackup governs.";

fn is_plane(rec: &LayerRecord) -> bool {
    matches!(
        rec.copper_role.as_deref().or(rec.function.as_deref()),
        Some("plane")
    )
}

fn copper_row(project: &Project, layer: Layer) -> Option<usize> {
    let name = project.layer_name(layer);
    project
        .board
        .layer_records
        .iter()
        .position(|r| r.kind == "copper" && r.name == name)
}

fn dielectric_between(project: &Project, lo: usize, hi: usize) -> (f64, f64) {
    let (a, b) = if lo < hi { (lo, hi) } else { (hi, lo) };
    let rows = &project.board.layer_records;
    let mut height = 0.0;
    let mut er_num = 0.0;
    let mut er_den = 0.0;
    for row in rows.iter().take(b).skip(a + 1) {
        if row.is_dielectric() && row.thickness > 0.0 {
            height += row.thickness;
            if let Some(dk) = row.dk() {
                er_num += dk * row.thickness;
                er_den += row.thickness;
            }
        } else if row.kind == "copper" && row.thickness > 0.0 {
            height += row.thickness;
        }
    }
    let er = if er_den > 0.0 {
        er_num / er_den
    } else {
        project.board.er
    };
    (height, er)
}

fn nearest_plane_row(project: &Project, signal_row: usize, step: i32) -> Option<usize> {
    let rows = &project.board.layer_records;
    let mut i = signal_row as i32 + step;
    while (0..rows.len() as i32).contains(&i) {
        let row = &rows[i as usize];
        if row.kind == "copper" {
            if is_plane(row) {
                return Some(i as usize);
            }
            return None;
        }
        i += step;
    }
    None
}

enum Geometry {
    Microstrip {
        method: &'static str,
        reference: String,
    },
    Stripline {
        method: &'static str,
        reference: String,
    },
    Coupled {
        method: &'static str,
        pair: String,
    },
}

struct StackSample {
    height: f64,
    er: f64,
    copper: f64,
    geometry: Geometry,
}

fn stack_sample(project: &Project, layer: Layer) -> Option<StackSample> {
    let copper = copper_thickness(project, layer);
    if project.board.layer_records.is_empty() {
        let (height, er) = project
            .dielectric_beside(&project.layer_name(layer))
            .unwrap_or((project.board.thickness, project.board.er));
        return Some(StackSample {
            height,
            er,
            copper,
            geometry: Geometry::Microstrip {
                method: "Hammerstad and Jensen microstrip",
                reference: "board reference".into(),
            },
        });
    }
    let sig = copper_row(project, layer)?;
    let below = nearest_plane_row(project, sig, 1);
    let above = nearest_plane_row(project, sig, -1);
    match (below, above) {
        (Some(b), Some(a)) if b != a => {
            let (height, er) = dielectric_between(project, a, b);
            if height <= 0.0 {
                return None;
            }
            let reference = format!(
                "{} and {}",
                project.board.layer_records[a].name,
                project.board.layer_records[b].name
            );
            Some(StackSample {
                height,
                er,
                copper,
                geometry: Geometry::Stripline {
                    method: "symmetric stripline",
                    reference,
                },
            })
        }
        (Some(b), _) => {
            let (height, er) = dielectric_between(project, sig, b);
            if height <= 0.0 {
                return None;
            }
            Some(StackSample {
                height,
                er,
                copper,
                geometry: Geometry::Microstrip {
                    method: "Hammerstad and Jensen microstrip",
                    reference: project.board.layer_records[b].name.clone(),
                },
            })
        }
        (_, Some(a)) => {
            let (height, er) = dielectric_between(project, sig, a);
            if height <= 0.0 {
                return None;
            }
            Some(StackSample {
                height,
                er,
                copper,
                geometry: Geometry::Microstrip {
                    method: "Hammerstad and Jensen microstrip",
                    reference: project.board.layer_records[a].name.clone(),
                },
            })
        }
        _ => {
            let (height, er) = project
                .dielectric_beside(&project.layer_name(layer))
                .unwrap_or((project.board.thickness, project.board.er));
            Some(StackSample {
                height,
                er,
                copper,
                geometry: Geometry::Microstrip {
                    method: "Hammerstad and Jensen microstrip",
                    reference: "nearest copper".into(),
                },
            })
        }
    }
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

fn pair_for_net<'a>(project: &'a Project, net: Id) -> Option<&'a DiffPair> {
    project
        .board
        .diff_pairs
        .iter()
        .find(|d| d.net_p == net || d.net_n == net)
}

fn outside_band(computed: f64, target: f64, tolerance_pct: f64) -> bool {
    let band = target * tolerance_pct / 100.0;
    computed < target - band - 1e-9 || computed > target + band + 1e-9
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn finding_message(
    project: &Project,
    net: Id,
    track: &Track,
    computed: f64,
    target: f64,
    sample: &StackSample,
) -> String {
    let layer = project.layer_name(track.layer);
    let (method, detail) = match &sample.geometry {
        Geometry::Microstrip { method, reference } => (*method, reference.as_str()),
        Geometry::Stripline { method, reference } => (*method, reference.as_str()),
        Geometry::Coupled { method, pair } => (*method, pair.as_str()),
    };
    format!(
        "{} is {:.2} ohm on {layer} over {detail}, target {:.2} ohm ({method}). {MODEL_NOTE}",
        project.net_name(net),
        r2(computed),
        r2(target),
    )
}

fn segment_impedance(
    project: &Project,
    track: &Track,
    pair: Option<&DiffPair>,
    sample: &StackSample,
) -> Option<(f64, StackSample)> {
    match (&sample.geometry, pair) {
        (_, Some(pair)) if pair.net_p == track.net => {
            let coupled = imp::coupled_microstrip(track.width, pair.gap, sample.height, sample.er)?;
            let mut sample = sample.clone();
            sample.geometry = Geometry::Coupled {
                method: "edge-coupled microstrip differential",
                pair: pair.name.clone(),
            };
            Some((coupled.z_diff(), sample))
        }
        (_, Some(_)) => None,
        (Geometry::Stripline { .. }, None) => imp::stripline(
            track.width,
            sample.height,
            sample.copper,
            sample.er,
        )
        .map(|z| (z, sample.clone())),
        (_, None) => imp::microstrip(track.width, sample.height, sample.copper, sample.er)
            .map(|m| (m.ohms, sample.clone())),
    }
}

impl Clone for StackSample {
    fn clone(&self) -> Self {
        Self {
            height: self.height,
            er: self.er,
            copper: self.copper,
            geometry: match &self.geometry {
                Geometry::Microstrip { method, reference } => Geometry::Microstrip {
                    method,
                    reference: reference.clone(),
                },
                Geometry::Stripline { method, reference } => Geometry::Stripline {
                    method,
                    reference: reference.clone(),
                },
                Geometry::Coupled { method, pair } => Geometry::Coupled {
                    method,
                    pair: pair.clone(),
                },
            },
        }
    }
}

/// Severity is `project.rule_severity("impedance_target", "error")`.
pub(crate) fn impedance_target_findings(project: &Project) -> Vec<Finding> {
    let mut out = Vec::new();
    for track in &project.board.tracks {
        let Some((target, tolerance_pct)) = project.impedance_target_for_net(track.net) else {
            continue;
        };
        let pair = pair_for_net(project, track.net);
        if pair.is_some_and(|p| p.net_n == track.net) {
            continue;
        }
        let Some(sample) = stack_sample(project, track.layer) else {
            continue;
        };
        let Some((computed, sample)) = segment_impedance(project, track, pair, &sample) else {
            continue;
        };
        if !outside_band(computed, target, tolerance_pct) {
            continue;
        }
        out.push(Finding {
            code: "impedance_target",
            severity: project.rule_severity("impedance_target", "error"),
            message: finding_message(project, track.net, track, computed, target, &sample),
            objects: vec![track.id],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{DiffPair, Net, Point};

    fn codes(p: &Project) -> Vec<&'static str> {
        impedance_target_findings(p)
            .into_iter()
            .map(|f| f.code)
            .collect()
    }

    fn four_layer_stack() -> Project {
        let mut p = Project::default();
        p.board.thickness = 1.6;
        p.board.er = 4.5;
        p.board.layer_records = vec![
            LayerRecord {
                name: "F.Cu".into(),
                kind: "copper".into(),
                thickness: 0.035,
                copper_role: Some("signal".into()),
                ..Default::default()
            },
            LayerRecord {
                name: "Prepreg1".into(),
                kind: "prepreg".into(),
                thickness: 0.2,
                dk: Some(4.5),
                ..Default::default()
            },
            LayerRecord {
                name: "In1.Cu".into(),
                kind: "copper".into(),
                thickness: 0.035,
                copper_role: Some("plane".into()),
                ..Default::default()
            },
            LayerRecord {
                name: "Core".into(),
                kind: "core".into(),
                thickness: 1.0,
                dk: Some(4.5),
                ..Default::default()
            },
            LayerRecord {
                name: "In2.Cu".into(),
                kind: "copper".into(),
                thickness: 0.035,
                copper_role: Some("plane".into()),
                ..Default::default()
            },
            LayerRecord {
                name: "Prepreg2".into(),
                kind: "prepreg".into(),
                thickness: 0.2,
                dk: Some(4.5),
                ..Default::default()
            },
            LayerRecord {
                name: "B.Cu".into(),
                kind: "copper".into(),
                thickness: 0.035,
                copper_role: Some("signal".into()),
                ..Default::default()
            },
        ];
        p.nets.push(Net {
            id: 1,
            name: "SIG".into(),
            target_impedance: Some(50.0),
            impedance_tolerance_pct: Some(10.0),
            ..Default::default()
        });
        p
    }

    fn add_track(p: &mut Project, id: Id, width: f64) {
        p.board.tracks.push(Track {
            id,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(40.0, 10.0),
            width,
            layer: Layer::Front,
            arc: None,
        });
    }

    #[test]
    fn wrong_width_flags_and_target_width_passes_on_four_layer() -> Result<(), String> {
        let mut p = four_layer_stack();
        let sample = stack_sample(&p, Layer::Front).ok_or("stack sample")?;
        let good = imp::width_for_ohms(50.0, |w| {
            imp::microstrip(w, sample.height, sample.copper, sample.er).map(|m| m.ohms)
        })
        .ok_or("solve 50 ohm width")?;
        add_track(&mut p, 10, 0.15);
        p.validate()?;
        assert!(codes(&p).contains(&"impedance_target"));
        p.board.tracks.clear();
        add_track(&mut p, 11, good);
        p.validate()?;
        assert!(!codes(&p).contains(&"impedance_target"));
        Ok(())
    }

    #[test]
    fn differential_pair_wrong_gap_flags() -> Result<(), String> {
        let mut p = four_layer_stack();
        p.nets[0].target_impedance = Some(100.0);
        p.nets.push(Net {
            id: 2,
            name: "SIGN".into(),
            ..Default::default()
        });
        p.board.diff_pairs.push(DiffPair {
            id: 3,
            name: "USB".into(),
            net_p: 1,
            net_n: 2,
            gap: 0.15,
            length_tolerance: None,
            profile: None,
        });
        let width = 0.3;
        for (id, y) in [(10, 10.0), (11, 10.0 + width + 0.15)] {
            p.board.tracks.push(Track {
                id,
                net: if id == 10 { 1 } else { 2 },
                start: Point::new(10.0, y),
                end: Point::new(40.0, y),
                width,
                layer: Layer::Front,
                arc: None,
            });
        }
        p.validate()?;
        assert!(codes(&p).contains(&"impedance_target"));
        p.board.diff_pairs[0].gap = 0.85;
        assert!(!codes(&p).contains(&"impedance_target"));
        Ok(())
    }

    #[test]
    fn no_target_means_no_report() -> Result<(), String> {
        let mut p = four_layer_stack();
        p.nets[0].target_impedance = None;
        add_track(&mut p, 10, 0.15);
        p.validate()?;
        assert!(codes(&p).is_empty());
        Ok(())
    }
}
