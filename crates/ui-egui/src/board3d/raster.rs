//! Software renderer for the 3D tab: a depth-buffered triangle rasteriser with
//! linear-light shading and 2x2 supersampling, returning an image for egui.
//!
//! Context: `board3d.rs` owns the camera and the mesh and calls [`render`] only
//! when the picture would change (see `Frame` there). The mesh comes from
//! `pcbcraft_io::board_mesh`; this file never edits it.
//! Local rules: flat shading per triangle with a real depth buffer; diffuse
//! lighting runs in linear sRGB with a hemisphere ambient term and a cheap
//! environment reflection on metal. The mask sheet is green with openings over
//! the pads; where it lies directly over copper (tracks, pours) it is drawn a
//! little lighter, the way copper shows through real mask.
//!
//! # Dependencies
//! - crates/io/src/board_mesh.rs (`BoardMesh`, `PartKind`)
//! - crates/io/src/mesh_material.rs (`MaterialClass`, colour helpers)
//! - crates/ui-egui/src/board3d.rs (`Basis`, `flip`)
//!
//! # Do not
//! - Use the GPU or any thread; it runs in headless tests and the browser build.
use super::{Basis, FOV_TAN, NEAR, V, cross, dot, flip, sub, unit};
use pcbcraft_io::{BoardMesh, MaterialClass, PartKind, linear_to_srgb_byte, srgb_to_linear};

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
const RESISTOR: [f32; 3] = [48.0, 38.0, 32.0];
const CAN: [f32; 3] = [62.0, 58.0, 54.0];
const CONNECTOR: [f32; 3] = [28.0, 32.0, 38.0];
const CERAMIC: [f32; 3] = [210.0, 205.0, 195.0];
const LENS: [f32; 3] = [180.0, 220.0, 160.0];
const PICKED: [f32; 3] = [255.0, 190.0, 40.0];

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

fn srgb255(c: [f32; 3]) -> [f32; 3] {
    [c[0] / 255.0, c[1] / 255.0, c[2] / 255.0]
}

struct MatLook {
    base_srgb: [f32; 3],
    metal: bool,
    shine: f32,
}

fn material_look(
    mat: MaterialClass,
    part: &pcbcraft_io::MeshPart,
    authored: [f32; 3],
    look: &Look,
) -> MatLook {
    match mat {
        MaterialClass::Authored => MatLook {
            base_srgb: authored,
            metal: false,
            shine: 0.22,
        },
        MaterialClass::TinLead => MatLook {
            base_srgb: srgb255(TIN),
            metal: true,
            shine: 0.55,
        },
        MaterialClass::GoldContact => MatLook {
            base_srgb: srgb255(if look.gold { GOLD } else { TIN }),
            metal: true,
            shine: 0.62,
        },
        MaterialClass::Ceramic => MatLook {
            base_srgb: srgb255(FR4),
            metal: false,
            shine: 0.1,
        },
        MaterialClass::Resistor => MatLook {
            base_srgb: srgb255(RESISTOR),
            metal: false,
            shine: 0.18,
        },
        MaterialClass::CanSleeve => MatLook {
            base_srgb: srgb255(CAN),
            metal: false,
            shine: 0.28,
        },
        MaterialClass::ConnectorPlastic => MatLook {
            base_srgb: srgb255(CONNECTOR),
            metal: false,
            shine: 0.2,
        },
        MaterialClass::Lens => MatLook {
            base_srgb: srgb255(LENS),
            metal: false,
            shine: 0.35,
        },
        MaterialClass::Marking => MatLook {
            base_srgb: srgb255(SILK),
            metal: false,
            shine: 0.0,
        },
        MaterialClass::MouldCompound => {
            let green = matches!(part.kind, PartKind::MaskTop | PartKind::MaskBottom);
            MatLook {
                base_srgb: srgb255(if green { MASK } else { PACKAGE }),
                metal: false,
                shine: if green { 0.0 } else { 0.22 },
            }
        }
    }
}

fn env_reflection(n: V, view: V) -> f32 {
    let up = [0.0, 0.0, 1.0];
    let hemi = (dot(n, up) * 0.5 + 0.5).clamp(0.0, 1.0) as f32;
    let sky = 0.55 + 0.35 * hemi;
    let facing = dot(n, view).max(0.0) as f32;
    sky * (0.35 + 0.65 * facing.powf(3.0))
}

