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
            PackageFamily::Chip => chip_mesh(&self.key, &self.category),
            PackageFamily::Sot => sot_mesh(&self.key),
            PackageFamily::GullWing => gull_mesh(&self.key),
            PackageFamily::Qfn => qfn_mesh(&self.key),
            PackageFamily::Bga => box_mesh(&self.key, 0.8),
            PackageFamily::Dip => dip_mesh(&self.key),
            PackageFamily::PinHeader => pin_header_mesh(&self.key),
            PackageFamily::Cylinder => cylinder_mesh(&self.key, &self.category),
            PackageFamily::Axial | PackageFamily::Radial => box_mesh(&self.key, 1.0),
            PackageFamily::Unknown => box_mesh(&self.key, 1.0),
        }
    }
}

impl ParamBody {
    /// Build a parametric body directly from a footprint name.
    ///
    /// This is the fastest path and the one the part importer prefers, since
    /// it avoids measuring pads. The name must parse into a recognised family;
    /// otherwise an error string is returned.
    ///
    /// # Rules
    /// - The name must parse into a recognised family.
    /// - A zero height is replaced by the family default.
    /// - The category falls back to passive when empty.
    ///
    /// # Dependencies
    /// - crates/library/src/package_key.rs (PackageKey::from_footprint_name)
    ///
    /// # Do not
    /// - Never guess dimensions for a name that does not parse.
    pub fn from_footprint_name(name: &str, category: &str) -> Result<Self, String> {
        let mut key = PackageKey::from_footprint_name(name)?;
        if key.height_mm <= 0.0 {
            key.height_mm = Self::default_height(key.family, key.pitch_mm);
        }
        Ok(Self::from_key(&key, category))
    }

    /// Resolve the body for a footprint using the priority order.
    ///
    /// A stored body description on the part wins, then the footprint name
    /// grammar, then the existing pad inference. Exactly one of the three
    /// sources is used, first match wins.
    ///
    /// # Rules
    /// - Exactly one of the three sources is used, first match wins.
    /// - The pad inference result also gets the family default height when
    ///   the key has none.
    ///
    /// # Dependencies
    /// - crates/library/src/package_key.rs (PackageKey::from_footprint_name,
    ///   PackageKey::from_footprint)
    /// - crates/library/src/lib.rs (FootprintDefinition)
    ///
    /// # Do not
    /// - Never re-classify when a stored body was passed in.
    pub fn from_footprint(
        footprint: &crate::FootprintDefinition,
        stored: Option<&ParamBody>,
    ) -> Result<Self, String> {
        if let Some(body) = stored {
            return Ok(body.clone());
        }
        if let Ok(key) = PackageKey::from_footprint_name(&footprint.name) {
            let mut key = key;
            if key.height_mm <= 0.0 {
                key.height_mm = Self::default_height(key.family, key.pitch_mm);
            }
            return Ok(Self::from_key(&key, ""));
        }
        let mut key = PackageKey::from_footprint(footprint)?;
        if key.height_mm <= 0.0 {
            key.height_mm = Self::default_height(key.family, key.pitch_mm);
        }
        Ok(Self::from_key(&key, ""))
    }

