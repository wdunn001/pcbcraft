//! `cargo xtask showcase`: builds the original sample projects in `examples/`.
//!
//! The generator drives an engine `Session` with ordinary commands, so the
//! output is reproducible and reviewable: running it twice writes identical
//! bytes. The design itself lives in `design.rs`; this file turns it into
//! library items, a netlist, schematic pages and a synced board.
//!
//! # Rules
//! - Only engine commands change the project; the one direct write is the
//!   final file save.
//! - Output must not depend on time, environment or hash-map order.
//!
//! # Dependencies
//! - xtask/src/showcase/design.rs
//! - xtask/src/showcase/parts.rs
//! - xtask/src/showcase/board.rs
//! - crates/engine/src/lib.rs
//! - crates/io/src/lib.rs
//!
//! # Do not
//! - Read a third-party library file; every part is defined in `parts.rs`.
mod board;
mod copper;
mod design;
mod finish;
mod layout;
mod parts;
mod routes;

use design::{DIRECT, PARTS, PINS, POWER_NETS, SHEETS};
use parts::{BoxUnit, PinSpec};
use pcbcraft_engine::Session;
use pcbcraft_geom::Point;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// Engine session plus the lookups the generator needs while it builds.
pub struct Gen {
    pub s: Session,
    pub comp: BTreeMap<&'static str, u64>,
    pub nets: BTreeMap<String, u64>,
}

type Res<T> = Result<T, String>;

impl Gen {
    /// Run one engine command and return its `result` object.
    pub fn cmd(&mut self, id: &str, params: Value) -> Res<Value> {
        let out = self
            .s
            .execute(id, params, None)
            .map_err(|e| format!("{id}: {e}"))?;
        Ok(out["result"].clone())
    }

    pub fn id_of(&self, reference: &str) -> Res<u64> {
        self.comp
            .get(reference)
            .copied()
            .ok_or_else(|| format!("unknown reference {reference}"))
    }
}

fn pin(number: &'static str, name: &'static str, kind: &'static str) -> PinSpec {
    (number, name, kind)
}

