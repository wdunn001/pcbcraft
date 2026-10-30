//! Software renderer for the 3D tab: a depth-buffered triangle rasteriser with
//! three-point lighting and 2x2 supersampling, returning an image for egui.
//!
//! Context: `board3d.rs` owns the camera and the mesh and calls [`render`] only
//! when the picture would change (see `Frame` there). The mesh comes from
//! `pcbcraft_io::board_mesh`; this file never edits it.
//! Local rules: flat shading per triangle with a real depth buffer, so coplanar
//! faces shade the same and nothing depends on triangle order; the lights are
//! fixed to the camera (key upper left, fill right, rim behind, a soft head light) so the board
//! reads the same from every angle. The mask sheet is green with openings over
//! the pads; where it lies directly over copper (tracks, pours) it is drawn a
//! little lighter, the way copper shows through real mask.
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
pub(super) struct Look {
    /// Copper looks like gold plating when true, tin when false.
    pub gold: bool,
    pub selected: Option<pcbcraft_doc::Id>,
    /// Samples per axis; 1 while the camera moves, 2 when it rests.
    pub ss: usize,
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

fn norm(v: V) -> V {
    unit(v)
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
    let n = (ss * ss) as u32;
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 3];
            for dy in 0..ss {
                for dx in 0..ss {
                    let p = color[(y * ss + dy) * sw + x * ss + dx];
                    sum[0] += p[0] as u32;
                    sum[1] += p[1] as u32;
                    sum[2] += p[2] as u32;
                }
            }
            out.push(egui::Color32::from_rgb(
                (sum[0] / n) as u8,
                (sum[1] / n) as u8,
                (sum[2] / n) as u8,
            ));
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

#[cfg(test)]
pub(crate) fn render_bench(
    mesh: &BoardMesh,
    basis: &Basis,
    w: usize,
    h: usize,
) -> egui::ColorImage {
    render(
        mesh,
        basis,
        w,
        h,
        &Look {
            gold: false,
            selected: None,
            ss: 2,
        },
    )
}
