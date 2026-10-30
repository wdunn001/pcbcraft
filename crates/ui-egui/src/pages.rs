//! Schematic pages: the row of tabs above the schematic canvas, the typed
//! page commands every surface shares, and the page card in Properties.
//!
//! Context: the active page id lives on `App` (`active_page`); the document
//! model (`Sheet`, `net_pages`) and the page commands live in
//! `../../doc/src/lib.rs` and `../../engine/src/lib.rs`. The Layers tree
//! (`layers_tree.rs`) and the tab strip both call the methods here, so a page
//! is added, renamed and removed the same way from every place.
//!
//! Local rules: page 0 is the Main page. It is not in `Project::sheets`, so it
//! can be neither renamed nor removed. The row stays a single function
//! (`page_switcher`) so the Schematic tool strip can host it later. The strip
//! has a plus tab instead of Add, Rename and Remove buttons; those actions
//! live in the tab's right-click menu, in Properties, and on F2.

use super::*;

/// What a row of a page tab's right-click menu does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PageAct {
    Rename,
    Add,
    Remove,
}

/// Egui memory key for the page tab being renamed in place: `(page id, text so far)`.
fn rename_key() -> egui::Id {
    egui::Id::new("schematic-page-tab-rename")
}

/// What a finished inline name edit asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NameEdit {
    /// Enter, or focus moved away: keep the new text.
    Commit,
    /// Escape: keep the old name.
    Cancel,
}

/// Draw a one-line name editor in place of a label and report how it ended.
///
/// Why: F2 rename turns a tree row or a page tab into a field where it stands.
/// The tree, the tab strip and any later surface share this one editor so
/// Enter, Escape and click-away behave alike.
///
/// # Rules
/// - Takes focus on its first frame only; `salt` keys that first frame.
/// - Enter and losing focus commit, Escape cancels. The caller applies the
///   result through a typed command and clears its own rename state.
///
/// # Dependencies
/// - `crates/ui-egui/src/targets.rs` (`text_field`, `MIN_TARGET`).
///
/// # Do not
/// - Keep the draft here; the caller owns the text.
pub(crate) fn inline_name_edit(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    width: f32,
    tip: &str,
) -> Option<NameEdit> {
    let id = egui::Id::new(("inline-name-edit", salt));
    let started = egui::Id::new(("inline-name-edit-started", id));
    let response = ui
        .add(crate::targets::text_field(text).id(id).desired_width(width))
        .on_hover_text(tip);
    crate::a11y::Named::named(response.clone(), &t!("tree.rename.title"));
    if !ui.data(|d| d.get_temp::<bool>(started).unwrap_or(false)) {
        response.request_focus();
        ui.data_mut(|d| d.insert_temp(started, true));
        return None;
    }
    if response.lost_focus() {
        ui.data_mut(|d| d.remove::<bool>(started));
        let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
        return Some(if escaped {
            NameEdit::Cancel
        } else {
            NameEdit::Commit
        });
    }
    None
}

impl App {
    /// The right-click menu of a page tab: rename, add, then remove last.
    ///
    /// # Rules
    /// - Page 0 is the Main page: Rename and Remove are disabled and say why.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/menu.rs (`Menu`, `Item`).
    ///
    /// # Do not
    /// - Run a command here; `page_switcher` runs the returned [`PageAct`].
    fn page_menu(page: Id) -> Vec<crate::ui_kit::menu::Entry<PageAct>> {
        use crate::ui_kit::menu::{Item, Menu};
        use crate::ui_kit::{icons as kit, menu_icons as glyph};
        let named = page != 0;
        let why = t!("schematic.pages.main.tip");
        Menu::new()
            .group([Item::new(
                PageAct::Rename,
                t!("schematic.pages.rename"),
                t!("schematic.pages.rename.tip"),
            )
            .icon(glyph::RENAME)
            .enabled_if(named, why.clone())
            .into()])
            .group([Item::new(
                PageAct::Add,
                t!("schematic.pages.add"),
                t!("schematic.pages.add.tip"),
            )
            .icon(kit::PLUS)
            .into()])
            .destructive(
                Item::new(
                    PageAct::Remove,
                    t!("schematic.pages.remove"),
                    t!("schematic.pages.remove.tip"),
                )
                .icon(glyph::TRASH)
                .enabled_if(named, why),
            )
            .entries()
    }

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

