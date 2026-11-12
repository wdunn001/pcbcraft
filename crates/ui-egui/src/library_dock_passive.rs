//! The resistor and capacitor generator in the Library dock (issue #757, part of #696).
//!
//! Context: `generate_passive` builds the footprint, symbol and part number of a chip resistor or
//! capacitor from a value, and `values_between` lists the values of an E series. Nothing in the
//! window used them. This section shows one card for each of Resistor and Capacitor under the
//! connector series. A card opens a form: the series (E6, E12, E24, E96), the chip size, a range
//! of values (one decade) and a value from that series inside the range. The form says the part
//! number and value it makes, and Place hands the symbol and footprint to the engine's
//! `part.place`, the same path the connector series use, so the part, its symbol and its
//! footprint join the project in one undo step.
//!
//! # Rules
//! - Only values from the chosen series can be picked, so a value outside the series cannot be
//!   asked for. Sizes are the four the generator knows.
//! - The tolerance follows the series (E6 20, E12 10, E24 5, E96 1 percent).
//! - This module uses no std-only API, so the browser build reaches it.
//!
//! # Dependencies
//! - crates/library/src/passive_series.rs (`ESeries`, `PassiveKind`, `values_between`)
//! - crates/library/src/passive_generator.rs (`generate_passive`)
//! - crates/engine/src/authoring.rs (`part.place`)
//! - crates/ui-egui/src/library_dock.rs (`kind_allows`, `dock_sheet`, `with_sheet`)
//!
//! # Do not
//! - Offer a value the series does not hold or a size the generator does not know.
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::rows::list_row_high;
use crate::{App, Tab, fonts, t};
use pcbcraft_library::passive_series::{ESeries, PassiveKind, values_between};
use pcbcraft_library::{GeneratedPassive, generate_passive};
use serde_json::json;

/// The chip sizes the generator knows.
pub(crate) const PACKAGES: [&str; 4] = ["0402", "0603", "0805", "1206"];

/// The series the form offers, with the name shown on its chip.
const SERIES: [(ESeries, &str); 4] = [
    (ESeries::E6, "E6"),
    (ESeries::E12, "E12"),
    (ESeries::E24, "E24"),
    (ESeries::E96, "E96"),
];

/// Height of one card row.
const ROW: f32 = 52.0;

/// What the person has chosen for the generator that is open.
pub(crate) struct PassiveState {
    /// The card that is open.
    open: Option<PassiveKind>,
    series: ESeries,
    package: &'static str,
    /// The decade of values on offer: values from 10^exp up to just below 10^(exp+1).
    exp: i32,
    /// The value chosen, always one the series lists in the decade.
    value: f64,
}

impl Default for PassiveState {
    fn default() -> Self {
        Self {
            open: None,
            series: ESeries::E12,
            package: "0603",
            exp: 4,
            value: 1e4,
        }
    }
}

/// The decades a kind offers, as powers of ten.
fn decades(kind: PassiveKind) -> std::ops::RangeInclusive<i32> {
    match kind {
        PassiveKind::Resistor => 0..=6,
        PassiveKind::Capacitor => -12..=-5,
    }
}

/// The values of a series from one decade, ascending.
pub(crate) fn decade_values(series: ESeries, exp: i32) -> Vec<f64> {
    let low = 10f64.powi(exp);
    values_between(series, low, low * 9.999).unwrap_or_default()
}

