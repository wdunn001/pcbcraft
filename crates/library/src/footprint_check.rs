//! Read-only library-quality checks on authored footprints.
use crate::footprint_pad_outline::{pad_outline_closed, pad_outlines_overlap};
use crate::{FootprintDefinition, Graphic, Model3D};
use pcbcraft_geom::{Point, point_in_polygon, polygon_edges, segments_cross};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const SILK_PAD_CLEARANCE_MM: f64 = 0.2;
const GRID_MM: f64 = 0.01;
const MM_PER_INCH: f64 = 25.4;
const COURTYARD_LAYER: &str = "pcbcraft.courtyard";
const SILK_LAYER: &str = "F.SilkS";
const FAB_LAYER: &str = "F.Fab";
const REFDES_LAYER: &str = "F.Fab";

/// Severity of a footprint validation finding.
///
/// # Rules
/// - `Error` findings must be fixed before the footprint can be published.
/// - `Warning` findings are advisory and may be accepted by the library owner.
///
/// # Dependencies
/// - crates/library/src/footprint_check.rs None.
///
/// # Do not
/// - Add intermediate severity levels; the two-tier model is intentional.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Error,
    Warning,
}

/// A single validation finding for a footprint.
///
/// # Rules
/// - `code` is a stable machine-readable identifier (e.g. `lib_pad_overlap`).
/// - `severity` indicates how critical the issue is.
/// - `message` describes the problem in human-readable form.
/// - `hint` suggests how to fix the issue.
///
/// # Dependencies
/// - crates/library/src/footprint_check.rs `FindingSeverity`
///
/// # Do not
/// - Change `code` values without updating all callers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FootprintFinding {
    pub code: String,
    pub severity: FindingSeverity,
    pub message: String,
    pub hint: String,
}

/// Run library-quality checks on a footprint definition.
///
/// # Rules
/// - Returns a vector of findings; an empty vector means the footprint passes all checks.
/// - Checks run in a fixed order: pad overlap, duplicate pad numbers, pad sequence,
///   pin 1 marking, courtyard, silkscreen, fabrication, origin, drill, layers, text, model.
/// - Early returns on error findings within each check category.
///
/// # Dependencies
/// - crates/library/src/lib.rs `FootprintDefinition`
/// - crates/library/src/footprint_check.rs `FootprintFinding`
/// - crates/library/src/footprint_check.rs `FindingSeverity`
/// - crates/library/src/footprint_pad_outline.rs `pad_outline_closed`, `pad_outlines_overlap` from `footprint_pad_outline`
///
/// # Do not
/// - Change the order of checks without updating the test suite.
/// - Add checks that mutate the definition; this function is read-only.
pub fn check_footprint(def: &FootprintDefinition) -> Vec<FootprintFinding> {
    let mut findings = Vec::new();
    check_pad_overlap(def, &mut findings);
    check_duplicate_pad_numbers(def, &mut findings);
    check_pad_number_sequence(def, &mut findings);
    check_pin1(def, &mut findings);
    check_pin1_quadrant(def, &mut findings);
    check_courtyard(def, &mut findings);
    check_courtyard_grid(def, &mut findings);
    check_silk_over_pad(def, &mut findings);
    check_silk_under_body(def, &mut findings);
    check_silk_stroke(def, &mut findings);
    check_fab_outline(def, &mut findings);
    check_origin_centre(def, &mut findings);
    check_drill_in_pad(def, &mut findings);
    check_npth_copper(def, &mut findings);
    check_smd_layers(def, &mut findings);
    check_refdes_text(def, &mut findings);
    check_model(def, &mut findings);
    check_ipc_name_hint(def, &mut findings);
    findings
}

pub(crate) fn push(
    findings: &mut Vec<FootprintFinding>,
    code: &str,
    severity: FindingSeverity,
    message: &str,
    hint: &str,
) {
    findings.push(FootprintFinding {
        code: code.to_owned(),
        severity,
        message: message.to_owned(),
        hint: hint.to_owned(),
    });
}

fn check_pad_overlap(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    for i in 0..def.pads.len() {
        for j in (i + 1)..def.pads.len() {
            if pad_outlines_overlap(&def.pads[i], &def.pads[j]) {
                push(
                    findings,
                    "lib_pad_overlap",
                    FindingSeverity::Error,
                    "Two pads share the same copper area",
                    "Move or resize the pads so their outlines no longer overlap",
                );
                return;
            }
        }
    }
}