/// Every library symbol the design uses.
fn symbols() -> Vec<pcbcraft_library::SymbolDefinition> {
    let io = "bidirectional";
    let pw = "power_in";
    vec![
        parts::resistor("R_0402", "10 k", "Chip_0402"),
        parts::resistor("R_0603", "1 k", "Chip_0603"),
        parts::capacitor("C_0402", "100 nF", "Chip_0402"),
        parts::capacitor("C_0603", "10 uF", "Chip_0603"),
        parts::diode("LED_0603", "D", "LED", "LED_0603", true),
        parts::diode("TVS_SOD323", "D", "TVS", "SOD-323", false),
        parts::fuse("FUSE_1206", "500 mA", "Chip_1206"),
        parts::switch("SW_TACT", "Button", "Tact_4p5"),
        parts::crystal("XTAL_3225", "16 MHz", "Crystal_3225"),
        parts::test_point("TP_PAD", "TP", "Test_Pad"),
        parts::boxed(
            "USB_C_PWR",
            "J",
            "USB-C",
            "Power and USB 2.0 receptacle",
            "USB_C_Receptacle",
            &[BoxUnit {
                unit: 1,
                width: 12.7,
                left: vec![
                    pin("1", "GND", pw),
                    pin("8", "GND", pw),
                    pin("9", "SHELL", "passive"),
                    pin("10", "SHELL", "passive"),
                    pin("11", "SHELL", "passive"),
                    pin("12", "SHELL", "passive"),
                ],
                right: vec![
                    pin("2", "VBUS", "power_out"),
                    pin("7", "VBUS", "power_out"),
                    pin("3", "CC1", io),
                    pin("6", "CC2", io),
                    pin("4", "D+", io),
                    pin("5", "D-", io),
                ],
                top: vec![],
                bottom: vec![],
            }],
        ),
        parts::boxed(
            "BATT_2",
            "J",
            "Battery",
            "Two-pin battery connector",
            "Battery_2P",
            &[BoxUnit {
                unit: 1,
                width: 10.16,
                left: vec![],
                right: vec![pin("1", "+", "passive"), pin("2", "-", pw)],
                top: vec![],
                bottom: vec![],
            }],
        ),
        parts::boxed(
            "CHG_SOT23_5",
            "U",
            "Charger",
            "Single-cell linear charger",
            "SOT-23-5",
            &[BoxUnit {
                unit: 1,
                width: 15.24,
                left: vec![
                    pin("1", "VDD", pw),
                    pin("4", "PROG", "input"),
                    pin("2", "VSS", pw),
                ],
                right: vec![
                    pin("5", "STAT", "open_collector"),
                    pin("3", "VBAT", "power_out"),
                ],
                top: vec![],
                bottom: vec![],
            }],
        ),
        parts::boxed(
            "LDO_SOT23_5",
            "U",
            "Regulator",
            "Low-dropout regulator",
            "SOT-23-5",
            &[BoxUnit {
                unit: 1,
                width: 15.24,
                left: vec![
                    pin("1", "IN", pw),
                    pin("3", "EN", "input"),
                    pin("2", "GND", pw),
                ],
                right: vec![pin("5", "OUT", "power_out"), pin("4", "NC", "no_connect")],
                top: vec![],
                bottom: vec![],
            }],
        ),
        parts::boxed(
            "SWITCH_SOT23_5",
            "U",
            "Load switch",
            "High-side load switch",
            "SOT-23-5",
            &[BoxUnit {
                unit: 1,
                width: 15.24,
                left: vec![
                    pin("1", "IN", pw),
                    pin("3", "EN", "input"),
                    pin("2", "GND", pw),
                ],
                right: vec![pin("5", "OUT", "power_out"), pin("4", "NC", "no_connect")],
                top: vec![],
                bottom: vec![],
            }],
        ),
        parts::boxed(
            "MCU_QFN32",
            "U",
            "Microcontroller",
            "32-bit microcontroller with USB device",
            "QFN-32",
            &[
                BoxUnit {
                    unit: 1,
                    width: 25.4,
                    left: vec![
                        pin("7", "USB_DP", io),
                        pin("6", "USB_DM", io),
                        pin("29", "SWDIO", io),
                        pin("28", "SWCLK", "input"),
                        pin("27", "UART_TX", "output"),
                        pin("26", "UART_RX", "input"),
                        pin("24", "SPI_CS", "output"),
                        pin("23", "SPI_SCK", "output"),
                        pin("22", "SPI_MOSI", "output"),
                        pin("21", "SPI_MISO", "input"),
                        pin("5", "BTN2", "input"),
                    ],
                    right: vec![
                        pin("9", "LED0", "output"),
                        pin("10", "LED1", "output"),
                        pin("11", "LED2", "output"),
                        pin("12", "LED3", "output"),
                        pin("13", "LED4", "output"),
                        pin("14", "LED5", "output"),
                        pin("20", "BTN1", "input"),
                        pin("19", "I2C_SCL", io),
                        pin("18", "I2C_SDA", io),
                        pin("17", "SENSOR_EN", "output"),
                        pin("4", "PA0", io),
                    ],
                    top: vec![],
                    bottom: vec![],
                },
                BoxUnit {
                    unit: 2,
                    width: 20.32,
                    left: vec![
                        pin("25", "NRST", "input"),
                        pin("3", "XIN", "input"),
                        pin("2", "XOUT", "output"),
                    ],
                    right: vec![
                        pin("1", "VDD", pw),
                        pin("8", "VDDUSB", pw),
                        pin("15", "VDD", pw),
                        pin("30", "VDD", pw),
                        pin("32", "VDDA", pw),
                    ],
                    top: vec![],
                    bottom: vec![
                        pin("16", "VSS", pw),
                        pin("31", "VSS", pw),
                        pin("33", "EP", pw),
                    ],
                },
            ],
        ),
        parts::boxed(
            "FLASH_SOIC8",
            "U",
            "SPI flash",
            "Serial flash memory",
            "SOIC-8",
            &[BoxUnit {
                unit: 1,
                width: 15.24,
                left: vec![
                    pin("1", "CS", "input"),
                    pin("2", "SCK", "input"),
                    pin("3", "DI", "input"),
                    pin("4", "DO", "output"),
                    pin("6", "WP", "input"),
                    pin("7", "HOLD", "input"),
                ],
                right: vec![pin("8", "VCC", pw), pin("5", "GND", pw)],
                top: vec![],
                bottom: vec![],
            }],
        ),
        parts::boxed(
            "ENV_DFN8",
            "U",
            "Climate sensor",
            "Temperature, humidity and pressure sensor",
            "DFN-8",
            &[BoxUnit {
                unit: 1,
                width: 15.24,
                left: vec![
                    pin("2", "SCL", io),
                    pin("4", "SDA", io),
                    pin("6", "CSB", "input"),
                    pin("5", "SDO", "passive"),
                ],
                right: vec![
                    pin("1", "VDDIO", pw),
                    pin("8", "VDD", pw),
                    pin("3", "GND", pw),
                    pin("7", "GND", pw),
                ],
                top: vec![],
                bottom: vec![],
            }],
        ),
        parts::boxed(
            "LIGHT_SOT23_6",
            "U",
            "Light sensor",
            "Ambient light sensor",
            "SOT-23-6",
            &[BoxUnit {
                unit: 1,
                width: 15.24,
                left: vec![
                    pin("1", "SCL", io),
                    pin("3", "SDA", io),
                    pin("4", "ADDR", "passive"),
                ],
                right: vec![
                    pin("6", "VDD", pw),
                    pin("2", "GND", pw),
                    pin("5", "INT", "output"),
                ],
                top: vec![],
                bottom: vec![],
            }],
        ),
        parts::boxed(
            "HDR_1X7",
            "J",
            "Debug",
            "Seven-pin programming and serial header",
            "Header_1x7_1.27",
            &[BoxUnit {
                unit: 1,
                width: 12.7,
                left: vec![],
                right: vec![
                    pin("1", "3V3", pw),
                    pin("2", "GND", pw),
                    pin("3", "SWDIO", io),
                    pin("4", "SWCLK", "output"),
                    pin("5", "RX", "input"),
                    pin("6", "TX", "output"),
                    pin("7", "NRST", io),
                ],
                top: vec![],
                bottom: vec![],
            }],
        ),
    ]
}

