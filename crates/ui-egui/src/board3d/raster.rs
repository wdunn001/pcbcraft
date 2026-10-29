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
//! - Use the GPU or any thread; it runs in headless tests and the browser build.
use super::{Basis, FOV_TAN, NEAR, V, cross, dot, flip, sub, unit};
use pcbcraft_io::{BoardMesh, PartKind};

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

/// Default solder mask: a deep green.
pub(super) const MASK_SRGB: [u8; 3] = [23, 86, 52];
/// Default silkscreen ink: off-white.
pub(super) const SILK_SRGB: [u8; 3] = [223, 227, 224];

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

/// What a triangle is made of and how light reaches it, computed once.
#[derive(Clone, Copy)]
struct Tri {
    n: F3,
    /// Diffuse colour times the light arriving on the face.
    diffuse: F3,
    /// Reflectance at normal incidence; zero for none.
    f0: F3,
    rough: f32,
    /// True when the sample needs a view vector for its reflection.
    shiny: bool,
    /// The same triangle seen under mask: diffuse colour when covered.
    covered: F3,
    /// Faces up on the board surface, so a contact shadow may fall on it.
    ground: bool,
}

const F_COPPER: u8 = 1;
const F_COVERED: u8 = 2;

/// Surface description before lighting.
struct Surface {
    albedo: F3,
    metal: f32,
    rough: f32,
}

fn surface(
    part: &pcbcraft_io::MeshPart,
    n: F3,
    up_facing: bool,
    look: &Look,
    mats: &Mats,
) -> Surface {
    let plastic = |c: F3, rough: f32| Surface {
        albedo: c,
        metal: 0.0,
        rough,
    };
    match part.kind {
        PartKind::Board => {
            if up_facing {
                plastic(mats.mask, 0.7)
            } else {
                plastic(mats.fr4, 0.96)
            }
        }
        PartKind::CopperTop | PartKind::CopperBottom => Surface {
            albedo: mats.copper,
            metal: if look.gold { 0.92 } else { 0.95 },
            rough: if look.gold { 0.27 } else { 0.40 },
        },
        PartKind::MaskTop | PartKind::MaskBottom => plastic(mats.mask, 0.62),
        PartKind::SilkTop | PartKind::SilkBottom => plastic(mats.silk, 0.63),
        PartKind::Lead => Surface {
            albedo: part.color.map_or(mats.tin, lin3f),
            metal: 1.0,
            rough: 0.44,
        },
        PartKind::Body => {
            let family = part.family.as_deref().unwrap_or("");
            let colour = part.color.map(lin3f);
            match family {
                "radial" if n[2] > 0.9 || n[2] < -0.9 => Surface {
                    albedo: colour.unwrap_or(mats.tin),
                    metal: 0.9,
                    rough: 0.38,
                },
                "radial" => Surface {
                    albedo: colour.unwrap_or(lin3([20, 24, 32])),
                    metal: 0.3,
                    rough: 0.35,
                },
                "chip" => plastic(colour.unwrap_or(lin3([52, 46, 40])), 0.5),
                "axial" => plastic(colour.unwrap_or(lin3([190, 150, 96])), 0.6),
                "mark" => plastic(colour.unwrap_or(lin3([170, 172, 175])), 0.6),
                _ => plastic(colour.unwrap_or(mats.package), 0.42),
            }
        }
    }
}

struct Mats {
    mask: F3,
    mask_over: F3,
    fr4: F3,
    silk: F3,
    package: F3,
    copper: F3,
    tin: F3,
    picked: F3,
}

/// Light one visible triangle: its material, the light on its face, and the
/// colour it shows under mask. Called once per triangle that wins a sample.
fn make_tri(mesh: &BoardMesh, part: usize, nf: F3, look: &Look, mats: &Mats, rig: &Rig) -> Tri {
    let part = &mesh.parts[part];
    let is_mask = matches!(part.kind, PartKind::MaskTop | PartKind::MaskBottom);
    let picked = part.component.is_some() && part.component == look.selected;
    let solid_outward = !matches!(part.kind, PartKind::Body | PartKind::Lead);
    let mut s = surface(part, nf, nf[2].abs() >= 0.7, look, mats);
    if picked {
        s.albedo = mats.picked;
        s.metal = 0.0;
    }
    let f0 = mix3([0.04f32; 3], s.albedo, s.metal);
    let irr = rig.irradiance(nf);
    let diffuse = scale3(mul3(s.albedo, irr), 1.0 - s.metal);
    let covered = if is_mask {
        mul3(mats.mask_over, irr)
    } else {
        diffuse
    };
    Tri {
        n: nf,
        diffuse,
        f0,
        rough: s.rough,
        shiny: s.metal > 0.2 || s.rough < 0.5,
        covered,
        ground: solid_outward && nf[2] > 0.5,
    }
}

