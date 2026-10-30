//! The right-click menu of the canvas: what each target offers and what a pick does.
//!
//! Context: `canvas.rs` opens the menu at the pointer (`open_canvas_menu`) and
//! keeps `App::canvas_menu`; this file lists the rows for the target and runs the
//! row the person picked. The panel, rows, keyboard and shadow are drawn by
//! `../ui_kit/menu.rs`; nothing here paints.
//! Local rules: rows come in four groups, in this order: object actions, edit
//! (copy, paste, duplicate), view (fit, clear), then destructive (Delete). Every
//! change goes through `App::run` with a typed engine command, as the toolbar does.
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/menu.rs
//! - crates/ui-egui/src/ui_kit/menu_icons.rs
//! - crates/ui-egui/src/modifier_ui.rs (`menu_rows`, the footer key list)
//! - crates/ui-egui/src/canvas.rs (`CanvasMenuKind`)
//!
//! # Do not
//! - Draw a row or a panel here; add the row to the list and the action to `run_canvas_menu_act`.
use super::*;
use crate::ui_kit::icons;
use crate::ui_kit::menu::{self, Entry, Item, Menu, Outcome};
use crate::ui_kit::menu_icons as glyph;

/// What picking a row of the canvas menu does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CanvasAct {
    FitView,
    Paste,
    ClearSelection,
    Copy,
    StartWire,
    MarkNoConnect,
    Properties,
    StartRoute,
    CutTrack,
    ChangeWidth,
    AssignLayer(Layer),
    AssignSpan(Layer, Layer),
    Duplicate,
    Rotate,
    ToggleLock,
    Delete,
    ExportBom,
    ExportBomSelection,
    ToggleBomExclude,
}

impl App {
    /// Draw the canvas menu for one frame and act on the row picked.
    ///
    /// Why: the canvas keeps the menu open across frames in `canvas_menu`; this
    /// is the single place that shows it.
    ///
    /// # Rules
    /// - Returns nothing; the menu stays in `canvas_menu` until it closes or a row is picked.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/menu.rs `show`.
    ///
    /// # Do not
    /// - Call when `canvas_menu` is empty.
    pub(super) fn show_canvas_menu(
        &mut self,
        ctx: &egui::Context,
        screen: Pos2,
        kind: CanvasMenuKind,
    ) {
        let entries = self.canvas_menu_entries(ctx, &kind);
        match menu::show(ctx, egui::Id::new("canvas-context-menu"), screen, &entries) {
            Outcome::Open => self.canvas_menu = Some((screen, kind)),
            Outcome::Picked(act) => self.run_canvas_menu_act(ctx, &kind, act),
            Outcome::Closed => {}
        }
    }

