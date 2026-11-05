//! Parametric 3D body generation from package key data.
//!
//! Generates triangle meshes for common IC package families with real leads
//! sized from the package key dimensions. Each mesh has its lowest face at
//! z=0 (the PCB surface) and includes pin-1 indicators where applicable.
//!
//! # Rules
//! - All dimensions are in millimetres.
//! - Meshes must be watertight (no degenerate triangles).
//! - Leads must extend to cover the pad positions implied by the package key.
//! - Pin-1 indicators use a chamfer or dimple on the body.
//!
//! # Dependencies
//! - crates/library/src/part_types.rs (TriangleMesh)
//! - crates/library/src/package_key.rs (PackageKey, PackageFamily)
//!
//! # Do not
//! - Generate meshes with negative dimensions.
//! - Use std::time or other non-wasm-safe APIs.

use crate::{PackageFamily, PackageKey, TriangleMesh};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamBody {
    pub family: PackageFamily,
    #[serde(default = "default_category")]
    pub category: String,
    pub key: PackageKey,
}

fn default_category() -> String {
    "passive".into()
}

impl ParamBody {
    /// Build a parametric body from a package key and category string.
    ///
    /// # Rules
    /// - The category defaults to "passive" when empty.
    ///
    /// # Dependencies
    /// - crates/library/src/package_key.rs (PackageKey)
    ///
    /// # Do not
    /// - Pass a null or empty key.
    pub fn from_key(key: &PackageKey, category: &str) -> Self {
        Self {
            family: key.family,
            category: if category.is_empty() {
                default_category()
            } else {
                category.to_owned()
            },
            key: key.clone(),
        }
    }

    /// Generate the triangle mesh for this parametric body.
    ///
    /// # Rules
    /// - Returns a mesh with the lowest face at z=0.
    /// - Leads extend to cover the pad positions implied by the key.
    ///
    /// # Dependencies
    /// - crates/library/src/part_types.rs (TriangleMesh)
    ///
    /// # Do not
    /// - Assume the mesh is centred at the origin.
    pub fn mesh(&self) -> TriangleMesh {
        match self.family {
            PackageFamily::Chip => chip_mesh(&self.key),
            PackageFamily::Sot => sot_mesh(&self.key),
            PackageFamily::GullWing => gull_mesh(&self.key),
            PackageFamily::Qfn => qfn_mesh(&self.key),
            PackageFamily::Bga => box_mesh(&self.key, 0.8),
            PackageFamily::Dip => dip_mesh(&self.key),
            PackageFamily::PinHeader => pin_header_mesh(&self.key),
            PackageFamily::Cylinder => cylinder_mesh(&self.key),
            PackageFamily::Axial | PackageFamily::Radial => box_mesh(&self.key, 1.0),
            PackageFamily::Unknown => box_mesh(&self.key, 1.0),
        }
    }
}

/// Thickness of the raised pin-1 marker. The body is shortened by this much
/// so the marker top sits exactly at the stated package height.
const MARKER_T: f64 = 0.05;

/// Body box with a small raised pin-1 marker on a top corner. The marker top
/// is at `h`, so the whole shape never rises above the stated height.
fn body_with_marker(lx: f64, ly: f64, h: f64, marker: f64) -> TriangleMesh {
    let t = MARKER_T.min(h / 2.0);
    let body = box_prism(lx, ly, h - t);
    let mut dot = box_prism(marker, marker, t);
    translate_mesh(&mut dot, -lx / 2.0 + marker, -ly / 2.0 + marker, h - t);
    merge_meshes(body, dot)
}

/// Lead centre positions for one row of `count` leads at `pitch`, centred on
/// zero. A lead of width `w` is kept inside `half` so a row of pins never
/// widens the package beyond its body.
fn row_positions(count: usize, pitch: f64, half: f64, w: f64) -> Vec<f64> {
    let limit = (half - w / 2.0).max(0.0);
    let step = if pitch > 0.0 {
        pitch
    } else {
        2.0 * half / (count.max(1) as f64)
    };
    let mid = (count as f64 - 1.0) / 2.0;
    (0..count)
        .map(|i| ((i as f64 - mid) * step).clamp(-limit, limit))
        .collect()
}

