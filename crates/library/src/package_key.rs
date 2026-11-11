//! Package identity: family classification, pad-pattern inference and
//! package-name grammar (millimetres). Keys decide which parametric 3D
//! body a footprint gets and when two parts share one package.
//! Local rules: name grammar wins over pad inference, every dimension is
//! clamped to a finite positive range, and no panic, unwrap or unsafe
//! appears anywhere in this file.
use crate::{FootprintDefinition, Graphic};
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

impl PackageKey {
    /// Build a package key from the package text embedded in a footprint name.
    ///
    /// This parses the name first, before any pad measurement, so the caller
    /// can decide whether to trust the name or fall back to pad inference.
    ///
    /// # Rules
    ///
    /// - Names up to 64 characters are accepted.
    /// - Recognised families are chip size codes, SOT, SOIC/SOP/MSOP/SSOP/TSSOP,
    ///   QFP/LQFP/TQFP, QFN/DFN, DIP/SIP and pin headers.
    /// - Every dimension lands in a finite positive range and height_mm is never
    ///   zero.
    /// - Unknown names return Err.
    ///
    /// # Dependencies
    ///
    /// crates/library/src/param_body.rs (the body generator calls this first).
    ///
    /// # Do not
    ///
    /// Never fall back to a fabricated key here; return Err and let the caller
    /// use pad inference instead.
    pub fn from_footprint_name(name: &str) -> Result<Self, String> {
        if name.is_empty() || name.len() > 64 {
            return Err(format!("footprint name out of range: {name}"));
        }
        let (family, pin_count, pitch, length, width, span, height) = parse_package_name(name)?;
        if family == PackageFamily::Unknown {
            return Err(format!("unrecognised package name: {name}"));
        }
        Ok(Self {
            family,
            pin_count,
            pitch_mm: clamp_dim(pitch, 50.0),
            body_length_mm: clamp_dim(length, 60.0),
            body_width_mm: clamp_dim(width, 60.0),
            lead_span_mm: clamp_dim(span, 60.0),
            exposed_pad_mm: 0.0,
            height_mm: clamp_dim(height, 30.0),
            courtyard_half_x: None,
            courtyard_half_y: None,
        })
    }
}

/// Clamp a dimension to at least 0.1 and at most cap, mapping non-finite to 0.1.
fn clamp_dim(v: f64, cap: f64) -> f64 {
    if !v.is_finite() {
        0.1
    } else {
        v.clamp(0.1, cap)
    }
}

