//! Validation checks for library definitions.
//!
//! Every public `validate` method enforces the invariants that downstream
//! consumers rely on: finite coordinates, bounded object counts, non-empty
//! identifiers, and consistent pad geometry. Validation runs before a
//! definition is inserted into a library or returned by a source adapter.
//!
//! # Rules
//! - Text fields are at most 256 KB and must not contain NUL bytes.
//! - Coordinates must be finite and at most 100 000 mm in magnitude.
//! - Sizes, widths, radii, and drill diameters must be non-negative.
//! - Object counts (pins, pads, graphics) are capped at 20 000.
//! - Pin numbers must be unique within each unit and body style.
//! - Pad polygons must be simple, bounded, and contain the pad anchor.
//!
//! # Dependencies
//! - crates/library/src/validation.rs `crate::*`: all library types
//! - crates/geom/src/lib.rs point-in-polygon, segment intersection, boundary distance
//!
//! # Do not
//! - Add behavioral side effects to validation.
//! - Change error message strings without updating callers that match on them.

use crate::*;
use std::collections::BTreeSet;
fn bad(message: &str) -> Error {
    Error::Invalid(message.to_owned())
}
fn text(s: &str) -> Result<(), Error> {
    if s.len() > 256 * 1024 || s.contains('\0') {
        Err(bad("text exceeds bounds or contains NUL"))
    } else {
        Ok(())
    }
}
fn scalar(v: f64) -> bool {
    v.is_finite() && v.abs() <= 100_000.0
}
fn dimension(v: f64) -> bool {
    scalar(v) && v >= 0.0
}
fn positive_size(p: Point) -> bool {
    p.valid() && p.x > 0.0 && p.y > 0.0
}
fn metadata(raw: &str, unsupported: &[String]) -> Result<(), Error> {
    if raw.len() > 8 * 1024 * 1024 || unsupported.len() > 10_000 {
        return Err(Error::Limit("definition metadata"));
    }
    for s in unsupported {
        text(s)?;
    }
    Ok(())
}
impl Graphic {
    /// Validate that all geometry values are finite and within bounds.
    ///
    /// Checks coordinate ranges, non-negative widths and radii, and text
    /// field limits for every graphic variant.
    ///
    /// # Rules
    /// - Coordinates must be finite and at most 100 000 mm in magnitude.
    /// - Widths and radii must be non-negative.
    /// - Polylines must have between 2 and 10 000 points.
    /// - Text fields are at most 256 KB.
    ///
    /// # Do not
    /// - Call on partially constructed graphics.
    pub fn validate(&self) -> Result<(), Error> {
        let valid = match self {
            Self::Line {
                start,
                end,
                width,
                layer,
            } => {
                text(layer)?;
                start.valid() && end.valid() && dimension(*width)
            }
            Self::Rectangle {
                start,
                end,
                width,
                fill,
                layer,
            } => {
                text(fill)?;
                text(layer)?;
                start.valid() && end.valid() && dimension(*width)
            }
            Self::Circle {
                center,
                radius,
                width,
                fill,
                layer,
            } => {
                text(fill)?;
                text(layer)?;
                center.valid() && *radius > 0.0 && dimension(*radius) && dimension(*width)
            }
            Self::Arc {
                start,
                mid,
                end,
                width,
                layer,
            } => {
                text(layer)?;
                start.valid() && mid.valid() && end.valid() && dimension(*width)
            }
            Self::Polyline {
                points,
                width,
                fill,
                layer,
            } => {
                text(fill)?;
                text(layer)?;
                points.len() >= 2
                    && points.len() <= 10_000
                    && points.iter().all(|p| p.valid())
                    && dimension(*width)
            }
            Self::Text {
                position,
                angle,
                text: content,
                size,
                layer,
            } => {
                text(content)?;
                text(layer)?;
                position.valid() && scalar(*angle) && positive_size(*size)
            }
        };
        if valid {
            Ok(())
        } else {
            Err(bad("invalid library graphic geometry"))
        }
    }
}
impl SymbolDefinition {
    /// Validate symbol structure, pin uniqueness, and graphic references.
    ///
    /// Checks name, pin numbers, electrical types, graphical styles,
    /// object counts, text field limits, and validates every graphic.
    ///
    /// # Rules
    /// - Name must be non-empty.
    /// - Pin numbers must be unique within each unit and body style.
    /// - Graphical styles must be supported.
    /// - Object counts are capped at 20 000.
    ///
    /// # Do not
    /// - Call on symbols that have not finished loading.
    pub fn validate(&self) -> Result<(), Error> {
        if self.name.is_empty() || self.pins.len() > 20_000 || self.graphics.len() > 20_000 {
            return Err(bad("invalid symbol name or object count"));
        }
        for s in [
            &self.name,
            &self.reference,
            &self.value,
            &self.description,
            &self.keywords,
            &self.footprint,
            &self.set,
        ] {
            text(s)?;
        }
        if let Some(parent) = &self.extends {
            text(parent)?;
        }
        metadata(&self.raw_source, &self.unsupported)?;
        let mut keys = BTreeSet::new();
        for p in &self.pins {
            text(&p.number)?;
            text(&p.name)?;
            text(&p.electrical_type)?;
            text(&p.graphical_style)?;
            if !SymbolPin::graphical_style_supported(&p.graphical_style) {
                return Err(bad("invalid pin graphical style"));
            }
            if p.number.is_empty()
                || p.electrical_type.is_empty()
                || !p.position.valid()
                || !scalar(p.angle)
                || !dimension(p.length)
                || p.unit > 1024
                || p.body_style > 32
                || !keys.insert((p.unit, p.body_style, &p.number))
            {
                return Err(bad("invalid or duplicate symbol pin"));
            }
        }
        for g in &self.graphics {
            if g.unit > 1024 || g.body_style > 32 {
                return Err(bad("invalid graphic unit"));
            }
            g.primitive.validate()?;
        }
        Ok(())
    }
}
impl FootprintDefinition {
    /// Validate footprint structure, pad geometry, and rule overrides.
    ///
    /// Checks name, pad positions, sizes, drill values, chamfer and
    /// roundrect parameters, custom polygons, object counts, text field
    /// limits, and validates every graphic.
    ///
    /// # Rules
    /// - Name must be non-empty.
    /// - Pad positions must be finite and within bounds.
    /// - Drill values must be positive (except for unplated pads).
    /// - Chamfer ratios must be in (0, 0.5].
    /// - Roundrect ratios must be in [0, 0.5].
    /// - Custom pad polygons must be simple and contain the anchor.
    /// - Object counts are capped at 20 000.
    ///
    /// # Do not
    /// - Call on footprints that have not finished loading.
    pub fn validate(&self) -> Result<(), Error> {
        if let Some(rules) = &self.rules {
            for value in [rules.clearance, rules.mask_margin, rules.paste_margin]
                .into_iter()
                .flatten()
            {
                if !scalar(value) || value.abs() > 100.0 {
                    return Err(bad(
                        "footprint rule override must be finite, at most 100 mm",
                    ));
                }
            }
        }
        if let Some(body) = &self.box_body {
            for value in [body.width, body.depth, body.height] {
                if !scalar(value) || !(0.05..=100.0).contains(&value) {
                    return Err(bad(
                        "footprint box body size must be finite and between 0.05 and 100 mm",
                    ));
                }
            }
        }
        if self.name.is_empty() || self.pads.len() > 20_000 || self.graphics.len() > 20_000 {
            return Err(bad("invalid footprint name or object count"));
        }
        for s in [&self.name, &self.description, &self.keywords] {
            text(s)?;
        }
        metadata(&self.raw_source, &self.unsupported)?;
        if let Some(m) = &self.model {
            text(&m.path)?;
            metadata(&m.raw_source, &[])?;
            if m.path.is_empty()
                || m.path.len() > 1024
                || !m.offset.iter().all(|v| scalar(*v))
                || !m.rotate.iter().all(|v| scalar(*v))
                || !m.scale.iter().all(|v| scalar(*v) && *v > 0.0)
            {
                return Err(bad("invalid 3D model reference"));
            }
        }
        for p in &self.pads {
            for s in [&p.number, &p.pad_type, &p.shape] {
                text(s)?;
            }
            metadata(&p.raw_source, &[])?;
            if !p.position.valid()
                || !scalar(p.rotation)
                || (!positive_size(p.size) && p.pad_type != "unplated")
                || (p.layers.is_empty() && p.pad_type != "unplated")
                || p.layers.len() > 64
                || p.shape.is_empty()
                || p.pad_type.is_empty()
            {
                return Err(bad("invalid footprint pad"));
            }
            for layer in &p.layers {
                text(layer)?;
            }
            if p.pad_type == "unplated" {
                if p.drill.is_none() {
                    return Err(bad("unplated pad needs a drill"));
                }
                if p.layers.iter().any(|l| {
                    l.ends_with(".Cu") || l.as_str() == "*.Cu" || l == "F.Cu" || l == "B.Cu"
                }) {
                    return Err(bad("unplated pad must not include copper layers"));
                }
                continue;
            }
            if p.roundrect_ratio
                .is_some_and(|r| !r.is_finite() || !(0.0..=0.5).contains(&r))
            {
                return Err(bad("invalid roundrect ratio"));
            }
            if p.drill
                .as_ref()
                .is_some_and(|d| !positive_size(d.size) || !d.offset.valid())
            {
                return Err(bad("invalid pad drill"));
            }
            #[allow(clippy::collapsible_match)]
            match (p.shape.as_str(), &p.chamfer) {
                ("chamfered", Some(c)) => {
                    #[allow(clippy::collapsible_match)]
                    if !c.ratio.is_finite()
                        || c.ratio <= 0.0
                        || c.ratio > 0.5
                        || !c.corners.any()
                        || p.roundrect_ratio.is_some()
                    {
                        return Err(bad(
                            "chamfered pad needs a ratio in (0, 0.5], at least one corner and no corner rounding",
                        ));
                    }
                }
                ("chamfered", None) => return Err(bad("chamfered pad is missing its chamfer")),
                (shape, Some(_)) if shape != "chamfered" => {
                    return Err(bad("chamfer is only valid on a chamfered pad"));
                }
                _ => {}
            }
            if !p.polygon.is_empty() {
                if p.shape != "custom" || p.roundrect_ratio.is_some() {
                    return Err(bad("pad polygon is only valid on a custom pad"));
                }
                if let Some(message) = custom_polygon_error(p.size, &p.polygon) {
                    return Err(bad(message));
                }
            }
        }
        for g in &self.graphics {
            g.validate()?;
        }
        Ok(())
    }
}

