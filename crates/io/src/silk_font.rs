#![allow(dead_code)]
//! Stroke-based silkscreen font using seven-segment digit shapes.
//!
//! This module provides a minimal font for rendering text as line strokes
//! suitable for Gerber silkscreen output. Digits use seven-segment style
//! shapes; a small set of symbols is supported; unknown characters fall
//! back to a box outline.
//!
//! # Rules
//! - Cell width is 0.6 * size, advance is 0.8 * size, height is size.
//! - Text is drawn from the anchor toward +x and +y, then rotated.
//! - No std dependencies; wasm-compatible.
//!
//! # Dependencies
//! - crates/geom/src/lib.rs (Point and its rotate method)
//!
//! # Do not
//! - Add true bitmap or vector font data; keep it to line segments only.

use pcbcraft_geom::Point;

/// Width of one character cell in the stroke font, as a multiple of the text size.
const CELL_WIDTH: f64 = 0.6;
/// Horizontal advance per character, as a multiple of the text size.
const TEXT_ADVANCE: f64 = 0.8;

/// A unit-cell segment: start and end points in 0..=1 coordinates.
type UnitSegment = ((f64, f64), (f64, f64));

/// Digit zero: six segments (all except the middle).
const ZERO: [UnitSegment; 6] = [
    ((0.0, 0.0), (1.0, 0.0)), // top
    ((0.0, 0.0), (0.0, 0.5)), // top-left
    ((1.0, 0.0), (1.0, 0.5)), // top-right
    ((0.0, 0.5), (0.0, 1.0)), // bottom-left
    ((1.0, 0.5), (1.0, 1.0)), // bottom-right
    ((0.0, 1.0), (1.0, 1.0)), // bottom
];

/// Digit one: a single vertical bar at the center.
const ONE: [UnitSegment; 1] = [((0.5, 0.0), (0.5, 1.0))];

/// Digit two: top, top-right, middle, bottom-left, bottom.
const TWO: [UnitSegment; 5] = [
    ((0.0, 0.0), (1.0, 0.0)), // top
    ((1.0, 0.0), (1.0, 0.5)), // top-right
    ((0.0, 0.5), (1.0, 0.5)), // middle
    ((0.0, 0.5), (0.0, 1.0)), // bottom-left
    ((0.0, 1.0), (1.0, 1.0)), // bottom
];

/// Digit three: top, top-right, middle, bottom-right, bottom.
const THREE: [UnitSegment; 5] = [
    ((0.0, 0.0), (1.0, 0.0)), // top
    ((1.0, 0.0), (1.0, 0.5)), // top-right
    ((0.0, 0.5), (1.0, 0.5)), // middle
    ((1.0, 0.5), (1.0, 1.0)), // bottom-right
    ((0.0, 1.0), (1.0, 1.0)), // bottom
];

/// Digit four: top-left, top-right, middle, bottom-right.
const FOUR: [UnitSegment; 4] = [
    ((0.0, 0.0), (0.0, 0.5)), // top-left
    ((1.0, 0.0), (1.0, 0.5)), // top-right
    ((0.0, 0.5), (1.0, 0.5)), // middle
    ((1.0, 0.5), (1.0, 1.0)), // bottom-right
];

/// Digit five: top, top-left, middle, bottom-right, bottom.
const FIVE: [UnitSegment; 5] = [
    ((0.0, 0.0), (1.0, 0.0)), // top
    ((0.0, 0.0), (0.0, 0.5)), // top-left
    ((0.0, 0.5), (1.0, 0.5)), // middle
    ((1.0, 0.5), (1.0, 1.0)), // bottom-right
    ((0.0, 1.0), (1.0, 1.0)), // bottom
];

/// Digit six: top, top-left, middle, bottom-left, bottom-right, bottom.
const SIX: [UnitSegment; 6] = [
    ((0.0, 0.0), (1.0, 0.0)), // top
    ((0.0, 0.0), (0.0, 0.5)), // top-left
    ((0.0, 0.5), (1.0, 0.5)), // middle
    ((0.0, 0.5), (0.0, 1.0)), // bottom-left
    ((1.0, 0.5), (1.0, 1.0)), // bottom-right
    ((0.0, 1.0), (1.0, 1.0)), // bottom
];

