//! Number boxes in the language's format: the display side and the entry side.
//!
//! Context: egui's `DragValue` lets a formatter and a parser be swapped in, so
//! one helper gives every numeric box the active language's decimal marker
//! on display and one entry rule on input (`,` or `.`, from #661).
//!
//! Local rules: the stored value is always an `f64` in the unit the box
//! already used; only the text changes. Nothing here writes files, G-code or
//! the clipboard; those stay plain C-locale through `pcbcraft_text::numbers::fmt_machine`.
use pcbcraft_text::numbers::{fmt_ui, parse_number};

/// Give a `DragValue` the language's decimal marker and the shared entry rule.
///
/// Shows at most `places` decimals with trailing zeros dropped (`1,6`, not
/// `1,60`). A typed entry that is not a number leaves the old value in place.
///
/// # Rules
/// Display uses [`fmt_ui`]; entry uses [`parse_number`] and accepts both
/// markers in every language. Pass the tag from `Catalog::number_tag`.
///
/// # Dependencies
/// `../../text/src/numbers.rs` for `fmt_ui` and `parse_number`.
///
/// # Do not
/// Use on integer counters, and never on values that are written out.
pub fn localized<'a>(drag: egui::DragValue<'a>, tag: &str, places: usize) -> egui::DragValue<'a> {
    let tag = tag.to_string();
    drag.custom_formatter(move |value, range| {
        let max = places.max(*range.end());
        trim_zeros(&tag, fmt_ui(&tag, value, max), *range.start())
    })
    .custom_parser(|text| parse_box(text).ok())
}

/// [`localized`] with the tag of the language now showing.
///
/// # Rules
/// Same as [`localized`].
/// # Dependencies
/// `../../text/src/lib.rs` (`active_number_tag`).
/// # Do not
/// Cache the result across frames; the language can change.
pub fn localized_active(drag: egui::DragValue<'_>, places: usize) -> egui::DragValue<'_> {
    localized(drag, &pcbcraft_text::active_number_tag(), places)
}

/// Read a number typed in a text box, or the plain refusal message.
///
/// # Rules
/// Same entry rule as the drag boxes.
/// # Dependencies
/// `../../text/src/numbers.rs`.
/// # Do not
/// Show the error text anywhere but next to the box it came from.
pub fn parse_box(text: &str) -> Result<f64, String> {
    parse_number(text)
}

/// Drop trailing zeros after the decimal marker, keeping `min` decimals.
fn trim_zeros(tag: &str, text: String, min: usize) -> String {
    let marker = pcbcraft_text::numbers::style(tag).decimal;
    let Some(at) = text.find(marker) else {
        return text;
    };
    let keep_to = at + marker.len_utf8() + min;
    let mut end = text.len();
    while end > keep_to && text.as_bytes()[end - 1] == b'0' {
        end -= 1;
    }
    if end == at + marker.len_utf8() {
        end = at;
    }
    text[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_value_shows_a_decimal_comma_in_german_and_a_point_in_english() {
        assert_eq!(trim_zeros("de", fmt_ui("de", 1.6, 3), 0), "1,6");
        assert_eq!(trim_zeros("en", fmt_ui("en", 1.6, 3), 0), "1.6");
        assert_eq!(trim_zeros("de", fmt_ui("de", 2.0, 3), 0), "2");
        assert_eq!(trim_zeros("de", fmt_ui("de", 0.2, 3), 0), "0,2");
        assert_eq!(trim_zeros("de", fmt_ui("de", 1.5, 3), 2), "1,50");
    }

    #[test]
    fn typing_0_comma_2_into_a_converted_box_stores_0_point_2() {
        assert_eq!(parse_box("0,2"), Ok(0.2));
        assert_eq!(parse_box("0.2"), Ok(0.2));
        assert_eq!(parse_box(",2"), Ok(0.2));
    }

    #[test]
    fn a_bad_entry_keeps_the_old_value_and_shows_the_plain_message() {
        // egui keeps a DragValue's value when its parser gives None.
        let value = 0.25_f64;
        let typed = parse_box("1,2,3").ok().unwrap_or(value);
        assert_eq!(typed, 0.25, "a refused entry leaves the number alone");
        assert_eq!(
            parse_box("abc").err().as_deref(),
            Some("Type a number such as 0,2")
        );
        assert_eq!(trim_zeros("de", fmt_ui("de", typed, 3), 0), "0,25");
    }

    #[test]
    fn every_numeric_box_in_lib_rs_uses_the_localized_helper() {
        // Integer counters stay plain; every other box goes through the helper.
        const COUNTERS: [&str; 2] = ["&mut self.panel_columns", "&mut self.panel_rows"];
        let source = include_str!("lib.rs");
        let production = source.split("\n#[cfg(test)]").next().unwrap_or(source);
        let mut plain = Vec::new();
        for (at, _) in production.match_indices("egui::DragValue::new(") {
            let before = production[..at].trim_end();
            let rest = &production[at + "egui::DragValue::new(".len()..];
            let arg = rest.split(')').next().unwrap_or("");
            if !before.ends_with("localized_active(") && !COUNTERS.contains(&arg) {
                plain.push(arg.to_string());
            }
        }
        assert!(plain.is_empty(), "boxes not localized: {plain:?}");
    }
}
