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
//! - Embed a CJK font in the binary; they load on demand (see "CJK fallback" below).
//!
//! CJK fallback: IBM Plex has no Japanese or Chinese glyphs, so those scripts need a
//! font file. Files live in `crates/ui-egui/assets/fonts/` and ship beside the program,
//! never inside the binary or the wasm. They are appended behind every Plex family so
//! Latin text keeps Plex and only missing glyphs fall through. Native builds read
//! `fonts/` next to the executable or `../share/pcbcraft/fonts`; the browser build
//! fetches the file and calls [`add_fallback`]. One regular-weight file per language;
//! bold and titles use the same file.
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

/// The font file language `tag` needs, or `None` when the built-in fonts
/// already draw it.
///
/// # Rules
/// - One file per region (`ja`, `zh`); there is no single CJK font.
/// - Regular weight only.
///
/// # Dependencies
/// - `crates/ui-egui/assets/fonts/NotoSansJP-Regular.otf` and `NotoSansSC-Regular.otf` ship with `OFL-NotoSansCJK.txt`.
///
/// # Do not
/// - Do not add a system-font lookup here; a later issue may add one.
/// - Do not return a file for a language Plex already draws.
pub fn font_file_for(tag: &str) -> Option<&'static str> {
    match tag {
        "ja" => Some("NotoSansJP-Regular.otf"),
        "zh" => Some("NotoSansSC-Regular.otf"),
        _ => None,
    }
}

/// The family name for a font file: the file name without its extension.
/// The browser build needs this to install a fetched file under the same
/// name the native build uses.
///
/// # Do not
/// - Do not shorten to a language tag; two regions may share a tag.
pub fn font_stem(file: &str) -> &str {
    file.strip_suffix(".otf").unwrap_or(file)
}

/// Append font `name` (bytes of an OpenType or TrueType file) to the end
/// of the Proportional, Monospace, Plex Medium and Plex SemiBold families of `ctx`.
///
/// Appending, not replacing, keeps Latin text on the default font; only
/// characters no earlier font can draw fall through to this one. The font
/// becomes active at the start of the next frame. The call is idempotent:
/// a second call with the same name changes nothing.
///
/// # Rules
/// - The font goes last in both families so it is the last resort.
///
/// # Dependencies
/// - `apps/pcbcraft-web/src/main.rs` feeds fetched bytes to this function;
///   `load_for_language` below is the native caller.
///
/// # Do not
/// - Do not insert the font at the front; that would restyle Latin text.
pub fn add_fallback(ctx: &egui::Context, name: &str, bytes: Vec<u8>) {
    ctx.add_font(egui::epaint::text::FontInsert {
        name: name.to_string(),
        data: egui::FontData::from_owned(bytes),
        families: vec![
            egui::epaint::text::InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: egui::epaint::text::FontPriority::Lowest,
            },
            egui::epaint::text::InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: egui::epaint::text::FontPriority::Lowest,
            },
            egui::epaint::text::InsertFontFamily {
                family: medium(),
                priority: egui::epaint::text::FontPriority::Lowest,
            },
            egui::epaint::text::InsertFontFamily {
                family: semibold(),
                priority: egui::epaint::text::FontPriority::Lowest,
            },
        ],
    });
}

/// The folders holding font files, in search order: `fonts/` next to the
/// running program (portable archives and app bundles), then the
/// `../share/pcbcraft/fonts` folder system packages install to.
#[cfg(not(target_arch = "wasm32"))]
fn fonts_dirs() -> Vec<std::path::PathBuf> {
    let Some(exe) = std::env::current_exe().ok() else {
        return Vec::new();
    };
    let Some(dir) = exe.parent() else {
        return Vec::new();
    };
    vec![dir.join("fonts"), dir.join("../share/pcbcraft/fonts")]
}