/// Digit seven: top, top-right, bottom-right.
const SEVEN: [UnitSegment; 3] = [
    ((0.0, 0.0), (1.0, 0.0)), // top
    ((1.0, 0.0), (1.0, 0.5)), // top-right
    ((1.0, 0.5), (1.0, 1.0)), // bottom-right
];

/// Digit eight: all seven segments.
const EIGHT: [UnitSegment; 7] = [
    ((0.0, 0.0), (1.0, 0.0)), // top
    ((0.0, 0.0), (0.0, 0.5)), // top-left
    ((1.0, 0.0), (1.0, 0.5)), // top-right
    ((0.0, 0.5), (1.0, 0.5)), // middle
    ((0.0, 0.5), (0.0, 1.0)), // bottom-left
    ((1.0, 0.5), (1.0, 1.0)), // bottom-right
    ((0.0, 1.0), (1.0, 1.0)), // bottom
];

/// Digit nine: top, top-left, top-right, middle, bottom-right, bottom.
const NINE: [UnitSegment; 6] = [
    ((0.0, 0.0), (1.0, 0.0)), // top
    ((0.0, 0.0), (0.0, 0.5)), // top-left
    ((1.0, 0.0), (1.0, 0.5)), // top-right
    ((0.0, 0.5), (1.0, 0.5)), // middle
    ((1.0, 0.5), (1.0, 1.0)), // bottom-right
    ((0.0, 1.0), (1.0, 1.0)), // bottom
];

/// Dash: a horizontal line at half height.
const DASH: [UnitSegment; 1] = [((0.0, 0.5), (1.0, 0.5))];

/// Dot: a small square at the bottom center.
const DOT: [UnitSegment; 4] = [
    ((0.4, 0.9), (0.6, 0.9)),
    ((0.6, 0.9), (0.6, 1.0)),
    ((0.6, 1.0), (0.4, 1.0)),
    ((0.4, 1.0), (0.4, 0.9)),
];

/// Underscore: a horizontal line at the bottom.
const UNDERSCORE: [UnitSegment; 1] = [((0.0, 1.0), (1.0, 1.0))];

/// Plus sign: a cross at the center.
const PLUS: [UnitSegment; 2] = [((0.0, 0.5), (1.0, 0.5)), ((0.5, 0.0), (0.5, 1.0))];

/// Slash: a diagonal from top-right to bottom-left.
const SLASH: [UnitSegment; 1] = [((1.0, 0.0), (0.0, 1.0))];