/// Draw the mesh at `w` by `h` pixels.
///
/// Why: a painter-ordered triangle soup cannot hold coplanar layers apart; a
/// depth buffer can, and it costs little on a board.
///
/// # Rules
/// - Triangles with a corner behind the near plane are dropped, not clipped.
/// - Both faces draw, lit from the side that faces the camera.
/// - Parts draw in mesh order: copper, then mask, then silk, then bodies; the
///   mask and silk rules above lean on that order.
///
/// # Do not
/// - Call it every frame; `board3d.rs` caches the image.
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
    let mut ids = vec![0u32; sw * sh];
    let mut flags = vec![0u8; sw * sh];

    let toward_eye = [-basis.fwd[0], -basis.fwd[1], -basis.fwd[2]];
    // Seen from below the board, the face leans the other way: the rig turns over
    // with the camera so the underside is lit like the top, not from behind.
    let flipped = toward_eye[2] < 0.0;
    let basis_up = if flipped {
        [-basis.up[0], -basis.up[1], -basis.up[2]]
    } else {
        basis.up
    };
    // Upper right at about 46 degrees, a cool rim from behind, a soft fill from the left.
    let key = unit(comb(basis_up, 0.72, basis.right, 0.62, toward_eye, 0.55));
    let fill = unit(comb(basis_up, 0.30, basis.right, -0.85, toward_eye, 0.50));
    let rim = unit(comb(basis_up, 0.45, basis.right, -0.25, toward_eye, -0.85));
    let bounce = unit(comb(basis_up, -0.8, basis.right, 0.1, toward_eye, 0.45));
    let rig = Rig {
        key: v3(key),
        fill: v3(fill),
        rim: v3(rim),
        bounce: v3(bounce),
        up: v3(basis_up),
        eye: v3(toward_eye),
    };
    let mats = {
        let mask = lin3(look.mask);
        let copper = lin3(if look.gold { GOLD_SRGB } else { TIN_SRGB });
        Mats {
            mask,
            // Copper shows through the thin mask: lighter and warmer.
            mask_over: mix3(scale3(mask, 1.3), scale3(lin3(GOLD_SRGB), 0.35), 0.09),
            fr4: lin3(FR4_SRGB),
            silk: lin3(look.silk),
            package: lin3(PACKAGE_SRGB),
            copper,
            tin: lin3(TIN_SRGB),
            picked: lin3(PICKED_SRGB),
        }
    };

    let mut table: Vec<(u32, F3)> = Vec::new();
    for (pi, part) in mesh.parts.iter().enumerate() {
        let is_mask = matches!(part.kind, PartKind::MaskTop | PartKind::MaskBottom);
        let is_copper = matches!(part.kind, PartKind::CopperTop | PartKind::CopperBottom);
        let is_silk = matches!(part.kind, PartKind::SilkTop | PartKind::SilkBottom);
        let solid_outward = !matches!(part.kind, PartKind::Body | PartKind::Lead);
        let cam: Vec<Option<(f64, f64, f64, V)>> = part
            .positions
            .iter()
            .map(|p| {
                let world = flip(*p);
                let rel = sub(world, basis.eye);
                let z = dot(rel, basis.fwd);
                (z > NEAR).then(|| {
                    let x = dot(rel, basis.right);
                    let y = dot(rel, basis.up);
                    (cx + x / z * focal, cy - y / z * focal, 1.0 / z, world)
                })
            })
            .collect();
        for tri in part.indices.as_chunks::<3>().0.iter() {
            let (Some(Some(a)), Some(Some(b)), Some(Some(c))) = (
                cam.get(tri[0] as usize),
                cam.get(tri[1] as usize),
                cam.get(tri[2] as usize),
            ) else {
                continue;
            };
            let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            if area.abs() < 1e-12 {
                continue;
            }
            let x0 = a.0.min(b.0).min(c.0).floor().max(0.0) as usize;
            let x1 = (a.0.max(b.0).max(c.0).ceil().max(0.0) as usize).min(sw.saturating_sub(1));
            let y0 = a.1.min(b.1).min(c.1).floor().max(0.0) as usize;
            let y1 = (a.1.max(b.1).max(c.1).ceil().max(0.0) as usize).min(sh.saturating_sub(1));
            if x0 > x1 || y0 > y1 {
                continue;
            }
            // While the camera moves, specks under a pixel are not worth the time.
            if ss == 1 && x1 - x0 <= 1 && y1 - y0 <= 1 {
                continue;
            }
            let mut n = unit(cross(sub(b.3, a.3), sub(c.3, a.3)));
            let facing = dot(n, sub(basis.eye, a.3));
            if solid_outward {
                // Closed solids wound outward before the mirror: the far side is never seen.
                if facing >= 0.0 {
                    continue;
                }
                n = [-n[0], -n[1], -n[2]];
            } else if facing < 0.0 {
                n = [-n[0], -n[1], -n[2]];
            }
            let nf = v3(n);
            let id = table.len() as u32 + 1;
            table.push((pi as u32, nf));
            // Depth is linear in screen space once taken as 1/z: a plane.
            let dx = ((b.2 - a.2) * (c.1 - a.1) - (c.2 - a.2) * (b.1 - a.1)) / area;
            let dy = ((c.2 - a.2) * (b.0 - a.0) - (b.2 - a.2) * (c.0 - a.0)) / area;
            let edges = [(a, b), (b, c), (c, a)];
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
                let row = py * sw;
                for px in first..=last {
                    let iz = (a.2 + dx * (px as f64 + 0.5 - a.0) + dy * (fy - a.1)) as f32;
                    let at = row + px;
                    if iz > depth[at] {
                        let old = flags[at];
                        // Ink never lies over an exposed pad: the mask opening clips it.
                        if is_silk && old & F_COPPER != 0 {
                            continue;
                        }
                        // Copper no deeper than the mask is thick shows through it.
                        let covers = is_mask
                            && old & F_COPPER != 0
                            && depth[at] > 0.0
                            && (1.0 / depth[at] as f64 - 1.0 / iz as f64) < 0.08;
                        depth[at] = iz;
                        ids[at] = id;
                        flags[at] = if is_copper {
                            F_COPPER
                        } else if covers {
                            F_COVERED
                        } else {
                            0
                        };
                    }
                }
            }
        }
    }

    let mut shaded: Vec<Option<Tri>> = vec![None; table.len()];
    let shadow = contact_shadow(mesh, basis, w, h);
    // Shade: one pass over the samples, averaged into pixels in linear light.
    let tan = FOV_TAN as f32;
    let (right, up, fwd) = (v3(basis.right), v3(basis.up), v3(basis.fwd));
    let mut out = Vec::with_capacity(w * h);
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
                        ids[at] == ids[first] && flags[at] == flags[first] && ids[first] != 0
                    })
                });
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
                        let t = shaded[id - 1].get_or_insert_with(|| {
                            let (pi, nf) = table[id - 1];
                            make_tri(mesh, pi as usize, nf, look, &mats, &rig)
                        });
                        let t = &*t;
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
            out.push(egui::Color32::from_rgb(
                (px[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                (px[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                (px[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            ));
        }
    }
    egui::ColorImage::new([w, h], out)
}

/// Near-black studio backdrop with a soft pool of light under the board and a
/// vignette; `u` and `v` run 0..1 across the picture.
fn backdrop(u: f32, v: f32) -> F3 {
    let (dx, dy) = (u - 0.5, (v - 0.55) * 1.15);
    let r2 = dx * dx + dy * dy;
    let pool = (1.0 - r2 * 5.0).max(0.0);
    let vignette = (1.0 - r2 * 1.1).clamp(0.2, 1.0);
    add3(
        scale3(BG, vignette),
        scale3([0.0090, 0.0105, 0.0130], pool * pool),
    )
}

/// Soft contact shadow under part bodies: darkness 0..255 per output pixel.
///
/// Why: a flat-shaded board gives parts nothing to sit on. Each body's
/// footprint grows into four nested rings of rising darkness on the board
/// plane; the result is blurred, and only faces up on the board surface
/// take it, so it costs a few small polygons and two box blurs, never a
/// per-pixel search.
///
/// # Rules
/// - Rings are star polygons around the footprint centre, fanned into triangles.
/// - Only the bottom ring of each body counts; marks and leads add nothing.
fn contact_shadow(mesh: &BoardMesh, basis: &Basis, w: usize, h: usize) -> Vec<u8> {
    let mut map = vec![0u8; w * h];
    let focal = h as f64 / 2.0 / FOV_TAN;
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let project = |p: V| -> Option<(f64, f64)> {
        let world = flip(p);
        let rel = sub(world, basis.eye);
        let z = dot(rel, basis.fwd);
        (z > NEAR).then(|| {
            (
                cx + dot(rel, basis.right) / z * focal,
                cy - dot(rel, basis.up) / z * focal,
            )
        })
    };
    const RINGS: [(f64, u8); 4] = [(1.0, 46), (0.72, 78), (0.46, 108), (0.2, 140)];
    for part in &mesh.parts {
        if part.kind != PartKind::Body || part.family.as_deref() == Some("mark") {
            continue;
        }
        let Some((lo, hi)) = part.bounds() else {
            continue;
        };
        let height = hi[2] - lo[2];
        let reach = (0.35 + 0.55 * height).clamp(0.3, 2.4);
        let mut foot: Vec<(f64, f64)> = part
            .positions
            .iter()
            .filter(|p| p[2] <= lo[2] + 1e-6)
            .map(|p| (p[0], p[1]))
            .collect();
        if foot.len() < 3 {
            continue;
        }
        foot.dedup();
        let (mx, my) = (
            foot.iter().map(|p| p.0).sum::<f64>() / foot.len() as f64,
            foot.iter().map(|p| p.1).sum::<f64>() / foot.len() as f64,
        );
        foot.sort_by(|a, b| {
            (a.1 - my)
                .atan2(a.0 - mx)
                .total_cmp(&(b.1 - my).atan2(b.0 - mx))
        });
        for (grow, dark) in RINGS {
            let ring: Option<Vec<(f64, f64)>> = foot
                .iter()
                .map(|p| {
                    let (dx, dy) = (p.0 - mx, p.1 - my);
                    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                    let k = (len + reach * grow) / len;
                    project([mx + dx * k, my + dy * k, lo[2]])
                })
                .collect();
            let Some(ring) = ring else { continue };
            let centre = project([mx, my, lo[2]]);
            let Some(centre) = centre else { continue };
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                fill_triangle(&mut map, w, h, centre, a, b, dark);
            }
        }
    }
    blur(&mut map, w, h);
    blur(&mut map, w, h);
    map
}

/// Write `value` into every pixel centre inside the triangle, keeping the larger.
fn fill_triangle(
    map: &mut [u8],
    w: usize,
    h: usize,
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    value: u8,
) {
    let y0 = a.1.min(b.1).min(c.1).floor().max(0.0) as usize;
    let y1 = (a.1.max(b.1).max(c.1).ceil().max(0.0) as usize).min(h.saturating_sub(1));
    for py in y0..=y1 {
        let fy = py as f64 + 0.5;
        let (mut xl, mut xr) = (f64::INFINITY, f64::NEG_INFINITY);
        for (p, q) in [(a, b), (b, c), (c, a)] {
            if (p.1 <= fy) != (q.1 <= fy) {
                let x = p.0 + (fy - p.1) / (q.1 - p.1) * (q.0 - p.0);
                xl = xl.min(x);
                xr = xr.max(x);
            }
        }
        if xl > xr {
            continue;
        }
        let first = (xl - 0.5).ceil().max(0.0) as usize;
        let last = ((xr - 0.5).floor().max(-1.0) + 1.0).min(w as f64) as usize;
        for px in first..last {
            let at = py * w + px;
            map[at] = map[at].max(value);
        }
    }
}

/// One 3 by 3 box blur in place, as a horizontal then a vertical pass.
fn blur(map: &mut [u8], w: usize, h: usize) {
    if w < 3 || h < 3 {
        return;
    }
    let mut mid = vec![0u16; w * h];
    for y in 0..h {
        for x in 1..w - 1 {
            let i = y * w + x;
            mid[i] = u16::from(map[i - 1]) + u16::from(map[i]) + u16::from(map[i + 1]);
        }
    }
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            map[i] = ((mid[i - w] + mid[i] + mid[i + w]) / 9) as u8;
        }
    }
}
