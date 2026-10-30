//! Schematic placement and drawing of the showcase, page by page.
//!
//! Every part is listed once with its page, centre and rotation in grid steps
//! of 2.54 mm. The drawing functions then add the wires, supply symbols,
//! labels and page ports that read the three pages left to right: supply at
//! the top of a block, ground at the bottom, signals in between.
//!
//! # Rules
//! - Coordinates in `SPOTS` and in the page functions are grid steps; every
//!   pin end and wire vertex lands on the 2.54 mm grid.
//! - A wire is never drawn through a symbol: a part either touches a rail wire
//!   with a pin end, or reaches a supply symbol or label by a short stub.
//! - Net labels sit above their stub; page ports sit past the end of a stub.
//!
//! # Dependencies
//! - xtask/src/showcase/design.rs (pin to net assignments)
//! - xtask/src/showcase/mod.rs (`Gen`, `outward`)
//! - crates/engine/src/lib.rs (`wire.draw`, `wire.connect`, `label.place`,
//!   `schematic.place_port`)
//!
//! # Do not
//! - Change a net: this file only draws what the netlist already says.
use super::{Gen, Res, outward, pt};
use pcbcraft_geom::Point;
use serde_json::json;

/// One schematic grid step in millimetres.
pub const GRID: f64 = 2.54;

/// Width of one character of the 12 px monospace label font, in millimetres
/// at the zoom the A4 sheet is shown at.
pub const CHAR_MM: f64 = 2.7;

/// Where one part sits: page index, centre in grid steps, rotation in degrees.
///
/// Why: the layout is data, so moving a part is a one-line edit and the audit
/// can say which line to change.
///
/// # Rules
/// - `x` and `y` are whole grid steps; a part's pins then land on the grid.
/// - Two-pin parts stand upright only with rotation 90 or 270, which puts pin
///   1 at the bottom or the top.
///
/// # Dependencies
/// - `xtask/src/showcase/design.rs` (`PARTS`: every part listed there needs a spot)
///
/// # Do not
/// - Give two parts of one page the same spot.
pub struct Spot {
    pub reference: &'static str,
    pub sheet: usize,
    pub x: f64,
    pub y: f64,
    pub rot: f64,
}

const fn spot(reference: &'static str, sheet: usize, x: f64, y: f64, rot: f64) -> Spot {
    Spot {
        reference,
        sheet,
        x,
        y,
        rot,
    }
}

/// Every part of the design on its page.
pub const SPOTS: &[Spot] = &[
    // Power page.
    spot("J1", 0, 14.0, 23.0, 0.0),
    spot("F1", 0, 38.0, 18.0, 0.0),
    spot("U1", 0, 74.0, 25.0, 0.0),
    spot("R3", 0, 59.0, 26.0, 180.0),
    spot("D1", 0, 90.0, 26.0, 180.0),
    spot("R4", 0, 100.0, 26.0, 180.0),
    spot("R1", 0, 14.0, 40.0, 0.0),
    spot("R2", 0, 32.0, 40.0, 0.0),
    spot("D2", 0, 50.0, 40.0, 0.0),
    spot("C1", 0, 68.0, 40.0, 0.0),
    spot("C2", 0, 90.0, 40.0, 0.0),
    spot("J2", 0, 14.0, 58.0, 0.0),
    spot("TP1", 0, 30.0, 51.0, 0.0),
    spot("U2", 0, 50.0, 58.0, 0.0),
    spot("C3", 0, 32.0, 61.0, 0.0),
    spot("C4", 0, 74.0, 61.0, 0.0),
    spot("TP2", 0, 84.0, 57.0, 0.0),
    // Compute page.
    spot("U3", 1, 34.0, 21.0, 0.0),
    spot("J3", 1, 10.0, 22.0, 0.0),
    spot("U4", 1, 62.0, 21.0, 0.0),
    spot("C13", 1, 76.0, 14.0, 270.0),
    spot("Y1", 1, 22.0, 46.0, 0.0),
    spot("C5", 1, 9.0, 48.0, 270.0),
    spot("C6", 1, 35.0, 48.0, 270.0),
    spot("R5", 1, 14.0, 63.0, 0.0),
    spot("SW1", 1, 30.0, 63.0, 0.0),
    spot("C7", 1, 70.0, 41.0, 270.0),
    spot("C8", 1, 76.0, 41.0, 270.0),
    spot("C9", 1, 82.0, 41.0, 270.0),
    spot("C10", 1, 88.0, 41.0, 270.0),
    spot("C11", 1, 94.0, 41.0, 270.0),
    spot("C12", 1, 100.0, 41.0, 270.0),
    // Sensors and indicators page.
    spot("U7", 2, 30.0, 24.0, 0.0),
    spot("C17", 2, 46.0, 25.0, 270.0),
    spot("U5", 2, 68.0, 20.0, 0.0),
    spot("C14", 2, 71.0, 37.0, 0.0),
    spot("C15", 2, 83.0, 37.0, 0.0),
    spot("U6", 2, 100.0, 24.0, 0.0),
    spot("C16", 2, 95.0, 37.0, 0.0),
    spot("R8", 2, 94.0, 53.0, 0.0),
    spot("R9", 2, 94.0, 65.0, 0.0),
    spot("R10", 2, 36.0, 53.0, 0.0),
    spot("D3", 2, 42.0, 53.0, 0.0),
    spot("R11", 2, 57.0, 53.0, 0.0),
    spot("D4", 2, 63.0, 53.0, 0.0),
    spot("R12", 2, 78.0, 53.0, 0.0),
    spot("D5", 2, 84.0, 53.0, 0.0),
    spot("R13", 2, 36.0, 65.0, 0.0),
    spot("D6", 2, 42.0, 65.0, 0.0),
    spot("R14", 2, 57.0, 65.0, 0.0),
    spot("D7", 2, 63.0, 65.0, 0.0),
    spot("R15", 2, 78.0, 65.0, 0.0),
    spot("D8", 2, 84.0, 65.0, 0.0),
    spot("SW2", 2, 30.0, 41.0, 0.0),
    spot("SW3", 2, 56.0, 41.0, 0.0),
];

