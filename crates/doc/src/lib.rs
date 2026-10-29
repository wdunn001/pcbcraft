//! Canonical project data. No UI or protocol dependencies.
use pcbcraft_geom::{Point, positive};
use pcbcraft_library::PadShape;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type Id = u64;
pub const MAX_ITEMS: usize = 2048;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pin {
    pub number: String,
    pub name: String,
    pub offset: Point,
    pub net: Option<Id>,
    #[serde(default = "passive")]
    pub electrical_type: String,
    #[serde(default)]
    pub direction: Option<f64>,
    #[serde(default = "first_unit")]
    pub unit: u32,
}
fn first_unit() -> u32 {
    1
}
fn poured() -> bool {
    true
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SymbolUnitPlacement {
    pub unit: u32,
    pub position: Point,
    pub rotation: f64,
}
fn passive() -> String {
    "passive".into()
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LibrarySymbol {
    pub library: String,
    pub unit: u32,
    pub definition: pcbcraft_library::SymbolDefinition,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LibraryFootprint {
    pub library: String,
    pub definition: pcbcraft_library::FootprintDefinition,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Component {
    pub id: Id,
    pub reference: String,
    pub value: String,
    pub kind: String,
    pub position: Point,
    pub rotation: f64,
    pub pins: Vec<Pin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<SymbolUnitPlacement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_symbol: Option<LibrarySymbol>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_footprint: Option<LibraryFootprint>,
    /// Free-form component properties, such as a datasheet link, a manufacturer
    /// or an orderable part number. Keys are unique per component.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ComponentField>,
    /// The schematic sheet this component belongs to. Sheet 0 is the implicit
    /// root page that every design starts with.
    #[serde(default, skip_serializing_if = "is_root_sheet")]
    pub sheet: Id,
}
fn is_root_sheet(id: &Id) -> bool {
    *id == 0
}
/// A named schematic page. Every component belongs to exactly one sheet;
/// sheet 0 is the implicit root page and is not listed here.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    pub id: Id,
    pub name: String,
}
/// One named property on a component. A blank value is still a real property:
/// it says the key applies to this component and has not been filled in.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ComponentField {
    pub key: String,
    pub value: String,
}
impl Component {
    /// Value of a named component property, if the component carries it.
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.value.as_str())
    }
    pub fn unit_placement(&self, unit: u32) -> Option<(Point, f64)> {
        if unit == self.library_symbol.as_ref().map(|s| s.unit).unwrap_or(1) {
            Some((self.position, self.rotation))
        } else {
            self.units
                .iter()
                .find(|u| u.unit == unit)
                .map(|u| (u.position, u.rotation))
        }
    }
    /// Rendering views share the package ID and physical pin/net identity.
    pub fn schematic_views(&self) -> Vec<Component> {
        let mut views = vec![];
        for unit in std::iter::once(self.library_symbol.as_ref().map(|s| s.unit).unwrap_or(1))
            .chain(self.units.iter().map(|u| u.unit))
        {
            if let Some((position, rotation)) = self.unit_placement(unit) {
                let mut view = self.clone();
                view.position = position;
                view.rotation = rotation;
                view.pins.retain(|p| p.unit == unit);
                view.units.clear();
                if let Some(s) = &mut view.library_symbol {
                    s.unit = unit;
                }
                if !self.units.is_empty() {
                    let suffix = if unit <= 26 {
                        char::from_u32(64 + unit)
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| unit.to_string())
                    } else {
                        unit.to_string()
                    };
                    view.reference = format!("{}{suffix}", self.reference);
                }
                views.push(view);
            }
        }
        views
    }
    /// Local drawing extents used for selection and labels, independent of the renderer.
    pub fn drawing_points(&self) -> Vec<Point> {
        let unit = self.library_symbol.as_ref().map(|s| s.unit).unwrap_or(1);
        let mut points: Vec<Point> = self
            .pins
            .iter()
            .filter(|p| p.unit == unit)
            .map(|p| p.offset)
            .collect();
        if let Some(library) = &self.library_symbol {
            for graphic in library
                .definition
                .graphics
                .iter()
                .filter(|g| (g.unit == 0 || g.unit == unit) && g.body_style <= 1)
            {
                points.extend(graphic.primitive.path_points());
            }
        } else {
            points.extend([Point::new(-3.0, -2.0), Point::new(3.0, 2.0)]);
        }
        points
    }
    pub fn pin_position(&self, number: &str) -> Option<Point> {
        self.pins.iter().find(|p| p.number == number).and_then(|p| {
            self.unit_placement(p.unit)
                .map(|(position, rotation)| position.offset(p.offset.rotate(-rotation)))
        })
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Net {
    pub id: Id,
    pub name: String,
    /// Target total routed length for this net, in millimetres. Only checked
    /// when the net belongs to no net class; classified nets follow their
    /// class target.
    #[serde(default)]
    pub target_length: Option<f64>,
    /// How far this net may deviate from its target length, in millimetres.
    /// Defaults to zero, and requires a target.
    #[serde(default)]
    pub length_tolerance: Option<f64>,
}
/// A check the designer wrote themselves: one plain measurement with one
/// limit, re-run with the built-in checks after every edit. The measure is
/// one of a fixed list (a wire thinner than, a wire longer than, more parts,
/// more vias, or more wires on one net than the limit), so a check stays
/// readable without any query language.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CustomCheck {
    pub id: Id,
    /// Plain words for what this check means, shown wherever it reports.
    pub name: String,
    /// One of: "track width below", "track length above", "part count above",
    /// "via count above", "net track count above".
    pub measure: String,
    /// The number the measure is compared against: millimetres for wire
    /// measures, a plain count otherwise.
    pub limit: f64,
    /// "error" or "warning", how loudly a breach is reported.
    pub severity: String,
}
/// A named group of nets sharing one clearance requirement and one default
/// track width. A net belongs to at most one class; nets in no class follow
/// the board default.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NetClass {
    pub id: Id,
    pub name: String,
    /// Minimum copper clearance between members of this class and other
    /// copper, in millimetres.
    pub clearance: f64,
    /// Default track width for members of this class, in millimetres.
    pub width: f64,
    /// Smallest track width this class allows, in millimetres. Set to turn
    /// narrower member tracks into a design rule finding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f64>,
    /// Largest track width this class allows, in millimetres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f64>,
    /// Target total routed length for member nets, in millimetres. Nets in
    /// this class whose copper totals fall outside target ± tolerance are
    /// reported as length findings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_length: Option<f64>,
    /// How far a member net may deviate from the target length, in
    /// millimetres. Defaults to zero, an exact match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_tolerance: Option<f64>,
    /// Nets belonging to this class.
    #[serde(default)]
    pub nets: Vec<Id>,
}
/// A clearance requirement between two named nets, stricter than either
/// class or the board default when it is larger.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NetPairRule {
    /// Ids of the two nets the rule sits between.
    pub a: Id,
    pub b: Id,
    /// Minimum copper separation between the two nets, in millimetres.
    pub clearance: f64,
}
impl NetPairRule {
    fn valid(&self) -> bool {
        self.a != self.b
            && self.a != 0
            && self.b != 0
            && self.clearance.is_finite()
            && self.clearance > 0.0
            && self.clearance <= 100.0
    }
}
/// A differential pair: two named nets that carry one signal, routed side by
/// side with a fixed coupling gap and, optionally, matched total lengths.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiffPair {
    pub id: Id,
    /// Pair name, for example "USB_D".
    pub name: String,
    /// The positive-side net.
    pub net_p: Id,
    /// The negative-side net; must differ from the positive side.
    pub net_n: Id,
    /// Distance the two sides should keep from each other, in millimetres.
    /// Copper closer than this reports a coupling finding.
    pub gap: f64,
    /// How far the two routed lengths may differ, in millimetres; absent
    /// means length matching is not checked for this pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_tolerance: Option<f64>,
}
impl DiffPair {
    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= 64
            && self.net_p != self.net_n
            && self.net_p != 0
            && self.net_n != 0
            && self.gap.is_finite()
            && (0.02..=10.0).contains(&self.gap)
            && self
                .length_tolerance
                .is_none_or(|t| t.is_finite() && (0.0..=100.0).contains(&t))
    }
}
/// A rectangular board area that demands a stricter minimum copper
/// separation: copper lying wholly inside the rectangle, on the matching
/// layer and net when those filters are set, must clear by at least the
/// area's gap. The area raises the required clearance; it never lowers the
/// net-pair requirement.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RuleArea {
    pub id: Id,
    /// Optional note describing the area, such as a sensor or high-voltage
    /// zone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// When set, the area only constrains copper of this net on one side of
    /// each pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Id>,
    /// When set, the area only constrains copper on this copper layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<Layer>,
    /// Minimum copper separation demanded inside the area, in millimetres.
    pub clearance: f64,
    /// When set, components inside the area may not stand taller than this,
    /// in millimetres: a height restriction for boards that must fit under a
    /// cover or past another board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<f64>,
    /// The rectangle itself.
    pub region: RuleRegion,
}
impl RuleArea {
    fn valid(&self) -> bool {
        self.id != 0
            && self.clearance.is_finite()
            && self.clearance > 0.0
            && self.clearance <= 100.0
            && self
                .name
                .as_ref()
                .is_none_or(|n| !n.is_empty() && n.len() <= 64)
            && self.region.width.is_finite()
            && self.region.height.is_finite()
            && self.region.width > 0.0
            && self.region.height > 0.0
            && self.region.width <= 1000.0
            && self.region.height <= 1000.0
            && self.region.x >= -1000.0
            && self.region.y >= -1000.0
            && self.region.x + self.region.width <= 1000.0
            && self.region.y + self.region.height <= 1000.0
            && self.max_height.is_none_or(|h| (0.1..=100.0).contains(&h))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PinRef {
    pub component: Id,
    pub pin: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Wire {
    pub id: Id,
    pub net: Id,
    pub from: PinRef,
    pub to: PinRef,
}
/// A named annotation attached to a logical net: a local, global,
/// hierarchical or power label. Connectivity stays explicit: the label names a
/// net that a command already assigned; it never creates a net by matching names.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Label {
    pub id: Id,
    /// `label`, `global`, `hierarchical`, `power` or `junction`.
    pub kind: String,
    /// Logical net the label names.
    pub net: Id,
    /// Anchor position in millimetres. A junction stores its point on a wire.
    pub position: Point,
    /// Text rotation in degrees, ignored for junctions.
    pub rotation: f64,
    /// Bus notation carried by a bus label, like DB*8; empty for every other
    /// kind. A bus label names a group of nets, so it carries no single net.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}
impl Label {
    pub const KINDS: [&'static str; 6] = [
        "label",
        "global",
        "hierarchical",
        "power",
        "junction",
        "bus",
    ];
}
/// A flag marking a pin intentionally unconnected, that
/// suppresses its unconnected-pin violation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NoConnectFlag {
    pub id: Id,
    pub pin: PinRef,
}
/// A named hierarchy port placed on a schematic page, declaring that a net
/// enters or leaves that page here. Ports sharing a name on different pages
/// carry the same net; the engine enforces that when placing them.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub id: Id,
    pub name: String,
    pub net: Id,
    /// The page the port sits on; 0 is the root page.
    pub sheet: Id,
    pub position: Point,
}
/// Free-form schematic wire drawn by the user with explicit vertices.
/// Connectivity stays explicit: `net` is assigned by a command, never inferred from crossings.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DrawnWire {
    pub id: Id,
    /// Logical net carried by this wire, or none until it is assigned.
    pub net: Option<Id>,
    /// Vertices in draw order, in millimetres.
    pub points: Vec<Point>,
}
/// A free-standing drawing primitive for schematic annotations and board
/// graphics. Shapes are explicit; nothing is inferred from geometry.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Graphic {
    pub id: Id,
    /// `schematic` or `board`.
    pub layer: String,
    /// `line`, `rect`, `circle`, `arc`, `polygon` or `text`.
    pub shape: String,
    /// Vertices in draw order, in millimetres. A circle stores its centre first,
    /// an arc stores start, middle and end, and text stores its anchor.
    pub points: Vec<Point>,
    /// Stroke width in millimetres, circle and arc radius, or text size.
    pub width: f64,
    /// Text content for the text shape, empty for every other shape.
    pub text: String,
}
impl Graphic {
    /// Required vertex count for each shape, or none for an unknown shape.
    pub fn vertex_rule(shape: &str) -> Option<(usize, usize)> {
        match shape {
            "line" | "rect" => Some((2, 2)),
            "arc" => Some((3, 3)),
            "circle" | "text" => Some((1, 1)),
            "polygon" => Some((3, 256)),
            _ => None,
        }
    }
    /// Polyline used to render the shape, or none for text.
    pub fn outline(&self) -> Option<Vec<Point>> {
        let (min, _) = Self::vertex_rule(self.shape.as_str())?;
        if self.points.len() < min {
            return None;
        }
        let first = self.points.first().copied()?;
        match self.shape.as_str() {
            "line" => Some(self.points.clone()),
            "rect" => {
                let opposite = self.points.get(1).copied()?;
                Some(vec![
                    first,
                    Point::new(opposite.x, first.y),
                    opposite,
                    Point::new(first.x, opposite.y),
                    first,
                ])
            }
            "polygon" => {
                let mut points = self.points.clone();
                points.push(first);
                Some(points)
            }
            "circle" => {
                let start = Point::new(first.x + self.width, first.y);
                Some(self.arc_points(first, start, start, self.width, 0.0, 360.0))
            }
            "arc" => {
                let middle = self.points.get(1).copied()?;
                let last = self.points.get(2).copied()?;
                let (centre, radius) = circle_through(first, middle, last)?;
                let angle = |p: Point| (p.y - centre.y).atan2(p.x - centre.x).to_degrees();
                let (start, mid, end) = (angle(first), angle(middle), angle(last));
                // The middle point decides which way the arc sweeps.
                let counter_clockwise = on_arc(start, end, mid);
                let sweep = if counter_clockwise {
                    wrap_360(end - start)
                } else {
                    wrap_360(end - start) - 360.0
                };
                Some(self.arc_points(centre, first, last, radius, start, sweep))
            }
            _ => None,
        }
    }
    /// Samples a circular path without any trigonometric discontinuity at the ends:
    /// the first and last samples are the exact supplied endpoints.
    fn arc_points(
        &self,
        centre: Point,
        start: Point,
        end: Point,
        radius: f64,
        start_degrees: f64,
        sweep_degrees: f64,
    ) -> Vec<Point> {
        if !radius.is_finite() || radius <= 0.0 || !sweep_degrees.is_finite() {
            return vec![start];
        }
        let steps = (sweep_degrees.abs() / 15.0).ceil() as usize;
        let steps = steps.clamp(1, 256);
        let mut points = Vec::with_capacity(steps + 1);
        points.push(start);
        for step in 1..steps {
            let angle = start_degrees + sweep_degrees * step as f64 / steps as f64;
            let (sin, cos) = angle.to_radians().sin_cos();
            points.push(Point::new(centre.x + radius * cos, centre.y + radius * sin));
        }
        points.push(end);
        points
    }
}
/// Centre and radius of the unique circle through three points, or none when the
/// points are collinear.
pub fn circle_through(a: Point, b: Point, c: Point) -> Option<(Point, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if !d.is_finite() || d.abs() < 1e-9 {
        return None;
    }
    let (a2, b2, c2) = (
        a.x * a.x + a.y * a.y,
        b.x * b.x + b.y * b.y,
        c.x * c.x + c.y * c.y,
    );
    let centre = Point::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    );
    if !centre.valid() {
        return None;
    }
    let radius = centre.distance(a);
    if !positive(radius) {
        return None;
    }
    Some((centre, radius))
}
/// Wraps degrees into 0..360 so a sweep length can be computed.
fn wrap_360(degrees: f64) -> f64 {
    let value = degrees % 360.0;
    if value < 0.0 { value + 360.0 } else { value }
}
/// Whether an angle lies on the counter-clockwise arc from start to end.
fn on_arc(start: f64, end: f64, test: f64) -> bool {
    wrap_360(test - start) <= wrap_360(end - start)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pad {
    pub number: String,
    pub offset: Point,
    pub diameter: f64,
    pub net: Option<Id>,
    /// True when this pad is castellated: plated and cut at a board edge so
    /// another board can be soldered onto it. Absent means not castellated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub castellated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<pcbcraft_library::FootprintPad>,
}
impl Pad {
    pub fn drill_outline_points(&self) -> Vec<Point> {
        let Some(g) = &self.geometry else {
            return vec![];
        };
        let Some(drill) = &g.drill else { return vec![] };
        let mut hole = g.clone();
        hole.size = drill.size;
        hole.shape = "oval".into();
        hole.roundrect_ratio = None;
        hole.chamfer = None;
        hole.polygon.clear();
        hole.position = self.offset.offset(drill.offset.rotate(-g.rotation));
        Pad {
            number: String::new(),
            offset: hole.position,
            diameter: drill.size.x.min(drill.size.y),
            net: None,
            castellated: None,
            geometry: Some(hole),
        }
        .outline_points()
    }
    /// Maximum chord approximation error of the rounded copper outline.
    pub fn outline_error(&self) -> f64 {
        self.geometry
            .as_ref()
            .map(|p| {
                let radius = match p.shape.as_str() {
                    "oval" => p.size.x.min(p.size.y) / 2.0,
                    "roundrect" => p.size.x.min(p.size.y) * p.roundrect_ratio.unwrap_or(0.0),
                    _ => 0.0,
                };
                radius * (1.0 - (std::f64::consts::PI / 64.0).cos())
            })
            .unwrap_or(0.0)
    }
    /// Closed copper outline in footprint-local coordinates. Every shape except
    /// `custom` is convex; custom outlines are simple polygons.
    pub fn outline_points(&self) -> Vec<Point> {
        let Some(g) = &self.geometry else {
            return self.finish_outline(rounded_rect(
                Point::new(self.diameter, self.diameter),
                self.diameter / 2.0,
            ));
        };
        let points = match g.shape_kind() {
            PadShape::Circle | PadShape::Oval => rounded_rect(g.size, g.size.x.min(g.size.y) / 2.0),
            PadShape::RoundRect { ratio } => rounded_rect(g.size, g.size.x.min(g.size.y) * ratio),
            PadShape::Rect | PadShape::Unsupported => rounded_rect(g.size, 0.0),
            PadShape::Chamfered(chamfer) => chamfered_rect(g.size, chamfer),
            PadShape::Custom { polygon } => polygon.to_vec(),
        };
        let mut points = self.finish_outline(points);
        for p in &mut points {
            *p = self.offset.offset(p.rotate(-g.rotation));
        }
        points
    }
    fn finish_outline(&self, mut points: Vec<Point>) -> Vec<Point> {
        if let Some(first) = points.first().copied() {
            points.push(first);
        }
        if self.geometry.is_none() {
            for p in &mut points {
                *p = self.offset.offset(*p);
            }
        }
        points
    }
    /// True when the outline can be filled as one convex polygon.
    pub fn outline_is_convex(&self) -> bool {
        !self
            .geometry
            .as_ref()
            .is_some_and(|g| matches!(g.shape_kind(), PadShape::Custom { .. }))
    }
    /// Radius of a disc about the pad origin that is certainly copper; used for
    /// conservative connectivity.
    pub fn contact_radius(&self) -> f64 {
        let Some(g) = &self.geometry else {
            return self.diameter / 2.0;
        };
        match g.shape_kind() {
            PadShape::Chamfered(chamfer) => {
                let (hx, hy) = (g.size.x / 2.0, g.size.y / 2.0);
                let cut = g.size.x.min(g.size.y) * chamfer.ratio;
                hx.min(hy)
                    .min((hx + hy - cut) / std::f64::consts::SQRT_2)
                    .max(0.0)
            }
            _ => self.diameter / 2.0,
        }
    }
    pub fn bounding_radius(&self) -> f64 {
        self.geometry
            .as_ref()
            .map(|p| match p.shape_kind() {
                PadShape::Circle => p.size.x / 2.0,
                PadShape::Custom { polygon } => polygon
                    .iter()
                    .map(|v| v.x.hypot(v.y))
                    .fold(p.size.x.hypot(p.size.y) / 2.0, f64::max),
                _ => p.size.x.hypot(p.size.y) / 2.0,
            })
            .unwrap_or(self.diameter / 2.0)
    }
}
/// Open vertex ring of a rectangle with circular corners of the given radius.
fn rounded_rect(size: Point, radius: f64) -> Vec<Point> {
    let hx = size.x / 2.0;
    let hy = size.y / 2.0;
    if radius <= 1e-9 {
        return vec![
            Point::new(-hx, -hy),
            Point::new(hx, -hy),
            Point::new(hx, hy),
            Point::new(-hx, hy),
        ];
    }
    let mut points = Vec::with_capacity(68);
    for (x, y, start) in [
        (hx - radius, hy - radius, 0.0),
        (-hx + radius, hy - radius, 90.0),
        (-hx + radius, -hy + radius, 180.0),
        (hx - radius, -hy + radius, 270.0),
    ] {
        for i in 0..=16 {
            let angle = (start + i as f64 * 90.0 / 16.0_f64).to_radians();
            points.push(Point::new(
                x + radius * angle.cos(),
                y + radius * angle.sin(),
            ));
        }
    }
    points
}
/// Open vertex ring of a rectangle with straight cuts at the selected corners.
/// Corner names are pad-local with y pointing down.
pub fn chamfered_rect(size: Point, chamfer: pcbcraft_library::PadChamfer) -> Vec<Point> {
    let hx = size.x / 2.0;
    let hy = size.y / 2.0;
    let cut = size.x.min(size.y) * chamfer.ratio;
    let c = chamfer.corners;
    let mut points = Vec::with_capacity(8);
    let mut corner = |cut_it: bool, square: Point, before: Point, after: Point| {
        if cut_it {
            points.extend([before, after]);
        } else {
            points.push(square);
        }
    };
    corner(
        c.bottom_right,
        Point::new(hx, hy),
        Point::new(hx, hy - cut),
        Point::new(hx - cut, hy),
    );
    corner(
        c.bottom_left,
        Point::new(-hx, hy),
        Point::new(-hx + cut, hy),
        Point::new(-hx, hy - cut),
    );
    corner(
        c.top_left,
        Point::new(-hx, -hy),
        Point::new(-hx, -hy + cut),
        Point::new(-hx + cut, -hy),
    );
    corner(
        c.top_right,
        Point::new(hx, -hy),
        Point::new(hx - cut, -hy),
        Point::new(hx, -hy + cut),
    );
    // A 0.5 ratio on a square side makes neighbouring cuts meet; drop the
    // repeated meeting vertices so the ring stays simple.
    points.dedup_by(|a, b| a.distance(*b) < 1e-12);
    if points.len() > 1
        && points
            .first()
            .zip(points.last())
            .is_some_and(|(a, b)| a.distance(*b) < 1e-12)
    {
        points.pop();
    }
    points
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Footprint {
    pub id: Id,
    pub component: Id,
    pub position: Point,
    pub rotation: f64,
    /// True when this footprint is locked in place so placement commands refuse
    /// to move it. Absent means unlocked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    pub pads: Vec<Pad>,
}
impl Footprint {
    pub fn drawing_points(&self, project: &Project) -> Vec<Point> {
        let mut points: Vec<Point> = self.pads.iter().flat_map(|p| p.outline_points()).collect();
        if let Some(library) = project
            .component(self.component)
            .and_then(|c| c.library_footprint.as_ref())
        {
            for graphic in &library.definition.graphics {
                points.extend(graphic.path_points());
            }
        } else {
            points.extend([Point::new(-3.75, -1.5), Point::new(3.75, 1.5)]);
        }
        points
    }
    pub fn pad_position(&self, number: &str) -> Option<Point> {
        self.pads
            .iter()
            .find(|p| p.number == number)
            .map(|p| self.position.offset(p.offset.rotate(-self.rotation)))
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Layer {
    #[serde(rename = "F.Cu")]
    Front,
    #[serde(rename = "B.Cu")]
    Back,
}
impl Layer {
    pub fn name(self) -> &'static str {
        match self {
            Self::Front => "F.Cu",
            Self::Back => "B.Cu",
        }
    }
    /// Plain-English layer name for people who have never seen copper-layer
    /// shorthand; file formats keep using [`Self::name`].
    pub fn label(self) -> &'static str {
        match self {
            Self::Front => "Top",
            Self::Back => "Bottom",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: Id,
    pub net: Id,
    pub start: Point,
    pub end: Point,
    pub width: f64,
    pub layer: Layer,
    /// Midpoint of a circular arc through start and end; absent means a
    /// straight segment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc: Option<Point>,
}
impl Track {
    /// The circular arc if the track stores a usable midpoint.
    pub fn curve(&self) -> Option<pcbcraft_geom::Arc> {
        pcbcraft_geom::arc_through(self.start, self.arc?, self.end)
    }
    /// Routed length in millimetres: the arc if curved, the chord otherwise.
    pub fn length(&self) -> f64 {
        self.curve()
            .map_or_else(|| self.start.distance(self.end), |arc| arc.length())
    }
}
/// A plated through-hole via joining the front and back copper layers. A via
/// is one physical pad plated through the board, so it conducts on both layers.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Via {
    pub id: Id,
    pub net: Id,
    /// Centre position in millimetres.
    pub position: Point,
    /// Outer copper pad diameter in millimetres.
    pub diameter: f64,
    /// Drilled hole diameter in millimetres; zero keeps an unplated blind hole.
    pub drill: f64,
}
/// A solid copper region on one layer, bounded by a closed polygon and
/// belonging to a net. Same-net copper that falls inside the outline conducts
/// to the zone; the zone keeps its clearance from copper of other nets.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CopperZone {
    pub id: Id,
    pub net: Id,
    pub layer: Layer,
    /// Closed outline in millimetres: at least three distinct vertices with the
    /// first vertex repeated last.
    pub outline: Vec<Point>,
    /// Closed outlines removed from this zone: copper of other nets, or empty
    /// board, inside a cutout never conducts through the zone.
    #[serde(default)]
    pub cutouts: Vec<Vec<Point>>,
    /// Whether the zone is poured. An unfilled zone is drawn as a bare
    /// outline and carries no copper: it neither conducts to net copper
    /// inside it nor demands clearance from it until it is filled again.
    #[serde(default = "poured")]
    pub filled: bool,
    /// Optional tag naming the tool that generated this copper, such as
    /// "teardrop", so the pass can find and redo its own shapes. Absent for
    /// hand-drawn zones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cutout {
    pub id: Id,
    pub center: Point,
    pub radius: f64,
}
/// A rectangular area that silences one check code for objects inside it.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct RuleRegion {
    /// Lower-left corner in millimetres.
    pub x: f64,
    pub y: f64,
    /// Extent in millimetres; must be positive.
    pub width: f64,
    pub height: f64,
}
/// A text label drawn on the board, such as an assembly note or a marking.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BoardText {
    pub id: Id,
    /// The exact string to draw.
    pub text: String,
    /// Anchor position in millimetres: the lower-left of the first glyph.
    pub position: Point,
    /// Glyph height in millimetres.
    pub size: f64,
    /// Board side the label belongs to.
    pub layer: Layer,
    /// Text rotation in degrees about the anchor.
    #[serde(default)]
    pub rotation: f64,
    /// The component whose reference, value, footprint name and field keys
    /// resolve `${...}` tokens in this text at draw and export time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<Id>,
}
/// A named fabrication output recipe: the export kinds a job writes when it
/// runs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OutputJob {
    pub name: String,
    /// Export kinds, each one of: gerbers, netlist, bom, placement, dxf,
    /// cmp, idf, vrml, step, pdf, svg.
    pub kinds: Vec<String>,
}
impl OutputJob {
    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= 64
            && !self.kinds.is_empty()
            && self.kinds.len() <= 24
            && self.kinds.iter().all(|k| {
                matches!(
                    k.as_str(),
                    "gerbers"
                        | "netlist"
                        | "bom"
                        | "placement"
                        | "dxf"
                        | "cmp"
                        | "idf"
                        | "vrml"
                        | "step"
                        | "pdf"
                        | "svg"
                        | "ipc2581"
                        | "dsn"
                        | "ipc356"
                        | "gencad"
                        | "hyperlynx"
                        | "plot"
                        | "report"
                        | "ps"
                        | "odb"
                        | "hpgl"
                        | "stackup"
                        | "drill_chart"
                        | "stats"
                )
            })
    }
}
/// An unplated through hole: a drill that passes through the board without
/// plating or a net.
/// A net tie: two touching round pads that bridge exactly two nets at one
/// point, used to split a plane while keeping a single controlled crossing.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NetTie {
    pub id: Id,
    /// The two nets this tie bridges; they must differ.
    pub net_a: Id,
    pub net_b: Id,
    /// Centre of the bridge, midway between the two pads, in millimetres.
    pub position: Point,
    /// Angle of the pad-to-pad axis in degrees from the x axis.
    #[serde(default)]
    pub rotation: f64,
    /// Diameter of each of the two round pads, in millimetres.
    pub diameter: f64,
}
impl NetTie {
    fn valid(&self) -> bool {
        self.position.valid()
            && (0.3..=20.0).contains(&self.diameter)
            && self.rotation.is_finite()
            && self.net_a != self.net_b
    }
}
/// An optical assembly mark: a bare copper circle, usually 1 mm, that the
/// pick-and-place camera uses to locate and orient the board.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Fiducial {
    pub id: Id,
    /// Centre of the mark, in millimetres.
    pub position: Point,
    /// Diameter of the copper disc, in millimetres.
    pub diameter: f64,
    /// Copper layer the mark sits on.
    #[serde(default = "front_layer")]
    pub layer: Layer,
}
impl Fiducial {
    fn valid(&self) -> bool {
        self.position.valid() && (0.2..=10.0).contains(&self.diameter)
    }
}
fn front_layer() -> Layer {
    Layer::Front
}
/// An unplated through hole: a drill that passes through the board without
/// plating or a net.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Hole {
    pub id: Id,
    pub position: Point,
    /// Drill diameter in millimetres.
    pub diameter: f64,
}
impl Hole {
    fn valid(&self) -> bool {
        self.position.valid() && (0.05..=25.0).contains(&self.diameter)
    }
}
/// One named board layer record, kept for fabrication notes: a layer's role
/// and thickness as stated by the stackup the design was built against.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LayerRecord {
    /// Display name, for example "F.Cu", "Core" or "L2".
    pub name: String,
    /// Layer role: copper, core, prepreg, soldermask, silkscreen, paste or custom.
    pub kind: String,
    /// Layer thickness in millimetres.
    pub thickness: f64,
}
impl LayerRecord {
    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= 64
            && matches!(
                self.kind.as_str(),
                "copper" | "core" | "prepreg" | "soldermask" | "silkscreen" | "paste" | "custom"
            )
            && self.thickness.is_finite()
            && (0.0..=100.0).contains(&self.thickness)
    }
}
/// What a drawn layer tells a machine to do: a cutter follows cut and
/// perforation layers, a UV printer prints silk and mask layers, and "other"
/// layers are reference or documentation art.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtKind {
    #[serde(rename = "cut")]
    Cut,
    #[serde(rename = "perforation")]
    Perforation,
    #[serde(rename = "silk")]
    Silk,
    #[serde(rename = "mask")]
    Mask,
    #[serde(rename = "other")]
    Other,
}
impl ArtKind {
    /// Plain-English name of the job this layer gives a machine.
    pub fn label(self) -> &'static str {
        match self {
            Self::Cut => "cut line",
            Self::Perforation => "perforation",
            Self::Silk => "silk screen",
            Self::Mask => "solder mask",
            Self::Other => "drawing",
        }
    }
}
/// One figure drawn on a user layer, in board millimetres.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ArtShape {
    /// A straight stroke between two points.
    Line { start: Point, end: Point },
    /// A rectangle outline, optionally rotated about its centre.
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        /// Turn in degrees about the rectangle centre.
        #[serde(default)]
        rotate: f64,
    },
    /// A circle outline.
    Circle { center: Point, radius: f64 },
    /// A closed outline through these points.
    Poly { points: Vec<Point> },
    /// Text drawn with the machine's pen.
    Text {
        at: Point,
        text: String,
        /// Cap height in millimetres.
        #[serde(default = "art_text_size")]
        size: f64,
        /// Turn in degrees about the anchor point.
        #[serde(default)]
        rotate: f64,
    },
}
impl ArtShape {
    fn valid(&self) -> bool {
        match self {
            Self::Line { start, end } => start.valid() && end.valid(),
            Self::Rect {
                x,
                y,
                width,
                height,
                rotate,
            } => {
                x.is_finite()
                    && y.is_finite()
                    && (0.001..=1000.0).contains(width)
                    && (0.001..=1000.0).contains(height)
                    && rotate.is_finite()
                    && *x >= -1000.0
                    && *y >= -1000.0
                    && *x + *width <= 2000.0
                    && *y + *height <= 2000.0
            }
            Self::Circle { center, radius } => center.valid() && (0.001..=500.0).contains(radius),
            Self::Poly { points } => {
                (2..=64).contains(&points.len()) && points.iter().all(|pt| pt.valid())
            }
            Self::Text {
                at,
                text,
                size,
                rotate,
            } => {
                at.valid()
                    && !text.trim().is_empty()
                    && text.len() <= 128
                    && (0.2..=50.0).contains(size)
                    && rotate.is_finite()
            }
        }
    }
}
/// A layer the designer draws: card outlines to cut, perforations to snap
/// along, custom silk screens, masks or reference art. Copper stays where it
/// is; these layers sit beside it and are plotted for laser, mill and
/// printer output.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArtLayer {
    pub id: Id,
    /// Display name, unique per board, for example "Card Outline".
    pub name: String,
    /// What this layer tells a machine to do.
    pub kind: ArtKind,
    /// True draws the layer as evenly spaced dots: a perforation the board
    /// snaps along, or a light print guide.
    #[serde(default)]
    pub dotted: bool,
    /// False hides the layer everywhere on screen.
    #[serde(default = "art_visible")]
    pub visible: bool,
    /// The figures on this layer, in draw order.
    #[serde(default)]
    pub shapes: Vec<ArtShape>,
}
impl ArtLayer {
    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= 64
            && self.shapes.len() <= 512
            && self.shapes.iter().all(ArtShape::valid)
    }
}
fn art_visible() -> bool {
    true
}
fn art_text_size() -> f64 {
    1.0
}
fn board_finish() -> String {
    "HASL".into()
}
fn board_er() -> f64 {
    4.4
}
fn board_origin() -> Point {
    Point::default()
}
/// Machining notes recorded for fabrication: whether the board edges are
/// plated and the tolerances the mill and drill are held to.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct MachiningNotes {
    /// True when board edges are plated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_plating: Option<bool>,
    /// Drill hole diameter tolerance in millimetres, plus or minus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drill_tolerance: Option<f64>,
    /// Rout/edge dimension tolerance in millimetres, plus or minus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_tolerance: Option<f64>,
}
impl MachiningNotes {
    fn valid(&self) -> bool {
        self.drill_tolerance
            .is_none_or(|t| t.is_finite() && (0.0..=1.0).contains(&t))
            && self
                .edge_tolerance
                .is_none_or(|t| t.is_finite() && (0.0..=1.0).contains(&t))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Board {
    pub width: f64,
    pub height: f64,
    pub thickness: f64,
    pub clearance: f64,
    /// Surface finish applied to exposed copper, for example HASL or ENIG.
    #[serde(default = "board_finish")]
    pub finish: String,
    /// Relative permittivity of the substrate, used by impedance estimates.
    #[serde(default = "board_er")]
    pub er: f64,
    /// Origin that drill file coordinates are measured from, in millimetres.
    #[serde(default = "board_origin")]
    pub drill_origin: Point,
    /// Recorded per-layer fabrication notes, ordered by layer name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layer_records: Vec<LayerRecord>,
    /// Machining notes for fabrication, absent when none were recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machining: Option<MachiningNotes>,
    /// Clearance rules between specific pairs of nets, ordered by net ids.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pair_rules: Vec<NetPairRule>,
    /// Differential pairs: named net couples that route together with a
    /// coupling gap and, when set, matched lengths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diff_pairs: Vec<DiffPair>,
    /// Rectangular board areas that demand a stricter minimum copper
    /// separation for copper whose midpoints lie inside them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rule_areas: Vec<RuleArea>,
    /// Free-form closed board outline in millimetres, ordered around the
    /// perimeter and implicitly closed. Absent means the plain rectangle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Vec<Point>>,
    pub footprints: Vec<Footprint>,
    pub tracks: Vec<Track>,
    /// Plated through-hole vias joining front and back copper.
    #[serde(default)]
    pub vias: Vec<Via>,
    /// Optical assembly marks: bare copper circles the pick-and-place camera
    /// finds the board by.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fiducials: Vec<Fiducial>,
    /// Net-tie pads: two touching round pads bridging exactly two nets,
    /// ordered by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub netties: Vec<NetTie>,
    /// Unplated through holes drilled for mounting or clearance; they carry
    /// no copper and belong to no net.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Hole>,
    /// Layers the designer draws for machines: cuts, perforations, custom
    /// silk and mask art, plotted alongside the copper.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_layers: Vec<ArtLayer>,
    /// Poured copper zones, one polygon per zone on a single layer.
    #[serde(default)]
    pub zones: Vec<CopperZone>,
    pub cutouts: Vec<Cutout>,
    /// Text labels drawn on the board.
    #[serde(default)]
    pub texts: Vec<BoardText>,
}
/// One extra bill-of-materials column defined by the project: which
/// component property to read and the header to print for it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BomField {
    /// Component property key to read, for example "MPN".
    pub key: String,
    /// Column header to print; falls back to the key when empty.
    #[serde(default)]
    pub label: String,
}
impl BomField {
    fn valid(&self) -> bool {
        !self.key.trim().is_empty() && self.key.len() <= 64 && self.label.len() <= 64
    }
}
/// A text file attached to the project, for example a licence note or a
/// datasheet excerpt, stored verbatim so it travels with the design.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EmbeddedFile {
    pub name: String,
    pub data: String,
}
impl EmbeddedFile {
    fn valid(&self) -> bool {
        !self.name.trim().is_empty() && self.name.len() <= 128 && self.data.len() <= 262_144
    }
}
/// A set of drawn graphics that translate together as one unit, so a
/// multi-graphic annotation can be repositioned in a single step.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GraphicGroup {
    pub id: Id,
    pub graphics: Vec<Id>,
}
impl GraphicGroup {
    fn valid(&self) -> bool {
        self.graphics.len() >= 2 && self.graphics.len() <= 512 && {
            let unique: BTreeSet<Id> = self.graphics.iter().copied().collect();
            unique.len() == self.graphics.len()
        }
    }
}
/// An assembly variant of the design: a named alternate build in which a
/// chosen set of components is marked do not populate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Variant {
    /// Variant name, unique within the project.
    pub name: String,
    /// Optional note describing what this variant changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Ids of components this variant leaves unpopulated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dnp: Vec<Id>,
    /// Components this variant populates with a different package, ordered by
    /// component id. A variant bill of materials skips unpopulated parts and
    /// prints these swaps in place of the footprints the design places.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footprints: Vec<FootprintOverride>,
}
impl Variant {
    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= 64
            && self.note.as_ref().is_none_or(|n| n.len() <= 256)
            && self.dnp.len() <= 2048
            && self.footprints.len() <= 512
            && self.footprints.iter().all(|o| {
                !o.footprint.trim().is_empty() && o.footprint.len() <= 128 && o.component != 0
            })
    }
}
/// One component's package swap inside an assembly variant: this build
/// populates that component position with the named footprint instead of the
/// one the design places.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FootprintOverride {
    pub component: Id,
    pub footprint: String,
}
fn default_project_unit() -> String {
    "mm".into()
}
/// One row of the project library table: a nickname mapped to the folder
/// that holds library files, so library commands can address
/// `nickname/file.kicad_mod` without a machine-specific folder.
/// One named, reusable pad cross-section: the shape, size and optional drill
/// a footprint pad takes on when the stack is applied to it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Padstack {
    /// Name used by commands to pick this stack.
    pub name: String,
    /// Copper shape: "circle", "rect", "oval" or "roundrect".
    pub shape: String,
    /// Copper extent in millimetres, x by y.
    pub size: Point,
    /// Drill diameter in millimetres; absent means an un-drilled surface pad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drill: Option<f64>,
    /// Corner rounding ratio for roundrect copper, 0 to 0.5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roundrect_ratio: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LibraryLink {
    /// Nickname used as the first path segment by library commands.
    pub name: String,
    /// Folder that holds the library files for this nickname.
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub format: String,
    pub version: u32,
    pub name: String,
    /// The unit this design is read in: "mm", "mil" or "inch". Stored
    /// coordinates stay millimetres; inch fabrication output is converted,
    /// while mil is a reading and reporting convention.
    #[serde(default = "default_project_unit")]
    pub unit: String,
    pub components: Vec<Component>,
    pub nets: Vec<Net>,
    /// Named net classes carrying clearance and default width rules.
    #[serde(default)]
    pub net_classes: Vec<NetClass>,
    /// Named schematic pages, ordered by id; sheet 0 is the implicit root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sheets: Vec<Sheet>,
    /// Named hierarchy ports, ordered by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<Port>,
    /// Extra bill-of-materials columns, in print order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bom_fields: Vec<BomField>,
    /// Text files attached to the project, ordered by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<EmbeddedFile>,
    /// Assembly variants, ordered by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<Variant>,
    /// Groups of drawn graphics that translate together, ordered by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphic_groups: Vec<GraphicGroup>,
    /// Named fabrication output recipes, ordered by name, re-runnable after
    /// any edit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_jobs: Vec<OutputJob>,
    /// Checks the designer wrote themselves, in the order they were made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_checks: Vec<CustomCheck>,
    /// Free-form project metadata such as title, revision, company or notes,
    /// keyed by lower-case name and carried with the saved design.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
    /// Per-check-code severity overrides: "error", "warning" or "off" to
    /// silence a check whose findings this design accepts as normal.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rule_severity: BTreeMap<String, String>,
    /// Per-check-code object exclusions: a finding for one of these codes is
    /// dropped when every object it names is on that code's exclusion list.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rule_exclusions: BTreeMap<String, Vec<Id>>,
    /// Per-check-code rectangular areas: a finding for one of these codes is
    /// dropped when every object it names sits inside one of its rectangles.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rule_regions: BTreeMap<String, Vec<RuleRegion>>,
    /// Companion-part intent rules, from profiles or edited on the project.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companion_rules: Vec<CompanionRule>,
    /// Named library folders that library commands resolve through: a path
    /// whose first segment matches a nickname is rewritten to the mapped
    /// folder, so the design opens on another machine without retyping it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub library_table: Vec<LibraryLink>,
    /// Named reusable pad shapes that footprint pads can adopt, so one
    /// fabrication decision (a part's copper and drill) is defined once and
    /// applied to many pads instead of retyped per footprint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub padstacks: Vec<Padstack>,
    pub wires: Vec<Wire>,
    #[serde(default)]
    pub drawn_wires: Vec<DrawnWire>,
    /// Free-standing drawing primitives for schematic and board graphics.
    #[serde(default)]
    pub graphics: Vec<Graphic>,
    /// Named net annotations placed on the schematic.
    #[serde(default)]
    pub labels: Vec<Label>,
    /// Pins explicitly marked unconnected.
    #[serde(default)]
    pub no_connects: Vec<NoConnectFlag>,
    pub board: Board,
    #[serde(default)]
    pub library: ProjectLibrary,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectLibrary {
    #[serde(default)]
    pub notices: Vec<String>,
    pub symbols: Vec<pcbcraft_library::SymbolDefinition>,
    pub footprints: Vec<pcbcraft_library::FootprintDefinition>,
}
impl ProjectLibrary {
    pub fn validate(&self) -> Result<(), String> {
        if self.notices.len() > 256 || self.notices.iter().any(|s| s.len() > 128 * 1024) {
            return Err("library provenance notice limit exceeded".into());
        }
        if self.symbols.len() > 256 || self.footprints.len() > 256 {
            return Err("project library supports at most 256 symbols and 256 footprints".into());
        }
        let mut names = BTreeSet::new();
        for s in &self.symbols {
            s.validate().map_err(|e| e.to_string())?;
            if !names.insert(&s.name) {
                return Err("duplicate library symbol name".into());
            }
        }
        names.clear();
        for f in &self.footprints {
            f.validate().map_err(|e| e.to_string())?;
            if !names.insert(&f.name) {
                return Err("duplicate library footprint name".into());
            }
        }
        Ok(())
    }
}
/// A closed polygon outline: 3-256 distinct vertices, the first repeated last,
/// finite in-range coordinates and nonzero enclosed area.
/// Area in square millimetres of an implicitly closed polygon, by the
/// shoelace formula; zero for polygons with fewer than three vertices.
pub fn outline_area(points: &[Point]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for (i, a) in points.iter().enumerate() {
        let b = points[(i + 1) % points.len()];
        area += a.x * b.y - b.x * a.y;
    }
    (area / 2.0).abs()
}

