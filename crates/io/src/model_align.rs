//! Fit a 3D model transform to a footprint's pads and detect terminal pads on the lowest face.
//!
//! Footprint coordinates are millimetres, y down. Model coordinates follow [`model_matrix`].

use crate::board_mesh::{apply_matrix, model_matrix};
use crate::model_load::LoadedModel;
use pcbcraft_geom::Point;
use pcbcraft_library::{FootprintDefinition, Model3D, pad_centres, pad_centroid};

/// Residual above this is reported as a coarse alignment in plain words.
pub const COARSE_ALIGN_MM: f64 = 0.15;

/// Result of an auto-align search.
#[derive(Clone, Debug, PartialEq)]
pub struct AutoAlignResult {
    pub model: Model3D,
    /// Root-mean-square pad mismatch after the fit, in millimetres.
    pub residual_mm: f64,
    /// True when [`COARSE_ALIGN_MM`] is exceeded.
    pub coarse: bool,
    /// True when lead terminals could not be matched and the bounding-box fallback ran.
    pub bbox_fallback: bool,
}

fn normalize_degrees(v: f64) -> f64 {
    let mut d = v % 360.0;
    if d < 0.0 {
        d += 360.0;
    }
    d
}

fn footprint_xy(model: &Model3D, local: [f64; 3]) -> Point {
    let m = model_matrix(model, Point::default(), 0.0, false, 0.0);
    let w = apply_matrix(&m, local);
    Point::new(w[0], w[1])
}

