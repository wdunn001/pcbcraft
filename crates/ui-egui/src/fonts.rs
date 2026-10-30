//! Embedded fonts, the five-size type scale and the helpers call sites use for text.
//!
//! Context: the approved design uses IBM Plex Sans (400, 500, 600) for the
//! interface and JetBrains Mono for numbers, references and net names. Both are
//! open-licence files under `crates/ui-egui/assets/fonts/`. `./theme.rs` calls
//! `apply_text_styles` so one `theme::apply` sets colours, spacing and type.
//! Local rules: only five sizes exist (caption 11, mono 12, body 13, title 16,
//! display 22); call sites ask for a role, never for a number.
//!
//! # Dependencies
//! - crates/ui-egui/assets/fonts/README.md lists the files and licences.
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/font-size-exemptions.txt lists call sites still to migrate.
//!
//! # Do not
//! - Pass a raw font size or call `RichText::small()` or `.heading()` in new code.
//! - Drop the default fonts behind the Plex families; they cover scripts Plex lacks.
use egui::{FontData, FontDefinitions, FontFamily, FontId, RichText, Style, TextStyle};
use std::sync::Arc;

/// Caption size in points (section captions, hints, table heads, badges).
pub const SIZE_CAPTION: f32 = 11.0;
/// Mono size in points (values, references, net names, file names).
pub const SIZE_MONO: f32 = 12.0;
/// Body size in points (everything else, buttons, tabs).
pub const SIZE_BODY: f32 = 13.0;
/// Title size in points (card titles).
pub const SIZE_TITLE: f32 = 16.0;
/// Display size in points (one per page).
pub const SIZE_DISPLAY: f32 = 22.0;

const PLEX_REGULAR: &str = "plex-regular";
const PLEX_MEDIUM: &str = "plex-medium";
const PLEX_SEMIBOLD: &str = "plex-semibold";
const JETBRAINS_MONO: &str = "jetbrains-mono";

/// A type role of the design: one size and one weight each.
///
/// Why: call sites say what the text is, and the scale decides how it looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// 11 regular: hints and captions.
    Caption,
    /// 12 JetBrains Mono: values and references.
    Mono,
    /// 13 regular: body text.
    Body,
    /// 13 medium: buttons and tabs.
    Button,
    /// 13 semibold: strong text and the active tab.
    Strong,
    /// 16 semibold: card titles.
    Title,
    /// 22 semibold: the page name.
    Display,
}

impl Role {
    /// The font (size and family) of this role.
    pub fn font(self) -> FontId {
        match self {
            Role::Caption => FontId::new(SIZE_CAPTION, FontFamily::Proportional),
            Role::Mono => FontId::new(SIZE_MONO, FontFamily::Monospace),
            Role::Body => FontId::new(SIZE_BODY, FontFamily::Proportional),
            Role::Button => FontId::new(SIZE_BODY, medium()),
            Role::Strong => FontId::new(SIZE_BODY, semibold()),
            Role::Title => FontId::new(SIZE_TITLE, semibold()),
            Role::Display => FontId::new(SIZE_DISPLAY, semibold()),
        }
    }
}

/// The Plex Medium family (500).
pub fn medium() -> FontFamily {
    FontFamily::Name(PLEX_MEDIUM.into())
}

/// The Plex SemiBold family (600).
pub fn semibold() -> FontFamily {
    FontFamily::Name(PLEX_SEMIBOLD.into())
}

/// The named text style of the page title (22 semibold).
pub fn display_style() -> TextStyle {
    TextStyle::Name("display".into())
}

/// Text in a role, as `RichText`.
///
/// Why: the only sanctioned way to set a font on a label, so no call site
/// carries a raw size.
pub fn text(role: Role, s: impl Into<String>) -> RichText {
    RichText::new(s).font(role.font())
}

/// Caption text, 11 regular.
pub fn caption(s: impl Into<String>) -> RichText {
    text(Role::Caption, s)
}

