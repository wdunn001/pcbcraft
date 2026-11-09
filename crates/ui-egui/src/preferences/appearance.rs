//! The Appearance tab: theme, accent, interface size, motion, grid, pad labels and the map.
//!
//! Context: mockup `prefs-appearance`. Three cards: Look, Board and sheet view, Minimap.
//! Local rules: a change is written to `Preferences` the frame it is made, so the colours
//! and sizes of the whole app follow at once. Accent swatches store the dark-theme colour of
//! the choice; the palette nudges it to read on the chosen theme.
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/card.rs, form_row.rs, segmented.rs, swatch.rs, number_field.rs
//! - crates/ui-egui/src/minimap.rs (`MapPrefs`)
//! - crates/ui-egui/src/a11y.rs (`clamp_ui_scale`)
//!
//! # Do not
//! - Offer corner style or routing options; they belong to the project, a layer and the Route tool.
use super::note;
use crate::minimap::{MIN_OPACITY, MIN_WIDTH, MapPrefs};
use crate::theme::Tokens;
use crate::ui_kit::card::card;
use crate::ui_kit::checkbox::checkbox;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::number_field::{NumberSpec, number_field};
use crate::ui_kit::segmented::{Segment, SegmentedStyle, segmented_styled};
use crate::ui_kit::swatch::{Swatch, swatches};
use crate::{App, PadLabels, a11y, theme};
use egui::Color32;
use pcbcraft_text::t;

/// The two tabs that keep a map look.
const MAP_TABS: [&str; 2] = ["layout", "schematic"];

/// Upper bound of the map width in points.
const MAP_MAX_WIDTH: f64 = 600.0;

fn rgb(c: Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}

impl App {
    /// The Appearance tab.
    ///
    /// # Rules
    /// - Theme System follows the computer's light or dark setting when it reports one.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/swatch.rs
    ///
    /// # Do not
    /// - Store anything but the preference fields.
    pub(super) fn prefs_appearance(&mut self, ui: &mut egui::Ui) {
        self.prefs_look(ui);
        ui.add_space(theme::SPACE[3]);
        self.prefs_board_view(ui);
        ui.add_space(theme::SPACE[3]);
        self.prefs_minimap(ui);
        ui.add_space(theme::SPACE[1]);
        note(ui, t!("prefs.appearance.not_here"));
    }

