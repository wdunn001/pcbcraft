//! Initial RS-274X (Gerber X2 attributes) copper layer writer: pad flashes and
//! track draws only. No zones, vias, mask, paste, silkscreen or drill files yet.
//!
//! Circles use the standard C aperture. Axis-aligned rect and oval pads use R
//! and O. Every other pad (rotated rect/oval, roundrect, chamfered and custom
//! polygon pads) is flashed through an aperture macro with one outline
//! primitive (code 4) holding the pad's board-space outline relative to the
//! flash point. Chamfered and custom outlines are exact; roundrect corners are
//! chord approximated within `Pad::outline_error`.
use pcbcraft_doc::{Layer, Project};
use pcbcraft_geom::Point;
use pcbcraft_library::PadShape;
use std::collections::BTreeMap;

/// Millimetres to 4.6 fixed-point coordinate integers.
fn coordinate(value: f64) -> i64 {
    (value * 1_000_000.0).round() as i64
}
fn decimal(value: f64) -> String {
    let text = format!("{value:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() {
        "0".into()
    } else {
        text.into()
    }
}

enum Aperture {
    Standard(String),
    Outline(Vec<Point>),
}
impl Aperture {
    fn key(&self) -> String {
        match self {
            Self::Standard(body) => body.clone(),
            Self::Outline(points) => points
                .iter()
                .map(|p| format!("{},{}", decimal(p.x), decimal(p.y)))
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

/// Gerber copper image of one layer. Board coordinates are y-down from the
/// top-left board corner; Gerber is y-up, so y is mirrored about the board height.
pub fn gerber_copper(p: &Project, layer: Layer) -> Result<String, String> {
    p.validate()?;
    let height = p.board.height;
    let to_gerber = |point: Point| Point::new(point.x, height - point.y);
    let mut codes: BTreeMap<String, usize> = BTreeMap::new();
    let mut definitions = String::new();
    let mut define = |aperture: Aperture| -> usize {
        let key = aperture.key();
        if let Some(code) = codes.get(&key) {
            return *code;
        }
        let code = 10 + codes.len();
        match aperture {
            Aperture::Standard(body) => definitions.push_str(&format!("%ADD{code}{body}*%\n")),
            Aperture::Outline(points) => {
                // Outline primitive: exposure, vertex count after the start
                // point, the closed vertex list, then rotation.
                let mut ring = points.clone();
                if let Some(first) = points.first().copied() {
                    ring.push(first);
                }
                definitions.push_str(&format!(
                    "%AMPAD{code}*\n4,1,{},{},0*%\n%ADD{code}PAD{code}*%\n",
                    ring.len().saturating_sub(1),
                    ring.iter()
                        .map(|v| format!("{},{}", decimal(v.x), decimal(v.y)))
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
        }
        codes.insert(key, code);
        code
    };
    let mut body = String::new();
    let mut current: Option<usize> = None;
    let mut select = |body: &mut String, code: usize| {
        if current != Some(code) {
            body.push_str(&format!("D{code}*\n"));
            current = Some(code);
        }
    };
    for t in p.board.tracks.iter().filter(|t| t.layer == layer) {
        let code = define(Aperture::Standard(format!("C,{}", decimal(t.width))));
        select(&mut body, code);
        let (a, b) = (to_gerber(t.start), to_gerber(t.end));
        body.push_str(&format!(
            "X{}Y{}D02*\nX{}Y{}D01*\n",
            coordinate(a.x),
            coordinate(a.y),
            coordinate(b.x),
            coordinate(b.y)
        ));
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            let plated = pad
                .geometry
                .as_ref()
                .is_some_and(|g| g.pad_type == "thru_hole");
            if layer == Layer::Back && !plated {
                continue;
            }
            let Some(center) = f.pad_position(&pad.number) else {
                continue;
            };
            let rotation = f.rotation + pad.geometry.as_ref().map(|g| g.rotation).unwrap_or(0.0);
            let quarter = rotation.rem_euclid(90.0);
            let axis_aligned = quarter < 1e-9 || 90.0 - quarter < 1e-9;
            let swapped = (rotation.rem_euclid(180.0) - 90.0).abs() < 1e-9;
            let aperture = match pad.geometry.as_ref().map(|g| (g.shape_kind(), g.size)) {
                None => Aperture::Standard(format!("C,{}", decimal(pad.diameter))),
                Some((PadShape::Circle, size)) => {
                    Aperture::Standard(format!("C,{}", decimal(size.x)))
                }
                Some((kind @ (PadShape::Rect | PadShape::Oval), size)) if axis_aligned => {
                    let (w, h) = if swapped {
                        (size.y, size.x)
                    } else {
                        (size.x, size.y)
                    };
                    let letter = if kind == PadShape::Rect { "R" } else { "O" };
                    Aperture::Standard(format!("{letter},{}X{}", decimal(w), decimal(h)))
                }
                Some((PadShape::Unsupported, _)) => {
                    return Err(format!(
                        "pad {} has a shape the Gerber writer does not support",
                        pad.number
                    ));
                }
                Some(_) => {
                    let mut outline = pad.outline_points();
                    outline.pop();
                    let origin = to_gerber(center);
                    Aperture::Outline(
                        outline
                            .into_iter()
                            .map(|v| {
                                let board = to_gerber(f.position.offset(v.rotate(-f.rotation)));
                                Point::new(board.x - origin.x, board.y - origin.y)
                            })
                            .collect(),
                    )
                }
            };
            let code = define(aperture);
            select(&mut body, code);
            let at = to_gerber(center);
            body.push_str(&format!("X{}Y{}D03*\n", coordinate(at.x), coordinate(at.y)));
        }
    }
    let idx = layer.index();
    let index = idx + 1;
    let side = match idx {
        0 => "Top",
        1 => "Bot",
        _ => "Inr",
    };
    Ok(format!(
        "%TF.GenerationSoftware,Tangency Labs,PCBCraft*%\n%TF.FileFunction,Copper,L{index},{side}*%\n%TF.FilePolarity,Positive*%\n%FSLAX46Y46*%\n%MOMM*%\n%LPD*%\nG01*\n{definitions}{body}M02*\n"
    ))
}