/// Section caption: 11, 600 weight, uppercase.
pub fn section_caption(s: impl AsRef<str>) -> RichText {
    RichText::new(s.as_ref().to_uppercase()).font(FontId::new(SIZE_CAPTION, semibold()))
}

/// Mono text, 12 JetBrains Mono.
pub fn mono(s: impl Into<String>) -> RichText {
    text(Role::Mono, s)
}

/// Strong body text, 13 semibold.
pub fn strong(s: impl Into<String>) -> RichText {
    text(Role::Strong, s)
}

/// Card title text, 16 semibold.
pub fn title(s: impl Into<String>) -> RichText {
    text(Role::Title, s)
}

/// Page name text, 22 semibold.
pub fn display(s: impl Into<String>) -> RichText {
    text(Role::Display, s)
}

fn data(bytes: &'static [u8]) -> Arc<FontData> {
    Arc::new(FontData::from_static(bytes))
}

/// The font set of the app: Plex and JetBrains Mono first, egui's defaults behind.
///
/// # Rules
/// - `Proportional` starts with Plex Regular; `Monospace` starts with JetBrains Mono.
/// - Each named Plex weight keeps the default fallbacks, so glyphs Plex lacks still draw.
///
/// # Dependencies
/// - crates/ui-egui/assets/fonts/README.md
///
/// # Do not
/// - Remove the default families; that would blank scripts the bundled fonts do not cover.
pub fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        PLEX_REGULAR.into(),
        data(include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf")),
    );
    fonts.font_data.insert(
        PLEX_MEDIUM.into(),
        data(include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf")),
    );
    fonts.font_data.insert(
        PLEX_SEMIBOLD.into(),
        data(include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf")),
    );
    fonts.font_data.insert(
        JETBRAINS_MONO.into(),
        data(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
    );
    let fallback: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for name in [PLEX_MEDIUM, PLEX_SEMIBOLD] {
        let mut chain = vec![name.to_owned()];
        chain.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), chain);
    }
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, PLEX_REGULAR.to_owned());
    let monospace = fonts.families.entry(FontFamily::Monospace).or_default();
    monospace.insert(0, JETBRAINS_MONO.to_owned());
    fonts
}

/// Register the fonts on a context. Call once at start-up.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

