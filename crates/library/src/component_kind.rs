//! Map library symbol and footprint metadata to closed component kind slugs.

use crate::{FootprintDefinition, SymbolDefinition};

const KEYWORD_ROWS: &[(&str, &str)] = &[
    ("ldo", "ldo"),
    ("linear regulator", "ldo"),
    ("buck", "buck"),
    ("switching regulator", "buck"),
    ("dc-dc", "buck"),
    ("mcu", "mcu"),
    ("microcontroller", "mcu"),
    ("cpu", "mcu"),
    ("fpga", "ic"),
    ("asic", "ic"),
    ("opamp", "opamp"),
    ("operational amplifier", "opamp"),
    ("comparator", "opamp"),
    ("crystal", "crystal"),
    ("xtal", "crystal"),
    ("oscillator", "oscillator"),
    ("mosfet", "mosfet"),
    ("fet", "mosfet"),
    ("igbt", "mosfet"),
    ("tvs", "tvs"),
    ("esd", "tvs"),
    ("transil", "tvs"),
    ("inductor", "inductor"),
    ("choke", "inductor"),
    ("common mode choke", "common_mode_choke"),
    ("cmc", "common_mode_choke"),
    ("fuse", "fuse"),
    ("relay", "relay"),
    ("motor driver", "motor_driver"),
    ("stepper driver", "motor_driver"),
    ("h-bridge", "motor_driver"),
    ("sensor", "sensor"),
    ("antenna", "antenna"),
    ("test point", "test_point"),
    ("testpoint", "test_point"),
    ("power connector", "connector_power"),
    ("barrel jack", "connector_power"),
    ("resistor", "resistor"),
    ("capacitor", "capacitor"),
    ("cap ", "capacitor"),
    ("led", "led"),
    ("diode", "diode"),
    ("connector", "connector"),
];

const SET_ROWS: &[(&str, &str)] = &[
    ("regulator_ldo", "ldo"),
    ("regulator_switching", "buck"),
    ("mcu", "mcu"),
    ("microcontroller", "mcu"),
    ("crystal", "crystal"),
    ("oscillator", "oscillator"),
    ("connector_power", "connector_power"),
];

fn searchable(symbol: &SymbolDefinition, footprint: Option<&FootprintDefinition>) -> String {
    let mut parts = vec![
        symbol.keywords.as_str(),
        symbol.description.as_str(),
        symbol.set.as_str(),
        symbol.name.as_str(),
        symbol.reference.as_str(),
    ];
    if let Some(fp) = footprint {
        parts.push(fp.keywords.as_str());
        parts.push(fp.description.as_str());
        parts.push(fp.name.as_str());
    }
    parts.join(" ").to_ascii_lowercase()
}

fn match_rows(haystack: &str, rows: &[(&str, &str)]) -> Option<String> {
    rows.iter()
        .find(|(needle, _)| haystack.contains(needle))
        .map(|(_, kind)| (*kind).to_string())
}

fn passive_kind_from_pins(symbol: &SymbolDefinition) -> Option<&'static str> {
    let pin_count = symbol
        .pins
        .iter()
        .filter(|p| p.body_style <= 1 && p.visible)
        .map(|p| p.number.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if pin_count > 2 {
        return None;
    }
    let text = searchable(symbol, None);
    if text.contains("led") {
        return Some("led");
    }
    if text.contains("cap") || text.contains("capacitor") {
        return Some("capacitor");
    }
    if text.contains("res") || text.contains("resistor") {
        return Some("resistor");
    }
    if text.contains("diode") {
        return Some("diode");
    }
    if text.contains("inductor") || text.contains("choke") {
        return Some("inductor");
    }
    if text.contains("fuse") {
        return Some("fuse");
    }
    None
}

/// Infer a component kind slug from symbol metadata and optional footprint metadata.
pub fn infer_component_kind(
    symbol: &SymbolDefinition,
    footprint: Option<&FootprintDefinition>,
) -> String {
    let haystack = searchable(symbol, footprint);
    if let Some(kind) = match_rows(&haystack, SET_ROWS) {
        return kind;
    }
    if let Some(kind) = match_rows(&haystack, KEYWORD_ROWS) {
        return kind;
    }
    if let Some(kind) = passive_kind_from_pins(symbol) {
        return kind.into();
    }
    let pin_count = symbol
        .pins
        .iter()
        .filter(|p| p.body_style <= 1 && p.visible)
        .map(|p| p.number.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if pin_count > 2 {
        "ic".into()
    } else {
        "connector".into()
    }
}