fn check_duplicate_pad_numbers(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let mut seen = BTreeSet::new();
    for pad in &def.pads {
        if !seen.insert(pad.number.as_str()) {
            push(
                findings,
                "lib_pad_number_duplicate",
                FindingSeverity::Error,
                "More than one pad uses the same number",
                "Give each pad a unique number starting at 1",
            );
            return;
        }
    }
}

fn check_pad_number_sequence(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    if def.pads.is_empty() {
        return;
    }
    let mut numbers = BTreeSet::new();
    for pad in &def.pads {
        let Ok(value) = pad.number.parse::<u32>() else {
            continue;
        };
        if value == 0 {
            push(
                findings,
                "lib_pad_number_missing",
                FindingSeverity::Error,
                "Pad numbers should start at 1, not 0",
                "Renumber the first pad to 1 and count up without gaps",
            );
            return;
        }
        numbers.insert(value);
    }
    if numbers.is_empty() {
        push(
            findings,
            "lib_pad_number_missing",
            FindingSeverity::Error,
            "Pad numbers should include decimal integers starting at 1",
            "Renumber signal pads as 1, 2, 3 without gaps",
        );
        return;
    }
    let expected = numbers.len() as u32;
    for n in 1..=expected {
        if !numbers.contains(&n) {
            push(
                findings,
                "lib_pad_number_missing",
                FindingSeverity::Error,
                "Pad numbers skip a value in the 1..N sequence",
                "Fill in the missing pad number or renumber contiguously from 1",
            );
            return;
        }
    }
}

fn check_pin1(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    if def.pads.iter().any(|p| p.number == "1") && pin1_mark_present(def) {
        return;
    }
    push(
        findings,
        "lib_pin1_missing",
        FindingSeverity::Error,
        "Pin 1 is not marked on silk or with a distinct pad",
        "Add a small silk circle near the top-left corner or chamfer pad 1",
    );
}

fn pin1_mark_present(def: &FootprintDefinition) -> bool {
    for pad in &def.pads {
        if pad.number == "1" && pad.chamfer.is_some() {
            return true;
        }
    }
    for graphic in &def.graphics {
        if matches!(
            graphic,
            Graphic::Circle {
                center,
                radius,
                layer,
                ..
            } if layer == SILK_LAYER
                && *radius > 0.05
                && *radius < 2.0
                && center.x <= 0.0
                && center.y <= 0.0
        ) {
            return true;
        }
    }
    false
}

fn check_pin1_quadrant(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let Some(pad1) = def.pads.iter().find(|p| p.number == "1") else {
        return;
    };
    if pad1.position.x <= 0.05 && pad1.position.y <= 0.05 {
        return;
    }
    push(
        findings,
        "lib_pin1_not_top_left",
        FindingSeverity::Warning,
        "Pad 1 is not in the top-left quadrant of the footprint origin",
        "Move pad 1 toward the top-left corner or rotate the footprint to match zero orientation",
    );
}

fn check_courtyard(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let Some((start, end)) = courtyard_rectangle(def) else {
        push(
            findings,
            "lib_courtyard_missing",
            FindingSeverity::Error,
            "No courtyard rectangle is drawn on the pcbcraft.courtyard layer",
            "Generate or draw a courtyard that encloses all pads and the body outline",
        );
        return;
    };
    let (need_min, need_max) = required_courtyard_bounds(def);
    if need_min.x > need_max.x {
        return;
    }
    let margin = 1e-3;
    if start.x > need_min.x + margin
        || start.y > need_min.y + margin
        || end.x + margin < need_max.x
        || end.y + margin < need_max.y
    {
        push(
            findings,
            "lib_courtyard_too_small",
            FindingSeverity::Error,
            "The courtyard does not surround all pads and artwork with enough margin",
            "Expand the courtyard rectangle or regenerate it with a larger margin",
        );
    }
}

