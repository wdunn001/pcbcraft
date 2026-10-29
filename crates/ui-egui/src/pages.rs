//! Schematic page switcher: the row of tabs above the schematic canvas.
//!
//! Context: the active page id lives on `App` (`active_page`); the document
//! model (`Sheet`, `net_pages`) and the page commands live in
//! `../../doc/src/lib.rs` and `../../engine/src/lib.rs`. This file only draws
//! the row and forwards clicks to those typed commands.
//!
//! Local rules: page 0 is the Main page. It is not in `Project::sheets`, so it
//! can be neither renamed nor removed. The row stays a single function
//! (`page_switcher`) so the Schematic tool strip can host it later.

use super::*;

/// Egui memory key for the page name being typed, if a rename is open.
fn draft_key() -> egui::Id {
    egui::Id::new("schematic-page-rename-draft")
}

impl App {
    /// The page the schematic canvas shows: the chosen page while it still
    /// exists, otherwise the Main page.
    ///
    /// # Rules
    /// A page that was removed falls back to Main instead of an empty canvas.
    ///
    /// # Dependencies
    /// `../../doc/src/lib.rs` (`Project::sheets`).
    ///
    /// # Do not
    /// Store a page id anywhere else; `canvas.rs` painters and pickers read it here.
    pub(crate) fn schematic_page(&self) -> Id {
        let page = self.active_page;
        if page == 0 || self.project.sheets.iter().any(|s| s.id == page) {
            page
        } else {
            0
        }
    }

    /// Draw the page tabs with Add, Rename and Remove, and apply the click.
    ///
    /// When a connection is highlighted, tabs whose page carries it get a dot
    /// (`Project::net_pages`), so the user can walk every page it touches.
    /// Switching pages keeps the highlight.
    ///
    /// # Rules
    /// Every control has a tooltip from the text catalog. Page changes go
    /// through `tool_call` so they are undoable and bounded by the engine.
    ///
    /// # Dependencies
    /// `../../engine/src/lib.rs` (`schematic.new_sheet`, `schematic.rename_sheet`,
    /// `schematic.remove_sheet`).
    ///
    /// # Do not
    /// Call it for the board tab, and do not edit sheets directly.
    pub(crate) fn page_switcher(&mut self, ui: &mut egui::Ui) {
        let current = self.schematic_page();
        let lit: Vec<Id> = self
            .highlight_net
            .map(|net| self.project.net_pages(net))
            .unwrap_or_default();
        let mut pages: Vec<(Id, String)> = vec![(0, t!("schematic.pages.root"))];
        pages.extend(self.project.sheets.iter().map(|s| (s.id, s.name.clone())));
        let mut go = None;
        let mut add = false;
        let mut remove = false;
        let mut open_rename = false;
        let mut apply_name: Option<String> = None;
        let mut cancel = false;
        let mut draft: Option<String> = ui.data(|d| d.get_temp::<String>(draft_key()));
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            for (id, name) in &pages {
                let dot = lit.contains(id);
                let text = if dot {
                    format!("\u{25cf} {name}")
                } else {
                    name.clone()
                };
                let tip = if dot {
                    t!("schematic.pages.tab_lit.tip")
                } else {
                    t!("schematic.pages.tab.tip")
                };
                if ui
                    .selectable_label(*id == current, text)
                    .on_hover_text(tip)
                    .clicked()
                {
                    go = Some(*id);
                }
            }
            ui.add_space(8.0);
            if ui
                .button(t!("schematic.pages.add"))
                .on_hover_text(t!("schematic.pages.add.tip"))
                .clicked()
            {
                add = true;
            }
            if current != 0 {
                if let Some(text) = draft.as_mut() {
                    ui.add(
                        crate::targets::text_field(text)
                            .desired_width(120.0)
                            .hint_text(t!("schematic.pages.name_field")),
                    )
                    .on_hover_text(t!("schematic.pages.name_field.tip"));
                    if ui
                        .button(t!("schematic.pages.use_name"))
                        .on_hover_text(t!("schematic.pages.use_name.tip"))
                        .clicked()
                    {
                        apply_name = Some(text.clone());
                    }
                    if ui
                        .button(t!("schematic.pages.cancel"))
                        .on_hover_text(t!("schematic.pages.cancel.tip"))
                        .clicked()
                    {
                        cancel = true;
                    }
                } else {
                    if ui
                        .button(t!("schematic.pages.rename"))
                        .on_hover_text(t!("schematic.pages.rename.tip"))
                        .clicked()
                    {
                        open_rename = true;
                    }
                    if ui
                        .button(t!("schematic.pages.remove"))
                        .on_hover_text(t!("schematic.pages.remove.tip"))
                        .clicked()
                    {
                        remove = true;
                    }
                }
            }
        });
        ui.add_space(4.0);
        if let Some(id) = go {
            self.active_page = id;
            ui.data_mut(|d| d.remove_temp::<String>(draft_key()));
            return;
        }
        if add {
            let mut number = pages.len();
            while pages
                .iter()
                .any(|(_, n)| *n == t!("schematic.pages.new_name", number = number))
            {
                number += 1;
            }
            let name = t!("schematic.pages.new_name", number = number);
            let before = self.project.sheets.len();
            self.tool_call("schematic.new_sheet", json!({"name": name}));
            if self.project.sheets.len() > before
                && let Some(sheet) = self.project.sheets.last()
            {
                self.active_page = sheet.id;
            }
        }
        if open_rename {
            let name = pages
                .iter()
                .find(|(id, _)| *id == current)
                .map(|(_, n)| n.clone())
                .unwrap_or_default();
            ui.data_mut(|d| d.insert_temp(draft_key(), name));
        } else if let Some(text) = draft {
            ui.data_mut(|d| d.insert_temp(draft_key(), text));
        }
        if let Some(name) = apply_name {
            ui.data_mut(|d| d.remove_temp::<String>(draft_key()));
            self.tool_call(
                "schematic.rename_sheet",
                json!({"id": current, "name": name}),
            );
        }
        if cancel {
            ui.data_mut(|d| d.remove_temp::<String>(draft_key()));
        }
        if remove {
            self.tool_call("schematic.remove_sheet", json!({"id": current}));
            self.active_page = 0;
        }
    }
}