/// Chip package. The metal end caps wrap the two ends of the body and sit
/// inside the stated body length, as on a real chip resistor or capacitor.
/// The bounding box is therefore the body box: length by width by height.
fn chip_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        0.55
    };
    let lx = key.body_length_mm;
    let ly = key.body_width_mm;
    let cap_len = (lx / 3.0).min(0.3);

    // Middle section is slightly lower than the caps, which reach full height.
    let mut mesh = box_prism(lx - 2.0 * cap_len, ly, h * 0.9);
    for sign in [-1.0, 1.0] {
        let mut cap = box_prism(cap_len, ly, h);
        translate_mesh(&mut cap, sign * (lx / 2.0 - cap_len / 2.0), 0.0, 0.0);
        mesh = merge_meshes(mesh, cap);
    }
    mesh
}

/// SOT-23 style package. Leads run along y and their tips land at half the
/// lead span. Pins split between the two long sides.
fn sot_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        1.1
    };
    let lx = key.body_length_mm;
    let ly = key.body_width_mm;
    let lead_len = ((key.lead_span_mm - ly) / 2.0).max(0.0);
    let lead_w = 0.4;
    let lead_h = 0.1;

    let mut mesh = body_with_marker(lx, ly, h, 0.3);
    if lead_len > 0.0 {
        let pins = key.pin_count as usize;
        let low = pins.div_ceil(2);
        let high = pins / 2;
        let pitch = key.pitch_mm;
        // Three pin parts put their two pins at one pitch either side of centre.
        let low_xs = if pins == 3 && pitch > 0.0 {
            vec![-pitch, pitch]
        } else {
            row_positions(low, pitch, lx / 2.0, lead_w)
        };
        let high_xs = row_positions(high, pitch, lx / 2.0, lead_w);
        let y = ly / 2.0 + lead_len / 2.0;
        for (xs, sign) in [(low_xs, -1.0), (high_xs, 1.0)] {
            for x in xs {
                let mut lead = box_prism(lead_w, lead_len, lead_h);
                translate_mesh(&mut lead, x, sign * y, 0.0);
                mesh = merge_meshes(mesh, lead);
            }
        }
    }
    mesh
}

/// SOIC/SOP style package with gull-wing leads on both long sides. Lead tips
/// land at half the lead span.
fn gull_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else if key.pitch_mm >= 1.0 {
        1.6
    } else {
        1.1
    };
    let lx = key.body_length_mm.max(2.0);
    let ly = key.body_width_mm.max(1.5);
    let lead_len = ((key.lead_span_mm - ly) / 2.0).max(0.0);
    let lead_w = 0.3;
    let lead_h = 0.1;

    let mut mesh = body_with_marker(lx, ly, h, 0.3);
    if lead_len > 0.0 {
        let per_side = (key.pin_count / 2) as usize;
        let xs = row_positions(per_side, key.pitch_mm, lx / 2.0, lead_w);
        let y = ly / 2.0 + lead_len / 2.0;
        for sign in [-1.0, 1.0] {
            for x in &xs {
                let mut lead = box_prism(lead_w, lead_len, lead_h);
                translate_mesh(&mut lead, *x, sign * y, 0.0);
                mesh = merge_meshes(mesh, lead);
            }
        }
    }
    mesh
}