/// The listed value closest to `target` on a log scale, or the target when the list is empty.
pub(crate) fn nearest(list: &[f64], target: f64) -> f64 {
    list.iter()
        .copied()
        .min_by(|a, b| {
            let da = (a / target).ln().abs();
            let db = (b / target).ln().abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(target)
}

/// The tolerance in percent that goes with a series.
pub(crate) fn tolerance_for(series: ESeries) -> u8 {
    match series {
        ESeries::E6 => 20,
        ESeries::E12 => 10,
        ESeries::E24 => 5,
        ESeries::E96 => 1,
    }
}

/// A value as a person reads it, for example `4.7 kohm` or `100 nF`.
pub(crate) fn value_text(kind: PassiveKind, value: f64) -> String {
    let (scale, unit) = match kind {
        PassiveKind::Resistor => {
            if value >= 1e6 {
                (1e6, "Mohm")
            } else if value >= 1e3 {
                (1e3, "kohm")
            } else {
                (1.0, "ohm")
            }
        }
        PassiveKind::Capacitor => {
            if value >= 1e-6 {
                (1e-6, "uF")
            } else if value >= 1e-9 {
                (1e-9, "nF")
            } else {
                (1e-12, "pF")
            }
        }
    };
    let number = (value / scale * 100.0).round() / 100.0;
    t!(
        "library.dock.passive.value",
        n = number.to_string(),
        unit = unit
    )
}

/// The cards a search keeps: both for an empty search, those whose name the words match.
pub(crate) fn passive_cards(query: &str) -> Vec<PassiveKind> {
    let q = query.trim().to_lowercase();
    let mut cards = Vec::new();
    for (kind, word) in [
        (PassiveKind::Resistor, "resistor"),
        (PassiveKind::Capacitor, "capacitor"),
    ] {
        if q.is_empty() || word.contains(&q) || "passive generator".contains(&q) {
            cards.push(kind);
        }
    }
    cards
}

impl PassiveState {
    #[cfg(test)]
    pub(crate) fn open_for_test(&self) -> Option<PassiveKind> {
        self.open
    }

    #[cfg(test)]
    pub(crate) fn value_for_test(&self) -> f64 {
        self.value
    }

    /// Open a card on a common choice: E12, 0603, 10 kohm or 100 nF.
    pub(crate) fn start(&mut self, kind: PassiveKind) {
        self.open = Some(kind);
        self.series = ESeries::E12;
        self.package = "0603";
        let (exp, target) = match kind {
            PassiveKind::Resistor => (4, 1e4),
            PassiveKind::Capacitor => (-7, 1e-7),
        };
        self.exp = exp;
        self.value = nearest(&decade_values(self.series, exp), target);
    }

    /// Change the series and keep the closest value it holds in the decade.
    pub(crate) fn set_series(&mut self, series: ESeries) {
        self.series = series;
        self.value = nearest(&decade_values(series, self.exp), self.value);
    }

    /// Change the decade and start on its first value.
    pub(crate) fn set_decade(&mut self, exp: i32) {
        self.exp = exp;
        if let Some(first) = decade_values(self.series, exp).first() {
            self.value = *first;
        }
    }

    /// The part the choices make.
    pub(crate) fn make(&self, kind: PassiveKind) -> Result<GeneratedPassive, String> {
        generate_passive(kind, self.value, self.package, tolerance_for(self.series))
    }
}

impl App {
    /// The cards the dock shows now, none when the section is not wanted.
    pub(crate) fn dock_passive_wanted(&self, query: &str) -> Vec<PassiveKind> {
        if self.dock.tag.is_some() {
            return Vec::new();
        }
        passive_cards(query)
            .into_iter()
            .filter(|kind| match kind {
                PassiveKind::Resistor => self.kind_allows("resistor"),
                PassiveKind::Capacitor => self.kind_allows("capacitor"),
            })
            .collect()
    }

    /// Draw the heading and one card for each of Resistor and Capacitor.
    pub(crate) fn dock_passive_section(&mut self, ui: &mut egui::Ui, query: &str) {
        let cards = self.dock_passive_wanted(query);
        if cards.is_empty() {
            return;
        }
        let muted = self.muted();
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(t!("library.dock.passive.heading"))
                .strong()
                .font(fonts::Role::Caption.font()),
        );
        ui.label(fonts::caption(t!("library.dock.passive.intro")).color(muted));
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        for kind in cards {
            let (name, tip, summary) = match kind {
                PassiveKind::Resistor => (
                    t!("library.dock.passive.resistor.row"),
                    t!("library.dock.passive.resistor.row.tip"),
                    t!("library.dock.passive.resistor.summary"),
                ),
                PassiveKind::Capacitor => (
                    t!("library.dock.passive.capacitor.row"),
                    t!("library.dock.passive.capacitor.row.tip"),
                    t!("library.dock.passive.capacitor.summary"),
                ),
            };
            let open = self.dock.passive.open == Some(kind);
            let response = list_row_high(ui, ROW, open, &name, &tip, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add(
                        egui::Label::new(egui::RichText::new(&name).strong())
                            .truncate()
                            .selectable(false),
                    );
                    ui.add(
                        egui::Label::new(fonts::caption(summary).color(muted))
                            .truncate()
                            .selectable(false),
                    );
                });
            });
            if response.clicked() {
                if open {
                    self.dock.passive.open = None;
                } else {
                    self.dock.passive.start(kind);
                }
            }
            if open {
                self.dock_passive_panel(ui, kind);
            }
        }
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.add_space(6.0);
    }

    /// The choices for the open card, the part they make and the Place button.
    fn dock_passive_panel(&mut self, ui: &mut egui::Ui, kind: PassiveKind) {
        let muted = self.muted();
        let chip = |ui: &mut egui::Ui, text: &str, tip: &str, on: bool| {
            ui.add(
                egui::Button::new(text)
                    .selected(on)
                    .min_size(egui::Vec2::splat(crate::targets::MIN_TARGET)),
            )
            .on_hover_text(tip)
            .clicked()
        };
        ui.add_space(4.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let state = &mut self.dock.passive;
            ui.horizontal_wrapped(|ui| {
                ui.label(fonts::caption(t!("library.dock.passive.series")).color(muted));
                for (series, name) in SERIES {
                    let tip = t!(
                        "library.dock.passive.series.tip",
                        series = name,
                        count = pcbcraft_library::passive_series::decade(series).len(),
                        tol = tolerance_for(series)
                    );
                    if chip(ui, name, &tip, state.series == series) {
                        state.set_series(series);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(fonts::caption(t!("library.dock.passive.package")).color(muted));
                for code in PACKAGES {
                    let tip = t!("library.dock.passive.package.tip", code = code);
                    if chip(ui, code, &tip, state.package == code) {
                        state.package = code;
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(fonts::caption(t!("library.dock.passive.range")).color(muted));
                for exp in decades(kind) {
                    let low = 10f64.powi(exp);
                    let text = value_text(kind, low);
                    let tip = t!(
                        "library.dock.passive.range.tip",
                        low = text,
                        high = value_text(kind, low * 10.0)
                    );
                    if chip(ui, &text, &tip, state.exp == exp) {
                        state.set_decade(exp);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(fonts::caption(t!("library.dock.passive.value_label")).color(muted));
                for v in decade_values(state.series, state.exp) {
                    let text = value_text(kind, v);
                    let tip = t!("library.dock.passive.value.tip", value = text);
                    let on = (state.value / v - 1.0).abs() < 1e-9;
                    if chip(ui, &text, &tip, on) {
                        state.value = v;
                    }
                }
            });
        });
        let made = self.dock.passive.make(kind);
        match &made {
            Ok(part) => {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(t!(
                            "library.dock.passive.makes",
                            number = part.mpn,
                            value = value_text(kind, part.value)
                        ))
                        .strong(),
                    )
                    .truncate(),
                );
            }
            Err(said) => {
                ui.label(fonts::caption(said.clone()).color(crate::theme::tokens(ui.ctx()).red));
            }
        }
        let ready = made.is_ok();
        let target = if self.tab == Tab::Pcb {
            "board"
        } else {
            "schematic"
        };
        let (label, tip) = if target == "board" {
            (
                t!("library.dock.passive.place_board"),
                t!("library.dock.passive.place_board.tip"),
            )
        } else {
            (
                t!("library.dock.passive.place"),
                t!("library.dock.passive.place.tip"),
            )
        };
        if button::primary_button(ui, None, &label, &tip, Size::Normal, ready).clicked()
            && let Ok(part) = made
        {
            self.dock_passive_place(kind, part, target);
        }
        ui.add_space(6.0);
    }

    /// Hand a generated part to `part.place`: it imports the symbol and footprint into the project
    /// library and places the part, all as one undo step.
    fn dock_passive_place(&mut self, kind: PassiveKind, part: GeneratedPassive, target: &str) {
        let at = crate::canvas::snap(if target == "board" {
            self.pcb.center
        } else {
            self.schematic.center
        });
        let sheet = self.dock_sheet(target);
        let params = crate::library_dock::with_sheet(
            json!({
                "target": target,
                "position": at,
                "symbol": part.symbol,
                "footprint": part.footprint,
                "notice": t!("library.dock.passive.notice", number = part.mpn),
                "value": value_text(kind, part.value),
            }),
            sheet,
        );
        self.run("part.place", params);
        self.dock_after_place(target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resistor_card_opens_on_ten_kilohms_in_0603_and_makes_a_part() -> Result<(), String> {
        let mut state = PassiveState::default();
        state.start(PassiveKind::Resistor);
        assert_eq!(state.open_for_test(), Some(PassiveKind::Resistor));
        assert!((state.value_for_test() - 1e4).abs() < 1e-6);
        let part = state.make(PassiveKind::Resistor)?;
        assert_eq!(part.mpn, "RES-0603-10K-10");
        Ok(())
    }

    #[test]
    fn a_capacitor_card_opens_on_100_nanofarads() -> Result<(), String> {
        let mut state = PassiveState::default();
        state.start(PassiveKind::Capacitor);
        assert!((state.value_for_test() - 1e-7).abs() < 1e-15);
        let part = state.make(PassiveKind::Capacitor)?;
        assert!(part.mpn.starts_with("CAP-0603-100N"), "{}", part.mpn);
        Ok(())
    }

    #[test]
    fn changing_the_series_keeps_a_value_the_new_series_holds() {
        let mut state = PassiveState::default();
        state.start(PassiveKind::Resistor);
        state.set_decade(3);
        state.value = 4700.0;
        state.set_series(ESeries::E96);
        let list = decade_values(ESeries::E96, 3);
        assert!(
            list.iter()
                .any(|v| (state.value / v - 1.0).abs() < 1e-9),
            "the value {} is in the E96 decade",
            state.value
        );
        assert!((state.value - 4640.0).abs() < 1.0 || (state.value - 4750.0).abs() < 1.0);
    }

    #[test]
    fn a_decade_lists_only_values_of_the_series_inside_it() {
        let e12 = decade_values(ESeries::E12, 2);
        assert_eq!(e12.len(), 12);
        assert!(e12.iter().all(|v| (100.0..1000.0).contains(v)));
        assert_eq!(decade_values(ESeries::E96, 0).len(), 96);
        assert_eq!(decade_values(ESeries::E6, -9).len(), 6);
    }

    #[test]
    fn every_decade_of_both_kinds_makes_parts_for_every_size_and_series() -> Result<(), String> {
        for kind in [PassiveKind::Resistor, PassiveKind::Capacitor] {
            let mut state = PassiveState::default();
            state.start(kind);
            for exp in decades(kind) {
                for (series, _) in SERIES {
                    state.set_series(series);
                    state.set_decade(exp);
                    for package in PACKAGES {
                        state.package = package;
                        state
                            .make(kind)
                            .map_err(|e| format!("{kind:?} 1e{exp} {package}: {e}"))?;
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn values_read_with_their_unit() {
        assert_eq!(value_text(PassiveKind::Resistor, 4700.0), "4.7 kohm");
        assert_eq!(value_text(PassiveKind::Resistor, 10.0), "10 ohm");
        assert_eq!(value_text(PassiveKind::Resistor, 1e6), "1 Mohm");
        assert_eq!(value_text(PassiveKind::Capacitor, 1e-7), "100 nF");
        assert_eq!(value_text(PassiveKind::Capacitor, 2.2e-12), "2.2 pF");
        assert_eq!(value_text(PassiveKind::Capacitor, 1e-5), "10 uF");
    }

    #[test]
    fn a_word_keeps_only_the_card_it_matches() {
        assert_eq!(passive_cards("").len(), 2);
        assert_eq!(passive_cards("resis"), vec![PassiveKind::Resistor]);
        assert_eq!(passive_cards("Capacitor"), vec![PassiveKind::Capacitor]);
        assert!(passive_cards("nothing like this").is_empty());
    }

    #[test]
    fn a_tolerance_follows_the_series() {
        assert_eq!(tolerance_for(ESeries::E96), 1);
        assert_eq!(tolerance_for(ESeries::E6), 20);
    }
}