    fn prefs_look(&mut self, ui: &mut egui::Ui) {
        card(
            ui,
            &t!("prefs.look"),
            None,
            |_| {},
            |ui| {
                let mut at = if self.preferences.theme_system {
                    2
                } else if self.preferences.dark {
                    0
                } else {
                    1
                };
                let before = at;
                let (dark, light, system) = (
                    t!("prefs.theme.dark"),
                    t!("prefs.theme.light"),
                    t!("prefs.theme.system"),
                );
                let (dark_tip, light_tip, system_tip) = (
                    t!("prefs.theme.dark.tip"),
                    t!("prefs.theme.light.tip"),
                    t!("prefs.theme.system.tip"),
                );
                let name = t!("prefs.theme");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.theme.hint")),
                    None,
                    |ui| {
                        segmented_styled(
                            ui,
                            "prefs-theme",
                            &mut at,
                            &[
                                Segment::text(&dark, &dark_tip),
                                Segment::text(&light, &light_tip),
                                Segment::text(&system, &system_tip),
                            ],
                            SegmentedStyle {
                                full_width: false,
                                group_name: Some(&name),
                            },
                        );
                    },
                );
                if at != before {
                    self.preferences.theme_system = at == 2;
                    if at != 2 {
                        self.preferences.dark = at == 0;
                    }
                }
                self.prefs_accent(ui);
                let mut percent = f64::from(self.preferences.ui_scale) * 100.0;
                let name = t!("prefs.size");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.size.hint")),
                    None,
                    |ui| {
                        number_field(
                            ui,
                            "prefs-ui-scale",
                            &mut percent,
                            &NumberSpec {
                                unit: "%",
                                step: 5.0,
                                min: f64::from(a11y::MIN_UI_SCALE) * 100.0,
                                max: f64::from(a11y::MAX_UI_SCALE) * 100.0,
                                places: 0,
                                width: 104.0,
                            },
                            &name,
                            &t!("prefs.size.tip"),
                        );
                    },
                );
                self.preferences.ui_scale = a11y::clamp_ui_scale((percent / 100.0) as f32);
                let name = t!("prefs.motion");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.motion.hint")),
                    None,
                    |ui| {
                        checkbox(
                            ui,
                            &mut self.preferences.reduced_motion,
                            &name,
                            &t!("prefs.motion.tip"),
                        );
                    },
                );
                let name = t!("prefs.tours");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.tours.hint")),
                    None,
                    |ui| {
                        checkbox(
                            ui,
                            &mut self.preferences.offer_tours,
                            &name,
                            &t!("prefs.tours.tip"),
                        );
                    },
                );
            },
        );
    }

    /// The Accent colour row: five swatches.
    fn prefs_accent(&mut self, ui: &mut egui::Ui) {
        let now = theme::tokens(ui.ctx());
        let canvas = theme::CanvasColors::for_dark(now.is_dark());
        let dark = Tokens::DARK;
        let canonical = [
            rgb(dark.copper),
            rgb(dark.mask),
            rgb(dark.cyan),
            rgb(dark.gold),
            rgb(theme::CanvasColors::DARK.courtyard),
        ];
        let shown = [now.copper, now.mask, now.cyan, now.gold, canvas.courtyard];
        let names = [
            t!("prefs.accent.copper"),
            t!("prefs.accent.green"),
            t!("prefs.accent.blue"),
            t!("prefs.accent.gold"),
            t!("prefs.accent.purple"),
        ];
        let tip = t!("prefs.accent.tip");
        let items: Vec<Swatch<'_>> = shown
            .iter()
            .zip(names.iter())
            .map(|(color, name)| Swatch {
                color: *color,
                name,
                tip: &tip,
            })
            .collect();
        let selected = canonical.iter().position(|c| *c == self.preferences.accent);
        let picked = form_row(
            ui,
            FormWidth::Page,
            &t!("prefs.accent"),
            Some(&t!("prefs.accent.hint")),
            None,
            |ui| swatches(ui, &items, selected),
        );
        if let Some(i) = picked
            && let Some(c) = canonical.get(i)
        {
            self.preferences.accent = *c;
        }
    }

    fn prefs_board_view(&mut self, ui: &mut egui::Ui) {
        card(
            ui,
            &t!("prefs.board_view"),
            None,
            |_| {},
            |ui| {
                let name = t!("prefs.grid");
                form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                    checkbox(ui, &mut self.preferences.grid, &name, &t!("prefs.grid.tip"));
                });
                let mut at = match self.preferences.pad_labels {
                    PadLabels::Number => 0,
                    PadLabels::Net => 1,
                    PadLabels::Off => 2,
                };
                let before = at;
                let (number, net, off) = (
                    t!("prefs.pads.number"),
                    t!("prefs.pads.net"),
                    t!("prefs.pads.off"),
                );
                let (number_tip, net_tip, off_tip) = (
                    t!("prefs.pads.number.tip"),
                    t!("prefs.pads.net.tip"),
                    t!("prefs.pads.off.tip"),
                );
                let name = t!("prefs.pads");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.pads.hint")),
                    None,
                    |ui| {
                        segmented_styled(
                            ui,
                            "prefs-pad-labels",
                            &mut at,
                            &[
                                Segment::text(&number, &number_tip),
                                Segment::text(&net, &net_tip),
                                Segment::text(&off, &off_tip),
                            ],
                            SegmentedStyle {
                                full_width: false,
                                group_name: Some(&name),
                            },
                        );
                    },
                );
                if at != before {
                    self.preferences.pad_labels = match at {
                        1 => PadLabels::Net,
                        2 => PadLabels::Off,
                        _ => PadLabels::Number,
                    };
                }
            },
        );
    }

    fn prefs_minimap(&mut self, ui: &mut egui::Ui) {
        let look = self
            .preferences
            .minimap
            .get(MAP_TABS[0])
            .cloned()
            .unwrap_or_default()
            .clamped();
        let mut next = look.clone();
        card(
            ui,
            &t!("prefs.minimap"),
            None,
            |_| {},
            |ui| {
                let name = t!("prefs.minimap.pin");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.minimap.pin.hint")),
                    None,
                    |ui| {
                        checkbox(ui, &mut next.pinned, &name, &t!("prefs.minimap.pin.tip"));
                    },
                );
                let mut width = f64::from(next.width);
                let name = t!("prefs.minimap.size");
                form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                    number_field(
                        ui,
                        "prefs-minimap-size",
                        &mut width,
                        &NumberSpec {
                            unit: "px",
                            step: 10.0,
                            min: f64::from(MIN_WIDTH),
                            max: MAP_MAX_WIDTH,
                            places: 0,
                            width: 104.0,
                        },
                        &name,
                        &t!("prefs.minimap.size.tip"),
                    );
                });
                next.width = width as f32;
                let mut percent = f64::from(next.opacity) * 100.0;
                let name = t!("prefs.minimap.opacity");
                form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                    number_field(
                        ui,
                        "prefs-minimap-opacity",
                        &mut percent,
                        &NumberSpec {
                            unit: "%",
                            step: 5.0,
                            min: f64::from(MIN_OPACITY) * 100.0,
                            max: 100.0,
                            places: 0,
                            width: 104.0,
                        },
                        &name,
                        &t!("prefs.minimap.opacity.tip"),
                    );
                });
                next.opacity = (percent / 100.0) as f32;
            },
        );
        let next = next.clamped();
        if next != look {
            for key in MAP_TABS {
                let mut each: MapPrefs = self
                    .preferences
                    .minimap
                    .get(key)
                    .cloned()
                    .unwrap_or_default()
                    .clamped();
                each.pinned = next.pinned;
                each.width = next.width;
                each.opacity = next.opacity;
                self.preferences.minimap.insert(key.to_owned(), each);
            }
        }
    }
}
