//! Culls and level-of-detail choices before rasterisation.

use pcbcraft_io::{BoardMesh, MeshPart, PartKind};
use super::super::{dot, flip, sub, Basis, V, NEAR};

/// Screen-space size below which a body draws as its bounding box.
const LOD_BOX_PX: f64 = 6.0;
/// Screen-space size below which pin posts are skipped.
const LOD_LITE_PX: f64 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lod {
    Box,
    Lite,
    Full,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TriSlice<'a> {
    pub positions: &'a [[f64; 3]],
    pub indices: &'a [u32],
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Drawable<'a> {
    Tris {
        part: &'a MeshPart,
        slice: TriSlice<'a>,
    },
    Instanced {
        part: &'a MeshPart,
        index_len: usize,
    },
    Box {
        part: &'a MeshPart,
        min: V,
        max: V,
    },
}

pub(super) struct Scene<'a> {
    pub drawables: Vec<Drawable<'a>>,
}

pub(super) fn prepare<'a>(
    mesh: &'a BoardMesh,
    basis: &Basis,
    sw: usize,
    sh: usize,
    focal: f64,
    cx: f64,
    cy: f64,
    ss: usize,
) -> Scene<'a> {
    let mut drawables = Vec::new();
    for part in &mesh.parts {
        if part.kind == PartKind::Body {
            if let Some(draw) = body_drawable(part, basis, sw, sh, focal, cx, cy, ss) {
                drawables.push(draw);
            }
            continue;
        }
        if frustum_skip_part(part, basis, sw, sh, focal, cx, cy) {
            continue;
        }
        if !part.indices.is_empty() {
            drawables.push(Drawable::Tris {
                part,
                slice: TriSlice {
                    positions: &part.positions,
                    indices: &part.indices,
                },
            });
        }
    }
    Scene { drawables }
}

fn body_drawable<'a>(
    part: &'a MeshPart,
    basis: &Basis,
    sw: usize,
    sh: usize,
    focal: f64,
    cx: f64,
    cy: f64,
    ss: usize,
) -> Option<Drawable<'a>> {
    let (min, max) = part.world_bounds()?;
    if frustum_skip_bounds(min, max, basis, sw, sh, focal, cx, cy) {
        return None;
    }
    let lod = body_lod(min, max, basis, focal, cx, cy, ss);
    if lod == Lod::Box {
        return Some(Drawable::Box { part, min, max });
    }
    if let (Some(proto), Some(_)) = (&part.prototype, &part.instance) {
        let tri_count = proto.indices.len() / 3;
        let use_tris = if lod == Lod::Lite {
            proto.shell_triangles.min(tri_count as u32) as usize
        } else {
            tri_count
        };
        if use_tris == 0 {
            return None;
        }
        return Some(Drawable::Instanced {
            part,
            index_len: use_tris * 3,
        });
    }
    if part.indices.is_empty() {
        return None;
    }
    let shell = part
        .prototype
        .as_ref()
        .map(|p| p.shell_triangles as usize)
        .unwrap_or(part.triangle_count());
    let use_tris = if lod == Lod::Lite {
        shell.min(part.triangle_count())
    } else {
        part.triangle_count()
    };
    Some(Drawable::Tris {
        part,
        slice: TriSlice {
            positions: &part.positions,
            indices: &part.indices[..use_tris * 3],
        },
    })
}

fn body_lod(
    min: V,
    max: V,
    basis: &Basis,
    focal: f64,
    cx: f64,
    cy: f64,
    ss: usize,
) -> Lod {
    let size = projected_size(min, max, basis, focal, cx, cy) * ss as f64;
    if size < LOD_BOX_PX {
        Lod::Box
    } else if size < LOD_LITE_PX {
        Lod::Lite
    } else {
        Lod::Full
    }
}

fn projected_size(min: V, max: V, basis: &Basis, focal: f64, cx: f64, cy: f64) -> f64 {
    let mut min_sx = f64::INFINITY;
    let mut min_sy = f64::INFINITY;
    let mut max_sx = f64::NEG_INFINITY;
    let mut max_sy = f64::NEG_INFINITY;
    for z in [min[2], max[2]] {
        for &x in [min[0], max[0]] {
            for &y in [min[1], max[1]] {
                let p = flip([x, y, z]);
                let rel = sub(p, basis.eye);
                let depth = dot(rel, basis.fwd);
                if depth <= NEAR {
                    continue;
                }
                let sx = cx + dot(rel, basis.right) / depth * focal;
                let sy = cy - dot(rel, basis.up) / depth * focal;
                min_sx = min_sx.min(sx);
                min_sy = min_sy.min(sy);
                max_sx = max_sx.max(sx);
                max_sy = max_sy.max(sy);
            }
        }
    }
    (max_sx - min_sx).max(max_sy - min_sy).max(0.0)
}

fn frustum_skip_part(
    part: &MeshPart,
    basis: &Basis,
    sw: usize,
    sh: usize,
    focal: f64,
    cx: f64,
    cy: f64,
) -> bool {
    let Some((min, max)) = part.bounds() else {
        return true;
    };
    frustum_skip_bounds(min, max, basis, sw, sh, focal, cx, cy)
}

fn frustum_skip_bounds(
    min: V,
    max: V,
    basis: &Basis,
    sw: usize,
    sh: usize,
    focal: f64,
    cx: f64,
    cy: f64,
) -> bool {
    let mut any_in_front = false;
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for z in [min[2], max[2]] {
        for &x in [min[0], max[0]] {
            for &y in [min[1], max[1]] {
                let p = flip([x, y, z]);
                let rel = sub(p, basis.eye);
                let depth = dot(rel, basis.fwd);
                if depth <= NEAR {
                    continue;
                }
                any_in_front = true;
                let sx = cx + dot(rel, basis.right) / depth * focal;
                let sy = cy - dot(rel, basis.up) / depth * focal;
                min_x = min_x.min(sx);
                min_y = min_y.min(sy);
                max_x = max_x.max(sx);
                max_y = max_y.max(sy);
            }
        }
    }
    if !any_in_front {
        return true;
    }
    max_x < 0.0 || max_y < 0.0 || min_x > sw as f64 || min_y > sh as f64
}

/// Twelve triangles for an axis-aligned box in board space.
pub(super) fn box_indices() -> [[usize; 3]; 12] {
    [
        [0, 1, 2],
        [0, 2, 3],
        [4, 6, 5],
        [4, 7, 6],
        [0, 4, 5],
        [0, 5, 1],
        [2, 6, 7],
        [2, 7, 3],
        [0, 3, 7],
        [0, 7, 4],
        [1, 5, 6],
        [1, 6, 2],
    ]
}

pub(super) fn box_corners(min: V, max: V) -> [V; 8] {
    [
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [max[0], max[1], min[2]],
        [min[0], max[1], min[2]],
        [min[0], min[1], max[2]],
        [max[0], min[1], max[2]],
        [max[0], max[1], max[2]],
        [min[0], max[1], max[2]],
    ]
}
