//! The Keys tab: the shortcut table with search, area filter and rebinding, the profiles
//! card and the tool modifiers card.
//!
//! Context: mockup `prefs-keys`. The table lists every rebindable action with its area and
//! its key. Change waits for the next key press; a key already in use is refused with the
//! name of the action that holds it, until that action is cleared or changed.
//! Local rules: the key capture itself runs in `App::handle_shortcuts` (`lib.rs`), which
//! reads `App::binding_capture`; this file only starts and shows it.
//!
//! # Dependencies
//! - crates/ui-egui/src/keymap.rs (`rebindable_actions`, `Keymap`)
//! - crates/ui-egui/src/ui_kit/table.rs, segmented.rs, card.rs
//! - crates/ui-egui/src/modifier_ui.rs (`keys_card`)
//!
//! # Do not
//! - Bind a key here; capture and the conflict check are in the keymap.
use super::note;
use crate::keymap::{self, KeyAction};
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::card::card;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::icons;
use crate::ui_kit::segmented::{Segment, SegmentedStyle, segmented_styled};
use crate::ui_kit::select::select;
use crate::ui_kit::table::{self, Column, key_cap};
use crate::{App, a11y::Named, fonts, modifier_ui, theme};
use pcbcraft_text::t;

/// Which area of the app a shortcut belongs to, for the filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Area {
    Tool,
    Layers,
    View,
    General,
}

fn area_of(action: KeyAction) -> Area {
    match action {
        KeyAction::WireTool
        | KeyAction::RouteTool
        | KeyAction::MoveTool
        | KeyAction::ViaTool
        | KeyAction::ReturnToSelect
        | KeyAction::SymbolPalette => Area::Tool,
        KeyAction::LayerNext | KeyAction::LayerPrev => Area::Layers,
        KeyAction::ZoomFitDesign
        | KeyAction::ZoomToSelection
        | KeyAction::MinimapPeek
        | KeyAction::ToggleNetHighlight
        | KeyAction::OpenInspector => Area::View,
        _ => Area::General,
    }
}

fn area_name(area: Area) -> String {
    match area {
        Area::Tool => t!("prefs.keys.area.tool"),
        Area::Layers => t!("prefs.keys.area.layers"),
        Area::View => t!("prefs.keys.area.view"),
        Area::General => t!("prefs.keys.area.general"),
    }
}

/// The filter choices in order: everything, then one area each.
const FILTER: [Option<Area>; 4] = [None, Some(Area::Tool), Some(Area::Layers), Some(Area::View)];

impl App {
    /// The Keys tab.
    ///
    /// # Rules
    /// - A row shows when its label, area or key text holds the search words.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/keymap.rs
    ///
    /// # Do not
    /// - Start a capture for a non-rebindable action.
    pub(super) fn prefs_keys(&mut self, ui: &mut egui::Ui) {
        self.prefs_shortcuts_card(ui);
        ui.add_space(theme::SPACE[3]);
        self.prefs_profiles_card(ui);
        ui.add_space(theme::SPACE[3]);
        card(
            ui,
            &t!("prefs.keys.modifiers"),
            None,
            |_| {},
            |ui| modifier_ui::keys_card(self, ui),
        );
    }

    fn prefs_shortcuts_card(&mut self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        card(
            ui,
            &t!("prefs.keys.shortcuts"),
            None,
            |_| {},
            |ui| {
                ui.horizontal(|ui| {
                    let search_name = t!("prefs.keys.search");
                    ui.add(
                        crate::targets::text_field(&mut self.prefs_ui.keys_search)
                            .id_salt("prefs-keys-search")
                            .hint_text(search_name.clone())
                            .desired_width(240.0),
                    )
                    .named(&search_name)
                    .on_hover_text(t!("prefs.keys.search.tip"));
                    let labels = [
                        t!("prefs.keys.filter.all"),
                        t!("prefs.keys.area.tool"),
                        t!("prefs.keys.area.layers"),
                        t!("prefs.keys.area.view"),
                    ];
                    let tips = [
                        t!("prefs.keys.filter.all.tip"),
                        t!("prefs.keys.filter.tool.tip"),
                        t!("prefs.keys.filter.layers.tip"),
                        t!("prefs.keys.filter.view.tip"),
                    ];
                    let items: Vec<Segment<'_>> = labels
                        .iter()
                        .zip(tips.iter())
                        .map(|(l, tip)| Segment::text(l, tip))
                        .collect();
                    let group = t!("prefs.keys.filter");
                    segmented_styled(
                        ui,
                        "prefs-keys-area",
                        &mut self.prefs_ui.keys_area,
                        &items,
                        SegmentedStyle {
                            full_width: false,
                            group_name: Some(&group),
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if button::secondary_button(
                            ui,
                            Some(icons::REFRESH),
                            &t!("prefs.keys.reset_all"),
                            &t!("prefs.keys.reset_all.tip"),
                            Size::Normal,
                            true,
                        )
                        .clicked()
                        {
                            self.preferences.active_keymap_mut().reset_to_default();
                            self.binding_capture = None;
                            self.keymap_notice = t!("prefs.keys.reset_done");
                        }
                    });
                });
                ui.add_space(theme::SPACE[1]);
                self.prefs_shortcut_rows(ui);
                if !self.keymap_notice.is_empty() {
                    ui.add_space(theme::SPACE[1]);
                    let notice = self.keymap_notice.clone();
                    ui.label(fonts::text(fonts::Role::Body, notice).color(t.copper));
                }
                ui.add_space(theme::SPACE[1]);
                note(ui, t!("prefs.keys.help"));
            },
        );
    }

