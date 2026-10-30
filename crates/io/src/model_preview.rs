//! Fixed-viewpoint thumbnails for triangle meshes from [`LoadedModel`].
//!
//! # Rules
//! - Orthographic projection from a constant camera direction; no GPU.
//! - Empty models yield a flat background image.
//!
//! # Dependencies
//! - crates/io/src/model_load.rs
//!
//! # Do not
//! - Read files here; callers pass an already loaded model.

use crate::model_load::LoadedModel;
use image::RgbImage;

/// Default edge length in pixels for library card thumbnails.
pub const MODEL_PREVIEW_SIZE: u32 = 128;

/// How large the preview image is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelPreviewOptions {
    /// Width in pixels; values above 512 are clamped.
    pub width: u32,
    /// Height in pixels; values above 512 are clamped.
    pub height: u32,
}

impl Default for ModelPreviewOptions {
    fn default() -> Self {
        Self {
            width: MODEL_PREVIEW_SIZE,
            height: MODEL_PREVIEW_SIZE,
        }
    }
}

const BG: [u8; 3] = [30, 44, 64];
const FILL: [u8; 3] = [131, 170, 161];

/// Draw `model` from a fixed isometric direction into an RGB image.
///
/// # Rules
/// - Fits the model axis-aligned bounds with a small margin.
/// - Uses flat shading with one directional light tied to the camera.
///
/// # Dependencies
/// - [`LoadedModel::bounds`]
///
/// # Do not
/// - Apply a footprint placement matrix; the mesh is shown in model space.
pub fn preview_loaded_model(model: &LoadedModel, opts: &ModelPreviewOptions) -> RgbImage {
    let w = opts.width.clamp(16, 512);
    let h = opts.height.clamp(16, 512);
    let mut image = RgbImage::from_pixel(w, h, image::Rgb(BG));
    if model.indices.len() < 3 || model.positions.is_empty() {
        return image;
    }
    let Some((lo, hi)) = model.bounds() else {
        return image;
    };
    let center = [
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    ];
    let extent = [
        (hi[0] - lo[0]).max(1e-6),
        (hi[1] - lo[1]).max(1e-6),
        (hi[2] - lo[2]).max(1e-6),
    ];
    let view = normalize([0.65, -0.55, 0.52]);
    let up_ref = [0.0, 0.0, 1.0];
    let right = normalize(cross(up_ref, view));
    let up = normalize(cross(view, right));
    let scale = (w.min(h) as f64 * 0.42) / extent[0].max(extent[1]).max(extent[2]);
    let cx = w as f64 / 2.0;
    let cy = h as f64 / 2.0;
    let light = normalize([0.4, -0.3, 0.85]);
    let base = model
        .color
        .map(|c| [c[0], c[1], c[2]])
        .unwrap_or([FILL[0] as f32 / 255.0, FILL[1] as f32 / 255.0, FILL[2] as f32 / 255.0]);
    let mut depth = vec![f64::INFINITY; (w * h) as usize];
    let mut shaded = vec![BG; (w * h) as usize];
    let mut tri = 0;
    while tri + 2 < model.indices.len() {
        let i0 = model.indices[tri] as usize;
        let i1 = model.indices[tri + 1] as usize;
        let i2 = model.indices[tri + 2] as usize;
        tri += 3;
        if i0 >= model.positions.len() || i1 >= model.positions.len() || i2 >= model.positions.len()
        {
            continue;
        }
        let p0 = model.positions[i0];
        let p1 = model.positions[i1];
        let p2 = model.positions[i2];
        let n = face_normal(p0, p1, p2);
        if dot3(n, view) >= 0.0 {
            continue;
        }
        let shade = (dot3(n, light).max(0.15).min(1.0) * 255.0) as u8;
        let color = [
            (base[0] * shade as f32) as u8,
            (base[1] * shade as f32) as u8,
            (base[2] * shade as f32) as u8,
        ];
        let s0 = project(p0, center, scale, view, right, up, cx, cy);
        let s1 = project(p1, center, scale, view, right, up, cx, cy);
        let s2 = project(p2, center, scale, view, right, up, cx, cy);
        let min_x = s0[0].min(s1[0]).min(s2[0]).floor().max(0.0) as i32;
        let max_x = s0[0].max(s1[0]).max(s2[0]).ceil().min(w as f64 - 1.0) as i32;
        let min_y = s0[1].min(s1[1]).min(s2[1]).floor().max(0.0) as i32;
        let max_y = s0[1].max(s1[1]).max(s2[1]).ceil().min(h as f64 - 1.0) as i32;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f64 + 0.5;
                let py = y as f64 + 0.5;
                let (u, v, wgt) = barycentric(px, py, s0, s1, s2);
                if u < 0.0 || v < 0.0 || wgt < 0.0 {
                    continue;
                }
                let z = u * p0[2] + v * p1[2] + wgt * p2[2];
                let idx = y as u32 * w + x as u32;
                let slot = idx as usize;
                if z < depth[slot] {
                    depth[slot] = z;
                    shaded[slot] = color;
                }
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            if depth[idx].is_finite() {
                image.put_pixel(x, y, image::Rgb(shaded[idx]));
            }
        }
    }
    image
}

fn project(
    p: [f64; 3],
    center: [f64; 3],
    scale: f64,
    view: [f64; 3],
    right: [f64; 3],
    up: [f64; 3],
    cx: f64,
    cy: f64,
) -> [f64; 2] {
    let rel = [p[0] - center[0], p[1] - center[1], p[2] - center[2]];
    let sx = dot3(rel, right) * scale + cx;
    let sy = -dot3(rel, up) * scale + cy;
    let _depth = dot3(rel, view);
    [sx, sy]
}

fn barycentric(px: f64, py: f64, a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> (f64, f64, f64) {
    let v0 = [b[0] - a[0], b[1] - a[1]];
    let v1 = [c[0] - a[0], c[1] - a[1]];
    let v2 = [px - a[0], py - a[1]];
    let den = v0[0] * v1[1] - v1[0] * v0[1];
    if den.abs() < 1e-12 {
        return (-1.0, -1.0, -1.0);
    }
    let u = (v2[0] * v1[1] - v1[0] * v2[1]) / den;
    let v = (v0[0] * v2[1] - v2[0] * v0[1]) / den;
    (u, v, 1.0 - u - v)
}

fn face_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    normalize(cross(ab, ac))
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-12 {
        return [0.0, 0.0, 1.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_model;

    const WEDGE: &[u8] = include_bytes!("../tests/fixtures/wedge.gltf");

    #[test]
    fn a_loaded_wedge_produces_a_non_empty_thumbnail() -> Result<(), String> {
        let model = load_model("wedge.gltf", WEDGE).map_err(|e| e.0)?;
        let img = preview_loaded_model(&model, &ModelPreviewOptions::default());
        assert_eq!(img.width(), MODEL_PREVIEW_SIZE);
        let lit = img
            .pixels()
            .filter(|p| p.0 != BG)
            .count();
        assert!(lit > 400, "expected painted pixels, got {lit}");
        Ok(())
    }

    #[test]
    fn preview_output_is_deterministic() -> Result<(), String> {
        let model = load_model("wedge.gltf", WEDGE).map_err(|e| e.0)?;
        let opts = ModelPreviewOptions {
            width: 64,
            height: 64,
        };
        let a = preview_loaded_model(&model, &opts);
        let b = preview_loaded_model(&model, &opts);
        assert_eq!(a.as_raw(), b.as_raw());
        Ok(())
    }
}
