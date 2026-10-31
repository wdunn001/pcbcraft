//! Software renderer for the 3D tab: a depth-buffered, deferred triangle
//! rasteriser with a studio look, returning an image for egui.
//!
//! Context: `board3d.rs` owns the camera and the mesh and calls [`render`] only
//! when the picture would change (see `Frame` there). The mesh comes from
//! `pcbcraft_io::board_mesh`; this file never edits it.
//! Local rules: pass one writes a depth buffer and a triangle id per sample, so
//! overdraw costs nothing to shade; pass two shades each visible sample in
//! linear light, averages the samples of a pixel, then applies exposure, the
//! ACES filmic curve and the sRGB transfer. Flat shading per triangle with a
//! real depth buffer, so coplanar faces shade the same and nothing depends on
//! triangle order. The lights follow the camera (a warm key from the upper
//! right, a cool rim from behind, a soft fill and a faint bounce from below) so
//! the board reads the same from every angle; shiny materials reflect a small
//! procedural studio built from the same lights, which is what makes gold pads
//! and tinned leads glint. Exposed pads: the mask sheet has openings over the
//! pads, so copper shows there as plating (gold or tin); where mask lies
//! directly over copper (tracks, pours) it is drawn a little lighter and
//! warmer, the way copper shows through real mask. Silkscreen is clipped where
//! the mask is open: ink over an exposed pad is skipped.
//!
//! # Dependencies
//! - crates/io/src/board_mesh.rs (`BoardMesh`, `PartKind`)
//! - crates/ui-egui/src/board3d.rs (`Basis`, `flip`)
//!
//! # Do not
//! - Use the GPU; native builds may rasterise scanline bands in parallel.
use super::{FOV_TAN, NEAR, V, cross, dot, flip, sub, unit, Basis};
use pcbcraft_io::{apply_matrix, BoardMesh, PartKind};
use scene::{box_corners, box_indices, prepare, Drawable, Scene};

mod scene;

/// What to draw and how, besides the camera.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Look {
    /// Copper looks like gold plating when true, tin when false.
    pub gold: bool,
    pub selected: Option<pcbcraft_doc::Id>,
    /// Samples per axis; 1 while the camera moves, 2 when it rests.
    pub ss: usize,
    /// Solder mask colour, sRGB 0..255.
    pub mask: [u8; 3],
    /// Silkscreen ink colour, sRGB 0..255.
    pub silk: [u8; 3],
}

const BG_TOP: [f32; 3] = [44.0, 48.0, 56.0];
const BG_BOTTOM: [f32; 3] = [18.0, 20.0, 25.0];
const MASK: [f32; 3] = [18.0, 104.0, 58.0];
const FR4: [f32; 3] = [196.0, 172.0, 112.0];
const MASK_OVER: [f32; 3] = [52.0, 136.0, 84.0];
const SILK: [f32; 3] = [244.0, 244.0, 238.0];
const PACKAGE: [f32; 3] = [38.0, 40.0, 47.0];
const GOLD: [f32; 3] = [222.0, 176.0, 62.0];
const TIN: [f32; 3] = [200.0, 202.0, 206.0];
const PICKED: [f32; 3] = [255.0, 190.0, 40.0];
#[cfg(not(target_arch = "wasm32"))]
const BAND_ROWS: usize = 48;

type F3 = [f32; 3];

const BG: F3 = [0.0030, 0.0037, 0.0048]; // #0a0c0f in linear light
const FR4_SRGB: [u8; 3] = [186, 158, 102];
const PACKAGE_SRGB: [u8; 3] = [24, 25, 28];
const GOLD_SRGB: [u8; 3] = [217, 164, 65];
const TIN_SRGB: [u8; 3] = [201, 205, 211];
const PICKED_SRGB: [u8; 3] = [255, 170, 30];
const EXPOSURE: f32 = 1.15;