fn check_courtyard_grid(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let Some((start, end)) = courtyard_rectangle(def) else {
        return;
    };
    for coord in [start.x, start.y, end.x, end.y] {
        if !on_grid(coord, GRID_MM) {
            push(
                findings,
                "lib_courtyard_off_grid",
                FindingSeverity::Error,
                "Courtyard corners are not on the 0.01 mm grid",
                "Snap the courtyard rectangle to a 0.01 mm grid",
            );
            return;
        }
    }
}

/// Half width and half height of the courtyard rectangle in millimetres, when one is drawn.
///
/// # Rules
/// - Uses the `pcbcraft.courtyard` rectangle when present.
///
/// # Dependencies
/// - crates/library/src/footprint_check.rs `courtyard_rectangle`
///
/// # Do not
/// - Include silk or fab bounds; courtyard layer only.
pub fn courtyard_half_extents(def: &FootprintDefinition) -> Option<(f64, f64)> {
    let (min, max) = courtyard_rectangle(def)?;
    Some((
        min.x.abs().max(max.x.abs()),
        min.y.abs().max(max.y.abs()),
    ))
}

fn courtyard_rectangle(def: &FootprintDefinition) -> Option<(Point, Point)> {
    for graphic in &def.graphics {
        #[allow(clippy::collapsible_if)]
        if let Graphic::Rectangle {
            start, end, layer, ..
        } = graphic
        {
            if layer == COURTYARD_LAYER {
                let min = Point::new(start.x.min(end.x), start.y.min(end.y));
                let max = Point::new(start.x.max(end.x), start.y.max(end.y));
                return Some((min, max));
            }
        }
    }
    None
}

fn required_courtyard_bounds(def: &FootprintDefinition) -> (Point, Point) {
    let mut min = Point::new(f64::MAX, f64::MAX);
    let mut max = Point::new(f64::MIN, f64::MIN);
    let mut grow_point = |at: Point| {
        min.x = min.x.min(at.x);
        min.y = min.y.min(at.y);
        max.x = max.x.max(at.x);
        max.y = max.y.max(at.y);
    };
    for pad in &def.pads {
        for p in pad_outline_closed(pad) {
            grow_point(p);
        }
    }
    for graphic in &def.graphics {
        if matches!(
            graphic,
            Graphic::Rectangle { layer, .. } if layer == COURTYARD_LAYER
        ) {
            continue;
        }
        for p in graphic_path_sample(graphic) {
            grow_point(p);
        }
    }
    let excess = 0.25_f64;
    (
        Point::new(min.x - excess, min.y - excess),
        Point::new(max.x + excess, max.y + excess),
    )
}

fn check_silk_over_pad(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let silk_segments: Vec<(Point, Point)> = def.graphics.iter().flat_map(silk_segments).collect();
    if silk_segments.is_empty() {
        return;
    }
    for pad in &def.pads {
        let outline = pad_outline_closed(pad);
        let ring = open_ring(&outline);
        if ring.len() < 3 {
            continue;
        }
        for (a, b) in &silk_segments {
            if segment_near_polygon(*a, *b, &ring, SILK_PAD_CLEARANCE_MM) {
                push(
                    findings,
                    "lib_silk_over_pad",
                    FindingSeverity::Error,
                    "Silkscreen artwork passes within 0.2 mm of a pad opening",
                    "Move silk lines away from pads or shrink the body outline",
                );
                return;
            }
        }
    }
}

fn silk_segments(graphic: &Graphic) -> Vec<(Point, Point)> {
    match graphic {
        Graphic::Line {
            start, end, layer, ..
        } if layer == SILK_LAYER => vec![(*start, *end)],
        Graphic::Rectangle {
            start, end, layer, ..
        } if layer == SILK_LAYER => {
            let min = Point::new(start.x.min(end.x), start.y.min(end.y));
            let max = Point::new(start.x.max(end.x), start.y.max(end.y));
            vec![
                (Point::new(min.x, min.y), Point::new(max.x, min.y)),
                (Point::new(max.x, min.y), Point::new(max.x, max.y)),
                (Point::new(max.x, max.y), Point::new(min.x, max.y)),
                (Point::new(min.x, max.y), Point::new(min.x, min.y)),
            ]
        }
        Graphic::Polyline { points, layer, .. } if layer == SILK_LAYER && points.len() >= 2 => {
            points.windows(2).map(|w| (w[0], w[1])).collect()
        }
        Graphic::Arc {
            start,
            mid,
            end,
            layer,
            ..
        } if layer == SILK_LAYER => match pcbcraft_geom::arc_through(*start, *mid, *end) {
            Some(arc) => (0..=8)
                .map(|i| arc.point_at(i as f64 / 8.0))
                .collect::<Vec<_>>()
                .windows(2)
                .map(|w| (w[0], w[1]))
                .collect(),
            None => vec![],
        },
        _ => vec![],
    }
}

