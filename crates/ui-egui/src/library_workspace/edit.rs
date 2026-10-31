//! The Edit page: the existing symbol and footprint editor, shown inside the Library tab.
//!
//! Context: issues #695 and #124. The editor of definitions lives in `authoring.rs`
//! (`library_editor`) and keeps its draft in `App::library.editor`. This page loads the
//! showing part's symbol or footprint into that draft and calls the same editor, so there is
//! one editor and a fix to it fixes both places.
//!
//! Local rules: a built-in symbol is drawn by the app and has no file to edit, so editing it
//! starts a new symbol with a plain note. Reading an installed symbol runs on a worker and
//! the draft appears when it arrives.
//!
//! # Dependencies
//! - crates/ui-egui/src/authoring.rs (`Draft`, `library_editor`, `new_symbol`, `new_footprint`)
//! - crates/ui-egui/src/library_dock.rs (`spawn_read`, `dock_card_footprint`)
//!
//! The pin map under the editor lists each symbol pin with the footprint pad it reaches (pins and
//! pads meet by number) and lets the person pick another pad for a pin in the draft.
//!
//! # Do not
//! - Copy editor code here.
use super::*;
use crate::a11y::Named;
use crate::authoring::Draft;
use library_dock::{Reply, spawn_read};
use pcbcraft_library::SymbolDefinition;

/// A definition being read for the editor.
pub(crate) struct DraftRead {
    rx: Reply,
}

impl App {
    /// Put the showing part's symbol in the editor.
    pub(crate) fn library_load_symbol_draft(&mut self, ctx: &egui::Context) {
        self.library.editor.notice = None;
        self.library.editor.needs_fit = true;
        let Some(card) = self.library_showing() else {
            self.library.editor.draft = Some(Draft::Symbol(authoring::new_symbol()));
            return;
        };
        match &card {
            Card::Project(d) => self.library.editor.draft = Some(Draft::Symbol((**d).clone())),
            Card::Installed(i) => {
                let (path, name) = (i.path.clone(), i.name.clone());
                let rx = spawn_read(ctx, move || {
                    pcbcraft_engine::library::read_tool(
                        "library.inspect_symbol",
                        &json!({"path": path, "name": name}),
                    )
                });
                self.libws.draft_read = Some(DraftRead { rx });
            }
            Card::Online(p, _) => {
                let def = self
                    .dock_online_symbol(&p.sku)
                    .unwrap_or_else(authoring::new_symbol);
                self.library.editor.draft = Some(Draft::Symbol(def));
            }
            Card::BuiltIn(..) => {
                let mut def = authoring::new_symbol();
                def.name = format!("{}_custom", card.name());
                self.library.editor.draft = Some(Draft::Symbol(def));
                self.library.editor.notice = Some(t!("libws.edit.built_in_note"));
            }
        }
    }

    /// Put the showing part's footprint in the editor.
    pub(crate) fn library_load_footprint_draft(&mut self, ctx: &egui::Context) {
        let _ = ctx;
        self.library.editor.notice = None;
        self.library.editor.needs_fit = true;
        let found = self
            .library_showing()
            .and_then(|card| self.dock_card_footprint(&card));
        match found {
            Some(def) => self.library.editor.draft = Some(Draft::Footprint(def)),
            None => {
                self.library.editor.draft = Some(Draft::Footprint(authoring::new_footprint()));
                self.library.editor.notice = Some(t!("libws.edit.no_footprint_note"));
            }
        }
    }

