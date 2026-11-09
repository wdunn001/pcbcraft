//! Pad kinds: plain, exposed and notched, and the notch geometry.
//!
//! Context: issue 1209. A pad kind is read from the pad data that is already
//! stored and written to every output (layers and custom polygon), so saved
//! designs, the fabrication writers and the checks need no new field. This file
//! names the kinds, classifies a pad, and builds the outline of a notched pad.
//!
//! # Rules
//! - Notched means a `custom` pad with a polygon; exposed means a surface pad
//!   with an open mask and no paste layer; everything else is plain.
//! - A notch is cut from a rectangle and stays at most half the rectangle, so
//!   the centred anchor rectangle that custom pads need always fits inside.
//! - Every bound is checked and reported as text; nothing here panics.
//!
//! # Dependencies
//! - crates/library/src/lib.rs (`FootprintPad`)
//! - crates/library/src/validation.rs (`custom_polygon_error`, the anchor rule)
//!
//! # Do not
//! - Store the kind in a second field; derive it so it cannot disagree.
use crate::FootprintPad;
use pcbcraft_geom::Point;

/// Smallest notch width, depth or remaining arm, in millimetres.
pub const MIN_NOTCH_MM: f64 = 0.05;

/// What a pad is for, read from its stored layers and shape.
///
/// # Rules
/// - `Notched` wins over `Exposed` because the outline is the larger fact.
/// - `Exposed` is a surface pad with an open mask and no paste layer.
/// - `Plain` is every other pad, including a plated through-hole pad.
///
/// # Dependencies
/// - crates/library/src/pad_kind.rs (`FootprintPad::kind`)
///
/// # Do not
/// - Add a variant without a place in the Layout pad controls and the writers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadKind {
    /// An ordinary pad.
    Plain,
    /// Bare copper with an open mask and no paste.
    Exposed,
    /// A rectangle with one corner or edge cut out.
    Notched,
}

impl PadKind {
    /// The word the commands and the pad table use for this kind.
    pub fn word(self) -> &'static str {
        match self {
            PadKind::Plain => "plain",
            PadKind::Exposed => "exposed",
            PadKind::Notched => "notched",
        }
    }

    /// The kind a command word names, or `None` for any other text.
    pub fn from_word(word: &str) -> Option<PadKind> {
        match word {
            "plain" => Some(PadKind::Plain),
            "exposed" => Some(PadKind::Exposed),
            "notched" => Some(PadKind::Notched),
            _ => None,
        }
    }
}

/// Where a notch sits on the rectangle it is cut from, in y-down pad space.
///
/// # Rules
/// - A corner notch removes a `width` by `depth` block (x by y) from that corner.
/// - An edge notch removes a block centred on that edge; `width` runs along the
///   edge and `depth` runs into the pad.
///
/// # Dependencies
/// - crates/library/src/pad_kind.rs (`notched_rectangle`)
///
/// # Do not
/// - Reorder the variants; the Layout pad list shows them in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotchPlace {
    /// The top left corner.
    CornerNw,
    /// The top right corner.
    CornerNe,
    /// The bottom right corner.
    CornerSe,
    /// The bottom left corner.
    CornerSw,
    /// The middle of the top edge.
    EdgeN,
    /// The middle of the right edge.
    EdgeE,
    /// The middle of the bottom edge.
    EdgeS,
    /// The middle of the left edge.
    EdgeW,
}

impl NotchPlace {
    /// Every place, in the order the Layout list shows them.
    pub const ALL: [NotchPlace; 8] = [
        NotchPlace::CornerNw,
        NotchPlace::CornerNe,
        NotchPlace::CornerSe,
        NotchPlace::CornerSw,
        NotchPlace::EdgeN,
        NotchPlace::EdgeE,
        NotchPlace::EdgeS,
        NotchPlace::EdgeW,
    ];

    /// The word the commands use for this place.
    pub fn word(self) -> &'static str {
        match self {
            NotchPlace::CornerNw => "corner_nw",
            NotchPlace::CornerNe => "corner_ne",
            NotchPlace::CornerSe => "corner_se",
            NotchPlace::CornerSw => "corner_sw",
            NotchPlace::EdgeN => "edge_n",
            NotchPlace::EdgeE => "edge_e",
            NotchPlace::EdgeS => "edge_s",
            NotchPlace::EdgeW => "edge_w",
        }
    }

    /// The place a command word names, or `None` for any other text.
    pub fn from_word(word: &str) -> Option<NotchPlace> {
        NotchPlace::ALL.into_iter().find(|p| p.word() == word)
    }
}

impl FootprintPad {
    /// True when a paste layer (front, back or all) is listed.
    pub fn has_paste(&self) -> bool {
        self.layers.iter().any(|l| l.ends_with(".Paste"))
    }

    /// True when a mask layer (front, back or all) is listed, so the mask is open.
    pub fn mask_open(&self) -> bool {
        self.layers.iter().any(|l| l.ends_with(".Mask"))
    }

