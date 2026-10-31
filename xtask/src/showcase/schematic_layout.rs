//! Schematic layout audit: part overlaps and frame overruns.
//!
//! This file validates that the showcase schematic design has no part
//! overlaps and that every part sits within the sheet frame. It mirrors
//! the canvas drawing code's understanding of symbol sizes to compute
//! bounding boxes for each part.
//!
//! # Rules
//! - Coordinates are millimetres on a 2.54 grid.
//! - Bounding boxes include the symbol body, pins, and label areas.
//! - The frame is the inner drawing area (sheet minus the band).
//!
//! # Dependencies
//! - xtask/src/showcase/design.rs
//! - xtask/src/showcase/parts.rs
//! - crates/ui-egui/src/canvas/sheet.rs (frame constants mirrored below)
//!
//! # Do not
//! - Assume symbol sizes match the library file; use these constants.

use pcbcraft_geom::Point;

/// Millimetres between symbol pin grid points.
const GRID: f64 = 2.54;

/// Width of the margin band that carries the zone letters and numbers.
const BAND: f64 = 12.0;

/// A4 sheet dimensions in millimetres (height, width).
const A4_H: f64 = 297.0;
const A4_W: f64 = 210.0;

/// Bounding box in symbol local coordinates.
#[derive(Debug, Clone, Copy)]
pub struct BBox {
    pub lo: Point,
    pub hi: Point,
}

impl BBox {
    pub fn new(lo_x: f64, lo_y: f64, hi_x: f64, hi_y: f64) -> Self {
        Self {
            lo: Point::new(lo_x, lo_y),
            hi: Point::new(hi_x, hi_y),
        }
    }

    /// Transform the box to world coordinates given a placement.
    pub fn at(&self, pos: Point, rotation: f64) -> Self {
        let lo = pos.offset(self.lo.rotate(-rotation));
        let hi = pos.offset(self.hi.rotate(-rotation));
        let x_lo = lo.x.min(hi.x);
        let x_hi = lo.x.max(hi.x);
        let y_lo = lo.y.min(hi.y);
        let y_hi = lo.y.max(hi.y);
        BBox::new(x_lo, y_lo, x_hi, y_hi)
    }

    /// Check if this box overlaps another.
    pub fn overlaps(&self, other: &BBox) -> bool {
        self.lo.x < other.hi.x
            && self.hi.x > other.lo.x
            && self.lo.y < other.hi.y
            && self.hi.y > other.lo.y
    }

    /// Check if this box is entirely within another box.
    pub fn inside(&self, other: &BBox) -> bool {
        self.lo.x >= other.lo.x
            && self.hi.x <= other.hi.x
            && self.lo.y >= other.lo.y
            && self.hi.y <= other.hi.y
    }
}

/// Bounding box for a resistor symbol.
pub fn resistor_bbox() -> BBox {
    // Body: 5.08mm wide, 2.032mm tall. Pins at ±5.08mm.
    // Include label area: reference above, value below.
    BBox::new(-5.08 - GRID, -4.0, 5.08 + GRID, 4.0)
}

/// Bounding box for a capacitor symbol.
pub fn capacitor_bbox() -> BBox {
    // Body: 2.54mm wide, 4.064mm tall. Pins at ±3.81mm.
    // Include label area.
    BBox::new(-3.81 - GRID, -5.0, 3.81 + GRID, 5.0)
}

/// Bounding box for a diode symbol.
pub fn diode_bbox() -> BBox {
    // Body: 2.54mm wide, 3.048mm tall. Pins at ±3.81mm.
    // Include label area.
    BBox::new(-3.81 - GRID, -5.0, 3.81 + GRID, 5.0)
}

/// Bounding box for a fuse symbol.
pub fn fuse_bbox() -> BBox {
    // Same as resistor.
    resistor_bbox()
}

/// Bounding box for a switch symbol.
pub fn switch_bbox() -> BBox {
    // Similar to resistor with blade.
    BBox::new(-5.08 - GRID, -4.0, 5.08 + GRID, 4.0)
}

/// Bounding box for a crystal symbol.
pub fn crystal_bbox() -> BBox {
    // Body: 5.08mm wide, 3.048mm tall. Pins at ±5.08mm.
    BBox::new(-5.08 - GRID, -4.0, 5.08 + GRID, 4.0)
}

/// Bounding box for a test point symbol.
pub fn test_point_bbox() -> BBox {
    // Small pad with one pin.
    BBox::new(-2.54, -2.54, 2.54, 2.54)
}

/// Bounding box for a boxed IC symbol.
pub fn boxed_bbox(width: f64, pin_count: usize) -> BBox {
    let rows = pin_count.max(1);
    let half_rows = (rows - 1) as f64 / 2.0;
    let top_y = -(half_rows.ceil()) * GRID;
    let bottom_y = top_y + (rows - 1) as f64 * GRID;
    let hw = width / 2.0;
    // Include pin extension and label area.
    BBox::new(-hw - GRID, top_y - 4.0, hw + GRID, bottom_y + 4.0)
}

/// Inner frame bounds for an A4 sheet.
pub fn a4_frame() -> BBox {
    BBox::new(BAND, BAND, A4_W - BAND, A4_H - BAND)
}

/// Result of the schematic layout audit.
pub struct AuditResult {
    pub overlaps: Vec<(String, String)>,
    pub frame_overruns: Vec<String>,
}