fn courtyard_extents(fp: &FootprintDefinition) -> (Option<f64>, Option<f64>) {
    for g in &fp.graphics {
        if let Graphic::Rectangle {
            start, end, layer, ..
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
        return Ok((PackageFamily::Dip, span_y, span_x.max(6.0), span_y));
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

/// Parsed shape of a package name: family, pin count, pitch, body length,
/// body width, lead span and height, all in millimetres.
type PackageShape = (PackageFamily, u32, f64, f64, f64, f64, f64);

/// Parse a footprint name into a package key.
///
/// Reads the package grammar out of a footprint name and returns the family,
/// pin count, pitch, body length, body width, lead span and height.
///
/// # Rules
///
/// Case insensitive; separators between letters and digits are ignored.
///
/// # Dependencies
///
/// crates/library/src/package_key.rs (PackageFamily)
///
/// # Do not
///
/// Never panic; return Err for names that do not match.
fn parse_package_name(name: &str) -> Result<PackageShape, String> {
    let compact: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if compact.is_empty() || compact.len() > 64 {
        return Err("package name is empty or too long".to_string());
    }

    fn first_digits(compact: &str, at: usize) -> Option<(u32, usize)> {
        let rest = &compact[at..];
        let mut value: u32 = 0;
        let mut len = 0;
        for ch in rest.chars() {
            if ch.is_ascii_digit() {
                let d = (ch as u32) - ('0' as u32);
                value = value.checked_mul(10)?.checked_add(d)?;
                len += 1;
            } else {
                break;
            }
        }
        if len == 0 { None } else { Some((value, len)) }
    }
    fn starts_with_any<'a>(compact: &str, prefixes: &[&'a str]) -> Option<&'a str> {
        let mut best: Option<&str> = None;
        for prefix in prefixes {
            if compact.starts_with(prefix) && best.is_none_or(|done| prefix.len() > done.len()) {
                best = Some(prefix);
            }
        }
        best
    }

    let mut work = compact.clone();
    if work.starts_with('P') || work.starts_with('C') {
        work = work[1..].to_string();
    }
    for prefix in ["TR", "RL", "R"] {
        if work.starts_with(prefix) {
            work = work[prefix.len()..].to_string();
            break;
        }
    }
    for suffix in ["TR", "RL", "R"] {
        if work.ends_with(suffix) {
            work = work[..work.len() - suffix.len()].to_string();
            break;
        }
    }

    let chip_sizes: [(&str, f64, f64, f64); 11] = [
        ("01005", 0.5, 0.25, 0.13),
        ("0201", 0.6, 0.3, 0.3),
        ("0402", 1.0, 0.5, 0.5),
        ("0603", 1.6, 0.8, 0.8),
        ("0805", 2.0, 1.25, 1.25),
        ("1206", 3.2, 1.6, 1.6),
        ("1210", 3.2, 2.5, 2.5),
        ("1812", 4.5, 3.2, 3.2),
        ("2010", 5.0, 2.5, 2.5),
        ("2512", 6.3, 3.2, 3.2),
        ("0204", 0.6, 0.4, 0.3),
    ];
    for (code, length, width, height) in &chip_sizes {
        if work == *code {
            return Ok((
                PackageFamily::Chip,
                2,
                0.0,
                *length,
                *width,
                *length,
                *height,
            ));
        }
    }

    if let Some(rest) = work.strip_prefix("SOT") {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let table: [(&str, u32, f64, f64, f64, f64, f64); 4] = [
            ("323", 3, 0.65, 2.0, 1.25, 2.2, 1.0),
            ("523", 5, 0.65, 1.7, 1.25, 1.9, 1.0),
            ("89", 3, 1.0, 4.5, 2.5, 6.2, 1.5),
            ("23", 3, 0.95, 2.9, 1.3, 2.8, 1.1),
        ];
        if let Some((code, pins, pitch, length, width, span, height)) =
            table.iter().find(|entry| digits.starts_with(entry.0))
        {
            let pin_count = digits[code.len()..]
                .parse::<u32>()
                .ok()
                .filter(|over| [3, 4, 5, 6, 8].contains(over))
                .unwrap_or(*pins);
            return Ok((
                PackageFamily::Sot,
                pin_count,
                *pitch,
                *length,
                *width,
                *span,
                *height,
            ));
        }
        return Err("unknown SOT code".to_string());
    }

    if let Some(prefix) = starts_with_any(&work, &["MSOP", "SSOP", "TSSOP", "SOIC", "SOP", "SO"]) {
        let after_prefix = prefix.len();
        let (pin_count, pitch, width, span, length, height) =
            if let Some((pins, _)) = first_digits(&work, after_prefix) {
                if pins % 2 != 0 || !(2..=56).contains(&pins) {
                    return Err("invalid pin count for SOIC/SOP/SO/MSOP/SSOP/TSSOP".to_string());
                }
                match prefix {
                    "SOIC" | "SOP" | "SO" => {
                        let length = ((pins / 2) as f64 - 1.0) * 1.27 + 2.4;
                        (pins, 1.27, 3.9, 6.0, length, 1.5)
                    }
                    "MSOP" => {
                        let length = ((pins / 2) as f64 - 1.0) * 0.65 + 2.0;
                        (pins, 0.65, 3.0, 5.0, length, 1.0)
                    }
                    "TSSOP" => {
                        let length = ((pins / 2) as f64 - 1.0) * 0.65 + 3.0;
                        (pins, 0.65, 4.4, 6.4, length, 1.0)
                    }
                    "SSOP" => {
                        let length = ((pins / 2) as f64 - 1.0) * 0.635 + 2.4;
                        (pins, 0.635, 3.9, 5.6, length, 1.1)
                    }
                    _ => {
                        let length = ((pins / 2) as f64 - 1.0) * 1.27 + 2.4;
                        (pins, 1.27, 3.9, 6.0, length, 1.5)
                    }
                }
            } else {
                return Err("missing pin count for SOIC/SOP/SO/MSOP/SSOP/TSSOP".to_string());
            };
        return Ok((
            PackageFamily::GullWing,
            pin_count,
            pitch,
            length,
            width,
            span,
            height,
        ));
    }

    if let Some(prefix) = starts_with_any(&work, &["LQFP", "TQFP", "QFP"]) {
        let after_prefix = prefix.len();
        if let Some((pins, _)) = first_digits(&work, after_prefix) {
            if pins % 4 != 0 || !(4..=60).contains(&(pins / 4)) {
                return Err("invalid pin count for QFP".to_string());
            }
            let per_side = pins / 4;
            let body_side = (per_side as f64 - 1.0) * 0.5 + 1.5;
            let span = body_side + 2.9;
            return Ok((
                PackageFamily::GullWing,
                pins,
                0.5,
                body_side,
                body_side,
                span,
                1.6,
            ));
        }
    }

    if let Some(prefix) = starts_with_any(&work, &["QFN", "DFN"]) {
        let after_prefix = prefix.len();
        if let Some((pins, _)) = first_digits(&work, after_prefix) {
            if !(4..=100).contains(&pins) {
                return Err("invalid pin count for QFN/DFN".to_string());
            }
            let body = ((pins / 4).max(1) as f64 - 1.0) * 0.5 + 2.0;
            let length = body.max(2.0);
            let span = body + 1.0;
            return Ok((PackageFamily::Qfn, pins, 0.5, length, length, span, 0.9));
        }
    }

    if let Some(prefix) = starts_with_any(&work, &["PDIP", "DIP", "SIP"]) {
        let after_prefix = prefix.len();
        if let Some((pins, _)) = first_digits(&work, after_prefix) {
            if !(4..=40).contains(&pins) {
                return Err("invalid pin count for DIP/SIP".to_string());
            }
            if prefix == "SIP" {
                let length = (pins as f64 - 1.0) * 2.54 + 2.0;
                return Ok((PackageFamily::Dip, pins, 2.54, length, 2.54, 2.54, 3.3));
            }
            let length = ((pins / 2) as f64 - 1.0) * 2.54 + 4.06;
            let span = length + 2.54;
            return Ok((PackageFamily::Dip, pins, 2.54, length, 6.35, span, 3.3));
        }
    }

    if work.contains("X") || work.contains("HEADER") || work.contains("HDR") {
        let mut rows: u32 = 1;
        let mut cols: u32 = 0;
        if let Some(pos) = work.find("X") {
            if let Some((r, _)) = first_digits(&work, 0) {
                rows = r;
            }
            if let Some((c, _)) = first_digits(&work, pos + 1) {
                cols = c;
            }
        } else if let Some((c, _)) = first_digits(&work, 0) {
            cols = c;
        }
        if cols == 0
            && let Some((c, _)) = first_digits(&work, work.len() - 1)
        {
            cols = c;
        }
        let pin_count = rows.checked_mul(cols).ok_or("pin count overflow")?;
        if !(1..=100).contains(&pin_count) {
            return Err("invalid pin count for pin header".to_string());
        }
        let body_length = rows.max(cols) as f64 * 2.54;
        let body_width = rows.min(cols) as f64 * 2.54;
        return Ok((
            PackageFamily::PinHeader,
            pin_count,
            2.54,
            body_length,
            body_width,
            body_length,
            2.54,
        ));
    }

    Err("package name is not recognised".to_string())
}
