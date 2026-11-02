//! The Machines view of the Library tab: every machine a person can use, as cards.
//!
//! Context: issue #1167. Built-in machines, the person's own machines and the copies kept
//! in the open project are all shown the same way, with a source badge, so a built-in
//! machine is an ordinary card. A card can be added to the project, duplicated, hidden
//! (built-in only), saved to the person's own machines, updated from the library when the
//! project copy is behind, or removed from the person's own machines.
//!
//! Local rules: this module holds view state only. The list comes from the read-only
//! `machine.library_list` command and every change runs a `machine.*` command, so a project
//! change is one undo step and a library change touches only the preferences folder.
//!
//! # Dependencies
//! - crates/engine/src/machines.rs (the `machine.*` library commands)
//! - crates/ui-egui/src/ui_kit/card.rs, segmented.rs, select.rs, dialog.rs
//!
//! # Do not
//! - Read machine files here; ask the engine.
//! - Name a machine id; the list is whatever the library holds.
use super::*;
use crate::ui_kit::{
    card::card,
    checkbox::checkbox,
    dialog::{self, Dialog, Outcome},
    segmented::{Segment, segmented},
    select::select,
};

/// Machine kinds a person can start a blank machine from, as (engine id, catalog key).
const KINDS: [&str; 5] = [
    "pick_place",
    "laser_galvo",
    "laser_gantry",
    "cnc_mill",
    "uv_printer",
];

/// The plain name of a machine kind for the Add machine list.
fn kind_name(kind: &str) -> String {
    match kind {
        "pick_place" => t!("libws.machines.kind.pick_place"),
        "laser_galvo" => t!("libws.machines.add.kind.laser_galvo"),
        "laser_gantry" => t!("libws.machines.add.kind.laser_gantry"),
        "cnc_mill" => t!("libws.machines.kind.cnc_mill"),
        _ => t!("libws.machines.add.kind.uv_printer"),
    }
}

/// The kind filter choices: all, pick and place, laser, mill.
const KIND_FILTERS: [&str; 4] = ["", "pick_place", "laser", "cnc_mill"];

/// The source filter choices: all, built in, mine, project.
const SOURCE_FILTERS: [&str; 4] = ["all", "built_in", "mine", "project"];

/// The Add machine form while it is open.
pub(crate) struct AddForm {
    /// 0 starts from a library machine, 1 from a blank machine of a kind.
    from: usize,
    /// Index into the library machines list when starting from one.
    library_pick: usize,
    /// Index into [`KINDS`] when starting blank.
    kind_pick: usize,
    name: String,
    to_project: bool,
    keep: bool,
}

impl Default for AddForm {
    fn default() -> Self {
        Self {
            from: 0,
            library_pick: 0,
            kind_pick: 0,
            name: String::new(),
            to_project: true,
            keep: false,
        }
    }
}

/// View state of the Machines view.
#[derive(Default)]
pub(crate) struct MachineLib {
    /// True when the Library tab shows machines instead of parts.
    pub(crate) on: bool,
    kind: usize,
    source: usize,
    query: String,
    show_hidden: bool,
    /// The last `machine.library_list` answer and the revision it was read at.
    pub(crate) cards: Vec<Value>,
    loaded_at: Option<u64>,
    pub(crate) stale: bool,
    pub(crate) add: Option<AddForm>,
}

