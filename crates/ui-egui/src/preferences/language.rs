//! The Language and units tab: the text language, default units, grids and number format.
//!
//! Context: mockup `prefs-language-units`. Two cards: Language, Units and grid.
//! Local rules: the language applies at once through `App::set_language`. Units, grids and
//! the number format are kept in `Preferences` for new projects; each project keeps its own.
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/card.rs, form_row.rs, segmented.rs, select.rs, number_field.rs
//! - crates/text/src/lib.rs (`shipped_languages`)
//!
//! # Do not
//! - Put a unit in a label; the unit sits inside the field.
use super::note;
use crate::ui_kit::card::card;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::number_field::{NumberSpec, number_field};
use crate::ui_kit::segmented::{Segment, SegmentedStyle, segmented_styled};
use crate::ui_kit::select::select;
use crate::{App, theme};
use pcbcraft_text::t;

/// Default unit ids, in the order of the segmented control.
pub(crate) const UNITS: [&str; 3] = ["mm", "mil", "inch"];

impl App {
    /// The Language and units tab.
    ///
    /// # Rules
    /// - The language list is `System default` then every shipped language by its own name.
    ///
    /// # Dependencies
    /// - crates/text/src/lib.rs
    ///
    /// # Do not
    /// - Translate a language's name; each is shown in itself.
    pub(super) fn prefs_language(&mut self, ui: &mut egui::Ui) {
        self.prefs_language_card(ui);
        ui.add_space(theme::SPACE[3]);
        self.prefs_units_card(ui);
    }

    fn prefs_language_card(&mut self, ui: &mut egui::Ui) {
        let languages = pcbcraft_text::shipped_languages();
        let system = t!("menu.settings.language.system");
        let mut labels: Vec<String> = vec![system];
        labels.extend(languages.iter().map(|l| l.name.clone()));
        let options: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut at = languages
            .iter()
            .position(|l| l.tag == self.preferences.language)
            .map_or(0, |i| i + 1);
        let before = at;
        let names = languages
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        card(
            ui,
            &t!("prefs.language"),
            None,
            |_| {},
            |ui| {
                let name = t!("prefs.language");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.language.hint")),
                    None,
                    |ui| {
                        select(
                            ui,
                            "prefs-language",
                            &mut at,
                            &options,
                            &name,
                            &t!("prefs.language.tip"),
                            220.0,
                        );
                    },
                );
                note(ui, t!("prefs.language.note", names = names));
            },
        );
        if at != before {
            let tag = match at.checked_sub(1).and_then(|i| languages.get(i)) {
                Some(language) => language.tag.clone(),
                None => pcbcraft_text::SYSTEM.to_owned(),
            };
            let ctx = ui.ctx().clone();
            self.set_language(&ctx, &tag);
            ctx.request_repaint();
        }
    }

    fn prefs_units_card(&mut self, ui: &mut egui::Ui) {
        card(
            ui,
            &t!("prefs.units"),
            None,
            |_| {},
            |ui| {
                let mut at = UNITS
                    .iter()
                    .position(|u| *u == self.preferences.default_units)
                    .unwrap_or(0);
                let before = at;
                let (mm, mil, inch) = (
                    t!("prefs.units.mm"),
                    t!("prefs.units.mil"),
                    t!("prefs.units.inch"),
                );
                let (mm_tip, mil_tip, inch_tip) = (
                    t!("prefs.units.mm.tip"),
                    t!("prefs.units.mil.tip"),
                    t!("prefs.units.inch.tip"),
                );
                let name = t!("prefs.units.default");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.units.default.hint")),
                    None,
                    |ui| {
                        segmented_styled(
                            ui,
                            "prefs-units",
                            &mut at,
                            &[
                                Segment::text(&mm, &mm_tip),
                                Segment::text(&mil, &mil_tip),
                                Segment::text(&inch, &inch_tip),
                            ],
                            SegmentedStyle {
                                full_width: false,
                                group_name: Some(&name),
                            },
                        );
                    },
                );
                if at != before
                    && let Some(unit) = UNITS.get(at)
                {
                    self.preferences.default_units = (*unit).to_owned();
                }
                let spec = NumberSpec {
                    unit: "mm",
                    step: 0.01,
                    min: 0.01,
                    max: 25.4,
                    places: 2,
                    width: 120.0,
                };
                let mut grid = f64::from(self.preferences.default_grid_mm);
                let name = t!("prefs.grid.default");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.grid.default.hint")),
                    None,
                    |ui| {
                        number_field(
                            ui,
                            "prefs-default-grid",
                            &mut grid,
                            &spec,
                            &name,
                            &t!("prefs.grid.default.tip"),
                        );
                    },
                );
                self.preferences.default_grid_mm = grid as f32;
                let mut board = f64::from(self.preferences.board_grid_mm);
                let name = t!("prefs.grid.board");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.grid.board.hint")),
                    None,
                    |ui| {
                        number_field(
                            ui,
                            "prefs-board-grid",
                            &mut board,
                            &spec,
                            &name,
                            &t!("prefs.grid.board.tip"),
                        );
                    },
                );
                self.preferences.board_grid_mm = board as f32;
                let mut format = usize::from(self.preferences.number_format).min(2);
                let before = format;
                let (comma, dot, space) = (
                    t!("prefs.format.comma"),
                    t!("prefs.format.dot"),
                    t!("prefs.format.space"),
                );
                let (comma_tip, dot_tip, space_tip) = (
                    t!("prefs.format.comma.tip"),
                    t!("prefs.format.dot.tip"),
                    t!("prefs.format.space.tip"),
                );
                let name = t!("prefs.format");
                form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                    segmented_styled(
                        ui,
                        "prefs-number-format",
                        &mut format,
                        &[
                            Segment::text(&comma, &comma_tip),
                            Segment::text(&dot, &dot_tip),
                            Segment::text(&space, &space_tip),
                        ],
                        SegmentedStyle {
                            full_width: false,
                            group_name: Some(&name),
                        },
                    );
                });
                if format != before {
                    self.preferences.number_format = format as u8;
                }
            },
        );
    }
}