pub fn validate_closed_outline(outline: &[Point]) -> Result<(), String> {
    if outline.len() < 4 || outline.len() > 257 {
        return Err("a zone outline needs 3 to 256 vertices".into());
    }
    if outline.first() != outline.last() {
        return Err("zone outline must be closed".into());
    }
    if !outline.iter().all(|pt| pt.valid()) {
        return Err("zone outline vertex is out of range".into());
    }
    let mut area = 0.0;
    for edge in outline.windows(2) {
        area += edge[0].x * edge[1].y - edge[1].x * edge[0].y;
    }
    if area.abs() < 1e-6 {
        return Err("zone outline has no area".into());
    }
    Ok(())
}
impl Default for Project {
    fn default() -> Self {
        Self {
            library_table: Vec::new(),
            padstacks: Vec::new(),
            library: ProjectLibrary::default(),
            format: "pcbcraft".into(),
            version: 1,
            name: "Untitled circuit".into(),
            unit: "mm".into(),
            components: vec![],
            nets: vec![],
            net_classes: vec![],
            sheets: vec![],
            ports: vec![],
            bom_fields: vec![],
            output_jobs: vec![],
            custom_checks: vec![],
            files: vec![],
            variants: vec![],
            graphic_groups: vec![],
            meta: Default::default(),
            rule_severity: Default::default(),
            rule_exclusions: Default::default(),
            rule_regions: Default::default(),
            companion_rules: vec![],
            wires: vec![],
            drawn_wires: vec![],
            graphics: vec![],
            labels: vec![],
            no_connects: vec![],
            board: Board {
                width: 60.0,
                height: 40.0,
                thickness: 1.6,
                clearance: 0.2,
                finish: board_finish(),
                er: board_er(),
                drill_origin: board_origin(),
                layer_records: vec![],
                machining: None,
                pair_rules: vec![],
                diff_pairs: vec![],
                rule_areas: vec![],
                user_layers: vec![],
                outline: None,
                footprints: vec![],
                tracks: vec![],
                vias: vec![],
                holes: vec![],
                fiducials: vec![],
                netties: vec![],
                zones: vec![],
                cutouts: vec![],
                texts: vec![],
            },
        }
    }
}

