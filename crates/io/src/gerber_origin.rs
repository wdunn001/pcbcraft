//! One design origin for every Gerber layer file in a fabrication bundle.
//!
//! The board's drill origin (`Board::drill_origin`) is the point a fabricator
//! measures from. The drill writers subtract it themselves; this module moves
//! every other Gerber file by the same amount so copper, mask, paste, silk and
//! the outline register with the drills.
//!
//! # Rules
//! - Only absolute `X` and `Y` words are moved; arc centre offsets (`I`, `J`),
//!   comments, attributes and aperture macros are left alone.
//! - The `%TF.MD5` line is recomputed over the moved body so it stays valid.
//! - A zero origin returns the text unchanged, byte for byte.
//!
//! # Dependencies
//! - crates/io/src/gerber.rs coordinate (the 4.6 fixed-point rounding)
//! - crates/io/src/gerber_fidelity.rs gerber_md5_hex (the MD5 digest)
//! - crates/io/src/lib.rs gerber_files_core (the only caller)
//!
//! # Do not
//! - Move a drill file or the drill map here; their writers already apply it.

/// Shift every absolute `X` and `Y` coordinate in a Gerber file by minus
/// `(dx, dy)` file units (4.6 fixed point), then refresh the MD5 attribute.
///
/// # Rules
/// A line is moved only when it is made of `G`, `X`, `Y`, `I`, `J` and `D`
/// words and ends with `*`; any other line is copied as it is.
///
/// # Dependencies
/// - crates/io/src/gerber_fidelity.rs gerber_md5_hex (the MD5 digest)
///
/// # Do not
/// Call it on a file that already carries the origin.
pub(crate) fn shift_gerber(text: &str, dx: i64, dy: i64) -> String {
    if dx == 0 && dy == 0 {
        return text.to_owned();
    }
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| shift_line(line, dx, dy).unwrap_or_else(|| line.to_owned()))
        .collect();
    let Some(md5_at) = lines.iter().position(|l| l.contains("TF.MD5,")) else {
        return join(&lines);
    };
    let had_end = lines.last().is_some_and(|l| l == "M02*");
    let kept: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != md5_at)
        .map(|(_, l)| l.as_str())
        .collect();
    let body_end = if had_end { kept.len() - 1 } else { kept.len() };
    let body = format!("{}\n", kept[..body_end].join("\n"));
    let hash = crate::gerber_fidelity::gerber_md5_hex(&body);
    lines[md5_at] = format!("%TF.MD5,{hash}*%");
    join(&lines)
}

fn join(lines: &[String]) -> String {
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// The line with its `X` and `Y` words moved, or `None` when the line is not a
/// plain coordinate or function line.
fn shift_line(line: &str, dx: i64, dy: i64) -> Option<String> {
    let body = line.strip_suffix('*')?;
    let mut out = String::with_capacity(line.len() + 4);
    let mut rest = body;
    let mut moved = false;
    while let Some(letter) = rest.chars().next() {
        if !matches!(letter, 'G' | 'X' | 'Y' | 'I' | 'J' | 'D') {
            return None;
        }
        let tail = &rest[1..];
        let digits = tail
            .char_indices()
            .find(|(i, c)| !(c.is_ascii_digit() || (*i == 0 && *c == '-')))
            .map_or(tail.len(), |(i, _)| i);
        let value: i64 = tail[..digits].parse().ok()?;
        rest = &tail[digits..];
        out.push(letter);
        match letter {
            'X' => {
                moved = true;
                out.push_str(&(value - dx).to_string());
            }
            'Y' => {
                moved = true;
                out.push_str(&(value - dy).to_string());
            }
            // Other words keep their text, so `D01` stays `D01`.
            _ => out.push_str(&tail[..digits]),
        }
    }
    if !moved {
        return None;
    }
    out.push('*');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_only_absolute_words() {
        assert_eq!(
            shift_line("X5000000Y-1000000D01*", 2_000_000, 1_000_000).as_deref(),
            Some("X3000000Y-2000000D01*")
        );
        assert_eq!(
            shift_line("X0Y0I1500000J0D01*", 1_000_000, 0).as_deref(),
            Some("X-1000000Y0I1500000J0D01*")
        );
        assert_eq!(shift_line("G04 X1 comment*", 1, 1), None);
        assert_eq!(shift_line("D10*", 1, 1), None);
    }

    #[test]
    fn zero_origin_is_byte_identical_and_md5_stays_valid() -> Result<(), String> {
        let body = "%FSLAX46Y46*%\n%MOMM*%\nD10*\nX1000000Y2000000D03*\n";
        let hash = crate::gerber_fidelity::gerber_md5_hex(body);
        let text =
            format!("%FSLAX46Y46*%\n%TF.MD5,{hash}*%\n%MOMM*%\nD10*\nX1000000Y2000000D03*\nM02*\n");
        assert_eq!(shift_gerber(&text, 0, 0), text);
        let moved = shift_gerber(&text, 1_000_000, 0);
        assert!(moved.contains("X0Y2000000D03*"));
        crate::gerber_fidelity::verify_gerber_md5(&moved)
    }
}
