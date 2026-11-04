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
//! - crates/ui-egui/src/ui_kit/rows.rs, segmented.rs, select.rs, dialog.rs
//! - crates/ui-egui/src/library_workspace/machines_view.rs (cards, toolbar, selected machine)
//!
//! # Do not
//! - Read machine files here; ask the engine.
//! - Name a machine id; the list is whatever the library holds.
use super::*;
use crate::theme::Tokens;
use crate::ui_kit::{
    checkbox::checkbox,
    dialog::{self, Dialog, Outcome},
    icons,
    rows::list_row,
    segmented::{Segment, segmented},
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

/// The kind rows of the left list, as kind filters. The first row is all kinds.
const KIND_ROWS: [&str; 6] = [
    "",
    "pick_place",
    "laser",
    "cnc_mill",
    "uv_printer",
    "reflow_oven",
];

/// The source rows of the left list, as source words of the engine.
const SOURCE_ROWS: [&str; 4] = ["built_in", "mine", "project", "shared"];

/// The words and tip of kind row `i`.
fn kind_row_words(i: usize) -> (String, String) {
    match i {
        0 => (
            t!("libws.machines.filter.all"),
            t!("libws.machines.filter.all.tip"),
        ),
        1 => (
            t!("libws.machines.kind.pick_place"),
            t!("libws.machines.kind.pick_place.tip"),
        ),
        2 => (
            t!("libws.machines.kind.laser"),
            t!("libws.machines.kind.laser.tip"),
        ),
        3 => (
            t!("libws.machines.kind.cnc_mill"),
            t!("libws.machines.kind.cnc_mill.tip"),
        ),
        4 => (
            t!("libws.machines.kind.uv_printer"),
            t!("libws.machines.kind.uv_printer.tip"),
        ),
        _ => (
            t!("libws.machines.kind.reflow_oven"),
            t!("libws.machines.kind.reflow_oven.tip"),
        ),
    }
}

/// The words and tip of source row `i`.
fn source_row_words(i: usize) -> (String, String) {
    match i {
        0 => (
            t!("libws.machines.source.built_in"),
            t!("libws.machines.source.built_in.tip"),
        ),
        1 => (
            t!("libws.machines.source.mine"),
            t!("libws.machines.source.mine.tip"),
        ),
        2 => (
            t!("libws.machines.source.project"),
            t!("libws.machines.source.project.tip"),
        ),
        _ => (
            t!("libws.machines.source.shared"),
            t!("libws.machines.source.shared.tip"),
        ),
    }
}

/// The Add machine form while it is open.
pub(crate) struct AddForm {
    /// 0 starts from a library machine, 1 from a machine file (not available yet), 2 from a
    /// machine on the project, 3 from a blank machine of a kind.
    from: usize,
    /// The id of the machine picked to copy; the first one listed when none is picked.
    pick: Option<String>,
    /// The search box of the list of machines to copy.
    query: String,
    /// Index into [`ADD_KIND_FILTERS`].
    kind_filter: usize,
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
            pick: None,
            query: String::new(),
            kind_filter: 0,
            kind_pick: 0,
            name: String::new(),
            to_project: true,
            keep: false,
        }
    }
}

/// The kind filters of the Add machine list: all, pick and place, laser, mill.
const ADD_KIND_FILTERS: [&str; 4] = ["", "pick_place", "laser", "cnc_mill"];

/// View state of the Machines view.
#[derive(Default)]
pub(crate) struct MachineLib {
    /// Index into [`KIND_ROWS`]; 0 is all kinds.
    pub(super) kind: usize,
    /// Index into [`SOURCE_ROWS`]; `None` is every source.
    pub(super) source: Option<usize>,
    pub(super) query: String,
    pub(super) show_hidden: bool,
    /// The id of the selected machine card; the right aside shows it.
    pub(crate) selected: Option<String>,
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
    pub(super) fn machine_lib_run(&mut self, command: &str, params: Value) {
        self.run(command, params);
        self.libws.machines.stale = true;
    }

    /// True when the Library page's Machines tab is the one showing.
    pub(crate) fn library_on_machines(&mut self) -> bool {
        self.region().left_tab == "machines"
    }

