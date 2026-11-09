//! WCAG 2.2 contrast arithmetic and the check that every theme passes it.
//!
//! Context: criteria 1.4.3 (text 4.5:1, large text 3:1) and 1.4.11 (non-text
//! parts and focus indicators 3:1). Colours come from `./palette.rs`, the same
//! values `App::style` and `canvas.rs` paint with.
//! Local rules: a failing pair is fixed in `palette.rs` or listed with a reason
//! in `../contrast-exemptions.txt`; never loosen a minimum here.
use egui::Color32;

/// Relative luminance of an sRGB colour (WCAG 2.2 definition).
fn luminance(c: Color32) -> f64 {
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// The WCAG contrast ratio of two opaque colours, from 1.0 up to 21.0.
///
/// Why: one shared oracle for the palette nudge and the theme check.
///
/// # Rules
/// - Symmetric: the lighter colour is always the numerator.
///
/// # Dependencies
/// - `crates/ui-egui/src/palette.rs` supplies the colours it is asked about.
///
/// # Do not
/// - Pass a translucent colour; composite it over its background first.
pub(crate) fn contrast_ratio(a: Color32, b: Color32) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x >= y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::{ThemeColours, palette, visuals};
    use crate::theme::{Tokens, composite};

    /// One foreground against one background with the minimum it must reach.
    struct Pair {
        name: &'static str,
        fg: Color32,
        bg: Color32,
        min: f64,
    }

    const TEXT: f64 = 4.5;
    const NON_TEXT: f64 = 3.0;

    /// Composite a translucent colour over an opaque background (premultiplied).
    fn over(src: Color32, bg: Color32) -> Color32 {
        let keep = 1.0 - f32::from(src.a()) / 255.0;
        let mix = |s: u8, d: u8| (f32::from(s) + f32::from(d) * keep).round().min(255.0) as u8;
        Color32::from_rgb(
            mix(src.r(), bg.r()),
            mix(src.g(), bg.g()),
            mix(src.b(), bg.b()),
        )
    }

    /// Every pair the shell and canvas draw for one theme.
    fn pairs(t: &ThemeColours) -> Vec<Pair> {
        let v = visuals(t);
        let c = &t.canvas;
        let selected = over(v.selection.bg_fill, t.panel);
        vec![
            Pair {
                name: "ink-on-panel",
                fg: t.ink,
                bg: t.panel,
                min: TEXT,
            },
            Pair {
                name: "ink-on-surface",
                fg: t.ink,
                bg: t.surface,
                min: TEXT,
            },
            Pair {
                name: "ink-on-hover",
                fg: t.ink,
                bg: t.hover,
                min: TEXT,
            },
            Pair {
                name: "ink-on-icon-button",
                fg: t.ink,
                bg: t.icon_fill,
                min: TEXT,
            },
            Pair {
                name: "ink-on-selection",
                fg: t.ink,
                bg: selected,
                min: TEXT,
            },
            Pair {
                name: "muted-on-panel",
                fg: t.muted,
                bg: t.panel,
                min: TEXT,
            },
            Pair {
                name: "muted-on-surface",
                fg: t.muted,
                bg: t.surface,
                min: TEXT,
            },
            Pair {
                name: "muted-on-hover",
                fg: t.muted,
                bg: t.hover,
                min: TEXT,
            },
            Pair {
                name: "accent-text-on-panel",
                fg: t.accent,
                bg: t.panel,
                min: TEXT,
            },
            Pair {
                name: "accent-text-on-surface",
                fg: t.accent,
                bg: t.surface,
                min: TEXT,
            },
            Pair {
                name: "accent-text-on-hover",
                fg: t.accent,
                bg: t.hover,
                min: TEXT,
            },
            Pair {
                name: "link-on-panel",
                fg: t.link,
                bg: t.panel,
                min: TEXT,
            },
            Pair {
                name: "link-on-surface",
                fg: t.link,
                bg: t.surface,
                min: TEXT,
            },
            Pair {
                name: "warning-on-panel",
                fg: t.warn,
                bg: t.panel,
                min: TEXT,
            },
            Pair {
                name: "warning-on-surface",
                fg: t.warn,
                bg: t.surface,
                min: TEXT,
            },
            Pair {
                name: "error-on-panel",
                fg: t.error,
                bg: t.panel,
                min: TEXT,
            },
            Pair {
                name: "error-on-surface",
                fg: t.error,
                bg: t.surface,
                min: TEXT,
            },
            Pair {
                name: "weak-text-on-panel",
                fg: v.weak_text_color(),
                bg: t.panel,
                min: TEXT,
            },
            Pair {
                name: "noninteractive-text-on-fill",
                fg: v.widgets.noninteractive.fg_stroke.color,
                bg: v.widgets.noninteractive.weak_bg_fill,
                min: TEXT,
            },
            Pair {
                name: "inactive-text-on-fill",
                fg: v.widgets.inactive.fg_stroke.color,
                bg: v.widgets.inactive.weak_bg_fill,
                min: TEXT,
            },
            Pair {
                name: "hovered-text-on-fill",
                fg: v.widgets.hovered.fg_stroke.color,
                bg: v.widgets.hovered.weak_bg_fill,
                min: TEXT,
            },
            Pair {
                name: "focus-ring-on-panel",
                fg: v.selection.stroke.color,
                bg: t.panel,
                min: NON_TEXT,
            },
            Pair {
                name: "focus-ring-on-surface",
                fg: v.selection.stroke.color,
                bg: t.surface,
                min: NON_TEXT,
            },
            Pair {
                name: "hovered-border-on-panel",
                fg: v.widgets.hovered.bg_stroke.color,
                bg: t.panel,
                min: NON_TEXT,
            },
            Pair {
                name: "front-copper-on-board",
                fg: c.front,
                bg: c.board,
                min: NON_TEXT,
            },
            Pair {
                name: "back-copper-on-board",
                fg: c.back,
                bg: c.board,
                min: NON_TEXT,
            },
            Pair {
                name: "pad-on-board",
                fg: c.pad,
                bg: c.board,
                min: NON_TEXT,
            },
            Pair {
                name: "net-tie-on-board",
                fg: c.net_tie,
                bg: c.board,
                min: NON_TEXT,
            },
            Pair {
                name: "mask-on-board",
                fg: c.mask,
                bg: c.board,
                min: NON_TEXT,
            },
            Pair {
                name: "net-highlight-on-board",
                fg: t.accent,
                bg: c.board,
                min: NON_TEXT,
            },
            Pair {
                name: "cut-marker-on-board",
                fg: c.front,
                bg: c.board,
                min: NON_TEXT,
            },
            Pair {
                name: "board-edge-on-canvas",
                fg: t.accent,
                bg: t.surface,
                min: NON_TEXT,
            },
            Pair {
                name: "wire-on-canvas",
                fg: c.wire,
                bg: t.surface,
                min: NON_TEXT,
            },
            Pair {
                name: "pad-label-hole-on-hole",
                fg: c.pad_label_hole,
                bg: t.surface,
                min: TEXT,
            },
            Pair {
                name: "pad-label-smd-on-pad",
                fg: c.pad_label_smd,
                bg: c.pad,
                min: TEXT,
            },
        ]
    }

    /// `theme/pair: ratio < min` lines for every failing pair of one theme.
    fn failing(theme: &str, t: &ThemeColours) -> Vec<(String, String)> {
        pairs(t)
            .into_iter()
            .filter_map(|p| {
                let ratio = contrast_ratio(p.fg, p.bg);
                (ratio < p.min).then(|| {
                    (
                        p.name.to_string(),
                        format!("{theme}/{}: {ratio:.2}:1, needs {:.1}:1", p.name, p.min),
                    )
                })
            })
            .collect()
    }

    /// The exemption names written in `contrast-exemptions.txt`.
    fn exemptions() -> Result<Vec<(String, String)>, String> {
        let mut out = Vec::new();
        for line in include_str!("../contrast-exemptions.txt").lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (name, reason) = line
                .split_once('=')
                .ok_or_else(|| format!("exemption without a reason: {line}"))?;
            if reason.trim().is_empty() {
                return Err(format!("exemption without a reason: {line}"));
            }
            out.push((name.trim().to_string(), reason.trim().to_string()));
        }
        Ok(out)
    }

    /// Every shipped theme: both modes, the default accent, two more, and the
    /// picker extremes.
    const THEMES: [(&str, bool, [u8; 3]); 14] = [
        ("dark/default", true, [91, 224, 188]),
        ("dark/orange", true, [255, 120, 60]),
        ("dark/blue", true, [80, 140, 255]),
        ("dark/black", true, [0, 0, 0]),
        ("dark/white", true, [255, 255, 255]),
        ("dark/dim", true, [30, 30, 60]),
        ("dark/red", true, [255, 0, 0]),
        ("light/default", false, [91, 224, 188]),
        ("light/orange", false, [255, 120, 60]),
        ("light/blue", false, [80, 140, 255]),
        ("light/black", false, [0, 0, 0]),
        ("light/white", false, [255, 255, 255]),
        ("light/yellow", false, [255, 255, 0]),
        ("light/red", false, [255, 0, 0]),
    ];

    #[test]
    fn contrast_ratio_matches_known_values() {
        let white = Color32::WHITE;
        let black = Color32::BLACK;
        assert!((contrast_ratio(black, white) - 21.0).abs() < 1e-9);
        assert!(
            (contrast_ratio(white, black) - 21.0).abs() < 1e-9,
            "symmetric"
        );
        let grey = Color32::from_rgb(119, 119, 119);
        assert!((contrast_ratio(grey, grey) - 1.0).abs() < 1e-12);
        // #777777 on white is the textbook 4.48:1 near-miss for AA text.
        assert!((contrast_ratio(grey, white) - 4.48).abs() < 0.01);
        // #767676 on white is the darkest grey that passes: 4.54:1.
        let pass = Color32::from_rgb(118, 118, 118);
        assert!((contrast_ratio(pass, white) - 4.54).abs() < 0.01);
    }

    #[test]
    fn every_theme_meets_contrast_minimums() -> Result<(), String> {
        let exempt = exemptions()?;
        let mut failures = Vec::new();
        let mut still_failing = Vec::new();
        for (theme, dark, accent) in THEMES {
            for (name, line) in failing(theme, &palette(dark, accent)) {
                if exempt.iter().any(|(e, _)| *e == name) {
                    still_failing.push(name);
                } else {
                    failures.push(line);
                }
            }
        }
        assert!(
            failures.is_empty(),
            "pairs below the WCAG minimum:\n{}",
            failures.join("\n")
        );
        for (name, reason) in &exempt {
            assert!(
                still_failing.contains(name),
                "exemption {name} ({reason}) no longer fails; remove it"
            );
        }
        Ok(())
    }

    #[test]
    fn adding_a_theme_without_passing_the_check_fails() {
        let mut bad = palette(true, [91, 224, 188]);
        bad.ink = Color32::from_rgb(40, 56, 80);
        bad.canvas.front = bad.canvas.board;
        let names: Vec<String> = failing("fixture", &bad).into_iter().map(|f| f.0).collect();
        assert!(names.contains(&"ink-on-panel".to_string()), "{names:?}");
        assert!(
            names.contains(&"front-copper-on-board".to_string()),
            "{names:?}"
        );
    }

    /// Every role pair of the design tokens for one theme, with its minimum.
    ///
    /// Why: the design's colour roles are checked pair by pair, so a changed token
    /// that breaks reading fails here, not in front of a user.
    fn token_pairs(t: &Tokens) -> Vec<Pair> {
        use crate::ui_kit::button::{brighten, darken};
        let mut pairs = Vec::new();
        let mut add = |name: &'static str, fg: Color32, bg: Color32, min: f64| {
            pairs.push(Pair { name, fg, bg, min });
        };
        for bg in [t.ground, t.panel, t.raised, t.hover] {
            add("text-on-surface", t.text, bg, TEXT);
            add("muted-on-surface", t.muted, bg, TEXT);
            add("copper-on-surface", t.copper, bg, TEXT);
            add("mask-on-surface", t.mask, bg, TEXT);
            add("gold-on-surface", t.gold, bg, TEXT);
            add("cyan-on-surface", t.cyan, bg, TEXT);
            add("red-on-surface", t.red, bg, TEXT);
        }
        add("onacc-on-copper", t.onacc, t.copper, TEXT);
        add("onacc-on-copper-hover", t.onacc, brighten(t.copper), TEXT);
        add("onacc-on-copper-active", t.onacc, darken(t.copper), TEXT);
        for bg in [t.panel, t.ground] {
            add(
                "copper-on-copper-tint",
                t.copper,
                composite(t.copper_tint, bg),
                TEXT,
            );
            add(
                "gold-on-gold-tint",
                t.gold,
                composite(t.gold_tint, bg),
                TEXT,
            );
            add("red-on-red-tint", t.red, composite(t.red_tint, bg), TEXT);
            add(
                "mask-on-mask-tint",
                t.mask,
                composite(t.mask_tint, bg),
                TEXT,
            );
            add(
                "text-on-copper-tint",
                t.text,
                composite(t.copper_tint, bg),
                TEXT,
            );
        }
        for bg in [t.ground, t.panel, t.raised] {
            add("edge-on-surface", t.edge, bg, NON_TEXT);
        }
        add("copper-bar-on-panel", t.copper, t.panel, NON_TEXT);
        add("focus-ring-on-panel", t.text, t.panel, NON_TEXT);
        add("focus-ring-on-ground", t.text, t.ground, NON_TEXT);
        pairs
    }

    /// The pairs of `t` that fall below their minimum, as readable lines.
    fn token_failures(label: &str, t: &Tokens) -> Vec<String> {
        token_pairs(t)
            .into_iter()
            .filter_map(|p| {
                let ratio = contrast_ratio(p.fg, p.bg);
                (ratio < p.min)
                    .then(|| format!("{label}/{}: {ratio:.2}:1, needs {:.1}:1", p.name, p.min))
            })
            .collect()
    }

    /// Both shipped themes pass every token pair.
    #[test]
    fn every_design_token_pair_meets_contrast_in_both_themes() {
        let mut failures = token_failures("dark", &Tokens::DARK);
        failures.extend(token_failures("light", &Tokens::LIGHT));
        assert!(
            failures.is_empty(),
            "token pairs below the minimum:\n{}",
            failures.join("\n")
        );
    }

    /// A deliberately bad token turns the gate red, in either text or non-text pairs.
    #[test]
    fn a_deliberately_bad_token_is_caught() {
        let mut bad = Tokens::DARK;
        bad.muted = Color32::from_rgb(60, 80, 72);
        let lines = token_failures("bad", &bad);
        assert!(
            lines.iter().any(|l| l.contains("muted-on-surface")),
            "{lines:?}"
        );
        let mut bad_edge = Tokens::LIGHT;
        bad_edge.edge = bad_edge.panel;
        let lines = token_failures("bad", &bad_edge);
        assert!(
            lines.iter().any(|l| l.contains("edge-on-surface")),
            "{lines:?}"
        );
        let mut bad_tint = Tokens::DARK;
        bad_tint.copper = Color32::from_rgb(90, 60, 40);
        assert!(!token_failures("bad", &bad_tint).is_empty());
    }
}
