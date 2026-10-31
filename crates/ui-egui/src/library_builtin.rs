//! What the library list shows for the built-in parts: a short catalog line
//! for each kind and the footprint the board draws for it, as a library
//! definition so the card thumbnail uses the same drawing path as installed
//! packages.
//!
//! # Rules
//! - The pads match what `board.sync` gives a template part: two round pads
//!   5 mm apart, 1.6 mm across. Silk and drill are added for the picture only.
//! - A kind with no entry returns `None`; the card then shows the placeholder.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs#TEMPLATES
//! - crates/engine/src/lib.rs#component_pads (the pad positions to match)
//! - crates/library/src/lib.rs (FootprintDefinition, Graphic)
//! - crates/text/strings/en.txt (library.dock.builtin.*)

use pcbcraft_geom::Point;
use pcbcraft_library::{Drill, FootprintDefinition, FootprintPad, Graphic};
use pcbcraft_text::t;

/// One line saying what a built-in kind is.
///
/// # Rules
/// - Text comes from the catalog; an unknown kind gives an empty string.
///
/// # Do not
/// - Add a kind here without a `library.dock.builtin.<kind>` catalog line.
pub(crate) fn describe(kind: &str) -> String {
    match kind {
        "resistor" => t!("library.dock.builtin.resistor"),
        "led" => t!("library.dock.builtin.led"),
        "capacitor" => t!("library.dock.builtin.capacitor"),
        "connector" => t!("library.dock.builtin.connector"),
        _ => String::new(),
    }
}

/// The footprint of a built-in kind: two through-hole pads, a silk outline
/// and the mark that tells the kind apart. `None` for a kind we do not know.
///
/// # Rules
/// - Pad 1 is at x = -2.5 mm and pad 2 at x = +2.5 mm, as the board places them.
pub(crate) fn footprint(kind: &str) -> Option<FootprintDefinition> {
    if !matches!(kind, "resistor" | "led" | "capacitor" | "connector") {
        return None;
    }
    let pad = |number: &str, x: f64| FootprintPad {
        number: number.into(),
        pad_type: "thru_hole".into(),
        shape: "circle".into(),
        position: Point::new(x, 0.0),
        rotation: 0.0,
        size: Point::new(1.6, 1.6),
        layers: vec!["*.Cu".into(), "*.Mask".into()],
        drill: Some(Drill {
            size: Point::new(0.8, 0.8),
            offset: Point::new(0.0, 0.0),
        }),
        roundrect_ratio: None,
        chamfer: None,
        polygon: Vec::new(),
        raw_source: String::new(),
    };
    let line = |a: (f64, f64), b: (f64, f64)| Graphic::Line {
        start: Point::new(a.0, a.1),
        end: Point::new(b.0, b.1),
        width: 0.15,
        layer: "F.SilkS".into(),
    };
    let rect = |a: (f64, f64), b: (f64, f64)| Graphic::Rectangle {
        start: Point::new(a.0, a.1),
        end: Point::new(b.0, b.1),
        width: 0.15,
        fill: "none".into(),
        layer: "F.SilkS".into(),
    };
    let mut graphics = vec![rect((-3.75, -1.5), (3.75, 1.5))];
    match kind {
        "resistor" => graphics.push(rect((-1.6, -0.6), (1.6, 0.6))),
        "led" => graphics.push(line((1.2, -1.0), (1.2, 1.0))),
        "capacitor" => {
            graphics.push(line((-0.4, -1.0), (-0.4, 1.0)));
            graphics.push(line((0.4, -1.0), (0.4, 1.0)));
        }
        _ => graphics.push(line((-1.2, -1.0), (-1.2, 1.0))),
    }
    Some(FootprintDefinition {
        name: kind.to_owned(),
        description: describe(kind),
        keywords: String::new(),
        pads: vec![pad("1", -2.5), pad("2", 2.5)],
        graphics,
        model: None,
        rules: None,
        box_body: None,
        raw_source: String::new(),
        unsupported: Vec::new(),
        retained: Vec::new(),
    })
}
