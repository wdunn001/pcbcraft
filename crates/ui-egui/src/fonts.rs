//! On-demand fallback fonts for scripts the built-in fonts cannot draw.
//!
//! The built-in fonts carry no CJK glyphs, so Japanese or Chinese text
//! paints as empty boxes until a fallback font is loaded. Fonts ship as
//! separate data files, never inside the wasm binary, and are appended to
//! the end of each family so Latin text keeps the default font and only
//! missing glyphs fall through.
//!
//! Local rules:
//! - One font file per regional language; Regular weight only. Bold falls
//!   back to Regular because a second file would double the download.
//! - The native build reads fonts only from `fonts/` beside the program
//!   or the `../share/pcbcraft/fonts` folder system packages install to,
//!   never from the current directory.
//! - The browser build fetches the file over HTTP and calls
//!   [`add_fallback`] itself; this crate performs no fetch.

/// The font file language `tag` needs, or `None` when the built-in fonts
/// already draw it.
///
/// # Rules
/// - One file per region (`ja`, `zh`); there is no single CJK font.
/// - Regular weight only.
///
/// # Dependencies
/// - `assets/fonts/NotoSansJP-Regular.otf` ships with its license file.
///
/// # Do not
/// - Do not add a system-font lookup here; a later issue may add one.
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
/// of the Proportional and Monospace families of `ctx`.
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
/// - `assets/fonts/` holds the files installers copy next to the program.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped Japanese font, kept in the repository with its license
    /// file beside it; the coverage test reads it from there.
    const JP_FONT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/fonts/NotoSansJP-Regular.otf"
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

    #[test]
    fn font_file_for_names_one_regular_file_per_language() {
        assert_eq!(font_file_for("ja"), Some("NotoSansJP-Regular.otf"));
        assert_eq!(font_file_for("zh"), Some("NotoSansSC-Regular.otf"));
        assert_eq!(font_file_for("en"), None);
        assert_eq!(font_file_for(""), None);
    }
}