// Letter glyphs (uppercase; lowercase maps to uppercase).
const LETTER_A: [UnitSegment; 3] = [
    ((0.0, 1.0), (0.5, 0.0)),
    ((0.5, 0.0), (1.0, 1.0)),
    ((0.2, 0.5), (0.8, 0.5)),
];
const LETTER_B: [UnitSegment; 6] = [
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 0.0), (0.8, 0.0)),
    ((0.8, 0.0), (0.8, 0.5)),
    ((0.0, 0.5), (0.8, 0.5)),
    ((0.8, 0.5), (0.8, 1.0)),
    ((0.0, 1.0), (0.8, 1.0)),
];
const LETTER_C: [UnitSegment; 3] = [
    ((1.0, 0.0), (0.0, 0.0)),
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 1.0), (1.0, 1.0)),
];
const LETTER_D: [UnitSegment; 4] = [
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 0.0), (0.8, 0.0)),
    ((0.8, 0.0), (0.8, 1.0)),
    ((0.8, 1.0), (0.0, 1.0)),
];
const LETTER_E: [UnitSegment; 4] = [
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 0.0), (1.0, 0.0)),
    ((0.0, 0.5), (1.0, 0.5)),
    ((0.0, 1.0), (1.0, 1.0)),
];
const LETTER_F: [UnitSegment; 3] = [
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 0.0), (1.0, 0.0)),
    ((0.0, 0.5), (1.0, 0.5)),
];
const LETTER_G: [UnitSegment; 5] = [
    ((1.0, 0.0), (0.0, 0.0)),
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 1.0), (1.0, 1.0)),
    ((1.0, 1.0), (1.0, 0.5)),
    ((1.0, 0.5), (0.5, 0.5)),
];
const LETTER_H: [UnitSegment; 3] = [
    ((0.0, 0.0), (0.0, 1.0)),
    ((1.0, 0.0), (1.0, 1.0)),
    ((0.0, 0.5), (1.0, 0.5)),
];
const LETTER_I: [UnitSegment; 3] = [
    ((0.0, 0.0), (1.0, 0.0)),
    ((0.5, 0.0), (0.5, 1.0)),
    ((0.0, 1.0), (1.0, 1.0)),
];
const LETTER_J: [UnitSegment; 3] = [
    ((0.0, 0.0), (1.0, 0.0)),
    ((1.0, 0.0), (1.0, 1.0)),
    ((1.0, 1.0), (0.5, 1.0)),
];
const LETTER_K: [UnitSegment; 3] = [
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 0.5), (1.0, 0.0)),
    ((0.0, 0.5), (1.0, 1.0)),
];
const LETTER_L: [UnitSegment; 2] = [((0.0, 0.0), (0.0, 1.0)), ((0.0, 1.0), (1.0, 1.0))];
const LETTER_M: [UnitSegment; 4] = [
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 0.0), (0.5, 1.0)),
    ((0.5, 1.0), (1.0, 0.0)),
    ((1.0, 0.0), (1.0, 1.0)),
];
const LETTER_N: [UnitSegment; 3] = [
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 0.0), (1.0, 1.0)),
    ((1.0, 0.0), (1.0, 1.0)),
];
// O reuses the ZERO box shape.
const LETTER_P: [UnitSegment; 4] = [
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 0.0), (1.0, 0.0)),
    ((1.0, 0.0), (1.0, 0.5)),
    ((0.0, 0.5), (1.0, 0.5)),
];
const LETTER_Q: [UnitSegment; 5] = [
    ((0.0, 0.0), (1.0, 0.0)),
    ((1.0, 0.0), (1.0, 1.0)),
    ((1.0, 1.0), (0.0, 1.0)),
    ((0.0, 1.0), (0.0, 0.0)),
    ((0.7, 0.7), (1.0, 1.0)),
];
const LETTER_R: [UnitSegment; 5] = [
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 0.0), (1.0, 0.0)),
    ((1.0, 0.0), (1.0, 0.5)),
    ((0.0, 0.5), (1.0, 0.5)),
    ((1.0, 0.5), (1.0, 1.0)),
];
const LETTER_S: [UnitSegment; 5] = [
    ((1.0, 0.0), (0.0, 0.0)),
    ((0.0, 0.0), (0.0, 0.5)),
    ((0.0, 0.5), (1.0, 0.5)),
    ((1.0, 0.5), (1.0, 1.0)),
    ((1.0, 1.0), (0.0, 1.0)),
];
const LETTER_T: [UnitSegment; 2] = [((0.0, 0.0), (1.0, 0.0)), ((0.5, 0.0), (0.5, 1.0))];
const LETTER_U: [UnitSegment; 3] = [
    ((0.0, 0.0), (0.0, 1.0)),
    ((0.0, 1.0), (1.0, 1.0)),
    ((1.0, 1.0), (1.0, 0.0)),
];
const LETTER_V: [UnitSegment; 2] = [((0.0, 0.0), (0.5, 1.0)), ((0.5, 1.0), (1.0, 0.0))];
const LETTER_W: [UnitSegment; 4] = [
    ((0.0, 0.0), (0.25, 1.0)),
    ((0.25, 1.0), (0.5, 0.0)),
    ((0.5, 0.0), (0.75, 1.0)),
    ((0.75, 1.0), (1.0, 0.0)),
];
const LETTER_X: [UnitSegment; 2] = [((0.0, 0.0), (1.0, 1.0)), ((1.0, 0.0), (0.0, 1.0))];
const LETTER_Y: [UnitSegment; 3] = [
    ((0.0, 0.0), (0.5, 0.5)),
    ((1.0, 0.0), (0.5, 0.5)),
    ((0.5, 0.5), (0.5, 1.0)),
];
const LETTER_Z: [UnitSegment; 3] = [
    ((0.0, 0.0), (1.0, 0.0)),
    ((1.0, 0.0), (0.0, 1.0)),
    ((0.0, 1.0), (1.0, 1.0)),
];

