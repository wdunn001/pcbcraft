use std::collections::BTreeMap;

use super::review::{ReviewDecision, ReviewedField};
use super::types::{DatasheetExtractBundle, ExtractedPinRow};
use crate::SymbolDefinition;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartFieldPatch {
    pub pin_names: BTreeMap<String, String>,
    pub properties: BTreeMap<String, String>,
}

pub fn build_part_patch(bundle: &DatasheetExtractBundle) -> Result<PartFieldPatch, String> {
    let mut patch = PartFieldPatch::default();
    for row in &bundle.pin_table {
        if let Some(name) = effective_pin_name(row) {
            patch.pin_names.insert(row.value.pin_number.clone(), name);
        }
    }
    for row in &bundle.ordering_codes {
        if let Some(code) = effective_ordering(row) {
            patch
                .properties
                .insert("Ordering code".into(), code);
        }
    }
    Ok(patch)
}

pub fn apply_patch_to_symbol(symbol: &mut SymbolDefinition, patch: &PartFieldPatch) {
    for pin in &mut symbol.pins {
        if let Some(name) = patch.pin_names.get(&pin.number) {
            pin.name.clone_from(name);
        }
    }
}

fn effective_pin_name(row: &ReviewedField<ExtractedPinRow>) -> Option<String> {
    match row.decision {
        ReviewDecision::Rejected | ReviewDecision::Pending => None,
        ReviewDecision::Accepted => Some(row.value.pin_name.clone()),
        ReviewDecision::Edited => row.edited.as_ref().map(|r| r.pin_name.clone()),
    }
}

fn effective_ordering(
    row: &ReviewedField<super::types::ExtractedOrderingCode>,
) -> Option<String> {
    match row.decision {
        ReviewDecision::Rejected | ReviewDecision::Pending => None,
        ReviewDecision::Accepted => Some(row.value.code.clone()),
        ReviewDecision::Edited => row.edited.as_ref().map(|r| r.code.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datasheet_extract::provenance::{DatasheetRect, DatasheetSourceRecord};
    use crate::datasheet_extract::review::ReviewedField;
    use crate::datasheet_extract::types::{DatasheetExtractBundle, ExtractedPinRow};
    use crate::SymbolPin;
    use pcbcraft_geom::Point;

    fn sample_row(
        decision: ReviewDecision,
        name: &str,
    ) -> Result<ReviewedField<ExtractedPinRow>, String> {
        let source = DatasheetSourceRecord::new(
            0,
            1,
            DatasheetRect {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            },
            "1 OUT",
            None,
        )?;
        let mut field = ReviewedField::new(ExtractedPinRow {
            pin_number: "1".into(),
            pin_name: name.into(),
            source,
        });
        field.decision = decision;
        Ok(field)
    }

    #[test]
    fn rejected_pins_never_reach_the_part() -> Result<(), String> {
        let mut bundle = DatasheetExtractBundle::default();
        bundle
            .pin_table
            .push(sample_row(ReviewDecision::Rejected, "BAD")?);
        let patch = build_part_patch(&bundle)?;
        assert!(patch.pin_names.is_empty());
        Ok(())
    }

    #[test]
    fn accepted_pin_renames_apply() -> Result<(), String> {
        let mut bundle = DatasheetExtractBundle::default();
        bundle
            .pin_table
            .push(sample_row(ReviewDecision::Accepted, "VOUT")?);
        let patch = build_part_patch(&bundle)?;
        let mut symbol = SymbolDefinition {
            name: "U".into(),
            reference: "U".into(),
            value: "U".into(),
            description: String::new(),
            keywords: String::new(),
            footprint: String::new(),
            set: String::new(),
            pins: vec![SymbolPin {
                number: "1".into(),
                name: "~".into(),
                position: Point::ZERO,
                angle: 0.0,
                length: 2.54,
                electrical_type: "passive".into(),
                unit: 1,
                body_style: 1,
                graphical_style: "line".into(),
                visible: true,
            }],
            graphics: Vec::new(),
            extends: None,
            raw_source: String::new(),
            unsupported: Vec::new(),
        };
        apply_patch_to_symbol(&mut symbol, &patch);
        assert_eq!(symbol.pins[0].name, "VOUT");
        Ok(())
    }
}