    fn prefs_shortcut_rows(&mut self, ui: &mut egui::Ui) {
        let heads = (
            t!("prefs.keys.col.action"),
            t!("prefs.keys.col.area"),
            t!("prefs.keys.col.key"),
        );
        let cols = [
            Column {
                title: &heads.0,
                width: 460.0,
            },
            Column {
                title: &heads.1,
                width: 140.0,
            },
            Column {
                title: &heads.2,
                width: 160.0,
            },
            Column {
                title: "",
                width: 0.0,
            },
        ];
        table::head(ui, &cols);
        let t = theme::tokens(ui.ctx());
        let words = self.prefs_ui.keys_search.trim().to_lowercase();
        let filter = FILTER.get(self.prefs_ui.keys_area).copied().flatten();
        let mut shown = 0;
        for action in keymap::rebindable_actions() {
            let area = area_of(action);
            if filter.is_some_and(|f| f != area) {
                continue;
            }
            let label = keymap::action_label(action);
            let chord = self
                .preferences
                .active_keymap()
                .chord_for(action)
                .map_or_else(|| t!("prefs.keys.unassigned"), keymap::format_chord);
            let hay = format!("{label} {} {chord}", area_name(area)).to_lowercase();
            if !words.split_whitespace().all(|w| hay.contains(w)) {
                continue;
            }
            shown += 1;
            let waiting = self.binding_capture == Some(action);
            table::row(ui, &cols, |cells| {
                cells.cell(|ui| {
                    ui.add(
                        egui::Label::new(fonts::text(fonts::Role::Body, label).color(t.text))
                            .truncate(),
                    )
                    .on_hover_text(keymap::action_tooltip(action));
                });
                cells.cell(|ui| {
                    ui.label(fonts::text(fonts::Role::Body, area_name(area)).color(t.muted));
                });
                cells.cell(|ui| {
                    if waiting {
                        ui.label(
                            fonts::text(fonts::Role::Body, t!("prefs.keys.press")).color(t.copper),
                        );
                    } else {
                        key_cap(ui, &chord, &t!("prefs.keys.cap.tip", action = label));
                    }
                });
                cells.cell(|ui| {
                    let tip = t!("prefs.keys.change.tip", action = label);
                    if button::secondary_button(
                        ui,
                        None,
                        &t!("prefs.keys.change"),
                        &tip,
                        Size::Small,
                        true,
                    )
                    .clicked()
                    {
                        self.binding_capture = Some(action);
                        self.keymap_notice = t!("prefs.keys.press_for", action = label);
                    }
                });
            });
        }
        if shown == 0 {
            ui.add_space(theme::SPACE[1]);
            ui.label(fonts::text(fonts::Role::Body, t!("prefs.keys.none")).color(t.muted));
        }
    }