impl App {
    /// Read the library list into the view state when it is out of date.
    ///
    /// # Rules
    /// - Reads again after any change and when the project revision moves.
    /// - Never sets the status line; a read is not news.
    pub(crate) fn machine_lib_refresh(&mut self) {
        if !self.libws.machines.stale && self.libws.machines.loaded_at == Some(self.revision) {
            return;
        }
        let answer = self.session.lock().ok().and_then(|mut s| {
            s.execute(
                "machine.library_list",
                json!({"include_hidden": true}),
                None,
            )
            .ok()
        });
        self.libws.machines.cards = answer
            .as_ref()
            .and_then(|v| v.pointer("/result/machines"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        self.libws.machines.loaded_at = Some(self.revision);
        self.libws.machines.stale = false;
    }

    /// Run one machine command from the view and read the list again afterwards.
    fn machine_lib_run(&mut self, command: &str, params: Value) {
        self.run(command, params);
        self.libws.machines.stale = true;
    }

    /// The Parts and Machines switch at the left of the Library tool bar.
    pub(crate) fn library_view_switch(&mut self, ui: &mut egui::Ui) {
        let parts = t!("libws.view.parts");
        let machines = t!("libws.view.machines");
        let (parts_tip, machines_tip) = (t!("libws.view.parts.tip"), t!("libws.view.machines.tip"));
        let mut selected = usize::from(self.libws.machines.on);
        let items = [
            Segment::text(&parts, &parts_tip),
            Segment::text(&machines, &machines_tip),
        ];
        if segmented(ui, "library-view-switch", &mut selected, &items).changed() {
            self.libws.machines.on = selected == 1;
            self.libws.machines.stale = true;
        }
    }

    /// The centre of the Library tab when it shows machines.
    pub(crate) fn library_machines_page(&mut self, ui: &mut egui::Ui) {
        self.machine_lib_refresh();
        let frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(24, 16));
        egui::ScrollArea::vertical()
            .id_salt("library-machines-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                frame.show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = theme::SPACE[3];
                    self.machine_lib_filters(ui);
                    self.machine_lib_cards(ui);
                });
            });
        self.machine_lib_add_dialog(ui.ctx());
    }

    fn machine_lib_filters(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let labels = [
                t!("libws.machines.filter.all"),
                t!("libws.machines.kind.pick_place"),
                t!("libws.machines.kind.laser"),
                t!("libws.machines.kind.cnc_mill"),
            ];
            let tips = [
                t!("libws.machines.filter.all.tip"),
                t!("libws.machines.kind.pick_place.tip"),
                t!("libws.machines.kind.laser.tip"),
                t!("libws.machines.kind.cnc_mill.tip"),
            ];
            let items: Vec<Segment<'_>> = labels
                .iter()
                .zip(&tips)
                .map(|(l, tip)| Segment::text(l, tip))
                .collect();
            segmented(ui, "machine-kind-filter", &mut self.libws.machines.kind, &items);
            let labels = [
                t!("libws.machines.filter.all_sources"),
                t!("libws.machines.source.built_in"),
                t!("libws.machines.source.mine"),
                t!("libws.machines.source.project"),
            ];
            let tips = [
                t!("libws.machines.filter.all_sources.tip"),
                t!("libws.machines.source.built_in.tip"),
                t!("libws.machines.source.mine.tip"),
                t!("libws.machines.source.project.tip"),
            ];
            let items: Vec<Segment<'_>> = labels
                .iter()
                .zip(&tips)
                .map(|(l, tip)| Segment::text(l, tip))
                .collect();
            segmented(
                ui,
                "machine-source-filter",
                &mut self.libws.machines.source,
                &items,
            );
        });
        ui.horizontal(|ui| {
            let search = ui
                .add(
                    crate::targets::text_field(&mut self.libws.machines.query)
                        .hint_text(t!("libws.machines.search"))
                        .desired_width(260.0),
                )
                .on_hover_text(t!("libws.machines.search.tip"));
            let enabled = search.enabled();
            search.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::TextEdit,
                    enabled,
                    t!("libws.machines.search"),
                )
            });
            checkbox(
                ui,
                &mut self.libws.machines.show_hidden,
                &t!("libws.machines.show_hidden"),
                &t!("libws.machines.show_hidden.tip"),
            );
            ui.label(t!("libws.machines.show_hidden"));
            if primary_button(
                ui,
                None,
                &t!("libws.machines.add"),
                &t!("libws.machines.add.tip"),
                Size::Normal,
                true,
            )
            .clicked()
            {
                self.libws.machines.add = Some(AddForm::default());
            }
        });
    }

    /// True when `card` passes the kind, source, search and hidden filters.
    fn machine_lib_shows(&self, card: &Value) -> bool {
        let m = &self.libws.machines;
        let text = |key: &str| card.get(key).and_then(Value::as_str).unwrap_or("");
        let kind = text("kind");
        let want_kind = KIND_FILTERS.get(m.kind).copied().unwrap_or("");
        let kind_ok = match want_kind {
            "" => true,
            "laser" => kind.starts_with("laser"),
            other => kind == other,
        };
        let want_source = SOURCE_FILTERS.get(m.source).copied().unwrap_or("all");
        let source_ok = want_source == "all" || text("source") == want_source;
        let hidden = card.get("hidden").and_then(Value::as_bool) == Some(true);
        let query = m.query.trim().to_lowercase();
        let query_ok = query.is_empty()
            || ["name", "vendor", "model", "id"]
                .iter()
                .any(|k| text(k).to_lowercase().contains(&query));
        kind_ok && source_ok && query_ok && (m.show_hidden || !hidden)
    }

    fn machine_lib_cards(&mut self, ui: &mut egui::Ui) {
        let cards: Vec<Value> = self
            .libws
            .machines
            .cards
            .iter()
            .filter(|c| self.machine_lib_shows(c))
            .cloned()
            .collect();
        if cards.is_empty() {
            ui.label(egui::RichText::new(t!("libws.machines.none")).color(self.muted()));
        }
        for c in &cards {
            self.machine_lib_card(ui, c);
        }
    }

    fn machine_lib_card(&mut self, ui: &mut egui::Ui, c: &Value) {
        let text = |key: &str| c.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
        let flag = |key: &str| c.get(key).and_then(Value::as_bool) == Some(true);
        let (id, name, source) = (text("id"), text("name"), text("source"));
        let caption = format!("{} {}", text("vendor"), text("model"));
        let in_project = flag("in_project");
        let hidden = flag("hidden");
        let copy_state = text("copy_state");
        let muted = self.muted();
        let mut action: Option<(&str, Value)> = None;
        card(
            ui,
            &name,
            Some(caption.trim()),
            |ui| {
                let (badge, tip) = match source.as_str() {
                    "built_in" => (
                        t!("libws.machines.source.built_in.badge"),
                        t!("libws.machines.source.built_in.tip"),
                    ),
                    "mine" => (
                        t!("libws.machines.source.mine.badge"),
                        t!("libws.machines.source.mine.tip"),
                    ),
                    _ => (
                        t!("libws.machines.source.project.badge"),
                        t!("libws.machines.source.project.tip"),
                    ),
                };
                ui.label(egui::RichText::new(badge).color(muted))
                    .on_hover_text(tip);
            },
            |ui| {
                let kind = text("kind");
                ui.label(
                    egui::RichText::new(format!(
                        "{} \u{b7} {}",
                        kind.replace('_', " "),
                        t!("libws.machines.work_area",
                            x = c.pointer("/work_area/x").and_then(Value::as_f64).unwrap_or(0.0),
                            y = c.pointer("/work_area/y").and_then(Value::as_f64).unwrap_or(0.0))
                    ))
                    .color(muted),
                );
                if in_project {
                    ui.label(t!("libws.machines.in_project"));
                }
                match copy_state.as_str() {
                    "library_newer" => {
                        ui.label(t!("libws.machines.behind"));
                    }
                    "edited" => {
                        ui.label(t!("libws.machines.edited"));
                    }
                    _ => {}
                }
                if hidden {
                    ui.label(t!("libws.machines.hidden_note"));
                }
                ui.horizontal_wrapped(|ui| {
                    if secondary_button(
                        ui,
                        None,
                        &t!("libws.machines.use"),
                        &if in_project {
                            t!("libws.machines.use.done_tip")
                        } else {
                            t!("libws.machines.use.tip")
                        },
                        Size::Small,
                        !in_project && source != "project",
                    )
                    .clicked()
                    {
                        action = Some(("machine.install", json!({"id": id})));
                    }
                    if secondary_button(
                        ui,
                        None,
                        &t!("libws.machines.duplicate"),
                        &t!("libws.machines.duplicate.tip"),
                        Size::Small,
                        true,
                    )
                    .clicked()
                    {
                        action = Some((
                            "machine.duplicate",
                            json!({"id": id, "keep_in_my_machines": source != "project", "add_to_project": true}),
                        ));
                    }
                    if copy_state == "library_newer"
                        && secondary_button(
                            ui,
                            None,
                            &t!("libws.machines.update"),
                            &t!("libws.machines.update.tip"),
                            Size::Small,
                            true,
                        )
                        .clicked()
                    {
                        action = Some(("machine.update_from_library", json!({"id": id})));
                    }
                    if source == "built_in" {
                        let (label, tip) = if hidden {
                            (t!("libws.machines.show"), t!("libws.machines.show.tip"))
                        } else {
                            (t!("libws.machines.hide"), t!("libws.machines.hide.tip"))
                        };
                        if secondary_button(ui, None, &label, &tip, Size::Small, true).clicked() {
                            action =
                                Some(("machine.library_hide", json!({"id": id, "hidden": !hidden})));
                        }
                    }
                    if source == "project"
                        && secondary_button(
                            ui,
                            None,
                            &t!("libws.machines.save"),
                            &t!("libws.machines.save.tip"),
                            Size::Small,
                            true,
                        )
                        .clicked()
                    {
                        action = Some(("machine.library_save", json!({"id": id})));
                    }
                    if source == "mine"
                        && secondary_button(
                            ui,
                            None,
                            &t!("libws.machines.remove"),
                            &t!("libws.machines.remove.tip"),
                            Size::Small,
                            true,
                        )
                        .clicked()
                    {
                        action = Some(("machine.library_remove", json!({"id": id})));
                    }
                });
            },
        );
        if let Some((command, params)) = action {
            self.machine_lib_run(command, params);
        }
    }

    /// The Add machine dialog: start from a library machine or a blank kind, name it,
    /// choose where it is kept, and create it.
    fn machine_lib_add_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.libws.machines.add.take() else {
            return;
        };
        let pickable: Vec<(String, String)> = self
            .libws
            .machines
            .cards
            .iter()
            .filter(|c| c.get("hidden").and_then(Value::as_bool) != Some(true))
            .filter_map(|c| {
                Some((
                    c.get("id")?.as_str()?.to_owned(),
                    c.get("name")?.as_str()?.to_owned(),
                ))
            })
            .collect();
        let title = t!("libws.machines.add");
        let cancel = t!("libws.machines.add.cancel");
        let cancel_tip = t!("libws.machines.add.cancel.tip");
        let create = t!("libws.machines.add.create");
        let create_tip = t!("libws.machines.add.create.tip");
        let dialog = Dialog {
            id: "add-machine",
            title: &title,
            width: 460.0,
            tabs: &[],
            note: None,
            cancel: Some((&cancel, &cancel_tip)),
            primary: (&create, &create_tip),
        };
        let mut tab = 0;
        let outcome = dialog::show(ctx, &dialog, &mut tab, |ui| {
            let library = t!("libws.machines.add.from_library");
            let blank = t!("libws.machines.add.from_blank");
            let (library_tip, blank_tip) = (
                t!("libws.machines.add.from_library.tip"),
                t!("libws.machines.add.from_blank.tip"),
            );
            let items = [
                Segment::text(&library, &library_tip),
                Segment::text(&blank, &blank_tip),
            ];
            segmented(ui, "add-machine-from", &mut form.from, &items);
            ui.add_space(8.0);
            if form.from == 0 {
                let names: Vec<&str> = pickable.iter().map(|(_, n)| n.as_str()).collect();
                select(
                    ui,
                    "add-machine-library",
                    &mut form.library_pick,
                    &names,
                    &t!("libws.machines.add.library_pick"),
                    &t!("libws.machines.add.library_pick.tip"),
                    360.0,
                );
            } else {
                let names: Vec<String> = KINDS.iter().map(|k| kind_name(k)).collect();
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                select(
                    ui,
                    "add-machine-kind",
                    &mut form.kind_pick,
                    &names,
                    &t!("libws.machines.add.kind_pick"),
                    &t!("libws.machines.add.kind_pick.tip"),
                    360.0,
                );
            }
            ui.add_space(8.0);
            let field = ui
                .add(
                    crate::targets::text_field(&mut form.name)
                        .hint_text(t!("libws.machines.add.name"))
                        .desired_width(360.0),
                )
                .on_hover_text(t!("libws.machines.add.name.tip"));
            let enabled = field.enabled();
            field.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::TextEdit,
                    enabled,
                    t!("libws.machines.add.name"),
                )
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                checkbox(
                    ui,
                    &mut form.to_project,
                    &t!("libws.machines.add.to_project"),
                    &t!("libws.machines.add.to_project.tip"),
                );
                ui.label(t!("libws.machines.add.to_project"));
            });
            ui.horizontal(|ui| {
                checkbox(
                    ui,
                    &mut form.keep,
                    &t!("libws.machines.add.keep"),
                    &t!("libws.machines.add.keep.tip"),
                );
                ui.label(t!("libws.machines.add.keep"));
            });
        });
        match outcome {
            Outcome::Open => self.libws.machines.add = Some(form),
            Outcome::Cancel => {}
            Outcome::Primary => {
                let name = form.name.trim().to_owned();
                let mut params = json!({
                    "keep_in_my_machines": form.keep,
                    "add_to_project": form.to_project,
                });
                if !name.is_empty() {
                    params["name"] = json!(name);
                }
                if form.from == 0 {
                    let Some((id, _)) = pickable.get(form.library_pick.min(pickable.len().saturating_sub(1))) else {
                        return;
                    };
                    params["based_on"] = json!(id);
                } else {
                    params["kind"] = json!(KINDS[form.kind_pick.min(KINDS.len() - 1)]);
                }
                self.machine_lib_run("machine.add", params);
            }
        }
    }
}
