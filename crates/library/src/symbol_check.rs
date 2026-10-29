//! Read-only library-quality checks on authored symbols.
use crate::SymbolDefinition;
use crate::footprint_check::{FindingSeverity, FootprintFinding, push};
use pcbcraft_geom::Point;

const PIN_GRID_MM: f64 = 2.54;
const PIN_GRID_FINE_MM: f64 = 1.27;

/// Run library-quality checks on a symbol definition.
pub fn check_symbol(def: &SymbolDefinition) -> Vec<FootprintFinding> {
    let mut findings = Vec::new();
    check_pin_grid(def, &mut findings);
    check_pin_types(def, &mut findings);
    check_duplicate_pin_positions(def, &mut findings);
    check_symbol_origin_grid(def, &mut findings);
    check_pins_on_sides(def, &mut findings);
    findings
}

fn check_pin_grid(def: &SymbolDefinition, findings: &mut Vec<FootprintFinding>) {
    if def.pins.is_empty() {
        return;
    }
    let coarse_ok = def
        .pins
        .iter()
        .all(|p| on_grid_point(p.position, PIN_GRID_MM));
    let fine_ok = def
        .pins
        .iter()
        .all(|p| on_grid_point(p.position, PIN_GRID_FINE_MM));
    if coarse_ok || fine_ok {
        return;
    }
    push(
        findings,
        "lib_pin_off_grid",
        FindingSeverity::Error,
        "Symbol pin positions are not on the 2.54 mm or 1.27 mm grid",
        "Snap pin endpoints to the schematic grid before saving the symbol",
    );
}

fn check_pin_types(def: &SymbolDefinition, findings: &mut Vec<FootprintFinding>) {
    for pin in &def.pins {
        if pin.electrical_type.eq_ignore_ascii_case("unspecified") {
            push(
                findings,
                "lib_pin_type_unspecified",
                FindingSeverity::Error,
                "At least one pin still has an unspecified electrical type",
                "Set input, output, passive or power types so ERC can classify the nets",
            );
            return;
        }
    }
}

fn check_duplicate_pin_positions(def: &SymbolDefinition, findings: &mut Vec<FootprintFinding>) {
    for (i, a) in def.pins.iter().enumerate() {
        for b in def.pins.iter().skip(i + 1) {
            if a.unit == b.unit
                && a.body_style == b.body_style
                && points_close(a.position, b.position)
            {
                push(
                    findings,
                    "lib_pin_duplicate_position",
                    FindingSeverity::Error,
                    "Two pins share the same position on the symbol",
                    "Move or merge the overlapping pins",
                );
                return;
            }
        }
    }
}

fn check_symbol_origin_grid(def: &SymbolDefinition, findings: &mut Vec<FootprintFinding>) {
    if def.pins.is_empty() {
        return;
    }
    let origin_touched = def
        .pins
        .iter()
        .any(|p| p.position.x.abs() <= 1e-3 || p.position.y.abs() <= 1e-3);
    if origin_touched {
        return;
    }
    push(
        findings,
        "lib_symbol_origin_off_grid",
        FindingSeverity::Error,
        "No pin sits on the symbol origin axes",
        "Place the symbol so pin rows pass through the origin on the schematic grid",
    );
}

fn check_pins_on_sides(def: &SymbolDefinition, findings: &mut Vec<FootprintFinding>) {
    if def.pins.len() < 4 {
        return;
    }
    let mut top_bottom = 0_u32;
    for pin in &def.pins {
        if pin.position.x.abs() <= pin.position.y.abs() {
            top_bottom += 1;
        }
    }
    if top_bottom == 0 {
        return;
    }
    push(
        findings,
        "lib_symbol_pins_top_bottom",
        FindingSeverity::Warning,
        "Some pins sit on the top or bottom edge instead of the left or right side",
        "For rectangular IC symbols, place pins on the vertical sides",
    );
}

fn on_grid_point(at: Point, step: f64) -> bool {
    on_axis(at.x, step) && on_axis(at.y, step)
}

fn on_axis(value: f64, step: f64) -> bool {
    if !value.is_finite() || !step.is_finite() || step <= 0.0 {
        return false;
    }
    let scaled = value / step;
    (scaled - scaled.round()).abs() <= 1e-3
}

fn points_close(a: Point, b: Point) -> bool {
    a.distance(b) <= 1e-3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SymbolPin;

    fn sample_pin(number: &str, x: f64, y: f64, electrical_type: &str) -> SymbolPin {
        SymbolPin {
            number: number.into(),
            name: number.into(),
            position: Point::new(x, y),
            angle: 0.0,
            length: 2.54,
            electrical_type: electrical_type.into(),
            unit: 1,
            body_style: 1,
            graphical_style: "line".into(),
            visible: true,
        }
    }

    #[test]
    fn unspecified_pin_type_is_reported() {
        let def = SymbolDefinition {
            name: "U".into(),
            reference: "U".into(),
            value: "U".into(),
            description: String::new(),
            keywords: String::new(),
            footprint: String::new(),
            set: String::new(),
            pins: vec![sample_pin("1", 0.0, 2.54, "unspecified")],
            graphics: vec![],
            extends: None,
            raw_source: String::new(),
            unsupported: vec![],
        };
        assert!(
            check_symbol(&def)
                .iter()
                .any(|f| f.code == "lib_pin_type_unspecified")
        );
    }

    #[test]
    fn off_grid_pin_is_reported() {
        let def = SymbolDefinition {
            name: "U".into(),
            reference: "U".into(),
            value: "U".into(),
            description: String::new(),
            keywords: String::new(),
            footprint: String::new(),
            set: String::new(),
            pins: vec![sample_pin("1", 0.1, 0.2, "passive")],
            graphics: vec![],
            extends: None,
            raw_source: String::new(),
            unsupported: vec![],
        };
        assert!(
            check_symbol(&def)
                .iter()
                .any(|f| f.code == "lib_pin_off_grid")
        );
    }
}