impl AuditResult {
    pub fn is_clean(&self) -> bool {
        self.overlaps.is_empty() && self.frame_overruns.is_empty()
    }

    pub fn report(&self) -> String {
        let mut lines = Vec::new();
        if !self.overlaps.is_empty() {
            lines.push("Overlaps:");
            for (a, b) in &self.overlaps {
                lines.push(format!("  {a} overlaps {b}"));
            }
        }
        if !self.frame_overruns.is_empty() {
            lines.push("Frame overruns:");
            for ref in &self.frame_overruns {
                lines.push(format!("  {ref} is outside the frame"));
            }
        }
        if self.is_clean() {
            lines.push("Clean: no overlaps, no frame overruns.");
        }
        lines.join("\n")
    }
}

/// Audit the schematic layout for overlaps and frame overruns.
///
/// Takes a list of parts with their reference, symbol type, position, and rotation.
/// Returns an audit result with any issues found.
pub fn audit(parts: &[PartPlacement]) -> AuditResult {
    let mut overlaps = Vec::new();
    let mut frame_overruns = Vec::new();

    let frame = a4_frame();

    // Compute world bounding boxes for all parts.
    let bboxes: Vec<(String, BBox)> = parts
        .iter()
        .map(|p| {
            let local = part_bbox(p);
            let world = local.at(p.position, p.rotation);
            (p.reference.to_string(), world)
        })
        .collect();

    // Check for overlaps.
    for i in 0..bboxes.len() {
        for j in (i + 1)..bboxes.len() {
            if bboxes[i].1.overlaps(&bboxes[j].1) {
                overlaps.push((bboxes[i].0.clone(), bboxes[j].0.clone()));
            }
        }
    }

    // Check for frame overruns.
    for (ref, bbox) in &bboxes {
        if !bbox.inside(&frame) {
            frame_overruns.push(ref.clone());
        }
    }

    AuditResult {
        overlaps,
        frame_overruns,
    }
}

/// Part placement for the audit.
#[derive(Debug)]
pub struct PartPlacement {
    pub reference: &'static str,
    pub symbol: &'static str,
    pub position: Point,
    pub rotation: f64,
}

/// Compute the local bounding box for a part based on its symbol type.
fn part_bbox(p: &PartPlacement) -> BBox {
    match p.symbol {
        "R_0402" | "R_0603" => resistor_bbox(),
        "C_0402" | "C_0603" => capacitor_bbox(),
        "LED_0603" | "TVS_SOD323" => diode_bbox(),
        "FUSE_1206" => fuse_bbox(),
        "SW_TACT" => switch_bbox(),
        "XTAL_3225" => crystal_bbox(),
        "TP_PAD" => test_point_bbox(),
        "USB_C_PWR" => boxed_bbox(12.7, 12),
        "CHG_SOT23_5" => boxed_bbox(15.24, 5),
        "LDO_SOT23_5" => boxed_bbox(12.7, 5),
        "SWITCH_SOT23_5" => switch_bbox(),
        "MCU_QFN32" => boxed_bbox(30.48, 32),
        "FLASH_SOIC8" => boxed_bbox(12.7, 8),
        "ENV_DFN8" => boxed_bbox(10.16, 8),
        "LIGHT_SOT23_6" => boxed_bbox(10.16, 6),
        _ => BBox::new(-5.0, -5.0, 5.0, 5.0),
    }
}

/// Audit the full showcase design using the design data directly.
pub fn audit_all(parts: &[crate::showcase::design::Part]) -> Result<(), String> {
    let placements: Vec<PartPlacement> = parts
        .iter()
        .map(|p| PartPlacement {
            reference: p.reference,
            symbol: p.footprint,
            position: Point::new(p.sch.0, p.sch.1),
            rotation: p.sch_rot,
        })
        .collect();
    let result = audit(&placements);
    if !result.is_clean() {
        return Err(result.report());
    }
    Ok(())
}

/// Label position and extent for overlap checking.
#[derive(Debug)]
struct Label {
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Label {
    fn overlaps(&self, other: &Label) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

/// Estimate label width in mm based on character count.
/// Schematic uses monospace 15pt for reference, 12pt for value.
fn label_width(text: &str, is_reference: bool) -> f64 {
    let char_width = if is_reference { 8.0 } else { 6.0 };
    (text.chars().count() as f64) * char_width / 10.0
}

/// Audit schematic labels for overlaps.
pub fn audit_labels(parts: &[crate::showcase::design::Part]) -> Result<(), String> {
    let mut labels: Vec<Label> = Vec::new();

    for p in parts {
        let pos = Point::new(p.sch.0, p.sch.1);
        // Reference label: above the component
        let ref_width = label_width(p.reference, true);
        labels.push(Label {
            name: format!("{} reference", p.reference),
            x: pos.x - ref_width / 2.0,
            y: pos.y - 8.0,
            width: ref_width,
            height: 4.0,
        });
        // Value label: below the component
        let val_width = label_width(p.value, false);
        labels.push(Label {
            name: format!("{} value", p.reference),
            x: pos.x - val_width / 2.0,
            y: pos.y + 6.0,
            width: val_width,
            height: 3.0,
        });
    }

    let mut problems = Vec::new();
    for i in 0..labels.len() {
        for j in (i + 1)..labels.len() {
            if labels[i].overlaps(&labels[j]) {
                problems.push(format!(
                    "{} overlaps {}",
                    labels[i].name, labels[j].name
                ));
            }
        }
    }

    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    Ok(())
}