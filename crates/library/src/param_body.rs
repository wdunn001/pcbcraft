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

    pub fn mesh(&self) -> TriangleMesh {
        match self.family {
            PackageFamily::Chip => chip_mesh(&self.key),
            PackageFamily::Sot | PackageFamily::GullWing => gull_mesh(&self.key),
            PackageFamily::Qfn => qfn_mesh(&self.key),
            PackageFamily::Bga => box_mesh(&self.key, 0.8),
            PackageFamily::Dip => dip_mesh(&self.key),
            PackageFamily::PinHeader => box_mesh(&self.key, 2.54),
            PackageFamily::Cylinder => cylinder_mesh(&self.key),
            PackageFamily::Axial | PackageFamily::Radial => box_mesh(&self.key, 1.0),
            PackageFamily::Unknown => box_mesh(&self.key, 1.0),
        }
    }
}

fn chip_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 { key.height_mm } else { 0.55 };
    box_prism(key.body_length_mm, key.body_width_mm, h)
}

fn gull_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else if key.pitch_mm >= 1.0 {
        1.6
    } else {
        1.1
    };
    box_prism(key.body_length_mm.max(2.0), key.body_width_mm.max(1.5), h)
}

fn qfn_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 { key.height_mm } else { 0.9 };
    box_prism(key.body_length_mm.max(3.0), key.body_width_mm.max(3.0), h)
}

fn dip_mesh(key: &PackageKey) -> TriangleMesh {
    let h = if key.height_mm > 0.0 { key.height_mm } else { 3.3 };
    box_prism(key.body_width_mm.max(6.0), key.body_length_mm.max(8.0), h)
}

fn cylinder_mesh(key: &PackageKey) -> TriangleMesh {
    let r = key.body_width_mm.max(2.0) / 2.0;
    let h = if key.height_mm > 0.0 { key.height_mm } else { 5.0 };
    let segments = 12;
    let mut vertices = Vec::with_capacity(segments * 2);
    for i in 0..segments {
        let t = std::f64::consts::TAU * f64::from(i) / f64::from(segments);
        vertices.push([r * t.cos(), r * t.sin(), 0.0]);
    }
    for i in 0..segments {
        let t = std::f64::consts::TAU * f64::from(i) / f64::from(segments);
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
    TriangleMesh { vertices, triangles }
}

fn box_mesh(key: &PackageKey, default_h: f64) -> TriangleMesh {
    let h = if key.height_mm > 0.0 {
        key.height_mm
    } else {
        default_h
    };
    box_prism(key.body_length_mm.max(1.0), key.body_width_mm.max(1.0), h)
}

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
    TriangleMesh { vertices, triangles }
}