pub struct Template {
    pub kind: &'static str,
    pub label: &'static str,
    pub prefix: &'static str,
    pub value: &'static str,
}
/// Component kinds allowed on schematic parts without an embedded library symbol.
/// Starter templates are a subset; companion and profile rules may reference the rest.
pub const SCHEMATIC_KINDS: &[&str] = &[
    "resistor",
    "capacitor",
    "led",
    "connector",
    "connector_power",
    "mcu",
    "ic",
    "ldo",
    "buck",
    "relay",
    "fuse",
    "diode",
    "tvs",
    "esd",
    "inductor",
    "mosfet",
    "antenna",
    "crystal",
    "transceiver",
];
pub fn schematic_kind_valid(kind: &str) -> bool {
    TEMPLATES.iter().any(|t| t.kind == kind) || SCHEMATIC_KINDS.contains(&kind)
}
/// Bounded parse of a component value string for companion rule validation.
pub fn parse_companion_value(value: &str) -> Option<f64> {
    let mut s = value.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    s = s
        .replace('ω', "")
        .replace("ohms", "")
        .replace("ohm", "")
        .replace(' ', "");
    let (num, scale) = if let Some(stripped) = s.strip_suffix("pf") {
        (stripped, 1e-12)
    } else if let Some(stripped) = s.strip_suffix("nf") {
        (stripped, 1e-9)
    } else if let Some(stripped) = s.strip_suffix("uf") {
        (stripped, 1e-6)
    } else if let Some(stripped) = s.strip_suffix("mf") {
        (stripped, 1e-3)
    } else if let Some(stripped) = s.strip_suffix('m') {
        (stripped, 1e6)
    } else if let Some(stripped) = s.strip_suffix('k') {
        (stripped, 1e3)
    } else {
        (s.as_str(), 1.0)
    };
    let v: f64 = num.parse().ok()?;
    if !v.is_finite() || v <= 0.0 {
        return None;
    }
    Some(v * scale)
}
/// One companion-part rule persisted on the project or copied from a profile row.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CompanionRule {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub net_patterns: Vec<String>,
    pub required_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_min: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_max: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sink_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exactly: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Finding code to emit; defaults to `missing_companion` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding_code: Option<String>,
}
impl CompanionRule {
    fn valid(&self) -> bool {
        !self.id.trim().is_empty()
            && self.id.len() <= 64
            && !self.name.trim().is_empty()
            && self.name.len() <= 64
            && !self.required_kind.trim().is_empty()
            && self.required_kind.len() <= 32
            && self.net_patterns.len() <= 16
            && self.net_patterns.iter().all(|p| !p.is_empty() && p.len() <= 64)
            && self
                .value_min
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 32)
            && self
                .value_max
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 32)
            && self
                .before_kind
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 32)
            && self
                .anchor_kind
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 32)
            && self
                .sink_kind
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 32)
            && self.role.as_ref().is_none_or(|s| s.len() <= 32)
            && self
                .finding_code
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 32)
    }
}
pub const TEMPLATES: &[Template] = &[
    Template {
        kind: "resistor",
        label: "Resistor",
        prefix: "R",
        value: "1k",
    },
    Template {
        kind: "led",
        label: "LED",
        prefix: "D",
        value: "LED",
    },
    Template {
        kind: "capacitor",
        label: "Capacitor",
        prefix: "C",
        value: "100nF",
    },
    Template {
        kind: "connector",
        label: "2-pin connector",
        prefix: "J",
        value: "Power",
    },
];
pub fn component_template(
    kind: &str,
    id: Id,
    reference: String,
    position: Point,
) -> Option<Component> {
    let t = TEMPLATES.iter().find(|t| t.kind == kind)?;
    let names = if kind == "led" {
        ["A", "K"]
    } else if kind == "connector" {
        ["+", "−"]
    } else {
        ["1", "2"]
    };
    Some(Component {
        units: vec![],
        id,
        reference,
        value: t.value.into(),
        kind: kind.into(),
        position,
        rotation: 0.0,
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sheet: 0,
        pins: names
            .iter()
            .enumerate()
            .map(|(i, n)| Pin {
                number: (i + 1).to_string(),
                name: (*n).into(),
                offset: Point::new(if i == 0 { -5.0 } else { 5.0 }, 0.0),
                net: None,
                electrical_type: passive(),
                direction: None,
                unit: 1,
            })
            .collect(),
    })
}
impl Project {
    pub const UNITS: [&'static str; 3] = ["mm", "mil", "inch"];

    pub fn component(&self, id: Id) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }
    pub fn footprint(&self, id: Id) -> Option<&Footprint> {
        self.board.footprints.iter().find(|f| f.id == id)
    }
    pub fn pin(&self, r: &PinRef) -> Option<&Pin> {
        self.component(r.component)?
            .pins
            .iter()
            .find(|p| p.number == r.pin)
    }
    pub fn pin_position(&self, r: &PinRef) -> Option<Point> {
        self.component(r.component)?.pin_position(&r.pin)
    }
    /// Presentation-only orthogonal path that leaves pins in their outward direction.
    /// Wire crossings never implicitly connect nets; logical connectivity is explicit.
    pub fn wire_path(&self, w: &Wire) -> Option<Vec<Point>> {
        let a = self.pin_position(&w.from)?;
        let b = self.pin_position(&w.to)?;
        let direction = |r: &PinRef| -> Option<Point> {
            let pin = self.pin(r)?;
            let local = if let Some(angle) = pin.direction {
                Point::new(-angle.to_radians().cos(), angle.to_radians().sin())
            } else {
                pin.offset
            };
            Some(local.rotate(-self.component(r.component)?.unit_placement(pin.unit)?.1))
        };
        let da = direction(&w.from)?;
        let db = direction(&w.to)?;
        Some(orthogonal_path(a, b, da, db))
    }
    pub fn net_name(&self, id: Id) -> &str {
        self.nets
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.name.as_str())
            .unwrap_or("unassigned")
    }
    /// The net class a net belongs to, if it belongs to one.
    pub fn net_class(&self, net: Id) -> Option<&NetClass> {
        self.net_classes.iter().find(|c| c.nets.contains(&net))
    }
    /// Severity a check code should report at: the project override when it
    /// names one of the two real levels, otherwise the check's own default.
    pub fn rule_severity(&self, code: &str, default: &str) -> String {
        match self.rule_severity.get(code).map(String::as_str) {
            Some("error") | Some("warning") => self.rule_severity[code].clone(),
            _ => default.to_string(),
        }
    }
    /// True when a check code has been silenced for this design.
    pub fn rule_disabled(&self, code: &str) -> bool {
        self.rule_severity.get(code).is_some_and(|s| s == "off")
    }
    /// True when a finding for `code` naming exactly these objects has been
    /// excluded: every named object must sit on that code's exclusion list.
    pub fn rule_excluded(&self, code: &str, objects: &[Id]) -> bool {
        !objects.is_empty()
            && self
                .rule_exclusions
                .get(code)
                .is_some_and(|ids| objects.iter().all(|o| ids.contains(o)))
    }
    /// True when a finding of `code` naming `objects` falls inside one of
    /// that code's rectangular regions: every named object must have a
    /// position and all of them must sit inside the same rectangle.
    pub fn rule_region_excluded(&self, code: &str, objects: &[Id]) -> bool {
        let Some(regions) = self.rule_regions.get(code) else {
            return false;
        };
        !objects.is_empty()
            && regions.iter().any(|r| {
                objects.iter().all(|o| {
                    self.object_position(*o).is_some_and(|p| {
                        p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height
                    })
                })
            })
    }
    /// The position that represents a design object for region tests: track
    /// midpoints, via and footprint anchors, zone centroids, cutout centres.
    pub fn object_position(&self, id: Id) -> Option<Point> {
        let b = &self.board;
        if let Some(t) = b.tracks.iter().find(|t| t.id == id) {
            return Some(Point::new(
                (t.start.x + t.end.x) / 2.0,
                (t.start.y + t.end.y) / 2.0,
            ));
        }
        if let Some(v) = b.vias.iter().find(|v| v.id == id) {
            return Some(v.position);
        }
        if let Some(z) = b.zones.iter().find(|z| z.id == id) {
            let n = z.outline.len().saturating_sub(1);
            if n == 0 {
                return None;
            }
            let sx: f64 = z.outline[..n].iter().map(|p| p.x).sum();
            let sy: f64 = z.outline[..n].iter().map(|p| p.y).sum();
            return Some(Point::new(sx / n as f64, sy / n as f64));
        }
        if let Some(c) = b.cutouts.iter().find(|c| c.id == id) {
            return Some(c.center);
        }
        if let Some(f) = b.footprints.iter().find(|f| f.id == id) {
            return Some(f.position);
        }
        if let Some(t) = b.texts.iter().find(|t| t.id == id) {
            return Some(t.position);
        }
        if let Some(c) = self.components.iter().find(|c| c.id == id) {
            return Some(c.position);
        }
        None
    }
    /// Minimum clearance required between copper on `a` and copper on `b`:
    /// the board default, raised by the class of either net when classified.
    pub fn required_clearance(&self, a: Id, b: Id) -> f64 {
        let mut best = [a, b]
            .iter()
            .map(|net| {
                self.net_class(*net)
                    .map(|c| c.clearance)
                    .unwrap_or(self.board.clearance)
            })
            .max_by(f64::total_cmp)
            .unwrap_or(self.board.clearance);
        if let Some(rule) = self
            .board
            .pair_rules
            .iter()
            .find(|r| (r.a == a && r.b == b) || (r.a == b && r.b == a))
            && rule.clearance > best
        {
            best = rule.clearance;
        }
        best
    }
    /// Minimum copper separation for a pair of copper items whose midpoints
    /// sit at the given positions on the given layers. A rule area that
    /// covers both midpoints, and matches the pair's nets and layers when
    /// those filters are set, raises the requirement; it never lowers it.
    pub fn required_clearance_at(
        &self,
        a: Id,
        b: Id,
        at_a: Point,
        at_b: Point,
        layer_a: Layer,
        layer_b: Layer,
    ) -> f64 {
        let mut best = self.required_clearance(a, b);
        for area in &self.board.rule_areas {
            if area.clearance <= best {
                continue;
            }
            if area.net.is_some_and(|n| a != n && b != n) {
                continue;
            }
            if area.layer.is_some_and(|l| layer_a != l && layer_b != l) {
                continue;
            }
            let inside = |p: Point| {
                p.x >= area.region.x
                    && p.x <= area.region.x + area.region.width
                    && p.y >= area.region.y
                    && p.y <= area.region.y + area.region.height
            };
            if inside(at_a) && inside(at_b) {
                best = area.clearance;
            }
        }
        best
    }
    /// The drawn layer with this id, if any.
    pub fn user_layer(&self, id: Id) -> Option<&ArtLayer> {
        self.board.user_layers.iter().find(|l| l.id == id)
    }
    pub fn ids(&self) -> impl Iterator<Item = Id> + '_ {
        self.components
            .iter()
            .map(|x| x.id)
            .chain(self.sheets.iter().map(|x| x.id))
            .chain(self.ports.iter().map(|x| x.id))
            .chain(self.nets.iter().map(|x| x.id))
            .chain(self.net_classes.iter().map(|x| x.id))
            .chain(self.wires.iter().map(|x| x.id))
            .chain(self.drawn_wires.iter().map(|x| x.id))
            .chain(self.graphics.iter().map(|x| x.id))
            .chain(self.labels.iter().map(|x| x.id))
            .chain(self.no_connects.iter().map(|x| x.id))
            .chain(self.board.footprints.iter().map(|x| x.id))
            .chain(self.board.tracks.iter().map(|x| x.id))
            .chain(self.board.vias.iter().map(|x| x.id))
            .chain(self.board.zones.iter().map(|x| x.id))
            .chain(self.board.cutouts.iter().map(|x| x.id))
            .chain(self.board.holes.iter().map(|x| x.id))
            .chain(self.board.fiducials.iter().map(|x| x.id))
            .chain(self.board.netties.iter().map(|x| x.id))
            .chain(self.board.diff_pairs.iter().map(|x| x.id))
            .chain(self.board.rule_areas.iter().map(|x| x.id))
            .chain(self.board.user_layers.iter().map(|x| x.id))
            .chain(self.board.texts.iter().map(|x| x.id))
    }
    pub fn validate(&self) -> Result<(), String> {
        self.library.validate()?;
        if self
            .components
            .iter()
            .map(|c| 1 + c.units.len())
            .sum::<usize>()
            > MAX_ITEMS
        {
            return Err("schematic unit limit reached (2048)".into());
        }
        if self.padstacks.len() > 256 {
            return Err("the design defines at most 256 padstacks".into());
        }
        for stack in &self.padstacks {
            if stack.name.is_empty()
                || stack.name.len() > 64
                || !["circle", "rect", "oval", "roundrect"].contains(&stack.shape.as_str())
                || !positive(stack.size.x)
                || !positive(stack.size.y)
                || stack.size.x > 100.0
                || stack.size.y > 100.0
                || (stack.shape == "circle" && (stack.size.x - stack.size.y).abs() > 1e-6)
                || stack
                    .drill
                    .is_some_and(|d| !positive(d) || d >= stack.size.x.min(stack.size.y))
                || (stack.shape == "roundrect"
                    && !stack.roundrect_ratio.is_some_and(|r| r > 0.0 && r <= 0.5))
                || (stack.shape != "roundrect" && stack.roundrect_ratio.is_some_and(|r| r > 0.0))
                || self
                    .padstacks
                    .iter()
                    .filter(|s| s.name == stack.name)
                    .count()
                    > 1
            {
                return Err(
                    "padstacks need a unique name, a supported shape, a positive size and a drill smaller than the copper"
                        .into(),
                );
            }
        }
        if self.library_table.len() > 64 {
            return Err("the library table holds at most 64 entries".into());
        }
        for link in &self.library_table {
            if link.name.is_empty()
                || link.name.len() > 64
                || link.name.contains(['/', '\\'])
                || link.path.is_empty()
                || link.path.len() > 1024
            {
                return Err(
                    "library nicknames are 1 to 64 path-safe characters with a folder".into(),
                );
            }
            if self
                .library_table
                .iter()
                .filter(|l| l.name == link.name)
                .count()
                > 1
            {
                return Err("library table nicknames must be unique".into());
            }
        }
        if self.format != "pcbcraft" || self.version != 1 {
            return Err("unsupported project format/version".into());
        }
        if !Self::UNITS.contains(&self.unit.as_str()) {
            return Err("project unit must be mm, mil or inch".into());
        }
        if self.name.is_empty() || self.name.len() > 256 {
            return Err("project name must be 1–256 bytes".into());
        }
        if self.sheets.len() > 128 {
            return Err("a schematic supports at most 128 sheets".into());
        }
        if self.ports.len() > 256 {
            return Err("a schematic supports at most 256 ports".into());
        }
        let mut port_names = BTreeSet::new();
        for port in &self.ports {
            if port.name.is_empty() || port.name.len() > 63 {
                return Err("port name must be 1 to 63 bytes".into());
            }
            if !port_names.insert((port.sheet, &port.name)) {
                return Err("duplicate port name on one page".into());
            }
            if !port.position.valid() {
                return Err("port position is out of range".into());
            }
        }
        let mut sheet_names = BTreeSet::new();
        for s in &self.sheets {
            if s.name.is_empty() || s.name.len() > 128 {
                return Err("sheet name must be 1–128 bytes".into());
            }
            if !sheet_names.insert(&s.name) {
                return Err("duplicate sheet name".into());
            }
        }
        if self.ids().count() > MAX_ITEMS {
            return Err("project object limit reached (2048)".into());
        }
        let mut ids = BTreeSet::new();
        for id in self.ids() {
            if id == 0 || id > u32::MAX as u64 || !ids.insert(id) {
                return Err("duplicate or out-of-range object ID".into());
            }
        }
        if self.board.tracks.len()
            + self
                .board
                .footprints
                .iter()
                .map(|f| f.pads.len())
                .sum::<usize>()
            > 4096
        {
            return Err("copper item limit reached (4096)".into());
        }
        let nets: BTreeSet<_> = self.nets.iter().map(|n| n.id).collect();
        let mut names = BTreeSet::new();
        for n in &self.nets {
            if n.name.is_empty() || n.name.len() > 128 || !names.insert(&n.name) {
                return Err("invalid or duplicate net name".into());
            }
            if n.target_length
                .is_some_and(|t| !t.is_finite() || t <= 0.0 || t > 100_000.0)
                || n.length_tolerance
                    .is_some_and(|t| !t.is_finite() || t < 0.0 || t > 100_000.0)
                || (n.length_tolerance.is_some() && n.target_length.is_none())
            {
                return Err("invalid net length target or tolerance".into());
            }
        }
        if self.net_classes.len() > 64 {
            return Err("net class limit reached (64)".into());
        }
        let mut class_names = BTreeSet::new();
        let mut classified: BTreeSet<Id> = BTreeSet::new();
        for c in &self.net_classes {
            if c.name.is_empty()
                || c.name.len() > 128
                || !class_names.insert(&c.name)
                || c.nets.len() > 256
                || !c.clearance.is_finite()
                || c.clearance <= 0.0
                || c.clearance > 100.0
                || !c.width.is_finite()
                || c.width <= 0.0
                || c.width > 100.0
                || c.min_width
                    .is_some_and(|w| !w.is_finite() || w <= 0.0 || w > 100.0)
                || c.max_width
                    .is_some_and(|w| !w.is_finite() || w <= 0.0 || w > 100.0)
                || c.min_width
                    .zip(c.max_width)
                    .is_some_and(|(min, max)| min > max)
                || c.target_length
                    .is_some_and(|t| !t.is_finite() || t <= 0.0 || t > 10_000.0)
                || c.length_tolerance
                    .is_some_and(|t| !t.is_finite() || t < 0.0 || t > 1000.0)
                || (c.length_tolerance.is_some() && c.target_length.is_none())
            {
                return Err("invalid net class".into());
            }
            for net in &c.nets {
                if !nets.contains(net) || !classified.insert(*net) {
                    return Err("net class membership is invalid".into());
                }
            }
        }
        if self.board.pair_rules.len() > 64 {
            return Err("a board carries at most 64 net pair clearance rules".into());
        }
        if self.board.rule_areas.len() > 64 {
            return Err("a board carries at most 64 rule areas".into());
        }
        if self.board.holes.len() > 512 {
            return Err("a board carries at most 512 unplated holes".into());
        }
        for h in &self.board.holes {
            if !h.valid() {
                return Err("unplated holes need a sane position and a 0.05 to 25 mm drill".into());
            }
        }
        if self.board.fiducials.len() > 128 {
            return Err("a board carries at most 128 fiducial marks".into());
        }
        for f in &self.board.fiducials {
            if !f.valid() {
                return Err("fiducial marks need a sane position and a 0.2 to 10 mm disc".into());
            }
        }
        if self.board.netties.len() > 128 {
            return Err("a board carries at most 128 net ties".into());
        }
        for t in &self.board.netties {
            if !t.valid() || !nets.contains(&t.net_a) || !nets.contains(&t.net_b) {
                return Err("net ties bridge two known nets with sane 0.3 to 20 mm pads".into());
            }
        }
        if self.board.diff_pairs.len() > 64 {
            return Err("a board carries at most 64 differential pairs".into());
        }
        let mut pair_names: BTreeSet<&str> = BTreeSet::new();
        for d in &self.board.diff_pairs {
            if !d.valid()
                || !nets.contains(&d.net_p)
                || !nets.contains(&d.net_n)
                || !pair_names.insert(d.name.as_str())
            {
                return Err(
                    "differential pairs name two known nets with a unique name and a 0.02 to 10 mm gap"
                        .into(),
                );
            }
        }
        for r in &self.board.rule_areas {
            if !r.valid() || r.net.is_some_and(|n| !nets.contains(&n)) {
                return Err("rule areas need a valid rectangle, a sane gap and a known net".into());
            }
        }
        if self.board.user_layers.len() > 32 {
            return Err("a board carries at most 32 drawn layers".into());
        }
        let mut art_names: BTreeSet<&str> = BTreeSet::new();
        for l in &self.board.user_layers {
            if !l.valid() || !art_names.insert(l.name.as_str()) {
                return Err("drawn layers need a unique name and sane figures".into());
            }
        }
        let mut paired: BTreeSet<(Id, Id)> = BTreeSet::new();
        for r in &self.board.pair_rules {
            let key = (r.a.min(r.b), r.a.max(r.b));
            if !r.valid() || !nets.contains(&r.a) || !nets.contains(&r.b) || !paired.insert(key) {
                return Err(
                    "net pair clearance rules need known distinct nets, one rule per pair".into(),
                );
            }
        }
        if self.output_jobs.len() > 16 {
            return Err("a project carries at most 16 output jobs".into());
        }
        let mut job_names = BTreeSet::new();
        for j in &self.output_jobs {
            if !j.valid() || !job_names.insert(&j.name) {
                return Err("output jobs need a unique name and known export kinds".into());
            }
        }
        if self.bom_fields.len() > 16 {
            return Err("a bill of materials lists at most 16 properties".into());
        }
        let mut bom_keys = BTreeSet::new();
        for f in &self.bom_fields {
            if !f.valid() || !bom_keys.insert(&f.key) {
                return Err("bill-of-materials fields need unique short keys".into());
            }
        }
        if self.files.len() > 32 {
            return Err("a project attaches at most 32 files".into());
        }
        let mut file_names = BTreeSet::new();
        for f in &self.files {
            if !f.valid() || !file_names.insert(&f.name) {
                return Err("attached files need unique short names and 256 KiB or less".into());
            }
        }
        if self.graphic_groups.len() > 256 {
            return Err("a project defines at most 256 graphic groups".into());
        }
        let graphic_ids: BTreeSet<Id> = self.graphics.iter().map(|g| g.id).collect();
        let mut grouped = BTreeSet::new();
        for g in &self.graphic_groups {
            let known = g
                .graphics
                .iter()
                .all(|id| graphic_ids.contains(id) && grouped.insert(*id));
            if !g.valid() || !known {
                return Err(
                    "graphic groups need two to 512 distinct existing graphics, each in at \
                     most one group"
                        .into(),
                );
            }
        }
        if self.meta.len() > 16 {
            return Err("a project records at most 16 metadata entries".into());
        }
        for (key, value) in &self.meta {
            if key.is_empty()
                || key.len() > 32
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                || value.len() > 4096
            {
                return Err(
                    "metadata keys are 1 to 32 lowercase alphanumeric bytes and values fit 4 KiB"
                        .into(),
                );
            }
        }
        if self.variants.len() > 16 {
            return Err("a project defines at most 16 variants".into());
        }
        let mut variant_names = BTreeSet::new();
        for v in &self.variants {
            if !v.valid() || !variant_names.insert(&v.name) {
                return Err(
                    "variants need unique short names, 2048 flags or less and 512 footprint \
                     swaps or less"
                        .into(),
                );
            }
        }
        if self.rule_severity.len() > 64 {
            return Err("severity overrides cover at most 64 check codes".into());
        }
        for (code, severity) in &self.rule_severity {
            if code.is_empty()
                || code.len() > 32
                || !matches!(severity.as_str(), "error" | "warning" | "off")
            {
                return Err("invalid severity override".into());
            }
        }
        if self.rule_exclusions.len() > 64 {
            return Err("exclusions cover at most 64 check codes".into());
        }
        for (code, ids) in &self.rule_exclusions {
            if code.is_empty() || code.len() > 32 {
                return Err("invalid exclusion check code".into());
            }
            if ids.len() > 256 {
                return Err("a check code excludes at most 256 objects".into());
            }
        }
        if self.companion_rules.len() > 64 {
            return Err("a project carries at most 64 companion rules".into());
        }
        let mut companion_ids = BTreeSet::new();
        for rule in &self.companion_rules {
            if !rule.valid() || !companion_ids.insert(&rule.id) {
                return Err("companion rules need unique ids and bounded fields".into());
            }
            if let (Some(min), Some(max)) = (&rule.value_min, &rule.value_max) {
                if parse_companion_value(min).is_none() || parse_companion_value(max).is_none() {
                    return Err("companion rule value range must parse".into());
                }
            }
        }
        if self.rule_regions.len() > 64 {
            return Err("region exclusions cover at most 64 check codes".into());
        }
        for (code, regions) in &self.rule_regions {
            if code.is_empty() || code.len() > 32 {
                return Err("invalid region exclusion check code".into());
            }
            if regions.len() > 64 {
                return Err("a check code carries at most 64 regions".into());
            }
            for r in regions {
                if !r.x.is_finite()
                    || !r.y.is_finite()
                    || !r.width.is_finite()
                    || !r.height.is_finite()
                    || r.width <= 0.0
                    || r.height <= 0.0
                    || r.width > 1000.0
                    || r.height > 1000.0
                    || r.x < -1000.0
                    || r.y < -1000.0
                    || r.x + r.width > 1000.0
                    || r.y + r.height > 1000.0
                {
                    return Err("region exclusion rectangle is out of range".into());
                }
            }
        }
        let mut refs = BTreeSet::new();
        for c in &self.components {
            if !c.position.valid()
                || !c.rotation.is_finite()
                || c.rotation.abs() > 360_000.0
                || !refs.insert(&c.reference)
                || c.reference.is_empty()
                || c.reference.len() > 64
                || c.value.len() > 256
                || (c.library_symbol.is_none() && !schematic_kind_valid(&c.kind))
            {
                return Err("invalid component".into());
            }
            if c.fields.len() > 32 {
                return Err("a component carries at most 32 fields".into());
            }
            let mut field_keys: BTreeSet<&str> = BTreeSet::new();
            for f in &c.fields {
                if f.key.is_empty()
                    || f.key.len() > 64
                    || f.value.len() > 256
                    || !field_keys.insert(f.key.as_str())
                {
                    return Err("invalid component field".into());
                }
            }
            if let Some(symbol) = &c.library_symbol {
                symbol.definition.validate().map_err(|e| e.to_string())?;
                if symbol.unit != 1
                    || c.kind != "library"
                    || symbol.library.len() > 256
                    || !symbol.definition.unsupported.is_empty()
                {
                    return Err("invalid or unsupported embedded library symbol".into());
                }
                let pins: Vec<_> = symbol
                    .definition
                    .pins
                    .iter()
                    .filter(|p| p.body_style <= 1)
                    .collect();
                if pins.len() != c.pins.len()
                    || pins.iter().any(|p| {
                        !c.pins.iter().any(|pin| {
                            pin.number == p.number
                                && pin.name == p.name
                                && pin.offset == p.position
                                && pin.direction == Some(p.angle)
                                && pin.electrical_type == p.electrical_type
                                && pin.unit == p.unit.max(1)
                        })
                    })
                {
                    return Err("component pins do not match embedded symbol definition".into());
                }
                let expected = symbol_units(&symbol.definition)?;
                let actual: BTreeSet<_> = std::iter::once(1)
                    .chain(c.units.iter().map(|u| u.unit))
                    .collect();
                if expected != actual
                    || c.units.len() + 1 != actual.len()
                    || c.units.iter().any(|u| {
                        u.unit <= 1
                            || !u.position.valid()
                            || !u.rotation.is_finite()
                            || u.rotation.abs() > 360_000.0
                    })
                {
                    return Err("invalid schematic unit placements".into());
                }
            } else if !c.units.is_empty() || c.pins.iter().any(|p| p.unit != 1) {
                return Err("starter symbols have one unit".into());
            }
            if let Some(footprint) = &c.library_footprint {
                footprint.definition.validate().map_err(|e| e.to_string())?;
                validate_footprint_support(&footprint.definition)?;
                if footprint.library.len() > 256 {
                    return Err("invalid footprint library name".into());
                }
                let pins: BTreeSet<_> = c.pins.iter().map(|p| p.number.as_str()).collect();
                let pads: BTreeSet<_> = footprint
                    .definition
                    .pads
                    .iter()
                    .map(|p| p.number.as_str())
                    .collect();
                if pins != pads {
                    return Err("embedded footprint pad numbers do not match symbol pins".into());
                }
            }
            if c.pins.is_empty() || c.pins.len() > 64 {
                return Err("invalid pin count".into());
            }
            let mut numbers = BTreeSet::new();
            for p in &c.pins {
                if p.number.is_empty()
                    || p.number.len() > 16
                    || p.name.len() > 64
                    || !numbers.insert(&p.number)
                    || !p.offset.valid()
                    || p.direction
                        .is_some_and(|angle| !angle.is_finite() || angle.abs() > 360_000.0)
                    || p.electrical_type.len() > 64
                    || p.net.is_some_and(|n| !nets.contains(&n))
                {
                    return Err("invalid pin or net reference".into());
                }
            }
        }
        for w in &self.wires {
            if !nets.contains(&w.net)
                || self.pin(&w.from).is_none_or(|p| p.net != Some(w.net))
                || self.pin(&w.to).is_none_or(|p| p.net != Some(w.net))
            {
                return Err("wire has inconsistent pin/net references".into());
            }
        }
        for w in &self.drawn_wires {
            if w.net.is_some_and(|net| !nets.contains(&net)) {
                return Err("drawn wire references an unknown net".into());
            }
            if w.points.len() < 2 || w.points.len() > 256 {
                return Err("drawn wire needs 2–256 vertices".into());
            }
            if !w.points.iter().all(|p| p.valid()) {
                return Err("drawn wire has an out-of-range vertex".into());
            }
            if w.points.windows(2).any(|p| p[0] == p[1]) {
                return Err("drawn wire repeats a vertex".into());
            }
        }
        for g in &self.graphics {
            if g.layer != "schematic" && g.layer != "board" {
                return Err("graphic layer must be schematic or board".into());
            }
            let (min, max) = match Graphic::vertex_rule(&g.shape) {
                Some(rule) => rule,
                None => return Err("unknown graphic shape".into()),
            };
            if g.points.len() < min || g.points.len() > max {
                return Err("graphic has the wrong number of vertices for its shape".into());
            }
            if !g.points.iter().all(|p| p.valid()) {
                return Err("graphic has an out-of-range vertex".into());
            }
            if !positive(g.width) {
                return Err("graphic width, radius or text size must be positive".into());
            }
            if g.text.len() > 1024 {
                return Err("graphic text is too long".into());
            }
            if (g.shape == "text") != !g.text.is_empty() {
                return Err("only text graphics may carry text".into());
            }
            if g.shape == "line" && g.points[0] == g.points[1] {
                return Err("line graphic has no length".into());
            }
            if g.shape == "rect"
                && (g.points[0].x == g.points[1].x || g.points[0].y == g.points[1].y)
            {
                return Err("rect graphic has no area".into());
            }
            if g.shape == "arc" && circle_through(g.points[0], g.points[1], g.points[2]).is_none() {
                return Err("arc graphic vertices are collinear".into());
            }
            if g.shape == "circle"
                && g.points[0] == Point::new(g.points[0].x + g.width, g.points[0].y)
            {
                return Err("circle graphic has zero radius".into());
            }
        }
        for l in &self.labels {
            if !Label::KINDS.iter().any(|k| *k == l.kind) {
                return Err("unknown label kind".into());
            }
            if l.kind == "bus" {
                if l.net != 0 || l.text.is_empty() || l.text.len() > 128 {
                    return Err("bus label carries its notation text and no single net".into());
                }
            } else if !self.nets.iter().any(|n| n.id == l.net) {
                return Err("label references an unknown net".into());
            }
            if !l.position.valid() {
                return Err("label position is out of range".into());
            }
            if !l.rotation.is_finite() || l.rotation.abs() > 360_000.0 {
                return Err("invalid label rotation".into());
            }
            if l.kind == "junction"
                && !self
                    .drawn_wires
                    .iter()
                    .any(|w| w.net == Some(l.net) && w.points.contains(&l.position))
            {
                return Err("junction must sit on a vertex of a drawn wire of its net".into());
            }
        }
        for f in &self.no_connects {
            let Some(c) = self.component(f.pin.component) else {
                return Err("no-connect flag references an unknown component".into());
            };
            if !c.pins.iter().any(|p| p.number == f.pin.pin) {
                return Err("no-connect flag references an unknown pin".into());
            }
            if self.wires.iter().any(|w| {
                (w.from.component == f.pin.component && w.from.pin == f.pin.pin)
                    || (w.to.component == f.pin.component && w.to.pin == f.pin.pin)
            }) {
                return Err("no-connect flag conflicts with a connected pin".into());
            }
        }
        let b = &self.board;
        if !positive(b.width)
            || !positive(b.height)
            || !positive(b.thickness)
            || !positive(b.clearance)
            || b.finish.is_empty()
            || b.finish.len() > 64
            || !(1.0..=20.0).contains(&b.er)
            || !b.drill_origin.valid()
            || b.drill_origin.x.abs() > 1000.0
            || b.drill_origin.y.abs() > 1000.0
            || b.layer_records.len() > 32
            || b.layer_records.iter().any(|l| !l.valid())
            || b.machining.as_ref().is_some_and(|m| !m.valid())
        {
            return Err("invalid board dimensions/rules".into());
        }
        if let Some(points) = &b.outline {
            let distinct: BTreeSet<(u64, u64)> = points
                .iter()
                .map(|p| (p.x.to_bits(), p.y.to_bits()))
                .collect();
            let inside = points.iter().all(|p| {
                p.valid() && p.x >= 0.0 && p.x <= b.width && p.y >= 0.0 && p.y <= b.height
            });
            if points.len() < 3
                || points.len() > 64
                || distinct.len() != points.len()
                || !inside
                || outline_area(points) < 0.01
            {
                return Err(
                    "a board outline needs 3 to 64 distinct vertices inside the board".into(),
                );
            }
        }
        let mut placed = BTreeSet::new();
        for f in &b.footprints {
            if self.component(f.component).is_none()
                || !placed.insert(f.component)
                || !f.position.valid()
                || !f.rotation.is_finite()
                || f.rotation.abs() > 360_000.0
                || f.pads.is_empty()
                || f.pads.len() > 64
            {
                return Err("invalid footprint".into());
            }
            let mut numbers = BTreeSet::new();
            if let Some(library) = self
                .component(f.component)
                .and_then(|c| c.library_footprint.as_ref())
            {
                if f.pads.len() != library.definition.pads.len()
                    || f.pads.iter().any(|p| {
                        p.geometry.is_none()
                            || (p.diameter
                                - p.geometry
                                    .as_ref()
                                    .map(|g| g.size.x.min(g.size.y))
                                    .unwrap_or(0.0))
                            .abs()
                                > 1e-6
                    })
                {
                    return Err("placed footprint pads must retain embedded geometry".into());
                }
            } else if self
                .component(f.component)
                .is_some_and(|c| c.library_symbol.is_some())
            {
                return Err(
                    "library symbol requires an assigned footprint before PCB placement".into(),
                );
            }
            for p in &f.pads {
                if !numbers.insert(&p.number)
                    || !p.offset.valid()
                    || !positive(p.diameter)
                    || p.net.is_some_and(|n| !nets.contains(&n))
                    || self.component(f.component).is_none_or(|c| {
                        !c.pins
                            .iter()
                            .any(|pin| pin.number == p.number && pin.net == p.net)
                    })
                {
                    return Err("invalid pad or stale pad net; sync the PCB".into());
                }
                if let Some(geometry) = &p.geometry {
                    if geometry.position != p.offset
                        || geometry.number != p.number
                        || !positive(geometry.size.x)
                        || !positive(geometry.size.y)
                    {
                        return Err("pad geometry does not match imported footprint".into());
                    }
                    let definition = self
                        .component(f.component)
                        .and_then(|c| c.library_footprint.as_ref())
                        .ok_or("pad geometry requires an embedded footprint definition")?;
                    if !definition
                        .definition
                        .pads
                        .iter()
                        .any(|source| source == geometry)
                    {
                        return Err("pad geometry is not present in embedded definition".into());
                    }
                }
            }
        }
        for t in &b.tracks {
            if !nets.contains(&t.net)
                || !t.start.valid()
                || !t.end.valid()
                || !positive(t.width)
                || t.start.distance(t.end) < 1e-6
                || t.arc.is_some_and(|m| !m.valid())
                || t.arc
                    .is_some_and(|m| pcbcraft_geom::arc_through(t.start, m, t.end).is_none())
            {
                return Err("invalid track geometry or net".into());
            }
        }
        for v in &b.vias {
            if !nets.contains(&v.net)
                || !v.position.valid()
                || !positive(v.diameter)
                || v.diameter > 100.0
                || v.drill < 0.0
                || v.drill >= v.diameter
            {
                return Err("invalid via geometry or net".into());
            }
        }
        for z in &b.zones {
            if !nets.contains(&z.net) {
                return Err("zone references an unknown net".into());
            }
            validate_closed_outline(&z.outline)?;
            for cut in &z.cutouts {
                validate_closed_outline(cut)?;
            }
            if let Some(kind) = &z.kind
                && (kind.is_empty() || kind.len() > 32)
            {
                return Err("zone kind must be 1 to 32 characters".into());
            }
        }
        for c in &b.cutouts {
            if !c.center.valid()
                || !positive(c.radius)
                || c.center.x - c.radius <= 0.0
                || c.center.y - c.radius <= 0.0
                || c.center.x + c.radius >= b.width
                || c.center.y + c.radius >= b.height
            {
                return Err("cutout must be strictly inside the board".into());
            }
        }
        if b.texts.len() > 256 {
            return Err("a board carries at most 256 text labels".into());
        }
        for t in &b.texts {
            if t.text.is_empty()
                || t.text.len() > 128
                || !t.position.valid()
                || !t.size.is_finite()
                || t.size < 0.1
                || t.size > 50.0
                || !t.rotation.is_finite()
                || t.rotation.abs() > 360_000.0
            {
                return Err("board text must be 1 to 128 bytes with a finite position and a size between 0.1 and 50 mm".into());
            }
            if let Some(bound) = t.component
                && self.component(bound).is_none()
            {
                return Err("board text is bound to an unknown component".into());
            }
        }
        Ok(())
    }
}

