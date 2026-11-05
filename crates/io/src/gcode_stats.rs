//! G-code program statistics: estimated machine time, travel distance and move count.
//!
//! Reads a G-code text file (one line at a time) and computes how long it will take
//! a machine to run it. Only the modal commands the app itself writes are handled
//! in this first slice; arcs are rejected. The parser tracks absolute position
//! and converts inches to millimetres so the caller always gets metric totals.
//!
//! # Rules
//! - The parser is line-oriented and never reads past the end of the input.
//! - G90/G91, G20/G21, G0/G1 and F are the only modal words that matter here.
//! - Rapid moves take time at a constant 3000 mm/min; cutting moves use the feed.
//!
//! # Dependencies
//! - crates/io/src/lib.rs (pub use)
//!
//! # Do not
//! - Add support for G2/G3, canned cycles or M6 tool changes yet; they are
//!   planned for the next slice of this feature.

/// The constant rapid (jog) feed rate used for G0 moves, in millimetres per minute.
///
/// This is a fixed estimate: real machines vary, but the app needs a single
/// number to show the user before the job runs.
const RAPID_MM_PER_MIN: f64 = 3000.0;

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
/// - G2/G3 arcs are not supported yet and are rejected.
/// - More than 1,000,000 lines is an error.
///
/// # Do not
/// - Use this for validation; it is for estimates only.
pub fn gcode_stats(text: &str) -> Result<GcodeStats, String> {
    let mut rapid_mm = 0.0;
    let mut cut_mm = 0.0;
    let mut time_s = 0.0;
    let mut moves = 0u32;

    // Current absolute position in mm (X, Y, Z).
    let mut at = [0.0, 0.0, 0.0];
    // Modal state.
    let mut absolute = true; // G90
    let mut metric = true; // G21
    let mut motion: Option<u8> = None; // Some(0) = G0, Some(1) = G1
    let mut feed_mm_per_min = 0.0;

    let mut line_count = 0usize;
    for raw_line in text.lines() {
        line_count += 1;
        if line_count > 1_000_000 {
            return Err("program too long: more than 1,000,000 lines".into());
        }

        // Strip inline and trailing comments.
        let line = strip_comments(raw_line);
        let words = tokenize(&line);
        if words.is_empty() {
            continue;
        }

        // Parse G words: extract the number after 'G'.
        for word in &words {
            let upper = word.to_uppercase();
            if let Some(rest) = upper.strip_prefix('G')
                && let Ok(n) = rest.parse::<u8>()
            {
                match n {
                    0 => motion = Some(0),
                    1 => motion = Some(1),
                    2 | 3 => return Err("arcs are not supported yet".into()),
                    20 => metric = false,
                    21 => metric = true,
                    90 => absolute = true,
                    91 => absolute = false,
                    _ => {} // Ignore other G codes (M codes, etc.)
                }
            }
        }

        // If no motion word on this line but we have a prior one and there are
        // X/Y/Z words, reuse the modal motion.
        let motion = match motion {
            Some(m) => m,
            None => continue,
        };

        // Extract axis values.
        let mut has_axis = false;
        let mut target = [at[0], at[1], at[2]];
        for word in &words {
            let upper = word.to_uppercase();
            if let Some(rest) = upper.strip_prefix('X')
                && let Ok(v) = rest.parse::<f64>()
            {
                let mm = if metric { v } else { v * 25.4 };
                if absolute {
                    target[0] = mm;
                } else {
                    target[0] = at[0] + mm;
                }
                has_axis = true;
            } else if let Some(rest) = upper.strip_prefix('Y')
                && let Ok(v) = rest.parse::<f64>()
            {
                let mm = if metric { v } else { v * 25.4 };
                if absolute {
                    target[1] = mm;
                } else {
                    target[1] = at[1] + mm;
                }
                has_axis = true;
            } else if let Some(rest) = upper.strip_prefix('Z')
                && let Ok(v) = rest.parse::<f64>()
            {
                let mm = if metric { v } else { v * 25.4 };
                if absolute {
                    target[2] = mm;
                } else {
                    target[2] = at[2] + mm;
                }
                has_axis = true;
            } else if let Some(rest) = upper.strip_prefix('F')
                && let Ok(v) = rest.parse::<f64>()
            {
                // F is always mm/min regardless of G20/G21.
                feed_mm_per_min = v;
            }
        }

        if !has_axis {
            continue;
        }

        // Compute move distance and time.
        let dist = distance(&at, &target);
        if motion == 0 {
            // G0 rapid.
            rapid_mm += dist;
            if dist > 0.0 && RAPID_MM_PER_MIN > 0.0 {
                time_s += dist / RAPID_MM_PER_MIN * 60.0;
            }
        } else {
            // G1 cutting.
            if feed_mm_per_min <= 0.0 {
                return Err("cutting move with no feed rate".into());
            }
            cut_mm += dist;
            if dist > 0.0 {
                time_s += dist / feed_mm_per_min * 60.0;
            }
        }
        moves += 1;
        at = target;
    }

    Ok(GcodeStats {
        rapid_mm,
        cut_mm,
        time_s,
        moves,
    })
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
    fn arcs_are_refused_for_now() {
        let program = "G2 X10 Y10 I5 J5";
        let message = gcode_stats(program).err().unwrap_or_default();
        assert!(
            message.contains("arcs are not supported yet"),
            "the error names the unsupported arcs: {message}"
        );
    }
}