    /// The rows for a target, grouped, with the modifier hint as the footer.
    fn canvas_menu_entries(
        &self,
        ctx: &egui::Context,
        kind: &CanvasMenuKind,
    ) -> Vec<Entry<CanvasAct>> {
        let copy = || {
            Item::new(CanvasAct::Copy, "Copy", command_tip("selection.copy_text"))
                .icon(glyph::COPY)
                .shortcut(menu::chord(ctx, egui::Key::C))
                .into()
        };
        let properties = |tip: &str| -> Entry<CanvasAct> {
            Item::new(CanvasAct::Properties, "Properties", tip)
                .icon(glyph::PROPERTIES)
                .shortcut("E")
                .into()
        };
        let mut menu = Menu::new();
        match kind {
            CanvasMenuKind::Empty => {
                let ready = self.edit.clip.is_some();
                menu = menu
                    .group([Item::new(
                        CanvasAct::Paste,
                        "Paste",
                        if ready {
                            command_tip("selection.paste")
                        } else {
                            "Copy something first".to_string()
                        },
                    )
                    .icon(glyph::PASTE)
                    .shortcut(menu::chord(ctx, egui::Key::V))
                    .enabled_if(ready, "Copy something first")
                    .into()])
                    .group([
                        Item::new(
                            CanvasAct::FitView,
                            "Fit view",
                            "Zoom the view so the whole design fits.",
                        )
                        .icon(icons::FIT)
                        .shortcut("Home")
                        .into(),
                        Item::new(
                            CanvasAct::ClearSelection,
                            "Clear selection",
                            "Drop the current highlight so nothing is selected.",
                        )
                        .icon(icons::CLOSE)
                        .into(),
                    ]);
            }
            CanvasMenuKind::Pin(_) => {
                menu = menu
                    .group([
                        Item::new(
                            CanvasAct::StartWire,
                            "Start wire",
                            "Wire tool: click another pin to connect them.",
                        )
                        .icon(glyph::WIRE)
                        .into(),
                        Item::new(
                            CanvasAct::MarkNoConnect,
                            "Mark no connect",
                            "Mark this pin deliberately unconnected, or clear an existing mark.",
                        )
                        .icon(icons::CLOSE)
                        .into(),
                        properties("Load this pin's part into the inspector."),
                    ])
                    .group([copy()]);
            }
            CanvasMenuKind::Pad(..) => {
                menu = menu
                    .group([
                        Item::new(
                            CanvasAct::StartRoute,
                            "Start route",
                            "Route tool: click another pad to draw a track between them.",
                        )
                        .icon(glyph::WIRE)
                        .into(),
                        properties("Load this pad's package into the inspector."),
                    ])
                    .group([copy()]);
            }
            CanvasMenuKind::Track(id) => {
                menu = menu
                    .group(
                        self.track_rows(*id)
                            .into_iter()
                            .chain([properties("Load this wire into the inspector.")]),
                    )
                    .destructive(
                        Item::new(CanvasAct::Delete, "Delete", command_tip("object.delete"))
                            .icon(glyph::TRASH)
                            .shortcut("Del"),
                    );
            }
            CanvasMenuKind::Object(id) => {
                let id = *id;
                let is_track = self.project.board.tracks.iter().any(|t| t.id == id);
                let is_via = self.project.board.vias.iter().any(|v| v.id == id);
                let part =
                    self.project.component(id).is_some() || self.project.footprint(id).is_some();
                let footprint = self.project.footprint(id).is_some();
                let locked = self
                    .project
                    .footprint(id)
                    .is_some_and(|fp| fp.locked == Some(true));
                let mut object: Vec<Entry<CanvasAct>> = Vec::new();
                if is_via {
                    object.push(self.span_row(id));
                }
                if part {
                    let tip = if locked {
                        "This part is locked. Unlock it first."
                    } else {
                        "Turn the selected object by 90 degrees."
                    };
                    object.push(
                        Item::new(CanvasAct::Rotate, "Rotate", tip)
                            .icon(glyph::ROTATE)
                            .shortcut("R")
                            .enabled_if(!locked, tip)
                            .into(),
                    );
                }
                if footprint {
                    object.push(
                        Item::new(
                            CanvasAct::ToggleLock,
                            if locked { "Unlock" } else { "Lock in place" },
                            if locked {
                                "Let this part move, rotate and dock again."
                            } else {
                                "Freeze this part so no accidental click, key or dock can nudge it off where you placed it."
                            },
                        )
                        .icon(if locked { icons::LOCK_OPEN } else { icons::LOCK })
                        .into(),
                    );
                }
                if is_track {
                    object.extend(self.track_rows(id));
                }
                object.push(properties("Load this object into the inspector."));
                let mut edit: Vec<Entry<CanvasAct>> = vec![copy()];
                if part {
                    edit.push(
                        Item::new(
                            CanvasAct::Duplicate,
                            "Duplicate",
                            "Places a copy of this part nearby, with its own new reference number. Undo removes the copy.",
                        )
                        .icon(glyph::DUPLICATE)
                        .into(),
                    );
                }
                let delete_tip = if locked {
                    "This part is locked. Unlock it first.".to_string()
                } else {
                    command_tip("object.delete")
                };
                let mut bom: Vec<Entry<CanvasAct>> = Vec::new();
                if part {
                    bom = self
                        .bom_menu_rows(id)
                        .into_iter()
                        .map(|row| {
                            let (act, label, tip, checked) = row;
                            Item::new(act, label, tip)
                                .icon(glyph::PROPERTIES)
                                .checked(checked)
                                .into()
                        })
                        .collect();
                }
                menu = menu.group(object).group(edit).group(bom).destructive(
                    Item::new(CanvasAct::Delete, "Delete", delete_tip.clone())
                        .icon(glyph::TRASH)
                        .shortcut("Del")
                        .enabled_if(!locked, delete_tip),
                );
            }
        }
        let hint = crate::modifier_ui::menu_rows(
            &self.preferences.modifiers,
            self.tool.kind(),
            crate::tool_modifiers::Platform::of(ctx),
        );
        menu.footer(hint, t!("modifier.menu.tip")).entries()
    }