pub fn symbol_units(def: &pcbcraft_library::SymbolDefinition) -> Result<BTreeSet<u32>, String> {
    let units: BTreeSet<_> = def
        .pins
        .iter()
        .filter(|p| p.body_style <= 1)
        .map(|p| p.unit.max(1))
        .chain(
            def.graphics
                .iter()
                .filter(|g| g.body_style <= 1 && g.unit > 0)
                .map(|g| g.unit),
        )
        .collect();
    if !units.contains(&1) || units.len() > 32 || units.iter().any(|u| *u > 32) {
        return Err("placement supports 1–32 schematic units, including unit 1".into());
    }
    Ok(units)
}
pub fn library_component(
    definition: pcbcraft_library::SymbolDefinition,
    library: String,
    id: Id,
    reference: String,
    position: Point,
    value: Option<String>,
) -> Result<Component, String> {
    definition.validate().map_err(|e| e.to_string())?;
    if !definition.unsupported.is_empty() {
        return Err(format!(
            "unsupported symbol features: {}",
            definition.unsupported.join(", ")
        ));
    }
    let unit_ids = symbol_units(&definition)?;
    let mut numbers = BTreeSet::new();
    let pins: Vec<_> = definition
        .pins
        .iter()
        .filter(|p| p.body_style <= 1)
        .map(|p| Pin {
            number: p.number.clone(),
            name: p.name.clone(),
            offset: p.position,
            net: None,
            electrical_type: p.electrical_type.clone(),
            direction: Some(p.angle),
            unit: p.unit.max(1),
        })
        .collect();
    if pins.is_empty() || pins.len() > 64 || pins.iter().any(|p| !numbers.insert(&p.number)) {
        return Err("symbol placement requires 1–64 unique physical pin numbers across units; repeated common pins are not supported yet".into());
    }
    let extent = pins
        .iter()
        .map(|p| p.offset)
        .chain(
            definition
                .graphics
                .iter()
                .flat_map(|g| g.primitive.path_points()),
        )
        .fold(Point::new(7.0, 5.0), |m, p| {
            Point::new(m.x.max(p.x.abs()), m.y.max(p.y.abs()))
        });
    let units = unit_ids
        .into_iter()
        .filter(|u| *u > 1)
        .enumerate()
        .map(|(i, unit)| {
            let index = i + 1;
            SymbolUnitPlacement {
                unit,
                position: position.offset(Point::new(
                    (index % 3) as f64 * (extent.x * 2.0 + 10.0),
                    (index / 3) as f64 * (extent.y * 2.0 + 12.0),
                )),
                rotation: 0.0,
            }
        })
        .collect();
    Ok(Component {
        id,
        reference,
        value: value.unwrap_or_else(|| definition.value.clone()),
        kind: "library".into(),
        position,
        rotation: 0.0,
        pins,
        units,
        fields: vec![],
        sheet: 0,
        library_symbol: Some(LibrarySymbol {
            library,
            unit: 1,
            definition,
        }),
        library_footprint: None,
    })
}
pub fn validate_footprint_support(
    definition: &pcbcraft_library::FootprintDefinition,
) -> Result<(), String> {
    definition.validate().map_err(|e| e.to_string())?;
    if definition.graphics.iter().any(|g| match g {
        pcbcraft_library::Graphic::Line { layer, .. }
        | pcbcraft_library::Graphic::Rectangle { layer, .. }
        | pcbcraft_library::Graphic::Circle { layer, .. }
        | pcbcraft_library::Graphic::Arc { layer, .. }
        | pcbcraft_library::Graphic::Polyline { layer, .. }
        | pcbcraft_library::Graphic::Text { layer, .. } => layer == "Edge.Cuts",
    }) {
        return Err("footprint-contained Edge.Cuts require mechanical cut geometry support, which is planned".into());
    }
    if !definition.unsupported.is_empty() {
        return Err(format!(
            "footprint has unsupported primitives: {}",
            definition.unsupported.join(", ")
        ));
    }
    if definition.graphics.iter().any(|g| match g {
        pcbcraft_library::Graphic::Line { layer, .. }
        | pcbcraft_library::Graphic::Rectangle { layer, .. }
        | pcbcraft_library::Graphic::Circle { layer, .. }
        | pcbcraft_library::Graphic::Arc { layer, .. }
        | pcbcraft_library::Graphic::Polyline { layer, .. }
        | pcbcraft_library::Graphic::Text { layer, .. } => layer.ends_with(".Cu"),
    }) {
        return Err("footprint copper graphics require conductive geometry support; use numbered pads instead".into());
    }
    let mut numbers = BTreeSet::new();
    for p in &definition.pads {
        let copper_ok = if p.pad_type == "thru_hole" {
            p.layers.iter().any(|l| l == "*.Cu")
                || (p.layers.iter().any(|l| l == "F.Cu") && p.layers.iter().any(|l| l == "B.Cu"))
        } else {
            p.layers.iter().any(|l| l == "F.Cu")
                && !p.layers.iter().any(|l| l == "B.Cu" || l == "*.Cu")
        };
        let drill_ok = if p.pad_type == "thru_hole" {
            drill_inside_pad(p)
        } else {
            p.drill.is_none()
        };
        if !["smd", "thru_hole"].contains(&p.pad_type.as_str())
            || matches!(p.shape_kind(), PadShape::Unsupported)
            || (p.shape == "circle" && (p.size.x - p.size.y).abs() > 1e-6)
            || !copper_ok
            || !drill_ok
            || p.number.is_empty()
            || !numbers.insert(&p.number)
        {
            return Err("placement supports unique numbered front SMD or plated through-hole circle/rect/roundrect/oval/chamfered/custom-polygon pads with a drill strictly inside the pad; unnumbered mechanical holes and back SMD placement are planned".into());
        }
    }
    if definition.pads.is_empty() || definition.pads.len() > 64 {
        return Err("imported footprint must have 1–64 pads".into());
    }
    Ok(())
}

