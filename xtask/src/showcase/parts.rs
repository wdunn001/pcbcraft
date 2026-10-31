//! Original symbols and footprints for the showcase sample project.
//!
//! Every shape here is drawn from published package dimensions (pad pitch,
//! body size) by the code below; nothing is copied from a third-party
//! library, so the generated project carries no foreign licence.
//!
//! # Rules
//! - Geometry is millimetres, y down, with pin angles in the library's
//!   counter-clockwise convention (0 points the pin at the body on its right).
//! - A symbol's pin numbers must equal its footprint's pad numbers.
//!
//! # Dependencies
//! - crates/library/src/lib.rs
//! - crates/geom/src/lib.rs
//!
//! # Do not
//! - Read a library file from disk; the generator must stay self-contained.
use pcbcraft_geom::Point;
use pcbcraft_library::{
    Drill, FootprintDefinition, FootprintPad, Graphic, SymbolDefinition, SymbolPin, UnitGraphic,
};

/// Millimetres between symbol pin grid points.
pub const GRID: f64 = 2.54;

fn pt(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

/// One pin of a boxed symbol: number, name and electrical type.
pub type PinSpec = (&'static str, &'static str, &'static str);

fn sym_pin(spec: PinSpec, x: f64, y: f64, angle: f64, unit: u32) -> SymbolPin {
    sym_pin_len(spec, x, y, angle, unit, GRID)
}

fn sym_pin_len(spec: PinSpec, x: f64, y: f64, angle: f64, unit: u32, length: f64) -> SymbolPin {
    SymbolPin {
        number: spec.0.into(),
        name: spec.1.into(),
        position: pt(x, y),
        angle,
        length,
        electrical_type: spec.2.into(),
        unit,
        body_style: 1,
        graphical_style: "line".into(),
        visible: true,
    }
}

fn ug(unit: u32, primitive: Graphic) -> UnitGraphic {
    UnitGraphic {
        unit,
        body_style: 1,
        primitive,
    }
}

fn line(unit: u32, a: (f64, f64), b: (f64, f64)) -> UnitGraphic {
    ug(
        unit,
        Graphic::Line {
            start: pt(a.0, a.1),
            end: pt(b.0, b.1),
            width: 0.254,
            layer: String::new(),
        },
    )
}

fn poly(unit: u32, points: &[(f64, f64)], fill: &str) -> UnitGraphic {
    ug(
        unit,
        Graphic::Polyline {
            points: points.iter().map(|p| pt(p.0, p.1)).collect(),
            width: 0.254,
            fill: fill.into(),
            layer: String::new(),
        },
    )
}

fn rect(unit: u32, a: (f64, f64), b: (f64, f64), fill: &str) -> UnitGraphic {
    ug(
        unit,
        Graphic::Rectangle {
            start: pt(a.0, a.1),
            end: pt(b.0, b.1),
            width: 0.254,
            fill: fill.into(),
            layer: String::new(),
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn symbol(
    name: &str,
    prefix: &str,
    value: &str,
    description: &str,
    footprint: &str,
    pins: Vec<SymbolPin>,
    graphics: Vec<UnitGraphic>,
) -> SymbolDefinition {
    SymbolDefinition {
        name: name.into(),
        reference: prefix.into(),
        value: value.into(),
        description: description.into(),
        keywords: String::new(),
        footprint: footprint.into(),
        set: "Showcase".into(),
        pins,
        graphics,
        extends: None,
        raw_source: String::new(),
        unsupported: vec![],
        retained: vec![],
    }
}

/// Pins and outline of one boxed unit.
///
/// `left`, `right`, `top` and `bottom` list the pins on each edge in reading
/// order; a pin with an empty number is a blank row that keeps the spacing.
/// Side pins sit `pitch` apart, top and bottom pins `column` apart, and every
/// pin has a stem of `STEM` millimetres, so a pin name and number never share
/// the stem with a wire. The body is `width` wide (a multiple of two grid
/// steps) and tall enough for the longer side column plus the top and bottom
/// pins.
pub struct BoxUnit {
    pub unit: u32,
    pub width: f64,
    pub pitch: f64,
    pub column: f64,
    pub left: Vec<PinSpec>,
    pub right: Vec<PinSpec>,
    pub top: Vec<PinSpec>,
    pub bottom: Vec<PinSpec>,
}

/// Length of the stem of every boxed-symbol pin.
pub const STEM: f64 = 2.0 * GRID;

fn box_unit(u: &BoxUnit, pins: &mut Vec<SymbolPin>, graphics: &mut Vec<UnitGraphic>) {
    let rows = u.left.len().max(u.right.len()).max(1);
    let top_y = -(((rows - 1) as f64 * u.pitch / 2.0) / GRID - 1e-9).ceil() * GRID;
    let bottom_y = top_y + (rows - 1) as f64 * u.pitch;
    let hw = u.width / 2.0;
    let hh_top = top_y - u.pitch / 2.0 - if u.top.is_empty() { GRID } else { 3.0 * GRID };
    let hh_bottom = bottom_y + u.pitch / 2.0 + if u.bottom.is_empty() { 0.0 } else { 3.0 * GRID };
    let mut put =
        |specs: &[PinSpec], x: &dyn Fn(usize) -> f64, y: &dyn Fn(usize) -> f64, angle: f64| {
            for (i, spec) in specs.iter().enumerate() {
                if !spec.0.is_empty() {
                    pins.push(sym_pin_len(*spec, x(i), y(i), angle, u.unit, STEM));
                }
            }
        };
    put(
        &u.left,
        &|_| -(hw + STEM),
        &|i| top_y + i as f64 * u.pitch,
        0.0,
    );
    put(
        &u.right,
        &|_| hw + STEM,
        &|i| top_y + i as f64 * u.pitch,
        180.0,
    );
    let column = |count: usize, i: usize| (i as f64 - (count as f64 - 1.0) / 2.0) * u.column;
    put(
        &u.top,
        &|i| column(u.top.len(), i),
        &|_| hh_top - STEM,
        270.0,
    );
    put(
        &u.bottom,
        &|i| column(u.bottom.len(), i),
        &|_| hh_bottom + STEM,
        90.0,
    );
    graphics.push(rect(u.unit, (-hw, hh_top), (hw, hh_bottom), "background"));
}

/// A boxed integrated-circuit or connector symbol built from one or more units.
pub fn boxed(
    name: &str,
    prefix: &str,
    value: &str,
    description: &str,
    footprint: &str,
    units: &[BoxUnit],
) -> SymbolDefinition {
    let mut pins = Vec::new();
    let mut graphics = Vec::new();
    for unit in units {
        box_unit(unit, &mut pins, &mut graphics);
    }
    symbol(name, prefix, value, description, footprint, pins, graphics)
}

const P1: PinSpec = ("1", "~", "passive");
const P2: PinSpec = ("2", "~", "passive");

fn two_pin(spec: (PinSpec, PinSpec), half: f64) -> Vec<SymbolPin> {
    vec![
        sym_pin(spec.0, -half, 0.0, 0.0, 1),
        sym_pin(spec.1, half, 0.0, 180.0, 1),
    ]
}

/// Resistor with a box body.
pub fn resistor(name: &str, value: &str, footprint: &str) -> SymbolDefinition {
    symbol(
        name,
        "R",
        value,
        "Fixed resistor",
        footprint,
        two_pin((P1, P2), 5.08),
        vec![rect(1, (-2.54, -1.016), (2.54, 1.016), "background")],
    )
}

/// Capacitor with two plates.
pub fn capacitor(name: &str, value: &str, footprint: &str) -> SymbolDefinition {
    symbol(
        name,
        "C",
        value,
        "Fixed capacitor",
        footprint,
        two_pin((P1, P2), 5.08),
        vec![
            line(1, (-2.54, 0.0), (-0.508, 0.0)),
            line(1, (0.508, 0.0), (2.54, 0.0)),
            line(1, (-0.508, -2.032), (-0.508, 2.032)),
            line(1, (0.508, -2.032), (0.508, 2.032)),
        ],
    )
}

/// Diode or light-emitting diode: pin 1 is the anode, pin 2 the cathode.
pub fn diode(
    name: &str,
    prefix: &str,
    value: &str,
    footprint: &str,
    light: bool,
) -> SymbolDefinition {
    let mut g = vec![
        poly(
            1,
            &[
                (-1.27, -1.524),
                (-1.27, 1.524),
                (1.27, 0.0),
                (-1.27, -1.524),
            ],
            "none",
        ),
        line(1, (1.27, -1.524), (1.27, 1.524)),
    ];
    if light {
        g.push(line(1, (0.0, -2.0), (1.4, -3.4)));
        g.push(line(1, (1.2, -2.0), (2.6, -3.4)));
        g.push(poly(1, &[(0.9, -3.4), (1.4, -3.4), (1.4, -2.9)], "none"));
        g.push(poly(1, &[(2.1, -3.4), (2.6, -3.4), (2.6, -2.9)], "none"));
    }
    g.push(line(1, (-2.54, 0.0), (-1.27, 0.0)));
    g.push(line(1, (1.27, 0.0), (2.54, 0.0)));
    let pins = vec![
        sym_pin(("1", "~", "passive"), -5.08, 0.0, 0.0, 1),
        sym_pin(("2", "~", "passive"), 5.08, 0.0, 180.0, 1),
    ];
    let description = if light {
        "Light-emitting diode"
    } else {
        "Transient voltage suppressor"
    };
    symbol(name, prefix, value, description, footprint, pins, g)
}

/// Resettable fuse drawn as a box with a diagonal.
pub fn fuse(name: &str, value: &str, footprint: &str) -> SymbolDefinition {
    symbol(
        name,
        "F",
        value,
        "Resettable fuse",
        footprint,
        two_pin((P1, P2), 5.08),
        vec![
            rect(1, (-2.54, -1.016), (2.54, 1.016), "background"),
            line(1, (-2.54, 1.016), (2.54, -1.016)),
        ],
    )
}

/// Push switch with a blade and two contacts.
pub fn switch(name: &str, value: &str, footprint: &str) -> SymbolDefinition {
    symbol(
        name,
        "SW",
        value,
        "Momentary push switch",
        footprint,
        two_pin((P1, P2), 5.08),
        vec![
            line(1, (-2.54, 0.0), (-0.6, 0.0)),
            line(1, (2.54, 0.0), (0.6, 0.0)),
            line(1, (-0.6, 0.0), (0.9, -1.6)),
            line(1, (0.0, -2.6), (0.0, -1.0)),
            line(1, (-1.0, -2.6), (1.0, -2.6)),
        ],
    )
}

/// Four-terminal crystal: pins 1 and 3 are the resonator, 2 and 4 the case.
pub fn crystal(name: &str, value: &str, footprint: &str) -> SymbolDefinition {
    let pins = vec![
        sym_pin(("1", "~", "passive"), -5.08, 0.0, 0.0, 1),
        sym_pin(("3", "~", "passive"), 5.08, 0.0, 180.0, 1),
        sym_pin(("2", "~", "passive"), -2.54, 5.08, 90.0, 1),
        sym_pin(("4", "~", "passive"), 2.54, 5.08, 90.0, 1),
    ];
    symbol(
        name,
        "Y",
        value,
        "Four-pad quartz crystal",
        footprint,
        pins,
        vec![
            rect(1, (-1.016, -1.778), (1.016, 1.778), "background"),
            line(1, (-2.54, -2.032), (-2.54, 2.032)),
            line(1, (2.54, -2.032), (2.54, 2.032)),
            line(1, (-1.016, 0.0), (-2.54, 0.0)),
            line(1, (1.016, 0.0), (2.54, 0.0)),
            line(1, (-2.54, 2.54), (2.54, 2.54)),
        ],
    )
}

/// Single-pin test point.
pub fn test_point(name: &str, value: &str, footprint: &str) -> SymbolDefinition {
    symbol(
        name,
        "TP",
        value,
        "Test point",
        footprint,
        vec![sym_pin(("1", "~", "passive"), -2.54, 0.0, 0.0, 1)],
        vec![ug(
            1,
            Graphic::Circle {
                center: pt(0.762, 0.0),
                radius: 0.762,
                width: 0.254,
                fill: "none".into(),
                layer: String::new(),
            },
        )],
    )
}

fn base_pad(number: &str, shape: &str, kind: &str, x: f64, y: f64, w: f64, h: f64) -> FootprintPad {
    let layers = if kind == "thru_hole" {
        vec!["*.Cu".to_owned(), "*.Mask".to_owned()]
    } else {
        vec!["F.Cu".to_owned(), "F.Mask".to_owned(), "F.Paste".to_owned()]
    };
    FootprintPad {
        number: number.into(),
        pad_type: kind.into(),
        shape: shape.into(),
        position: pt(x, y),
        rotation: 0.0,
        size: pt(w, h),
        layers,
        drill: None,
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    }
}

/// A rounded surface-mount pad.
pub fn smd(number: &str, x: f64, y: f64, w: f64, h: f64) -> FootprintPad {
    let mut p = base_pad(number, "roundrect", "smd", x, y, w, h);
    p.roundrect_ratio = Some(0.25);
    p
}

/// A plated through-hole pad; `oval` pads drill a round hole of `drill`.
pub fn tht(number: &str, shape: &str, x: f64, y: f64, w: f64, h: f64, drill: f64) -> FootprintPad {
    let mut p = base_pad(number, shape, "thru_hole", x, y, w, h);
    p.drill = Some(Drill {
        size: pt(drill, drill),
        offset: pt(0.0, 0.0),
    });
    if shape == "roundrect" {
        p.roundrect_ratio = Some(0.2);
    }
    p
}

fn gline(layer: &str, a: (f64, f64), b: (f64, f64), width: f64) -> Graphic {
    Graphic::Line {
        start: pt(a.0, a.1),
        end: pt(b.0, b.1),
        width,
        layer: layer.into(),
    }
}

fn grect(layer: &str, a: (f64, f64), b: (f64, f64), width: f64) -> Graphic {
    Graphic::Rectangle {
        start: pt(a.0, a.1),
        end: pt(b.0, b.1),
        width,
        fill: String::new(),
        layer: layer.into(),
    }
}

/// Courtyard and fabrication outline shared by every package.
fn outline(body: (f64, f64), court: (f64, f64)) -> Vec<Graphic> {
    vec![
        grect(
            "F.CrtYd",
            (-court.0 / 2.0, -court.1 / 2.0),
            (court.0 / 2.0, court.1 / 2.0),
            0.05,
        ),
        grect(
            "F.Fab",
            (-body.0 / 2.0, -body.1 / 2.0),
            (body.0 / 2.0, body.1 / 2.0),
            0.1,
        ),
    ]
}

fn footprint(
    name: &str,
    description: &str,
    pads: Vec<FootprintPad>,
    graphics: Vec<Graphic>,
) -> FootprintDefinition {
    FootprintDefinition {
        name: name.into(),
        description: description.into(),
        keywords: String::new(),
        pads,
        graphics,
        model: None,
        rules: None,
        raw_source: String::new(),
        unsupported: vec![],
        retained: vec![],
    }
}

/// Two-terminal chip package: pad centre offset, pad size and body size.
pub fn chip(
    name: &str,
    centre: f64,
    pad: (f64, f64),
    body: (f64, f64),
    led: bool,
) -> FootprintDefinition {
    let mut g = outline(body, (2.0 * centre + pad.0 + 0.3, pad.1.max(body.1) + 0.3));
    let silk_y = pad.1 / 2.0 + 0.2;
    if body.1 > 0.0 {
        g.push(gline(
            "F.SilkS",
            (-centre * 0.6, -silk_y),
            (centre * 0.6, -silk_y),
            0.12,
        ));
        g.push(gline(
            "F.SilkS",
            (-centre * 0.6, silk_y),
            (centre * 0.6, silk_y),
            0.12,
        ));
    }
    if led {
        g.push(gline(
            "F.SilkS",
            (centre + pad.0 / 2.0 + 0.25, -pad.1 / 2.0),
            (centre + pad.0 / 2.0 + 0.25, pad.1 / 2.0),
            0.2,
        ));
    }
    footprint(
        name,
        "Two-terminal chip package",
        vec![
            smd("1", -centre, 0.0, pad.0, pad.1),
            smd("2", centre, 0.0, pad.0, pad.1),
        ],
        g,
    )
}

/// Small-outline transistor package with three, five or six leads.
pub fn sot23(name: &str, leads: u8) -> FootprintDefinition {
    let mut pads = vec![
        smd("1", -1.25, -0.95, 1.0, 0.6),
        smd("2", -1.25, 0.0, 1.0, 0.6),
        smd("3", -1.25, 0.95, 1.0, 0.6),
    ];
    match leads {
        5 => {
            pads.push(smd("4", 1.25, 0.95, 1.0, 0.6));
            pads.push(smd("5", 1.25, -0.95, 1.0, 0.6));
        }
        _ => {
            pads.push(smd("4", 1.25, 0.95, 1.0, 0.6));
            pads.push(smd("5", 1.25, 0.0, 1.0, 0.6));
            pads.push(smd("6", 1.25, -0.95, 1.0, 0.6));
        }
    }
    let mut g = outline((1.6, 2.9), (3.9, 3.3));
    g.push(gline("F.SilkS", (-0.8, -1.55), (0.8, -1.55), 0.12));
    g.push(gline("F.SilkS", (-0.8, 1.55), (0.8, 1.55), 0.12));
    g.push(Graphic::Circle {
        center: pt(-2.2, -1.5),
        radius: 0.12,
        width: 0.12,
        fill: String::new(),
        layer: "F.SilkS".into(),
    });
    footprint(name, "Small-outline transistor package", pads, g)
}

/// Eight-lead small-outline package at 1.27 mm pitch.
pub fn soic8(name: &str) -> FootprintDefinition {
    let mut pads = Vec::new();
    for i in 0..4 {
        let y = -1.905 + 1.27 * f64::from(i);
        pads.push(smd(&(i + 1).to_string(), -2.7, y, 1.5, 0.6));
        pads.push(smd(&(8 - i).to_string(), 2.7, y, 1.5, 0.6));
    }
    let mut g = outline((3.9, 4.9), (6.8, 5.4));
    g.push(gline("F.SilkS", (-1.95, -2.55), (1.95, -2.55), 0.12));
    g.push(gline("F.SilkS", (-1.95, 2.55), (1.95, 2.55), 0.12));
    g.push(Graphic::Circle {
        center: pt(-3.9, -2.4),
        radius: 0.15,
        width: 0.12,
        fill: String::new(),
        layer: "F.SilkS".into(),
    });
    footprint(name, "Eight-lead small-outline package", pads, g)
}

/// Quad flat no-lead package with 8 pads per side and a centre pad numbered 33.
pub fn qfn32(name: &str) -> FootprintDefinition {
    let mut pads = Vec::new();
    let reach = 2.45;
    for i in 0..8 {
        let o = -1.75 + 0.5 * f64::from(i);
        pads.push(smd(&(1 + i).to_string(), -reach, o, 0.8, 0.3));
        pads.push(smd(&(9 + i).to_string(), o, reach, 0.3, 0.8));
        pads.push(smd(&(17 + i).to_string(), reach, -o, 0.8, 0.3));
        pads.push(smd(&(25 + i).to_string(), -o, -reach, 0.3, 0.8));
    }
    let mut ep = smd("33", 0.0, 0.0, 3.4, 3.4);
    ep.roundrect_ratio = Some(0.05);
    pads.push(ep);
    let mut g = outline((5.0, 5.0), (6.2, 6.2));
    for (a, b) in [
        ((-2.6, -2.1), (-2.6, -2.6)),
        ((-2.6, -2.6), (-2.1, -2.6)),
        ((2.6, -2.1), (2.6, -2.6)),
        ((2.6, -2.6), (2.1, -2.6)),
        ((-2.6, 2.1), (-2.6, 2.6)),
        ((-2.6, 2.6), (-2.1, 2.6)),
        ((2.6, 2.1), (2.6, 2.6)),
        ((2.6, 2.6), (2.1, 2.6)),
    ] {
        g.push(gline("F.SilkS", a, b, 0.12));
    }
    g.push(Graphic::Circle {
        center: pt(-3.3, -2.0),
        radius: 0.15,
        width: 0.12,
        fill: String::new(),
        layer: "F.SilkS".into(),
    });
    footprint(name, "Thirty-two pad quad flat no-lead package", pads, g)
}

/// Dual flat no-lead package with four pads a side at 0.5 mm pitch.
pub fn dfn8(name: &str) -> FootprintDefinition {
    let mut pads = Vec::new();
    for i in 0..4 {
        let y = -0.75 + 0.5 * f64::from(i);
        pads.push(smd(&(i + 1).to_string(), -1.15, y, 0.6, 0.3));
        pads.push(smd(&(8 - i).to_string(), 1.15, y, 0.6, 0.3));
    }
    let mut g = outline((2.5, 2.5), (3.4, 3.4));
    g.push(gline("F.SilkS", (-0.6, -1.4), (0.6, -1.4), 0.12));
    g.push(gline("F.SilkS", (-0.6, 1.4), (0.6, 1.4), 0.12));
    g.push(Graphic::Circle {
        center: pt(-1.8, -1.2),
        radius: 0.12,
        width: 0.12,
        fill: String::new(),
        layer: "F.SilkS".into(),
    });
    footprint(name, "Eight-pad dual flat no-lead package", pads, g)
}

/// Four-pad quartz crystal, 3.2 by 2.5 mm.
pub fn crystal_3225(name: &str) -> FootprintDefinition {
    let pads = vec![
        smd("1", -1.1, 0.85, 1.4, 1.15),
        smd("2", 1.1, 0.85, 1.4, 1.15),
        smd("3", 1.1, -0.85, 1.4, 1.15),
        smd("4", -1.1, -0.85, 1.4, 1.15),
    ];
    let mut g = outline((3.2, 2.5), (4.4, 3.4));
    g.push(gline("F.SilkS", (-1.0, -1.65), (1.0, -1.65), 0.12));
    g.push(gline("F.SilkS", (-1.0, 1.65), (1.0, 1.65), 0.12));
    g.push(gline("F.SilkS", (-2.3, -1.2), (-2.3, 1.2), 0.12));
    footprint(name, "Four-pad crystal, 3.2 by 2.5 mm", pads, g)
}

/// Surface-mount tactile switch with one wide pad on each side.
pub fn tact(name: &str) -> FootprintDefinition {
    let pads = vec![smd("1", -2.3, 0.0, 1.4, 2.4), smd("2", 2.3, 0.0, 1.4, 2.4)];
    let mut g = outline((4.5, 3.6), (6.6, 4.0));
    g.push(gline("F.SilkS", (-1.6, -1.9), (1.6, -1.9), 0.12));
    g.push(gline("F.SilkS", (-1.6, 1.9), (1.6, 1.9), 0.12));
    g.push(Graphic::Circle {
        center: pt(0.0, 0.0),
        radius: 1.0,
        width: 0.12,
        fill: String::new(),
        layer: "F.SilkS".into(),
    });
    footprint(name, "Tactile switch, 4.5 by 3.6 mm", pads, g)
}

/// Single row through-hole header at the given pitch.
pub fn header(name: &str, count: u32, pitch: f64, pad: f64, drill: f64) -> FootprintDefinition {
    let mut pads = Vec::new();
    for i in 0..count {
        let shape = if i == 0 { "roundrect" } else { "circle" };
        pads.push(tht(
            &(i + 1).to_string(),
            shape,
            0.0,
            f64::from(i) * pitch,
            pad,
            pad,
            drill,
        ));
    }
    let len = f64::from(count - 1) * pitch;
    let g = vec![
        grect(
            "F.CrtYd",
            (-pad / 2.0 - 0.4, -pad / 2.0 - 0.4),
            (pad / 2.0 + 0.4, len + pad / 2.0 + 0.4),
            0.05,
        ),
        grect(
            "F.Fab",
            (-pitch / 2.0, -pitch / 2.0),
            (pitch / 2.0, len + pitch / 2.0),
            0.1,
        ),
        gline(
            "F.SilkS",
            (-pad / 2.0 - 0.25, -pad / 2.0 - 0.25),
            (pad / 2.0 + 0.25, -pad / 2.0 - 0.25),
            0.12,
        ),
        gline(
            "F.SilkS",
            (-pad / 2.0 - 0.25, -pad / 2.0 - 0.25),
            (-pad / 2.0 - 0.25, len + pad / 2.0 + 0.25),
            0.12,
        ),
        gline(
            "F.SilkS",
            (pad / 2.0 + 0.25, -pad / 2.0 - 0.25),
            (pad / 2.0 + 0.25, len + pad / 2.0 + 0.25),
            0.12,
        ),
        gline(
            "F.SilkS",
            (-pad / 2.0 - 0.25, len + pad / 2.0 + 0.25),
            (pad / 2.0 + 0.25, len + pad / 2.0 + 0.25),
            0.12,
        ),
    ];
    footprint(name, "Single-row through-hole header", pads, g)
}

/// Two-pin battery connector with oval pads at 2.0 mm pitch.
pub fn battery_jst(name: &str) -> FootprintDefinition {
    let p1 = tht("1", "oval", -1.0, 0.0, 1.5, 2.2, 0.85);
    let p2 = tht("2", "oval", 1.0, 0.0, 1.5, 2.2, 0.85);
    let mut g = outline((6.0, 4.4), (7.0, 5.4));
    g.push(gline("F.SilkS", (-3.1, -2.4), (3.1, -2.4), 0.12));
    g.push(gline("F.SilkS", (-3.1, 2.4), (3.1, 2.4), 0.12));
    g.push(gline("F.SilkS", (-3.1, -2.4), (-3.1, 2.4), 0.12));
    g.push(gline("F.SilkS", (3.1, -2.4), (3.1, 2.4), 0.12));
    footprint(name, "Two-pin battery connector", vec![p1, p2], g)
}

/// Round test point whose copper is a chamfered polygon, a custom pad shape.
pub fn test_pad(name: &str) -> FootprintDefinition {
    let r = 0.9;
    let k = 0.38 * r;
    let polygon: Vec<Point> = [
        (-k, -r),
        (k, -r),
        (r, -k),
        (r, k),
        (k, r),
        (-k, r),
        (-r, k),
        (-r, -k),
    ]
    .iter()
    .map(|p| pt(p.0, p.1))
    .collect();
    let mut p = base_pad("1", "custom", "smd", 0.0, 0.0, 1.0, 1.0);
    p.layers = vec!["F.Cu".into(), "F.Mask".into()];
    p.polygon = polygon;
    let g = vec![
        grect("F.CrtYd", (-1.4, -1.4), (1.4, 1.4), 0.05),
        Graphic::Circle {
            center: pt(0.0, 0.0),
            radius: 1.25,
            width: 0.12,
            fill: String::new(),
            layer: "F.SilkS".into(),
        },
    ];
    footprint(
        name,
        "Test pad with a chamfered custom copper shape",
        vec![p],
        g,
    )
}

/// Mid-mount style USB receptacle with eight contact pads and four shell tabs.
pub fn usb_c(name: &str) -> FootprintDefinition {
    let map = ["1", "2", "3", "4", "5", "6", "7", "8"];
    let mut pads = Vec::new();
    for (i, n) in map.iter().enumerate() {
        let mut pad = smd(n, -3.5 + 1.0 * i as f64, 0.0, 0.6, 1.6);
        pad.shape = "rect".into();
        pad.roundrect_ratio = None;
        pads.push(pad);
    }
    for (i, (sx, sy, h)) in [
        (-5.0, -1.6, 2.0),
        (5.0, -1.6, 2.0),
        (-5.0, -5.6, 1.6),
        (5.0, -5.6, 1.6),
    ]
    .iter()
    .enumerate()
    {
        pads.push(tht(&(9 + i).to_string(), "oval", *sx, *sy, 1.0, *h, 0.6));
    }
    let g = vec![
        grect("F.CrtYd", (-6.0, -8.2), (6.0, 1.4), 0.05),
        grect("F.Fab", (-4.47, -7.35), (4.47, -0.2), 0.1),
        gline("F.SilkS", (-4.4, -7.4), (4.4, -7.4), 0.12),
        gline("F.SilkS", (-4.4, -7.4), (-4.4, -0.4), 0.12),
        gline("F.SilkS", (4.4, -7.4), (4.4, -0.4), 0.12),
    ];
    footprint(
        name,
        "Power and USB 2.0 receptacle, simplified contact set",
        pads,
        g,
    )
}