fn segment_near_polygon(a: Point, b: Point, ring: &[Point], clearance: f64) -> bool {
    for p in ring {
        if point_to_segment_distance(*p, a, b) < clearance {
            return true;
        }
    }
    for (c, d) in polygon_edges(ring) {
        if segment_segment_distance(a, b, c, d) < clearance {
            return true;
        }
        if segments_cross(a, b, c, d) {
            return true;
        }
    }
    let mid = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    point_in_polygon(mid, ring)
}

fn point_to_segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-18 {
        return p.distance(a);
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let proj = Point::new(a.x + t * dx, a.y + t * dy);
    p.distance(proj)
}

fn segment_segment_distance(a: Point, b: Point, c: Point, d: Point) -> f64 {
    [
        point_to_segment_distance(a, c, d),
        point_to_segment_distance(b, c, d),
        point_to_segment_distance(c, a, b),
        point_to_segment_distance(d, a, b),
    ]
    .into_iter()
    .fold(f64::MAX, f64::min)
}

fn check_silk_under_body(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    if def.pads.is_empty() || def.pads.iter().any(|p| p.pad_type != "smd") {
        return;
    }
    let Some((body_min, body_max)) = fab_rectangle(def) else {
        return;
    };
    let inset = 0.05;
    let inner_min = Point::new(body_min.x + inset, body_min.y + inset);
    let inner_max = Point::new(body_max.x - inset, body_max.y - inset);
    if inner_min.x >= inner_max.x || inner_min.y >= inner_max.y {
        return;
    }
    for (a, b) in def.graphics.iter().flat_map(silk_segments) {
        if point_in_rect(a, inner_min, inner_max) && point_in_rect(b, inner_min, inner_max) {
            push(
                findings,
                "lib_silk_under_body",
                FindingSeverity::Error,
                "Silkscreen runs under the package body on a surface-mount footprint",
                "Keep silk outside the fabrication body outline or move it to a keep-out layer",
            );
            return;
        }
    }
}

fn check_silk_stroke(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    for graphic in &def.graphics {
        let width = match graphic {
            Graphic::Line { width, layer, .. }
            | Graphic::Rectangle { width, layer, .. }
            | Graphic::Circle { width, layer, .. }
            | Graphic::Arc { width, layer, .. }
            | Graphic::Polyline { width, layer, .. } => {
                if layer == SILK_LAYER {
                    Some(*width)
                } else {
                    None
                }
            }
            _ => None,
        };
        if width.is_some_and(|w| w > 0.0 && w < 0.10) {
            push(
                findings,
                "lib_silk_too_thin",
                FindingSeverity::Error,
                "Silkscreen stroke is thinner than 0.10 mm",
                "Use at least 0.10 mm silk width for readable fabrication output",
            );
            return;
        }
    }
}

fn check_origin_centre(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    if def.pads.is_empty() || def.pads.iter().any(|p| p.pad_type != "smd") {
        return;
    }
    let (min, max) = pad_bounds(def);
    if min.x > max.x {
        return;
    }
    let centre = Point::new((min.x + max.x) / 2.0, (min.y + max.y) / 2.0);
    if centre.x.abs() <= 0.05 && centre.y.abs() <= 0.05 {
        return;
    }
    push(
        findings,
        "lib_origin_not_centre",
        FindingSeverity::Error,
        "The footprint origin is not at the body centre used for pick and place",
        "Move pads and artwork so the package centre sits on the origin",
    );
}

fn check_drill_in_pad(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    for pad in &def.pads {
        let Some(drill) = &pad.drill else {
            continue;
        };
        let hole = Point::new(
            pad.position.x + drill.offset.x,
            pad.position.y + drill.offset.y,
        );
        let outline = pad_outline_closed(pad);
        if outline.len() >= 3 && point_in_polygon(hole, &outline) {
            continue;
        }
        push(
            findings,
            "lib_drill_outside_pad",
            FindingSeverity::Error,
            "A drill centre lies outside its pad copper",
            "Centre the drill within the pad outline or enlarge the pad",
        );
        return;
    }
}