fn drill_inside_pad(p: &pcbcraft_library::FootprintPad) -> bool {
    let Some(drill) = &p.drill else { return false };
    // Standard circular/oval holes are capsules. A convex pad contains the
    // whole capsule iff it contains both endpoint discs of that capsule.
    let radius = drill.size.x.min(drill.size.y) / 2.0;
    let half_axis = Point::new(
        (drill.size.x / 2.0 - radius).max(0.0),
        (drill.size.y / 2.0 - radius).max(0.0),
    );
    let ends = [
        drill.offset.offset(half_axis),
        drill.offset.offset(Point::new(-half_axis.x, -half_axis.y)),
    ];
    let corner = match p.shape_kind() {
        PadShape::Circle | PadShape::Oval => p.size.x.min(p.size.y) / 2.0,
        PadShape::RoundRect { ratio } => p.size.x.min(p.size.y) * ratio,
        PadShape::Rect => 0.0,
        PadShape::Chamfered(chamfer) => {
            return capsule_inside_polygon(ends, radius, &chamfered_rect(p.size, chamfer));
        }
        PadShape::Custom { polygon } => return capsule_inside_polygon(ends, radius, polygon),
        PadShape::Unsupported => return false,
    };
    let clearance = |point: Point| {
        let dx = point.x.abs() - (p.size.x / 2.0 - corner);
        let dy = point.y.abs() - (p.size.y / 2.0 - corner);
        -(dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - corner)
    };
    radius > 0.0 && ends.into_iter().all(|end| clearance(end) > radius + 1e-6)
}
/// Exact for any simple polygon: the capsule axis starts inside and stays more
/// than `radius` from every edge.
fn capsule_inside_polygon(ends: [Point; 2], radius: f64, polygon: &[Point]) -> bool {
    let [a, b] = ends;
    radius > 0.0
        && pcbcraft_geom::point_in_polygon(a, polygon)
        && pcbcraft_geom::polygon_edges(polygon)
            .all(|(c, d)| pcbcraft_geom::segment_distance(a, b, c, d) > radius + 1e-6)
}

