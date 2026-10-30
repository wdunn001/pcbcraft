//! KiCad library file readers.
//!
//! Parses .kicad_sym symbol libraries, .kicad_mod footprint files, and
//! Specctra design session files into typed definitions. All readers validate
//! inputs, bound output sizes, and record unsupported features in the
//! `unsupported` field rather than failing.
//!
//! # Rules
//! - Coordinates are validated for finite range and bounded magnitude
//! - Inheritance chains are depth-limited and cycle-checked
//! - Duplicate symbol names in a library are errors
//! - Unsupported features are preserved in raw_source with a note
//!
//! # Dependencies
//! - crates/library/src/lib.rs SymbolDefinition, FootprintDefinition, and other public types
//! - crates/library/src/sexpr.rs S-expression parser for KiCad file format
//! - crates/geom/src/lib.rs Point type
//!
//! # Do not
//! - Change coordinate system handling (y-flip for symbol pins)
//! - Add support for features without explicit design approval
//! - Remove unsupported-feature tracking

use crate::{
    sexpr::{self, Node},
    *,
};
use pcbcraft_geom::Point;
use std::collections::{BTreeMap, BTreeSet};

/// The legacy model `at` form is in inches.
const MM_PER_INCH: f64 = 25.4;

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn num(s: &str) -> Result<f64, Error> {
    let n = s
        .parse::<f64>()
        .map_err(|_| invalid(format!("invalid number {s:?}")))?;
    if !n.is_finite() || n.abs() > 100_000.0 {
        return Err(invalid("number outside finite geometry bounds"));
    }
    Ok(n)
}
fn point(node: &Node, field: &str, flip: bool) -> Result<Point, Error> {
    let p = node
        .child(field)
        .ok_or_else(|| invalid(format!("missing {field}")))?;
    Ok(Point::new(
        num(p.value(1))?,
        num(p.value(2))? * if flip { -1.0 } else { 1.0 },
    ))
}
fn pin_visible(node: &Node) -> bool {
    for child in &node.children {
        if child.name() != "hide" {
            continue;
        }
        if child.children.len() <= 1 {
            return false;
        }
        if child.field("yes") == "yes" || child.value(1) == "yes" {
            return false;
        }
    }
    true
}
fn pin_graphical_style(part: &Node) -> (String, bool) {
    let token = part.value(2);
    let style = if token.is_empty() { "line" } else { token };
    if SymbolPin::graphical_style_supported(style) {
        (style.to_owned(), false)
    } else {
        ("line".to_owned(), true)
    }
}
fn angle(node: &Node) -> Result<f64, Error> {
    node.child("at")
        .map(|a| {
            if a.value(3).is_empty() {
                Ok(0.0)
            } else {
                num(a.value(3))
            }
        })
        .unwrap_or(Ok(0.0))
}
fn prop(node: &Node, name: &str) -> Option<String> {
    node.children
        .iter()
        .find(|x| x.name() == "property" && x.value(1) == name)
        .map(|x| x.value(2).to_owned())
}
fn graphic(node: &Node, flip: bool) -> Result<Option<Graphic>, Error> {
    let width = match node
        .child("stroke")
        .and_then(|x| x.child("width"))
        .or_else(|| node.child("width"))
    {
        Some(w) => num(w.value(1))?,
        None => 0.0,
    };
    let layer = node.field("layer").to_owned();
    let fill = node
        .child("fill")
        .map(|x| {
            if x.field("type").is_empty() {
                x.value(1)
            } else {
                x.field("type")
            }
        })
        .unwrap_or("none")
        .to_owned();
    let fill = match fill.as_str() {
        "no" => "none".to_owned(),
        "yes" => "solid".to_owned(),
        _ => fill,
    };
    Ok(Some(match node.name() {
        "polyline" | "fp_poly" => {
            let pts = node
                .child("pts")
                .ok_or_else(|| invalid("missing polygon points"))?;
            let points = pts
                .children
                .iter()
                .filter(|x| x.name() == "xy")
                .map(|x| {
                    Ok(Point::new(
                        num(x.value(1))?,
                        num(x.value(2))? * if flip { -1.0 } else { 1.0 },
                    ))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            Graphic::Polyline {
                points,
                width,
                fill,
                layer,
            }
        }
        "rectangle" | "fp_rect" => Graphic::Rectangle {
            start: point(node, "start", flip)?,
            end: point(node, "end", flip)?,
            width,
            fill,
            layer,
        },
        "fp_line" => Graphic::Line {
            start: point(node, "start", flip)?,
            end: point(node, "end", flip)?,
            width,
            layer,
        },
        "circle" => Graphic::Circle {
            center: point(node, "center", flip)?,
            radius: num(node.field("radius"))?,
            width,
            fill,
            layer,
        },
        "fp_circle" => {
            let center = point(node, "center", flip)?;
            Graphic::Circle {
                center,
                radius: center.distance(point(node, "end", flip)?),
                width,
                fill,
                layer,
            }
        }
        "arc" | "fp_arc" => Graphic::Arc {
            start: point(node, "start", flip)?,
            mid: point(node, "mid", flip)?,
            end: point(node, "end", flip)?,
            width,
            layer,
        },
        "text" | "fp_text" => {
            let size = node
                .child("effects")
                .and_then(|x| x.child("font"))
                .map(|x| point(x, "size", false))
                .transpose()?
                .unwrap_or(Point::new(1.27, 1.27));
            Graphic::Text {
                position: point(node, "at", flip)?,
                angle: angle(node)?,
                text: node
                    .value(if node.name() == "fp_text" { 2 } else { 1 })
                    .to_owned(),
                size,
                layer,
            }
        }
        _ => return Ok(None),
    }))
}
fn unit(name: &str) -> Result<(u32, u32), Error> {
    let mut parts = name.rsplit('_');
    let style = parts
        .next()
        .and_then(|x| x.parse().ok())
        .ok_or_else(|| invalid("invalid symbol body style"))?;
    let unit = parts
        .next()
        .and_then(|x| x.parse().ok())
        .ok_or_else(|| invalid("invalid symbol unit"))?;
    Ok((unit, style))
}
fn resolve(
    name: &str,
    nodes: &BTreeMap<String, &Node>,
    source: &str,
    resolved: &mut BTreeMap<String, SymbolDefinition>,
    stack: &mut BTreeSet<String>,
    output_bytes: &mut usize,
) -> Result<SymbolDefinition, Error> {
    if let Some(s) = resolved.get(name) {
        return Ok(s.clone());
    }
    if stack.len() >= 64 {
        return Err(Error::Limit("inheritance depth"));
    }
    if !stack.insert(name.to_owned()) {
        return Err(invalid(format!("cyclic symbol inheritance at {name}")));
    }
    let node = nodes
        .get(name)
        .ok_or_else(|| invalid(format!("missing inherited symbol {name}")))?;
    let parent = node.child("extends").map(|x| x.value(1).to_owned());
    let mut s = if let Some(parent) = &parent {
        resolve(parent, nodes, source, resolved, stack, output_bytes)?
    } else {
        SymbolDefinition {
            name: name.to_owned(),
            reference: String::new(),
            value: name.to_owned(),
            description: String::new(),
            keywords: String::new(),
            footprint: String::new(),
            set: String::new(),
            pins: Vec::new(),
            graphics: Vec::new(),
            extends: None,
            raw_source: String::new(),
            unsupported: Vec::new(),
        }
    };
    s.name = name.to_owned();
    s.extends = parent;
    s.raw_source = source[node.span.clone()].to_owned();
    for (key, target) in [
        ("Reference", &mut s.reference),
        ("Value", &mut s.value),
        ("Description", &mut s.description),
        ("ki_keywords", &mut s.keywords),
        ("Footprint", &mut s.footprint),
    ] {
        if let Some(value) = prop(node, key) {
            *target = value;
        }
    }
    if let Some(value) = prop(node, "ki_description") {
        s.description = value;
    }
    let mut local_units = BTreeSet::new();
    for child in &node.children {
        if child.name() == "symbol" {
            let (u, b) = unit(child.value(1))?;
            if !local_units.insert((u, b)) {
                return Err(invalid("duplicate symbol unit/body definition"));
            }
            // A derived local unit replaces that unit's inherited drawing/pins.
            s.pins.retain(|p| p.unit != u || p.body_style != b);
            s.graphics.retain(|g| g.unit != u || g.body_style != b);
            for part in &child.children {
                if part.name() == "pin" {
                    let (graphical_style, unknown_style) = pin_graphical_style(part);
                    if unknown_style {
                        s.unsupported.push(format!(
                            "pin {} graphical style {} is preserved in raw source",
                            part.field("number"),
                            part.value(2)
                        ));
                    }
                    s.pins.push(SymbolPin {
                        number: part.field("number").to_owned(),
                        name: part.field("name").to_owned(),
                        position: point(part, "at", true)?,
                        angle: angle(part)?,
                        length: num(part.field("length"))?,
                        electrical_type: part.value(1).to_owned(),
                        unit: u,
                        body_style: b,
                        graphical_style,
                        visible: pin_visible(part),
                    });
                    if part.child("alternate").is_some() {
                        s.unsupported.push(format!(
                            "pin {} alternate functions preserved in raw source",
                            part.field("number")
                        ));
                    }
                } else if let Some(primitive) = graphic(part, true)? {
                    s.graphics.push(UnitGraphic {
                        unit: u,
                        body_style: b,
                        primitive,
                    });
                } else if part.atom.is_none() {
                    s.unsupported
                        .push(format!("symbol unit expression {}", part.name()));
                }
            }
        } else if child.atom.is_none()
            && !matches!(
                child.name(),
                "property"
                    | "extends"
                    | "pin_names"
                    | "pin_numbers"
                    | "in_bom"
                    | "on_board"
                    | "exclude_from_sim"
                    | "in_pos_files"
                    | "duplicate_pin_numbers_are_jumpers"
                    | "embedded_fonts"
            )
        {
            s.unsupported
                .push(format!("symbol expression {}", child.name()));
        }
    }
    if node.field("embedded_fonts") == "yes" {
        s.unsupported
            .push("embedded fonts require unsupported font decoding".into());
    }
    s.validate()?;
    // Inheritance can expand a tiny source into many large copies. Bound the
    // resulting definitions as well as the input AST, before adding each copy.
    let bytes = s.raw_source.len()
        + s.name.len()
        + s.reference.len()
        + s.value.len()
        + s.description.len()
        + s.keywords.len()
        + s.footprint.len()
        + s.pins
            .iter()
            .map(|p| 256 + p.number.len() + p.name.len() + p.electrical_type.len())
            .sum::<usize>()
        + s.graphics
            .iter()
            .map(|g| {
                512 + match &g.primitive {
                    Graphic::Polyline { points, .. } => points.len() * 32,
                    Graphic::Text { text, .. } => text.len(),
                    _ => 0,
                }
            })
            .sum::<usize>()
        + s.unsupported.iter().map(|x| 64 + x.len()).sum::<usize>();
    *output_bytes = output_bytes
        .checked_add(bytes)
        .ok_or(Error::Limit("resolved library bytes"))?;
    if *output_bytes > 128 * 1024 * 1024 {
        return Err(Error::Limit("resolved library bytes"));
    }
    stack.remove(name);
    resolved.insert(name.to_owned(), s.clone());
    Ok(s)
}
pub fn read_symbols(source: &str) -> Result<Vec<SymbolDefinition>, Error> {
    let root = sexpr::parse(source)?;
    if root.name() != "kicad_symbol_lib" {
        return Err(invalid("expected kicad_symbol_lib"));
    }
    let mut nodes = BTreeMap::new();
    for node in root.children.iter().filter(|x| x.name() == "symbol") {
        if nodes.insert(node.value(1).to_owned(), node).is_some() {
            return Err(invalid("duplicate symbol name"));
        }
    }
    if nodes.len() > 20_000 {
        return Err(Error::Limit("symbol count"));
    }
    let mut result = BTreeMap::new();
    let mut output_bytes = 0;
    for name in nodes.keys() {
        resolve(
            name,
            &nodes,
            source,
            &mut result,
            &mut BTreeSet::new(),
            &mut output_bytes,
        )?;
    }
    Ok(result.into_values().collect())
}
pub fn read_footprint(source: &str) -> Result<FootprintDefinition, Error> {
    let node = sexpr::parse(source)?;
    if !matches!(node.name(), "footprint" | "module") {
        return Err(invalid("expected footprint"));
    }
    let mut s = FootprintDefinition {
        name: node.value(1).to_owned(),
        description: node.field("descr").to_owned(),
        keywords: node.field("tags").to_owned(),
        pads: Vec::new(),
        graphics: Vec::new(),
        model: None,
        rules: None,
        raw_source: source[node.span.clone()].to_owned(),
        unsupported: Vec::new(),
    };
    for child in &node.children {
        if child.name() == "point" {
            // KiCad PCB_POINT is a non-fabricated reference/snapping marker.
            // Retain its exact expression in raw_source, not copper/artwork.
            point(child, "at", false)?;
            if num(child.field("size"))? <= 0.0 {
                return Err(invalid("invalid reference point size"));
            }
        } else if child.name() == "pad" {
            let drill = child
                .child("drill")
                .map(|d| {
                    let oval = d.value(1) == "oval";
                    let x = num(d.value(if oval { 2 } else { 1 }))?;
                    let y = if oval { num(d.value(3))? } else { x };
                    Ok(Drill {
                        size: Point::new(x, y),
                        offset: if d.child("offset").is_some() {
                            point(d, "offset", false)?
                        } else {
                            Point::default()
                        },
                    })
                })
                .transpose()?;
            let layers = child
                .child("layers")
                .ok_or_else(|| invalid("missing pad layers"))?
                .children
                .iter()
                .skip(1)
                .filter_map(|x| x.atom.clone())
                .collect();
            let mut shape = child.value(3).to_owned();
            let number = child.value(1).to_owned();
            let size = point(child, "size", false)?;
            let mut roundrect_ratio = child
                .child("roundrect_rratio")
                .map(|r| num(r.value(1)))
                .transpose()?;
            let chamfer = pad_chamfer(child, &number, &mut s.unsupported)?;
            if let Some(c) = chamfer {
                if shape == "rect"
                    || (shape == "roundrect" && roundrect_ratio.unwrap_or(0.0) <= 0.0)
                {
                    shape = "chamfered".into();
                    roundrect_ratio = None;
                } else {
                    s.unsupported.push(format!(
                        "pad {number} chamfer on {shape} geometry (ratio {})",
                        c.ratio
                    ));
                }
            }
            let chamfer = chamfer.filter(|_| shape == "chamfered");
            let polygon = if shape == "custom" {
                custom_pad_polygon(child, size, &number, &mut s.unsupported)?
            } else {
                if child.child("primitives").is_some() {
                    s.unsupported
                        .push(format!("pad {number} primitives on a {shape} pad"));
                }
                Vec::new()
            };
            if !matches!(
                shape.as_str(),
                "circle" | "rect" | "oval" | "roundrect" | "chamfered" | "custom"
            ) {
                s.unsupported.push(format!(
                    "pad {number} shape {shape} requires unsupported geometry"
                ));
            }
            let pad_type = match child.value(2) {
                "np_thru_hole" => "unplated".into(),
                token => token.to_owned(),
            };
            s.pads.push(FootprintPad {
                number,
                pad_type,
                shape,
                position: point(child, "at", false)?,
                rotation: angle(child)?,
                size,
                layers,
                drill,
                roundrect_ratio,
                chamfer,
                polygon,
                raw_source: source[child.span.clone()].to_owned(),
            });
        } else if child.name() == "model" {
            if s.model.is_some() {
                s.unsupported.push("more than one 3D model attached".into());
            } else {
                let triple =
                    |holder: Option<&Node>, fallback: [f64; 3]| -> Result<[f64; 3], Error> {
                        let Some(holder) = holder else {
                            return Ok(fallback);
                        };
                        let Some(xyz) = holder.child("xyz") else {
                            return Err(invalid("model transform needs an xyz triple"));
                        };
                        Ok([num(xyz.value(1))?, num(xyz.value(2))?, num(xyz.value(3))?])
                    };
                // Files from before the `offset` form place the model with
                // `at`, in inches. `offset` is millimetres and wins when both
                // are present.
                let offset = if child.child("offset").is_some() {
                    triple(child.child("offset"), [0.0; 3])?
                } else {
                    let inches = triple(child.child("at"), [0.0; 3])?;
                    // Round away binary noise so 3 inches reads as 76.2 mm.
                    let to_mm = |inches: f64| (inches * MM_PER_INCH * 1e9).round() / 1e9;
                    let mm = [to_mm(inches[0]), to_mm(inches[1]), to_mm(inches[2])];
                    if mm.iter().any(|v| v.abs() > 100_000.0) {
                        return Err(invalid("model offset outside finite geometry bounds"));
                    }
                    mm
                };
                let scale = triple(child.child("scale"), [1.0; 3])?;
                let rotate = triple(child.child("rotate"), [0.0; 3])?;
                let cad_part = child
                    .child("cad_part")
                    .map(|n| n.value(1).to_owned())
                    .filter(|s| !s.is_empty());
                s.model = Some(Model3D {
                    path: child.value(1).to_owned(),
                    offset,
                    scale,
                    rotate,
                    cad_part,
                    raw_source: source[child.span.clone()].to_owned(),
                });
            }
        } else if matches!(
            child.name(),
            "clearance" | "solder_mask_margin" | "solder_paste_margin"
        ) {
            let value = num(child.value(1))?;
            let rules = s.rules.get_or_insert_default();
            let (slot, label) = match child.name() {
                "clearance" => (&mut rules.clearance, "local clearance"),
                "solder_mask_margin" => (&mut rules.mask_margin, "solder mask margin"),
                _ => (&mut rules.paste_margin, "solder paste margin"),
            };
            if slot.is_some() {
                s.unsupported.push(format!("duplicate {label} override"));
            } else {
                *slot = Some(value);
            }
        } else if let Some(g) = graphic(child, false)? {
            s.graphics.push(g);
        } else if child.atom.is_none()
            && !matches!(
                child.name(),
                "version"
                    | "generator"
                    | "generator_version"
                    | "layer"
                    | "descr"
                    | "tags"
                    | "property"
                    | "attr"
                    | "duplicate_pad_numbers_are_jumpers"
                    | "embedded_fonts"
            )
        {
            s.unsupported
                .push(format!("footprint expression {}", child.name()));
        }
    }
    s.validate()?;
    Ok(s)
}

/// Read the net map of a Specctra design session: every named net with its
/// pins split from the `REF-PIN` entries. Nets without pins are skipped.
pub fn read_specctra_session(source: &str) -> Result<Vec<SessionNet>, Error> {
    let node = sexpr::parse(source)?;
    if node.name() != "session" {
        return Err(invalid("expected a design session"));
    }
    let map = node
        .child("net_map_out")
        .ok_or_else(|| invalid("session has no net map"))?;
    let mut nets = Vec::new();
    for net in map.children.iter().filter(|c| c.name() == "net") {
        let name = net
            .child("name")
            .map(|n| n.value(1).to_owned())
            .unwrap_or_default();
        if name.is_empty() || name.len() > 128 {
            return Err(invalid("session net name must be 1 to 128 bytes"));
        }
        let Some(pins) = net.child("pins") else {
            continue;
        };
        let mut list = Vec::new();
        for pin in pins.children.iter().filter(|c| c.name() == "pin") {
            let token = pin.value(1);
            let Some((reference, number)) = token.rsplit_once('-') else {
                return Err(invalid("session pin entries need the REF-PIN form"));
            };
            if reference.is_empty()
                || reference.len() > 64
                || number.is_empty()
                || number.len() > 64
            {
                return Err(invalid("session pin entry is out of bounds"));
            }
            list.push((reference.to_owned(), number.to_owned()));
        }
        if list.len() > 128 {
            return Err(invalid("a session net lists at most 128 pins"));
        }
        nets.push(SessionNet { name, pins: list });
        if nets.len() > 256 {
            return Err(invalid("a session holds at most 256 nets"));
        }
    }
    let total: usize = nets.iter().map(|n| n.pins.len()).sum();
    if total > 4096 {
        return Err(invalid("a session lists at most 4096 pins"));
    }
    Ok(nets)
}

/// KiCad expresses chamfers as `(chamfer_ratio r) (chamfer corner...)` on a
/// rect or roundrect pad. Returns `None` when no corner is cut.
fn pad_chamfer(
    pad: &Node,
    number: &str,
    unsupported: &mut Vec<String>,
) -> Result<Option<PadChamfer>, Error> {
    let Some(names) = pad.child("chamfer") else {
        return Ok(None);
    };
    let mut corners = ChamferCorners::default();
    for name in names
        .children
        .iter()
        .skip(1)
        .filter_map(|x| x.atom.as_deref())
    {
        match name {
            "top_left" => corners.top_left = true,
            "top_right" => corners.top_right = true,
            "bottom_left" => corners.bottom_left = true,
            "bottom_right" => corners.bottom_right = true,
            other => {
                unsupported.push(format!("pad {number} chamfer corner {other}"));
                return Ok(None);
            }
        }
    }
    let ratio = pad
        .child("chamfer_ratio")
        .map(|r| num(r.value(1)))
        .transpose()?
        .unwrap_or(0.0);
    if !corners.any() || ratio <= 0.0 {
        return Ok(None);
    }
    if ratio > 0.5 {
        unsupported.push(format!("pad {number} chamfer ratio {ratio} exceeds 0.5"));
        return Ok(None);
    }
    Ok(Some(PadChamfer { ratio, corners }))
}

/// A custom pad is supported when its primitives are exactly one filled,
/// zero-width polygon that contains the pad's anchor. Its copper is then that
/// polygon alone. Anything else is recorded as unsupported.
fn custom_pad_polygon(
    pad: &Node,
    anchor: Point,
    number: &str,
    unsupported: &mut Vec<String>,
) -> Result<Vec<Point>, Error> {
    let primitives: Vec<&Node> = pad
        .child("primitives")
        .map(|p| p.children.iter().filter(|x| x.atom.is_none()).collect())
        .unwrap_or_default();
    let [poly] = primitives[..] else {
        unsupported.push(format!(
            "pad {number} custom geometry needs exactly one polygon primitive"
        ));
        return Ok(Vec::new());
    };
    let width = poly.child("width").map(|w| num(w.value(1))).transpose()?;
    let filled = poly
        .child("fill")
        .is_none_or(|f| matches!(f.value(1), "yes" | "solid"));
    if poly.name() != "gr_poly" || width.unwrap_or(0.0) != 0.0 || !filled {
        unsupported.push(format!(
            "pad {number} custom primitive {} is not a filled zero-width polygon",
            poly.name()
        ));
        return Ok(Vec::new());
    }
    let Some(pts) = poly.child("pts") else {
        unsupported.push(format!("pad {number}: custom pad polygon has no points"));
        return Ok(Vec::new());
    };
    let points = pts
        .children
        .iter()
        .filter(|x| x.name() == "xy")
        .take(crate::MAX_PAD_POLYGON_VERTICES + 1)
        .map(|xy| Ok(Point::new(num(xy.value(1))?, num(xy.value(2))?)))
        .collect::<Result<Vec<_>, Error>>()?;
    if let Some(problem) = crate::custom_polygon_error(anchor, &points) {
        unsupported.push(format!("pad {number}: {problem}"));
        return Ok(Vec::new());
    }
    Ok(points)
}
