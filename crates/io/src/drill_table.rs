//! The drill table: one row per drill span, plating, function and size.
//!
//! A fabricator reads which holes go through which layers, whether they are
//! plated, what they are for, how big they are and how many there are. The
//! rows are computed from the same hits the Excellon files are written from,
//! so the table can never disagree with the files. The drill map sheet and
//! the fabrication readme print the same lines in the same order, and the
//! Manufacture tab shows the rows as data.
//!
//! # Rules
//! - Row order is span (upper layer, then lower), plated before unplated, function (via, component pad, mounting), diameter, then slot length.
//! - Diameters and slot lengths are in millimetres whatever the project unit.
//! - The tolerance comes from the board's recorded drill tolerance and is empty when none is recorded.
//!
//! # Dependencies
//! - `crates/io/src/fab_export.rs` `collect_drill_hits`, `DrillFunction`.
//! - `crates/doc/src/lib.rs` `Project`.
//!
//! # Do not
//! - Count holes anywhere but `collect_drill_hits`.
//! - Count back-drills anywhere but `backdrill::collect_backdrill_hits`; they are unplated rows of function Back-drill and stay out of `collect_drill_hits`.

use crate::fab_export::{DrillFunction, collect_drill_hits};
use pcbcraft_doc::Project;
use std::collections::BTreeMap;

/// How a span relates to the outer layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanKind {
    /// Top to bottom.
    Through,
    /// One end on an outer layer.
    Blind,
    /// Both ends on inner layers.
    Buried,
}

impl SpanKind {
    /// Plain-language name for the table.
    pub fn label(self) -> &'static str {
        match self {
            SpanKind::Through => "Through",
            SpanKind::Blind => "Blind",
            SpanKind::Buried => "Buried",
        }
    }
}

impl DrillFunction {
    /// Plain-language name for the table.
    pub fn label(self) -> &'static str {
        match self {
            DrillFunction::Via => "Via",
            DrillFunction::ComponentPad => "Component pad",
            DrillFunction::Mounting => "Mounting",
            DrillFunction::BackDrill => "Back-drill",
        }
    }
}

/// One row of the drill table.
#[derive(Clone, Debug, PartialEq)]
pub struct DrillRow {
    /// Upper copper layer of the span, counted from 1 at the top.
    pub from_layer: u8,
    /// Lower copper layer of the span, counted from 1 at the top.
    pub to_layer: u8,
    /// Through, blind or buried.
    pub kind: SpanKind,
    /// True for a plated hole.
    pub plated: bool,
    /// What the hole is for.
    pub function: DrillFunction,
    /// Drill diameter in millimetres.
    pub diameter_mm: f64,
    /// Slot length in millimetres beyond the diameter; zero for a round hole.
    pub slot_mm: f64,
    /// How many holes share this row.
    pub count: usize,
    /// Plus or minus tolerance in millimetres, when the board records one.
    pub tolerance_mm: Option<f64>,
}

/// The drill table of a project, in print order.
///
/// # Rules
/// - Validates the project first; an invalid project is an error.
/// - One row per distinct span, plating, function, diameter and slot length.
///
/// # Dependencies
/// - crates/io/src/fab_export.rs `collect_drill_hits`.
///
/// # Do not
/// - Merge rows of different functions that share a size; the fab reads the function.
pub fn drill_table(p: &Project) -> Result<Vec<DrillRow>, String> {
    let hits = collect_drill_hits(p)?;
    let last = p.copper_layer_count().saturating_sub(1);
    let tolerance = p
        .board
        .machining
        .as_ref()
        .and_then(|m| m.drill_tolerance)
        .filter(|t| *t > 0.0);
    let mut rows: BTreeMap<(u8, u8, bool, DrillFunction, u64, u64), usize> = BTreeMap::new();
    for h in &hits {
        *rows
            .entry((h.span.0, h.span.1, !h.plated, h.function, h.d_um, h.span_um))
            .or_default() += 1;
    }
    for h in crate::backdrill::collect_backdrill_hits(p) {
        let (a, b) = h.range();
        *rows
            .entry((a, b, true, DrillFunction::BackDrill, h.d_um, 0))
            .or_default() += 1;
    }
    Ok(rows
        .into_iter()
        .map(
            |((a, b, unplated, function, d_um, slot_um), count)| DrillRow {
                from_layer: a + 1,
                to_layer: b + 1,
                kind: if a == 0 && b >= last {
                    SpanKind::Through
                } else if a == 0 || b >= last {
                    SpanKind::Blind
                } else {
                    SpanKind::Buried
                },
                plated: !unplated,
                function,
                diameter_mm: d_um as f64 / 1000.0,
                slot_mm: slot_um as f64 / 1000.0,
                count,
                tolerance_mm: tolerance,
            },
        )
        .collect())
}

/// The header line printed above the table rows.
pub const DRILL_TABLE_HEADER: &str =
    "SPAN,KIND,PLATED,FUNCTION,DIAMETER_MM,SLOT_MM,COUNT,TOLERANCE_MM";

/// The printed lines for a table, header first. The drill map sheet and the
/// fabrication readme both print exactly these lines.
///
/// # Rules
/// - One line per row, in row order, with commas between fields and no commas inside them.
///
/// # Dependencies
/// - crates/io/src/drill_table.rs `DrillRow`.
///
/// # Do not
/// - Format a row anywhere else; two formats would drift apart.
pub fn drill_table_lines(rows: &[DrillRow]) -> Vec<String> {
    let mut lines = vec![DRILL_TABLE_HEADER.to_string()];
    for r in rows {
        lines.push(format!(
            "L{}-L{},{},{},{},{:.3},{:.3},{},{}",
            r.from_layer,
            r.to_layer,
            r.kind.label(),
            if r.plated { "plated" } else { "unplated" },
            r.function.label(),
            r.diameter_mm,
            r.slot_mm,
            r.count,
            r.tolerance_mm
                .map_or("none".to_string(), |t| format!("{t:.3}")),
        ));
    }
    lines
}