fn shade_pixel(
    mat: MatLook,
    n: V,
    toward_eye: V,
    key: V,
    fill: V,
    rim: V,
    half: V,
) -> [u8; 3] {
    let base = srgb_to_linear(mat.base_srgb);
    let hemi = (dot(n, [0.0, 0.0, 1.0]) * 0.5 + 0.5).max(0.0) as f32;
    let sky = srgb_to_linear([0.55, 0.62, 0.72]);
    let ground = srgb_to_linear([0.06, 0.05, 0.05]);
    let amb = [
        ground[0] + (sky[0] - ground[0]) * hemi,
        ground[1] + (sky[1] - ground[1]) * hemi,
        ground[2] + (sky[2] - ground[2]) * hemi,
    ];
    let ndv = dot(n, toward_eye).max(0.0) as f32;
    let ndk = dot(n, key).max(0.0) as f32;
    let ndf = dot(n, fill).max(0.0) as f32;
    let ndr = dot(n, rim).max(0.0) as f32;
    let diff = 0.18 * ndv + 0.38 * ndk + 0.16 * ndf + 0.1 * ndr;
    let mut col = [
        base[0] * (amb[0] * 0.55 + diff),
        base[1] * (amb[1] * 0.55 + diff),
        base[2] * (amb[2] * 0.55 + diff),
    ];
    if mat.metal {
        let env = env_reflection(n, toward_eye);
        col[0] += env * 0.45;
        col[1] += env * 0.42;
        col[2] += env * 0.35;
    }
    let spec = mat.shine * dot(n, half).max(0.0).powf(24.0) as f32;
    col[0] += spec;
    col[1] += spec;
    col[2] += spec;
    [
        linear_to_srgb_byte(col[0]),
        linear_to_srgb_byte(col[1]),
        linear_to_srgb_byte(col[2]),
    ]
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
    for part in &mesh.parts {
        let is_mask = matches!(part.kind, PartKind::MaskTop | PartKind::MaskBottom);
        let is_copper = matches!(part.kind, PartKind::CopperTop | PartKind::CopperBottom);
        let picked = part.component.is_some() && part.component == look.selected;
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
        let mut tri_i = 0usize;
        for tri in part.indices.as_chunks::<3>().0.iter() {
            let mat = part
                .materials
                .get(tri_i)
                .copied()
                .unwrap_or(MaterialClass::MouldCompound);
            let authored = part.authored_colors.get(tri_i).copied().unwrap_or([0.0; 3]);
            tri_i += 1;
            let mut mat_look = material_look(mat, part, authored, look);
            if picked {
                mat_look.base_srgb = srgb255(PICKED);
                mat_look.metal = false;
            }
            if part.kind == PartKind::Board && mat == MaterialClass::Ceramic {
                // Side walls read as bare FR4, tops as substrate tan.
                mat_look.base_srgb = srgb255(CERAMIC);
            }
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
            if ss == 1 && x1 - x0 <= 1 && y1 - y0 <= 1 {
                continue;
            }
            let mut n = norm(cross(sub(b.3, a.3), sub(c.3, a.3)));
            let facing = dot(n, sub(basis.eye, a.3));
            if part.kind != PartKind::Body {
                if facing >= 0.0 {
                    continue;
                }
                n = [-n[0], -n[1], -n[2]];
            } else if facing < 0.0 {
                n = [-n[0], -n[1], -n[2]];
            }
            if part.kind == PartKind::Board && n[2].abs() < 0.7 {
                mat_look.base_srgb = srgb255(FR4);
            }
            let shade = shade_pixel(mat_look, n, toward_eye, key, fill, rim, half);
            let over = {
                let m = material_look(MaterialClass::MouldCompound, part, [0.0; 3], look);
                let mut over_look = m;
                over_look.base_srgb = srgb255(MASK_OVER);
                shade_pixel(over_look, n, toward_eye, key, fill, rim, half)
            };
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
                        let covers = is_mask
                            && copper[at]
                            && depth[at] > 0.0
                            && (1.0 / depth[at] as f64 - 1.0 / iz as f64) < 0.08;
                        depth[at] = iz;
                        color[at] = if covers { over } else { shade };
                        copper[at] = is_copper;
                    }
                }
            }
        }
    }
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
