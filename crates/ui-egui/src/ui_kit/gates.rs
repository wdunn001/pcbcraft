//! Gates for the kit: accessible names and developer wording.
//!
//! Context: design system rules 1 and 2 (no developer text; every control has a
//! name) and ticket #764. The contrast gate lives next to the existing contrast
//! tests in `crates/ui-egui/src/contrast.rs`. These tests run in `cargo test`, so CI
//! fails when a kit control has no accessible name, or when a catalog string reads
//! like debug output.
//! Local rules: test-only; the detector is a plain function so other tests can call it.
//! Strings that still carry developer wording are listed with an issue in
//! `crates/ui-egui/developer-wording-exemptions.txt`; the list can only shrink.
//!
//! # Dependencies
//! - crates/ui-egui/src/a11y.rs
//! - crates/ui-egui/src/ui_kit/gallery.rs
//! - crates/text/strings/en.txt
//! - crates/ui-egui/developer-wording-exemptions.txt
//!
//! # Do not
//! - Add an exemption without an issue number; fix the string instead.

/// Why a piece of UI text counts as developer wording, or `None` when it reads as plain language.
///
/// Why: a first-time user should never see revision counters, key hint lists, tool
/// readouts or command ids.
///
/// # Rules
/// - Revision counters: `rev 16`, `rev {revision}`, `revision 3`.
/// - Key hint lists: a `Keys:` lead, or three or more `Key action` segments in a row.
/// - Tool readouts: a leading `Tool:`.
/// - Command ids: snake_case words, `::` paths and dotted lowercase ids.
///
/// # Do not
/// - Flag a single shortcut in a tooltip; one shortcut belongs there.
pub(crate) fn developer_wording(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| c.is_whitespace() || ",;:()\"'".contains(c))
        .map(|w| w.trim_matches('.'))
        .filter(|w| !w.is_empty())
        .collect();
    for pair in words.windows(2) {
        if matches!(pair[0], "rev" | "revision")
            && pair[1]
                .trim_start_matches('#')
                .trim_start_matches('{')
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c.is_alphabetic())
            && (pair[1].starts_with('{')
                || pair[1]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit() || c == '#'))
        {
            return Some(format!("revision counter: {} {}", pair[0], pair[1]));
        }
    }
    if lower.trim_start().starts_with("tool:") {
        return Some("tool readout: Tool:".to_owned());
    }
    if lower.contains("keys:") {
        return Some("key hint list: Keys:".to_owned());
    }
    let hint_segments = text
        .split([',', ';', '/', '|', '\u{b7}'])
        .filter(|segment| {
            let mut parts = segment.split_whitespace();
            let Some(key) = parts.next() else {
                return false;
            };
            let key_like = matches!(key, "Esc" | "Del" | "Tab" | "Enter" | "Space")
                || (key.len() <= 2
                    && key
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
            key_like
                && parts
                    .next()
                    .is_some_and(|w| w.chars().all(|c| c.is_ascii_lowercase()))
        })
        .count();
    if hint_segments >= 3 {
        return Some("key hint list".to_owned());
    }
    if lower
        .split_whitespace()
        .any(|w| w.contains("::") && w.chars().any(|c| c.is_alphabetic()))
    {
        return Some("command id: path with ::".to_owned());
    }
    for word in &words {
        let snake = word.contains('_')
            && word
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            && !word.starts_with('_')
            && !word.ends_with('_');
        let dotted = word.matches('.').count() >= 2
            && word
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')
            && !word.starts_with("www");
        if snake || dotted {
            return Some(format!("command id: {word}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::button::{self, Size};
    use crate::ui_kit::icons;
    use crate::ui_kit::testing::{context, frame};

    /// Plain wording passes; each kind of developer wording is caught.
    #[test]
    fn the_detector_separates_plain_from_developer_wording() {
        for plain in [
            "Run checks",
            "Save the project (Ctrl+S).",
            "Every edit can be undone.",
            "Corners are cut at 45 degrees.",
            "3 warnings",
            "Go to Plot files",
        ] {
            assert_eq!(developer_wording(plain), None, "{plain}");
        }
        for (bad, kind) in [
            ("rev {revision}", "revision"),
            ("rev 16", "revision"),
            ("Revision 3", "revision"),
            ("Tool: Pick", "tool"),
            ("Keys: W wire, X route", "key hint"),
            ("W wire, X route, M move", "key hint"),
            ("Run set_net_class now", "command"),
            ("engine::apply failed", "command"),
            ("calls place.part.add", "command"),
        ] {
            let got = developer_wording(bad);
            assert!(
                got.as_deref().is_some_and(|g| g.contains(kind)),
                "{bad}: {got:?}"
            );
        }
    }

    /// Catalog entries still carrying developer wording: `key | #issue | reason`.
    fn exemptions() -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        for line in include_str!("../../developer-wording-exemptions.txt").lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('|').map(str::trim).collect();
            if parts.len() < 3 || !parts[1].starts_with('#') || parts[2].is_empty() {
                return Err(format!("exemption needs `key | #issue | reason`: {line}"));
            }
            keys.push(parts[0].to_owned());
        }
        Ok(keys)
    }

    /// Every catalog label and tooltip is plain language, except the listed ones.
    #[test]
    fn no_catalog_text_carries_developer_wording() -> Result<(), String> {
        let exempt = exemptions()?;
        let mut found = Vec::new();
        let mut hit_keys = Vec::new();
        for line in include_str!("../../../text/strings/en.txt").lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if let Some(why) = developer_wording(value) {
                hit_keys.push(key.to_owned());
                if !exempt.iter().any(|k| k == key) {
                    found.push(format!("{key} = {value}  ({why})"));
                }
            }
        }
        assert!(
            found.is_empty(),
            "developer wording in the catalog:\n{}",
            found.join("\n")
        );
        for key in &exempt {
            assert!(
                hit_keys.contains(key),
                "exemption {key} no longer matches developer wording; remove it"
            );
        }
        Ok(())
    }

    /// The names of interactive nodes in one frame that have no name.
    fn unnamed(out: &egui::FullOutput) -> Vec<egui::accesskit::Role> {
        crate::a11y::audit::interactive_nodes(out)
            .into_iter()
            .filter(|(_, name)| name.is_empty())
            .map(|(role, _)| role)
            .collect()
    }

    /// An icon button with no name is caught; the same button with a name passes.
    #[test]
    fn a_deliberately_unlabelled_icon_button_is_caught() {
        let ctx = context(true);
        let bad = frame(&ctx, vec![], |ui| {
            button::icon_button(ui, icons::PLUS, "", "Add a layer", Size::Normal, true);
        });
        assert_eq!(
            unnamed(&bad).len(),
            1,
            "the gate missed an unlabelled icon button"
        );
        let good = frame(&ctx, vec![], |ui| {
            button::icon_button(
                ui,
                icons::PLUS,
                "Add layer",
                "Add a layer",
                Size::Normal,
                true,
            );
        });
        assert!(unnamed(&good).is_empty());
    }
}