fn lin(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn lin3(c: [u8; 3]) -> F3 {
    [lin(c[0]), lin(c[1]), lin(c[2])]
}
fn lin3f(c: [f32; 3]) -> F3 {
    lin3([
        (c[0] * 255.0).clamp(0.0, 255.0) as u8,
        (c[1] * 255.0).clamp(0.0, 255.0) as u8,
        (c[2] * 255.0).clamp(0.0, 255.0) as u8,
    ])
}
fn mul3(a: F3, b: F3) -> F3 {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}
fn scale3(a: F3, k: f32) -> F3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn add3(a: F3, b: F3) -> F3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mix3(a: F3, b: F3, t: f32) -> F3 {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}
fn d3(a: F3, b: F3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn v3(a: V) -> F3 {
    [a[0] as f32, a[1] as f32, a[2] as f32]
}
fn norm3(a: F3) -> F3 {
    let n = d3(a, a).sqrt().max(1e-12);
    [a[0] / n, a[1] / n, a[2] / n]
}

/// ACES filmic curve (Narkowicz fit) on one channel, then the sRGB transfer.
fn display_exact(x: f32) -> f32 {
    let x = x * EXPOSURE;
    let t = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
    let t = t.clamp(0.0, 1.0);
    if t <= 0.003_130_8 {
        t * 12.92
    } else {
        1.055 * t.powf(1.0 / 2.4) - 0.055
    }
}

const LUT_TOP: f32 = 8.0;
const LUT_SIZE: usize = 4096;

/// [`display_exact`] through a table: a power per channel per pixel is the
/// slowest thing a still frame does otherwise.
fn display(x: f32) -> f32 {
    static LUT: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    let lut = LUT.get_or_init(|| {
        (0..=LUT_SIZE)
            .map(|i| display_exact(i as f32 / LUT_SIZE as f32 * LUT_TOP))
            .collect()
    });
    let at = (x.max(0.0) / LUT_TOP * LUT_SIZE as f32).min(LUT_SIZE as f32 - 1.0);
    let i = at as usize;
    let f = at - i as f32;
    lut[i] + (lut[i + 1] - lut[i]) * f
}

/// The rig, in the view world: three directional lights and a bounce.
struct Rig {
    key: F3,
    fill: F3,
    rim: F3,
    bounce: F3,
    up: F3,
    eye: F3,
}
const KEY_COLOUR: F3 = [1.0, 0.86, 0.70];
const FILL_COLOUR: F3 = [0.82, 0.93, 0.80];
const RIM_COLOUR: F3 = [0.66, 0.78, 1.0];
const BOUNCE_COLOUR: F3 = [0.55, 0.50, 0.20];
const KEY_POWER: f32 = 0.95;
const FILL_POWER: f32 = 0.32;
const RIM_POWER: f32 = 0.55;
const BOUNCE_POWER: f32 = 0.10;

impl Rig {
    /// Light arriving on a surface with unit normal `n`, per channel.
    fn irradiance(&self, n: F3) -> F3 {
        let up = 0.5 + 0.5 * d3(n, self.up);
        // A dim studio: cool sky above, a warm dark floor below.
        let mut e = mix3([0.025, 0.022, 0.020], [0.085, 0.100, 0.125], up);
        e = add3(e, scale3(KEY_COLOUR, KEY_POWER * d3(n, self.key).max(0.0)));
        e = add3(
            e,
            scale3(FILL_COLOUR, FILL_POWER * d3(n, self.fill).max(0.0)),
        );
        e = add3(e, scale3(RIM_COLOUR, RIM_POWER * d3(n, self.rim).max(0.0)));
        add3(
            e,
            scale3(BOUNCE_COLOUR, BOUNCE_POWER * d3(n, self.bounce).max(0.0)),
        )
    }
    /// What a mirror-like surface sees along `r`: a small studio room made of
    /// the same lights, blurred by `k` (a high `k` is a sharp mirror).
    fn environment(&self, r: F3, k: f32) -> F3 {
        let sky = 0.5 + 0.5 * d3(r, self.up);
        let mut e = mix3([0.020, 0.018, 0.016], [0.10, 0.12, 0.15], sky);
        // The big soft box in front of the board, where the viewer stands.
        let front = (d3(r, self.eye) - 1.0) * 2.2;
        e = add3(e, scale3([0.36, 0.38, 0.40], front.exp()));
        let lobe = |dir: F3, colour: F3, power: f32, k: f32| {
            scale3(colour, power * ((d3(r, dir) - 1.0) * k).exp())
        };
        e = add3(e, lobe(self.key, KEY_COLOUR, 2.4, k));
        e = add3(e, lobe(self.fill, FILL_COLOUR, 0.8, k));
        add3(e, lobe(self.rim, RIM_COLOUR, 2.0, k))
    }
}

fn comb(a: V, x: f64, b: V, y: f64, c: V, z: f64) -> V {
    [
        a[0] * x + b[0] * y + c[0] * z,
        a[1] * x + b[1] * y + c[1] * z,
        a[2] * x + b[2] * y + c[2] * z,
    ]
}

struct FrameCtx<'a> {
    basis: &'a Basis,
    look: &'a Look,
    sw: usize,
    sh: usize,
    focal: f64,
    cx: f64,
    cy: f64,
    toward_eye: V,
    key: V,
    fill: V,
    rim: V,
    half: V,
    ss: usize,
}

struct TriCam {
    a: (f64, f64, f64, V),
    b: (f64, f64, f64, V),
    c: (f64, f64, f64, V),
}

/// Draw the mesh at `w` by `h` pixels.
pub(super) fn render(
    mesh: &BoardMesh,
    basis: &Basis,
    w: usize,
    h: usize,
    look: &Look,
) -> egui::ColorImage {
    let ss = look.ss.max(1);
    let (sw, sh) = (w * ss, h * ss);
    let focal = sh as f64 / 2.0 / FOV_TAN;
    let (cx, cy) = (sw as f64 / 2.0, sh as f64 / 2.0);
    let mut depth = vec![0.0f32; sw * sh];
    let mut color = vec![[0u8; 3]; sw * sh];
    let mut copper = vec![false; sw * sh];
    for y in 0..sh {
        let t = y as f32 / sh.max(1) as f32;
        let row = [
            (BG_TOP[0] + (BG_BOTTOM[0] - BG_TOP[0]) * t) as u8,
            (BG_TOP[1] + (BG_BOTTOM[1] - BG_TOP[1]) * t) as u8,
            (BG_TOP[2] + (BG_BOTTOM[2] - BG_TOP[2]) * t) as u8,
        ];
        color[y * sw..(y + 1) * sw].fill(row);
    }
    let toward_eye = [-basis.fwd[0], -basis.fwd[1], -basis.fwd[2]];
    let key = norm(comb(basis.up, 0.8, basis.right, -0.55, toward_eye, 0.6));
    let fill = norm(comb(basis.up, 0.15, basis.right, 0.85, toward_eye, 0.5));
    let rim = norm(comb(basis.up, 0.5, basis.right, 0.2, toward_eye, -0.85));
    let half = norm([
        key[0] + toward_eye[0],
        key[1] + toward_eye[1],
        key[2] + toward_eye[2],
    ]);
    let scene = prepare(mesh, basis, sw, sh, focal, cx, cy, ss);
    let ctx = FrameCtx {
        basis,
        look,
        sw,
        sh,
        focal,
        cx,
        cy,
        toward_eye,
        key,
        fill,
        rim,
        half,
        ss,
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        let bands: Vec<(usize, usize)> = (0..sh)
            .step_by(BAND_ROWS)
            .map(|y0| (y0, (y0 + BAND_ROWS).min(sh).saturating_sub(1)))
            .collect();
        bands.par_iter().for_each(|&(y0, y1)| {
            raster_scene_band(&scene, &ctx, &mut depth, &mut color, &mut copper, y0, y1);
        });
    }
    #[cfg(target_arch = "wasm32")]
    raster_scene_band(&scene, &ctx, &mut depth, &mut color, &mut copper, 0, sh.saturating_sub(1));

    let mut out = Vec::with_capacity(w * h);
    let mut flat: Option<(u32, u8, egui::Color32)> = None;
    let inv = 1.0 / (ss * ss) as f32;
    let (fw, fh) = (w as f32, h as f32);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0.0f32; 3];
            // Inside a face every sample of the pixel is the same triangle: shade once.
            let first = (y * ss) * sw + x * ss;
            let uniform = ss > 1
                && (0..ss).all(|dy| {
                    (0..ss).all(|dx| {
                        let at = (y * ss + dy) * sw + x * ss + dx;
                        ids[at] == ids[first] && flags[at] == flags[first]
                    })
                });
            // A flat, shadow-free run of one triangle repeats its last pixel.
            let id0 = ids[first];
            if (uniform || ss == 1)
                && id0 != 0
                && let Some((pid, pfl, px)) = flat
                && pid == id0
                && pfl == flags[first]
                && shadow[y * w + x] == 0
            {
                out.push(px);
                continue;
            }
            let (rows, cols, weight) = if uniform {
                (1, 1, (ss * ss) as f32)
            } else {
                (ss, ss, 1.0)
            };
            for dy in 0..rows {
                let sy = y * ss + dy;
                let ndc_y = -((sy as f32 + 0.5) / sh as f32 * 2.0 - 1.0) * tan;
                for dx in 0..cols {
                    let sx = x * ss + dx;
                    let at = sy * sw + sx;
                    let id = ids[at] as usize;
                    let c = if id == 0 {
                        backdrop(x as f32 / fw, y as f32 / fh)
                    } else {
                        if slot[id - 1] == 0 {
                            let (pi, nf) = table[id - 1];
                            lit.push(make_tri(mesh, pi as usize, nf, look, &mats, &rig));
                            slot[id - 1] = lit.len() as u32;
                        }
                        let t = &lit[slot[id - 1] as usize - 1];
                        let mut c = if flags[at] & F_COVERED != 0 {
                            t.covered
                        } else {
                            t.diffuse
                        };
                        if t.ground {
                            c = scale3(c, 1.0 - f32::from(shadow[y * w + x]) / 255.0);
                        }
                        if t.shiny {
                            let ndc_x = ((sx as f32 + 0.5) / sw as f32 * 2.0 - 1.0)
                                * tan
                                * (sw as f32 / sh as f32);
                            let ray = norm3([
                                fwd[0] + right[0] * ndc_x + up[0] * ndc_y,
                                fwd[1] + right[1] * ndc_x + up[1] * ndc_y,
                                fwd[2] + right[2] * ndc_x + up[2] * ndc_y,
                            ]);
                            let v = [-ray[0], -ray[1], -ray[2]];
                            let nv = d3(t.n, v).max(0.0);
                            let r = [
                                2.0 * nv * t.n[0] - v[0],
                                2.0 * nv * t.n[1] - v[1],
                                2.0 * nv * t.n[2] - v[2],
                            ];
                            let k = 6.0 + (1.0 - t.rough).powi(3) * 400.0;
                            let env = rig.environment(r, k);
                            let fres = (1.0 - nv).powi(5);
                            let gloss = (1.0 - t.rough).max(0.04);
                            let f = [
                                t.f0[0] + (gloss - t.f0[0]).max(0.0) * fres,
                                t.f0[1] + (gloss - t.f0[1]).max(0.0) * fres,
                                t.f0[2] + (gloss - t.f0[2]).max(0.0) * fres,
                            ];
                            c = add3(c, mul3(f, env));
                        }
                        c
                    };
                    sum = add3(sum, scale3(c, weight));
                }
            }
            // Ordered dither so the near-black backdrop does not band.
            let dither = (((x * 7 + y * 13) % 16) as f32 / 16.0 - 0.5) / 255.0;
            let px = [
                display(sum[0] * inv) + dither,
                display(sum[1] * inv) + dither,
                display(sum[2] * inv) + dither,
            ];
            let colour = egui::Color32::from_rgb(
                (px[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                (px[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                (px[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            );
            flat = None;
            if (uniform || ss == 1) && id0 != 0 && shadow[y * w + x] == 0 {
                let at = slot[id0 as usize - 1] as usize;
                if at > 0 && !lit[at - 1].shiny {
                    flat = Some((id0, flags[first], colour));
                }
            }
            out.push(colour);
        }
    }
    egui::ColorImage::new([w, h], out)
}

fn raster_scene_band(
    scene: &Scene<'_>,
    ctx: &FrameCtx<'_>,
    depth: &mut [f32],
    color: &mut [[u8; 3]],
    copper: &mut [bool],
    band_y0: usize,
    band_y1: usize,
) {
    for draw in &scene.drawables {
        match *draw {
            Drawable::Tris { part, slice } => {
                shade_part_tris(part, slice.positions, slice.indices, ctx, depth, color, copper, band_y0, band_y1);
            }
            Drawable::Instanced { part, index_len } => {
                let (Some(proto), Some(m)) = (&part.prototype, &part.instance) else {
                    continue;
                };
                let indices = &proto.indices[..index_len];
                for tri in indices.as_chunks::<3>().0.iter() {
                    let (Some(a), Some(b), Some(c)) = (
                        proto.positions.get(tri[0] as usize),
                        proto.positions.get(tri[1] as usize),
                        proto.positions.get(tri[2] as usize),
                    ) else {
                        continue;
                    };
                    let wa = apply_matrix(m, *a);
                    let wb = apply_matrix(m, *b);
                    let wc = apply_matrix(m, *c);
                    if let Some(cam) = project_tri(ctx, wa, wb, wc)
                        && let Some(shade) = shade_tri(part, &cam, ctx)
                    {
                        raster_tri(&cam, &shade, part, ctx, depth, color, copper, band_y0, band_y1);
                    }
                }
            }
            Drawable::Box { part, min, max } => {
                let corners = box_corners(min, max);
                for face in box_indices() {
                    let wa = corners[face[0]];
                    let wb = corners[face[1]];
                    let wc = corners[face[2]];
                    if let Some(cam) = project_tri(ctx, wa, wb, wc)
                        && let Some(shade) = shade_tri(part, &cam, ctx)
                    {
                        raster_tri(&cam, &shade, part, ctx, depth, color, copper, band_y0, band_y1);
                    }
                }
            }
        }
    }
}

fn shade_part_tris(
    part: &pcbcraft_io::MeshPart,
    positions: &[[f64; 3]],
    indices: &[u32],
    ctx: &FrameCtx<'_>,
    depth: &mut [f32],
    color: &mut [[u8; 3]],
    copper: &mut [bool],
    band_y0: usize,
    band_y1: usize,
) {
    for tri in indices.as_chunks::<3>().0.iter() {
        let (Some(a), Some(b), Some(c)) = (
            positions.get(tri[0] as usize),
            positions.get(tri[1] as usize),
            positions.get(tri[2] as usize),
        ) else {
            continue;
        };
        if let Some(cam) = project_tri(ctx, *a, *b, *c)
            && let Some(shade) = shade_tri(part, &cam, ctx)
        {
            raster_tri(&cam, &shade, part, ctx, depth, color, copper, band_y0, band_y1);
        }
    }
}

fn project_tri(ctx: &FrameCtx<'_>, a: V, b: V, c: V) -> Option<TriCam> {
    let project = |p: V| {
        let world = flip(p);
        let rel = sub(world, ctx.basis.eye);
        let z = dot(rel, ctx.basis.fwd);
        (z > NEAR).then(|| {
            let x = dot(rel, ctx.basis.right);
            let y = dot(rel, ctx.basis.up);
            (
                ctx.cx + x / z * ctx.focal,
                ctx.cy - y / z * ctx.focal,
                1.0 / z,
                world,
            )
        })
    };
    let a = project(a)?;
    let b = project(b)?;
    let c = project(c)?;
    Some(TriCam { a, b, c })
}

struct TriShade {
    base: [f32; 3],
    lit: f64,
    spec: f64,
    over: [u8; 3],
    is_mask: bool,
    is_copper: bool,
}

fn shade_tri(part: &pcbcraft_io::MeshPart, cam: &TriCam, ctx: &FrameCtx<'_>) -> Option<TriShade> {
    let is_mask = matches!(part.kind, PartKind::MaskTop | PartKind::MaskBottom);
    let is_copper = matches!(part.kind, PartKind::CopperTop | PartKind::CopperBottom);
    let picked = part.component.is_some() && part.component == ctx.look.selected;
    let (base_color, shine) = match part.kind {
        PartKind::Board => (MASK, 0.12),
        PartKind::CopperTop | PartKind::CopperBottom => {
            (if ctx.look.gold { GOLD } else { TIN }, 0.55)
        }
        PartKind::SilkTop | PartKind::SilkBottom => (SILK, 0.0),
        PartKind::MaskTop | PartKind::MaskBottom => (MASK, 0.0),
        PartKind::Body => (
            part.color
                .or_else(|| {
                    part.prototype
                        .as_ref()
                        .and_then(|p| p.color)
                })
                .map(|c| [c[0] * 255.0, c[1] * 255.0, c[2] * 255.0])
                .unwrap_or(PACKAGE),
            0.25,
        ),
    };
    let area = (cam.b.0 - cam.a.0) * (cam.c.1 - cam.a.1) - (cam.b.1 - cam.a.1) * (cam.c.0 - cam.a.0);
    if area.abs() < 1e-12 {
        return None;
    }
    let mut n = norm(cross(sub(cam.b.3, cam.a.3), sub(cam.c.3, cam.a.3)));
    let facing = dot(n, sub(ctx.basis.eye, cam.a.3));
    if part.kind != PartKind::Body {
        if facing >= 0.0 {
            return None;
        }
        n = [-n[0], -n[1], -n[2]];
    } else if facing < 0.0 {
        n = [-n[0], -n[1], -n[2]];
    }
    let mut base = base_color;
    if part.kind == PartKind::Board && n[2].abs() < 0.7 {
        base = FR4;
    }
    if picked {
        base = PICKED;
    }
    let lit = 0.28
        + 0.42 * dot(n, ctx.toward_eye).max(0.0)
        + 0.36 * dot(n, ctx.key).max(0.0)
        + 0.20 * dot(n, ctx.fill).max(0.0)
        + 0.14 * dot(n, ctx.rim).max(0.0);
    let spec = shine * dot(n, ctx.half).max(0.0).powi(24) * 255.0;
    let over = {
        let m = [
            MASK_OVER[0] * lit as f32 + spec as f32,
            MASK_OVER[1] * lit as f32 + spec as f32,
            MASK_OVER[2] * lit as f32 + spec as f32,
        ];
        [
            m[0].min(255.0) as u8,
            m[1].min(255.0) as u8,
            m[2].min(255.0) as u8,
        ]
    };
    Some(TriShade {
        base,
        lit,
        spec,
        over,
        is_mask,
        is_copper,
    })
}

fn raster_tri(
    cam: &TriCam,
    shade: &TriShade,
    _part: &pcbcraft_io::MeshPart,
    ctx: &FrameCtx<'_>,
    depth: &mut [f32],
    color: &mut [[u8; 3]],
    copper: &mut [bool],
    band_y0: usize,
    band_y1: usize,
) {
    let area = (cam.b.0 - cam.a.0) * (cam.c.1 - cam.a.1) - (cam.b.1 - cam.a.1) * (cam.c.0 - cam.a.0);
    let x0 = cam.a.0.min(cam.b.0).min(cam.c.0).floor().max(0.0) as usize;
    let x1 = (cam.a.0.max(cam.b.0).max(cam.c.0).ceil().max(0.0) as usize).min(ctx.sw.saturating_sub(1));
    let y0 = cam.a.1.min(cam.b.1).min(cam.c.1).floor().max(0.0) as usize;
    let y1 = (cam.a.1.max(cam.b.1).max(cam.c.1).ceil().max(0.0) as usize).min(ctx.sh.saturating_sub(1));
    if x0 > x1 || y0 > y1 {
        return;
    }
    if y1 < band_y0 || y0 > band_y1 {
        return;
    }
    if ctx.ss == 1 && x1 - x0 <= 1 && y1 - y0 <= 1 {
        return;
    }
    let y0 = y0.max(band_y0);
    let y1 = y1.min(band_y1);
    let rgb = [
        (shade.base[0] * shade.lit as f32 + shade.spec as f32).min(255.0) as u8,
        (shade.base[1] * shade.lit as f32 + shade.spec as f32).min(255.0) as u8,
        (shade.base[2] * shade.lit as f32 + shade.spec as f32).min(255.0) as u8,
    ];
    let dx = ((cam.b.2 - cam.a.2) * (cam.c.1 - cam.a.1) - (cam.c.2 - cam.a.2) * (cam.b.1 - cam.a.1)) / area;
    let dy = ((cam.c.2 - cam.a.2) * (cam.b.0 - cam.a.0) - (cam.b.2 - cam.a.2) * (cam.c.0 - cam.a.0)) / area;
    let edges = [(cam.a, cam.b), (cam.b, cam.c), (cam.c, cam.a)];
    for py in y0..=y1 {
        let fy = py as f64 + 0.5;
        let (mut xl, mut xr) = (f64::INFINITY, f64::NEG_INFINITY);
        for (p, q) in edges {
            if (p.1 <= fy) != (q.1 <= fy) {
                let x = p.0 + (fy - p.1) / (q.1 - p.1) * (q.0 - p.0);
                xl = xl.min(x);
                xr = xr.max(x);
            }
        }
        if xl > xr {
            continue;
        }
        let first = ((xl - 0.5).ceil().max(x0 as f64)) as usize;
        let last = ((xr - 0.5).floor().min(x1 as f64)) as usize;
        if (xr - 0.5).floor() < first as f64 {
            continue;
        }
        let row = py * ctx.sw;
        for px in first..=last {
            let iz = (cam.a.2 + dx * (px as f64 + 0.5 - cam.a.0) + dy * (fy - cam.a.1)) as f32;
            let at = row + px;
            if iz > depth[at] {
                let covers = shade.is_mask
                    && copper[at]
                    && depth[at] > 0.0
                    && (1.0 / depth[at] as f64 - 1.0 / iz as f64) < 0.08;
                depth[at] = iz;
                color[at] = if covers { shade.over } else { rgb };
                copper[at] = shade.is_copper;
            }
        }
    }
}

