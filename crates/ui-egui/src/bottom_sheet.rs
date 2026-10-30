//! The bottom sheet: a collapsible panel under the canvas with the tabs
//! Findings, Output and Console.
//!
//! Context: it replaces the old Checks page (epic #758, tickets #769, #779).
//! Collapsed it is a 36 px tab strip; View > Checks and the status bar counts
//! open it on Findings.
//!
//! Local rules: all words come from `crates/text/strings/en.txt`; every
//! control carries a tooltip; findings are read from `App::checks` and
//! changed only through engine commands (`rules.exclude`,
//! `rules.exclude_region`).

use super::*;

/// What a row of a finding's right-click menu does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FindingAct {
    Show,
    Hide,
    Mute,
}

/// The tabs of the bottom sheet.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum SheetTab {
    #[default]
    Findings,
    Output,
    Console,
}

impl SheetTab {
    /// The region tab id this variant answers to.
    fn id(self) -> &'static str {
        match self {
            SheetTab::Findings => "findings",
            SheetTab::Output => "output",
            SheetTab::Console => "console",
        }
    }
    fn from_id(id: &str) -> Self {
        match id {
            "output" => SheetTab::Output,
            "console" => SheetTab::Console,
            _ => SheetTab::Findings,
        }
    }
}

/// Which findings the Findings tab lists.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum SeverityFilter {
    #[default]
    All,
    Errors,
    Warnings,
    Notes,
}

/// State of the bottom sheet.
#[derive(Clone, Debug, Default)]
pub(crate) struct SheetState {
    pub(crate) filter: SeverityFilter,
}

/// One finding flattened for display.
struct Row {
    key: &'static str,
    code: String,
    message: String,
    objects: Vec<Id>,
    /// 0 error, 1 warning, 2 note.
    rank: u8,
}

impl App {
    /// Height of the tab strip, and of the sheet when collapsed.
    const SHEET_STRIP: f32 = 36.0;
    /// Height of the sheet when open.
    const SHEET_OPEN: f32 = 232.0;

    /// Opens the sheet on a tab.
    ///
    /// # Rules
    /// Used by View > Checks and the status bar counts.
    pub(crate) fn open_sheet(&mut self, tab: SheetTab) {
        let region = self.region();
        region.bottom_open = true;
        region.bottom_tab = tab.id().to_owned();
    }

    fn sheet_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (key, name) in [("erc", "erc"), ("drc", "drc")] {
            let Some(list) = self.checks.get(name).and_then(Value::as_array) else {
                continue;
            };
            for f in list {
                let code = f
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("finding")
                    .to_owned();
                let rank = match self.project.rule_severity(&code, "warning").as_str() {
                    "error" => 0,
                    "info" => 2,
                    _ => 1,
                };
                rows.push(Row {
                    key,
                    code,
                    message: f
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    objects: f
                        .get("objects")
                        .and_then(Value::as_array)
                        .map(|v| v.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default(),
                    rank,
                });
            }
        }
        rows.sort_by_key(|r| r.rank);
        rows
    }