/// QFN/DFN package with an exposed pad and thin leads on all four sides.
/// Leads end flush with the body edge, or at half the lead span when the
/// key gives a wider span.
fn qfn_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        0.9
    };
    let lx = key.body_length_mm.max(3.0);
    let ly = key.body_width_mm.max(3.0);
    let tip_x = (key.lead_span_mm / 2.0).max(lx / 2.0);
    let tip_y = (key.lead_span_mm / 2.0).max(ly / 2.0);
    let lead_len = 0.5;
    let lead_h = 0.1;
    let lead_w = if key.pitch_mm > 0.0 {
        (key.pitch_mm * 0.6).min(0.3)
    } else {
        0.2
    };

    let mut mesh = body_with_marker(lx, ly, h, 0.3);

    // Exposed pad on the bottom face.
    let pad = if key.exposed_pad_mm > 0.0 {
        key.exposed_pad_mm.min(lx.min(ly) - 0.2)
    } else {
        lx.min(ly) * 0.6
    };
    mesh = merge_meshes(mesh, box_prism(pad, pad, 0.05));

    let per_side = (key.pin_count / 4) as usize;
    let xs = row_positions(per_side, key.pitch_mm, lx / 2.0, lead_w);
    let ys = row_positions(per_side, key.pitch_mm, ly / 2.0, lead_w);
    for side in 0..4 {
        let (positions, lead_dx, lead_dy) = match side {
            0 | 2 => (&xs, lead_w, lead_len),
            _ => (&ys, lead_len, lead_w),
        };
        for p in positions {
            let mut lead = box_prism(lead_dx, lead_dy, lead_h);
            match side {
                0 => translate_mesh(&mut lead, *p, -(tip_y - lead_len / 2.0), 0.0),
                1 => translate_mesh(&mut lead, tip_x - lead_len / 2.0, *p, 0.0),
                2 => translate_mesh(&mut lead, *p, tip_y - lead_len / 2.0, 0.0),
                _ => translate_mesh(&mut lead, -(tip_x - lead_len / 2.0), *p, 0.0),
            }
            mesh = merge_meshes(mesh, lead);
        }
    }
    mesh
}

/// DIP package. The body width runs along x and the length along y. Pins sit
/// in two rows along y, one row each side, at the row spacing given by the
/// lead span. Each lead reaches from the body edge out to half the lead span.
fn dip_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        3.3
    };
    let lx = key.body_width_mm.max(6.0);
    let ly = key.body_length_mm.max(8.0);
    let lead_len = (key.lead_span_mm / 2.0 - lx / 2.0).max(0.0);
    let lead_w = 0.5;
    let lead_h = 0.1;

    let mut mesh = body_with_marker(lx, ly, h, 0.5);
    if lead_len > 0.0 {
        let per_side = (key.pin_count / 2) as usize;
        let ys = row_positions(per_side, key.pitch_mm, ly / 2.0, lead_w);
        let x = lx / 2.0 + lead_len / 2.0;
        for sign in [-1.0, 1.0] {
            for y in &ys {
                let mut lead = box_prism(lead_len, lead_w, lead_h);
                translate_mesh(&mut lead, sign * x, *y, 0.0);
                mesh = merge_meshes(mesh, lead);
            }
        }
    }
    mesh
}

/// Pin header with individual pins.
fn pin_header_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        2.54
    };
    let lx = key.body_length_mm;
    let ly = key.body_width_mm;

    // Base plastic body
    let mut mesh = box_prism(lx, ly, h * 0.4);
    translate_mesh(&mut mesh, 0.0, 0.0, 0.0);

    // Individual pins
    let pin_count = key.pin_count as i32;
    let pin_diameter = 0.8;
    let pin_length = h * 0.6;
    for i in 0..pin_count {
        let pin = box_prism(pin_diameter, pin_diameter, pin_length);
        let mut pin = pin;
        let x = -lx / 2.0 + lx * (i as f64 + 0.5) / pin_count as f64;
        translate_mesh(&mut pin, x, 0.0, h * 0.4);
        mesh = merge_meshes(mesh, pin);
    }

    mesh
}

fn cylinder_mesh(key: &PackageKey) -> TriangleMesh {
    let r = key.body_width_mm.max(2.0) / 2.0;
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        5.0
    };
    let segments = 12;
    let mut vertices = Vec::with_capacity(segments * 2);
    for i in 0..segments {
        let t = std::f64::consts::TAU * i as f64 / segments as f64;
        vertices.push([r * t.cos(), r * t.sin(), 0.0]);
    }
    for i in 0..segments {
        let t = std::f64::consts::TAU * i as f64 / segments as f64;
        vertices.push([r * t.cos(), r * t.sin(), h]);
    }
    let mut triangles = Vec::new();
    for i in 0..segments {
        let next = (i + 1) % segments;
        let a = i as u32;
        let b = next as u32;
        let c = (i + segments) as u32;
        let d = (next + segments) as u32;
        triangles.push([a, b, d]);
        triangles.push([a, d, c]);
    }
    TriangleMesh {
        vertices,
        triangles,
    }
}

