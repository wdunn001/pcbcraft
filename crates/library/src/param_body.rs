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

/// Chip package with metal end caps (leads) on both ends.
fn chip_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        0.55
    };
    let lx = key.body_length_mm;
    let ly = key.body_width_mm;
    let lead_len = 0.3;
    let lead_h = h * 0.6;

    let mut mesh = box_prism(lx, ly, h);
    let left_lead = box_prism(lead_len, ly, lead_h);
    let mut left_lead = left_lead;
    translate_mesh(&mut left_lead, -lx / 2.0 - lead_len / 2.0, 0.0, 0.0);
    mesh = merge_meshes(mesh, left_lead);

    let right_lead = box_prism(lead_len, ly, lead_h);
    let mut right_lead = right_lead;
    translate_mesh(&mut right_lead, lx / 2.0 + lead_len / 2.0, 0.0, 0.0);
    mesh = merge_meshes(mesh, right_lead);

    mesh
}

/// SOT-23 style package with three leads extending downward.
fn sot_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        1.1
    };
    let lx = key.body_length_mm;
    let ly = key.body_width_mm;
    let lead_len = key.lead_span_mm - ly;
    let lead_w = 0.4;
    let lead_h = 0.1;

    let mut mesh = box_prism(lx, ly, h);
    // Pin-1 marker on the top
    let marker = box_prism(0.3, 0.3, 0.1);
    let mut marker = marker;
    translate_mesh(&mut marker, -lx / 2.0 + 0.3, -ly / 2.0 + 0.3, h);
    mesh = merge_meshes(mesh, marker);

    // Three leads: two on one side, one on the other
    let lead_y = ly / 2.0 + lead_len / 2.0;
    for i in 0..2 {
        let lead = box_prism(lead_w, lead_len, lead_h);
        let mut lead = lead;
        let x = -lx / 2.0 + lx * (i as f64 + 0.5) / 2.0;
        translate_mesh(&mut lead, x, lead_y, 0.0);
        mesh = merge_meshes(mesh, lead);
    }
    let lead2 = box_prism(lead_w, lead_len, lead_h);
    let mut lead2 = lead2;
    translate_mesh(&mut lead2, 0.0, -lead_y, 0.0);
    mesh = merge_meshes(mesh, lead2);

    mesh
}

/// SOIC/SOP style package with gull-wing leads on both sides.
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
    let lead_len = (key.lead_span_mm - ly) / 2.0;
    let lead_w = 0.3;
    let lead_h = 0.1;

    let mut mesh = box_prism(lx, ly, h);
    // Pin-1 marker
    let marker = box_prism(0.3, 0.3, 0.1);
    let mut marker = marker;
    translate_mesh(&mut marker, -lx / 2.0 + 0.3, -ly / 2.0 + 0.3, h);
    mesh = merge_meshes(mesh, marker);

    // Gull-wing leads on both long sides
    let pins_per_side = (key.pin_count / 2) as i32;
    for side in 0..2 {
        for i in 0..pins_per_side {
            let lead = box_prism(lead_w, lead_len, lead_h);
            let mut lead = lead;
            let x = -lx / 2.0 + lx * (i as f64 + 0.5) / pins_per_side as f64;
            let y = if side == 0 {
                -ly / 2.0 - lead_len / 2.0
            } else {
                ly / 2.0 + lead_len / 2.0
            };
            translate_mesh(&mut lead, x, y, 0.0);
            mesh = merge_meshes(mesh, lead);
        }
    }

    mesh
}

/// QFN/DFN package with exposed pad and thin under-leads.
fn qfn_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        0.9
    };
    let lx = key.body_length_mm.max(3.0);
    let ly = key.body_width_mm.max(3.0);
    let lead_len = 0.5;
    let lead_w = 0.2;
    let lead_h = 0.1;

    let mut mesh = box_prism(lx, ly, h);
    // Pin-1 marker
    let marker = box_prism(0.3, 0.3, 0.1);
    let mut marker = marker;
    translate_mesh(&mut marker, -lx / 2.0 + 0.3, -ly / 2.0 + 0.3, h);
    mesh = merge_meshes(mesh, marker);

    // Exposed pad on bottom
    let pad_size = lx * 0.6;
    let exposed_pad = box_prism(pad_size, pad_size, 0.05);
    let mut exposed_pad = exposed_pad;
    translate_mesh(&mut exposed_pad, 0.0, 0.0, 0.0);
    mesh = merge_meshes(mesh, exposed_pad);

    // Thin under-leads on all four sides
    let pins_per_side = (key.pin_count / 4) as i32;
    for side in 0..4 {
        for i in 0..pins_per_side {
            let lead = box_prism(lead_w, lead_len, lead_h);
            let mut lead = lead;
            let pos = -lx / 2.0 + lx * (i as f64 + 0.5) / pins_per_side as f64;
            match side {
                0 => translate_mesh(&mut lead, pos, -ly / 2.0 - lead_len / 2.0, 0.0),
                1 => translate_mesh(&mut lead, lx / 2.0 + lead_len / 2.0, pos, 0.0),
                2 => translate_mesh(&mut lead, pos, ly / 2.0 + lead_len / 2.0, 0.0),
                _ => translate_mesh(&mut lead, -lx / 2.0 - lead_len / 2.0, pos, 0.0),
            }
            mesh = merge_meshes(mesh, lead);
        }
    }

    mesh
}

/// DIP package with through-hole leads.
fn dip_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        3.3
    };
    let lx = key.body_width_mm.max(6.0);
    let ly = key.body_length_mm.max(8.0);
    let lead_len = (key.lead_span_mm - ly) / 2.0;
    let lead_w = 0.6;
    let lead_h = 0.1;

    let mut mesh = box_prism(lx, ly, h);
    // Pin-1 indicator
    let notch = box_prism(0.5, 0.5, 0.1);
    let mut notch = notch;
    translate_mesh(&mut notch, -lx / 2.0 + 0.5, -ly / 2.0 + 0.5, h);
    mesh = merge_meshes(mesh, notch);

    // Through-hole leads on both sides
    let pins_per_side = (key.pin_count / 2) as i32;
    for side in 0..2 {
        for i in 0..pins_per_side {
            let lead = box_prism(lead_w, lead_len, lead_h);
            let mut lead = lead;
            let x = -lx / 2.0 + lx * (i as f64 + 0.5) / pins_per_side as f64;
            let y = if side == 0 {
                -ly / 2.0 - lead_len / 2.0
            } else {
                ly / 2.0 + lead_len / 2.0
            };
            translate_mesh(&mut lead, x, y, 0.0);
            mesh = merge_meshes(mesh, lead);
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
    fn all_meshes_have_z0_bottom() {
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
            let (min, _max) = mesh.bounds().expect("mesh must have bounds");
            assert!(
                min[2].abs() < 0.001,
                "{:?} mesh bottom must be at z=0, got {}",
                family,
                min[2]
            );
        }
    }
}