fn orthogonal_path(a: Point, b: Point, da: Point, db: Point) -> Vec<Point> {
    let vertical_a = da.y.abs() > da.x.abs();
    let vertical_b = db.y.abs() > db.x.abs();
    if vertical_a != vertical_b {
        let outward = |direction: Point, vertical: bool| {
            if vertical {
                Point::new(0.0, if direction.y >= 0.0 { 3.0 } else { -3.0 })
            } else {
                Point::new(if direction.x >= 0.0 { 3.0 } else { -3.0 }, 0.0)
            }
        };
        let start = a.offset(outward(da, vertical_a));
        let end = b.offset(outward(db, vertical_b));
        let bend = if vertical_a {
            Point::new(end.x, start.y)
        } else {
            Point::new(start.x, end.y)
        };
        let mut points = vec![a, start, bend, end, b];
        points.dedup_by(|a, b| a.distance(*b) < 1e-9);
        return points;
    }
    if da.y.abs() > da.x.abs() && db.y.abs() > db.x.abs() {
        return orthogonal_path(
            Point::new(a.y, a.x),
            Point::new(b.y, b.x),
            Point::new(da.y, da.x),
            Point::new(db.y, db.x),
        )
        .into_iter()
        .map(|p| Point::new(p.y, p.x))
        .collect();
    }
    let sa = if da.x >= 0.0 { 1.0 } else { -1.0 };
    let sb = if db.x >= 0.0 { 1.0 } else { -1.0 };
    if sa == sb && (a.y - b.y).abs() > 1e-6 {
        let x = if sa < 0.0 {
            a.x.min(b.x) - 3.0
        } else {
            a.x.max(b.x) + 3.0
        };
        return vec![a, Point::new(x, a.y), Point::new(x, b.y), b];
    }
    if sa != sb && (b.x - a.x) * sa > 0.0 {
        let x = (a.x + b.x) / 2.0;
        return vec![a, Point::new(x, a.y), Point::new(x, b.y), b];
    }
    let start = Point::new(a.x + sa * 3.0, a.y);
    let end = Point::new(b.x + sb * 3.0, b.y);
    let y = a.y.max(b.y) + 8.0;
    vec![
        a,
        start,
        Point::new(start.x, y),
        Point::new(end.x, y),
        end,
        b,
    ]
}