    /// The key of the Layers tree row that stands for page `id`.
    ///
    /// # Dependencies
    /// `layers_tree.rs` (`schematic_children` builds rows with this key).
    pub(crate) fn page_row_key(id: Id) -> String {
        format!("sec:sheet:{id}")
    }

    /// Show page `id`: open it on the canvas and select it, so Properties
    /// shows its card. Clears any selected object, which may sit on another page.
    ///
    /// # Rules
    /// Keeps a highlighted connection; only the page and the selection change.
    ///
    /// # Do not
    /// Set `active_page` directly from a surface; the tab, the tree row and
    /// Properties must agree on which page is selected.
    pub(crate) fn show_page(&mut self, id: Id) {
        self.active_page = id;
        self.select(None);
        self.tree.selected_key = Some(Self::page_row_key(id));
    }

    /// Add an empty page with the next free name ("Page 2", "Page 3"...) and show it.
    ///
    /// # Rules
    /// Goes through `schematic.new_sheet`, so it is undoable and the engine
    /// bounds the name.
    ///
    /// # Dependencies
    /// `../../engine/src/lib.rs` (`schematic.new_sheet`).
    ///
    /// # Do not
    /// Pick the name in a caller; two surfaces would number pages differently.
    pub(crate) fn add_page(&mut self) {
        let taken: Vec<String> = std::iter::once(t!("schematic.pages.root"))
            .chain(self.project.sheets.iter().map(|s| s.name.clone()))
            .collect();
        let mut number = taken.len();
        while taken
            .iter()
            .any(|n| *n == t!("schematic.pages.new_name", number = number))
        {
            number += 1;
        }
        let name = t!("schematic.pages.new_name", number = number);
        let before = self.project.sheets.len();
        self.tool_call("schematic.new_sheet", json!({"name": name}));
        if self.project.sheets.len() > before
            && let Some(sheet) = self.project.sheets.last()
        {
            let id = sheet.id;
            self.show_page(id);
        }
    }

    /// Rename page `id`. The Main page, an empty name and an unchanged name do nothing.
    ///
    /// # Dependencies
    /// `../../engine/src/lib.rs` (`schematic.rename_sheet` refuses a name that
    /// another page already has and reports it in the status line).
    pub(crate) fn rename_page(&mut self, id: Id, name: &str) {
        let name = name.trim();
        let current = self.project.sheets.iter().find(|s| s.id == id);
        if id == 0 || name.is_empty() || current.is_none_or(|s| s.name == name) {
            return;
        }
        self.tool_call("schematic.rename_sheet", json!({"id": id, "name": name}));
    }

    /// Remove page `id`; its parts move to the Main page. The Main page cannot be removed.
    ///
    /// # Rules
    /// Passes `move_to: 0`, so a page that holds parts is removed instead of
    /// refused. Shows Main afterwards when the removed page was showing.
    ///
    /// # Dependencies
    /// `../../engine/src/lib.rs` (`schematic.remove_sheet`).
    pub(crate) fn remove_page(&mut self, id: Id) {
        if id == 0 {
            self.message = t!("schematic.pages.main.tip");
            return;
        }
        if self.tool_call("schematic.remove_sheet", json!({"id": id, "move_to": 0}))
            && (self.active_page == id
                || self.tree.selected_key.as_deref() == Some(Self::page_row_key(id).as_str()))
        {
            self.show_page(0);
        }
    }