/// Fallback box for unknown characters.
const BOX: [UnitSegment; 4] = [
    ((0.0, 0.0), (1.0, 0.0)),
    ((1.0, 0.0), (1.0, 1.0)),
    ((1.0, 1.0), (0.0, 1.0)),
    ((0.0, 1.0), (0.0, 0.0)),
];

/// Convert a unit-cell segment to a design-coordinate stroke at the given position and size.
fn segment_to_stroke(seg: UnitSegment, origin: Point, size: f64) -> (Point, Point) {
    let (start, end) = seg;
    let start_pt = origin.offset(Point::new(start.0 * CELL_WIDTH * size, start.1 * size));
    let end_pt = origin.offset(Point::new(end.0 * CELL_WIDTH * size, end.1 * size));
    (start_pt, end_pt)
}

/// Get the unit-cell segments for a single character.
fn char_segments(ch: char) -> &'static [UnitSegment] {
    match ch {
        '0' => &ZERO,
        '1' => &ONE,
        '2' => &TWO,
        '3' => &THREE,
        '4' => &FOUR,
        '5' => &FIVE,
        '6' => &SIX,
        '7' => &SEVEN,
        '8' => &EIGHT,
        '9' => &NINE,
        'A' | 'a' => &LETTER_A,
        'B' | 'b' => &LETTER_B,
        'C' | 'c' => &LETTER_C,
        'D' | 'd' => &LETTER_D,
        'E' | 'e' => &LETTER_E,
        'F' | 'f' => &LETTER_F,
        'G' | 'g' => &LETTER_G,
        'H' | 'h' => &LETTER_H,
        'I' | 'i' => &LETTER_I,
        'J' | 'j' => &LETTER_J,
        'K' | 'k' => &LETTER_K,
        'L' | 'l' => &LETTER_L,
        'M' | 'm' => &LETTER_M,
        'N' | 'n' => &LETTER_N,
        'O' | 'o' => &ZERO,
        'P' | 'p' => &LETTER_P,
        'Q' | 'q' => &LETTER_Q,
        'R' | 'r' => &LETTER_R,
        'S' | 's' => &LETTER_S,
        'T' | 't' => &LETTER_T,
        'U' | 'u' => &LETTER_U,
        'V' | 'v' => &LETTER_V,
        'W' | 'w' => &LETTER_W,
        'X' | 'x' => &LETTER_X,
        'Y' | 'y' => &LETTER_Y,
        'Z' | 'z' => &LETTER_Z,
        '-' => &DASH,
        '.' => &DOT,
        '_' => &UNDERSCORE,
        '+' => &PLUS,
        '/' => &SLASH,
        ' ' => &[],
        _ => &BOX,
    }
}

