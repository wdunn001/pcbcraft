//! G-code program statistics and toolpath segments.
//!
//! Reads a G-code text file and computes either time/distance estimates or the
//! sequence of toolpath segments for preview. The shared parser tracks absolute
//! position, converts inches to millimetres, and understands G0/G1 linear moves
//! and G2/G3 circular arcs with I/J centre offsets.
//!
//! # Rules
//! - The parser is line-oriented and never reads past the end of the input.
//! - G90/G91, G20/G21, G0/G1/G2/G3, T and F are the only modal words that matter here.
//! - Rapid moves take time at a constant 3000 mm/min; cutting moves use the feed.
//! - Arcs are split into chords of at most 5 degrees for toolpath preview.
//!
//! # Dependencies
//! - crates/io/src/lib.rs (pub use)
//!
//! # Do not
//! - Add canned cycles here; they are planned for a later slice.

/// The constant rapid (jog) feed rate used for G0 moves, in millimetres per minute.
///
/// This is a fixed estimate: real machines vary, but the app needs a single
/// number to show the user before the job runs.
const RAPID_MM_PER_MIN: f64 = 3000.0;

/// Maximum number of toolpath segments to produce; programs that would generate
/// more are considered too complex for preview.
const MAX_TOOLPATH_SEGMENTS: usize = 200_000;

/// Maximum arc sweep per chord, in degrees. Larger arcs are split into multiple
/// straight-line segments so the preview renders smooth curves.
const ARC_CHORD_DEGREES: f64 = 5.0;

use std::f64::consts::PI;

/// Summary of a G-code program: how far it moves, how long it takes, how many moves.
///
/// All distances are in millimetres (inches are converted on input) and the
/// time is an estimate based on the modal feed rate at each cutting move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GcodeStats {
    /// Total rapid (G0) travel in millimetres.
    pub rapid_mm: f64,
    /// Total cutting (G1) travel in millimetres.
    pub cut_mm: f64,
    /// Estimated machine time in seconds.
    pub time_s: f64,
    /// Number of G0 and G1 moves found.
    pub moves: u32,
}

/// One straight-line segment of a toolpath, ready for preview rendering.
///
/// Arcs are split into chords of at most five degrees, so every segment is a
/// straight line. The `rapid` flag distinguishes G0 travel from cutting moves,
/// and `tool` records the active tool number so the preview can colour by tool.
///
/// # Rules
/// - `from` and `to` are in millimetres, always absolute.
/// - `tool` is the tool number active during the segment (0 if none selected).
///
/// # Dependencies
/// - `crates/io/src/gcode_stats.rs` `gcode_toolpath` produces these.
///
/// # Do not
/// - Use this for time estimates; use `GcodeStats` for that.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolpathSegment {
    /// Start point in millimetres.
    pub from: [f64; 3],
    /// End point in millimetres.
    pub to: [f64; 3],
    /// True for G0 rapid moves, false for cutting moves.
    pub rapid: bool,
    /// Tool number active during this segment.
    pub tool: u32,
}

/// Internal representation of a parsed G-code move, shared by both the stats
/// and toolpath functions.
struct ParsedMove {
    from: [f64; 3],
    to: [f64; 3],
    motion: u8,
    feed: f64,
    tool: u32,
    centre_offset: [f64; 2],
    clockwise: bool,
}

/// Stateful iterator that parses G-code text and yields one move at a time.
///
/// The iterator tracks modal state (position, feed, tool, motion type) across
/// lines and yields a `ParsedMove` for each line that contains axis words.
/// Lines without axis words (e.g. "T2 M6") update state but produce no move.
struct GcodeParser {
    lines: std::vec::IntoIter<String>,
    at: [f64; 3],
    absolute: bool,
    metric: bool,
    motion: Option<u8>,
    feed: f64,
    tool: u32,
    line_count: usize,
}

impl GcodeParser {
    fn new(text: &str) -> Self {
        let lines = text
            .lines()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .into_iter();
        Self {
            lines,
            at: [0.0, 0.0, 0.0],
            absolute: true,
            metric: true,
            motion: None,
            feed: 0.0,
            tool: 0,
            line_count: 0,
        }
    }
}

impl Iterator for GcodeParser {
    type Item = Result<ParsedMove, String>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let line = self.lines.next()?;

            self.line_count += 1;
            if self.line_count > 1_000_000 {
                return Some(Err("program too long: more than 1,000,000 lines".into()));
            }