    /// The name page `id` shows in tabs and Properties.
    pub(crate) fn page_name(&self, id: Id) -> String {
        if id == 0 {
            return t!("schematic.pages.root");
        }
        self.project
            .sheets
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone())
            .unwrap_or_default()
    }

    /// The Properties card of a page: its name (edit to rename), how many
    /// parts it holds, and Remove page.
    ///
    /// `preview` is true while the pointer rests on the page's row in the
    /// Layers tree: the card is read-only and the selection is unchanged.
    ///
    /// # Rules
    /// - The name commits on Enter or when it loses focus, through `rename_page`.
    /// - Main shows its name and a note instead of a name field and Remove.
    ///
    /// # Dependencies
    /// `ui_kit/section.rs`, `ui_kit/form_row.rs`, `ui_kit/button.rs`;
    /// `properties.rs` (`properties_header`).
    ///
    /// # Do not
    /// Offer buttons in the preview; the pointer is leaving for the card.
    pub(crate) fn page_properties(&mut self, ui: &mut egui::Ui, id: Id, preview: bool) {
        use crate::ui_kit::{
            button::{Size, secondary_button},
            form_row::{FormWidth, form_row},
            section::section,
        };
        if id != 0 && !self.project.sheets.iter().any(|s| s.id == id) {
            return;
        }
        let name = self.page_name(id);
        let parts = self
            .project
            .components
            .iter()
            .filter(|c| c.sheet == id)
            .count();
        let caption = if preview {
            t!("props.page.preview")
        } else {
            t!("props.page.caption")
        };
        self.properties_header(ui, &name, &caption);
        let draft_id = egui::Id::new(("page-name-draft", id));
        let mut remove = false;
        let mut rename: Option<String> = None;
        section(
            ui,
            ("page", id),
            &t!("props.page.general"),
            &t!("props.page.general.tip"),
            |ui| {
                form_row(
                    ui,
                    FormWidth::Aside,
                    &t!("props.page.name"),
                    None,
                    None,
                    |ui| {
                        if id == 0 || preview {
                            let muted = theme::tokens(ui.ctx()).text;
                            ui.label(fonts::text(fonts::Role::Body, &name).color(muted));
                            return;
                        }
                        let mut text: String = ui
                            .data(|d| d.get_temp(draft_id))
                            .unwrap_or_else(|| name.clone());
                        let response = ui
                            .add(
                                crate::targets::text_field(&mut text)
                                    .id(draft_id.with("field"))
                                    .desired_width(f32::INFINITY),
                            )
                            .on_hover_text(t!("props.page.name.tip"));
                        crate::a11y::Named::named(response.clone(), &t!("props.page.name"));
                        if response.lost_focus() {
                            ui.data_mut(|d| d.remove::<String>(draft_id));
                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                return;
                            }
                            rename = Some(text);
                        } else {
                            ui.data_mut(|d| d.insert_temp(draft_id, text));
                        }
                    },
                );
                form_row(
                    ui,
                    FormWidth::Aside,
                    &t!("props.page.parts"),
                    None,
                    None,
                    |ui| {
                        ui.label(fonts::text(fonts::Role::Mono, parts.to_string()));
                    },
                );
                if id == 0 {
                    ui.add_space(6.0);
                    ui.label(fonts::caption(t!("props.page.main_note")));
                } else if !preview {
                    ui.add_space(8.0);
                    if secondary_button(
                        ui,
                        None,
                        &t!("schematic.pages.remove"),
                        &t!("schematic.pages.remove.tip"),
                        Size::Normal,
                        true,
                    )
                    .clicked()
                    {
                        remove = true;
                    }
                }
            },
        );
        if let Some(text) = rename {
            self.rename_page(id, &text);
        }
        if remove {
            self.remove_page(id);
        }
    }

    /// Draw the page tabs and a plus tab, and apply what the user did.
    ///
    /// When a connection is highlighted, tabs whose page carries it get a dot
    /// (`Project::net_pages`), so the user can walk every page it touches.
    /// Switching pages keeps the highlight. Right-click on a tab offers Add
    /// page, Rename and Remove page; F2 turns the showing tab into a name field.
    ///
    /// # Rules
    /// Every control has a tooltip from the text catalog. Page changes go
    /// through `add_page`, `rename_page` and `remove_page` so they are
    /// undoable and bounded by the engine.
    ///
    /// # Dependencies
    /// `../../engine/src/lib.rs` (`schematic.new_sheet`, `schematic.rename_sheet`,
    /// `schematic.remove_sheet`).
    ///
    /// # Do not
    /// Call it for the board tab, and do not edit sheets directly. Do not add
    /// Add, Rename or Remove buttons back to the strip.
    pub(crate) fn page_switcher(&mut self, ui: &mut egui::Ui) {
        let current = self.schematic_page();
        let lit: Vec<Id> = self
            .highlight_net
            .map(|net| self.project.net_pages(net))
            .unwrap_or_default();
        let mut pages: Vec<(Id, String)> = vec![(0, t!("schematic.pages.root"))];
        pages.extend(self.project.sheets.iter().map(|s| (s.id, s.name.clone())));
        let mut rename: Option<(Id, String)> = ui.data(|d| d.get_temp(rename_key()));
        let mut go = None;
        let mut tab_act: Option<(Id, PageAct)> = None;
        let mut add = false;
        let mut remove = None;
        let mut start_rename = None;
        let mut finished: Option<(Id, String, NameEdit)> = None;
        let mut tab_ids: Vec<egui::Id> = Vec::new();
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            for (id, name) in &pages {
                if let Some((rid, text)) = rename.as_mut()
                    && rid == id
                {
                    if let Some(done) = inline_name_edit(
                        ui,
                        ("page-tab", *id),
                        text,
                        120.0,
                        &t!("schematic.pages.name_field.tip"),
                    ) {
                        finished = Some((*id, text.clone(), done));
                    }
                    continue;
                }
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
                let response = ui.selectable_label(*id == current, text).on_hover_text(tip);
                tab_ids.push(response.id);
                if response.clicked() {
                    go = Some(*id);
                }
                if let Some(act) =
                    crate::ui_kit::menu::context(&response, "page-tab", || Self::page_menu(*id))
                {
                    tab_act = Some((*id, act));
                }
            }
            let plus = ui
                .add(
                    egui::Button::new(t!("schematic.pages.add_tab"))
                        .min_size(egui::Vec2::splat(crate::targets::MIN_TARGET)),
                )
                .on_hover_text(t!("schematic.pages.add.tip"));
            crate::a11y::name_control(&plus, &t!("schematic.pages.add"), None);
            if plus.clicked() {
                add = true;
            }
        });
        ui.add_space(4.0);
        // F2 renames the page showing, unless another control holds the keyboard.
        let free = ui.memory(|m| m.focused().is_none_or(|f| tab_ids.contains(&f)));
        if free
            && rename.is_none()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2))
        {
            start_rename = Some(current);
        }
        match tab_act {
            Some((_, PageAct::Add)) => add = true,
            Some((id, PageAct::Rename)) => start_rename = Some(id),
            Some((id, PageAct::Remove)) => remove = Some(id),
            None => {}
        }
        if let Some(id) = go {
            ui.data_mut(|d| d.remove::<(Id, String)>(rename_key()));
            self.show_page(id);
            return;
        }
        if let Some(id) = start_rename {
            if id == 0 {
                self.message = t!("schematic.pages.main.tip");
            } else {
                let edit = (id, self.page_name(id));
                ui.data_mut(|d| d.insert_temp(rename_key(), edit.clone()));
                rename = Some(edit);
            }
        }
        match (finished, rename) {
            (Some((id, text, done)), _) => {
                ui.data_mut(|d| d.remove::<(Id, String)>(rename_key()));
                if done == NameEdit::Commit {
                    self.rename_page(id, &text);
                }
            }
            (None, Some(edit)) => {
                ui.data_mut(|d| d.insert_temp(rename_key(), edit));
            }
            (None, None) => {}
        }
        if add {
            self.add_page();
        }
        if let Some(id) = remove {
            self.remove_page(id);
        }
    }
}