fn check_npth_copper(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    for pad in &def.pads {
        if pad.pad_type == "np_thru_hole" && pad.layers.iter().any(|l| l.contains("Cu")) {
            push(
                findings,
                "lib_npth_has_copper",
                FindingSeverity::Error,
                "A non-plated hole still lists copper layers",
                "Remove copper layers from np_thru_hole pads",
            );
            return;
        }
    }
}

fn check_smd_layers(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    for pad in &def.pads {
        if pad.pad_type != "smd" {
            continue;
        }
        let has_mask = pad.layers.iter().any(|l| l.contains("Mask"));
        let has_paste = pad.layers.iter().any(|l| l.contains("Paste"));
        if has_mask && has_paste {
            continue;
        }
        push(
            findings,
            "lib_smd_pad_missing_mask_or_paste",
            FindingSeverity::Error,
            "A surface-mount pad is missing solder mask or paste layers",
            "Add F.Mask and F.Paste (or B-side equivalents) to every SMD pad",
        );
        return;
    }
}

fn check_refdes_text(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    for graphic in &def.graphics {
        if let Graphic::Text {
            size, layer, text, ..
        } = graphic
        {
            if !matches!(layer.as_str(), REFDES_LAYER | SILK_LAYER) {
                continue;
            }
            if text.contains("REF") || text.contains('*') {
                let height = size.y.max(size.x);
                if height > 0.0 && height < 1.0 {
                    push(
                        findings,
                        "lib_refdes_text_small",
                        FindingSeverity::Error,
                        "Reference designator text is shorter than 1.0 mm",
                        "Increase reference text height to at least 1.0 mm for assembly legibility",
                    );
                    return;
                }
            }
        }
    }
}

fn check_model(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let smd = !def.pads.is_empty() && def.pads.iter().all(|p| p.pad_type == "smd");
    if smd && def.model.is_none() {
        push(
            findings,
            "lib_model_missing",
            FindingSeverity::Warning,
            "No 3D model is attached to this surface-mount footprint",
            "Attach a STEP or WRL model aligned to the footprint origin and pin 1",
        );
    }
    let Some(model) = &def.model else {
        return;
    };
    if model_offset_looks_like_inches(model) {
        push(
            findings,
            "lib_model_origin_mismatch",
            FindingSeverity::Error,
            "The 3D model offset looks like inch values stored as millimetres",
            "Use the offset form in millimetres or the legacy at form in inches, not both mixed up",
        );
    }
}

fn check_ipc_name_hint(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    if ipc_land_pattern_name(&def.name) {
        return;
    }
    push(
        findings,
        "lib_name_not_ipc",
        FindingSeverity::Warning,
        "The footprint name does not follow the IPC land-pattern pattern with an M, N or L density suffix",
        "Rename to the family pattern with a Most, Nominal or Least suffix, for example a chip name ending in N",
    );
}

fn check_fab_outline(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let has_fab = def.graphics.iter().any(|g| match g {
        Graphic::Rectangle { layer, .. } | Graphic::Polyline { layer, .. } => layer == FAB_LAYER,
        _ => false,
    });
    if !has_fab {
        push(
            findings,
            "lib_fab_outline_missing",
            FindingSeverity::Error,
            "No fabrication outline is drawn on F.Fab",
            "Add a rectangle on F.Fab that matches the package body",
        );
    }
}

fn graphic_path_sample(graphic: &Graphic) -> Vec<Point> {
    match graphic {
        Graphic::Line { start, end, .. } => vec![*start, *end],
        Graphic::Rectangle { start, end, .. } => vec![*start, *end],
        Graphic::Circle { center, .. } => vec![*center],
        Graphic::Arc {
            start, mid, end, ..
        } => vec![*start, *mid, *end],
        Graphic::Polyline { points, .. } => points.clone(),
        Graphic::Text { position, .. } => vec![*position],
    }
}