    /// The family default seated height used whenever a key carries no height.
    ///
    /// This ensures every body has a real package height. The values match
    /// the numbers already used inside the mesh generators.
    ///
    /// # Rules
    /// - Values match the numbers already used inside the mesh generators.
    ///
    /// # Dependencies
    /// - crates/library/src/package_key.rs (PackageFamily)
    ///
    /// # Do not
    /// - Never return zero.
    pub fn default_height(family: PackageFamily, pitch_mm: f64) -> f64 {
        match family {
            PackageFamily::Chip => 0.55,
            PackageFamily::Sot => 1.1,
            PackageFamily::GullWing => {
                if pitch_mm >= 1.0 {
                    1.6
                } else {
                    1.1
                }
            }
            PackageFamily::Qfn => 0.9,
            PackageFamily::Bga => 0.8,
            PackageFamily::Dip => 3.3,
            PackageFamily::PinHeader => 2.54,
            PackageFamily::Cylinder => 5.0,
            PackageFamily::Axial | PackageFamily::Radial | PackageFamily::Unknown => 1.0,
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
/// Polarised categories get a band near the positive end so a chip LED or
/// tantalum reads at a glance.
///
/// # Rules
/// - Caps are capped at 0.3 mm and never exceed one third of the body length.
/// - The middle section is 90% of the stated height so the caps read as raised.
/// - The polarisation band is a thin box at the right end cap, full width and
///   height, never wider than 10% of the body length.
///
/// # Dependencies
/// - crates/library/src/package_key.rs
///
/// # Do not
/// - Let the band extend past the body length.
/// - Change the cap height or the 90% middle section ratio.
fn chip_mesh(key: &PackageKey, category: &str) -> TriangleMesh {
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

    // Polarisation band on the right end cap for recognisable categories.
    let lower = category.to_lowercase();
    if lower.contains("led")
        || lower.contains("diode")
        || lower.contains("tantal")
        || lower.contains("cap")
    {
        let band_w = (lx / 12.0).max(0.1);
        let mut band = box_prism(band_w, ly, h);
        translate_mesh(&mut band, lx / 2.0 - cap_len / 2.0, 0.0, 0.0);
        mesh = merge_meshes(mesh, band);
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

/// Generate a cylinder mesh for axial packages (axial caps, resistors, diodes).
///
/// # Rules
/// - Radius derived from body_width_mm (minimum 2.0 mm).
/// - Height from key.height_mm or 5.0 mm default.
/// - Closed tube with bottom and top fan caps.
/// - Polarity slab merged for LED/diode/polar categories.
///
/// # Dependencies
/// - crates/library/src/package_key.rs
///
/// # Do not
/// - Use unwrap, expect, panic, todo, or unsafe.
fn cylinder_mesh(key: &PackageKey, category: &str) -> TriangleMesh {
    let r = key.body_width_mm.max(2.0) / 2.0;
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        5.0
    };
    let segments = 12;
    let mut vertices = Vec::with_capacity(segments * 2 + 2);
    for i in 0..segments {
        let t = std::f64::consts::TAU * i as f64 / segments as f64;
        vertices.push([r * t.cos(), r * t.sin(), 0.0]);
    }
    for i in 0..segments {
        let t = std::f64::consts::TAU * i as f64 / segments as f64;
        vertices.push([r * t.cos(), r * t.sin(), h]);
    }
    let bottom_center = vertices.len() as u32;
    vertices.push([0.0, 0.0, 0.0]);
    let top_center = vertices.len() as u32;
    vertices.push([0.0, 0.0, h]);
    let mut triangles = Vec::new();
    for i in 0..segments {
        let next = (i + 1) % segments;
        let a = i as u32;
        let b = next as u32;
        let c = (i + segments) as u32;
        let d = (next + segments) as u32;
        triangles.push([a, b, d]);
        triangles.push([a, d, c]);
        triangles.push([bottom_center, b, a]);
        triangles.push([top_center, c, d]);
    }
    let mut mesh = TriangleMesh {
        vertices,
        triangles,
    };
    let cat_lower = category.to_lowercase();
    if cat_lower.contains("led") || cat_lower.contains("diode") || cat_lower.contains("polar") {
        let slab = box_prism(0.3, r.max(0.3) * 1.2, h * 0.4);
        let mut slab = slab;
        translate_mesh(&mut slab, r * 0.5, 0.0, h * 0.6);
        mesh = merge_meshes(mesh, slab);
    }
    mesh
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

    /// Sample footprint names and their expected package categories.
    fn grammar_samples() -> Vec<(&'static str, &'static str)> {
        vec![
            ("0201", "resistor"),
            ("R_0603", "resistor"),
            ("C0805", "capacitor"),
            ("1206", "inductor"),
            ("SOT-23", "transistor"),
            ("SOT-23-5", "ic"),
            ("SOT-23-6", "ic"),
            ("SOT323", "transistor"),
            ("SOT-89", "transistor"),
            ("SOIC-8", "ic"),
            ("SOP-16", "ic"),
            ("TSSOP-8", "ic"),
            ("SSOP-20", "ic"),
            ("MSOP-8", "ic"),
            ("LQFP-48", "ic"),
            ("TQFP-64", "ic"),
            ("QFP-100", "ic"),
            ("QFN-16", "ic"),
            ("QFN-24", "ic"),
            ("DFN-8", "ic"),
            ("DIP-8", "ic"),
            ("PDIP-14", "ic"),
            ("DIP-28", "ic"),
            ("HDR-1X4", "connector"),
            ("PIN-HEADER-2X05", "connector"),
        ]
    }

    /// Build parametric bodies from the grammar sample names.
    fn grammar_bodies() -> Vec<(&'static str, ParamBody)> {
        grammar_samples()
            .into_iter()
            .filter_map(|(name, category)| {
                ParamBody::from_footprint_name(name, category)
                    .ok()
                    .map(|body| (name, body))
            })
            .collect()
    }

    #[test]
    fn grammar_recognises_every_sample_package_name() {
        assert_eq!(
            grammar_bodies().len(),
            grammar_samples().len(),
            "every sample package name must build a parametric body"
        );
    }

    #[test]
    fn property_mesh_stays_inside_the_courtyard() -> Result<(), String> {
        for (name, body) in grammar_bodies() {
            let mut key = body.key.clone();
            let half_x = (key.lead_span_mm.max(key.body_length_mm) / 2.0) + 0.25;
            let half_y = (key
                .lead_span_mm
                .max(key.body_length_mm)
                .max(key.body_width_mm)
                / 2.0)
                + 0.25;
            key.courtyard_half_x = Some(half_x);
            key.courtyard_half_y = Some(half_y);
            let body = ParamBody::from_key(&key, &body.category);
            let mesh = body.mesh();
            let (min, max) = mesh
                .bounds()
                .ok_or_else(|| format!("{name} body has no bounds"))?;
            assert!(
                max[0].abs().max(min[0].abs()) <= half_x + 1e-9,
                "{name} mesh exceeds courtyard in x"
            );
            assert!(
                max[1].abs().max(min[1].abs()) <= half_y + 1e-9,
                "{name} mesh exceeds courtyard in y"
            );
            assert!(
                max[2] <= key.height_mm + 1e-9,
                "{name} mesh exceeds package height"
            );
        }
        Ok(())
    }

    #[test]
    fn property_lowest_face_sits_on_the_board() -> Result<(), String> {
        for (name, body) in grammar_bodies() {
            let mesh = body.mesh();
            let (min, _) = mesh
                .bounds()
                .ok_or_else(|| format!("{name} body has no bounds"))?;
            assert!(
                min[2].abs() < 1e-9,
                "{name} mesh bottom is not at z=0, got {}",
                min[2]
            );
        }
        Ok(())
    }

    #[test]
    fn property_every_pad_is_covered_by_a_lead() -> Result<(), String> {
        for (name, body) in grammar_bodies() {
            let mesh = body.mesh();
            let (_min, max) = mesh
                .bounds()
                .ok_or_else(|| format!("{name} body has no bounds"))?;
            match body.family {
                PackageFamily::Chip => {
                    assert!(
                        max[0] >= body.key.body_length_mm / 2.0 - 1e-9,
                        "{name} chip end caps do not reach pads"
                    );
                }
                PackageFamily::Sot | PackageFamily::GullWing => {
                    assert!(
                        max[1] >= body.key.lead_span_mm / 2.0 - 1e-9,
                        "{name} leads do not reach pad span"
                    );
                }
                PackageFamily::Qfn => {
                    assert!(
                        max[0] >= body.key.lead_span_mm / 2.0 - 1e-9,
                        "{name} qfn leads do not reach pad span in x"
                    );
                    assert!(
                        max[1] >= body.key.lead_span_mm / 2.0 - 1e-9,
                        "{name} qfn leads do not reach pad span in y"
                    );
                }
                PackageFamily::Dip => {
                    assert!(
                        max[0] >= body.key.lead_span_mm / 2.0 - 1e-9,
                        "{name} dip leads do not reach pad span"
                    );
                }
                PackageFamily::PinHeader => {
                    assert!(
                        mesh.vertices.len() == (body.key.pin_count as usize + 1) * 8,
                        "{name} pin header must have one pin post per pad plus the body"
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }

    #[test]
    fn property_no_degenerate_triangles() {
        for (name, body) in grammar_bodies() {
            let mesh = body.mesh();
            assert!(
                mesh.validate().is_ok(),
                "{name} mesh has degenerate triangles"
            );
        }
    }
}
