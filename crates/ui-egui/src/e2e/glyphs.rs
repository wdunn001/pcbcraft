//! Every non-ASCII character the interface draws must exist in a loaded font.
//!
//! Context: a character missing from the bundled IBM Plex and JetBrains Mono files draws as an
//! empty box (the Assistant status dot did). This scans the string literals of the app's source
//! and the English catalog and asks the real font set whether each character can be drawn.
//! Rules: the scan skips comments and the e2e folder; a character that fails must be replaced by
//! an icon or a painted shape, never added to `ALLOWED_GAPS` without a reason.
use super::super::*;
use super::support::*;

/// Characters known to be missing that are not drawn as text (none today).
const ALLOWED_GAPS: &[char] = &[];

/// Non-ASCII characters of the string literals on one source line, ignoring `//` comments.
fn literal_chars(line: &str) -> Vec<char> {
    let code = line.split("//").next().unwrap_or("");
    if code.trim_start().starts_with('*') {
        return Vec::new();
    }
    let mut inside = false;
    let mut out = Vec::new();
    let mut previous = ' ';
    for c in code.chars() {
        if c == '"' && previous != '\\' {
            inside = !inside;
        } else if inside && !c.is_ascii() {
            out.push(c);
        }
        previous = c;
    }
    out
}

fn visit(dir: &std::path::Path, found: &mut std::collections::BTreeMap<char, String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "e2e") {
                continue;
            }
            visit(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut in_tests = false;
            for (n, line) in text.lines().enumerate() {
                if line.trim() == "#[cfg(test)]" {
                    in_tests = true;
                }
                if in_tests {
                    continue;
                }
                for c in literal_chars(line) {
                    found
                        .entry(c)
                        .or_insert_with(|| format!("{}:{}", path.display(), n + 1));
                }
            }
        }
    }
}

#[test]
fn every_non_ascii_character_the_interface_draws_is_in_a_loaded_font() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut app = App::create(
        pcbcraft_mcp::shared(pcbcraft_engine::Session::default()),
        Preferences::default(),
        None,
        None,
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let mut found = std::collections::BTreeMap::new();
    visit(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    for line in pcbcraft_text::ENGLISH.lines() {
        for c in line.chars().filter(|c| !c.is_ascii()) {
            found
                .entry(c)
                .or_insert_with(|| "crates/text/strings/en.txt".to_string());
        }
    }
    let proportional = egui::FontId::proportional(13.0);
    let mono = egui::FontId::monospace(12.0);
    let missing: Vec<String> = found
        .iter()
        .filter(|(c, _)| !ALLOWED_GAPS.contains(c))
        .filter(|(c, _)| {
            !ctx.fonts_mut(|f| f.has_glyph(&proportional, **c) && f.has_glyph(&mono, **c))
        })
        .map(|(c, at)| format!("U+{:04X} {c} first used at {at}", *c as u32))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "characters no loaded font can draw:\n{}",
            missing.join("\n")
        ))
    }
}

#[test]
fn the_scan_can_tell_a_missing_glyph_from_a_present_one() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut app = App::create(
        pcbcraft_mcp::shared(pcbcraft_engine::Session::default()),
        Preferences::default(),
        None,
        None,
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let font = egui::FontId::proportional(13.0);
    let (a, dot) = ctx.fonts_mut(|f| (f.has_glyph(&font, 'a'), f.has_glyph(&font, '\u{25CF}')));
    assert!(a, "a plain letter is drawable");
    assert!(
        !dot,
        "the black circle is not in any loaded font, so it must be painted"
    );
    Ok(())
}
