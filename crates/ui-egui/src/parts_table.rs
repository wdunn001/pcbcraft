//! The Parts tab of the bottom sheet: every part in one grid you can sort, filter and edit.
//!
//! Context: issue #169, first slice. One row per part with its reference, value,
//! footprint, maker and maker part number. The Reference, Value, Manufacturer and Part
//! number cells are text boxes that commit on Enter or when they lose focus.
//! Layout columns (issue #704) is a checkbox, off by default, that adds the read-only
//! Side, Rotation, Locked, X and Y columns of each placed footprint.
//! Second slice: the grid works as a bill of materials. A row can be picked (click, or
//! shift-click for a range) and one typed value filled into a chosen column in one
//! step; the grid can be saved as a spreadsheet, and Group identical merges rows that
//! the bill of materials would print as one line and adds a Quantity column.
//! Local rules: every edit is one engine command (`components.update` for reference
//! and value, `component.set_sourcing` for maker and part number), so one edit is one
//! undo step and a refused edit shows the engine's message and changes nothing. Sorting
//! and filtering only change what is shown; they never touch the document. Words come
//! from `crates/text/strings/en.txt`; the text being typed lives in egui memory.
//!
//! # Dependencies
//! - crates/ui-egui/src/bottom_sheet.rs (hosts the tab)
//! - crates/ui-egui/src/ui_kit/button.rs (header buttons)
//! - crates/ui-egui/src/parts_table_layout.rs (`layout_cells` for the Layout columns)
//! - crates/engine/src/lib.rs (`components.update`, `component.set_sourcing`, `components.table.export`)
//! - crates/io/src/bom_sheet.rs (`bom_sheet_rows` gives the grouping the bill uses)
//!
//! # Do not
//! - Write the document directly; go through `App::run`.
//! - Keep the edited text on `App`.
use super::*;
use crate::a11y::Named;
use crate::parts_table_layout::{LayoutCells, layout_cells};
use crate::parts_table_variant;
use crate::ui_kit::{
    button::{Size, ghost_button, secondary_button},
    icons as kit_icons,
};

/// The columns of the grid, left to right.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PartColumn {
    Reference,
    Value,
    Footprint,
    Manufacturer,
    PartNumber,
}

impl PartColumn {
    /// Every column in display order.
    pub(crate) const ALL: [PartColumn; 5] = [
        PartColumn::Reference,
        PartColumn::Value,
        PartColumn::Footprint,
        PartColumn::Manufacturer,
        PartColumn::PartNumber,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    fn label(self) -> String {
        match self {
            PartColumn::Reference => t!("parts_table.col_reference"),
            PartColumn::Value => t!("parts_table.col_value"),
            PartColumn::Footprint => t!("parts_table.col_footprint"),
            PartColumn::Manufacturer => t!("parts_table.col_manufacturer"),
            PartColumn::PartNumber => t!("parts_table.col_part_number"),
        }
    }

    fn editable(self) -> bool {
        self != PartColumn::Footprint
    }
}

/// The columns a fill can write, in the order their buttons appear.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum FillColumn {
    #[default]
    Value,
    Manufacturer,
    PartNumber,
}

impl FillColumn {
    const ALL: [FillColumn; 3] = [
        FillColumn::Value,
        FillColumn::Manufacturer,
        FillColumn::PartNumber,
    ];

    fn column(self) -> PartColumn {
        match self {
            FillColumn::Value => PartColumn::Value,
            FillColumn::Manufacturer => PartColumn::Manufacturer,
            FillColumn::PartNumber => PartColumn::PartNumber,
        }
    }

    /// The `components.update` key that carries this column.
    fn key(self) -> &'static str {
        match self {
            FillColumn::Value => "value",
            FillColumn::Manufacturer => "manufacturer",
            FillColumn::PartNumber => "mpn",
        }
    }
}

/// What the grid shows: the sorted column, one filter text per column, the picked
/// rows, the fill box, the save path, whether identical parts are merged and whether
/// the Layout columns are on.
#[derive(Clone, Debug, Default)]
pub(crate) struct PartsTableState {
    sort: Option<(PartColumn, bool)>,
    filters: [String; 5],
    selected: std::collections::BTreeSet<Id>,
    anchor: Option<Id>,
    fill_column: FillColumn,
    fill_text: String,
    export_path: String,
    grouped: bool,
    layout: bool,
    variant: Option<String>,
}

