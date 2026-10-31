use crate::{FootprintDefinition, Graphic};
use pcbcraft_geom::Point;
use serde::{Deserialize, Serialize};

pub const PITCH_TOLERANCE_MM: f64 = 0.08;
pub const SPAN_TOLERANCE_MM: f64 = 0.15;
pub const BODY_TOLERANCE_MM: f64 = 0.2;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageFamily {
    Chip,
    Sot,
    GullWing,
    Qfn,
    Bga,
    Dip,
    PinHeader,
    Cylinder,
    Axial,
    Radial,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageKey {
    pub family: PackageFamily,
    pub pin_count: u32,
    pub pitch_mm: f64,
    pub body_length_mm: f64,
    pub body_width_mm: f64,
    pub lead_span_mm: f64,
    #[serde(default)]
    pub exposed_pad_mm: f64,
    #[serde(default)]
    pub height_mm: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub courtyard_half_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub courtyard_half_y: Option<f64>,
}

impl PackageKey {
    pub fn matches(&self, other: &Self) -> bool {
        self.family == other.family
            && self.pin_count == other.pin_count
            && (self.pitch_mm - other.pitch_mm).abs() <= PITCH_TOLERANCE_MM
            && (self.lead_span_mm - other.lead_span_mm).abs() <= SPAN_TOLERANCE_MM
            && (self.body_length_mm - other.body_length_mm).abs() <= BODY_TOLERANCE_MM
            && (self.body_width_mm - other.body_width_mm).abs() <= BODY_TOLERANCE_MM
            && (self.exposed_pad_mm - other.exposed_pad_mm).abs() <= BODY_TOLERANCE_MM
    }

    pub fn from_footprint(fp: &FootprintDefinition) -> Result<Self, String> {
        if fp.pads.is_empty() {
            return Err("footprint has no pads".into());
        }
        let mut signal: Vec<&crate::FootprintPad> = fp
            .pads
            .iter()
            .filter(|p| p.number != "EP" && p.pad_type != "unplated")
            .collect();
        if signal.is_empty() {
            signal = fp.pads.iter().collect();
        }
        let pin_count = signal.len() as u32;
        let xs: Vec<f64> = signal.iter().map(|p| p.position.x).collect();
        let ys: Vec<f64> = signal.iter().map(|p| p.position.y).collect();
        let min_x = xs.iter().copied().fold(f64::INFINITY, f64::min);
        let max_x = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let min_y = ys.iter().copied().fold(f64::INFINITY, f64::min);
        let max_y = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let span_x = max_x - min_x;
        let span_y = max_y - min_y;
        let pitch_mm = estimate_pitch(&signal)?;
        let exposed = fp
            .pads
            .iter()
            .find(|p| p.number == "EP")
            .map(|p| p.size.x.max(p.size.y))
            .unwrap_or(0.0);
        let (family, body_length_mm, body_width_mm, lead_span_mm) =
            classify(pin_count, span_x, span_y, pitch_mm, &signal)?;
        let (courtyard_half_x, courtyard_half_y) = courtyard_extents(fp);
        Ok(Self {
            family,
            pin_count,
            pitch_mm,
            body_length_mm,
            body_width_mm,
            lead_span_mm,
            exposed_pad_mm: exposed,
            height_mm: 0.0,
            courtyard_half_x,
            courtyard_half_y,
        })
    }
}

fn courtyard_extents(fp: &FootprintDefinition) -> (Option<f64>, Option<f64>) {
    for g in &fp.graphics {
        if let Graphic::Rectangle {
            start,
            end,
            layer,
            ..
        } = g
            && layer == "pcbcraft.courtyard"
        {
            return (
                Some(start.x.abs().max(end.x.abs())),
                Some(start.y.abs().max(end.y.abs())),
            );
        }
    }
    (None, None)
}

fn estimate_pitch(pads: &[&crate::FootprintPad]) -> Result<f64, String> {
    let mut pitches = Vec::new();
    for a in pads {
        for b in pads {
            if a.number == b.number {
                continue;
            }
            let dx = (a.position.x - b.position.x).abs();
            let dy = (a.position.y - b.position.y).abs();
            if dx > 0.05 && dy < 0.05 {
                pitches.push(dx);
            }
            if dy > 0.05 && dx < 0.05 {
                pitches.push(dy);
            }
        }
    }
    pitches.sort_by(|x, y| x.total_cmp(y));
    pitches
        .first()
        .copied()
        .filter(|p| p.is_finite() && *p > 0.0 && *p < 50.0)
        .ok_or_else(|| "could not estimate pad pitch".into())
}

fn classify(
    pin_count: u32,
    span_x: f64,
    span_y: f64,
    pitch_mm: f64,
    pads: &[&crate::FootprintPad],
) -> Result<(PackageFamily, f64, f64, f64), String> {
    let on_left = pads.iter().filter(|p| p.position.x < -0.01).count();
    let on_right = pads.iter().filter(|p| p.position.x > 0.01).count();
    let on_top = pads.iter().filter(|p| p.position.y < -0.01).count();
    let on_bottom = pads.iter().filter(|p| p.position.y > 0.01).count();
    let sides = [on_left > 0, on_right > 0, on_top > 0, on_bottom > 0]
        .into_iter()
        .filter(|b| *b)
        .count();
    if pin_count == 2 && span_x.max(span_y) < 5.0 {
        return Ok((
            PackageFamily::Chip,
            span_x.max(1.0),
            span_y.max(0.6),
            span_x.max(span_y),
        ));
    }
    if sides >= 4 && pin_count >= 16 {
        let grid = (pin_count as f64).sqrt();
        if (grid - grid.round()).abs() < 0.01 {
            return Ok((
                PackageFamily::Bga,
                span_x.max(span_y),
                span_x.max(span_y),
                span_x.max(span_y),
            ));
        }
        return Ok((
            PackageFamily::GullWing,
            span_x.max(span_y),
            span_x.min(span_y).max(1.0),
            span_x.max(span_y),
        ));
    }
    if sides >= 3 {
        return Ok((
            PackageFamily::GullWing,
            span_x.max(span_y),
            span_x.min(span_y).max(1.0),
            span_x.max(span_y),
        ));
    }
    if on_left > 0 && on_right > 0 && (on_top == 0 && on_bottom == 0) {
        let soic = pitch_mm >= 1.0 && span_y > 3.0;
        return Ok((
            if soic {
                PackageFamily::GullWing
            } else {
                PackageFamily::Sot
            },
            span_x.max(1.0),
            span_y.max(1.0),
            span_y,
        ));
    }
    if pin_count <= 3 && span_x.max(span_y) < 6.0 {
        return Ok((
            PackageFamily::Sot,
            span_x.max(1.0),
            span_y.max(1.0),
            span_y.max(span_x),
        ));
    }
    if pin_count >= 8 && span_y > span_x && on_top == 0 && on_bottom == 0 {
        return Ok((
            PackageFamily::Dip,
            span_y,
            span_x.max(6.0),
            span_y,
        ));
    }
    if pin_count >= 4 && sides == 1 {
        return Ok((
            PackageFamily::PinHeader,
            span_x.max(span_y),
            pitch_mm * 2.0,
            span_x.max(span_y),
        ));
    }
    Ok((
        PackageFamily::Unknown,
        span_x.max(1.0),
        span_y.max(1.0),
        span_x.max(span_y),
    ))
}

pub fn pad_extent(pad: &crate::FootprintPad) -> (Point, Point) {
    let hx = pad.size.x / 2.0;
    let hy = pad.size.y / 2.0;
    (
        pad.position.offset(Point::new(-hx, -hy)),
        pad.position.offset(Point::new(hx, hy)),
    )
}