            let line = strip_comments(&line);
            let words = tokenize(&line);
            if words.is_empty() {
                continue;
            }

            // Parse modal words: G, T, M
            for word in &words {
                let upper = word.to_uppercase();
                if let Some(rest) = upper.strip_prefix('G')
                    && let Ok(n) = rest.parse::<u8>()
                {
                    match n {
                        0..=3 => self.motion = Some(n),
                        20 => self.metric = false,
                        21 => self.metric = true,
                        90 => self.absolute = true,
                        91 => self.absolute = false,
                        _ => {}
                    }
                }
                if let Some(rest) = upper.strip_prefix('T')
                    && let Ok(n) = rest.parse::<u32>()
                {
                    self.tool = n;
                }
            }

            // Parse axis words
            let mut has_axis = false;
            let mut target = self.at;
            let mut centre_offset = [0.0, 0.0];

            for word in &words {
                let upper = word.to_uppercase();
                if let Some(rest) = upper.strip_prefix('X') {
                    if let Ok(v) = rest.parse::<f64>() {
                        let mm = if self.metric { v } else { v * 25.4 };
                        target[0] = if self.absolute { mm } else { self.at[0] + mm };
                        has_axis = true;
                    }
                } else if let Some(rest) = upper.strip_prefix('Y') {
                    if let Ok(v) = rest.parse::<f64>() {
                        let mm = if self.metric { v } else { v * 25.4 };
                        target[1] = if self.absolute { mm } else { self.at[1] + mm };
                        has_axis = true;
                    }
                } else if let Some(rest) = upper.strip_prefix('Z') {
                    if let Ok(v) = rest.parse::<f64>() {
                        let mm = if self.metric { v } else { v * 25.4 };
                        target[2] = if self.absolute { mm } else { self.at[2] + mm };
                        has_axis = true;
                    }
                } else if let Some(rest) = upper.strip_prefix('I') {
                    if let Ok(v) = rest.parse::<f64>() {
                        centre_offset[0] = if self.metric { v } else { v * 25.4 };
                    }
                } else if let Some(rest) = upper.strip_prefix('J') {
                    if let Ok(v) = rest.parse::<f64>() {
                        centre_offset[1] = if self.metric { v } else { v * 25.4 };
                    }
                } else if let Some(rest) = upper.strip_prefix('R') {
                    if rest.parse::<f64>().is_ok() {
                        return Some(Err("R-word arcs are not supported".into()));
                    }
                } else if let Some(rest) = upper.strip_prefix('F')
                    && let Ok(v) = rest.parse::<f64>()
                {
                    self.feed = v;
                }
            }

            if !has_axis {
                continue;
            }

            let motion = match self.motion {
                Some(m) => m,
                None => continue,
            };

            let move_record = ParsedMove {
                from: self.at,
                to: target,
                motion,
                feed: self.feed,
                tool: self.tool,
                centre_offset,
                clockwise: motion == 2,
            };

            // Check for cutting move with no feed
            if motion != 0 && self.feed <= 0.0 {
                return Some(Err("cutting move with no feed rate".into()));
            }

            self.at = target;
            return Some(Ok(move_record));
        }
    }
}

/// Compute the path length and time estimate for a G-code program.
///
/// The text is parsed line by line. Comments in parentheses and after a
/// semicolon are ignored. The parser tracks absolute position and the
/// current modal motion (G0 or G1) and feed rate (F). G20 inches are
/// converted to millimetres so the totals are always metric.
///
/// # Rules
/// - G0 moves add to `rapid_mm` and take time at `RAPID_MM_PER_MIN`.
/// - G1 moves add to `cut_mm` and take time at the current feed.
/// - A G1 move with no feed set is an error.
/// - G2/G3 arcs with I/J centre offsets are supported; R-word arcs are not.
/// - More than 1,000,000 lines is an error.
///
/// # Do not
/// - Use this for validation; it is for estimates only.
pub fn gcode_stats(text: &str) -> Result<GcodeStats, String> {
    let mut rapid_mm = 0.0;
    let mut cut_mm = 0.0;
    let mut time_s = 0.0;
    let mut moves = 0u32;

    for result in GcodeParser::new(text) {
        let move_record = result?;

        let dist = if move_record.motion == 2 || move_record.motion == 3 {
            let start_xy = [move_record.from[0], move_record.from[1]];
            let end_xy = [move_record.to[0], move_record.to[1]];
            let dz = move_record.to[2] - move_record.from[2];
            arc_length(
                &start_xy,
                &end_xy,
                &move_record.centre_offset,
                move_record.clockwise,
                dz,
            )?
        } else {
            distance(&move_record.from, &move_record.to)
        };

        if move_record.motion == 0 {
            rapid_mm += dist;
            if dist > 0.0 && RAPID_MM_PER_MIN > 0.0 {
                time_s += dist / RAPID_MM_PER_MIN * 60.0;
            }
        } else {
            cut_mm += dist;
            if dist > 0.0 {
                time_s += dist / move_record.feed * 60.0;
            }
        }
        moves += 1;
    }

    Ok(GcodeStats {
        rapid_mm,
        cut_mm,
        time_s,
        moves,
    })
}

