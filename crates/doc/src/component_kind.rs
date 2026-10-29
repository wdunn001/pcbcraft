//! Closed component kind vocabulary for profile and companion rules.

use crate::{Component, Project};
use pcbcraft_library::{FootprintDefinition, SymbolDefinition};

pub const COMPONENT_KIND_SLUGS: &[&str] = &[
    "mcu",
    "ic",
    "ldo",
    "buck",
    "crystal",
    "oscillator",
    "mosfet",
    "diode",
    "tvs",
    "inductor",
    "fuse",
    "relay",
    "motor_driver",
    "opamp",
    "sensor",
    "antenna",
    "test_point",
    "connector_power",
    "common_mode_choke",
    "resistor",
    "capacitor",
    "led",
    "connector",
];

pub const DECOUPLING_EXCLUDED_KINDS: &[&str] = &["resistor", "capacitor", "connector", "led"];

const LEGACY_LIBRARY_KIND: &str = "library";

pub fn kind_matches(kind: &str, expected: &str) -> bool {
    let k = kind.to_ascii_lowercase();
    let e = expected.to_ascii_lowercase();
    if k == e {
        return true;
    }
    matches!(
        (k.as_str(), e.as_str()),
        ("mcu", "ic") | ("ic", "mcu") | ("tvs", "esd") | ("esd", "tvs")
    )
}

pub fn is_valid_component_kind(kind: &str) -> bool {
    !kind.is_empty()
        && COMPONENT_KIND_SLUGS
            .iter()
            .any(|slug| kind_matches(kind, slug))
}

pub fn is_ic_like_kind(kind: &str) -> bool {
    !DECOUPLING_EXCLUDED_KINDS
        .iter()
        .any(|excluded| kind_matches(kind, excluded))
}

pub fn allowed_kinds_message() -> String {
    COMPONENT_KIND_SLUGS.join(", ")
}

pub fn parse_component_kind(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(format!(
            "kind must be 1 to 64 bytes; allowed: {}",
            allowed_kinds_message()
        ));
    }
    let lower = trimmed.to_ascii_lowercase();
    if !is_valid_component_kind(&lower) {
        return Err(format!(
            "unknown component kind \"{}\"; allowed: {}",
            trimmed,
            allowed_kinds_message()
        ));
    }
    Ok(lower)
}

fn infer_for_component(c: &Component) -> Option<String> {
    let symbol = c.library_symbol.as_ref().map(|s| &s.definition)?;
    let footprint = c
        .library_footprint
        .as_ref()
        .map(|f| &f.definition);
    Some(pcbcraft_library::infer_component_kind(
        symbol,
        footprint,
    ))
}

fn coerce_legacy_kind(kind: &str, inferred: Option<&str>) -> String {
    if kind.is_empty() {
        return inferred.unwrap_or("ic").to_string();
    }
    if kind == LEGACY_LIBRARY_KIND {
        return inferred.unwrap_or("ic").to_string();
    }
    if is_valid_component_kind(kind) {
        return kind.to_ascii_lowercase();
    }
    inferred.unwrap_or("ic").to_string()
}

impl Project {
    /// Assign kinds for legacy projects before validation or save.
    pub fn migrate_component_kinds(&mut self) {
        for c in &mut self.components {
            let inferred = infer_for_component(c);
            c.kind = coerce_legacy_kind(&c.kind, inferred.as_deref());
        }
    }
}

pub fn validate_component_kinds(components: &[Component]) -> Result<(), String> {
    for c in components {
        if c.kind.is_empty() {
            return Err(format!(
                "component {} has no kind; allowed: {}",
                c.reference,
                allowed_kinds_message()
            ));
        }
        if !is_valid_component_kind(&c.kind) {
            return Err(format!(
                "component {} has unknown kind \"{}\"; allowed: {}",
                c.reference,
                c.kind,
                allowed_kinds_message()
            ));
        }
    }
    Ok(())
}

pub fn assign_kind_from_library(
    symbol: &SymbolDefinition,
    footprint: Option<&FootprintDefinition>,
) -> String {
    pcbcraft_library::infer_component_kind(symbol, footprint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Component, Point, Project};
    use pcbcraft_library::{SymbolDefinition, SymbolPin};

    fn ldo_symbol() -> SymbolDefinition {
        SymbolDefinition {
            name: "LDO_TEST".into(),
            reference: "U".into(),
            value: "LDO_TEST".into(),
            description: "Low drop-out regulator".into(),
            keywords: "LDO".into(),
            footprint: String::new(),
            set: String::new(),
            pins: vec![
                SymbolPin {
                    number: "1".into(),
                    name: "IN".into(),
                    position: Point::new(-5.0, 0.0),
                    angle: 0.0,
                    length: 2.54,
                    electrical_type: "power_in".into(),
                    unit: 1,
                    body_style: 0,
                    graphical_style: "line".into(),
                    visible: true,
                },
                SymbolPin {
                    number: "2".into(),
                    name: "GND".into(),
                    position: Point::new(0.0, 5.0),
                    angle: 270.0,
                    length: 2.54,
                    electrical_type: "power_in".into(),
                    unit: 1,
                    body_style: 0,
                    graphical_style: "line".into(),
                    visible: true,
                },
                SymbolPin {
                    number: "3".into(),
                    name: "OUT".into(),
                    position: Point::new(5.0, 0.0),
                    angle: 180.0,
                    length: 2.54,
                    electrical_type: "power_out".into(),
                    unit: 1,
                    body_style: 0,
                    graphical_style: "line".into(),
                    visible: true,
                },
            ],
            graphics: vec![],
            extends: None,
            raw_source: String::new(),
            unsupported: vec![],
        }
    }

    #[test]
    fn ldo_keywords_assign_ldo_kind() -> Result<(), String> {
        let definition = ldo_symbol();
        let c = crate::library_component(
            definition,
            "Regulator".into(),
            1,
            "U1".into(),
            Point::default(),
            None,
        )?;
        assert_eq!(c.kind, "ldo");
        Ok(())
    }

    #[test]
    fn legacy_library_kind_migrates_to_ic() {
        let mut p = Project::default();
        p.components.push(Component {
            id: 1,
            reference: "U9".into(),
            value: "MYSTERY".into(),
            kind: "library".into(),
            position: Point::default(),
            rotation: 0.0,
            pins: vec![],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
        });
        p.migrate_component_kinds();
        assert_eq!(p.components[0].kind, "ic");
    }

    #[test]
    fn empty_kind_rejected_on_validate() -> Result<(), String> {
        let mut p = Project::default();
        let mut c = crate::component_template("resistor", 1, "R1".into(), Point::default())
            .ok_or("template")?;
        c.kind.clear();
        p.components.push(c);
        let err = match p.validate() {
            Ok(()) => return Err("empty kind must fail validation".into()),
            Err(e) => e,
        };
        assert!(err.contains("allowed"));
        Ok(())
    }

    #[test]
    fn kind_aliases_match_for_rules() {
        assert!(kind_matches("mcu", "ic"));
        assert!(kind_matches("tvs", "esd"));
    }
}
