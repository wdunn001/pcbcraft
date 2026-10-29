//! Read-only library-quality checks on authored footprints.
use crate::footprint_pad_outline::{pad_outline_closed, pad_outlines_overlap};
use crate::{FootprintDefinition, Graphic};
use pcbcraft_geom::{Point, point_in_polygon, polygon_edges, segments_cross};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const SILK_PAD_CLEARANCE_MM: f64 = 0.2;
const COURTYARD_LAYER: &str = "pcbcraft.courtyard";
const SILK_LAYER: &str = "F.SilkS";
const FAB_LAYER: &str = "F.Fab";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FootprintFinding {
    pub code: String,
    pub severity: FindingSeverity,
    pub message: String,
    pub hint: String,
}

/// Run MVP library checks on a footprint definition.
pub fn check_footprint(def: &FootprintDefinition) -> Vec<FootprintFinding> {
    let mut findings = Vec::new();
    check_pad_overlap(def, &mut findings);
    check_duplicate_pad_numbers(def, &mut findings);
    check_pin1(def, &mut findings);
    check_courtyard(def, &mut findings);
    check_silk_over_pad(def, &mut findings);
    check_fab_outline(def, &mut findings);
    findings
}

fn push(
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
