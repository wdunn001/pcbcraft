//! Compare project-library parts for alternates UI (#719).
//!
//! Returns typed symbol fields and pin lists so callers can highlight differences.

use crate::{FootprintDefinition, SymbolDefinition};
use serde::{Deserialize, Serialize};

/// Most other parts in one compare call (baseline plus two others is three parts).
pub const MAX_PART_COMPARE_OTHERS: usize = 2;

/// One row in a comparison table.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PartFieldRow {
    pub field: String,
    pub baseline: String,
    pub other: String,
    pub same: bool,
}

/// Pin numbers that differ between two symbols.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PartPinCompare {
    pub shared_pins: usize,
    pub pins_only_in_baseline: Vec<String>,
    pub pins_only_in_other: Vec<String>,
}

/// Footprint pad numbers that differ when both footprints are present.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FootprintPadCompare {
    pub shared_pads: usize,
    pub pads_only_in_baseline: Vec<String>,
    pub pads_only_in_other: Vec<String>,
}

/// Compare one other part against the baseline.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PartCompareEntry {
    pub name: String,
    pub fields: Vec<PartFieldRow>,
    pub pins: PartPinCompare,
    pub baseline_footprint: String,
    pub other_footprint: String,
    pub footprint_fields_same: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footprint_pads: Option<FootprintPadCompare>,
}

fn pin_numbers(symbol: &SymbolDefinition) -> Vec<String> {
    symbol.pins.iter().map(|p| p.number.clone()).collect()
}

fn only_in(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .filter(|n| !right.contains(n))
        .cloned()
        .collect()
}

fn field_row(field: &str, baseline: &str, other: &str) -> PartFieldRow {
    PartFieldRow {
        field: field.into(),
        baseline: baseline.into(),
        other: other.into(),
        same: baseline == other,
    }
}

/// Effective default footprint name for a symbol, with an optional alternate override.
pub fn effective_footprint_name(symbol: &SymbolDefinition, override_name: Option<&str>) -> String {
    override_name
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| symbol.footprint.clone())
}

/// Build typed field rows and pin diff for two library symbols.
pub fn compare_symbol_fields(
    baseline: &SymbolDefinition,
    other: &SymbolDefinition,
) -> (Vec<PartFieldRow>, PartPinCompare) {
    let fields = vec![
        field_row("reference", &baseline.reference, &other.reference),
        field_row("value", &baseline.value, &other.value),
        field_row("description", &baseline.description, &other.description),
        field_row("keywords", &baseline.keywords, &other.keywords),
        field_row("footprint", &baseline.footprint, &other.footprint),
        field_row("set", &baseline.set, &other.set),
    ];
    let pb = pin_numbers(baseline);
    let po = pin_numbers(other);
    let pins = PartPinCompare {
        shared_pins: pb.iter().filter(|n| po.contains(n)).count(),
        pins_only_in_baseline: only_in(&pb, &po),
        pins_only_in_other: only_in(&po, &pb),
    };
    (fields, pins)
}

fn pad_numbers(footprint: &FootprintDefinition) -> Vec<String> {
    footprint.pads.iter().map(|p| p.number.clone()).collect()
}

/// Compare pad numbers on two footprints when both are known.
pub fn compare_footprint_pads(
    baseline: &FootprintDefinition,
    other: &FootprintDefinition,
) -> FootprintPadCompare {
    let pb = pad_numbers(baseline);
    let po = pad_numbers(other);
    FootprintPadCompare {
        shared_pads: pb.iter().filter(|n| po.contains(n)).count(),
        pads_only_in_baseline: only_in(&pb, &po),
        pads_only_in_other: only_in(&po, &pb),
    }
}

/// Compare one other symbol to the baseline, optionally resolving footprints from the library.
pub fn compare_one_part(
    baseline: &SymbolDefinition,
    other: &SymbolDefinition,
    baseline_footprint_override: Option<&str>,
    other_footprint_override: Option<&str>,
    resolve_footprint: &dyn Fn(&str) -> Option<FootprintDefinition>,
) -> PartCompareEntry {
    let (fields, pins) = compare_symbol_fields(baseline, other);
    let baseline_fp = effective_footprint_name(baseline, baseline_footprint_override);
    let other_fp = effective_footprint_name(other, other_footprint_override);
    let footprint_fields_same = baseline_fp == other_fp;
    let footprint_pads = match (resolve_footprint(&baseline_fp), resolve_footprint(&other_fp)) {
        (Some(a), Some(b)) => Some(compare_footprint_pads(&a, &b)),
        _ => None,
    };
    PartCompareEntry {
        name: other.name.clone(),
        fields,
        pins,
        baseline_footprint: baseline_fp,
        other_footprint: other_fp,
        footprint_fields_same,
        footprint_pads,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_geom::Point;

    fn sample(name: &str, value: &str) -> SymbolDefinition {
        SymbolDefinition {
            name: name.into(),
            reference: "R".into(),
            value: value.into(),
            description: String::new(),
            keywords: String::new(),
            footprint: "FP".into(),
            set: String::new(),
            pins: vec![crate::SymbolPin {
                number: "1".into(),
                name: "~".into(),
                position: Point::new(0.0, 0.0),
                angle: 0.0,
                length: 2.54,
                electrical_type: "passive".into(),
                unit: 1,
                body_style: 1,
                graphical_style: "line".into(),
                visible: true,
            }],
            graphics: vec![],
            extends: None,
            raw_source: String::new(),
            unsupported: vec![],
        }
    }

    #[test]
    fn field_rows_mark_differing_values() -> Result<(), String> {
        let a = sample("A", "10k");
        let b = sample("B", "22k");
        let (rows, _) = compare_symbol_fields(&a, &b);
        let value = rows.iter().find(|r| r.field == "value").ok_or("value")?;
        assert!(!value.same);
        let reference = rows.iter().find(|r| r.field == "reference").ok_or("ref")?;
        assert!(reference.same);
        Ok(())
    }

    #[test]
    fn pin_diff_lists_numbers_only_on_one_side() -> Result<(), String> {
        let a = sample("A", "1");
        let mut b = sample("B", "1");
        b.pins.push(crate::SymbolPin {
            number: "2".into(),
            name: "~".into(),
            position: Point::new(0.0, 0.0),
            angle: 0.0,
            length: 2.54,
            electrical_type: "passive".into(),
            unit: 1,
            body_style: 1,
            graphical_style: "line".into(),
            visible: true,
        });
        let (_, pins) = compare_symbol_fields(&a, &b);
        assert_eq!(pins.shared_pins, 1);
        assert!(pins.pins_only_in_other.contains(&"2".into()));
        assert!(pins.pins_only_in_baseline.is_empty());
        Ok(())
    }
}