/// Load the fallback font for language `tag` from the folders listed by
/// `fonts_dirs` below.
///
/// Returns `Some(file)` when the language needs a font that is not on
/// disk, so the caller can keep the default fonts and say so in plain
/// words. Languages with no entry in [`font_file_for`] return `None` and
/// change nothing.
///
/// # Rules
/// - Read relative to the program, never the current directory.
///
/// # Dependencies
/// - `crates/ui-egui/assets/fonts/` holds the files installers copy next to the program.
///
/// # Do not
/// - Do not fail the app when the file is missing; boxes are survivable.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_for_language(ctx: &egui::Context, tag: &str) -> Option<&'static str> {
    let file = font_file_for(tag)?;
    for dir in fonts_dirs() {
        if let Ok(bytes) = std::fs::read(dir.join(file)) {
            add_fallback(ctx, font_stem(file), bytes);
            return None;
        }
    }
    Some(file)
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

    /// The shipped Japanese font, kept in the repository with its license
    /// file beside it; the coverage test reads it from there.
    const JP_FONT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/fonts/NotoSansJP-Regular.otf"
    );

    fn jp_font_bytes() -> Vec<u8> {
        std::fs::read(JP_FONT).unwrap_or_default()
    }

    /// Run one frame so queued fonts become active. This test has no GPU
    /// renderer; discard texture uploads on purpose.
    fn settle(ctx: &egui::Context) {
        let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {});
        output.textures_delta.clear();
    }

    #[test]
    fn every_character_in_a_ja_test_string_has_a_glyph_in_the_loaded_font() {
        let bytes = jp_font_bytes();
        assert!(
            !bytes.is_empty(),
            "{JP_FONT} must exist with its license file beside it"
        );
        // Words a Japanese user meets in an EDA: board, design, wiring,
        // settings, display, plus both kana scripts and a middle dot.
        let sample = "基板設計配線設定表示回路寸法 かなひらがな カタカナピカ・全角";
        let parsed = skrifa::FontRef::new(&bytes);
        assert!(
            parsed.is_ok(),
            "the font file did not parse: {:?}",
            parsed.as_ref().err()
        );
        if let Ok(font) = &parsed {
            use skrifa::MetadataProvider;
            let cmap = font.charmap();
            for ch in sample.chars() {
                assert!(
                    cmap.map(ch as u32).is_some(),
                    "NotoSansJP-Regular.otf has no glyph for {ch}"
                );
            }
        }
    }

    #[test]
    fn adding_a_fallback_font_keeps_latin_on_the_default_font() {
        let ctx = egui::Context::default();
        add_fallback(&ctx, "NotoSansJP-Regular", jp_font_bytes());
        settle(&ctx);
        let definitions = ctx.fonts(|view| view.definitions().clone());
        let proportional = definitions
            .families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        let default_first = egui::FontDefinitions::default()
            .families
            .get(&egui::FontFamily::Proportional)
            .and_then(|list| list.first())
            .cloned();
        assert_eq!(
            proportional.first(),
            default_first.as_ref(),
            "Latin text stays on the default font"
        );
        assert_eq!(
            proportional.last().map(String::as_str),
            Some("NotoSansJP-Regular"),
            "the CJK font is the last resort"
        );
        let monospace = definitions
            .families
            .get(&egui::FontFamily::Monospace)
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            monospace.last().map(String::as_str),
            Some("NotoSansJP-Regular"),
            "the CJK font also backs the monospace family"
        );
        assert!(definitions.font_data.contains_key("NotoSansJP-Regular"));
    }

    #[test]
    fn adding_the_same_fallback_twice_changes_nothing() {
        let ctx = egui::Context::default();
        add_fallback(&ctx, "NotoSansJP-Regular", jp_font_bytes());
        settle(&ctx);
        add_fallback(&ctx, "NotoSansJP-Regular", jp_font_bytes());
        settle(&ctx);
        let definitions = ctx.fonts(|view| view.definitions().clone());
        let proportional = definitions
            .families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            proportional
                .iter()
                .filter(|n| *n == "NotoSansJP-Regular")
                .count(),
            1
        );
    }

    /// The Chinese font covers common simplified characters an EDA user meets.
    #[test]
    fn every_character_in_a_zh_test_string_has_a_glyph_in_the_sc_font() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/NotoSansSC-Regular.otf"
        );
        let bytes = std::fs::read(path).unwrap_or_default();
        assert!(!bytes.is_empty(), "{path} must exist");
        let parsed = skrifa::FontRef::new(&bytes);
        assert!(parsed.is_ok(), "the font file did not parse");
        if let Ok(font) = &parsed {
            use skrifa::MetadataProvider;
            let cmap = font.charmap();
            for ch in "电路板设计布线设置显示尺寸".chars() {
                assert!(cmap.map(ch as u32).is_some(), "no glyph for {ch}");
            }
        }
    }

    /// Medium and SemiBold text (buttons, titles) also falls back to the CJK font.
    #[test]
    fn bold_families_fall_back_to_the_cjk_font_too() {
        let ctx = egui::Context::default();
        install(&ctx);
        add_fallback(&ctx, "NotoSansJP-Regular", jp_font_bytes());
        settle(&ctx);
        settle(&ctx);
        let definitions = ctx.fonts(|view| view.definitions().clone());
        for family in [medium(), semibold()] {
            let chain = definitions
                .families
                .get(&family)
                .cloned()
                .unwrap_or_default();
            assert_eq!(chain.last().map(String::as_str), Some("NotoSansJP-Regular"));
            assert_eq!(chain.first().map(|n| n.starts_with("plex")), Some(true));
        }
    }

    #[test]
    fn font_file_for_names_one_regular_file_per_language() {
        assert_eq!(font_file_for("ja"), Some("NotoSansJP-Regular.otf"));
        assert_eq!(font_file_for("zh"), Some("NotoSansSC-Regular.otf"));
        assert_eq!(font_file_for("en"), None);
        assert_eq!(font_file_for(""), None);
    }
}