/// Every library footprint the design uses.
fn footprints() -> Vec<pcbcraft_library::FootprintDefinition> {
    vec![
        parts::chip("Chip_0402", 0.48, (0.56, 0.62), (1.0, 0.5), false),
        parts::chip("Chip_0603", 0.8, (0.8, 0.9), (1.6, 0.8), false),
        parts::chip("Chip_1206", 1.5, (1.1, 1.7), (3.2, 1.6), false),
        parts::chip("LED_0603", 0.8, (0.8, 0.9), (1.6, 0.8), true),
        parts::chip("SOD-323", 1.05, (0.8, 0.7), (1.7, 1.25), true),
        parts::sot23("SOT-23-5", 5),
        parts::sot23("SOT-23-6", 6),
        parts::soic8("SOIC-8"),
        parts::qfn32("QFN-32"),
        parts::dfn8("DFN-8"),
        parts::crystal_3225("Crystal_3225"),
        parts::tact("Tact_4p5"),
        parts::header("Header_1x7_1.27", 7, 1.27, 0.95, 0.55),
        parts::battery_jst("Battery_2P"),
        parts::test_pad("Test_Pad"),
        parts::usb_c("USB_C_Receptacle"),
    ]
}

fn pt(p: Point) -> Value {
    json!({"x": p.x, "y": p.y})
}

/// Outward direction of a pin in schematic space, as a unit vector.
fn outward(g: &Gen, reference: &str, number: &str) -> Res<Point> {
    let id = g.id_of(reference)?;
    let c = g.s.project.component(id).ok_or("missing component")?;
    let p = c
        .pins
        .iter()
        .find(|p| p.number == number)
        .ok_or("missing pin")?;
    let angle = p.direction.unwrap_or(0.0).to_radians();
    let (_, rot) = c.unit_placement(p.unit).ok_or("missing unit")?;
    let v = Point::new(-angle.cos(), angle.sin());
    let r = v.rotate(-rot);
    Ok(Point::new(r.x.round(), r.y.round()))
}