#[cfg(test)]
mod drill_tests {
    use super::*;
    #[test]
    fn native_library_unknown_fields_are_rejected_instead_of_discarded() -> Result<(), String> {
        assert!(serde_json::from_value::<ProjectLibrary>(serde_json::json!({"symbols":[],"footprints":[],"license_metadata":"must not disappear"})).is_err());
        let definition = dip()?;
        let mut value = serde_json::to_value(&definition).map_err(|e| e.to_string())?;
        value["license_metadata"] = serde_json::json!("must not disappear");
        assert!(serde_json::from_value::<pcbcraft_library::FootprintDefinition>(value).is_err());
        for field in ["pad", "drill"] {
            let mut value = serde_json::to_value(&definition).map_err(|e| e.to_string())?;
            if field == "pad" {
                value["pads"][0]["unsupported_future_field"] = serde_json::json!(true);
            } else {
                value["pads"][0]["drill"]["unsupported_future_field"] = serde_json::json!(true);
            }
            assert!(
                serde_json::from_value::<pcbcraft_library::FootprintDefinition>(value).is_err()
            );
        }
        let symbol = pcbcraft_library::read_symbols(include_str!(
            "../../library/tests/fixtures/device.kicad_sym"
        ))
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.name == "R")
        .ok_or("missing resistor")?;
        for path in ["root", "pin", "graphic", "graphic-primitive", "point"] {
            let mut value = serde_json::to_value(&symbol).map_err(|e| e.to_string())?;
            match path {
                "root" => value["unsupported_future_field"] = serde_json::json!(true),
                "pin" => value["pins"][0]["unsupported_future_field"] = serde_json::json!(true),
                "point" => {
                    value["pins"][0]["position"]["unsupported_future_field"] =
                        serde_json::json!(true)
                }
                "graphic-primitive" => {
                    value["graphics"][0]["primitive"]["Rectangle"]["unsupported_future_field"] =
                        serde_json::json!(true)
                }
                _ => value["graphics"][0]["unsupported_future_field"] = serde_json::json!(true),
            };
            assert!(serde_json::from_value::<pcbcraft_library::SymbolDefinition>(value).is_err());
        }
        Ok(())
    }
    #[test]
    fn copper_graphics_cannot_bypass_pad_connectivity_checks() -> Result<(), String> {
        let mut definition = dip()?;
        validate_footprint_support(&definition)?;
        definition.graphics.push(pcbcraft_library::Graphic::Line {
            start: Point::default(),
            end: Point::new(7.62, 0.0),
            width: 0.3,
            layer: "F.Cu".into(),
        });
        assert!(
            validate_footprint_support(&definition).is_err_and(|e| e.contains("copper graphics"))
        );
        Ok(())
    }
    fn assert_outward(path: &[Point], a: Point, b: Point, da: Point, db: Point) {
        assert!(path.len() >= 3);
        assert_eq!(path.first(), Some(&a));
        assert_eq!(path.last(), Some(&b));
        let first = Point::new(path[1].x - a.x, path[1].y - a.y);
        let last = Point::new(path[path.len() - 2].x - b.x, path[path.len() - 2].y - b.y);
        assert!(
            first.x * da.x + first.y * da.y > 0.0,
            "first leg must leave its pin outward"
        );
        assert!(
            last.x * db.x + last.y * db.y > 0.0,
            "last leg must approach from the outward side"
        );
        assert!(path.windows(2).all(
            |pair| (pair[0].x - pair[1].x).abs() < 1e-8 || (pair[0].y - pair[1].y).abs() < 1e-8
        ));
    }
    #[test]
    fn mixed_pin_orientations_have_outward_stubs_at_both_ends() {
        let a = Point::default();
        let b = Point::new(10.0, 10.0);
        for vertical in [Point::new(0.0, -1.0), Point::new(0.0, 1.0)] {
            for horizontal in [Point::new(-1.0, 0.0), Point::new(1.0, 0.0)] {
                for (da, db) in [(vertical, horizontal), (horizontal, vertical)] {
                    assert_outward(&orthogonal_path(a, b, da, db), a, b, da, db);
                }
            }
        }
    }
    #[test]
    fn wire_path_uses_rotated_secondary_unit_pin_endpoints_and_directions() -> Result<(), String> {
        let definition = pcbcraft_library::read_symbols(include_str!(
            "../../library/tests/fixtures/lm358.kicad_sym"
        ))
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.name == "LM358")
        .ok_or("missing LM358")?;
        let mut c = library_component(
            definition,
            "Amplifier_Operational".into(),
            1,
            "U1".into(),
            Point::new(20.0, 20.0),
            None,
        )?;
        c.units
            .iter_mut()
            .find(|u| u.unit == 2)
            .ok_or("missing unit2")?
            .rotation = 90.0;
        for pin in &mut c.pins {
            if ["5", "1"].contains(&pin.number.as_str()) {
                pin.net = Some(2);
            }
        }
        let mut project = Project::default();
        project.components.push(c);
        project.nets.push(Net {
            id: 2,
            name: "signal".into(),
            ..Default::default()
        });
        let wire = Wire {
            id: 3,
            net: 2,
            from: PinRef {
                component: 1,
                pin: "5".into(),
            },
            to: PinRef {
                component: 1,
                pin: "1".into(),
            },
        };
        project.wires.push(wire.clone());
        project.validate()?;
        let a = project
            .pin_position(&wire.from)
            .ok_or("missing first point")?;
        let b = project
            .pin_position(&wire.to)
            .ok_or("missing second point")?;
        assert_outward(
            &project.wire_path(&wire).ok_or("missing path")?,
            a,
            b,
            Point::new(0.0, 1.0),
            Point::new(1.0, 0.0),
        );
        Ok(())
    }
    fn dip() -> Result<pcbcraft_library::FootprintDefinition, String> {
        pcbcraft_library::read_footprint(include_str!(
            "../../library/tests/fixtures/DIP-8_W7.62mm.kicad_mod"
        ))
        .map_err(|e| e.to_string())
    }
    #[test]
    fn real_dip_and_valid_offset_slots_fit_actual_pad_shapes() -> Result<(), String> {
        let mut definition = dip()?;
        validate_footprint_support(&definition)?;
        let p = &mut definition.pads[0];
        p.shape = "oval".into();
        p.size = Point::new(4.0, 2.0);
        p.roundrect_ratio = None;
        p.drill = Some(pcbcraft_library::Drill {
            size: Point::new(1.6, 0.6),
            offset: Point::new(0.3, 0.2),
        });
        validate_footprint_support(&definition)?;
        let p = &mut definition.pads[0];
        p.shape = "circle".into();
        p.size = Point::new(2.0, 2.0);
        p.drill = Some(pcbcraft_library::Drill {
            size: Point::new(1.4, 0.2),
            offset: Point::new(0.1, 0.1),
        });
        validate_footprint_support(&definition)?;
        Ok(())
    }
    #[test]
    fn diagonal_holes_cannot_escape_circle_oval_or_rounded_corners() -> Result<(), String> {
        for (shape, size, ratio, drill_size, offset) in [
            (
                "circle",
                Point::new(2.0, 2.0),
                None,
                Point::new(0.5, 0.5),
                Point::new(0.6, 0.6),
            ),
            (
                "oval",
                Point::new(4.0, 2.0),
                None,
                Point::new(0.5, 0.5),
                Point::new(1.6, 0.6),
            ),
            (
                "roundrect",
                Point::new(2.0, 2.0),
                Some(0.25),
                Point::new(0.2, 0.2),
                Point::new(0.86, 0.86),
            ),
        ] {
            let mut definition = dip()?;
            let p = &mut definition.pads[0];
            p.shape = shape.into();
            p.size = size;
            p.roundrect_ratio = ratio;
            p.drill = Some(pcbcraft_library::Drill {
                size: drill_size,
                offset,
            });
            assert!(
                validate_footprint_support(&definition).is_err(),
                "{shape} must reject drill escaping its actual outline"
            );
        }
        let mut definition = dip()?;
        let p = &mut definition.pads[0];
        p.shape = "rect".into();
        p.size = Point::new(2.0, 2.0);
        p.roundrect_ratio = None;
        p.drill = Some(pcbcraft_library::Drill {
            size: Point::new(0.5, 0.5),
            offset: Point::new(0.6, 0.6),
        });
        validate_footprint_support(&definition)?;
        Ok(())
    }

    #[test]
    fn checks_written_by_the_designer_survive_a_save_and_reload() -> Result<(), String> {
        let mut p = Project::default();
        p.custom_checks.push(CustomCheck {
            id: 7,
            name: "keep it small".into(),
            measure: "part count above".into(),
            limit: 5.0,
            severity: "warning".into(),
        });
        let text = serde_json::to_string(&p).map_err(|e| e.to_string())?;
        let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        assert_eq!(back.custom_checks, p.custom_checks, "the check travels");
        let plain = serde_json::to_string(&Project::default()).map_err(|e| e.to_string())?;
        let old: Project = serde_json::from_str(&plain).map_err(|e| e.to_string())?;
        assert!(
            old.custom_checks.is_empty(),
            "designs saved before this feature load with none"
        );
        Ok(())
    }

    #[test]
    fn a_set_name_on_a_library_part_survives_a_save() -> Result<(), String> {
        let def = pcbcraft_library::SymbolDefinition {
            name: "SetR".into(),
            reference: "R".into(),
            value: "10k".into(),
            description: "thin film".into(),
            keywords: "resistor".into(),
            footprint: String::new(),
            set: "Power bits".into(),
            pins: vec![],
            graphics: vec![],
            extends: None,
            raw_source: String::new(),
            unsupported: vec![],
        };
        let text = serde_json::to_string(&def).map_err(|e| e.to_string())?;
        assert!(text.contains("Power bits"), "the set travels in the file");
        let back: pcbcraft_library::SymbolDefinition =
            serde_json::from_str(&text).map_err(|e| e.to_string())?;
        assert_eq!(back.set, "Power bits", "and reads back intact");
        let plain = text.replace("\"set\":\"Power bits\",", "");
        let old: pcbcraft_library::SymbolDefinition =
            serde_json::from_str(&plain).map_err(|e| e.to_string())?;
        assert_eq!(
            old.set, "",
            "parts saved before sets existed load with no set"
        );
        Ok(())
    }
    #[test]
    fn chamfered_and_custom_through_hole_pads_keep_their_drill_inside_real_copper()
    -> Result<(), String> {
        let mut definition = dip()?;
        let p = &mut definition.pads[0];
        p.shape = "chamfered".into();
        p.size = Point::new(2.0, 2.0);
        p.roundrect_ratio = None;
        p.chamfer = Some(pcbcraft_library::PadChamfer {
            ratio: 0.5,
            corners: pcbcraft_library::ChamferCorners::ALL,
        });
        p.drill = Some(pcbcraft_library::Drill {
            size: Point::new(0.8, 0.8),
            offset: Point::default(),
        });
        validate_footprint_support(&definition)?;
        // Inside the square, but beyond the bottom-right cut.
        if let Some(d) = definition.pads[0].drill.as_mut() {
            d.size = Point::new(0.3, 0.3);
            d.offset = Point::new(0.6, 0.6);
        }
        assert!(validate_footprint_support(&definition).is_err());
        let p = &mut definition.pads[0];
        p.shape = "custom".into();
        p.chamfer = None;
        p.size = Point::new(0.4, 0.4);
        p.polygon = vec![
            Point::new(-1.0, -1.0),
            Point::new(0.3, -1.0),
            Point::new(0.3, -0.3),
            Point::new(1.0, -0.3),
            Point::new(1.0, 1.0),
            Point::new(-1.0, 1.0),
        ];
        // A notched pad: a hole centred in its notch has no copper around it.
        p.drill = Some(pcbcraft_library::Drill {
            size: Point::new(0.3, 0.3),
            offset: Point::new(0.65, -0.65),
        });
        assert!(validate_footprint_support(&definition).is_err());
        if let Some(d) = definition.pads[0].drill.as_mut() {
            d.offset = Point::new(-0.5, 0.5);
        }
        validate_footprint_support(&definition)?;
        let pad = Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 0.4,
            net: None,
            geometry: Some(definition.pads[0].clone()),
            castellated: None,
        };
        assert!(!pad.outline_is_convex());
        assert_eq!(pad.outline_points().len(), 7);
        assert!((pad.bounding_radius() - 2.0_f64.sqrt()).abs() < 1e-12);
        Ok(())
    }
    #[test]
    fn chamfer_outline_cuts_each_selected_corner_and_merges_meeting_cuts() {
        let all = chamfered_rect(
            Point::new(2.0, 2.0),
            pcbcraft_library::PadChamfer {
                ratio: 0.5,
                corners: pcbcraft_library::ChamferCorners::ALL,
            },
        );
        assert_eq!(all.len(), 4, "a fully cut square is a diamond");
        assert!((pcbcraft_geom::polygon_area(&all).abs() - 2.0).abs() < 1e-12);
        let one = chamfered_rect(
            Point::new(2.0, 1.0),
            pcbcraft_library::PadChamfer {
                ratio: 0.2,
                corners: pcbcraft_library::ChamferCorners {
                    top_left: true,
                    ..Default::default()
                },
            },
        );
        assert_eq!(one.len(), 5);
        assert!((pcbcraft_geom::polygon_area(&one).abs() - (2.0 - 0.02)).abs() < 1e-12);
        assert!(pcbcraft_geom::polygon_is_simple(&one));
    }
}