#[allow(clippy::collapsible_if)]
fn open_ring(closed: &[Point]) -> Vec<Point> {
    if closed.len() < 2 {
        return closed.to_vec();
    }
    let mut ring = closed.to_vec();
    if ring.len() > 1 {
        if let (Some(first), Some(last)) = (ring.first(), ring.last()) {
            if first.distance(*last) < 1e-9 {
                ring.pop();
            }
        }
    }
    ring
}

fn on_grid(value: f64, step: f64) -> bool {
    if !value.is_finite() || !step.is_finite() || step <= 0.0 {
        return false;
    }
    let scaled = value / step;
    (scaled - scaled.round()).abs() <= 1e-3
}

fn fab_rectangle(def: &FootprintDefinition) -> Option<(Point, Point)> {
    for graphic in &def.graphics {
        if let Graphic::Rectangle {
            start, end, layer, ..
        } = graphic
            && layer == FAB_LAYER
        {
            let min = Point::new(start.x.min(end.x), start.y.min(end.y));
            let max = Point::new(start.x.max(end.x), start.y.max(end.y));
            return Some((min, max));
        }
    }
    None
}

fn point_in_rect(p: Point, min: Point, max: Point) -> bool {
    p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y
}

fn pad_bounds(def: &FootprintDefinition) -> (Point, Point) {
    let mut min = Point::new(f64::MAX, f64::MAX);
    let mut max = Point::new(f64::MIN, f64::MIN);
    for pad in &def.pads {
        for p in pad_outline_closed(pad) {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
    }
    (min, max)
}

fn model_offset_looks_like_inches(model: &Model3D) -> bool {
    if !model.raw_source.contains("(offset") {
        return false;
    }
    model.offset.iter().any(|component| {
        if component.abs() < MM_PER_INCH - 0.01 {
            return false;
        }
        let inches = component / MM_PER_INCH;
        (inches - inches.round()).abs() < 0.02 && inches.abs() >= 1.0
    })
}

fn ipc_land_pattern_name(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.len() < 2 {
        return false;
    }
    let Some(last) = trimmed.chars().last() else {
        return false;
    };
    matches!(last, 'M' | 'N' | 'L')
        && trimmed
            .chars()
            .take(trimmed.len().saturating_sub(1))
            .any(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Drill, FootprintPad};

    fn minimal_smd_pad() -> FootprintPad {
        FootprintPad {
            number: "1".into(),
            pad_type: "smd".into(),
            shape: "rect".into(),
            position: Point::new(-1.0, 0.0),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: vec!["F.Cu".into(), "F.Mask".into(), "F.Paste".into()],
            drill: None,
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        }
    }

    #[test]
    fn ipc_name_hint_flags_generic_names() {
        let def = FootprintDefinition {
            name: "MyFootprint".into(),
            description: String::new(),
            keywords: String::new(),
            pads: vec![],
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
            retained: vec![],
        };
        let codes: Vec<_> = check_footprint(&def).into_iter().map(|f| f.code).collect();
        assert!(codes.iter().any(|c| c == "lib_name_not_ipc"));
    }

    #[test]
    fn model_offset_inches_in_mm_form_is_flagged() {
        let def = FootprintDefinition {
            name: "RESC2012X70N".into(),
            description: String::new(),
            keywords: String::new(),
            pads: vec![minimal_smd_pad()],
            graphics: vec![],
            model: Some(Model3D {
                path: "pkg.step".into(),
                offset: [25.4, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                rotate: [0.0, 0.0, 0.0],
                cad_part: None,
                raw_source: "(offset (xyz 25.4 0 0))".into(),
            }),
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
            retained: vec![],
        };
        assert!(
            check_footprint(&def)
                .iter()
                .any(|f| f.code == "lib_model_origin_mismatch")
        );
    }

    #[test]
    fn drill_outside_pad_is_reported() {
        let mut pad = minimal_smd_pad();
        pad.drill = Some(Drill {
            size: Point::new(0.8, 0.8),
            offset: Point::new(5.0, 0.0),
        });
        let def = FootprintDefinition {
            name: "RESC2012X70N".into(),
            description: String::new(),
            keywords: String::new(),
            pads: vec![pad],
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
            retained: vec![],
        };
        assert!(
            check_footprint(&def)
                .iter()
                .any(|f| f.code == "lib_drill_outside_pad")
        );
    }
}