fn box_mesh(key: &PackageKey, default_h: f64) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        default_h
    };
    box_prism(key.body_length_mm.max(1.0), key.body_width_mm.max(1.0), h)
}

/// Create a box prism centred at origin, sitting on z=0.
fn box_prism(lx: f64, ly: f64, lz: f64) -> TriangleMesh {
    let (hx, hy, hz) = (lx / 2.0, ly / 2.0, lz);
    let vertices = vec![
        [-hx, -hy, 0.0],
        [hx, -hy, 0.0],
        [hx, hy, 0.0],
        [-hx, hy, 0.0],
        [-hx, -hy, hz],
        [hx, -hy, hz],
        [hx, hy, hz],
        [-hx, hy, hz],
    ];
    let triangles = vec![
        [0, 1, 2],
        [0, 2, 3],
        [4, 6, 5],
        [4, 7, 6],
        [0, 4, 5],
        [0, 5, 1],
        [1, 5, 6],
        [1, 6, 2],
        [2, 6, 7],
        [2, 7, 3],
        [3, 7, 4],
        [3, 4, 0],
    ];
    TriangleMesh {
        vertices,
        triangles,
    }
}

/// Translate a mesh by the given offsets.
fn translate_mesh(mesh: &mut TriangleMesh, dx: f64, dy: f64, dz: f64) {
    for v in &mut mesh.vertices {
        v[0] += dx;
        v[1] += dy;
        v[2] += dz;
    }
}