/// Centre of each unit of the microcontroller, in grid steps: unit number,
/// x, y. Unit 1, placed with the part, is the debug and flash interface, 2 the
/// general-purpose pins, 3 the indicator outputs and 4 the supply and clock.
pub const U3_UNITS: &[(u32, f64, f64)] = &[(2, 86.0, 21.0), (3, 90.0, 60.0), (4, 54.0, 46.0)];

/// The spot of a part, if it has one.
///
/// # Rules
/// - Total: a reference without a spot gives `None`, never a panic.
///
/// # Dependencies
/// - `SPOTS` above.
///
/// # Do not
/// - Look a part up by anything but its reference.
pub fn spot_of(reference: &str) -> Option<&'static Spot> {
    SPOTS.iter().find(|s| s.reference == reference)
}

fn gp(x: f64, y: f64) -> Point {
    Point::new(x * GRID, y * GRID)
}

/// Drawing state: the generator and the page being drawn.
struct Pen<'a> {
    g: &'a mut Gen,
    sheets: &'a [u64],
    page: usize,
    /// Every object created, with the page it must show on.
    made: &'a mut Vec<(u64, usize)>,
}

impl Pen<'_> {
    /// End of a pin, where a wire starts.
    fn at(&self, reference: &str, number: &str) -> Res<Point> {
        let id = self.g.id_of(reference)?;
        self.g
            .s
            .project
            .component(id)
            .and_then(|c| c.pin_position(number))
            .ok_or_else(|| format!("no pin {reference}.{number}"))
    }

    fn net(&self, name: &str) -> Res<u64> {
        self.g
            .nets
            .get(name)
            .copied()
            .ok_or_else(|| format!("unknown net {name}"))
    }

    fn note(&mut self, result: &serde_json::Value) -> Res<u64> {
        let id = result["id"].as_u64().ok_or("object id")?;
        self.made.push((id, self.page));
        Ok(id)
    }

    /// Draw a wire of `net` through the points, in grid steps.
    fn wire(&mut self, net: &str, points: &[Point]) -> Res<()> {
        let net_id = self.net(net)?;
        let list: Vec<_> = points.iter().map(|p| pt(*p)).collect();
        let r = self
            .g
            .cmd("wire.draw", json!({"points": list, "net": net_id}))?;
        self.note(&r)?;
        Ok(())
    }

    fn label_of(&mut self, kind: &str, net: &str, at: Point) -> Res<()> {
        let net_id = self.net(net)?;
        let r = self.g.cmd(
            "label.place",
            json!({"kind": kind, "net": net_id, "position": pt(at), "rotation": 0.0}),
        )?;
        self.note(&r)?;
        Ok(())
    }

    /// A supply symbol on `at`: a bar standing on it for a rail, bars hanging
    /// from it for ground.
    fn flag(&mut self, net: &str, at: Point) -> Res<()> {
        self.label_of("power", net, at)
    }

    /// A filled dot where a wire meets another wire or a pin.
    fn dot(&mut self, net: &str, at: Point) -> Res<()> {
        self.label_of("junction", net, at)
    }

    /// A net name above a wire; `at` is the middle of the text.
    fn tag(&mut self, net: &str, at: Point) -> Res<()> {
        self.label_of("label", net, at)
    }

    /// A page port: the net name in brackets, centred on `at`.
    fn port(&mut self, net: &str, at: Point) -> Res<()> {
        let net_id = self.net(net)?;
        let r = self.g.cmd(
            "schematic.place_port",
            json!({"name": net, "net": net_id, "sheet": self.sheets[self.page], "position": pt(at)}),
        )?;
        let id = r["id"].as_u64().ok_or("port id")?;
        self.made.push((id, self.page));
        Ok(())
    }

    /// Outward direction of a pin as a unit step.
    fn out(&self, reference: &str, number: &str) -> Res<Point> {
        outward(self.g, reference, number)
    }

    /// A wire from a pin straight out for `len` grid steps; returns its end.
    fn stub(&mut self, net: &str, reference: &str, number: &str, len: f64) -> Res<Point> {
        let start = self.at(reference, number)?;
        let d = self.out(reference, number)?;
        let end = Point::new(start.x + d.x * len * GRID, start.y + d.y * len * GRID);
        self.wire(net, &[start, end])?;
        Ok(end)
    }

    /// Stub plus a net label sitting above its outer part.
    fn sig(&mut self, net: &str, reference: &str, number: &str, len: f64) -> Res<()> {
        let end = self.stub(net, reference, number, len)?;
        let d = self.out(reference, number)?;
        let w = net.chars().count() as f64 * CHAR_MM;
        let back = w / 2.0 + 1.27;
        self.tag(net, Point::new(end.x - d.x * back, end.y))
    }

    /// Stub plus a page port that continues the line.
    fn off(&mut self, net: &str, reference: &str, number: &str, len: f64) -> Res<()> {
        let end = self.stub(net, reference, number, len)?;
        let d = self.out(reference, number)?;
        let w = (net.chars().count() + 2) as f64 * CHAR_MM;
        let push = w / 2.0 + 1.27;
        self.port(net, Point::new(end.x + d.x * push, end.y))
    }

    /// A top or bottom pin whose supply symbol stands off to the side, clear of
    /// the part's reference and value: out `stem` steps, then `run` steps along
    /// x (negative to the left).
    fn side(&mut self, net: &str, reference: &str, number: &str, stem: f64, run: f64) -> Res<()> {
        let start = self.at(reference, number)?;
        let d = self.out(reference, number)?;
        let bend = Point::new(start.x, start.y + d.y * stem * GRID);
        let end = Point::new(bend.x + run * GRID, bend.y);
        self.wire(net, &[start, bend, end])?;
        self.flag(net, end)
    }

    /// A plain wire between two pins with the net name above its middle.
    fn run(&mut self, net: &str, a: (&str, &str), b: (&str, &str)) -> Res<()> {
        let (pa, pb) = (self.at(a.0, a.1)?, self.at(b.0, b.1)?);
        self.wire(net, &[pa, pb])?;
        self.tag(net, Point::new((pa.x + pb.x) / 2.0, pa.y))
    }

    /// Stub plus a supply symbol: a stub of zero length puts the symbol on the pin.
    fn rail(&mut self, net: &str, reference: &str, number: &str, len: f64) -> Res<()> {
        let end = if len > 0.0 {
            self.stub(net, reference, number, len)?
        } else {
            self.at(reference, number)?
        };
        self.flag(net, end)
    }
}