    /// Open the Library page on its Machines tab.
    pub(crate) fn show_library_machines(&mut self) {
        self.set_library_left_tab("machines");
        self.libws.machines.stale = true;
    }

    /// The left body of the Machines tab: the search, the kind list, the source list and
    /// Show hidden machines, with Add machine and Import pinned at the bottom.
    pub(crate) fn library_machines_left(&mut self, ui: &mut egui::Ui) {
        self.machine_lib_refresh();
        egui::Panel::bottom("library-machines-actions")
            .frame(egui::Frame::NONE.inner_margin(theme::aside_margin()))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if primary_button(
                        ui,
                        Some(icons::PLUS),
                        &t!("libws.machines.add"),
                        &t!("libws.machines.add.tip"),
                        Size::Normal,
                        true,
                    )
                    .clicked()
                    {
                        self.libws.machines.add = Some(AddForm::default());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        secondary_button(
                            ui,
                            Some(icons::DOWNLOAD),
                            &t!("libws.machines.import"),
                            &t!("libws.machines.import.off"),
                            Size::Normal,
                            false,
                        );
                    });
                });
            });
        egui::ScrollArea::vertical()
            .id_salt("library-machines-filters")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                self.machine_lib_filters(ui);
            });
    }

    /// The centre of the Library tab when it shows machines.
    pub(crate) fn library_machines_page(&mut self, ui: &mut egui::Ui) {
        self.machine_lib_refresh();
        let frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(
            theme::SPACE[4] as i8,
            theme::SPACE[3] as i8,
        ));
        egui::ScrollArea::vertical()
            .id_salt("library-machines-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                frame.show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = theme::SPACE[3];
                    // Cards are clickable; a selectable label would take the click from its card.
                    ui.style_mut().interaction.selectable_labels = false;
                    self.machine_lib_page(ui);
                });
            });
        self.machine_lib_add_dialog(ui.ctx());
    }

    /// Count of cards a row would list, ignoring the other filters.
    fn machine_lib_count(&self, kind: Option<&str>, source: Option<&str>) -> usize {
        self.libws
            .machines
            .cards
            .iter()
            .filter(|c| self.libws.machines.show_hidden || !card_flag(c, "hidden"))
            .filter(|c| kind.is_none_or(|k| kind_matches(k, card_text(c, "kind"))))
            .filter(|c| source.is_none_or(|s| card_text(c, "source") == s))
            .count()
    }

    fn machine_lib_filters(&mut self, ui: &mut egui::Ui) {
        // Rows are clickable; a selectable label would take the click from its row.
        ui.style_mut().interaction.selectable_labels = false;
        let t = theme::tokens(ui.ctx());
        let pad = egui::Margin::symmetric(theme::SPACE[2] as i8, theme::SPACE[1] as i8);
        egui::Frame::NONE.inner_margin(pad).show(ui, |ui| {
            let search = ui
                .add(
                    crate::targets::text_field(&mut self.libws.machines.query)
                        .hint_text(t!("libws.machines.search"))
                        .desired_width(f32::INFINITY),
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
        });
        let caption = |ui: &mut egui::Ui, words: String| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(
                    theme::SPACE[2] as i8,
                    theme::SPACE[1] as i8,
                ))
                .show(ui, |ui| {
                    ui.label(fonts::section_caption(words).color(t.muted));
                });
        };
        caption(ui, t!("libws.machines.caption.kind"));
        for (i, kind) in KIND_ROWS.iter().enumerate() {
            let count = self.machine_lib_count((!kind.is_empty()).then_some(*kind), None);
            let (label, tip) = kind_row_words(i);
            let on = self.libws.machines.kind == i;
            ui.push_id(("machine-kind-row", i), |ui| {
                if machine_lib_row(ui, on, &label, count, &tip).clicked() {
                    self.libws.machines.kind = i;
                }
            });
        }
        ui.add_space(theme::SPACE[2]);
        caption(ui, t!("libws.machines.caption.source"));
        for (i, source) in SOURCE_ROWS.iter().enumerate() {
            let count = self.machine_lib_count(None, Some(source));
            let (label, tip) = source_row_words(i);
            let on = self.libws.machines.source == Some(i);
            ui.push_id(("machine-source-row", i), |ui| {
                if machine_lib_row(ui, on, &label, count, &tip).clicked() {
                    self.libws.machines.source = if on { None } else { Some(i) };
                }
            });
        }
        ui.add_space(theme::SPACE[2]);
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(
                theme::SPACE[2] as i8,
                theme::SPACE[1] as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    checkbox(
                        ui,
                        &mut self.libws.machines.show_hidden,
                        &t!("libws.machines.show_hidden"),
                        &t!("libws.machines.show_hidden.tip"),
                    );
                    ui.label(t!("libws.machines.show_hidden"));
                });
            });
    }

    /// True when `card` passes the kind, source, search and hidden filters.
    pub(super) fn machine_lib_shows(&self, card: &Value) -> bool {
        let m = &self.libws.machines;
        let kind = card_text(card, "kind");
        let want_kind = KIND_ROWS.get(m.kind).copied().unwrap_or("");
        let kind_ok = want_kind.is_empty() || kind_matches(want_kind, kind);
        let source_ok = m
            .source
            .and_then(|i| SOURCE_ROWS.get(i))
            .is_none_or(|s| card_text(card, "source") == *s);
        let query = m.query.trim().to_lowercase();
        let query_ok = query.is_empty()
            || ["name", "vendor", "model", "id"]
                .iter()
                .any(|k| card_text(card, k).to_lowercase().contains(&query));
        kind_ok && source_ok && query_ok && (m.show_hidden || !card_flag(card, "hidden"))
    }

    /// The machines the Add machine list can copy from, as (id, name, note, source word).
    fn machine_lib_add_choices(&self, form: &AddForm) -> Vec<(String, String, String, String)> {
        let query = form.query.trim().to_lowercase();
        let want = ADD_KIND_FILTERS
            .get(form.kind_filter)
            .copied()
            .unwrap_or("");
        self.libws
            .machines
            .cards
            .iter()
            .filter(|c| !card_flag(c, "hidden"))
            .filter(|c| {
                form.from != 2 || card_flag(c, "in_project") || card_text(c, "source") == "project"
            })
            .filter(|c| want.is_empty() || kind_matches(want, card_text(c, "kind")))
            .filter(|c| {
                query.is_empty()
                    || ["name", "vendor", "model", "id"]
                        .iter()
                        .any(|k| card_text(c, k).to_lowercase().contains(&query))
            })
            .map(|c| {
                let note = [card_text(c, "vendor"), card_text(c, "model")]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" \u{b7} ");
                (
                    card_text(c, "id").to_owned(),
                    card_text(c, "name").to_owned(),
                    note,
                    card_text(c, "source").to_owned(),
                )
            })
            .collect()
    }

    /// The Add machine dialog: start from a library machine, a machine on the project or a
    /// blank kind, name it, choose where it is kept, and create it.
    fn machine_lib_add_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.libws.machines.add.take() else {
            return;
        };
        let t = theme::tokens(ctx);
        let choices = self.machine_lib_add_choices(&form);
        let picked = form
            .pick
            .as_ref()
            .filter(|id| choices.iter().any(|c| &c.0 == *id))
            .cloned()
            .or_else(|| choices.first().map(|c| c.0.clone()));
        let title = t!("libws.machines.add");
        let cancel = t!("libws.machines.add.cancel");
        let cancel_tip = t!("libws.machines.add.cancel.tip");
        let create = t!("libws.machines.add.create");
        let create_tip = t!("libws.machines.add.create.tip");
        let note = t!("libws.machines.add.note");
        let dialog = Dialog {
            id: "add-machine",
            title: &title,
            width: theme::SPACE[5] * 38.0,
            tabs: &[],
            note: Some(&note),
            cancel: Some((&cancel, &cancel_tip)),
            primary: (&create, &create_tip),
        };
        let mut tab = 0;
        let outcome = dialog::show(ctx, &dialog, &mut tab, |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            let gap = theme::SPACE[4];
            egui::Frame::NONE
                .inner_margin(egui::Margin::same(theme::SPACE[3] as i8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(theme::SPACE[2], theme::SPACE[1]);
                    let side = ((ui.available_width() - gap) / 2.0).max(0.0);
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(side, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| self.machine_lib_add_options(ui, &mut form, &t),
                        );
                        ui.add_space(gap);
                        ui.allocate_ui_with_layout(
                            egui::vec2(side, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                self.machine_lib_add_choose(ui, &mut form, &choices, &picked, &t);
                            },
                        );
                    });
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
                if form.from == 3 {
                    params["kind"] = json!(KINDS[form.kind_pick.min(KINDS.len() - 1)]);
                } else {
                    let Some(id) = picked else {
                        self.libws.machines.add = Some(form);
                        return;
                    };
                    params["based_on"] = json!(id);
                }
                self.machine_lib_run("machine.add", params);
            }
        }
    }

    /// The left half of the Add machine window: where the new machine starts from.
    fn machine_lib_add_options(&mut self, ui: &mut egui::Ui, form: &mut AddForm, t: &Tokens) {
        ui.label(fonts::section_caption(t!("libws.machines.add.start_from")).color(t.muted));
        let options = [
            (
                0,
                icons::COPY,
                t!("libws.machines.add.opt.library"),
                t!("libws.machines.add.opt.library.note"),
                true,
            ),
            (
                1,
                icons::DOWNLOAD,
                t!("libws.machines.add.opt.file"),
                t!("libws.machines.add.opt.file.note"),
                false,
            ),
            (
                2,
                icons::LAYERS,
                t!("libws.machines.add.opt.project"),
                t!("libws.machines.add.opt.project.note"),
                true,
            ),
            (
                3,
                icons::PLUS,
                t!("libws.machines.add.opt.blank"),
                t!("libws.machines.add.opt.blank.note"),
                true,
            ),
        ];
        for (index, icon, title, note, enabled) in options {
            let on = form.from == index;
            let scope = ui.scope_builder(
                egui::UiBuilder::new()
                    .id_salt(("add-machine-option", index))
                    .sense(if enabled {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    }),
                |ui| {
                    let (fill, stroke) = if on {
                        (
                            t.copper_tint,
                            egui::Stroke::new(theme::FOCUS_STROKE, t.copper),
                        )
                    } else {
                        (t.panel, egui::Stroke::new(theme::BORDER, t.line))
                    };
                    egui::Frame::new()
                        .fill(fill)
                        .stroke(stroke)
                        .corner_radius(egui::CornerRadius::same(theme::RADIUS_CARD))
                        .inner_margin(egui::Margin::same(theme::SPACE[2] as i8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = theme::SPACE[0];
                            let ink = if enabled { t.text } else { t.muted };
                            ui.vertical_centered(|ui| {
                                let (r, _) = ui.allocate_exact_size(
                                    egui::Vec2::splat(icons::ICON_SIZE),
                                    egui::Sense::hover(),
                                );
                                (icon.0)(ui.painter(), r, if enabled { t.copper } else { t.muted });
                                ui.label(fonts::strong(&title).color(ink));
                                ui.label(fonts::caption(&note).color(t.muted));
                            });
                        });
                },
            );
            let response = scope.response;
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, enabled, on, &title)
            });
            let tip = if enabled {
                note.clone()
            } else {
                t!("libws.machines.add.opt.file.off")
            };
            if response.on_hover_text(tip).clicked() && enabled {
                form.from = index;
            }
        }
    }

    /// The right half of the Add machine window: the machine to copy or the kind, the name and
    /// where the new machine is kept.
    fn machine_lib_add_choose(
        &mut self,
        ui: &mut egui::Ui,
        form: &mut AddForm,
        choices: &[(String, String, String, String)],
        picked: &Option<String>,
        t: &Tokens,
    ) {
        ui.label(fonts::section_caption(t!("libws.machines.add.choose")).color(t.muted));
        if form.from == 3 {
            for (i, kind) in KINDS.iter().enumerate() {
                let label = kind_name(kind);
                ui.push_id(("add-machine-kind", i), |ui| {
                    if machine_lib_row_plain(ui, form.kind_pick == i, &label).clicked() {
                        form.kind_pick = i;
                    }
                });
            }
        } else {
            let search = ui
                .add(
                    crate::targets::text_field(&mut form.query)
                        .hint_text(t!("libws.machines.search"))
                        .desired_width(f32::INFINITY),
                )
                .on_hover_text(t!("libws.machines.search.tip"));
            let enabled = search.enabled();
            search.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::TextEdit,
                    enabled,
                    t!("libws.machines.add.search"),
                )
            });
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
            segmented(ui, "add-machine-kind-filter", &mut form.kind_filter, &items);
            if choices.is_empty() {
                ui.label(
                    fonts::text(fonts::Role::Body, t!("libws.machines.add.none")).color(t.muted),
                );
            }
            egui::ScrollArea::vertical()
                .id_salt("add-machine-choices")
                .max_height(theme::SPACE[5] * 8.0)
                .min_scrolled_height(theme::SPACE[5] * 8.0)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (id, name, note, source) in choices {
                        let on = picked.as_ref() == Some(id);
                        let tip = t!("libws.machines.add.pick.tip");
                        ui.push_id(("add-machine-choice", id), |ui| {
                            let row = list_row(ui, on, name, &tip, |ui| {
                                ui.label(fonts::strong(name).color(t.text));
                                ui.label(fonts::caption(note).color(t.muted));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(fonts::mono(source_word(source)).color(t.muted));
                                    },
                                );
                            });
                            if row.clicked() {
                                form.pick = Some(id.clone());
                            }
                        });
                    }
                });
        }
        ui.add_space(theme::SPACE[2]);
        ui.label(fonts::section_caption(t!("libws.machines.add.name_caption")).color(t.muted));
        let field = ui
            .add(
                crate::targets::text_field(&mut form.name)
                    .hint_text(t!("libws.machines.add.name"))
                    .desired_width(theme::SPACE[5] * 10.0),
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
        ui.horizontal(|ui| {
            checkbox(
                ui,
                &mut form.keep,
                &t!("libws.machines.add.keep"),
                &t!("libws.machines.add.keep.tip"),
            );
            ui.label(t!("libws.machines.add.keep"));
        });
        ui.horizontal(|ui| {
            checkbox(
                ui,
                &mut form.to_project,
                &t!("libws.machines.add.to_project"),
                &t!("libws.machines.add.to_project.tip"),
            );
            ui.label(t!("libws.machines.add.to_project"));
        });
    }
}

