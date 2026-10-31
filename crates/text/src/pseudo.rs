//! The pseudo-locale `en-XA`: English made accented and 40% longer.
//!
//! Context: built from `crates/text/strings/en.txt` at test time, never shipped
//! as a language. A screen that clips it, or lets a control leave its region,
//! will do the same to German or Finnish. Used by `crates/ui-egui/src/e2e/i18n.rs`.
//! Local rules: `{placeholder}` names are copied untouched so format calls still fill them.

use crate::{Catalog, parse};

/// The tag of the pseudo-locale.
pub const PSEUDO_TAG: &str = "en-XA";

/// Share of extra length added to every value.
const GROWTH_PERCENT: usize = 40;

/// One accented stand-in per ASCII letter; other characters stay.
fn accent(c: char) -> char {
    const FROM: &str = "aeiouyncAEIOUYNCsSzZ";
    const TO: &str = "àéîöûýñçÀÉÎÖÛÝÑÇšŠžŽ";
    FROM.chars()
        .position(|f| f == c)
        .and_then(|i| TO.chars().nth(i))
        .unwrap_or(c)
}

/// Accent the letters of `value` and pad it to 40% more characters.
///
/// Why: real translations run longer than English, and an accented letter proves
/// the font and the text path cope with more than ASCII.
///
/// # Rules
/// - Text inside `{...}` is copied as is.
/// - Padding is words of `~~` so the line can still wrap; an empty value stays empty.
///
/// # Dependencies
/// - `crates/text/strings/en.txt` supplies the values.
///
/// # Do not
/// - Ship the result as a language; it exists for tests.
pub fn pseudo_localize(value: &str) -> String {
    if value.trim().is_empty() {
        return value.to_owned();
    }
    let mut out = String::new();
    let mut in_name = false;
    let mut visible = 0usize;
    for c in value.chars() {
        match c {
            '{' => in_name = true,
            '}' => in_name = false,
            _ if !in_name => visible += 1,
            _ => {}
        }
        out.push(if in_name || c == '{' || c == '}' {
            c
        } else {
            accent(c)
        });
    }
    let extra = visible * GROWTH_PERCENT / 100;
    let mut added = 0;
    while added < extra {
        out.push_str(if added == 0 { " ~~" } else { "~~" });
        added += 3;
        if added < extra {
            out.push(' ');
            added += 1;
        }
    }
    out
}

impl Catalog {
    /// English with every value passed through [`pseudo_localize`], tagged `en-XA`.
    ///
    /// # Rules
    /// - Only for tests; the tag is not in `shipped_languages`.
    pub fn pseudo() -> Self {
        let text: String = parse(crate::ENGLISH)
            .0
            .iter()
            .map(|(key, value)| format!("{key} = {}\n", pseudo_localize(value)))
            .collect();
        Self::english().with_language(PSEUDO_TAG, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placeholders;

    #[test]
    fn letters_are_accented_and_text_is_forty_percent_longer() {
        let out = pseudo_localize("Open the board");
        assert!(out.starts_with("Öpéñ"), "{out}");
        assert!(out.chars().count() >= 14 * 140 / 100, "{out}");
    }

    #[test]
    fn placeholders_survive_untouched() {
        let out = pseudo_localize("Version {version} of {name}");
        assert_eq!(
            placeholders(&out),
            placeholders("Version {version} of {name}")
        );
    }

    #[test]
    fn the_pseudo_catalog_defines_every_english_key() {
        let pseudo = Catalog::pseudo();
        assert_eq!(pseudo.tag(), PSEUDO_TAG);
        assert_eq!(pseudo.get("app.about"), pseudo_localize("About"));
        assert!(pseudo.missing_keys().is_empty());
    }

    #[test]
    fn an_empty_value_stays_empty() {
        assert_eq!(pseudo_localize(""), "");
    }
}