/// Draw every page and return each object made with the page it must show on.
///
/// Why: wires and labels carry no page of their own; the document derives it
/// from the nearest pin of their net, so the caller checks the result with
/// `check_pages`.
///
/// # Rules
/// - Only wires, labels, junctions and ports are added; no net changes.
///
/// # Dependencies
/// - `xtask/src/showcase/mod.rs` (`Gen`), `crates/engine/src/lib.rs` (drawing commands)
///
/// # Do not
/// - Place a part here; parts are placed from `SPOTS`.
pub fn draw(g: &mut Gen, sheets: &[u64]) -> Res<Vec<(u64, usize)>> {
    let mut made = Vec::new();
    for page in 0..3 {
        let mut pen = Pen {
            g,
            sheets,
            page,
            made: &mut made,
        };
        match page {
            0 => power(&mut pen)?,
            1 => compute(&mut pen)?,
            _ => sensors(&mut pen)?,
        }
    }
    Ok(made)
}

fn power(p: &mut Pen) -> Res<()> {
    // USB connector: shell and ground pins share one ground symbol.
    for (n, y) in [
        ("1", 18.0),
        ("8", 20.0),
        ("9", 22.0),
        ("10", 24.0),
        ("11", 26.0),
        ("12", 28.0),
    ] {
        p.wire("GND", &[p.at("J1", n)?, gp(5.0, y)])?;
    }
    let trunk: Vec<Point> = (18..=30)
        .step_by(2)
        .map(|y| gp(5.0, f64::from(y)))
        .collect();
    p.wire("GND", &trunk)?;
    for y in [20.0, 22.0, 24.0, 26.0] {
        p.dot("GND", gp(5.0, y))?;
    }
    p.flag("GND", gp(5.0, 30.0))?;
    // Supply from the connector, through the fuse, to the charger.
    p.wire(
        "VBUS_RAW",
        &[p.at("J1", "2")?, gp(24.0, 18.0), p.at("F1", "1")?],
    )?;
    p.wire(
        "VBUS_RAW",
        &[p.at("J1", "7")?, gp(24.0, 20.0), gp(24.0, 18.0)],
    )?;
    p.dot("VBUS_RAW", gp(24.0, 18.0))?;
    p.flag("VBUS_RAW", gp(24.0, 18.0))?;
    p.wire("VBUS", &[p.at("F1", "2")?, p.at("U1", "1")?])?;
    p.tag("VBUS", gp(58.0, 18.0))?;
    // Configuration and data lines.
    p.sig("CC1", "J1", "3", 6.0)?;
    p.sig("CC2", "J1", "6", 6.0)?;
    p.off("USB_DP", "J1", "4", 3.0)?;
    p.off("USB_DN", "J1", "5", 3.0)?;
    for (r, net) in [("R1", "CC1"), ("R2", "CC2")] {
        p.sig(net, r, "1", 6.0)?;
        p.rail("GND", r, "2", 2.0)?;
    }
    // Charger.
    p.run("PROG", ("U1", "4"), ("R3", "1"))?;
    p.rail("GND", "R3", "2", 2.0)?;
    p.side("GND", "U1", "2", 1.0, 6.0)?;
    p.rail("VBAT", "U1", "3", 2.0)?;
    p.run("STAT", ("U1", "5"), ("D1", "2"))?;
    p.run("CHG_A", ("D1", "1"), ("R4", "2"))?;
    p.rail("VBUS", "R4", "1", 2.0)?;
    // Protection and bulk capacitors, each between its supply and ground.
    p.rail("GND", "D2", "1", 2.0)?;
    p.rail("VBUS_RAW", "D2", "2", 4.0)?;
    p.rail("VBUS", "C1", "1", 2.0)?;
    p.rail("GND", "C1", "2", 2.0)?;
    p.rail("VBAT", "C2", "1", 2.0)?;
    p.rail("GND", "C2", "2", 2.0)?;
    // Battery connector and test point.
    p.rail("VBAT", "J2", "1", 2.0)?;
    p.rail("GND", "J2", "2", 2.0)?;
    p.rail("VBAT", "TP1", "1", 4.0)?;
    // Regulator.
    p.side("VBAT", "U2", "1", 1.0, 6.0)?;
    p.rail("VBAT", "U2", "3", 2.0)?;
    p.side("GND", "U2", "2", 1.0, 6.0)?;
    p.rail("3V3", "U2", "5", 2.0)?;
    p.rail("VBAT", "C3", "1", 2.0)?;
    p.rail("GND", "C3", "2", 2.0)?;
    p.rail("3V3", "C4", "1", 2.0)?;
    p.rail("GND", "C4", "2", 2.0)?;
    p.rail("3V3", "TP2", "1", 4.0)?;
    Ok(())
}