/// Why a custom pad polygon is unusable, or `None` when it is a bounded simple
/// polygon that contains the pad's rectangular anchor of the given size.
pub fn custom_polygon_error(anchor: Point, polygon: &[Point]) -> Option<&'static str> {
    use pcbcraft_geom::{point_in_polygon, polygon_boundary_distance, polygon_edges};
    if polygon.len() < 3 || polygon.len() > MAX_PAD_POLYGON_VERTICES {
        return Some("custom pad polygon needs 3 to 256 vertices");
    }
    if polygon
        .iter()
        .any(|v| !v.x.is_finite() || !v.y.is_finite() || v.x.abs() > 1_000.0 || v.y.abs() > 1_000.0)
    {
        return Some("custom pad vertices must be finite and within 1000 mm of the pad origin");
    }
    if !pcbcraft_geom::polygon_is_simple(polygon) {
        return Some(
            "custom pad polygon must be simple, with non-zero area and no repeated vertices",
        );
    }
    if !positive_size(anchor) {
        return Some("custom pad anchor size must be positive");
    }
    let (hx, hy) = (anchor.x / 2.0, anchor.y / 2.0);
    let corners = [
        Point::new(-hx, -hy),
        Point::new(hx, -hy),
        Point::new(hx, hy),
        Point::new(-hx, hy),
    ];
    let on_or_inside =
        |p: Point| point_in_polygon(p, polygon) || polygon_boundary_distance(p, polygon) < 1e-7;
    let anchor_inside = corners.iter().all(|c| on_or_inside(*c))
        && !polygon_edges(&corners).any(|(a, b)| {
            polygon_edges(polygon).any(|(c, d)| pcbcraft_geom::segments_cross(a, b, c, d))
        })
        && !polygon
            .iter()
            .any(|v| v.x.abs() < hx - 1e-7 && v.y.abs() < hy - 1e-7);
    if !anchor_inside {
        return Some("custom pad anchor rectangle must lie inside the polygon");
    }
    None
}