    /// The kind of this pad, read from its shape, layers and drill.
    ///
    /// # Rules
    /// - A `custom` pad with a polygon is notched.
    /// - A surface pad with an open mask and no paste is exposed.
    /// - A drilled pad that is not custom is plain.
    ///
    /// # Dependencies
    /// - crates/library/src/pad_kind.rs (`has_paste`, `mask_open`)
    ///
    /// # Do not
    /// - Call a pad with no layers exposed; a legacy pad is plain.
    pub fn kind(&self) -> PadKind {
        if self.shape == "custom" && !self.polygon.is_empty() {
            PadKind::Notched
        } else if self.drill.is_none() && self.mask_open() && !self.has_paste() {
            PadKind::Exposed
        } else {
            PadKind::Plain
        }
    }
}

impl FootprintPad {
    /// The place, width and depth of the notch a notched pad was cut with.
    ///
    /// # Rules
    /// - Returns `None` for a pad that is not notched, or whose polygon was
    ///   not made by `notched_rectangle`.
    /// - The match is by outline, so a saved design reads back the same notch.
    ///
    /// # Dependencies
    /// - crates/library/src/pad_kind.rs (`notched_rectangle`)
    ///
    /// # Do not
    /// - Use this to validate a polygon; use `custom_polygon_error`.
    pub fn notch(&self) -> Option<(NotchPlace, f64, f64)> {
        if self.kind() != PadKind::Notched || self.polygon.len() > 8 {
            return None;
        }
        let xs = self.polygon.iter().map(|v| v.x);
        let ys = self.polygon.iter().map(|v| v.y);
        let (min_x, max_x) = xs.fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
        let (min_y, max_y) = ys.fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
        let (width, height) = (max_x - min_x, max_y - min_y);
        let mut candidates = vec![];
        for v in &self.polygon {
            for c in [
                width / 2.0 - v.x.abs(),
                2.0 * v.x.abs(),
                height / 2.0 - v.y.abs(),
                2.0 * v.y.abs(),
            ] {
                if c >= MIN_NOTCH_MM {
                    candidates.push(c);
                }
            }
        }
        let same = |a: &[Point], b: &[Point]| {
            a.len() == b.len()
                && a.iter().all(|p| {
                    b.iter()
                        .any(|q| (p.x - q.x).abs() < 1e-6 && (p.y - q.y).abs() < 1e-6)
                })
        };
        for place in NotchPlace::ALL {
            for &nw in &candidates {
                for &nd in &candidates {
                    if let Ok((ring, _)) = notched_rectangle(width, height, place, nw, nd)
                        && same(&ring, &self.polygon)
                    {
                        return Some((place, nw, nd));
                    }
                }
            }
        }
        None
    }
}

