//! The fixed rows of a schematic page (Symbols, Power ports, Drawing, Title
//! block): what selecting one shows in Properties, and what their menus do.
//!
//! Context: `layers_tree.rs` builds the rows and returns actions; the app calls
//! the methods here to apply them. Every edit goes through a typed engine
//! command (`schematic.set_page_setting`, `label.place`) so it is undoable and
//! the assistant can do the same. Visibility and locks are view state kept in
//! `Preferences::hidden_layers` and `TreeView::locked`, named by
//! `layers_tree::page_row_name`.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs (`schematic.set_page_setting`, `label.place`)
//! - crates/ui-egui/src/layers_tree.rs (`sheet_row_of`, `page_row_name`)
//! - crates/ui-egui/src/ui_kit/form_row.rs, section.rs, select.rs, number_field.rs
//!
//! # Do not
//! - Offer a way to delete a row; the four rows are fixed categories of a page.
//! - Keep a copy of a setting here; read it from `Project::page_settings_of`.

use super::*;
use crate::fonts::{self, Role};
use crate::theme;
use crate::ui_kit::{
    button::{Size, secondary_button},
    form_row::{FormWidth, form_row, toggle_row},
    number_field::{NumberSpec, number_field},
    section::section,
    select::select,
};

const ROW: FormWidth = FormWidth::Aside;
const SIZES: [&str; 8] = ["A7", "A6", "A5", "A4", "A3", "A2", "A1", "A0"];

