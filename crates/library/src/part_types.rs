use crate::{PackageKey, PinKind};
use serde::{Deserialize, Serialize};

pub const MAX_PARTS: usize = 256;
pub const MAX_PART_PINS: usize = 512;
pub const MAX_REQUIREMENTS: usize = 64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriangleMesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl TriangleMesh {
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let first = self.vertices.first()?;
        let mut min = *first;
        let mut max = *first;
        for v in &self.vertices {
            for i in 0..3 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
        Some((min, max))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Orderable {
    pub distributor: String,
    pub sku: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasheetRef {
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub retrieved_at: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub checked_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartFamily {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Transistor,
    Ic,
    Connector,
    Mechanical,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FootprintSource {
    Library,
    Generated,
    UserImported,
    UserEntered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MpnSource {
    Library,
    Distributor,
    UserEntered,
    UserImported,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinSpec {
    pub number: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default)]
    pub pads: Vec<String>,
    pub kind: PinKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub library_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    #[serde(default)]
    pub abs_max: Vec<String>,
    #[serde(default)]
    pub rec_op: Vec<String>,
    #[serde(default)]
    pub rails: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub kind: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Model3dRef {
    Step {
        path: String,
        #[serde(default)]
        offset: [f64; 3],
        #[serde(default = "unit_scale")]
        scale: [f64; 3],
        #[serde(default)]
        rotate: [f64; 3],
    },
    Wrl {
        path: String,
        #[serde(default)]
        offset: [f64; 3],
        #[serde(default = "unit_scale")]
        scale: [f64; 3],
        #[serde(default)]
        rotate: [f64; 3],
    },
    Gltf {
        path: String,
        #[serde(default)]
        offset: [f64; 3],
        #[serde(default = "unit_scale")]
        scale: [f64; 3],
        #[serde(default)]
        rotate: [f64; 3],
    },
    Parametric {
        body: crate::ParamBody,
    },
}

fn unit_scale() -> [f64; 3] {
    [1.0, 1.0, 1.0]
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationChecks {
    #[serde(default)]
    pub symbol_pads_match: bool,
    #[serde(default)]
    pub package_key_match: bool,
    #[serde(default)]
    pub model_fits: bool,
    #[serde(default)]
    pub datasheet_ok: bool,
    #[serde(default)]
    pub pin_kinds_present: bool,
    #[serde(default)]
    pub mpn_confirmed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    #[serde(default)]
    pub verified: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub checked_at: String,
    #[serde(default)]
    pub checks: VerificationChecks,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manufacturer: String,
    pub mpn: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_mpn: String,
    #[serde(default)]
    pub orderables: Vec<Orderable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<PartFamily>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lifecycle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msl: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datasheet: Option<DatasheetRef>,
    pub symbol: String,
    pub footprint: String,
    pub package_key: PackageKey,
    pub footprint_source: FootprintSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model3d: Option<Model3dRef>,
    #[serde(default)]
    pub pins: Vec<PinSpec>,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
    #[serde(default)]
    pub layout_notes: Vec<String>,
    #[serde(default)]
    pub verification: Verification,
    #[serde(default)]
    pub mpn_source: MpnSource,
    #[serde(default)]
    pub attribution: Vec<String>,
    #[serde(default)]
    pub import_warnings: Vec<String>,
}