    /// Profiles: pick, save as, delete, export and import a whole set of shortcuts.
    fn prefs_profiles_card(&mut self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        card(
            ui,
            &t!("prefs.keys.profiles"),
            None,
            |_| {},
            |ui| {
                let names: Vec<String> = {
                    let mut v: Vec<String> =
                        self.preferences.keymap_profiles.keys().cloned().collect();
                    v.sort();
                    v
                };
                let options: Vec<&str> = names.iter().map(String::as_str).collect();
                let mut at = names
                    .iter()
                    .position(|n| *n == self.preferences.active_keymap_profile)
                    .unwrap_or(0);
                let before = at;
                let name = t!("prefs.keys.profile");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.keys.profile.hint")),
                    None,
                    |ui| {
                        select(
                            ui,
                            "prefs-keys-profile",
                            &mut at,
                            &options,
                            &name,
                            &t!("prefs.keys.profile.tip"),
                            220.0,
                        );
                    },
                );
                if at != before
                    && let Some(picked) = names.get(at)
                {
                    self.preferences.active_keymap_profile = picked.clone();
                }
                let name = t!("prefs.keys.save_as");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.keys.save_as.hint")),
                    None,
                    |ui| {
                        ui.add(
                            crate::targets::text_field(&mut self.profile_name_draft)
                                .id_salt("prefs-keys-profile-name")
                                .desired_width(220.0),
                        )
                        .named(&name)
                        .on_hover_text(t!("prefs.keys.save_as.tip"));
                        if button::secondary_button(
                            ui,
                            None,
                            &t!("prefs.keys.save"),
                            &t!("prefs.keys.save.tip"),
                            Size::Normal,
                            true,
                        )
                        .clicked()
                        {
                            self.save_keymap_profile();
                        }
                    },
                );
                ui.add_space(theme::SPACE[1]);
                ui.horizontal(|ui| {
                    if button::secondary_button(
                        ui,
                        None,
                        &t!("prefs.keys.delete"),
                        &t!("prefs.keys.delete.tip"),
                        Size::Normal,
                        self.preferences.active_keymap_profile != "Default",
                    )
                    .clicked()
                    {
                        let gone = self.preferences.active_keymap_profile.clone();
                        self.preferences.keymap_profiles.remove(&gone);
                        self.preferences.active_keymap_profile = "Default".into();
                        self.keymap_notice = t!("prefs.keys.deleted", name = gone);
                    }
                    if button::secondary_button(
                        ui,
                        Some(icons::UPLOAD),
                        &t!("prefs.keys.export"),
                        &t!("prefs.keys.export.tip"),
                        Size::Normal,
                        true,
                    )
                    .clicked()
                    {
                        self.export_keymap_profile();
                    }
                    if button::secondary_button(
                        ui,
                        Some(icons::DOWNLOAD),
                        &t!("prefs.keys.import"),
                        &t!("prefs.keys.import.tip"),
                        Size::Normal,
                        !self.profile_import_text.trim().is_empty(),
                    )
                    .clicked()
                    {
                        self.import_keymap_profile();
                    }
                });
                ui.add_space(theme::SPACE[1]);
                let name = t!("prefs.keys.import_box");
                ui.add(
                    egui::TextEdit::multiline(&mut self.profile_import_text)
                        .id_salt("prefs-keys-import")
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .font(fonts::Role::Mono.font())
                        .hint_text(name.clone()),
                )
                .named(&name)
                .on_hover_text(t!("prefs.keys.import_box.tip"));
                ui.label(fonts::caption(t!("prefs.keys.profiles.note")).color(t.muted));
            },
        );
    }

    fn save_keymap_profile(&mut self) {
        let name = self.profile_name_draft.trim().to_owned();
        if name.is_empty() {
            self.keymap_notice = t!("prefs.keys.name_first");
        } else if name == "Default" {
            self.keymap_notice = t!("prefs.keys.default_kept");
        } else {
            let map = self.preferences.active_keymap().clone();
            self.preferences.keymap_profiles.insert(name.clone(), map);
            self.preferences.active_keymap_profile = name.clone();
            self.keymap_notice = t!("prefs.keys.saved", name = name);
            self.profile_name_draft.clear();
        }
    }

    fn export_keymap_profile(&mut self) {
        let file = keymap::KeymapProfileFile::from_keymap(
            self.preferences.active_keymap_profile.clone(),
            self.preferences.active_keymap(),
        );
        match serde_json::to_string_pretty(&file) {
            Ok(text) => {
                self.profile_import_text = text;
                self.keymap_notice = t!("prefs.keys.exported");
            }
            Err(e) => self.keymap_notice = e.to_string(),
        }
    }

    fn import_keymap_profile(&mut self) {
        match serde_json::from_str::<keymap::KeymapProfileFile>(&self.profile_import_text) {
            Ok(file) => {
                let name = file.name.clone();
                match file.into_keymap() {
                    Ok(map) => {
                        self.preferences.keymap_profiles.insert(name.clone(), map);
                        self.preferences.active_keymap_profile = name.clone();
                        self.keymap_notice = t!("prefs.keys.imported", name = name);
                    }
                    Err(e) => self.keymap_notice = e,
                }
            }
            Err(e) => self.keymap_notice = e.to_string(),
        }
    }
}