/// The pad-local polygon of a `width` by `height` rectangle with one notch cut
/// out, and the centred anchor rectangle that fits inside it.
///
/// # Rules
/// - `notch_width` and `notch_depth` are at least `MIN_NOTCH_MM`.
/// - An edge notch leaves at least `MIN_NOTCH_MM` of copper on each side of it
///   and is at most half the pad deep.
/// - A corner notch leaves an anchor of at least `MIN_NOTCH_MM` in x or in y.
///
/// # Dependencies
/// - crates/library/src/validation.rs (`custom_polygon_error` checks the result)
///
/// # Do not
/// - Pass a polygon built here to a writer without the pad validation.
pub fn notched_rectangle(
    width: f64,
    height: f64,
    place: NotchPlace,
    notch_width: f64,
    notch_depth: f64,
) -> Result<(Vec<Point>, Point), String> {
    let finite = [width, height, notch_width, notch_depth]
        .iter()
        .all(|v| v.is_finite());
    if !finite || width < MIN_NOTCH_MM || height < MIN_NOTCH_MM {
        return Err("the pad needs a finite width and height".into());
    }
    if notch_width < MIN_NOTCH_MM || notch_depth < MIN_NOTCH_MM {
        return Err("a notch is at least 0.05 millimetres wide and deep".into());
    }
    let (hx, hy) = (width / 2.0, height / 2.0);
    match place {
        NotchPlace::CornerNw
        | NotchPlace::CornerNe
        | NotchPlace::CornerSe
        | NotchPlace::CornerSw => {
            let wide = width - 2.0 * notch_width;
            let tall = height - 2.0 * notch_depth;
            if notch_width >= width
                || notch_depth >= height
                || (wide < MIN_NOTCH_MM && tall < MIN_NOTCH_MM)
            {
                return Err("the notch is too large for the pad".into());
            }
            let anchor = if wide * height >= width * tall {
                Point::new(wide, height)
            } else {
                Point::new(width, tall)
            };
            let (sx, sy) = match place {
                NotchPlace::CornerNw => (1.0, 1.0),
                NotchPlace::CornerNe => (-1.0, 1.0),
                NotchPlace::CornerSe => (-1.0, -1.0),
                _ => (1.0, -1.0),
            };
            let base = [
                Point::new(-hx + notch_width, -hy),
                Point::new(hx, -hy),
                Point::new(hx, hy),
                Point::new(-hx, hy),
                Point::new(-hx, -hy + notch_depth),
                Point::new(-hx + notch_width, -hy + notch_depth),
            ];
            let ring = base
                .iter()
                .map(|p| Point::new(p.x * sx, p.y * sy))
                .collect();
            Ok((ring, anchor))
        }
        _ => {
            let sideways = matches!(place, NotchPlace::EdgeE | NotchPlace::EdgeW);
            // Build a north notch on the pad turned to match, then turn it back.
            let (a, b) = if sideways { (hy, hx) } else { (hx, hy) };
            if notch_width > 2.0 * a - 2.0 * MIN_NOTCH_MM || notch_depth > b - MIN_NOTCH_MM / 2.0 {
                return Err("the notch is too large for the pad".into());
            }
            let w = notch_width / 2.0;
            let north = [
                Point::new(-a, -b),
                Point::new(-w, -b),
                Point::new(-w, -b + notch_depth),
                Point::new(w, -b + notch_depth),
                Point::new(w, -b),
                Point::new(a, -b),
                Point::new(a, b),
                Point::new(-a, b),
            ];
            let turn = |p: &Point| match place {
                NotchPlace::EdgeN => *p,
                NotchPlace::EdgeS => Point::new(p.x, -p.y),
                NotchPlace::EdgeE => Point::new(-p.y, p.x),
                _ => Point::new(p.y, -p.x),
            };
            let ring: Vec<Point> = north.iter().map(turn).collect();
            let anchor = if sideways {
                Point::new(width - 2.0 * notch_depth, height)
            } else {
                Point::new(width, height - 2.0 * notch_depth)
            };
            let anchor = Point::new(anchor.x.max(MIN_NOTCH_MM), anchor.y.max(MIN_NOTCH_MM));
            Ok((ring, anchor))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom_polygon_error;

    fn pad(shape: &str, layers: &[&str], polygon: Vec<Point>) -> FootprintPad {
        FootprintPad {
            number: "1".into(),
            pad_type: "smd".into(),
            shape: shape.into(),
            size: Point::new(1.0, 1.0),
            layers: layers.iter().map(|l| (*l).to_owned()).collect(),
            polygon,
            ..FootprintPad::default()
        }
    }

    #[test]
    fn kinds_are_read_from_layers_and_shape() {
        let plain = pad("rect", &["F.Cu", "F.Paste", "F.Mask"], vec![]);
        assert_eq!(plain.kind(), PadKind::Plain);
        let exposed = pad("rect", &["F.Cu", "F.Mask"], vec![]);
        assert_eq!(exposed.kind(), PadKind::Exposed);
        let covered = pad("rect", &["F.Cu"], vec![]);
        assert_eq!(
            covered.kind(),
            PadKind::Plain,
            "a covered pad is not exposed"
        );
        let notched = pad("custom", &["F.Cu", "F.Mask"], vec![Point::new(0.0, 0.0)]);
        assert_eq!(notched.kind(), PadKind::Notched);
    }

    #[test]
    fn every_place_makes_a_valid_custom_polygon() -> Result<(), String> {
        for place in NotchPlace::ALL {
            let (ring, anchor) = notched_rectangle(2.0, 1.4, place, 0.6, 0.4)?;
            assert_eq!(
                custom_polygon_error(anchor, &ring),
                None,
                "{} makes a usable polygon",
                place.word()
            );
            assert_eq!(NotchPlace::from_word(place.word()), Some(place));
            let mut read = pad("custom", &["F.Cu", "F.Mask"], ring);
            read.size = anchor;
            assert_eq!(
                read.notch()
                    .map(|(p, w, d)| (p, (w * 100.0).round(), (d * 100.0).round())),
                Some((place, 60.0, 40.0)),
                "{} reads back as it was cut",
                place.word()
            );
        }
        Ok(())
    }

    #[test]
    fn a_notch_that_eats_the_pad_is_refused() {
        assert!(notched_rectangle(1.0, 1.0, NotchPlace::EdgeN, 0.98, 0.2).is_err());
        assert!(notched_rectangle(1.0, 1.0, NotchPlace::EdgeN, 0.2, 0.9).is_err());
        assert!(notched_rectangle(1.0, 1.0, NotchPlace::CornerNw, 0.6, 0.6).is_err());
        assert!(notched_rectangle(1.0, 1.0, NotchPlace::CornerNw, 0.0, 0.2).is_err());
        assert!(notched_rectangle(f64::NAN, 1.0, NotchPlace::CornerNw, 0.2, 0.2).is_err());
    }
}
