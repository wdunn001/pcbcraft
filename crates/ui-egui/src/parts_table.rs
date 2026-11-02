//! The Parts tab of the bottom sheet: every part in one grid you can sort, filter and edit.
//!
//! Context: issue #169, first slice. One row per part with its reference, value,
//! footprint, maker and maker part number. The Reference, Value, Manufacturer and Part
//! number cells are text boxes that commit on Enter or when they lose focus.
//! Local rules: every edit is one engine command (`components.update` for reference
//! and value, `component.set_sourcing` for maker and part number), so one edit is one
//! undo step and a refused edit shows the engine's message and changes nothing. Sorting
//! and filtering only change what is shown; they never touch the document. Words come
//! from `crates/text/strings/en.txt`; the text being typed lives in egui memory.
//!
//! # Dependencies
//! - crates/ui-egui/src/bottom_sheet.rs (hosts the tab)
//! - crates/ui-egui/src/ui_kit/button.rs (header buttons)
//! - crates/engine/src/lib.rs (`components.update`, `component.set_sourcing`)
//!
//! # Do not
//! - Write the document directly; go through `App::run`.
//! - Keep the edited text on `App`.
use super::*;
use crate::a11y::Named;
use crate::ui_kit::{
    button::{Size, ghost_button},
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

/// What the grid shows: the sorted column and one filter text per column.
#[derive(Clone, Debug, Default)]
pub(crate) struct PartsTableState {
    sort: Option<(PartColumn, bool)>,
    filters: [String; 5],
}

/// One part as the grid shows it.
#[derive(Clone, Debug)]
struct PartRow {
    id: Id,
    cells: [String; 5],
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
            .add(crate::targets::text_field(&mut text).desired_width(140.0))
            .named(&name)
            .on_hover_text(tip);
        let done =
            (r.lost_focus() && text.trim() != current.as_str()).then(|| text.trim().to_owned());
        ui.data_mut(|d| d.insert_temp(key, (text, current.clone())));
        done
    }

    /// Draw the Parts tab.
    ///
    /// Why: a board with many parts needs its values and part numbers checked and filled
    /// in together, not one Properties dock at a time.
    ///
    /// # Rules
    /// - Header buttons sort; the same button again reverses the order.
    /// - A filter box keeps the rows whose text in that column contains it, ignoring case.
    /// - Each committed edit is one engine command and so one undo step.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/bottom_sheet.rs calls it for the Parts tab.
    ///
    /// # Do not
    /// - Edit the footprint here; it is read only in this slice.
    pub(crate) fn sheet_parts(&mut self, ui: &mut egui::Ui) {
        if self.project.components.is_empty() {
            ui.label(egui::RichText::new(t!("parts_table.empty")).color(self.muted()));
            return;
        }
        let mut state = std::mem::take(&mut self.parts_table);
        let rows = visible_rows(self.part_rows(), &state);
        let mut edit: Option<(&'static str, Value)> = None;
        egui::ScrollArea::horizontal()
            .id_salt("parts-table-scroll")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("parts-table-grid")
                    .num_columns(PartColumn::ALL.len())
                    .spacing([12.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
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
                        ui.end_row();
                        for column in PartColumn::ALL {
                            let name = t!("parts_table.filter_name", column = column.label());
                            ui.add(
                                crate::targets::text_field(&mut state.filters[column.index()])
                                    .desired_width(140.0),
                            )
                            .named(&name)
                            .on_hover_text(t!("parts_table.filter.tip"));
                        }
                        ui.end_row();
                        for row in &rows {
                            for column in PartColumn::ALL {
                                if !column.editable() {
                                    ui.label(&row.cells[column.index()])
                                        .on_hover_text(t!("parts_table.footprint.tip"));
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
                            ui.end_row();
                        }
                    });
            });
        if rows.is_empty() {
            ui.label(egui::RichText::new(t!("parts_table.no_match")).color(self.muted()));
        }
        self.parts_table = state;
        if let Some((command, params)) = edit {
            self.run(command, params);
        }
    }
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