    /// Cut, Change width and Assign layer for a wire.
    fn track_rows(&self, id: Id) -> Vec<Entry<CanvasAct>> {
        let Some(current) = self
            .project
            .board
            .tracks
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.layer)
        else {
            return Vec::new();
        };
        let faces: Vec<Entry<CanvasAct>> = [
            (
                Layer::Front,
                "Front",
                "Move this wire to the front copper layer.",
            ),
            (
                Layer::Back,
                "Back",
                "Move this wire to the back copper layer.",
            ),
        ]
        .into_iter()
        .filter(|(layer, ..)| *layer != current)
        .map(|(layer, label, tip)| Item::new(CanvasAct::AssignLayer(layer), label, tip).into())
        .collect();
        let mut rows: Vec<Entry<CanvasAct>> = vec![
            Item::new(
                CanvasAct::CutTrack,
                "Cut",
                "Break this stretch of wire into two equal halves so you can bend, branch or re-route part of it. Undo joins it back into one.",
            )
            .icon(glyph::SPLIT)
            .into(),
            Item::new(
                CanvasAct::ChangeWidth,
                "Change width",
                "Set this wire to the width shown in the inspector.",
            )
            .icon(glyph::WIDTH)
            .into(),
        ];
        if !faces.is_empty() {
            rows.push(Entry::submenu(
                "Assign layer",
                "Choose which copper layer this wire lives on.",
                Some(glyph::LAYERS),
                faces,
            ));
        }
        rows
    }

    /// "Assign layers" for a via: the spans this board allows, the current one marked in text.
    fn span_row(&self, id: Id) -> Entry<CanvasAct> {
        let current = self
            .project
            .board
            .vias
            .iter()
            .find(|v| v.id == id)
            .map(|v| self.project.via_span(v));
        let last = self.project.copper_layer_count().saturating_sub(1);
        let tip = command_tip("via.set_span");
        let spans = self
            .project
            .allowed_via_spans()
            .into_iter()
            .map(|(a, b)| {
                let mut label = if a.index() == 0 && b.index() >= last {
                    "Top to bottom".to_string()
                } else {
                    format!(
                        "{} to {}",
                        self.project.layer_name(a),
                        self.project.layer_name(b)
                    )
                };
                if Some((a, b)) == current {
                    label.push_str(" (current)");
                }
                Item::new(CanvasAct::AssignSpan(a, b), label, tip.clone()).into()
            })
            .collect();
        Entry::submenu(
            "Assign layers",
            "Choose which copper layers this via connects.",
            Some(glyph::LAYERS),
            spans,
        )
    }

    /// Run the row picked in the canvas menu on its target.
    fn run_canvas_menu_act(&mut self, ctx: &egui::Context, kind: &CanvasMenuKind, act: CanvasAct) {
        let id = match kind {
            CanvasMenuKind::Pad(id, _) | CanvasMenuKind::Track(id) | CanvasMenuKind::Object(id) => {
                Some(*id)
            }
            _ => None,
        };
        match (act, kind) {
            (CanvasAct::FitView, _) => self.needs_fit = true,
            (CanvasAct::Paste, _) => {
                if let Some(at) = self.edit.menu_at {
                    self.paste_last_copy_at(at);
                }
            }
            (CanvasAct::ClearSelection, _) => self.select(None),
            (CanvasAct::Copy, _) => self.copy_selection(ctx),
            (CanvasAct::StartWire, CanvasMenuKind::Pin(pin)) => {
                if let Some(at) = self.project.pin_position(&PinRef {
                    component: pin.component,
                    pin: pin.pin.clone(),
                }) {
                    self.wire_route = vec![at];
                }
                self.tool = Tool::Wire(Some(PinRef {
                    component: pin.component,
                    pin: pin.pin.clone(),
                }));
            }
            (CanvasAct::MarkNoConnect, _) => self.tool = Tool::NoConnect,
            (CanvasAct::Properties, _) => self.load_properties(),
            (CanvasAct::StartRoute, CanvasMenuKind::Pad(id, pad)) => {
                self.tool = Tool::Route(Some((*id, pad.clone())));
            }
            (CanvasAct::CutTrack, _) => {
                if let Some(id) = id
                    && let Some(track) = self.project.board.tracks.iter().find(|t| t.id == id)
                {
                    let at = Point::new(
                        (track.start.x + track.end.x) / 2.0,
                        (track.start.y + track.end.y) / 2.0,
                    );
                    self.run(
                        "track.split",
                        json!({"id": id, "at": {"x": at.x, "y": at.y}}),
                    );
                }
            }
            (CanvasAct::ChangeWidth, _) => {
                if let Some(id) = id {
                    self.run(
                        "track.set_width",
                        json!({"id": id, "width": self.track_width}),
                    );
                }
            }
            (CanvasAct::AssignLayer(layer), _) => {
                if let Some(id) = id {
                    let name = if layer == Layer::Front {
                        "F.Cu"
                    } else {
                        "B.Cu"
                    };
                    self.run("track.set_layer", json!({"id": id, "layer": name}));
                }
            }
            (CanvasAct::AssignSpan(a, b), _) => {
                if let Some(id) = id {
                    self.run(
                        "via.set_span",
                        json!({"id": id, "from_layer": self.project.layer_name(a), "to_layer": self.project.layer_name(b)}),
                    );
                }
            }
            (CanvasAct::Duplicate, _) => {
                if let Some(id) = id {
                    let target = self.project.footprint(id).map_or(id, |fp| fp.component);
                    self.run(
                        "object.duplicate",
                        json!({"ids": [target], "offset": {"x": 2.0, "y": 2.0}}),
                    );
                }
            }
            (CanvasAct::Rotate, _) => self.rotate(),
            (CanvasAct::ToggleLock, _) => {
                if let Some(id) = id {
                    let locked = self
                        .project
                        .footprint(id)
                        .is_some_and(|fp| fp.locked == Some(true));
                    self.run("footprint.set_locked", json!({"id": id, "locked": !locked}));
                }
            }
            (CanvasAct::ExportBom, _) => {
                self.open_dialog(crate::file_dialogs::Mode::Export, Some("bom"), None, vec![]);
            }
            (CanvasAct::ExportBomSelection, _) => {
                let parts = self.selected_part_components();
                self.open_dialog(crate::file_dialogs::Mode::Export, Some("bom"), None, parts);
            }
            (CanvasAct::ToggleBomExclude, _) => {
                if let Some(id) = id {
                    let component = self.project.footprint(id).map_or(id, |f| f.component);
                    let exclude = !self.part_excluded(id);
                    let mut targets = vec![component];
                    let selection = self.selected_part_components();
                    if selection.contains(&component) {
                        targets = selection;
                    }
                    self.set_bom_exclusion(&targets, exclude);
                }
            }
            (CanvasAct::Delete, _) => {
                if let Some(id) = id {
                    self.run("object.delete", json!({"id": id}));
                    self.select(None);
                }
            }
            _ => {}
        }
    }

    /// The bill-of-materials rows of a part's menu: (action, label, tip, checked).
    ///
    /// Why: the canvas menu and the Layers tree menu show the same three entries.
    ///
    /// # Rules
    /// - "Export BOM for selection..." appears only when more than one part is selected.
    /// - The exclusion row is a checkable item: checked while the part is left out.
    fn bom_menu_rows(&self, id: Id) -> Vec<(CanvasAct, String, String, bool)> {
        let excluded = self.part_excluded(id);
        let mut rows = vec![(
            CanvasAct::ExportBom,
            "Export BOM…".to_owned(),
            "A bill of materials is the shopping list for your board: every part, how many, and which part number to buy. Opens Export with all parts.".to_owned(),
            false,
        )];
        if self.selected_part_components().len() > 1 {
            rows.push((
                CanvasAct::ExportBomSelection,
                "Export BOM for selection…".to_owned(),
                "Write a bill of materials for only the parts you have selected.".to_owned(),
                false,
            ));
        }
        rows.push((
            CanvasAct::ToggleBomExclude,
            if excluded { "Include in BOM" } else { "Exclude from BOM" }.to_owned(),
            "Leave this part off the bill of materials, for example a test point or a logo. The bill is the shopping list for your board. Undo brings it back.".to_owned(),
            excluded,
        ));
        rows
    }
}
