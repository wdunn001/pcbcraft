//! Bounded KiCad library readers. Geometry is millimetres in screen (y-down) coordinates.
mod footprint_check;
mod footprint_generate;
mod footprint_pad_outline;
pub mod listing;
mod paths;
mod reader;
mod sexpr;
pub mod sources;
mod symbol_check;
mod validation;
pub use footprint_check::{FindingSeverity, FootprintFinding, check_footprint};
pub use footprint_generate::{LandPatternDensity, LandPatternParams, generate_footprint};
use pcbcraft_geom::Point;
pub use reader::{read_footprint, read_specctra_session, read_symbols};
use serde::{Deserialize, Serialize};
pub use sexpr::{Error, Limits, Node, parse as parse_sexpr, parse_limits as parse_sexpr_limits};
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
    /// File token after the electrical type: line, inverted, clock, and the
    /// other standard shape keywords. Not the unit alternate-body index.
    #[serde(default = "default_pin_graphical_style")]
    pub graphical_style: String,
    /// When false the pin may stay in the library but is omitted from schematic views.
    #[serde(default = "default_pin_visible")]
    pub visible: bool,
}
fn default_pin_graphical_style() -> String {
    "line".into()
}
fn default_pin_visible() -> bool {
    true
}
/// Standard symbol pin shape tokens from the interchange format.
pub const PIN_GRAPHICAL_STYLES: &[&str] = &[
    "line",
    "inverted",
    "clock",
    "inverted_clock",
    "input_low",
    "clock_low",
    "output_low",
    "edge_clock_high",
    "non_logic",
];
impl SymbolPin {
    pub fn graphical_style_supported(style: &str) -> bool {
        PIN_GRAPHICAL_STYLES.contains(&style)
    }
    /// Inner end of the pin stem in y-down millimetres (connection is `position`).
    pub fn stem_end(&self) -> Point {
        let angle = self.angle.to_radians();
        self.position.offset(Point::new(
            self.length * angle.cos(),
            -self.length * angle.sin(),
        ))
    }
    fn inward(&self) -> Point {
        let angle = self.angle.to_radians();
        Point::new(angle.cos(), -angle.sin())
    }
    fn perpendicular(&self) -> Point {
        let angle = self.angle.to_radians();
        Point::new(angle.sin(), angle.cos())
    }
    fn mul(v: Point, scale: f64) -> Point {
        Point::new(v.x * scale, v.y * scale)
    }
    const DECO_RADIUS: f64 = 0.635;
    pub fn circle_points(center: Point, radius: f64) -> Vec<Point> {
        (0..16)
            .map(|i| {
                let t = f64::from(i) * std::f64::consts::TAU / 16.0;
                center.offset(Point::new(radius * t.cos(), radius * t.sin()))
            })
            .collect()
    }
    fn bubble(&self) -> (Point, f64) {
        let inward = self.inward();
        (
            self.position.offset(Self::mul(inward, Self::DECO_RADIUS)),
            Self::DECO_RADIUS,
        )
    }
    fn clock_wedge(&self, depth: f64) -> Vec<Point> {
        let inward = self.inward();
        let perp = self.perpendicular();
        let half = Self::DECO_RADIUS;
        let base_a = self.position.offset(Self::mul(perp, -half));
        let base_b = self.position.offset(Self::mul(perp, half));
        let tip = self.position.offset(Self::mul(inward, depth));
        vec![base_a, base_b, tip]
    }
    fn active_low_tick(&self) -> Vec<Point> {
        let perp = self.perpendicular();
        let half = Self::DECO_RADIUS * 0.9;
        let at = self
            .position
            .offset(Self::mul(self.inward(), Self::DECO_RADIUS * 1.6));
        vec![
            at.offset(Self::mul(perp, -half)),
            at.offset(Self::mul(perp, half)),
        ]
    }
    fn non_logic_diamond(&self) -> Vec<Point> {
        let inward = self.inward();
        let perp = self.perpendicular();
        let r = Self::DECO_RADIUS * 0.75;
        vec![
            self.position.offset(Self::mul(inward, r)),
            self.position.offset(Self::mul(perp, r)),
            self.position.offset(Self::mul(inward, -r)),
            self.position.offset(Self::mul(perp, -r)),
        ]
    }
    /// Filled outline circles for bubble-style decorations at the connection.
    pub fn decoration_bubbles(&self) -> Vec<(Point, f64)> {
        self.decoration_bubbles_draw(true)
    }
    pub fn decoration_bubbles_draw(&self, require_visible: bool) -> Vec<(Point, f64)> {
        if require_visible && !self.visible {
            return Vec::new();
        }
        let bubble = |pin: &SymbolPin| pin.bubble();
        match self.graphical_style.as_str() {
            "inverted" | "output_low" => vec![bubble(self)],
            "inverted_clock" | "clock_low" => vec![bubble(self)],
            "input_low" => vec![bubble(self)],
            _ => Vec::new(),
        }
    }
    /// Closed or open polylines for non-circle pin decorations in y-down mm.
    pub fn decoration_paths(&self) -> Vec<Vec<Point>> {
        self.decoration_paths_draw(true)
    }
    pub fn decoration_paths_draw(&self, require_visible: bool) -> Vec<Vec<Point>> {
        if (require_visible && !self.visible) || self.graphical_style == "line" {
            return Vec::new();
        }
        let mut paths = Vec::new();
        let push_wedge = |paths: &mut Vec<Vec<Point>>, pin: &SymbolPin, depth: f64| {
            paths.push(pin.clock_wedge(depth));
        };
        match self.graphical_style.as_str() {
            "inverted" | "output_low" => {}
            "clock" => push_wedge(&mut paths, self, Self::DECO_RADIUS * 1.4),
            "inverted_clock" => {
                push_wedge(&mut paths, self, Self::DECO_RADIUS * 1.4);
            }
            "input_low" => {
                push_wedge(&mut paths, self, Self::DECO_RADIUS * 1.2);
                paths.push(self.active_low_tick());
            }
            "clock_low" => {
                push_wedge(&mut paths, self, Self::DECO_RADIUS * 1.2);
            }
            "edge_clock_high" => push_wedge(&mut paths, self, Self::DECO_RADIUS * 0.85),
            "non_logic" => paths.push(self.non_logic_diamond()),
            _ => {}
        }
        paths
    }
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

#[cfg(test)]
mod pin_graphical_tests {
    use super::*;
    #[test]
    fn inverted_clock_decoration_includes_connection_bubble() {
        let pin = SymbolPin {
            number: "1".into(),
            name: "CLK".into(),
            position: Point::new(0.0, 0.0),
            angle: 0.0,
            length: 2.54,
            electrical_type: "input".into(),
            unit: 1,
            body_style: 1,
            graphical_style: "inverted_clock".into(),
            visible: true,
        };
        let bubbles = pin.decoration_bubbles();
        assert_eq!(bubbles.len(), 1);
        let (center, radius) = bubbles[0];
        assert!(
            (pin.position.distance(center) - radius).abs() < 0.05,
            "connection sits on the bubble outline"
        );
        assert!(!pin.decoration_paths().is_empty());
    }
}