fn compute(p: &mut Pen) -> Res<()> {
    // Debug header to the interface unit.
    for (a, b, net) in [
        ("3", "29", "SWDIO"),
        ("4", "28", "SWCLK"),
        ("5", "27", "UART_TX"),
        ("6", "26", "UART_RX"),
    ] {
        p.wire(net, &[p.at("J3", a)?, p.at("U3", b)?])?;
    }
    p.rail("3V3", "J3", "1", 2.0)?;
    p.sig("NRST", "J3", "7", 8.0)?;
    p.rail("GND", "J3", "2", 2.0)?;
    // Flash on the SPI lines.
    for (a, b, net) in [
        ("24", "1", "SPI_CS"),
        ("23", "2", "SPI_SCK"),
        ("22", "3", "SPI_MOSI"),
        ("21", "4", "SPI_MISO"),
    ] {
        p.run(net, ("U3", a), ("U4", b))?;
    }
    // Its supply: the top pin and the two strap pins share one rail, with the
    // capacitor beside it.
    let vcc = p.at("U4", "8")?;
    let wp = p.at("U4", "6")?;
    let hold = p.at("U4", "7")?;
    let trunk = wp.x + 2.0 * GRID;
    let top = vcc.y;
    p.wire(
        "3V3",
        &[wp, Point::new(trunk, wp.y), Point::new(trunk, top)],
    )?;
    p.wire(
        "3V3",
        &[hold, Point::new(trunk, hold.y), Point::new(trunk, wp.y)],
    )?;
    p.wire("3V3", &[vcc, Point::new(trunk, top), p.at("C13", "1")?])?;
    p.dot("3V3", Point::new(trunk, top))?;
    p.dot("3V3", Point::new(trunk, wp.y))?;
    p.flag("3V3", Point::new(vcc.x + 4.0 * GRID, top))?;
    let c13 = p.at("C13", "2")?;
    p.wire("GND", &[c13, Point::new(c13.x + 4.0 * GRID, c13.y)])?;
    p.flag("GND", Point::new(c13.x + 4.0 * GRID, c13.y))?;
    p.side("GND", "U4", "5", 1.0, -4.0)?;
    // General-purpose pins leave the page as ports.
    for (n, net) in [
        ("7", "USB_DP"),
        ("6", "USB_DN"),
        ("20", "BTN1"),
        ("5", "BTN2"),
        ("19", "I2C_SCL"),
        ("18", "I2C_SDA"),
        ("17", "SENSOR_EN"),
    ] {
        p.off(net, "U3", n, 3.0)?;
    }
    for (n, net) in [
        ("9", "LED0"),
        ("10", "LED1"),
        ("11", "LED2"),
        ("12", "LED3"),
        ("13", "LED4"),
        ("14", "LED5"),
    ] {
        p.off(net, "U3", n, 3.0)?;
    }
    // Supply pins of the microcontroller share one rail with the capacitor bank.
    let trunk_x = gp(64.0, 0.0).x;
    let rail_y = gp(0.0, 39.0).y;
    let mut stem = Vec::new();
    for n in ["1", "8", "15", "30", "32"] {
        let pin = p.at("U3", n)?;
        p.wire("3V3", &[pin, Point::new(trunk_x, pin.y)])?;
        stem.push(Point::new(trunk_x, pin.y));
    }
    stem.reverse();
    stem.push(Point::new(trunk_x, rail_y));
    p.wire("3V3", &stem)?;
    for q in &stem[1..stem.len() - 1] {
        p.dot("3V3", *q)?;
    }
    let bank = ["C7", "C8", "C9", "C10", "C11", "C12"];
    let mut top = vec![Point::new(trunk_x, rail_y)];
    let mut bottom = Vec::new();
    for c in bank {
        top.push(p.at(c, "1")?);
        bottom.push(p.at(c, "2")?);
    }
    p.wire("3V3", &top)?;
    bottom.push(gp(104.0, 43.0));
    p.wire("GND", &bottom)?;
    for c in bank {
        p.dot("3V3", p.at(c, "1")?)?;
        p.dot("GND", p.at(c, "2")?)?;
    }
    p.flag("3V3", gp(66.0, 39.0))?;
    p.flag("GND", gp(104.0, 43.0))?;
    // Ground pins of the supply unit.
    let gnd: Vec<Point> = ["16", "31", "33"]
        .iter()
        .map(|n| p.at("U3", n))
        .collect::<Res<_>>()?;
    for q in &gnd {
        p.wire("GND", &[*q, Point::new(q.x, q.y + GRID)])?;
    }
    let low: Vec<Point> = gnd.iter().map(|q| Point::new(q.x, q.y + GRID)).collect();
    let end = Point::new(low[2].x + 4.0 * GRID, low[2].y);
    p.wire("GND", &[low[0], low[1], low[2], end])?;
    p.dot("GND", low[1])?;
    p.flag("GND", end)?;
    // Reset and clock.
    p.sig("NRST", "U3", "25", 6.0)?;
    p.sig("XIN", "U3", "3", 6.0)?;
    p.wire(
        "XOUT",
        &[p.at("Y1", "3")?, gp(35.0, 46.0), p.at("U3", "2")?],
    )?;
    p.dot("XOUT", gp(35.0, 46.0))?;
    p.tag("XOUT", gp(42.0, 46.0))?;
    p.wire("XIN", &[p.at("Y1", "1")?, gp(9.0, 46.0)])?;
    p.tag("XIN", gp(13.0, 46.0))?;
    p.wire("GND", &[p.at("C5", "2")?, gp(13.0, 50.0)])?;
    p.flag("GND", gp(13.0, 50.0))?;
    p.wire("GND", &[p.at("C6", "2")?, gp(39.0, 50.0)])?;
    p.flag("GND", gp(39.0, 50.0))?;
    p.wire("GND", &[p.at("Y1", "2")?, gp(21.0, 49.0), gp(30.0, 49.0)])?;
    p.wire("GND", &[p.at("Y1", "4")?, gp(23.0, 49.0)])?;
    p.dot("GND", gp(23.0, 49.0))?;
    p.flag("GND", gp(30.0, 49.0))?;
    p.rail("3V3", "R5", "1", 2.0)?;
    p.run("NRST", ("R5", "2"), ("SW1", "1"))?;
    p.rail("GND", "SW1", "2", 2.0)?;
    Ok(())
}