/// Set the five text styles of the design on a `Style`.
///
/// # Rules
/// - Small 11, Monospace 12, Body 13, Button 13 medium, Heading 16 semibold, `display` 22 semibold.
pub fn apply_text_styles(style: &mut Style) {
    style
        .text_styles
        .insert(TextStyle::Small, Role::Caption.font());
    style
        .text_styles
        .insert(TextStyle::Monospace, Role::Mono.font());
    style.text_styles.insert(TextStyle::Body, Role::Body.font());
    style
        .text_styles
        .insert(TextStyle::Button, Role::Button.font());
    style
        .text_styles
        .insert(TextStyle::Heading, Role::Title.font());
    style
        .text_styles
        .insert(display_style(), Role::Display.font());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// Every `.rs` file under a directory, sorted.
    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|x| x == "rs") {
                out.push(path);
            }
        }
    }

    /// How many ad hoc font calls a source line carries.
    fn ad_hoc_calls(line: &str) -> usize {
        let code = line.split("//").next().unwrap_or("");
        let mut n = 0;
        for needle in [
            ".small()",
            ".heading(",
            "FontId::proportional(",
            "FontId::monospace(",
            "FontId::new(",
        ] {
            n += code.matches(needle).count();
        }
        if code.contains("RichText") && code.contains(".size(") {
            n += 1;
        }
        n
    }

    /// The migration list: `path = count` of call sites left to move to the roles.
    fn exemptions() -> Result<std::collections::BTreeMap<String, usize>, String> {
        let mut map = std::collections::BTreeMap::new();
        for line in include_str!("../font-size-exemptions.txt").lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (path, count) = line
                .split_once('=')
                .ok_or_else(|| format!("bad exemption line: {line}"))?;
            let count: usize = count
                .trim()
                .parse()
                .map_err(|e| format!("bad count in {line}: {e}"))?;
            map.insert(path.trim().to_owned(), count);
        }
        Ok(map)
    }

    /// Counts of ad hoc font calls per source file, outside this file and the kit.
    fn counts() -> Vec<(String, usize)> {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        let mut out = Vec::new();
        for file in files {
            let rel = file
                .strip_prefix(env!("CARGO_MANIFEST_DIR"))
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            if rel.ends_with("src/fonts.rs") || rel.contains("src/ui_kit/") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            // Test modules after `#[cfg(test)]` are not production call sites.
            let production = text
                .split("\n#[cfg(test)]\nmod tests {")
                .next()
                .unwrap_or("");
            let n: usize = production.lines().map(ad_hoc_calls).sum();
            if n > 0 {
                out.push((rel, n));
            }
        }
        out
    }

    /// A call site that passes a raw font size fails, unless it is on the migration list.
    #[test]
    fn no_new_call_site_passes_a_raw_font_size() -> Result<(), String> {
        let exempt = exemptions()?;
        let mut bad = Vec::new();
        for (path, n) in counts() {
            let allowed = exempt.get(&path).copied().unwrap_or(0);
            if n > allowed {
                bad.push(format!("{path}: {n} ad hoc font calls, {allowed} allowed"));
            }
        }
        if bad.is_empty() {
            Ok(())
        } else {
            Err(bad.join("\n"))
        }
    }

    /// The detector sees the calls it must forbid.
    #[test]
    fn the_detector_flags_raw_sizes() {
        assert_eq!(ad_hoc_calls("ui.label(RichText::new(label).small());"), 1);
        assert_eq!(ad_hoc_calls("let a = x.small();"), 1);
        assert_eq!(ad_hoc_calls("ui.heading(\"x\");"), 1);
        assert_eq!(ad_hoc_calls("FontId::proportional(14.0)"), 1);
        assert_eq!(ad_hoc_calls("RichText::new(label).size(20.0)"), 1);
        assert_eq!(ad_hoc_calls("// ui.heading(\"x\")"), 0);
    }

    /// The five sizes are the only sizes the roles use.
    #[test]
    fn the_roles_use_only_the_five_sizes() {
        let allowed = [11.0, 12.0, 13.0, 16.0, 22.0];
        for role in [
            Role::Caption,
            Role::Mono,
            Role::Body,
            Role::Button,
            Role::Strong,
            Role::Title,
            Role::Display,
        ] {
            assert!(allowed.contains(&role.font().size), "{role:?}");
        }
        let mut style = Style::default();
        apply_text_styles(&mut style);
        for font in style.text_styles.values() {
            assert!(allowed.contains(&font.size), "{font:?}");
        }
        assert_eq!(style.text_styles.len(), 6);
    }

    /// The families resolve to Plex and JetBrains Mono with the defaults behind.
    #[test]
    fn families_put_the_bundled_fonts_first_and_keep_fallbacks() {
        let defs = definitions();
        let default_count = FontDefinitions::default()
            .families
            .get(&FontFamily::Proportional)
            .map_or(0, Vec::len);
        let proportional = &defs.families[&FontFamily::Proportional];
        assert_eq!(proportional[0], "plex-regular");
        assert_eq!(proportional.len(), default_count + 1);
        assert_eq!(defs.families[&FontFamily::Monospace][0], "jetbrains-mono");
        for family in [medium(), semibold()] {
            let chain = &defs.families[&family];
            assert_eq!(chain.len(), default_count + 1);
        }
    }

    /// Installing the fonts and laying out text in every role does not panic and sizes them.
    #[test]
    fn text_lays_out_in_every_role() {
        let ctx = egui::Context::default();
        install(&ctx);
        for _ in 0..2 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                for role in [
                    Role::Caption,
                    Role::Mono,
                    Role::Body,
                    Role::Title,
                    Role::Display,
                ] {
                    let r = ui.label(text(role, "Ag 0123 \u{3a9}"));
                    assert!(r.rect.height() > role.font().size * 0.9, "{role:?}");
                }
            });
            output.textures_delta.clear();
        }
    }
}