/// Merge two meshes into one.
fn merge_meshes(mut a: TriangleMesh, mut b: TriangleMesh) -> TriangleMesh {
    let offset = a.vertices.len() as u32;
    for tri in &mut b.triangles {
        tri[0] += offset;
        tri[1] += offset;
        tri[2] += offset;
    }
    a.vertices.extend(b.vertices);
    a.triangles.extend(b.triangles);
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_mesh_has_leads() {
        let key = PackageKey {
            family: PackageFamily::Chip,
            pin_count: 2,
            pitch_mm: 0.0,
            body_length_mm: 2.0,
            body_width_mm: 1.0,
            lead_span_mm: 2.6,
            exposed_pad_mm: 0.0,
            height_mm: 0.5,
            courtyard_half_x: None,
            courtyard_half_y: None,
        };
        let body = ParamBody::from_key(&key, "resistor");
        let mesh = body.mesh();
        assert!(
            mesh.vertices.len() > 8,
            "chip should have leads (more vertices than a box)"
        );
        assert!(mesh.validate().is_ok(), "chip mesh must be valid");
    }

    #[test]
    fn sot_mesh_has_three_leads() {
        let key = PackageKey {
            family: PackageFamily::Sot,
            pin_count: 3,
            pitch_mm: 0.95,
            body_length_mm: 2.9,
            body_width_mm: 1.3,
            lead_span_mm: 3.0,
            exposed_pad_mm: 0.0,
            height_mm: 1.1,
            courtyard_half_x: None,
            courtyard_half_y: None,
        };
        let body = ParamBody::from_key(&key, "transistor");
        let mesh = body.mesh();
        assert!(mesh.vertices.len() > 8, "SOT should have leads");
        assert!(mesh.validate().is_ok(), "SOT mesh must be valid");
    }

    #[test]
    fn gull_mesh_has_leads_on_both_sides() {
        let key = PackageKey {
            family: PackageFamily::GullWing,
            pin_count: 8,
            pitch_mm: 1.27,
            body_length_mm: 4.9,
            body_width_mm: 3.9,
            lead_span_mm: 6.0,
            exposed_pad_mm: 0.0,
            height_mm: 1.6,
            courtyard_half_x: None,
            courtyard_half_y: None,
        };
        let body = ParamBody::from_key(&key, "ic");
        let mesh = body.mesh();
        assert!(mesh.vertices.len() > 8, "SOIC should have leads");
        assert!(mesh.validate().is_ok(), "SOIC mesh must be valid");
    }

    #[test]
    fn qfn_mesh_has_exposed_pad_and_leads() {
        let key = PackageKey {
            family: PackageFamily::Qfn,
            pin_count: 16,
            pitch_mm: 0.5,
            body_length_mm: 4.0,
            body_width_mm: 4.0,
            lead_span_mm: 5.0,
            exposed_pad_mm: 2.0,
            height_mm: 0.9,
            courtyard_half_x: None,
            courtyard_half_y: None,
        };
        let body = ParamBody::from_key(&key, "ic");
        let mesh = body.mesh();
        assert!(mesh.vertices.len() > 8, "QFN should have leads and pad");
        assert!(mesh.validate().is_ok(), "QFN mesh must be valid");
    }

    #[test]
    fn dip_mesh_has_through_hole_leads() {
        let key = PackageKey {
            family: PackageFamily::Dip,
            pin_count: 14,
            pitch_mm: 2.54,
            body_length_mm: 19.3,
            body_width_mm: 6.35,
            lead_span_mm: 21.84,
            exposed_pad_mm: 0.0,
            height_mm: 3.3,
            courtyard_half_x: None,
            courtyard_half_y: None,
        };
        let body = ParamBody::from_key(&key, "ic");
        let mesh = body.mesh();
        assert!(mesh.vertices.len() > 8, "DIP should have leads");
        assert!(mesh.validate().is_ok(), "DIP mesh must be valid");
    }

    #[test]
    fn pin_header_mesh_has_individual_pins() {
        let key = PackageKey {
            family: PackageFamily::PinHeader,
            pin_count: 4,
            pitch_mm: 2.54,
            body_length_mm: 10.16,
            body_width_mm: 2.54,
            lead_span_mm: 10.16,
            exposed_pad_mm: 0.0,
            height_mm: 2.54,
            courtyard_half_x: None,
            courtyard_half_y: None,
        };
        let body = ParamBody::from_key(&key, "connector");
        let mesh = body.mesh();
        assert!(mesh.vertices.len() > 8, "pin header should have pins");
        assert!(mesh.validate().is_ok(), "pin header mesh must be valid");
    }

    #[test]
    fn all_meshes_have_z0_bottom() -> Result<(), String> {
        let families = [
            (PackageFamily::Chip, 2, 0.0, 2.0, 1.0, 2.6, 0.0, 0.5),
            (PackageFamily::Sot, 3, 0.95, 2.9, 1.3, 3.0, 0.0, 1.1),
            (PackageFamily::GullWing, 8, 1.27, 4.9, 3.9, 6.0, 0.0, 1.6),
            (PackageFamily::Qfn, 16, 0.5, 4.0, 4.0, 5.0, 2.0, 0.9),
            (PackageFamily::Dip, 14, 2.54, 19.3, 6.35, 21.84, 0.0, 3.3),
            (
                PackageFamily::PinHeader,
                4,
                2.54,
                10.16,
                2.54,
                10.16,
                0.0,
                2.54,
            ),
        ];
        for (family, pins, pitch, bl, bw, span, ep, h) in families {
            let key = PackageKey {
                family,
                pin_count: pins,
                pitch_mm: pitch,
                body_length_mm: bl,
                body_width_mm: bw,
                lead_span_mm: span,
                exposed_pad_mm: ep,
                height_mm: h,
                courtyard_half_x: None,
                courtyard_half_y: None,
            };
            let body = ParamBody::from_key(&key, "test");
            let mesh = body.mesh();
            let (min, _max) = mesh
                .bounds()
                .ok_or_else(|| format!("{family:?} mesh must have bounds"))?;
            assert!(
                min[2].abs() < 0.001,
                "{:?} mesh bottom must be at z=0, got {}",
                family,
                min[2]
            );
        }
        Ok(())
    }
}
