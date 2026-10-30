//! Localization gates behind `cargo xtask i18n`: the pseudo-locale walk and font coverage.
//!
//! Context: `en-XA` is English accented and 40% longer (`crates/text/src/pseudo.rs`). Every
//! screen of `a11y_gate::each_screen` is drawn under it at 1280 and 320 points wide; text
//! that spills out of its clip rectangle without an ellipsis, and controls that leave the
//! window, are counted per screen and compared with `pseudo-ratchet.txt`.
//! Local rules: counts may only go down. Docs: `docs/localization.md`.
//!
//! # Dependencies
//! - crates/ui-egui/src/e2e/a11y_gate.rs supplies the screens
//! - crates/ui-egui/src/e2e/pseudo-ratchet.txt
//! - crates/text/src/pseudo.rs
//!
//! # Do not
//! - Raise a count; shorten the layout's demands (wrap, truncate with an ellipsis) instead.

use super::super::*;
use super::a11y_gate::{each_screen, practice};
use super::support::*;
use std::sync::Arc;

/// Draw one frame on a screen of the given size, after the warm-up in `frame`.
fn frame_sized(ctx: &egui::Context, app: &mut App, width: f32, height: f32) -> egui::FullOutput {
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(width, height),
            )),
            ..Default::default()
        },
        |ui| app.show(ui),
    );
    out.textures_delta.clear();
    out
}

/// Painted text cut off by its clip rectangle with no ellipsis, as screen/width problems.
fn clipped_text(out: &egui::FullOutput) -> usize {
    let mut seen = std::collections::BTreeSet::new();
    for clipped in &out.shapes {
        if let egui::Shape::Text(t) = &clipped.shape {
            let text = t.galley.rect.translate(t.pos.to_vec2());
            let clip = clipped.clip_rect;
            let spills = text.max.x > clip.max.x + 1.0 || text.min.x < clip.min.x - 1.0;
            if spills
                && !t.galley.elided
                && !t.galley.text().trim().is_empty()
                && clip.width() > 0.0
            {
                seen.insert(t.galley.text().to_owned());
            }
        }
    }
    seen.len()
}

/// Controls whose rectangle leaves the window sideways.
fn escaped_controls(ctx: &egui::Context, app: &mut App, width: f32) -> usize {
    interactive_rects(ctx, app)
        .iter()
        .filter(|(_, r)| r.max.x > width + 1.0 || r.min.x < -1.0)
        .count()
}

fn ratchet() -> Vec<(String, usize)> {
    include_str!("pseudo-ratchet.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (key, count) = l.rsplit_once('|')?;
            Some((key.trim().to_string(), count.trim().parse().ok()?))
        })
        .collect()
}

#[test]
fn i18n_pseudo_locale_fits_every_screen_at_1280_and_320() -> Result<(), String> {
    let pseudo = Arc::new(pcbcraft_text::Catalog::pseudo());
    let setup = |app: &mut App| {
        app.catalog = pseudo.clone();
        pcbcraft_text::activate(&app.catalog);
    };
    let mut now: Vec<(String, usize)> = Vec::new();
    each_screen(&setup, &mut |name, ctx, app| {
        for width in [1280.0_f32, 320.0] {
            let out = frame_sized(ctx, app, width, 900.0);
            let out = if width < 400.0 {
                frame_sized(ctx, app, width, 900.0)
            } else {
                out
            };
            let clipped = clipped_text(&out);
            let escaped = escaped_controls(ctx, app, width);
            now.push((format!("clipped | {name} | {width:.0}"), clipped));
            now.push((format!("escaped | {name} | {width:.0}"), escaped));
        }
    })?;
    let allowed = ratchet();
    let mut problems = Vec::new();
    let mut lines = Vec::new();
    for (key, n) in &now {
        let max = allowed
            .iter()
            .find(|(k, _)| k == key)
            .map_or(0, |(_, m)| *m);
        if *n > 0 {
            lines.push(format!("{key} | {n}"));
        }
        if *n != max {
            problems.push(format!("{key}: {n}, file says {max}"));
        }
    }
    if let Ok(dir) = std::env::var("PCBCRAFT_I18N_REPORT") {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            std::path::Path::new(&dir).join("pseudo.txt"),
            lines.join("\n"),
        );
    }
    assert!(
        problems.is_empty(),
        "pseudo-locale counts moved (lower the file when fewer, fix the layout when more):\n{}\ncurrent lines for pseudo-ratchet.txt:\n{}",
        problems.join("\n"),
        lines.join("\n")
    );
    Ok(())
}

#[test]
fn i18n_every_character_of_every_locale_has_a_glyph() -> Result<(), String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../text/strings");
    let (ctx, mut app) = practice();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let font = crate::fonts::Role::Body.font();
    let mut missing = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let skip = ["converted-fns.txt", "plural-known.txt", "i18n-ratchet.txt"];
        if !name.ends_with(".txt") || skip.contains(&name.as_str()) || name.starts_with("fixture") {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).map_err(|e| e.to_string())?;
        let mut chars = std::collections::BTreeSet::new();
        for (_, value) in pcbcraft_text::parse(&text).0 {
            chars.extend(value.chars().filter(|c| !c.is_control()));
        }
        for c in chars {
            if !ctx.fonts_mut(|f| f.has_glyph(&font, c)) {
                missing.push(format!("{name}: U+{:04X} {c}", c as u32));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "characters no loaded font can draw (add a fallback font in fonts.rs):\n{}",
        missing.join("\n")
    );
    Ok(())
}
