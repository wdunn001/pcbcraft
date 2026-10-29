//! Initial RS-274X (Gerber X2 attributes) copper layer writer: pad flashes and
//! track draws only. No zones, vias, mask, paste, silkscreen or drill files yet.
//!
//! Everything fabrication-shaped lives here: the integer 4.6 coordinate
//! helper, the bounded decimal helper, apertures (standard bodies and outline
//! macros), the pad aperture builder, the shared X2 attribute header, and a
//! dependency-free MD5 for the TF.MD5 attribute. `gerber_copper` renders one
//! copper layer; `gerber_files` in lib.rs builds the whole fabrication
//! output. Both reuse the same aperture code, exactly as pad outlines
//! dictate: circles use C, axis-aligned rect and oval pads use R and O, and
//! every other pad (rotated rect/oval, roundrect, chamfered and custom
//! polygon pads) is flashed through an aperture macro with one outline
//! primitive (code 4) holding the pad's board-space outline relative to the
//! flash point. Chamfered and custom outlines are exact; roundrect corners
//! are chord approximated within `Pad::outline_error`.
use pcbcraft_doc::{Layer, Project};
use pcbcraft_geom::Point;
use pcbcraft_library::PadShape;
use std::collections::BTreeMap;

/// Millimetres or inches to 4.6 fixed-point coordinate integers.
pub(crate) fn coordinate(value: f64) -> i64 {
    (value * 1_000_000.0).round() as i64
}
/// A bounded decimal, trailing zeros trimmed, keeping six places at most.
pub(crate) fn decimal(value: f64) -> String {
    let text = format!("{value:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() {
        "0".into()
    } else {
        text.into()
    }
}

/// One flashable aperture: a standard circular, rectangular or oval aperture
/// body, or a single-primitive outline macro for everything else.
pub(crate) enum Aperture {
    Standard(String),
    Outline(Vec<Point>),
}
impl Aperture {
    pub(crate) fn key(&self) -> String {
        match self {
            Self::Standard(body) => body.clone(),
            Self::Outline(points) => points
                .iter()
                .map(|p| format!("{},{}", decimal(p.x), decimal(p.y)))
                .collect::<Vec<_>>()
                .join(","),
        }
    }
    /// The complete aperture definition, one line for a standard aperture and
    /// one macro plus its invocation for an outline aperture.
    pub(crate) fn definition(&self, code: u32) -> String {
        match self {
            Self::Standard(body) => format!("%ADD{code}{body}*%\n"),
            Self::Outline(points) => {
                // Outline primitive: exposure, vertex count after the start
                // point, the closed vertex list, then rotation.
                let mut ring = points.clone();
                if let Some(first) = points.first().copied() {
                    ring.push(first);
                }
                format!(
                    "%AMPAD{code}*\n4,1,{},{},0*%\n%ADD{code}PAD{code}*%\n",
                    ring.len().saturating_sub(1),
                    ring.iter()
                        .map(|v| format!("{},{}", decimal(v.x), decimal(v.y)))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
        }
    }
}

/// The aperture that flashes one pad at `center` in board space: standard C,
/// R or O for cheap cases, one outline macro for rotated pads, roundrects,
/// chamfers and custom polygons. `to_file` maps a board-space point into a
/// file-space point, so callers with a mirrored file pass their mirror. The
/// outline macro offsets are therefore produced in file-space as well.
pub(crate) fn pad_aperture(
    pad: &pcbcraft_doc::Pad,
    center: Point,
    footprint: &pcbcraft_doc::Footprint,
    to_file: impl Fn(Point) -> Point,
) -> Result<Aperture, String> {
    let rotation = footprint.rotation + pad.geometry.as_ref().map(|g| g.rotation).unwrap_or(0.0);
    let quarter = rotation.rem_euclid(90.0);
    let axis_aligned = quarter < 1e-9 || 90.0 - quarter < 1e-9;
    let swapped = (rotation.rem_euclid(180.0) - 90.0).abs() < 1e-9;
    let Some(geometry) = pad.geometry.as_ref() else {
        return Ok(Aperture::Standard(format!("C,{}", decimal(pad.diameter))));
    };
    Ok(match geometry.shape_kind() {
        PadShape::Circle => Aperture::Standard(format!("C,{}", decimal(geometry.size.x))),
        PadShape::Rect | PadShape::Oval if axis_aligned => {
            let (w, h) = if swapped {
                (geometry.size.y, geometry.size.x)
            } else {
                (geometry.size.x, geometry.size.y)
            };
            let letter = if geometry.shape_kind() == PadShape::Rect {
                "R"
            } else {
                "O"
            };
            Aperture::Standard(format!("{letter},{}X{}", decimal(w), decimal(h)))
        }
        PadShape::Unsupported => {
            return Err(format!(
                "pad {} has a shape the Gerber writer does not support",
                pad.number
            ));
        }
        _ => {
            let mut outline = pad.outline_points();
            outline.pop();
            let origin = to_file(center);
            Aperture::Outline(
                outline
                    .into_iter()
                    .map(|v| {
                        let board = footprint.position.offset(v.rotate(-footprint.rotation));
                        let file = to_file(board);
                        Point::new(file.x - origin.x, file.y - origin.y)
                    })
                    .collect(),
            )
        }
    })
}

/// Current UTC date and time in RFC 3339 form with second precision.
pub(crate) fn creation_date() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Civil calendar from a day count: the proleptic-Gregorian conversion,
    // integer division only, valid for any day within the supported range.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        (if month <= 2 { year + 1 } else { year }),
        month,
        day,
        rest / 3_600,
        (rest / 60) % 60,
        rest % 60
    )
}