/// The plain words of a source word, for the Add machine list.
fn source_word(source: &str) -> String {
    match source {
        "built_in" => t!("libws.machines.source.built_in.badge"),
        "mine" => t!("libws.machines.source.mine.badge"),
        _ => t!("libws.machines.source.project.badge"),
    }
}

/// A one line list row with only a name, for the kinds of a blank machine.
fn machine_lib_row_plain(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let t = theme::tokens(ui.ctx());
    list_row(
        ui,
        selected,
        label,
        &t!("libws.machines.add.kind_pick.tip"),
        |ui| {
            ui.label(fonts::text(fonts::Role::Body, label).color(t.text));
        },
    )
}

/// A text field of a library card, empty when it is missing.
pub(super) fn card_text<'a>(card: &'a Value, key: &str) -> &'a str {
    card.get(key).and_then(Value::as_str).unwrap_or("")
}

/// A true or false field of a library card.
pub(super) fn card_flag(card: &Value, key: &str) -> bool {
    card.get(key).and_then(Value::as_bool) == Some(true)
}

/// True when a machine of `kind` belongs to the kind row `want` (lasers come in two kinds).
fn kind_matches(want: &str, kind: &str) -> bool {
    match want {
        "laser" => kind.starts_with("laser"),
        other => kind == other,
    }
}

/// One row of the kind or source list: the name on the left and its count on the right.
fn machine_lib_row(
    ui: &mut egui::Ui,
    selected: bool,
    label: &str,
    count: usize,
    tip: &str,
) -> egui::Response {
    let t = theme::tokens(ui.ctx());
    list_row(ui, selected, label, tip, |ui| {
        let name = if selected {
            fonts::strong(label)
        } else {
            fonts::text(fonts::Role::Body, label)
        };
        ui.label(name.color(t.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(fonts::mono(count.to_string()).color(t.muted));
        });
    })
}