impl App {
    /// The objects the fixed row `row` of page `page` stands for.
    ///
    /// # Rules
    /// - Symbols are the parts on the page, power ports the power labels placed
    ///   for it, drawing the schematic graphics (they live on the Main page).
    /// - The title block is not an object, so it has none.
    pub(crate) fn sheet_row_ids(&self, page: Id, row: &str) -> Vec<Id> {
        let p = &self.project;
        match row {
            "symbols" => p
                .components
                .iter()
                .filter(|c| c.sheet == page)
                .map(|c| c.id)
                .collect(),
            "power" => p
                .labels
                .iter()
                .filter(|l| l.kind == "power" && p.schematic_label_page(l) == page)
                .map(|l| l.id)
                .collect(),
            "drawing" if page == 0 => p
                .graphics
                .iter()
                .filter(|g| g.layer == "schematic")
                .map(|g| g.id)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Show page `page` and open its fixed row `row` in Properties.
    ///
    /// # Do not
    /// - Call `show_page`; it would replace the row with the page card.
    pub(crate) fn select_sheet_row(&mut self, page: Id, row: &str) {
        self.active_page = page;
        self.select(None);
        self.tree.selected_key = Some(format!("{}/layer:{row}", Self::page_row_key(page)));
    }

    /// Select every object of a fixed row, unless its lock keeps the drawing from it.
    pub(crate) fn select_all_in_row(&mut self, page: Id, row: &str) {
        if self
            .tree
            .locked
            .contains(&layers_tree::page_row_name(page, row))
        {
            self.message = t!("tree.menu.select_all.locked");
            return;
        }
        let ids = self.sheet_row_ids(page, row);
        self.active_page = page;
        self.set_selection(ids);
        self.tree.selected_key = None;
    }

    /// Show or hide a fixed row on the canvas of its page.
    pub(crate) fn set_page_row_shown(&mut self, page: Id, row: &str, on: bool) {
        let name = layers_tree::page_row_name(page, row);
        let hidden = &mut self.preferences.hidden_layers;
        hidden.retain(|h| *h != name);
        if !on {
            hidden.push(name);
        }
    }

    /// Store one page setting through `schematic.set_page_setting`.
    ///
    /// # Rules
    /// - An unchanged value is a no-op, not an error in the status line.
    pub(crate) fn set_page_setting(&mut self, page: Id, field: &str, value: &str) -> bool {
        let p = self.project.page_settings_of(page);
        let same = match field {
            "title" => p.title == value,
            "revision" => p.revision == value,
            "date" => p.date == value,
            "size" => p.size == value,
            "company" => p.company == value,
            "drawn_by" => p.drawn_by == value,
            "line_color" => p.line_color == value,
            "title_shown" => p.title_hidden == (value == "false"),
            _ => false,
        };
        if same {
            return false;
        }
        self.tool_call(
            "schematic.set_page_setting",
            json!({"sheet": page, "field": field, "value": value}),
        )
    }

    /// Place a power port on `page`, tied to the net the Properties picker holds
    /// (the first net that page carries when none was picked).
    pub(crate) fn add_power_port(&mut self, page: Id) {
        let nets = self.power_port_nets(page);
        let Some(&net) = nets
            .get(self.power_port_pick(page).min(nets.len().saturating_sub(1)))
            .map(|(id, _)| id)
        else {
            self.message = t!("props.sheet.power.no_nets");
            return;
        };
        let existing = self.sheet_row_ids(page, "power").len() as f64;
        let c = self.schematic.center;
        let at = Point::new(
            (c.x / 1.27).round() * 1.27 + existing * 5.08,
            (c.y / 1.27).round() * 1.27,
        );
        self.tool_call(
            "label.place",
            json!({"kind": "power", "net": net, "position": {"x": at.x, "y": at.y}}),
        );
    }

    /// The nets a power port on `page` can tie to, by name: the nets the page's
    /// parts connect to, or every net on the Main page.
    fn power_port_nets(&self, page: Id) -> Vec<(Id, String)> {
        let mut nets: Vec<(Id, String)> = self
            .project
            .nets
            .iter()
            .filter(|n| page == 0 || self.project.net_pages(n.id).contains(&page))
            .map(|n| (n.id, n.name.clone()))
            .collect();
        nets.sort_by(|a, b| a.1.cmp(&b.1));
        nets
    }

    fn power_port_pick(&self, page: Id) -> usize {
        let nets = self.power_port_nets(page);
        let remembered = self
            .power_pick
            .get(&page)
            .and_then(|id| nets.iter().position(|(n, _)| n == id));
        remembered
            .or_else(|| {
                nets.iter()
                    .position(|(_, name)| name.eq_ignore_ascii_case("gnd"))
            })
            .unwrap_or(0)
    }

    /// Properties of a fixed page row.
    ///
    /// # Rules
    /// - Title block: its six fields and Show title block. Power ports: the
    ///   ports with their nets, Select all, Add power port. Drawing: count,
    ///   defaults for new graphics, Select all. Symbols: count and Select all.
    /// - Text fields commit when they lose focus; Escape cancels.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/layers_tree.rs` `tree_row_properties` routes here from the row's key.
    pub(crate) fn sheet_row_properties(&mut self, ui: &mut egui::Ui, page: Id, row: &str) {
        let name = self.page_name(page);
        let (caption, title, tip) = match row {
            "title" => (
                t!("props.sheet.title.caption"),
                t!("props.sheet.title.section"),
                t!("props.sheet.title.section.tip"),
            ),
            "power" => (
                t!("props.sheet.power.caption"),
                t!("props.sheet.power.section"),
                t!("props.sheet.power.section.tip"),
            ),
            "drawing" => (
                t!("props.sheet.drawing.caption"),
                t!("props.sheet.drawing.section"),
                t!("props.sheet.drawing.section.tip"),
            ),
            _ => (
                t!("props.sheet.symbols.caption"),
                t!("props.sheet.symbols.section"),
                t!("props.sheet.symbols.section.tip"),
            ),
        };
        self.properties_header(ui, &name, &caption);
        let row = row.to_owned();
        section(
            ui,
            ("sheet-row", page, row.clone()),
            &title,
            &tip,
            |ui| match row.as_str() {
                "title" => self.title_block_fields(ui, page),
                "power" => self.power_port_fields(ui, page),
                "drawing" => self.drawing_fields(ui, page),
                _ => self.symbol_fields(ui, page),
            },
        );
    }

    /// One editable text field of the page settings, saved when it loses focus.
    fn page_text_field(
        &mut self,
        ui: &mut egui::Ui,
        page: Id,
        field: &str,
        label: String,
        tip: String,
    ) {
        let current = {
            let s = self.project.page_settings_of(page);
            match field {
                "title" => s.title,
                "revision" => s.revision,
                "date" => s.date,
                "company" => s.company,
                "drawn_by" => s.drawn_by,
                _ => String::new(),
            }
        };
        let draft_id = egui::Id::new(("page-setting-draft", page, field));
        let mut commit: Option<String> = None;
        form_row(ui, ROW, &label, None, None, |ui| {
            let mut text: String = ui
                .data(|d| d.get_temp(draft_id))
                .unwrap_or_else(|| current.clone());
            let response = ui
                .add(
                    crate::targets::text_field(&mut text)
                        .id(draft_id.with("field"))
                        .desired_width(f32::INFINITY),
                )
                .on_hover_text(&tip);
            crate::a11y::Named::named(response.clone(), &label);
            if response.lost_focus() {
                ui.data_mut(|d| d.remove::<String>(draft_id));
                if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    commit = Some(text);
                }
            } else {
                ui.data_mut(|d| d.insert_temp(draft_id, text));
            }
        });
        if let Some(text) = commit {
            self.set_page_setting(page, field, text.trim());
        }
    }

    fn title_block_fields(&mut self, ui: &mut egui::Ui, page: Id) {
        for (field, label, tip) in [
            (
                "title",
                t!("props.sheet.field.title"),
                t!("props.sheet.field.title.tip"),
            ),
            (
                "revision",
                t!("props.sheet.field.revision"),
                t!("props.sheet.field.revision.tip"),
            ),
            (
                "date",
                t!("props.sheet.field.date"),
                t!("props.sheet.field.date.tip"),
            ),
        ] {
            self.page_text_field(ui, page, field, label, tip);
        }
        let current = self.project.page_settings_of(page).size;
        let auto = t!("props.sheet.size.auto");
        let mut options: Vec<&str> = vec![auto.as_str()];
        options.extend(SIZES);
        let mut index = SIZES
            .iter()
            .position(|s| *s == current)
            .map_or(0, |i| i + 1);
        let before = index;
        form_row(ui, ROW, &t!("props.sheet.field.size"), None, None, |ui| {
            select(
                ui,
                ("sheet-size", page),
                &mut index,
                &options,
                &t!("props.sheet.field.size"),
                &t!("props.sheet.field.size.tip"),
                120.0,
            );
        });
        if index != before {
            let value = if index == 0 { "" } else { SIZES[index - 1] };
            self.set_page_setting(page, "size", value);
        }
        for (field, label, tip) in [
            (
                "company",
                t!("props.sheet.field.company"),
                t!("props.sheet.field.company.tip"),
            ),
            (
                "drawn_by",
                t!("props.sheet.field.drawn_by"),
                t!("props.sheet.field.drawn_by.tip"),
            ),
        ] {
            self.page_text_field(ui, page, field, label, tip);
        }
        let mut shown = !self.project.page_settings_of(page).title_hidden;
        let before = shown;
        toggle_row(
            ui,
            ROW,
            &t!("props.sheet.show"),
            &t!("props.sheet.show.tip"),
            &mut shown,
        );
        if shown != before {
            self.set_page_setting(page, "title_shown", if shown { "true" } else { "false" });
        }
    }

    /// The count line and Select all button every selectable row shares.
    fn count_and_select_all(&mut self, ui: &mut egui::Ui, page: Id, row: &str) {
        let n = self.sheet_row_ids(page, row).len();
        form_row(ui, ROW, &t!("props.sheet.count"), None, None, |ui| {
            let ink = theme::tokens(ui.ctx()).text;
            ui.label(fonts::text(Role::Mono, n.to_string()).color(ink))
                .on_hover_text(t!("props.sheet.count.tip"));
        });
        let locked = self
            .tree
            .locked
            .contains(&layers_tree::page_row_name(page, row));
        let button = secondary_button(
            ui,
            None,
            &t!("props.sheet.select_all"),
            &t!("props.sheet.select_all.tip"),
            Size::Normal,
            !locked && n > 0,
        );
        if locked {
            ui.label(fonts::caption(t!("props.sheet.locked")));
        }
        if button.clicked() {
            self.select_all_in_row(page, row);
        }
    }

    fn symbol_fields(&mut self, ui: &mut egui::Ui, page: Id) {
        self.count_and_select_all(ui, page, "symbols");
    }

    fn power_port_fields(&mut self, ui: &mut egui::Ui, page: Id) {
        let ports: Vec<(Id, String)> = self
            .sheet_row_ids(page, "power")
            .into_iter()
            .filter_map(|id| {
                let l = self.project.labels.iter().find(|l| l.id == id)?;
                Some((id, self.project.net_name(l.net).to_owned()))
            })
            .collect();
        if ports.is_empty() {
            ui.label(fonts::caption(t!("props.sheet.power.none")));
        }
        let mut pick = None;
        for (id, net) in &ports {
            form_row(ui, ROW, net, None, None, |ui| {
                if secondary_button(
                    ui,
                    None,
                    &t!("props.sheet.power.select"),
                    &t!("props.sheet.power.row.tip"),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    pick = Some(*id);
                }
            });
        }
        if let Some(id) = pick {
            self.select(Some(id));
        }
        self.count_and_select_all(ui, page, "power");
        let nets = self.power_port_nets(page);
        let names: Vec<&str> = nets.iter().map(|(_, n)| n.as_str()).collect();
        let mut index = self.power_port_pick(page);
        if !names.is_empty() {
            let before = index;
            form_row(ui, ROW, &t!("props.sheet.power.net"), None, None, |ui| {
                select(
                    ui,
                    ("power-net", page),
                    &mut index,
                    &names,
                    &t!("props.sheet.power.net"),
                    &t!("props.sheet.power.net.tip"),
                    140.0,
                );
            });
            if index != before
                && let Some((id, _)) = nets.get(index)
            {
                self.power_pick.insert(page, *id);
            }
        }
        if secondary_button(
            ui,
            None,
            &t!("props.sheet.power.add"),
            &t!("props.sheet.power.add.tip"),
            Size::Normal,
            !nets.is_empty(),
        )
        .clicked()
        {
            if let Some((id, _)) = nets.get(index) {
                self.power_pick.insert(page, *id);
            }
            self.add_power_port(page);
        }
    }

    fn drawing_fields(&mut self, ui: &mut egui::Ui, page: Id) {
        self.count_and_select_all(ui, page, "drawing");
        ui.add_space(theme::SPACE[1]);
        ui.label(fonts::caption(t!("props.sheet.drawing.defaults")));
        let settings = self.project.page_settings_of(page);
        let mut width = settings.line_width;
        let spec = NumberSpec {
            unit: "mm",
            step: 0.05,
            min: 0.05,
            max: 10.0,
            places: 2,
            width: 110.0,
        };
        let changed = form_row(
            ui,
            ROW,
            &t!("props.sheet.drawing.width"),
            None,
            None,
            |ui| {
                number_field(
                    ui,
                    ("page-line-width", page),
                    &mut width,
                    &spec,
                    &t!("props.sheet.drawing.width"),
                    &t!("props.sheet.drawing.width.tip"),
                )
                .changed()
            },
        );
        if changed {
            self.set_page_setting(page, "line_width", &format!("{width}"));
        }
        let draft_id = egui::Id::new(("page-line-colour", page));
        let mut commit: Option<String> = None;
        let mut reset = false;
        form_row(
            ui,
            ROW,
            &t!("props.sheet.drawing.colour"),
            None,
            None,
            |ui| {
                let mut text: String = ui
                    .data(|d| d.get_temp(draft_id))
                    .unwrap_or_else(|| settings.line_color.clone());
                let response = ui
                    .add(
                        crate::targets::text_field(&mut text)
                            .id(draft_id.with("field"))
                            .hint_text("#rrggbb")
                            .desired_width(88.0),
                    )
                    .on_hover_text(t!("props.sheet.drawing.colour.tip"));
                crate::a11y::Named::named(response.clone(), &t!("props.sheet.drawing.colour"));
                if response.lost_focus() {
                    ui.data_mut(|d| d.remove::<String>(draft_id));
                    if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        commit = Some(text);
                    }
                } else {
                    ui.data_mut(|d| d.insert_temp(draft_id, text));
                }
                reset = secondary_button(
                    ui,
                    None,
                    &t!("props.sheet.drawing.colour.reset"),
                    &t!("props.sheet.drawing.colour.reset.tip"),
                    Size::Normal,
                    !settings.line_color.is_empty(),
                )
                .clicked();
            },
        );
        if reset {
            ui.data_mut(|d| d.remove::<String>(draft_id));
            self.set_page_setting(page, "line_color", "");
        } else if let Some(text) = commit {
            self.set_page_setting(page, "line_color", text.trim());
        }
    }
}