/// Parse a G-code program into a sequence of straight-line toolpath segments.
///
/// Arcs (G2/G3) are split into chords of at most five degrees, so the caller
/// receives only straight segments suitable for preview rendering. The active
/// tool number at each segment is recorded, and rapid (G0) moves are flagged.
///
/// # Rules
/// - `max_segments` caps the output size; if more segments would be produced,
///   an error is returned. The cap is clamped to 200,000.
/// - Tool number comes from the last Tn word seen; a bare M6 keeps the current number.
/// - Arcs are split into chords of at most 5 degrees.
///
/// # Dependencies
/// - `crates/io/src/gcode_stats.rs` `GcodeParser` (shared line parser).
///
/// # Do not
/// - Use this for time estimates; use `gcode_stats` for that.
pub fn gcode_toolpath(text: &str, max_segments: usize) -> Result<Vec<ToolpathSegment>, String> {
    let max = if max_segments > MAX_TOOLPATH_SEGMENTS {
        MAX_TOOLPATH_SEGMENTS
    } else {
        max_segments
    };
    let mut segments = Vec::new();

    for result in GcodeParser::new(text) {
        let move_record = result?;

        if move_record.motion == 2 || move_record.motion == 3 {
            let chords = split_arc_into_chords(&move_record, ARC_CHORD_DEGREES)?;
            for chord in chords {
                if segments.len() >= max {
                    return Err("toolpath has too many segments".into());
                }
                segments.push(chord);
            }
        } else {
            if segments.len() >= max {
                return Err("toolpath has too many segments".into());
            }
            segments.push(ToolpathSegment {
                from: move_record.from,
                to: move_record.to,
                rapid: move_record.motion == 0,
                tool: move_record.tool,
            });
        }
    }

    Ok(segments)
}

/// Split an arc move into straight-line chord segments.
///
/// The arc is defined by its start point, end point and centre offset. The
/// sweep angle is computed and divided into segments of at most `max_angle_deg`
/// degrees. Each segment is a straight line from one point on the arc to the next.
fn split_arc_into_chords(
    move_record: &ParsedMove,
    max_angle_deg: f64,
) -> Result<Vec<ToolpathSegment>, String> {
    let max_angle_rad = max_angle_deg * PI / 180.0;

    let centre = [
        move_record.from[0] + move_record.centre_offset[0],
        move_record.from[1] + move_record.centre_offset[1],
    ];

    let radius =
        (move_record.centre_offset[0].powi(2) + move_record.centre_offset[1].powi(2)).sqrt();

    let start_angle = (move_record.from[1] - centre[1]).atan2(move_record.from[0] - centre[0]);
    let end_angle = (move_record.to[1] - centre[1]).atan2(move_record.to[0] - centre[0]);

    let mut sweep = end_angle - start_angle;
    if move_record.clockwise {
        sweep = -sweep;
    }
    if sweep <= 0.0 {
        sweep += 2.0 * PI;
    }

    let mut num_segments = ((sweep / max_angle_rad).ceil()) as usize;
    if num_segments == 0 {
        num_segments = 1;
    }

    let angle_step = sweep / num_segments as f64;

    let mut segments = Vec::with_capacity(num_segments);
    let mut current_angle = start_angle;
    let mut prev_point = move_record.from;

    for i in 1..=num_segments {
        if move_record.clockwise {
            current_angle -= angle_step;
        } else {
            current_angle += angle_step;
        }

        let x = centre[0] + radius * current_angle.cos();
        let y = centre[1] + radius * current_angle.sin();
        let z = move_record.from[2]
            + (move_record.to[2] - move_record.from[2]) * (i as f64 / num_segments as f64);
        let point = [x, y, z];

        segments.push(ToolpathSegment {
            from: prev_point,
            to: point,
            rapid: false,
            tool: move_record.tool,
        });

        prev_point = point;
    }

    Ok(segments)
}