/// One part as the grid shows it. `quantity` is 0 for a single part and the number of
/// merged parts for a grouped row.
#[derive(Clone, Debug)]
struct PartRow {
    id: Id,
    cells: [String; 5],
    quantity: usize,
}

/// Width of an editable cell and of a filter box. Kept narrow so the Layout columns
/// still fit on screen beside the five part columns without scrolling sideways.
const CELL_WIDTH: f32 = 96.0;

/// How many Layout columns there are: Side, Rotation, Locked, X and Y.
const LAYOUT_COLUMNS: usize = 5;

/// The headings of the Layout columns, left to right.
fn layout_labels() -> [String; 5] {
    [
        t!("parts_table.col_side"),
        t!("parts_table.col_rotation"),
        t!("parts_table.col_locked"),
        t!("parts_table.col_x"),
        t!("parts_table.col_y"),
    ]
}

/// The five Layout cells of one row, or a note and blanks when the part has no
/// placed footprint.
fn layout_texts(cells: Option<LayoutCells>) -> [String; 5] {
    match cells {
        Some(c) => [
            c.side,
            c.rotation,
            if c.locked {
                t!("parts_table.locked_yes")
            } else {
                t!("parts_table.locked_no")
            },
            c.x,
            c.y,
        ],
        None => [
            t!("parts_table.not_placed"),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ],
    }
}

/// Compare two references so that R2 comes before R10.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    fn split(s: &str) -> (&str, u64, &str) {
        let digits_at = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
        let (prefix, rest) = s.split_at(digits_at);
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let (number, tail) = rest.split_at(end);
        (prefix, number.parse().unwrap_or(0), tail)
    }
    let (pa, na, ta) = split(a);
    let (pb, nb, tb) = split(b);
    pa.cmp(pb).then(na.cmp(&nb)).then(ta.cmp(tb))
}

/// The rows that pass the filters, in the chosen order.
fn visible_rows(rows: Vec<PartRow>, state: &PartsTableState) -> Vec<PartRow> {
    let mut rows: Vec<PartRow> = rows
        .into_iter()
        .filter(|row| {
            state.filters.iter().enumerate().all(|(i, needle)| {
                let needle = needle.trim().to_lowercase();
                needle.is_empty() || row.cells[i].to_lowercase().contains(&needle)
            })
        })
        .collect();
    let (column, descending) = state.sort.unwrap_or((PartColumn::Reference, false));
    let i = column.index();
    rows.sort_by(|a, b| {
        let order = if column == PartColumn::Reference {
            natural_cmp(&a.cells[i], &b.cells[i])
        } else {
            a.cells[i].to_lowercase().cmp(&b.cells[i].to_lowercase())
        };
        order.then_with(|| natural_cmp(&a.cells[0], &b.cells[0]))
    });
    if descending {
        rows.reverse();
    }
    rows
}

impl PartsTableState {
    /// True when the Layout columns are on. Stories read it.
    #[cfg(test)]
    pub(crate) fn layout_columns_on(&self) -> bool {
        self.layout
    }

    /// True when the grid is sorted by Value, largest first. Stories read it.
    #[cfg(test)]
    pub(crate) fn is_sorted_descending_by_value(&self) -> bool {
        self.sort == Some((PartColumn::Value, true))
    }
}

impl App {
    fn part_rows(&self) -> Vec<PartRow> {
        self.project
            .components
            .iter()
            .map(|c| {
                let footprint = match &c.library_footprint {
                    Some(f) => f.definition.name.clone(),
                    None => t!("parts_table.no_footprint"),
                };
                PartRow {
                    id: c.id,
                    cells: [
                        c.reference.clone(),
                        c.value.clone(),
                        footprint,
                        c.sourcing.manufacturer.clone(),
                        c.sourcing.mpn.clone(),
                    ],
                    quantity: 0,
                }
            })
            .collect()
    }