/// One indicator branch: the line arrives by a port, passes the resistor and
/// the diode, and ends on ground.
fn branch(p: &mut Pen, net: &str, resistor: &str, diode: &str) -> Res<()> {
    p.off(net, resistor, "1", 3.0)?;
    let node = format!("{net}_A");
    p.wire(&node, &[p.at(resistor, "2")?, p.at(diode, "1")?])?;
    p.rail("GND", diode, "2", 0.0)
}

fn sensors(p: &mut Pen) -> Res<()> {
    // Load switch.
    p.off("SENSOR_EN", "U7", "3", 3.0)?;
    p.side("3V3", "U7", "1", 1.0, 6.0)?;
    p.side("GND", "U7", "2", 1.0, 6.0)?;
    p.wire("3V3S", &[p.at("U7", "5")?, p.at("C17", "1")?])?;
    p.flag("3V3S", gp(40.0, 23.0))?;
    p.wire("GND", &[p.at("C17", "2")?, gp(50.0, 27.0)])?;
    p.flag("GND", gp(50.0, 27.0))?;
    // Climate sensor: supply pins share a rail, ground pins share a ground.
    let top: Vec<Point> = ["1", "6", "8"]
        .iter()
        .map(|n| p.at("U5", n))
        .collect::<Res<_>>()?;
    for q in &top {
        p.wire("3V3S", &[*q, Point::new(q.x, q.y - GRID)])?;
    }
    let mut rail: Vec<Point> = top.iter().map(|q| Point::new(q.x, q.y - GRID)).collect();
    rail.push(Point::new(rail[2].x + 4.0 * GRID, rail[2].y));
    p.wire("3V3S", &rail)?;
    p.dot("3V3S", rail[1])?;
    p.flag("3V3S", rail[3])?;
    let bottom: Vec<Point> = ["3", "5", "7"]
        .iter()
        .map(|n| p.at("U5", n))
        .collect::<Res<_>>()?;
    for q in &bottom {
        p.wire("GND", &[*q, Point::new(q.x, q.y + GRID)])?;
    }
    let low: Vec<Point> = bottom.iter().map(|q| Point::new(q.x, q.y + GRID)).collect();
    let end = Point::new(low[2].x + 4.0 * GRID, low[2].y);
    p.wire("GND", &[low[0], low[1], low[2], end])?;
    p.dot("GND", low[1])?;
    p.flag("GND", end)?;
    p.sig("I2C_SCL", "U5", "2", 10.0)?;
    p.sig("I2C_SDA", "U5", "4", 10.0)?;
    for c in ["C14", "C15"] {
        p.rail("3V3S", c, "1", 2.0)?;
        p.rail("GND", c, "2", 2.0)?;
    }
    // Light sensor.
    p.side("3V3S", "U6", "6", 1.0, 6.0)?;
    let bottom: Vec<Point> = ["2", "4"]
        .iter()
        .map(|n| p.at("U6", n))
        .collect::<Res<_>>()?;
    for q in &bottom {
        p.wire("GND", &[*q, Point::new(q.x, q.y + GRID)])?;
    }
    let low: Vec<Point> = bottom.iter().map(|q| Point::new(q.x, q.y + GRID)).collect();
    let end = Point::new(low[1].x + 4.0 * GRID, low[1].y);
    p.wire("GND", &[low[0], low[1], end])?;
    p.flag("GND", end)?;
    p.sig("I2C_SCL", "U6", "1", 10.0)?;
    p.sig("I2C_SDA", "U6", "3", 10.0)?;
    p.rail("3V3S", "C16", "1", 2.0)?;
    p.rail("GND", "C16", "2", 2.0)?;
    // Pull-ups.
    p.rail("3V3S", "R8", "1", 2.0)?;
    p.sig("I2C_SCL", "R8", "2", 10.0)?;
    p.rail("3V3S", "R9", "1", 2.0)?;
    p.sig("I2C_SDA", "R9", "2", 10.0)?;
    // Indicators and buttons.
    for (i, (r, d)) in [
        ("R10", "D3"),
        ("R11", "D4"),
        ("R12", "D5"),
        ("R13", "D6"),
        ("R14", "D7"),
        ("R15", "D8"),
    ]
    .into_iter()
    .enumerate()
    {
        branch(p, &format!("LED{i}"), r, d)?;
    }
    p.off("BTN1", "SW2", "1", 3.0)?;
    p.rail("GND", "SW2", "2", 0.0)?;
    p.off("BTN2", "SW3", "1", 3.0)?;
    p.rail("GND", "SW3", "2", 0.0)?;
    Ok(())
}