/// Build the whole project into `g`.
pub fn build(g: &mut Gen) -> Res<()> {
    g.cmd("project.rename", json!({"name": "Showcase sensor node"}))?;
    for (key, value) in [
        ("title", "Showcase sensor node"),
        ("revision", "A"),
        ("company", "PCBCraft examples"),
        ("licence", "CC BY-SA 4.0"),
        (
            "notes",
            "Original battery powered environment sensor node. Generated by cargo xtask showcase.",
        ),
    ] {
        g.cmd("project.set_meta", json!({"key": key, "value": value}))?;
    }
    for sym in symbols() {
        g.cmd(
            "library.symbol.upsert",
            json!({"definition": sym, "notice": "Original showcase symbol"}),
        )?;
    }
    for fp in footprints() {
        g.cmd(
            "library.footprint.upsert",
            json!({"definition": fp, "notice": "Original showcase footprint"}),
        )?;
    }
    // Pages.
    let mut sheet_ids = vec![0_u64];
    for name in &SHEETS[1..] {
        let r = g.cmd("schematic.new_sheet", json!({"name": name}))?;
        sheet_ids.push(r["id"].as_u64().ok_or("sheet id")?);
    }
    // Components.
    for p in PARTS {
        let r = g.cmd(
            "component.place_custom",
            json!({"name": p.symbol, "position": {"x": p.sch.0, "y": p.sch.1}, "reference": p.reference, "value": p.value, "rotation": p.sch_rot}),
        )?;
        let id = r["id"].as_u64().ok_or("component id")?;
        g.comp.insert(p.reference, id);
        if p.sheet != 0 {
            g.cmd(
                "schematic.move_component",
                json!({"id": id, "sheet": sheet_ids[p.sheet]}),
            )?;
        }
        g.cmd(
            "footprint.assign_custom",
            json!({"id": id, "name": p.footprint}),
        )?;
        g.cmd(
            "component.field",
            json!({"id": id, "key": "description", "value": p.note}),
        )?;
        g.cmd(
            "component.field",
            json!({"id": id, "key": "height", "value": format!("{}", p.height)}),
        )?;
    }
    // Second unit of the microcontroller sits on its own spot of the page.
    let u3 = g.id_of("U3")?;
    g.cmd(
        "component.move",
        json!({"id": u3, "unit": 2, "position": {"x": 195.58, "y": 81.28}}),
    )?;
    // Nets.
    let mut by_net: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (r, pins) in PINS {
        for (n, net) in *pins {
            by_net.entry(net).or_default().push((r, n));
        }
    }
    let mut list = Vec::new();
    for (net, pins) in &by_net {
        let pins: Vec<Value> = pins
            .iter()
            .map(|(r, n)| json!({"reference": r, "pin": n}))
            .collect();
        list.push(json!({"name": net, "pins": pins}));
    }
    g.cmd("netlist.import", json!({"nets": list}))?;
    for n in &g.s.project.nets {
        g.nets.insert(n.name.clone(), n.id);
    }
    for r in ["U2", "U3", "U6", "U7"] {
        let id = g.id_of(r)?;
        g.cmd("pins.set_all_no_connect", json!({"component": id}))?;
    }
    draw_schematic(g, &sheet_ids)?;
    g.cmd("board.sync", json!({}))?;
    board::build(g)?;
    // Loading a file adopts free-form MPN and manufacturer fields into the typed record, so
    // the generated project already holds them and a save, load and save round trip is stable.
    g.s.project.adopt_free_form_sourcing();
    Ok(())
}

/// Draw wires, power and net labels, ports and the LED bus on the pages.
fn draw_schematic(g: &mut Gen, sheet_ids: &[u64]) -> Res<()> {
    let direct: Vec<(&str, &str)> = DIRECT
        .iter()
        .flat_map(|(a, b)| [(a.0, a.1), (b.0, b.1)])
        .collect();
    for ((ra, pa), (rb, pb)) in DIRECT {
        let (a, b) = (g.id_of(ra)?, g.id_of(rb)?);
        g.cmd(
            "wire.connect",
            json!({"from": {"component": a, "pin": pa}, "to": {"component": b, "pin": pb}}),
        )?;
    }
    for (r, pins) in PINS {
        let id = g.id_of(r)?;
        for (n, net) in *pins {
            if direct.contains(&(*r, *n)) {
                continue;
            }
            let c = g.s.project.component(id).ok_or("component")?;
            let Some(at) = c.pin_position(n) else {
                continue;
            };
            let dir = outward(g, r, n)?;
            let end = Point::new(at.x + dir.x * 5.08, at.y + dir.y * 5.08);
            let net_id = g.nets[*net];
            g.cmd(
                "wire.draw",
                json!({"points": [pt(at), pt(end)], "net": net_id}),
            )?;
            let kind = if POWER_NETS.contains(net) {
                "power"
            } else {
                "label"
            };
            let rotation = if dir.y != 0.0 { 90.0 } else { 0.0 };
            g.cmd(
                "label.place",
                json!({"kind": kind, "net": net_id, "position": pt(end), "rotation": rotation}),
            )?;
        }
    }
    // Hierarchy ports for every net that crosses pages.
    let mut sheets_of: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (r, pins) in PINS {
        let page = PARTS
            .iter()
            .find(|p| p.reference == *r)
            .map(|p| p.sheet)
            .unwrap_or(0);
        for (_, net) in *pins {
            let v = sheets_of.entry(net).or_default();
            if !v.contains(&page) {
                v.push(page);
            }
        }
    }
    let mut row = [0_u32; 3];
    for (net, pages) in &sheets_of {
        if pages.len() < 2 || net.starts_with("LED") {
            continue;
        }
        for page in pages {
            let net_id = g.nets[*net];
            let y = 12.7 + f64::from(row[*page]) * 5.08;
            row[*page] += 1;
            g.cmd("schematic.place_port", json!({"name": net, "net": net_id, "sheet": sheet_ids[*page], "position": {"x": 7.62, "y": y}}))?;
        }
    }
    // The LED lines travel between pages as one bus.
    g.cmd(
        "wire.draw_bus",
        json!({"points": [{"x": 133.35, "y": 33.0}, {"x": 133.35, "y": 96.0}], "name": "LED*6"}),
    )?;
    Ok(())
}

