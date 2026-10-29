//! Bounded KiCad library readers. Geometry is millimetres in screen (y-down) coordinates.
mod footprint_check;
mod footprint_generate;
mod footprint_pad_outline;
mod paths;
mod reader;
mod sexpr;
mod symbol_check;
mod validation;
pub use footprint_check::{FindingSeverity, FootprintFinding, check_footprint};
pub use footprint_generate::{LandPatternDensity, LandPatternParams, generate_footprint};
use pcbcraft_geom::Point;
pub use reader::{read_footprint, read_specctra_session, read_symbols};
use serde::{Deserialize, Serialize};
pub use sexpr::{Error, Limits};
pub use symbol_check::check_symbol;
pub use validation::custom_polygon_error;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolDefinition {
    pub name: String,
    pub reference: String,
    pub value: String,
    pub description: String,
    pub keywords: String,
    pub footprint: String,
    /// Name of the collection this part belongs to, for example "Common
    /// parts". Empty means the part has no set and is listed on its own.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub set: String,
    pub pins: Vec<SymbolPin>,
    pub graphics: Vec<UnitGraphic>,
    pub extends: Option<String>,
    pub raw_source: String,
    pub unsupported: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolPin {
    pub number: String,
    pub name: String,
    pub position: Point,
    /// KiCad angle, counterclockwise in a y-up Cartesian frame.
    pub angle: f64,
    pub length: f64,
    pub electrical_type: String,
    pub unit: u32,
    pub body_style: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitGraphic {
    pub unit: u32,
    pub body_style: u32,
    pub primitive: Graphic,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Graphic {
    Line {
        start: Point,
        end: Point,
        width: f64,
        layer: String,
    },
    Rectangle {
        start: Point,
        end: Point,
        width: f64,
        fill: String,
        layer: String,
    },
    Circle {
        center: Point,
        radius: f64,
        width: f64,
        fill: String,
        layer: String,
    },
    Arc {
        start: Point,
        mid: Point,
        end: Point,
        width: f64,
        layer: String,
    },
    Polyline {
        points: Vec<Point>,
        width: f64,
        fill: String,
        layer: String,
    },
    Text {
        position: Point,
        angle: f64,
        text: String,
        size: Point,
        layer: String,
    },
}
/// One net of a Specctra design session and the component pins it names,
/// each pin given as a (reference, pin number) pair.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionNet {
    pub name: String,
    pub pins: Vec<(String, String)>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintDefinition {
    pub name: String,
    pub description: String,
    pub keywords: String,
    pub pads: Vec<FootprintPad>,
    pub graphics: Vec<Graphic>,
    /// A 3D model reference when the footprint carries one. The transform is
    /// kept exactly as the source states it; this tool records and re-emits
    /// it without rendering geometry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Model3D>,
    /// Design rule overrides scoped to this footprint: a local copper
    /// clearance and solder mask/paste margin shifts in millimetres. The
    /// clearance tightens checks; the margins are recorded and exported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<FootprintRules>,
    pub raw_source: String,
    pub unsupported: Vec<String>,
}
/// Per-footprint design rule overrides. Each value is a millimetre amount;
/// a negative margin shrinks the paste or mask opening.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintRules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_margin: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paste_margin: Option<f64>,
}
/// A reference to a 3D model file attached to a footprint, with the
/// placement transform from the source file's coordinate frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model3D {
    pub path: String,
    #[serde(default)]
    pub offset: [f64; 3],
    #[serde(default = "unit_scale")]
    pub scale: [f64; 3],
    #[serde(default)]
    pub rotate: [f64; 3],
    pub raw_source: String,
}
fn unit_scale() -> [f64; 3] {
    [1.0, 1.0, 1.0]
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintPad {
    pub number: String,
    pub pad_type: String,
    pub shape: String,
    pub position: Point,
    pub rotation: f64,
    pub size: Point,
    pub layers: Vec<String>,
    pub drill: Option<Drill>,
    pub roundrect_ratio: Option<f64>,
    /// Corner cuts of a `chamfered` pad; absent for every other shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chamfer: Option<PadChamfer>,
    /// Pad-local copper outline of a `custom` pad as an open vertex ring.
    /// The rectangular anchor given by `size` must lie inside it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub polygon: Vec<Point>,
    pub raw_source: String,
}
/// Most vertices accepted in a custom pad polygon.
pub const MAX_PAD_POLYGON_VERTICES: usize = 256;
/// Corners removed from a chamfered pad, named in pad-local y-down coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChamferCorners {
    pub top_left: bool,
    pub top_right: bool,
    pub bottom_left: bool,
    pub bottom_right: bool,
}
impl ChamferCorners {
    pub const ALL: Self = Self {
        top_left: true,
        top_right: true,
        bottom_left: true,
        bottom_right: true,
    };
    pub fn any(self) -> bool {
        self.top_left || self.top_right || self.bottom_left || self.bottom_right
    }
}
/// A chamfer cuts each selected corner by `ratio` times the shorter pad side
/// along both edges. The ratio is in (0, 0.5].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PadChamfer {
    pub ratio: f64,
    pub corners: ChamferCorners,
}
/// Typed view of a pad's copper shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PadShape<'a> {
    Circle,
    Rect,
    Oval,
    RoundRect { ratio: f64 },
    Chamfered(PadChamfer),
    Custom { polygon: &'a [Point] },
    Unsupported,
}
impl FootprintPad {
    pub fn shape_kind(&self) -> PadShape<'_> {
        match (self.shape.as_str(), self.chamfer) {
            ("circle", _) => PadShape::Circle,
            ("rect", _) => PadShape::Rect,
            ("oval", _) => PadShape::Oval,
            ("roundrect", _) => PadShape::RoundRect {
                ratio: self.roundrect_ratio.unwrap_or(0.0),
            },
            ("chamfered", Some(chamfer)) => PadShape::Chamfered(chamfer),
            ("custom", _) if !self.polygon.is_empty() => PadShape::Custom {
                polygon: &self.polygon,
            },
            _ => PadShape::Unsupported,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drill {
    pub size: Point,
    pub offset: Point,
}