fn model_bbox_centre_xy(model: &Model3D, mesh: &LoadedModel) -> Point {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &mesh.positions {
        let w = footprint_xy(model, *p);
        lo[0] = lo[0].min(w.x);
        lo[1] = lo[1].min(w.y);
        hi[0] = hi[0].max(w.x);
        hi[1] = hi[1].max(w.y);
    }
    Point::new((lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0)
}

/// Cluster bottom-face vertices into terminal centres in model-local coordinates.
pub fn model_lead_points(mesh: &LoadedModel) -> Vec<[f64; 3]> {
    let Some((lo, _hi)) = mesh.bounds() else {
        return Vec::new();
    };
    let z_cut = lo[2] + 0.05;
    let mut bottom: Vec<[f64; 3]> = mesh
        .positions
        .iter()
        .copied()
        .filter(|p| p[2] <= z_cut)
        .collect();
    if bottom.len() < 4 {
        return Vec::new();
    }
    let cell = 0.25_f64;
    let mut buckets: std::collections::BTreeMap<(i32, i32), (f64, f64, f64, u32)> =
        std::collections::BTreeMap::new();
    for p in &bottom {
        let key = (
            (p[0] / cell).floor() as i32,
            (p[1] / cell).floor() as i32,
        );
        let e = buckets.entry(key).or_insert((0.0, 0.0, 0.0, 0));
        e.0 += p[0];
        e.1 += p[1];
        e.2 += p[2];
        e.3 += 1;
    }
    let mut clusters: Vec<[f64; 3]> = buckets
        .into_values()
        .filter(|(_, _, _, n)| *n >= 3)
        .map(|(sx, sy, sz, n)| {
            let n = n as f64;
            [sx / n, sy / n, sz / n]
        })
        .collect();
    clusters.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    if clusters.len() > 64 {
        clusters.truncate(64);
    }
    bottom.clear();
    clusters
}

fn sort_by_x(points: &mut [Point]) {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
}

fn rms_match(leads: &[Point], pads: &[Point]) -> f64 {
    if leads.is_empty() || pads.is_empty() || leads.len() != pads.len() {
        return f64::INFINITY;
    }
    let mut ls = leads.to_vec();
    let mut ps = pads.to_vec();
    sort_by_x(&mut ls);
    sort_by_x(&mut ps);
    let sum: f64 = ls
        .iter()
        .zip(ps.iter())
        .map(|(a, b)| a.distance(*b).powi(2))
        .sum();
    (sum / leads.len() as f64).sqrt()
}

fn apply_translation(model: &mut Model3D, dx: f64, dy: f64) {
    model.offset[0] += dx;
    model.offset[1] += dy;
}

fn leads_in_footprint(model: &Model3D, leads_local: &[[f64; 3]]) -> Vec<Point> {
    leads_local.iter().map(|p| footprint_xy(model, *p)).collect()
}

fn translate_to_centroids(model: &mut Model3D, leads_local: &[[f64; 3]], pads: &[Point]) {
    let leads = leads_in_footprint(model, leads_local);
    if leads.is_empty() {
        return;
    }
    let n = leads.len() as f64;
    let lc = Point::new(
        leads.iter().map(|p| p.x).sum::<f64>() / n,
        leads.iter().map(|p| p.y).sum::<f64>() / n,
    );
    let n = pads.len() as f64;
    let pc = Point::new(
        pads.iter().map(|p| p.x).sum::<f64>() / n,
        pads.iter().map(|p| p.y).sum::<f64>() / n,
    );
    apply_translation(model, pc.x - lc.x, pc.y - lc.y);
}

/// Search translation, quarter-turn rotation and optional mirror to match pads.
pub fn auto_align_model(
    mesh: &LoadedModel,
    footprint: &FootprintDefinition,
    current: &Model3D,
    allow_flip: bool,
) -> AutoAlignResult {
    let pads = pad_centres(footprint);
    let mut leads_local = model_lead_points(mesh);
    let pad_count = pads.len();
    if pad_count == 2 && leads_local.len() > 2 {
        let mut best = (0usize, 1usize, 0.0_f64);
        for i in 0..leads_local.len() {
            for j in (i + 1)..leads_local.len() {
                let d = (leads_local[i][0] - leads_local[j][0]).abs();
                if d > best.2 {
                    best = (i, j, d);
                }
            }
        }
        leads_local = vec![leads_local[best.0], leads_local[best.1]];
    }
    let use_leads = !leads_local.is_empty()
        && pad_count > 0
        && (leads_local.len() == pad_count || leads_local.len() >= 2);

    let mut best: Option<(Model3D, f64, bool)> = None;

    let mirrors: &[bool] = if allow_flip {
        &[false, true]
    } else {
        &[false]
    };
    for &mirror in mirrors {
        for quarter in 0..4u32 {
            let mut trial = current.clone();
            if mirror {
                trial.scale[0] = -trial.scale[0].abs();
            }
            trial.rotate[2] = normalize_degrees(trial.rotate[2] + quarter as f64 * 90.0);
            if use_leads {
                translate_to_centroids(&mut trial, &leads_local, &pads);
                let residual = rms_match(&leads_in_footprint(&trial, &leads_local), &pads);
                if best.as_ref().is_none_or(|(_, r, _)| residual < *r) {
                    best = Some((trial, residual, false));
                }
            }
        }
    }

    if let Some((model, residual, bbox_fallback)) = best {
        return AutoAlignResult {
            coarse: residual > COARSE_ALIGN_MM,
            model,
            residual_mm: residual,
            bbox_fallback,
        };
    }

    let mut trial = current.clone();
    let bc = model_bbox_centre_xy(&trial, mesh);
    let pc = pad_centroid(footprint);
    apply_translation(&mut trial, pc.x - bc.x, pc.y - bc.y);
    let residual = bc.distance(pc);
    AutoAlignResult {
        model: trial,
        residual_mm: residual,
        coarse: residual > COARSE_ALIGN_MM,
        bbox_fallback: true,
    }
}

/// Shift every vertex by `(dx, dy, dz)` in model space (for tests).
pub fn shift_mesh(mesh: &LoadedModel, dx: f64, dy: f64, dz: f64) -> LoadedModel {
    LoadedModel {
        positions: mesh
            .positions
            .iter()
            .map(|p| [p[0] + dx, p[1] + dy, p[2] + dz])
            .collect(),
        indices: mesh.indices.clone(),
        color: mesh.color,
    }
}

/// A simple two-terminal chip mesh: body plus two bottom pads for alignment tests.
pub fn test_chip_mesh(pitch: f64) -> LoadedModel {
    let hw = pitch / 2.0 + 0.2;
    let body_z = 0.5;
    let mut positions = Vec::new();
    let mut push_box = |cx: f64, cy: f64, w: f64, h: f64, z0: f64, z1: f64| {
        let (x0, x1) = (cx - w / 2.0, cx + w / 2.0);
        let (y0, y1) = (cy - h / 2.0, cy + h / 2.0);
        for &(x, y, z) in &[
            (x0, y0, z0),
            (x1, y0, z0),
            (x1, y1, z0),
            (x0, y1, z0),
            (x0, y0, z1),
            (x1, y0, z1),
            (x1, y1, z1),
            (x0, y1, z1),
        ] {
            positions.push([x, y, z]);
        }
    };
    push_box(-hw, 0.0, 0.35, 0.6, 0.0, 0.05);
    push_box(hw, 0.0, 0.35, 0.6, 0.0, 0.05);
    push_box(0.0, 0.0, pitch + 0.6, 0.8, 0.06, body_z);
    let mut indices = Vec::new();
    for base in (0..positions.len()).step_by(8) {
        let v = |i| base as u32 + i;
        for &(a, b, c) in &[
            (0, 1, 2),
            (0, 2, 3),
            (4, 6, 5),
            (4, 7, 6),
            (0, 4, 5),
            (0, 5, 1),
            (1, 5, 6),
            (1, 6, 2),
            (2, 6, 7),
            (2, 7, 3),
            (3, 7, 4),
            (3, 4, 0),
        ] {
            indices.extend([v(a), v(b), v(c)]);
        }
    }
    LoadedModel {
        positions,
        indices,
        color: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_library::FootprintPad;

    fn two_pad_footprint(pitch: f64) -> FootprintDefinition {
        let half = pitch / 2.0;
        FootprintDefinition {
            name: "test".into(),
            description: String::new(),
            keywords: String::new(),
            pads: vec![
                FootprintPad {
                    number: "1".into(),
                    pad_type: "smd".into(),
                    shape: "rect".into(),
                    position: Point::new(-half, 0.0),
                    rotation: 0.0,
                    size: Point::new(0.5, 0.5),
                    layers: vec!["F.Cu".into()],
                    drill: None,
                    roundrect_ratio: None,
                    chamfer: None,
                    polygon: vec![],
                    raw_source: String::new(),
                },
                FootprintPad {
                    number: "2".into(),
                    pad_type: "smd".into(),
                    shape: "rect".into(),
                    position: Point::new(half, 0.0),
                    rotation: 0.0,
                    size: Point::new(0.5, 0.5),
                    layers: vec!["F.Cu".into()],
                    drill: None,
                    roundrect_ratio: None,
                    chamfer: None,
                    polygon: vec![],
                    raw_source: String::new(),
                },
            ],
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        }
    }

    fn unit_model() -> Model3D {
        Model3D {
            path: "chip.wrl".into(),
            offset: [0.0; 3],
            scale: [1.0; 3],
            rotate: [0.0; 3],
            raw_source: String::new(),
        }
    }

    #[test]
    fn auto_align_recovers_a_shifted_generated_body() {
        let pitch = 3.0;
        let fp = two_pad_footprint(pitch);
        let mesh = test_chip_mesh(pitch);
        let shifted = shift_mesh(&mesh, 0.32, -0.18, 0.0);
        let out = auto_align_model(&shifted, &fp, &unit_model(), true);
        assert!(
            out.residual_mm < 0.05,
            "residual {} mm",
            out.residual_mm
        );
        assert!(!out.bbox_fallback);
        let verify = auto_align_model(&mesh, &fp, &out.model, false);
        assert!(
            verify.residual_mm < 0.05,
            "after apply {}",
            verify.residual_mm
        );
    }

    #[test]
    fn auto_align_recovers_wrong_model_offset() {
        let pitch = 3.0;
        let fp = two_pad_footprint(pitch);
        let mesh = test_chip_mesh(pitch);
        let mut misaligned = unit_model();
        misaligned.offset = [0.25, 0.12, 0.0];
        let out = auto_align_model(&mesh, &fp, &misaligned, true);
        assert!(out.residual_mm < 0.05, "residual {}", out.residual_mm);
    }

    #[test]
    fn auto_align_falls_back_to_bbox_without_leads() {
        let fp = two_pad_footprint(2.0);
        let mesh = LoadedModel {
            positions: vec![[0.0, 0.0, 0.5], [1.0, 0.0, 0.5], [0.0, 1.0, 0.8]],
            indices: vec![0, 1, 2],
            color: None,
        };
        let out = auto_align_model(&mesh, &fp, &unit_model(), false);
        assert!(out.bbox_fallback);
    }
}