/// The shared X2 attribute block for every fabrication file: generation
/// software, creation date, file function, polarity, same-coordinates and an
/// MD5 digest when the caller computed one. `same_coordinates` marks a file
/// that plotters share without being shifted, mirrored or rotated relative to
/// the others, exactly as the writer produced it.
pub(crate) fn x2_header(
    file_function: &str,
    polarity: Option<&str>,
    same_coordinates: bool,
    md5: Option<&str>,
) -> String {
    let mut out = String::from("%TF.GenerationSoftware,Tangency Labs,PCBCraft*%\n");
    out.push_str(&format!("%TF.CreationDate,{}*%\n", creation_date()));
    out.push_str(&format!("%TF.FileFunction,{file_function}*%\n"));
    if let Some(polarity) = polarity {
        out.push_str(&format!("%TF.FilePolarity,{polarity}*%\n"));
    }
    if same_coordinates {
        out.push_str("%TF.SameCoordinates,PCBCraft*%\n");
    }
    if let Some(md5) = md5 {
        out.push_str(&format!("%TF.MD5,{md5}*%\n"));
    }
    out
}

/// RFC 1321 MD5, the public-domain reference algorithm, so no dependency is
/// added for one attribute. Every add wraps: the digest is defined on
/// unsigned word arithmetic that overflows by design.
pub(crate) fn md5_hex(message: &[u8]) -> String {
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    #[rustfmt::skip]
    const SINE_TABLE: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    let mut state: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    let mut padded = message.to_vec();
    let bits = (message.len() as u64).wrapping_mul(8);
    padded.push(0x80);
    let zeros = (120 - padded.len() % 64) % 64;
    padded.extend(vec![0; zeros]);
    padded.extend_from_slice(&bits.to_le_bytes());
    for block in padded.chunks(64) {
        let mut words = [0u32; 16];
        for (round, word) in words.iter_mut().enumerate() {
            *word = u32::from_le_bytes([
                block[round * 4],
                block[round * 4 + 1],
                block[round * 4 + 2],
                block[round * 4 + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (state[0], state[1], state[2], state[3]);
        for step in 0..64 {
            let (mixed, index) = match step / 16 {
                0 => ((b & c) | (!b & d), step),
                1 => ((d & b) | (!d & c), (5 * step + 1) % 16),
                2 => (b ^ c ^ d, (3 * step + 5) % 16),
                _ => (c ^ (b | !d), (7 * step) % 16),
            };
            let rotated = a
                .wrapping_add(mixed)
                .wrapping_add(SINE_TABLE[step])
                .wrapping_add(words[index])
                .rotate_left(SHIFTS[step]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(rotated);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }
    let mut hex = String::with_capacity(32);
    for word in state {
        for byte in word.to_le_bytes() {
            hex.push_str(&format!("{byte:02x}"));
        }
    }
    hex
}

/// Gerber copper image of one layer. Board coordinates are y-down from the
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
        definitions.push_str(&aperture.definition(code as u32));
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
            let aperture = pad_aperture(pad, center, f, to_gerber)?;
            let code = define(aperture);
            select(&mut body, code);
            let at = to_gerber(center);
            body.push_str(&format!("X{}Y{}D03*\n", coordinate(at.x), coordinate(at.y)));
        }
    }
    let (index, side) = match layer {
        Layer::Front => (1, "Top"),
        Layer::Back => (2, "Bot"),
    };
    Ok(format!(
        "%TF.GenerationSoftware,Tangency Labs,PCBCraft*%\n%TF.FileFunction,Copper,L{index},{side}*%\n%TF.FilePolarity,Positive*%\n%FSLAX46Y46*%\n%MOMM*%\n%LPD*%\nG01*\n{definitions}{body}M02*\n"
    ))
}

/// The MD5 digest is checked against the published reference vectors, so the
/// TF.MD5 attribute is a truthful fingerprint of the file.
#[cfg(test)]
mod md5_tests {
    use super::md5_hex;

    #[test]
    fn md5_matches_the_published_reference_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5_hex(b"message digest"),
            "f96b697d7cb7938d525a2f31aaf161d0"
        );
    }
}