    /// One text cell. Returns the trimmed text once the person finished editing it and it
    /// differs from the document.
    fn part_cell(
        ui: &mut egui::Ui,
        row: &PartRow,
        column: PartColumn,
        tip: &str,
    ) -> Option<String> {
        let current = &row.cells[column.index()];
        let key = ui.id().with(("parts-draft", row.id, column.index()));
        let (mut text, seen): (String, String) = ui
            .data_mut(|d| d.get_temp(key))
            .unwrap_or_else(|| (current.clone(), current.clone()));
        if seen != *current {
            text = current.clone();
        }
        let name = t!(
            "parts_table.cell_name",
            column = column.label(),
            part = row.cells[0].clone()
        );
        let r = ui
            .add(crate::targets::text_field(&mut text).desired_width(CELL_WIDTH))
            .named(&name)
            .on_hover_text(tip);
        let done =
            (r.lost_focus() && text.trim() != current.as_str()).then(|| text.trim().to_owned());
        ui.data_mut(|d| d.insert_temp(key, (text, current.clone())));
        done
    }

    /// Merge the shown rows the way the bill of materials does.
    ///
    /// Why: a board with forty identical resistors should read as one line with a
    /// quantity, so the grid can be used as the bill itself.
    ///
    /// # Rules
    /// - The merge key is the one `pcbcraft_io::bom_sheet_rows` uses (value, kind,
    ///   footprint, maker and part number), so grid and bill agree.
    /// - Merged rows keep the order in which their first part appeared in `rows`.
    ///
    /// # Dependencies
    /// - crates/io/src/bom_sheet.rs (`bom_sheet_rows` makes the groups)
    ///
    /// # Do not
    /// - Re-implement the grouping key here.
    fn grouped_rows(&self, rows: &[PartRow]) -> Vec<PartRow> {
        let params = pcbcraft_io::BomSheetParams {
            fields: vec!["Manufacturer".into(), "MPN".into()],
            variant: None,
            ids: rows.iter().map(|r| r.id).collect(),
            group: true,
        };
        let Ok(groups) = pcbcraft_io::bom_sheet_rows(&self.project, &params) else {
            return rows.to_vec();
        };
        let position = |reference: &str| rows.iter().position(|r| r.cells[0] == reference);
        let mut merged: Vec<(usize, PartRow)> = Vec::new();
        for group in groups {
            let Some(first) = group.references.iter().filter_map(|r| position(r)).min() else {
                continue;
            };
            let mut cells = rows[first].cells.clone();
            cells[0] = group.references.join(", ");
            merged.push((
                first,
                PartRow {
                    id: rows[first].id,
                    cells,
                    quantity: group.references.len(),
                },
            ));
        }
        merged.sort_by_key(|(first, _)| *first);
        merged.into_iter().map(|(_, row)| row).collect()
    }

    /// The spreadsheet text for what the grid shows.
    ///
    /// Why: the file must read like the grid, in the same columns, order and filter.
    ///
    /// # Rules
    /// - Plain mode asks the engine for the parts table and takes its footprint text;
    ///   rows the engine does not list are left out.
    /// - Grouped mode writes the merged rows and a Quantity column.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (`components.table.export`)
    ///
    /// # Do not
    /// - Write the file here; the caller does.
    fn parts_csv(&mut self, shown: &[PartRow], grouped: bool) -> Result<String, String> {
        let mut footprints: std::collections::BTreeMap<String, String> = Default::default();
        if !grouped {
            let out = self.run_and_keep("components.table.export", json!({"mode": "parts"}));
            let csv = out
                .get("csv")
                .and_then(Value::as_str)
                .ok_or_else(|| self.message.clone())?;
            for line in parse_csv(csv).into_iter().skip(1) {
                if let [reference, _, footprint, ..] = line.as_slice() {
                    footprints.insert(reference.clone(), footprint.clone());
                }
            }
        }
        let mut headers: Vec<String> = PartColumn::ALL.iter().map(|c| c.label()).collect();
        if grouped {
            headers.insert(0, t!("parts_table.col_quantity"));
        }
        let mut text = String::new();
        let line = |cells: Vec<String>| {
            let mut out = cells
                .iter()
                .map(|c| csv_cell(c))
                .collect::<Vec<_>>()
                .join(",");
            out.push('\n');
            out
        };
        text.push_str(&line(headers));
        for row in shown {
            let mut cells = row.cells.to_vec();
            if grouped {
                cells.insert(0, row.quantity.to_string());
            } else {
                match footprints.get(&row.cells[0]) {
                    Some(footprint) => cells[2] = footprint.clone(),
                    None => continue,
                }
            }
            text.push_str(&line(cells));
        }
        Ok(text)
    }