/// The same design with three known findings, for the Checks page: two
/// missing connections (warnings) and one piece of silkscreen over a pad
/// (an error).
pub fn findings_variant(base: &pcbcraft_doc::Project) -> Res<pcbcraft_doc::Project> {
    let mut g = Gen {
        s: Session::default(),
        comp: BTreeMap::new(),
        nets: BTreeMap::new(),
    };
    g.s.replace(base.clone()).map_err(|e| e.to_string())?;
    for net in ["LED0_A", "LED1_A"] {
        let id = base
            .nets
            .iter()
            .find(|n| n.name == net)
            .map(|n| n.id)
            .ok_or("net")?;
        let tracks: Vec<u64> = base
            .board
            .tracks
            .iter()
            .filter(|t| t.net == id)
            .map(|t| t.id)
            .collect();
        for t in tracks {
            g.cmd("object.delete", json!({"id": t}))?;
        }
    }
    g.cmd(
        "text.place",
        json!({"text": "HOT", "position": {"x": 3.0, "y": 44.4}, "size": 1.4, "layer": "F.Cu"}),
    )?;
    g.cmd(
        "project.rename",
        json!({"name": "Showcase sensor node with findings"}),
    )?;
    Ok(g.s.project.clone())
}

/// Entry point: `cargo xtask showcase [--out DIR]`.
pub fn run(root: &Path) -> Res<()> {
    let mut g = Gen {
        s: Session::default(),
        comp: BTreeMap::new(),
        nets: BTreeMap::new(),
    };
    build(&mut g)?;
    let out = root.join("examples").join("showcase.pcbcraft");
    pcbcraft_io::save(&g.s.project, &out)?;
    let p = &g.s.project;
    println!(
        "wrote {}: {} parts, {} nets, {} tracks, {} vias, {} zones",
        out.display(),
        p.components.len(),
        p.nets.len(),
        p.board.tracks.len(),
        p.board.vias.len(),
        p.board.zones.len()
    );
    let variant = findings_variant(&g.s.project)?;
    let out2 = root
        .join("examples")
        .join("showcase-with-findings.pcbcraft");
    pcbcraft_io::save(&variant, &out2)?;
    println!("wrote {}", out2.display());
    let checks = g.s.checks();
    println!(
        "erc {} drc {} airwires {}",
        checks["erc"].as_array().map_or(0, Vec::len),
        checks["drc"].as_array().map_or(0, Vec::len),
        checks["airwires"].as_array().map_or(0, Vec::len)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::Project;
    use std::sync::OnceLock;

    /// The generated project, built once per test process: a full build runs
    /// the checks many times, so every test shares one.
    fn shared() -> Result<&'static Project, String> {
        static CELL: OnceLock<Result<Project, String>> = OnceLock::new();
        CELL.get_or_init(|| {
            let mut g = Gen {
                s: Session::default(),
                comp: BTreeMap::new(),
                nets: BTreeMap::new(),
            };
            build(&mut g)?;
            Ok(g.s.project.clone())
        })
        .as_ref()
        .map_err(Clone::clone)
    }

    fn examples() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("examples")
    }

    fn saved_bytes(project: &Project) -> Result<Vec<u8>, String> {
        let dir = std::env::temp_dir().join(format!("showcase-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("out.pcbcraft");
        pcbcraft_io::save(project, &path)?;
        std::fs::read(&path).map_err(|e| e.to_string())
    }

    fn severity_counts(project: &Project) -> (usize, usize, usize) {
        let mut errors = 0;
        let mut warnings = 0;
        let mut other = 0;
        let all = pcbcraft_rules::erc(project)
            .into_iter()
            .chain(pcbcraft_rules::drc(project));
        for f in all {
            match f.severity.as_str() {
                "error" => errors += 1,
                "warning" => warnings += 1,
                _ => other += 1,
            }
        }
        (errors, warnings, other)
    }

    #[test]
    fn showcase_is_reproducible() -> Result<(), String> {
        let built = saved_bytes(shared()?)?;
        let again = {
            let mut g = Gen {
                s: Session::default(),
                comp: BTreeMap::new(),
                nets: BTreeMap::new(),
            };
            g.s.replace(shared()?.clone()).map_err(|e| e.to_string())?;
            saved_bytes(&g.s.project)?
        };
        assert_eq!(
            built, again,
            "saving the same project twice must give identical bytes"
        );
        let checked_in =
            std::fs::read(examples().join("showcase.pcbcraft")).map_err(|e| e.to_string())?;
        assert_eq!(
            built, checked_in,
            "examples/showcase.pcbcraft is out of date; run cargo xtask showcase"
        );
        let variant = std::fs::read(examples().join("showcase-with-findings.pcbcraft"))
            .map_err(|e| e.to_string())?;
        assert_eq!(
            saved_bytes(&findings_variant(shared()?)?)?,
            variant,
            "examples/showcase-with-findings.pcbcraft is out of date"
        );
        Ok(())
    }

    #[test]
    fn showcase_schematic_counts_are_stable() -> Result<(), String> {
        let p = shared()?;
        assert_eq!(p.sheets.len() + 1, 3, "root page plus two named pages");
        assert!((40..=60).contains(&p.components.len()), "40 to 60 parts");
        assert_eq!(p.components.len(), 55);
        assert_eq!(p.nets.len(), 41);
        Ok(())
    }

    #[test]
    fn showcase_ships_clean() -> Result<(), String> {
        let p = shared()?;
        assert_eq!(severity_counts(p), (0, 0, 0), "no finding of any severity");
        assert!(
            pcbcraft_rules::airwires(p).is_empty(),
            "every connection is made"
        );
        Ok(())
    }

    #[test]
    fn showcase_findings_variant_has_two_warnings_and_one_error() -> Result<(), String> {
        let v = findings_variant(shared()?)?;
        assert_eq!(
            severity_counts(&v),
            (1, 2, 0),
            "one error, two warnings, nothing else"
        );
        Ok(())
    }

    #[test]
    fn showcase_board_counts_are_stable() -> Result<(), String> {
        let b = &shared()?.board;
        assert_eq!(b.footprints.len(), 55, "parts on the board");
        assert_eq!(b.zones.len(), 5, "four pours and the sensor rail");
        assert_eq!(b.holes.len(), 4, "mounting holes");
        assert_eq!(b.tracks.len(), 232, "tracks");
        assert_eq!(b.vias.len(), 173, "vias");
        assert_eq!(shared()?.copper_layer_count(), 4, "four copper layers");
        assert_eq!(shared()?.variants.len(), 2, "two builds");
        assert_eq!(shared()?.output_jobs.len(), 3, "three output jobs");
        Ok(())
    }

    #[test]
    fn showcase_copper_runs_at_multiples_of_45_degrees() -> Result<(), String> {
        for t in &shared()?.board.tracks {
            let (dx, dy) = ((t.end.x - t.start.x).abs(), (t.end.y - t.start.y).abs());
            assert!(
                dx < 1e-6 || dy < 1e-6 || (dx - dy).abs() < 1e-3,
                "track {} is not at a multiple of 45 degrees",
                t.id
            );
        }
        Ok(())
    }

    #[test]
    fn showcase_labels_do_not_overlap() -> Result<(), String> {
        let problems = layout::audit(shared()?);
        assert!(problems.is_empty(), "label problems: {problems:?}");
        Ok(())
    }
}