    /// Take the answer of a running read, if it has arrived.
    pub(crate) fn library_poll_draft(&mut self) {
        let Some(read) = &self.libws.draft_read else {
            return;
        };
        match read.rx.try_recv() {
            Ok(result) => {
                self.libws.draft_read = None;
                match result
                    .ok()
                    .and_then(|v| v.get("definition").cloned())
                    .and_then(|d| serde_json::from_value::<SymbolDefinition>(d).ok())
                {
                    Some(def) => self.library.editor.draft = Some(Draft::Symbol(def)),
                    None => self.message = t!("libws.edit.read_failed"),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.libws.draft_read = None,
        }
    }

    /// The editor, with a short line saying where the definition came from.
    pub(super) fn library_editor_page(&mut self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        if self.libws.draft_read.is_some() {
            ui.label(fonts::text(fonts::Role::Body, t!("libws.edit.reading")).color(t.muted));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        if let Some(note) = self.library.editor.notice.clone() {
            ui.label(fonts::text(fonts::Role::Body, note).color(t.muted));
        }
        self.library_editor(ui);
        self.library_pin_map(ui);
    }

    /// The pin map: each pin of the symbol being edited and the pad it reaches.
    ///
    /// # Rules
    /// - Needs a symbol draft and the part's footprint; otherwise it says what is missing.
    /// - Choosing a pad renumbers that pin in the draft; the symbol is saved with the editor's
    ///   own save button, so one undo step covers it.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/library_workspace/edit.rs (`pin_map_rows`)
    /// - crates/ui-egui/src/authoring.rs (`Draft`)
    fn library_pin_map(&mut self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        let pads: Vec<String> = self
            .library_showing()
            .and_then(|card| self.dock_card_footprint(&card))
            .map(|d| d.pads.iter().map(|p| p.number.clone()).collect())
            .unwrap_or_default();
        let Some(Draft::Symbol(def)) = self.library.editor.draft.as_mut() else {
            return;
        };
        let pins: Vec<(String, String)> = def
            .pins
            .iter()
            .filter(|p| p.unit <= 1)
            .map(|p| (p.number.clone(), p.name.clone()))
            .collect();
        let (rows, unused) = pin_map_rows(&pins, &pads);
        let mapped = rows.iter().filter(|r| r.mapped).count();
        let mut renumber: Option<(usize, String)> = None;
        crate::ui_kit::card::card(
            ui,
            &t!("libws.pinmap.title"),
            Some(&t!("libws.pinmap.caption")),
            |ui| {
                let (text, kind) = if !rows.is_empty() && mapped == rows.len() {
                    (
                        t!("libws.pinmap.all", mapped = mapped, total = rows.len()),
                        crate::ui_kit::chip::BadgeKind::Success,
                    )
                } else {
                    (
                        t!("libws.pinmap.some", mapped = mapped, total = rows.len()),
                        crate::ui_kit::chip::BadgeKind::Warning,
                    )
                };
                crate::ui_kit::chip::badge(ui, &text, kind, &t!("libws.pinmap.badge.tip"));
            },
            |ui| {
                if pads.is_empty() {
                    ui.label(
                        fonts::text(fonts::Role::Body, t!("libws.pinmap.no_footprint"))
                            .color(t.muted),
                    );
                    return;
                }
                egui::Grid::new("library-pin-map")
                    .num_columns(4)
                    .spacing([24.0, 8.0])
                    .show(ui, |ui| {
                        for head in [
                            t!("libws.pinmap.pin"),
                            t!("libws.pinmap.name"),
                            t!("libws.pinmap.pad"),
                            t!("libws.pinmap.state"),
                        ] {
                            ui.label(fonts::section_caption(head).color(t.muted));
                        }
                        ui.end_row();
                        for (i, row) in rows.iter().enumerate() {
                            ui.label(fonts::mono(&row.number).color(t.text));
                            ui.label(fonts::text(fonts::Role::Body, &row.name).color(t.text));
                            egui::ComboBox::from_id_salt(("library-pin-pad", i))
                                .selected_text(t!("libws.pinmap.pad_n", number = row.number))
                                .show_ui(ui, |ui| {
                                    for pad in &pads {
                                        if ui
                                            .selectable_label(
                                                *pad == row.number,
                                                t!("libws.pinmap.pad_n", number = pad),
                                            )
                                            .on_hover_text(t!("libws.pinmap.pick.tip"))
                                            .clicked()
                                        {
                                            renumber = Some((i, pad.clone()));
                                        }
                                    }
                                })
                                .response
                                .named(&t!("libws.pinmap.pad_for", pin = row.number))
                                .on_hover_text(t!("libws.pinmap.pick.tip"));
                            let (text, kind) = if row.mapped {
                                (
                                    t!("libws.pinmap.mapped"),
                                    crate::ui_kit::chip::BadgeKind::Success,
                                )
                            } else {
                                (
                                    t!("libws.pinmap.no_pad"),
                                    crate::ui_kit::chip::BadgeKind::Warning,
                                )
                            };
                            crate::ui_kit::chip::badge(
                                ui,
                                &text,
                                kind,
                                &t!("libws.pinmap.badge.tip"),
                            );
                            ui.end_row();
                        }
                    });
                for pad in &unused {
                    ui.label(
                        fonts::caption(t!("libws.pinmap.unused", number = pad)).color(t.muted),
                    );
                }
            },
        );
        if let Some((i, number)) = renumber
            && let Some(Draft::Symbol(def)) = self.library.editor.draft.as_mut()
            && let Some(pin) = def.pins.iter_mut().filter(|p| p.unit <= 1).nth(i)
        {
            pin.number = number;
        }
    }
}

/// One line of the pin map.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PinRow {
    /// The pin's number.
    pub(crate) number: String,
    /// The pin's name.
    pub(crate) name: String,
    /// True when the footprint has exactly one pad with that number.
    pub(crate) mapped: bool,
}

/// Match each symbol pin to the footprint pad that has its number.
///
/// # Rules
/// - Pins and pads meet by number. A number found on no pad, or on two, is not mapped.
/// - The second result lists the pad numbers no pin reaches, in the footprint's order.
pub(crate) fn pin_map_rows(
    pins: &[(String, String)],
    pads: &[String],
) -> (Vec<PinRow>, Vec<String>) {
    let rows: Vec<PinRow> = pins
        .iter()
        .map(|(number, name)| PinRow {
            number: number.clone(),
            name: name.clone(),
            mapped: pads.iter().filter(|p| *p == number).count() == 1,
        })
        .collect();
    let mut unused = Vec::new();
    for pad in pads {
        if !pins.iter().any(|(n, _)| n == pad) && !unused.contains(pad) {
            unused.push(pad.clone());
        }
    }
    (rows, unused)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pins(list: &[&str]) -> Vec<(String, String)> {
        list.iter()
            .map(|n| ((*n).into(), format!("P{n}")))
            .collect()
    }
    fn pads(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| (*n).into()).collect()
    }

    #[test]
    fn pins_meet_pads_by_number_and_leftover_pads_are_listed() {
        let (rows, unused) = pin_map_rows(&pins(&["1", "2", "3"]), &pads(&["1", "2", "4"]));
        assert_eq!(
            rows.iter().map(|r| r.mapped).collect::<Vec<_>>(),
            [true, true, false]
        );
        assert_eq!(unused, ["4"]);
    }

    #[test]
    fn a_number_on_two_pads_is_not_mapped() {
        let (rows, _) = pin_map_rows(&pins(&["1"]), &pads(&["1", "1"]));
        assert!(
            !rows[0].mapped,
            "two pads share the number, so the pin is ambiguous"
        );
    }
}