    /// Save the grid to the typed path, or copy it in the browser build.
    fn export_parts(&mut self, ui: &egui::Ui, shown: &[PartRow], path: &str, grouped: bool) {
        let path = path.trim();
        if path.is_empty() {
            self.message = t!("parts_table.export_no_path");
            return;
        }
        let text = match self.parts_csv(shown, grouped) {
            Ok(text) => text,
            Err(e) => {
                self.message = e;
                return;
            }
        };
        if self.browser {
            ui.ctx().copy_text(text);
            self.message = t!("parts_table.export_copied");
            return;
        }
        match std::fs::write(path, text) {
            Ok(()) => {
                self.message = t!("parts_table.export_saved", path = path, rows = shown.len());
            }
            Err(e) => {
                self.message = t!("parts_table.export_failed", error = e.to_string());
            }
        }
    }

    /// Draw the Parts tab.
    ///
    /// Why: a board with many parts needs its values and part numbers checked and filled
    /// in together, not one Properties dock at a time, and the grid doubles as a bill of
    /// materials once it can be saved and merged.
    ///
    /// # Rules
    /// - Header buttons sort; the same button again reverses the order.
    /// - A filter box keeps the rows whose text in that column contains it, ignoring case.
    /// - Each committed edit is one engine command and so one undo step.
    /// - Fill writes one value to every picked row through one `components.update`.
    /// - Group identical shows merged, read only rows with a Quantity column.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/bottom_sheet.rs calls it for the Parts tab.
    ///
    /// # Do not
    /// - Edit the footprint here; it is read only.
    pub(crate) fn sheet_parts(&mut self, ui: &mut egui::Ui) {
        if self.project.components.is_empty() {
            ui.label(egui::RichText::new(t!("parts_table.empty")).color(self.muted()));
            return;
        }
        let mut state = std::mem::take(&mut self.parts_table);
        let rows = visible_rows(self.part_rows(), &state);
        let shown = if state.grouped {
            self.grouped_rows(&rows)
        } else {
            rows.clone()
        };
        state.selected.retain(|id| rows.iter().any(|r| r.id == *id));
        let has_variants = !self.project.variants.is_empty();
        if has_variants {
            let names: Vec<String> = self
                .project
                .variants
                .iter()
                .map(|v| v.name.clone())
                .collect();
            let mut idx = 0usize;
            if let Some(ref vn) = state.variant {
                idx = names
                    .iter()
                    .position(|n| n == vn)
                    .map(|p| p + 1)
                    .unwrap_or(0);
            }
            if let Some(new_idx) = parts_table_variant::variant_picker(ui, &names, &mut idx) {
                if new_idx == 0 {
                    state.variant = None;
                } else {
                    state.variant = Some(names[new_idx - 1].clone());
                }
            }
            ui.add_space(8.0);
        }
        let mut edit: Option<(&'static str, Value)> = None;
        let mut export = false;
        let mut fill = false;
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut state.grouped, t!("parts_table.group"))
                .on_hover_text(t!("parts_table.group.tip"));
            ui.add_space(12.0);
            ui.checkbox(&mut state.layout, t!("parts_table.layout"))
                .on_hover_text(t!("parts_table.layout.tip"));
            ui.add_space(12.0);
            ui.add_enabled_ui(!state.grouped, |ui| {
                ui.label(t!("parts_table.fill_label"));
                for choice in FillColumn::ALL {
                    let on = state.fill_column == choice;
                    let r = ui
                        .selectable_label(on, choice.column().label())
                        .on_hover_text(t!("parts_table.fill_column.tip"));
                    crate::a11y::name_control(
                        &r,
                        &t!(
                            "parts_table.fill_column_name",
                            column = choice.column().label()
                        ),
                        Some(on),
                    );
                    if r.clicked() {
                        state.fill_column = choice;
                    }
                }
                ui.add(crate::targets::text_field(&mut state.fill_text).desired_width(140.0))
                    .named(&t!("parts_table.fill_value_name"))
                    .on_hover_text(t!("parts_table.fill_value.tip"));
                fill = secondary_button(
                    ui,
                    None,
                    &t!("parts_table.fill"),
                    &t!("parts_table.fill.tip"),
                    Size::Small,
                    !state.selected.is_empty(),
                )
                .clicked();
            });
            ui.add_space(12.0);
            ui.add(crate::targets::text_field(&mut state.export_path).desired_width(220.0))
                .named(&t!("parts_table.export_path_name"))
                .on_hover_text(t!("parts_table.export_path.tip"));
            export = secondary_button(
                ui,
                None,
                &t!("parts_table.export"),
                &t!("parts_table.export.tip"),
                Size::Small,
                true,
            )
            .clicked();
        });
        let grouped = state.grouped;
        let layout = state.layout && !grouped;
        let mut picked: Option<(Id, bool)> = None;
        egui::ScrollArea::horizontal()
            .id_salt("parts-table-scroll")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("parts-table-grid")
                    .num_columns(
                        PartColumn::ALL.len()
                            + 1
                            + if layout { LAYOUT_COLUMNS } else { 0 }
                            + if state.variant.is_some() { 1 } else { 0 },
                    )
                    .spacing([12.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        if grouped {
                            ui.label(t!("parts_table.col_quantity"));
                        } else {
                            ui.label("");
                        }
                        for column in PartColumn::ALL {
                            let icon = match state.sort {
                                Some((c, false)) if c == column => Some(kit_icons::CHEVRON_UP),
                                Some((c, true)) if c == column => Some(kit_icons::CHEVRON_DOWN),
                                _ => None,
                            };
                            let name = t!("parts_table.sort_name", column = column.label());
                            if ghost_button(
                                ui,
                                icon,
                                &name,
                                &t!("parts_table.sort.tip"),
                                Size::Small,
                                true,
                            )
                            .clicked()
                            {
                                state.sort = match state.sort {
                                    Some((c, false)) if c == column => Some((column, true)),
                                    _ => Some((column, false)),
                                };
                            }
                        }
                        if layout {
                            for label in layout_labels() {
                                ui.label(label)
                                    .on_hover_text(t!("parts_table.layout_header.tip"));
                            }
                        }
                        if state.variant.is_some() {
                            ui.label(t!("parts_table.col_dnp"));
                        }
                        ui.end_row();
                        ui.label("");
                        for column in PartColumn::ALL {
                            let name = t!("parts_table.filter_name", column = column.label());
                            ui.add(
                                crate::targets::text_field(&mut state.filters[column.index()])
                                    .desired_width(CELL_WIDTH),
                            )
                            .named(&name)
                            .on_hover_text(t!("parts_table.filter.tip"));
                        }
                        if layout {
                            for _ in 0..LAYOUT_COLUMNS {
                                ui.label("");
                            }
                        }
                        if state.variant.is_some() {
                            ui.label("");
                        }
                        ui.end_row();
                        for row in &shown {
                            if grouped {
                                ui.label(row.quantity.to_string())
                                    .on_hover_text(t!("parts_table.quantity.tip"));
                            } else {
                                let mut on = state.selected.contains(&row.id);
                                let r = ui
                                    .add(egui::Checkbox::without_text(&mut on))
                                    .named(&t!(
                                        "parts_table.pick_name",
                                        part = row.cells[0].clone()
                                    ))
                                    .on_hover_text(t!("parts_table.pick.tip"));
                                if r.changed() {
                                    picked = Some((
                                        row.id,
                                        crate::tool_modifiers::held_now(ui.ctx()).shift,
                                    ));
                                }
                            }
                            for column in PartColumn::ALL {
                                if grouped || !column.editable() {
                                    let tip = if grouped {
                                        t!("parts_table.grouped_cell.tip")
                                    } else {
                                        t!("parts_table.footprint.tip")
                                    };
                                    ui.label(&row.cells[column.index()]).on_hover_text(tip);
                                    continue;
                                }
                                let tip = t!("parts_table.cell.tip");
                                if let Some(text) = Self::part_cell(ui, row, column, &tip) {
                                    edit = Some(match column {
                                        PartColumn::Reference => (
                                            "components.update",
                                            json!({"updates": [{"id": row.id, "reference": text}]}),
                                        ),
                                        PartColumn::Value => (
                                            "components.update",
                                            json!({"updates": [{"id": row.id, "value": text}]}),
                                        ),
                                        PartColumn::Manufacturer => (
                                            "component.set_sourcing",
                                            json!({"id": row.id, "manufacturer": text}),
                                        ),
                                        _ => (
                                            "component.set_sourcing",
                                            json!({"id": row.id, "mpn": text}),
                                        ),
                                    });
                                }
                            }
                            if layout {
                                for text in layout_texts(layout_cells(&self.project, row.id)) {
                                    ui.label(text)
                                        .on_hover_text(t!("parts_table.layout_cell.tip"));
                                }
                            }
                            if let Some(ref vn) = state.variant {
                                let is_dnp = self
                                    .project
                                    .variants
                                    .iter()
                                    .find(|v| v.name == *vn)
                                    .map(|v| v.dnp.contains(&row.id))
                                    .unwrap_or(false);
                                if let Some(new_dnp) =
                                    parts_table_variant::dnp_cell(ui, row.id, &row.cells[0], is_dnp)
                                {
                                    edit = Some((
                                        "variant.set_dnp",
                                        json!({"name": vn, "ids": [row.id], "dnp": new_dnp}),
                                    ));
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
        if shown.is_empty() {
            ui.label(egui::RichText::new(t!("parts_table.no_match")).color(self.muted()));
        }
        if let Some((id, shift)) = picked {
            state.pick(&rows, id, shift);
        }
        let path = state.export_path.clone();
        if fill {
            let key = state.fill_column.key();
            let text = state.fill_text.trim().to_owned();
            let updates: Vec<Value> = rows
                .iter()
                .filter(|r| state.selected.contains(&r.id))
                .map(|r| {
                    let mut entry = serde_json::Map::new();
                    entry.insert("id".into(), json!(r.id));
                    entry.insert(key.into(), json!(text));
                    Value::Object(entry)
                })
                .collect();
            if !updates.is_empty() {
                edit = Some(("components.update", json!({"updates": updates})));
            }
        }
        self.parts_table = state;
        if export {
            self.export_parts(ui, &shown, &path, grouped);
        }
        if let Some((command, params)) = edit {
            self.run(command, params);
        }
    }
}

impl PartsTableState {
    /// Pick or unpick a row; with Shift, pick every row between the last click and this one.
    fn pick(&mut self, rows: &[PartRow], id: Id, shift: bool) {
        let here = rows.iter().position(|r| r.id == id);
        let from = self
            .anchor
            .and_then(|a| rows.iter().position(|r| r.id == a));
        match (shift, here, from) {
            (true, Some(here), Some(from)) => {
                let (low, high) = (here.min(from), here.max(from));
                for row in &rows[low..=high] {
                    self.selected.insert(row.id);
                }
            }
            _ => {
                if !self.selected.remove(&id) {
                    self.selected.insert(id);
                }
            }
        }
        self.anchor = Some(id);
    }

    /// How many rows are picked. Stories read it.
    #[cfg(test)]
    pub(crate) fn picked_count(&self) -> usize {
        self.selected.len()
    }
}

/// Quote a spreadsheet cell only when it holds a comma, a quote or a line break.
fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

/// Split spreadsheet text into rows of cells, honouring quotes.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, _) => cell.push(c),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut cell)),
            (false, '\n') => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            (false, '\r') => {}
            (false, _) => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: Id, reference: &str, value: &str) -> PartRow {
        PartRow {
            id,
            cells: [
                reference.into(),
                value.into(),
                String::new(),
                String::new(),
                String::new(),
            ],
            quantity: 0,
        }
    }

    #[test]
    fn references_sort_by_number_not_by_letters() {
        let state = PartsTableState::default();
        let rows = visible_rows(
            vec![row(1, "R10", "1k"), row(2, "R2", "1k"), row(3, "C1", "1u")],
            &state,
        );
        let order: Vec<&str> = rows.iter().map(|r| r.cells[0].as_str()).collect();
        assert_eq!(order, ["C1", "R2", "R10"]);
    }

    #[test]
    fn a_filter_keeps_rows_containing_the_text_ignoring_case() {
        let mut state = PartsTableState::default();
        state.filters[1] = " 10K ".into();
        let rows = visible_rows(vec![row(1, "R1", "10k"), row(2, "R2", "4k7")], &state);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, 1);
    }

    #[test]
    fn sorting_descending_reverses_the_order() {
        let state = PartsTableState {
            sort: Some((PartColumn::Value, true)),
            ..PartsTableState::default()
        };
        let rows = visible_rows(vec![row(1, "R1", "1k"), row(2, "R2", "4k7")], &state);
        assert_eq!(rows[0].cells[1], "4k7");
    }
}