/// Convert text to a list of design-coordinate strokes for silkscreen output.
///
/// Each character is drawn in a cell of width 0.6 * size and height size,
/// with horizontal advance of 0.8 * size. The text is positioned from the
/// anchor point toward +x and +y, then rotated about the anchor.
///
/// # Rules
/// - Returns Vec of (start, end) points in design coordinates.
/// - Unknown characters fall back to a box outline.
/// - Rotation is in degrees, counter-clockwise.
pub(crate) fn text_strokes(
    text: &str,
    at: Point,
    size: f64,
    rotation_deg: f64,
) -> Vec<(Point, Point)> {
    let mut strokes = Vec::new();
    let mut origin = at;

    for ch in text.chars() {
        let segs = char_segments(ch);
        for seg in segs {
            let (start, end) = segment_to_stroke(*seg, origin, size);
            // Rotate about the anchor point
            let rel_start = Point::new(start.x - at.x, start.y - at.y);
            let rotated_start = rel_start.rotate(rotation_deg).offset(at);
            let rel_end = Point::new(end.x - at.x, end.y - at.y);
            let rotated_end = rel_end.rotate(rotation_deg).offset(at);
            strokes.push((rotated_start, rotated_end));
        }
        // Advance to next character position
        origin = origin.offset(Point::new(TEXT_ADVANCE * size, 0.0));
    }

    strokes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_one_is_one_vertical_stroke_from_the_baseline_to_the_cap_height() {
        let strokes = text_strokes("1", Point::new(10.0, 10.0), 2.0, 0.0);
        assert_eq!(strokes.len(), 1);
        let (start, end) = strokes[0];
        assert!((start.x - 10.6).abs() < 0.001);
        assert!((end.x - 10.6).abs() < 0.001);
        assert!((start.y - 10.0).abs() < 0.001 || (end.y - 10.0).abs() < 0.001);
        assert!((start.y - 12.0).abs() < 0.001 || (end.y - 12.0).abs() < 0.001);
    }

    #[test]
    fn a_dash_is_one_horizontal_stroke_at_half_height() {
        let strokes = text_strokes("-", Point::new(0.0, 0.0), 2.0, 0.0);
        assert_eq!(strokes.len(), 1);
        let (start, end) = strokes[0];
        assert!((start.y - 1.0).abs() < 0.001);
        assert!((end.y - 1.0).abs() < 0.001);
        assert!((start.x - 0.0).abs() < 0.001 || (end.x - 0.0).abs() < 0.001);
        assert!((start.x - 1.2).abs() < 0.001 || (end.x - 1.2).abs() < 0.001);
    }

    #[test]
    fn the_second_glyph_starts_one_advance_to_the_right() {
        let strokes = text_strokes("11", Point::new(0.0, 0.0), 2.0, 0.0);
        assert_eq!(strokes.len(), 2);
        let (start1, _) = strokes[0];
        let (start2, _) = strokes[1];
        assert!((start1.x - 0.6).abs() < 0.001);
        assert!((start2.x - 2.2).abs() < 0.001);
    }

    #[test]
    fn rotation_turns_the_text_about_the_anchor() {
        let strokes = text_strokes("-", Point::new(0.0, 0.0), 2.0, 90.0);
        assert_eq!(strokes.len(), 1);
        let (start, end) = strokes[0];
        assert!((start.x - end.x).abs() < 0.001);
        assert!((start.y - end.y).abs() > 1.0);
    }

    #[test]
    fn unknown_characters_fall_back_to_a_box() {
        let strokes = text_strokes("~", Point::new(0.0, 0.0), 2.0, 0.0);
        assert_eq!(strokes.len(), 4);
    }

    #[test]
    fn the_eight_has_seven_segments_and_the_zero_has_six() {
        let eight = text_strokes("8", Point::new(0.0, 0.0), 2.0, 0.0);
        assert_eq!(eight.len(), 7);
        let zero = text_strokes("0", Point::new(0.0, 0.0), 2.0, 0.0);
        assert_eq!(zero.len(), 6);
    }

    #[test]
    fn the_letter_l_is_a_vertical_and_a_horizontal_stroke() {
        let strokes = text_strokes("L", Point::new(10.0, 10.0), 2.0, 0.0);
        assert_eq!(strokes.len(), 2);
        // First stroke: vertical from (10,10) to (10,12)
        let (s1, e1) = strokes[0];
        assert!((s1.x - 10.0).abs() < 0.001);
        assert!((e1.x - 10.0).abs() < 0.001);
        assert!((s1.y - 10.0).abs() < 0.001 || (e1.y - 10.0).abs() < 0.001);
        assert!((s1.y - 12.0).abs() < 0.001 || (e1.y - 12.0).abs() < 0.001);
        // Second stroke: horizontal from (10,12) to (11.2,12)
        let (s2, e2) = strokes[1];
        assert!((s2.y - 12.0).abs() < 0.001);
        assert!((e2.y - 12.0).abs() < 0.001);
        assert!((s2.x - 10.0).abs() < 0.001 || (e2.x - 10.0).abs() < 0.001);
        assert!((s2.x - 11.2).abs() < 0.001 || (e2.x - 11.2).abs() < 0.001);
    }

    #[test]
    fn every_letter_has_at_least_one_stroke() {
        for ch in "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
            let strokes = text_strokes(&ch.to_string(), Point::new(0.0, 0.0), 2.0, 0.0);
            assert!(!strokes.is_empty(), "letter {} has no strokes", ch);
        }
    }
}