    /// Draws the sheet at the bottom of the centre area.
    ///
    /// # Rules
    /// Call before the canvas so the canvas gets the remaining height.
    ///
    /// # Do not
    /// Draw it across the asides; it sits under the canvas only.
    pub(crate) fn bottom_sheet(&mut self, ui: &mut egui::Ui) {
        let rows = self.sheet_rows();
        let count = rows.len();
        let open = self.region().bottom_open;
        let tab = SheetTab::from_id(&self.region().bottom_tab);
        let height = if open {
            Self::SHEET_OPEN
        } else {
            Self::SHEET_STRIP
        };
        egui::Panel::bottom("bottom-sheet")
            .exact_size(height)
            .resizable(false)
            .frame(Self::dock_frame(ui.style()).inner_margin(egui::Margin::symmetric(12, 4)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_min_height(Self::SHEET_STRIP - 8.0);
                    self.sheet_tab_strip(ui, count, open, tab);
                });
                if open {
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match tab {
                            SheetTab::Findings => self.sheet_findings(ui, rows),
                            SheetTab::Output => self.sheet_output(ui),
                            SheetTab::Console => self.sheet_console(ui),
                        });
                }
            });
    }

    fn sheet_tab_strip(&mut self, ui: &mut egui::Ui, count: usize, open: bool, current: SheetTab) {
        use crate::ui_kit::{
            button::{Size, icon_button, secondary_button},
            chip::BadgeKind,
            icons as kit_icons,
            tab_bar::{TabItem, TabSize, tab_bar},
        };
        let tabs = self.workspace().bottom_tabs();
        let labels: Vec<(SheetTab, String, String)> = tabs
            .iter()
            .map(|t| (SheetTab::from_id(t.id), (t.label)(), (t.tip)()))
            .collect();
        let items: Vec<TabItem<'_>> = labels
            .iter()
            .map(|(tab, label, tip)| {
                let mut item = TabItem::new(label, tip);
                if *tab == SheetTab::Findings && count > 0 {
                    item.badge = Some((count.to_string(), BadgeKind::Warning));
                }
                item
            })
            .collect();
        let mut index = labels.iter().position(|(t, ..)| *t == current).unwrap_or(0);
        // The tab bar takes the width left after the buttons at the right.
        let right_w = if open && current == SheetTab::Findings {
            300.0
        } else {
            40.0
        };
        let bar_w = (ui.available_width() - right_w).max(120.0);
        let response = ui
            .allocate_ui(egui::vec2(bar_w, TabSize::Sheet.height()), |ui| {
                tab_bar(ui, "bottom-sheet-tabs", &mut index, &items, TabSize::Sheet)
            })
            .inner;
        if response.changed()
            && let Some((tab, ..)) = labels.get(index)
        {
            self.open_sheet(*tab);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (icon, name, tip) = if open {
                (
                    kit_icons::CHEVRON_DOWN,
                    t!("sheet.collapse"),
                    t!("sheet.collapse.tip"),
                )
            } else {
                (
                    kit_icons::CHEVRON_UP,
                    t!("sheet.expand"),
                    t!("sheet.expand.tip"),
                )
            };
            if icon_button(ui, icon, &name, &tip, Size::Normal, true).clicked() {
                self.region().bottom_open = !open;
            }
            if open && current == SheetTab::Findings {
                let run = t!("sheet.run_checks");
                if secondary_button(
                    ui,
                    None,
                    &run,
                    &t!("sheet.run_checks.tip"),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    // Checks are recomputed from the session whenever its
                    // revision differs from ours, so a mismatch forces a rerun.
                    self.revision = self.revision.wrapping_add(1);
                    self.message = t!("sheet.checks_ran");
                }
                let options = [
                    SeverityFilter::All,
                    SeverityFilter::Errors,
                    SeverityFilter::Warnings,
                    SeverityFilter::Notes,
                ];
                let labels: Vec<String> = options.iter().map(|f| Self::filter_label(*f)).collect();
                let names: Vec<&str> = labels.iter().map(String::as_str).collect();
                let mut idx = options
                    .iter()
                    .position(|f| *f == self.sheet.filter)
                    .unwrap_or(0);
                crate::ui_kit::select::select(
                    ui,
                    "sheet-severity",
                    &mut idx,
                    &names,
                    &t!("sheet.filter.name"),
                    &t!("sheet.filter.tip"),
                    140.0,
                );
                if let Some(f) = options.get(idx) {
                    self.sheet.filter = *f;
                }
            }
        });
    }

    fn filter_label(f: SeverityFilter) -> String {
        match f {
            SeverityFilter::All => t!("sheet.filter_all"),
            SeverityFilter::Errors => t!("sheet.filter_errors"),
            SeverityFilter::Warnings => t!("sheet.filter_warnings"),
            SeverityFilter::Notes => t!("sheet.filter_notes"),
        }
    }

    fn sheet_findings(&mut self, ui: &mut egui::Ui, rows: Vec<Row>) {
        self.tour_panel_hints(ui);
        if rows.is_empty() {
            empty_state(ui, EMPTY_CHECKS, self.muted());
            ui.label(
                egui::RichText::new(t!("checks.checks_view.no_problems"))
                    .size(Self::BODY)
                    .color(self.accent()),
            );
        }
        let filter = self.sheet.filter;
        self.checks_view(ui);
        for (i, row) in rows.into_iter().enumerate() {
            let keep = match filter {
                SeverityFilter::All => true,
                SeverityFilter::Errors => row.rank == 0,
                SeverityFilter::Warnings => row.rank == 1,
                SeverityFilter::Notes => row.rank == 2,
            };
            if keep {
                ui.push_id(i, |ui| self.sheet_row(ui, row));
            }
        }
        ui.add_space(8.0);
        self.library_checks_section(ui);
    }

    fn sheet_row(&mut self, ui: &mut egui::Ui, row: Row) {
        // The whole row answers a right click; the buttons inside sit above it.
        let (area, area_response) = ui.allocate_exact_size(
            egui::vec2(
                ui.available_width(),
                crate::ui_kit::rows::FINDING_COMPACT_HEIGHT,
            ),
            egui::Sense::click(),
        );
        if let Some(act) =
            crate::ui_kit::menu::context(&area_response, "finding", || Self::finding_menu(&row))
        {
            match act {
                FindingAct::Show => self.show_finding(&row),
                FindingAct::Hide => self.run(
                    "rules.exclude",
                    json!({"code": row.code, "ids": row.objects.clone()}),
                ),
                FindingAct::Mute => self.mute_area(&row),
            }
        }
        let mut inner = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui = &mut inner;
        {
            let (colour, word) = match row.rank {
                0 => (self.accent(), t!("sheet.severity_error")),
                1 => (
                    Color32::from_rgb(230, 170, 60),
                    t!("sheet.severity_warning"),
                ),
                _ => (self.muted(), t!("sheet.severity_note")),
            };
            let (dot, response) =
                ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 5.0, colour);
            response.on_hover_text(format!(
                "{word} {}",
                custom_heading(&self.project, &row.code)
            ));
            if ui
                .add(
                    egui::Button::new(crate::fonts::text(
                        crate::fonts::Role::Body,
                        row.message.clone(),
                    ))
                    .frame(false),
                )
                .on_hover_text(t!("sheet.show.tip"))
                .clicked()
            {
                self.show_finding(&row);
            }
            if let Some(&first) = row.objects.first() {
                let net = self.project.net_name(first);
                if net != "unassigned" {
                    ui.label(
                        egui::RichText::new(net)
                            .size(Self::CAPTION)
                            .background_color(self.colours().icon_fill),
                    )
                    .on_hover_text(t!("sheet.net_chip.tip"));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let has = !row.objects.is_empty();
                use crate::ui_kit::button::{Size, ghost_button, secondary_button};
                if ghost_button(
                    ui,
                    None,
                    &t!("sheet.mute"),
                    &t!("sheet.mute.tip"),
                    Size::Small,
                    has,
                )
                .clicked()
                {
                    self.mute_area(&row);
                }
                if ghost_button(
                    ui,
                    None,
                    &t!("sheet.hide"),
                    &t!("sheet.hide.tip"),
                    Size::Small,
                    has,
                )
                .clicked()
                {
                    self.run(
                        "rules.exclude",
                        json!({"code": row.code, "ids": row.objects.clone()}),
                    );
                }
                if secondary_button(
                    ui,
                    None,
                    &t!("sheet.show"),
                    &t!("sheet.show.tip"),
                    Size::Small,
                    has,
                )
                .clicked()
                {
                    self.show_finding(&row);
                }
            });
        }
    }

    /// The right-click menu of a finding: show it, hide it, mute its area.
    ///
    /// # Rules
    /// - Hide and Mute need an object to act on; without one they are disabled and say why.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/menu.rs (`Menu`, `Item`).
    ///
    /// # Do not
    /// - Run the command here; `sheet_row` runs the returned [`FindingAct`].
    fn finding_menu(row: &Row) -> Vec<crate::ui_kit::menu::Entry<FindingAct>> {
        use crate::ui_kit::menu::{Item, Menu};
        use crate::ui_kit::{icons as kit, menu_icons as glyph};
        let has = !row.objects.is_empty();
        let why = t!("sheet.menu.no_objects.tip");
        Menu::new()
            .group([
                Item::new(FindingAct::Show, t!("sheet.show"), t!("sheet.show.tip"))
                    .icon(kit::EYE)
                    .enabled_if(has, why.clone())
                    .into(),
            ])
            .group([
                Item::new(FindingAct::Hide, t!("sheet.hide"), t!("sheet.hide.tip"))
                    .icon(kit::EYE_OFF)
                    .enabled_if(has, why.clone())
                    .into(),
                Item::new(FindingAct::Mute, t!("sheet.mute"), t!("sheet.mute.tip"))
                    .icon(glyph::MUTE)
                    .enabled_if(has, why)
                    .into(),
            ])
            .entries()
    }

    /// Selects the first object a finding names, on the sheet or board that
    /// the finding belongs to.
    fn show_finding(&mut self, row: &Row) {
        if let Some(id) = row.objects.first().copied() {
            self.change_tab(if row.key == "erc" {
                Tab::Schematic
            } else {
                Tab::Pcb
            });
            self.select(Some(id));
        }
    }

    /// Asks the check to leave the box around this finding's objects alone.
    fn mute_area(&mut self, row: &Row) {
        let spots: Vec<Point> = row
            .objects
            .iter()
            .filter_map(|o| self.project.object_position(*o))
            .collect();
        if spots.is_empty() {
            return;
        }
        let fold = |f: fn(f64, f64) -> f64, g: fn(&Point) -> f64| {
            spots.iter().map(g).reduce(f).unwrap_or(0.0)
        };
        let (min_x, max_x) = (fold(f64::min, |p| p.x), fold(f64::max, |p| p.x));
        let (min_y, max_y) = (fold(f64::min, |p| p.y), fold(f64::max, |p| p.y));
        let pad = 2.0;
        self.run(
            "rules.exclude_region",
            json!({
                "code": row.code,
                "x": min_x - pad,
                "y": min_y - pad,
                "width": (max_x - min_x) + pad * 2.0,
                "height": (max_y - min_y) + pad * 2.0,
            }),
        );
    }

    fn sheet_output(&mut self, ui: &mut egui::Ui) {
        let (fix, warnings, notes) = Self::finding_counts(&self.checks);
        ui.label(t!(
            "sheet.output_summary",
            errors = fix,
            warnings = warnings,
            notes = notes
        ));
        if !self.message.is_empty() {
            ui.label(egui::RichText::new(&self.message).color(self.muted()));
        }
    }

    fn sheet_console(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new(t!("sheet.console_revision", revision = self.revision))
                .color(self.muted()),
        );
        ui.label(egui::RichText::new(&self.message).color(self.muted()));
    }
}