/// Remove inline (in-parenthesis) and trailing (semicolon) comments.
fn strip_comments(line: &str) -> String {
    let mut result = String::new();
    let mut in_parens = false;
    for ch in line.chars() {
        if in_parens {
            if ch == ')' {
                in_parens = false;
            }
            continue;
        }
        if ch == '(' {
            in_parens = true;
            continue;
        }
        if ch == ';' {
            break;
        }
        result.push(ch);
    }
    result
}

/// Split a G-code line into space-delimited words.
fn tokenize(line: &str) -> Vec<String> {
    line.split_whitespace().map(|w| w.to_string()).collect()
}

/// 3D Euclidean distance between two points.
fn distance(from: &[f64; 3], to: &[f64; 3]) -> f64 {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let dz = to[2] - from[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Arc length in mm for an XY circular arc, with an optional Z (helix) component.
///
/// The arc is defined by its start point, end point and centre offset from the
/// start (I, J). The radius is taken from the centre offset and the sweep angle
/// is computed from the start and end positions relative to the centre. A helix
/// adds the Z travel as the square root of the sum of the squares.
///
/// # Rules
/// - The sweep is in (0, 2*pi] for the given direction.
/// - If the start and end are the same the arc is a full circle.
///
/// # Do not
/// - Use this for R-word arcs; they are not supported by this function.
fn arc_length(
    start: &[f64; 2],
    end: &[f64; 2],
    centre_offset: &[f64; 2],
    clockwise: bool,
    dz: f64,
) -> Result<f64, String> {
    let radius = (centre_offset[0] * centre_offset[0] + centre_offset[1] * centre_offset[1]).sqrt();
    if radius == 0.0 && (start[0] != end[0] || start[1] != end[1]) {
        return Err("arc with zero radius and different endpoints".into());
    }
    let centre = [start[0] + centre_offset[0], start[1] + centre_offset[1]];
    let start_angle = (start[1] - centre[1]).atan2(start[0] - centre[0]);
    let end_angle = (end[1] - centre[1]).atan2(end[0] - centre[0]);
    let mut sweep = end_angle - start_angle;
    if clockwise {
        sweep = -sweep;
    }
    if sweep <= 0.0 {
        sweep += 2.0 * PI;
    }
    let arc_xy = radius * sweep;
    let length = (arc_xy * arc_xy + dz * dz).sqrt();
    Ok(length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gcode_stats_of_a_two_move_program_matches_the_hand_count() -> Result<(), String> {
        let program = "G21\nG90\nG0 X3 Y4\nG1 X3 Y4 Z-1 F60";
        let stats = gcode_stats(program)?;
        assert!((stats.rapid_mm - 5.0).abs() < 1e-9);
        assert!((stats.cut_mm - 1.0).abs() < 1e-9);
        // Rapid: 5mm at 3000mm/min = 0.1s. Cut: 1mm at 60mm/min = 1.0s.
        assert!((stats.time_s - 1.1).abs() < 1e-9);
        assert_eq!(stats.moves, 2);
        Ok(())
    }

    #[test]
    fn relative_mode_adds_up() -> Result<(), String> {
        let program = "G21\nG91\nG1 X10 F100\nG1 Y10 F100";
        let stats = gcode_stats(program)?;
        assert!((stats.cut_mm - 20.0).abs() < 1e-9);
        assert_eq!(stats.moves, 2);
        // 20mm at 100mm/min = 12s.
        assert!((stats.time_s - 12.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn inches_are_converted() -> Result<(), String> {
        let program = "G20 G1 X1 F10";
        let stats = gcode_stats(program)?;
        assert!((stats.cut_mm - 25.4).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn a_cut_with_no_feed_is_an_error() {
        let program = "G1 X10";
        let message = gcode_stats(program).err().unwrap_or_default();
        assert!(
            message.contains("cutting move with no feed rate"),
            "the error names the missing feed: {message}"
        );
    }

    #[test]
    fn a_quarter_circle_arc_has_length_pi_over_two_times_radius() -> Result<(), String> {
        let program = "G21\nG90\nG1 X10 Y0 F60\nG3 X0 Y10 I-10 J0 F60";
        let stats = gcode_stats(program)?;
        let expected_cut = 10.0 + 10.0 * PI / 2.0;
        assert!(
            (stats.cut_mm - expected_cut).abs() < 1e-9,
            "cut_mm = {} expected {} (10 + 5*pi)",
            stats.cut_mm,
            expected_cut
        );
        Ok(())
    }

    #[test]
    fn a_full_circle_is_two_pi_r() -> Result<(), String> {
        let program = "G21\nG90\nG0 X10 Y0\nG2 X10 Y0 I-10 J0 F60";
        let stats = gcode_stats(program)?;
        let expected_cut = 20.0 * PI;
        assert!(
            (stats.cut_mm - expected_cut).abs() < 1e-9,
            "cut_mm = {} expected {} (20*pi)",
            stats.cut_mm,
            expected_cut
        );
        Ok(())
    }

    #[test]
    fn a_helix_adds_the_depth() -> Result<(), String> {
        let program = "G21\nG90\nG0 X10 Y0\nG2 X10 Y0 I-10 J0 Z-2 F60";
        let stats = gcode_stats(program)?;
        let arc_xy = 20.0 * PI;
        let expected = (arc_xy * arc_xy + 4.0).sqrt();
        assert!(
            (stats.cut_mm - expected).abs() < 1e-9,
            "cut_mm = {} expected {} (helix length)",
            stats.cut_mm,
            expected
        );
        Ok(())
    }

    #[test]
    fn r_word_arcs_are_refused() {
        let program = "G2 X10 Y10 R5";
        let message = gcode_stats(program).err().unwrap_or_default();
        assert!(
            message.contains("R-word arcs are not supported"),
            "the error names the unsupported R-word: {message}"
        );
    }

    #[test]
    fn toolpath_marks_rapids_and_tool_numbers() -> Result<(), String> {
        let program = "T2 M6\nG0 X5 Y0\nG1 X5 Y5 F60";
        let segments = gcode_toolpath(program, 100)?;
        assert_eq!(segments.len(), 2);

        // First segment: rapid from [0,0,0] to [5,0,0], tool 2
        assert!((segments[0].from[0] - 0.0).abs() < 1e-9);
        assert!((segments[0].from[1] - 0.0).abs() < 1e-9);
        assert!((segments[0].from[2] - 0.0).abs() < 1e-9);
        assert!((segments[0].to[0] - 5.0).abs() < 1e-9);
        assert!((segments[0].to[1] - 0.0).abs() < 1e-9);
        assert!((segments[0].to[2] - 0.0).abs() < 1e-9);
        assert!(segments[0].rapid);
        assert_eq!(segments[0].tool, 2);

        // Second segment: cutting from [5,0,0] to [5,5,0], tool 2
        assert!((segments[1].from[0] - 5.0).abs() < 1e-9);
        assert!((segments[1].from[1] - 0.0).abs() < 1e-9);
        assert!((segments[1].from[2] - 0.0).abs() < 1e-9);
        assert!((segments[1].to[0] - 5.0).abs() < 1e-9);
        assert!((segments[1].to[1] - 5.0).abs() < 1e-9);
        assert!((segments[1].to[2] - 0.0).abs() < 1e-9);
        assert!(!segments[1].rapid);
        assert_eq!(segments[1].tool, 2);

        Ok(())
    }

    #[test]
    fn an_arc_is_split_into_chords_that_end_at_the_arc_end() -> Result<(), String> {
        // 90-degree counterclockwise arc from (10,0) to (0,10), centre at (0,0)
        let program = "G21\nG90\nG1 X10 Y0 F60\nG3 X0 Y10 I-10 J0 F60";
        let segments = gcode_toolpath(program, 1000)?;

        // 1 straight segment + 18 arc chords (90/5 = 18)
        assert_eq!(segments.len(), 19);

        // The last segment must end at the arc endpoint (0, 10, 0)
        let last = &segments[segments.len() - 1];
        assert!(
            (last.to[0] - 0.0).abs() < 1e-6,
            "last segment x: {}",
            last.to[0]
        );
        assert!(
            (last.to[1] - 10.0).abs() < 1e-6,
            "last segment y: {}",
            last.to[1]
        );
        assert!((last.to[2] - 0.0).abs() < 1e-9);

        // All arc segments should be non-rapid
        for seg in &segments[1..] {
            assert!(!seg.rapid);
        }

        Ok(())
    }

    #[test]
    fn too_many_segments_is_an_error() {
        let program = "G21\nG90\nG0 X5 Y0\nG1 X5 Y5 F60";
        let result = gcode_toolpath(program, 1);
        assert!(result.is_err());
        let message = result.err().unwrap_or_default();
        assert!(
            message.contains("toolpath has too many segments"),
            "the error names the limit: {message}"
        );
    }
}
