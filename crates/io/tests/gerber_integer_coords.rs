//! Gerber coordinate words are integers (issue #684). The Gerber Layer Format
//! Specification reads X, Y, I and J through the `%FS` digits, so a 4.6 file
//! writes ten millimetres as `X10000000`. These tests walk every generated
//! file of the sample boards and the showcase, check each coordinate token
//! against the declared digit limits, and read each file back through the plot
//! reader with the geometry unchanged to within one nanometre.

use pcbcraft_io::{PlotLayer, gerber_files_at, parse_gerber_layer};
use std::collections::HashSet;
use std::path::Path;

const CREATED: &str = "2026-10-29T00:00:00Z";
const EXAMPLES: [&str; 5] = [
    "../crates/io/tests/fixtures/sample_led_panel_base.json",
    "showcase.pcbcraft",
    "showcase-with-findings.pcbcraft",
    "dual-amplifier.pcbcraft",
    "kicad-library-rc.pcbcraft",
];

/// A point in nanometres, the unit of one 4.6 coordinate step.
type Nm = (i64, i64);

/// One coordinate word: the axis letter and its text.
type Word = (char, String);

/// The coordinate words of one data line, or none for comments, extended
/// commands and attribute lines.
fn words(line: &str) -> Vec<Word> {
    let line = line.trim();
    if line.starts_with('%') || line.starts_with("G04") || line.starts_with("M02") {
        return Vec::new();
    }
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let letter = chars[at];
        at += 1;
        let start = at;
        while at < chars.len()
            && (chars[at].is_ascii_digit() || matches!(chars[at], '-' | '+' | '.'))
        {
            at += 1;
        }
        if matches!(letter, 'X' | 'Y' | 'I' | 'J') {
            out.push((letter, chars[start..at].iter().collect()));
        }
    }
    out
}

/// Digits of `FSLAX<i><d>Y<i><d>`, or none when the file has no format line.
fn format_digits(text: &str) -> Option<(usize, usize)> {
    let at = text.find("%FSLAX")?;
    let rest = &text[at + 6..];
    let mut digits = rest.chars();
    let integer = digits.next()?.to_digit(10)? as usize;
    let decimal = digits.next()?.to_digit(10)? as usize;
    Some((integer, decimal))
}

/// Every generated Gerber of a project, name and text.
fn gerbers(name: &str) -> Result<Vec<(String, String)>, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    let project = pcbcraft_io::load(&path)?;
    let files = gerber_files_at(&project, CREATED)?;
    Ok(files
        .into_iter()
        .filter(|(_, text)| text.contains("%FSLAX"))
        .map(|(file, text)| (format!("{name}:{file}"), text))
        .collect())
}

#[test]
fn every_coordinate_token_is_an_integer_within_the_fs_digits() -> Result<(), String> {
    let mut checked = 0usize;
    for example in EXAMPLES {
        for (file, text) in gerbers(example)? {
            let (integer, decimal) = format_digits(&text).ok_or(format!("{file}: no FS line"))?;
            for line in text.lines() {
                for (letter, number) in words(line) {
                    let digits = number.trim_start_matches(['-', '+']);
                    assert!(
                        !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()),
                        "{file}: {letter}{number} is not an integer in `{line}`"
                    );
                    assert!(
                        digits.len() <= integer + decimal,
                        "{file}: {letter}{number} has more than {} digits",
                        integer + decimal
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 1000, "the sample and showcase carry real data");
    Ok(())
}

/// The integer points a file draws, flashes and moves through, applying the
/// modal rule that a missing axis keeps its last value.
fn token_points(text: &str) -> (Vec<Nm>, Vec<Nm>) {
    let (mut x, mut y) = (0i64, 0i64);
    let (mut flashes, mut others) = (Vec::new(), Vec::new());
    for line in text.lines() {
        let mut moved = false;
        for (letter, number) in words(line) {
            let value: i64 = number.parse().unwrap_or(0);
            match letter {
                'X' => {
                    x = value;
                    moved = true;
                }
                'Y' => {
                    y = value;
                    moved = true;
                }
                _ => {}
            }
        }
        if !moved {
            continue;
        }
        if line.contains("D03*") {
            flashes.push((x, y));
        } else {
            others.push((x, y));
        }
    }
    (flashes, others)
}

fn near(set: &HashSet<(i64, i64)>, nm: (i64, i64)) -> bool {
    // One unit of the 4.6 format is one nanometre.
    (-1..=1).any(|dx| (-1..=1).any(|dy| set.contains(&(nm.0 + dx, nm.1 + dy))))
}

fn reader_points(layer: &PlotLayer) -> HashSet<(i64, i64)> {
    let nm = |(x, y): (f64, f64)| ((x * 1e6).round() as i64, (y * 1e6).round() as i64);
    let mut set = HashSet::new();
    for stroke in &layer.strokes {
        set.extend(stroke.points.iter().copied().map(nm));
    }
    for ring in &layer.regions {
        set.extend(ring.iter().copied().map(nm));
    }
    set.extend(layer.flashes.iter().map(|f| nm((f.x_mm, f.y_mm))));
    set
}

#[test]
fn reader_returns_the_written_geometry_within_one_nanometre() -> Result<(), String> {
    let mut files_checked = 0usize;
    for example in EXAMPLES {
        for (file, text) in gerbers(example)? {
            // Inch files scale by 25.4 in the reader's millimetre output.
            if text.contains("%MOIN*%") {
                continue;
            }
            let layer = parse_gerber_layer(&text, &file)?;
            let (flashes, others) = token_points(&text);
            assert_eq!(layer.flashes.len(), flashes.len(), "{file}: flash count");
            for (read, written) in layer.flashes.iter().zip(&flashes) {
                assert!(
                    (read.x_mm * 1e6 - written.0 as f64).abs() <= 1.0
                        && (read.y_mm * 1e6 - written.1 as f64).abs() <= 1.0,
                    "{file}: flash {read:?} differs from {written:?}"
                );
            }
            let seen = reader_points(&layer);
            for point in &others {
                assert!(near(&seen, *point), "{file}: {point:?} was not read back");
            }
            files_checked += 1;
        }
    }
    assert!(files_checked >= 10);
    Ok(())
}

#[test]
fn old_decimal_coordinates_read_the_same_as_integers() -> Result<(), String> {
    let integer = "%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nX10000000Y-5250000D02*\nX12500000Y5000000D01*\nX0Y1D03*\nM02*\n";
    let decimal = "%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nX10.000000Y-5.250000D02*\nX12.500000Y5.000000D01*\nX0.000000Y0.000001D03*\nM02*\n";
    let a = parse_gerber_layer(integer, "int.gbr")?;
    let b = parse_gerber_layer(decimal, "dec.gbr")?;
    assert_eq!(a.strokes.len(), 1);
    assert_eq!(a.strokes[0].points, b.strokes[0].points);
    assert_eq!(a.strokes[0].points[0], (10.0, -5.25));
    assert_eq!(a.flashes[0].x_mm, b.flashes[0].x_mm);
    assert!((a.flashes[0].y_mm - 1e-6).abs() < 1e-12);
    Ok(())
}