/// Fail when a drawn object shows on another page than the part it serves.
///
/// Wires and labels carry no page of their own: the document puts them on the
/// page of the nearest pin of their net, so a stray object would appear on
/// the wrong page without any error.
///
/// # Rules
/// - Every failing object is listed, so one run shows all of them.
///
/// # Dependencies
/// - `crates/doc/src/lib.rs` (`Project::on_schematic_page`)
///
/// # Do not
/// - Move an object to fix it here; move the part or the stub in `SPOTS` or the page function.
pub fn check_pages(g: &Gen, sheets: &[u64], made: &[(u64, usize)]) -> Res<()> {
    let p = &g.s.project;
    let mut bad = Vec::new();
    for (id, page) in made {
        if p.on_schematic_page(sheets[*page], *id) {
            continue;
        }
        let what = p
            .labels
            .iter()
            .find(|l| l.id == *id)
            .map(|l| {
                format!(
                    "{} label {} at {:.1},{:.1}",
                    l.kind,
                    p.net_name(l.net),
                    l.position.x / GRID,
                    l.position.y / GRID
                )
            })
            .or_else(|| {
                p.drawn_wires.iter().find(|w| w.id == *id).map(|w| {
                    let f = w.points.first().copied().unwrap_or_default();
                    format!(
                        "wire {:?} from {:.1},{:.1}",
                        w.net.map(|n| p.net_name(n)),
                        f.x / GRID,
                        f.y / GRID
                    )
                })
            })
            .unwrap_or_else(|| format!("object {id}"));
        bad.push(format!("{what} not on page {page}"));
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad.join("\n"))
    }
}

// ---------------------------------------------------------------------------
// Audit: the drawn schematic measured the way the canvas paints it.

/// Pixels per millimetre of the canvas when the A4 sheet is fitted to the
/// 1600 x 1000 capture window.
const PX: f64 = 2.69;

/// A rectangle as x0, y0, x1, y1 in millimetres.
type Rect = (f64, f64, f64, f64);

/// The middle of a rectangle in grid steps, for messages.
fn spot_text(r: Rect) -> String {
    format!(
        "{:.1},{:.1}",
        (r.0 + r.2) / 2.0 / GRID,
        (r.1 + r.3) / 2.0 / GRID
    )
}

fn grow(r: Rect, by: f64) -> Rect {
    (r.0 - by, r.1 - by, r.2 + by, r.3 + by)
}

fn hit(a: Rect, b: Rect) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

/// Whether the segment passes through the rectangle.
fn cuts(seg: (Point, Point), r: Rect) -> bool {
    let (a, b) = seg;
    if a.x.max(b.x) <= r.0 || a.x.min(b.x) >= r.2 || a.y.max(b.y) <= r.1 || a.y.min(b.y) >= r.3 {
        return false;
    }
    // Sample along the segment: it is axis-aligned or short, so this is exact enough.
    let n = 40;
    (0..=n).any(|i| {
        let t = f64::from(i) / f64::from(n);
        let (x, y) = (a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
        x > r.0 && x < r.2 && y > r.1 && y < r.3
    })
}

/// Where two segments cross at a point that is an end of neither.
fn crosses(a: (Point, Point), b: (Point, Point)) -> bool {
    let d = |p: Point, q: Point, r: Point| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (d(b.0, b.1, a.0), d(b.0, b.1, a.1));
    let (d3, d4) = (d(a.0, a.1, b.0), d(a.0, a.1, b.1));
    let eps = 1e-6;
    d1 * d2 < -eps && d3 * d4 < -eps
}

/// A mono-spaced text of `font` pixels as a rectangle: `anchor` is the middle
/// of the top, bottom or middle edge.
fn text_box(text: &str, font: f64, x: f64, y: f64, vertical: char) -> Rect {
    let w = text.chars().count() as f64 * 0.6 * font / PX;
    let h = font * 1.2 / PX;
    match vertical {
        'b' => (x - w / 2.0, y - h, x + w / 2.0, y),
        't' => (x - w / 2.0, y, x + w / 2.0, y + h),
        _ => (x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0),
    }
}

struct Page {
    texts: Vec<(String, Rect)>,
    bodies: Vec<(String, Rect)>,
    segments: Vec<(String, Point, Point)>,
    symbols: Vec<(String, Rect)>,
}

/// Everything one page puts on the canvas.
fn collect(p: &pcbcraft_doc::Project, page: u64) -> Page {
    let mut out = Page {
        texts: Vec::new(),
        bodies: Vec::new(),
        segments: Vec::new(),
        symbols: Vec::new(),
    };
    for c in p
        .components
        .iter()
        .flat_map(|c| c.schematic_views())
        .filter(|c| c.sheet == page)
    {
        let at = |q: Point| c.position.offset(q.rotate(-c.rotation));
        let Some(library) = &c.library_symbol else {
            continue;
        };
        let extent = c
            .drawing_points()
            .iter()
            .map(|q| q.rotate(-c.rotation).y.abs())
            .fold(0.0, f64::max)
            + 14.0 / PX;
        out.texts.push((
            format!("{} reference", c.reference),
            text_box(&c.reference, 15.0, c.position.x, c.position.y - extent, 'b'),
        ));
        out.texts.push((
            format!("{} value", c.reference),
            text_box(&c.value, 12.0, c.position.x, c.position.y + extent, 't'),
        ));
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for g in library
            .definition
            .graphics
            .iter()
            .filter(|g| (g.unit == 0 || g.unit == library.unit) && g.body_style <= 1)
        {
            for q in g.primitive.path_points() {
                let w = at(q);
                x0 = x0.min(w.x);
                y0 = y0.min(w.y);
                x1 = x1.max(w.x);
                y1 = y1.max(w.y);
            }
        }
        if x0 < x1 {
            out.bodies.push((c.reference.clone(), (x0, y0, x1, y1)));
        }
        for pin in library.definition.pins.iter().filter(|q| {
            (q.unit == 0 && library.unit == 1 || q.unit == library.unit) && q.body_style <= 1
        }) {
            if pin.name == "~" || pin.name.is_empty() {
                continue;
            }
            let (start, end) = (at(pin.position), at(pin.stem_end()));
            let (dx, dy) = (end.x - start.x, end.y - start.y);
            let w = pin.name.chars().count() as f64 * 6.0 / PX;
            let h = 10.0 / PX;
            let (sx, sy) = (3.0 / PX, 3.0 / PX);
            let r = if dx.abs() >= dy.abs() {
                if dx < 0.0 {
                    (end.x - sx - w, end.y - sy - h, end.x - sx, end.y - sy)
                } else {
                    (end.x + sx, end.y - sy - h, end.x + sx + w, end.y - sy)
                }
            } else if dy > 0.0 {
                (end.x + sx, end.y + sy, end.x + sx + w, end.y + sy + h)
            } else {
                (end.x + sx, end.y - sy - h, end.x + sx + w, end.y - sy)
            };
            out.texts
                .push((format!("{} pin name {}", c.reference, pin.name), r));
        }
        let centre = c.position;
        for pin in &c.pins {
            let q = at(pin.offset);
            let (tx, ty) = (centre.x - q.x, centre.y - q.y);
            let (cx, cy) = if tx.abs() >= ty.abs() {
                (q.x + tx.signum() * 7.0 / PX, q.y - 10.0 / PX)
            } else {
                (q.x + 9.0 / PX, q.y + ty.signum() * 9.0 / PX)
            };
            out.texts.push((
                format!("{} pin number {}", c.reference, pin.number),
                text_box(&pin.number, 11.0, cx, cy, 'm'),
            ));
        }
    }
    for w in &p.wires {
        if !p.on_schematic_page(page, w.id) {
            continue;
        }
        if let Some(path) = p.wire_path(w) {
            for s in path.windows(2) {
                out.segments
                    .push((p.net_name(w.net).to_string(), s[0], s[1]));
            }
            if let Some(run) = longest_run(&path) {
                let mid = Point::new((run.0.x + run.1.x) / 2.0, (run.0.y + run.1.y) / 2.0);
                out.texts.push((
                    format!("{} wire label", p.net_name(w.net)),
                    text_box(p.net_name(w.net), 12.0, mid.x, mid.y - 6.0 / PX, 'b'),
                ));
            }
        }
    }
    for w in &p.drawn_wires {
        if !p.on_schematic_page(page, w.id) {
            continue;
        }
        let net = w.net.map_or("", |n| p.net_name(n)).to_string();
        for s in w.points.windows(2) {
            out.segments.push((net.clone(), s[0], s[1]));
        }
    }
    for l in &p.labels {
        if !p.on_schematic_page(page, l.id) {
            continue;
        }
        let name = p.net_name(l.net);
        let (x, y) = (l.position.x, l.position.y);
        match l.kind.as_str() {
            "label" => out
                .texts
                .push((format!("label {name}"), text_box(name, 12.0, x, y, 'b'))),
            "power" => {
                if name.starts_with("GND") {
                    out.symbols.push((
                        format!("ground symbol {name}"),
                        (x - 1.8, y + 0.3, x + 1.8, y + 1.8),
                    ));
                    out.texts.push((
                        format!("ground name {name}"),
                        text_box(name, 12.0, x, y + 2.4, 't'),
                    ));
                } else {
                    out.symbols.push((
                        format!("supply symbol {name}"),
                        (x - 1.6, y - 5.0, x + 1.6, y - 0.3),
                    ));
                    out.texts.push((
                        format!("supply name {name}"),
                        text_box(name, 12.0, x, y - 5.8, 'b'),
                    ));
                }
            }
            _ => {}
        }
    }
    for port in p.ports.iter().filter(|q| q.sheet == page) {
        out.texts.push((
            format!("port {}", port.name),
            text_box(
                &format!("[{}]", port.name),
                12.0,
                port.position.x,
                port.position.y,
                'm',
            ),
        ));
    }
    out
}

/// The longest horizontal run of a path, else the longest run, as the canvas
/// picks where a wire's net name goes.
fn longest_run(path: &[Point]) -> Option<(Point, Point)> {
    let len = |a: &Point, b: &Point| a.distance(*b);
    path.windows(2)
        .filter(|w| (w[0].y - w[1].y).abs() < 1e-6)
        .max_by(|a, b| len(&a[0], &a[1]).total_cmp(&len(&b[0], &b[1])))
        .or_else(|| {
            path.windows(2)
                .max_by(|a, b| len(&a[0], &a[1]).total_cmp(&len(&b[0], &b[1])))
        })
        .map(|w| (w[0], w[1]))
}

/// The inner frame of the sheet and its title block, in millimetres, as the
/// canvas derives them from everything drawn on all pages.
fn frame(p: &pcbcraft_doc::Project) -> (Rect, Rect) {
    let mut pts: Vec<Point> = Vec::new();
    for c in p.components.iter().flat_map(|c| c.schematic_views()) {
        pts.push(c.position);
        for pin in &c.pins {
            pts.push(c.position.offset(pin.offset.rotate(-c.rotation)));
        }
        pts.extend(
            c.drawing_points()
                .iter()
                .map(|q| c.position.offset(q.rotate(-c.rotation))),
        );
    }
    for w in &p.wires {
        if let Some(path) = p.wire_path(w) {
            pts.extend(path);
        }
    }
    for w in &p.drawn_wires {
        pts.extend(w.points.iter().copied());
    }
    pts.extend(p.labels.iter().map(|l| l.position));
    pts.extend(p.ports.iter().map(|q| q.position));
    let (mut lo, mut hi) = (pts[0], pts[0]);
    for q in &pts {
        lo = Point::new(lo.x.min(q.x), lo.y.min(q.y));
        hi = Point::new(hi.x.max(q.x), hi.y.max(q.y));
    }
    let (clo, chi) = (
        Point::new(lo.x - 6.0, lo.y - 12.0),
        Point::new(hi.x + 6.0, hi.y + 14.0),
    );
    let (w, h) = (297.0, 210.0);
    let band = (w * 0.02_f64).clamp(3.0, 8.0);
    let tb_w = (w * 0.32_f64).clamp(36.0, 110.0);
    let tb_h = tb_w * 0.25;
    let (cx, cy) = ((clo.x + chi.x) / 2.0, (clo.y + chi.y) / 2.0);
    let sx = (cx - w / 2.0).round();
    let sy = (cy - band - (h - 2.0 * band - tb_h) / 2.0).round();
    let inner = (sx + band, sy + band, sx + w - band, sy + h - band);
    let block = (inner.2 - tb_w, inner.3 - tb_h, inner.2, inner.3);
    (inner, block)
}

/// Every problem the three pages show, as plain sentences; empty when the
/// drawing is clean.
///
/// Why: the pages are read as a drawing, so text on text, a wire through a
/// part, an object past the frame or off the grid must fail a test instead of
/// waiting for a screenshot.
///
/// # Rules
/// - Text is measured as the canvas paints it at the A4 fit of a 1600 x 1000
///   window (`PX`); the frame is derived the way the canvas derives it.
///
/// # Dependencies
/// - `crates/ui-egui/src/canvas.rs` (text sizes and offsets, mirrored by the constants here)
/// - `crates/ui-egui/src/canvas/sheet.rs` (frame and title block, mirrored by `frame`)
///
/// # Do not
/// - Edit the project; this only reads it.
pub fn audit(p: &pcbcraft_doc::Project) -> Vec<String> {
    let mut out = Vec::new();
    let (inner, block) = frame(p);
    let mut pages = vec![0];
    pages.extend(p.sheets.iter().map(|s| s.id));
    for (n, page) in pages.iter().enumerate() {
        let c = collect(p, *page);
        let at = |what: &str| format!("page {} {what}", n + 1);
        for (i, (na, a)) in c.texts.iter().enumerate() {
            let tight = grow(*a, -0.3);
            let own = |x: &str, y: &str| x.split(' ').next() == y.split(' ').next();
            for (nb, b) in c.texts.iter().skip(i + 1) {
                let numbers = na.contains("pin number") && nb.contains("pin number");
                if hit(tight, grow(*b, -0.3)) && !(numbers && own(na, nb)) {
                    out.push(at(&format!("{na} overlaps {nb} near {}", spot_text(*a))));
                }
            }
            // A pin number is painted by the canvas right beside its pin, on the
            // pin's own wire and symbol; only other text can collide with it.
            if na.contains("pin number") {
                continue;
            }
            for (nb, b) in &c.symbols {
                if hit(tight, *b) {
                    out.push(at(&format!("{na} overlaps {nb} near {}", spot_text(*a))));
                }
            }
            for (nb, b) in &c.bodies {
                let own = na.split(' ').next() == Some(nb.as_str()) && na.contains("pin name");
                if !own && hit(tight, *b) {
                    out.push(at(&format!("{na} covers part {nb}")));
                }
            }
            for (net, s0, s1) in &c.segments {
                if cuts((*s0, *s1), tight) {
                    out.push(at(&format!("{na} sits on a wire of {net}")));
                }
            }
            if a.0 < inner.0 + 2.0
                || a.1 < inner.1 + 2.0
                || a.2 > inner.2 - 2.0
                || a.3 > inner.3 - 2.0
            {
                out.push(at(&format!("{na} leaves the frame")));
            }
            if hit(*a, grow(block, 2.0)) {
                out.push(at(&format!("{na} touches the title block")));
            }
        }
        for (nb, b) in &c.bodies {
            for (net, s0, s1) in &c.segments {
                if cuts((*s0, *s1), grow(*b, -0.3)) {
                    out.push(at(&format!("a wire of {net} crosses part {nb}")));
                }
            }
        }
        for (i, (na, a)) in c.bodies.iter().enumerate() {
            for (nb, b) in c.bodies.iter().skip(i + 1) {
                if hit(grow(*a, -0.3), grow(*b, -0.3)) {
                    out.push(at(&format!("part {na} overlaps part {nb}")));
                }
            }
        }
        for (i, (na, a0, a1)) in c.segments.iter().enumerate() {
            for (nb, b0, b1) in c.segments.iter().skip(i + 1) {
                if na != nb && crosses((*a0, *a1), (*b0, *b1)) {
                    out.push(at(&format!("a wire of {na} crosses a wire of {nb}")));
                }
            }
        }
    }
    let on_grid = |v: f64| ((v / GRID).round() * GRID - v).abs() < 1e-6;
    for c in p.components.iter().flat_map(|c| c.schematic_views()) {
        for pin in &c.pins {
            if let Some(q) = c.pin_position(&pin.number)
                && !(on_grid(q.x) && on_grid(q.y))
            {
                out.push(format!(
                    "pin {}.{} is off the grid",
                    c.reference, pin.number
                ));
            }
        }
    }
    for w in &p.drawn_wires {
        if w.points.iter().any(|q| !(on_grid(q.x) && on_grid(q.y))) {
            out.push(format!(
                "a wire of {} has a vertex off the grid",
                w.net.map_or("", |n| p.net_name(n))
            ));
        }
    }
    out.sort();
    out.dedup();
    out
}
