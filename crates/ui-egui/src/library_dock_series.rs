//! The vendor connector series in the Library dock: real catalog entries, shown without a search.
//!
//! Context: issues #1557 and #1560. The library crate embeds a catalog of connector series
//! (`crates/library/data/connectors`), and `library.list` already reports them, but the dock asks
//! only for installed libraries, so a person who turned on Connectors saw nothing from it.
//! This section reads the embedded catalog directly, so it needs no worker and shows in the
//! browser build too.
//! Local rules: only fields the catalog holds are shown (series, maker, description, pitch and
//! the range of positions). A series is a configurable family, not a part number, and no
//! footprint exists until a pin count is chosen, so the row has no drawing.
//! The section shows by default, with an empty search and no filter, so a person who opens the
//! Library sees real components at once. A kind switch that does not allow connectors, a picked tag,
//! or typed words that match no series hide it.
//!
//! Picking a row opens its configuration under it: rows, positions per row, the lead or stack
//! code and alignment posts. The part number and footprint are generated from the series table
//! by `generate_connector_part` on every frame the panel is open (it is a small pure function),
//! and Place hands the symbol and footprint to the engine's `part.place`, which imports them into
//! the project library and places the part as one undo step.
//!
//! # Dependencies
//! - crates/library/src/connector_catalog.rs (`ConnectorCatalog`, one listing per series)
//! - crates/library/src/connector_series_data.rs (`series_spec`: position and row limits, codes)
//! - crates/library/src/connector_series_generate.rs (`generate_connector_part`)
//! - crates/engine/src/authoring.rs (`part.place`)
//! - crates/ui-egui/src/library_dock.rs (calls `dock_series_section` under the filters)
//!
//! # Do not
//! - Invent a series, a count or a drawing the catalog does not hold.
//! - Hide the section on an empty search with no filter; the owner wants real components in view.
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::rows::list_row_high;
use crate::{App, Tab, fonts, t};
use pcbcraft_library::connector_catalog::ConnectorCatalog;
use pcbcraft_library::listing::PartFilter;
use pcbcraft_library::{ConnectorSeriesOptions, generate_connector_part, series_spec};
use serde_json::json;

/// What the person has chosen for the series that is open.
#[derive(Default)]
pub(crate) struct SeriesState {
    /// The series prefix whose panel is open.
    open: Option<String>,
    rows: u32,
    positions: u32,
    lead: String,
    stack: String,
    posts: bool,
}

impl SeriesState {
    #[cfg(test)]
    pub(crate) fn open_for_test(&self) -> Option<&str> {
        self.open.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn positions_for_test(&self) -> u32 {
        self.positions
    }

    /// Open a series with the smallest useful choices from its table.
    fn start(&mut self, id: &str) {
        let Ok(spec) = series_spec(id) else {
            return;
        };
        self.open = Some(id.to_owned());
        self.rows = spec.limits.rows_min;
        self.positions = spec
            .limits
            .positions_per_row_min
            .max(4)
            .min(spec.limits.positions_per_row_max);
        self.lead = spec
            .lead_codes
            .first()
            .map(|c| c.code.clone())
            .unwrap_or_default();
        self.stack = spec
            .stack_codes
            .first()
            .map(|c| c.code.clone())
            .unwrap_or_default();
        self.posts = false;
    }

    fn options(&self) -> ConnectorSeriesOptions {
        ConnectorSeriesOptions {
            positions_per_row: self.positions,
            rows: self.rows,
            lead_code: self.lead.clone(),
            stack_code: (!self.stack.is_empty()).then(|| self.stack.clone()),
            alignment_posts: self.posts,
        }
    }
}

/// Height of one series row.
const ROW: f32 = 52.0;

/// One catalog series as the dock words it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SeriesRow {
    /// The series prefix, for example `TSW`.
    pub(crate) id: String,
    /// The maker.
    pub(crate) maker: String,
    /// What the catalog says the series is.
    pub(crate) summary: String,
    /// Pitch and the range of positions, from the catalog and the series table.
    pub(crate) detail: String,
}

/// The rows the catalog gives for a search: all of them for an empty search, the matches otherwise.
pub(crate) fn series_rows(query: &str) -> Vec<SeriesRow> {
    let Ok(catalog) = ConnectorCatalog::embedded() else {
        return Vec::new();
    };
    let Ok(listings) = ConnectorCatalog::listings() else {
        return Vec::new();
    };
    let filter = PartFilter {
        text: (!query.trim().is_empty()).then(|| query.trim().to_owned()),
        ..PartFilter::default()
    };
    let mut rows = Vec::new();
    for (series, listing) in catalog.series.iter().zip(listings.iter()) {
        if !filter.matches(listing) {
            continue;
        }
        let positions = pcbcraft_library::series_spec(&series.prefix)
            .map(|spec| {
                let low = spec.limits.positions_per_row_min * spec.limits.rows_min;
                let high = spec.limits.positions_per_row_max * spec.limits.rows_max;
                t!("library.dock.series.positions", low = low, high = high)
            })
            .unwrap_or_default();
        let detail = [listing.package_text.clone(), positions]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        rows.push(SeriesRow {
            id: series.prefix.clone(),
            maker: catalog.manufacturer.clone(),
            summary: series.description.clone(),
            detail,
        });
    }
    rows
}

/// The plain line that says why a position count cannot go further, when it sits at a limit.
pub(crate) fn limit_line(
    spec: &pcbcraft_library::ConnectorSeriesSpec,
    positions: u32,
) -> Option<String> {
    let limits = &spec.limits;
    if positions >= limits.positions_per_row_max {
        Some(t!(
            "library.dock.series.limit.max",
            max = limits.positions_per_row_max
        ))
    } else if positions <= limits.positions_per_row_min {
        Some(t!(
            "library.dock.series.limit.min",
            min = limits.positions_per_row_min
        ))
    } else {
        None
    }
}

impl App {
    /// The series rows the dock shows now, none when the section is not wanted.
    pub(crate) fn dock_series_wanted(&self, query: &str) -> Vec<SeriesRow> {
        if self.dock.tag.is_some() || !self.kind_allows("connector") {
            return Vec::new();
        }
        series_rows(query)
    }

    /// Draw the heading and one row per series under the filters.
    pub(crate) fn dock_series_section(&mut self, ui: &mut egui::Ui, query: &str) {
        let rows = self.dock_series_wanted(query);
        if rows.is_empty() {
            return;
        }
        let muted = self.muted();
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(t!("library.dock.series.heading", maker = rows[0].maker))
                .strong()
                .font(fonts::Role::Caption.font()),
        );
        ui.label(fonts::caption(t!("library.dock.series.intro")).color(muted));
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        for row in &rows {
            let name = t!("library.dock.series.row", id = row.id, maker = row.maker);
            let tip = t!(
                "library.dock.series.row.tip",
                id = row.id,
                maker = row.maker
            );
            let open = self.dock.series.open.as_deref() == Some(row.id.as_str());
            let response = list_row_high(ui, ROW, open, &name, &tip, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add(
                        egui::Label::new(egui::RichText::new(&name).strong())
                            .truncate()
                            .selectable(false),
                    );
                    ui.add(
                        egui::Label::new(
                            fonts::caption(format!("{}. {}", row.summary, row.detail)).color(muted),
                        )
                        .truncate()
                        .selectable(false),
                    );
                });
            });
            if response.clicked() {
                if open {
                    self.dock.series.open = None;
                } else {
                    self.dock.series.start(&row.id);
                }
            }
            if open {
                self.dock_series_panel(ui, &row.id);
            }
        }
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.add_space(6.0);
    }
}

impl App {
    /// The choices for the open series, the part they make, and the Place button.
    fn dock_series_panel(&mut self, ui: &mut egui::Ui, id: &str) {
        let Ok(spec) = series_spec(id) else {
            return;
        };
        let muted = self.muted();
        let chip = |ui: &mut egui::Ui, text: &str, tip: &str, on: bool| {
            ui.add(
                egui::Button::new(text)
                    .selected(on)
                    .min_size(egui::Vec2::splat(crate::targets::MIN_TARGET)),
            )
            .on_hover_text(tip)
            .clicked()
        };
        ui.add_space(4.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let state = &mut self.dock.series;
            if spec.limits.rows_min != spec.limits.rows_max {
                ui.horizontal_wrapped(|ui| {
                    ui.label(fonts::caption(t!("library.dock.series.rows")).color(muted));
                    for rows in spec.limits.rows_min..=spec.limits.rows_max {
                        let text = if rows == 1 {
                            t!("library.dock.series.rows.one")
                        } else {
                            t!("library.dock.series.rows.n", n = rows)
                        };
                        let tip = t!("library.dock.series.rows.tip", n = rows);
                        if chip(ui, &text, &tip, state.rows == rows) {
                            state.rows = rows;
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label(fonts::caption(t!("library.dock.series.positions_per_row")).color(muted));
                if button::secondary_button(
                    ui,
                    None,
                    &t!("library.dock.series.fewer"),
                    &t!("library.dock.series.fewer.tip"),
                    Size::Small,
                    state.positions > spec.limits.positions_per_row_min,
                )
                .clicked()
                {
                    state.positions -= 1;
                }
                ui.label(egui::RichText::new(state.positions.to_string()).strong());
                if button::secondary_button(
                    ui,
                    None,
                    &t!("library.dock.series.more"),
                    &t!("library.dock.series.more.tip"),
                    Size::Small,
                    state.positions < spec.limits.positions_per_row_max,
                )
                .clicked()
                {
                    state.positions += 1;
                }
            });
            if let Some(line) = limit_line(&spec, state.positions) {
                ui.label(fonts::caption(line).color(muted));
            }
            if !spec.lead_codes.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(fonts::caption(t!("library.dock.series.lead")).color(muted));
                    for code in &spec.lead_codes {
                        let tip = t!(
                            "library.dock.series.lead.tip",
                            code = code.code,
                            tail = code.tail_below_board_mm
                        );
                        if chip(ui, &code.code, &tip, state.lead == code.code) {
                            state.lead.clone_from(&code.code);
                        }
                    }
                });
            }
            if !spec.stack_codes.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(fonts::caption(t!("library.dock.series.stack")).color(muted));
                    for code in &spec.stack_codes {
                        let tip = t!(
                            "library.dock.series.stack.tip",
                            code = code.code,
                            height = code.mated_stack_height_mm
                        );
                        if chip(ui, &code.code, &tip, state.stack == code.code) {
                            state.stack.clone_from(&code.code);
                        }
                    }
                });
            }
            ui.checkbox(&mut state.posts, t!("library.dock.series.posts"))
                .on_hover_text(t!("library.dock.series.posts.tip"));
        });
        let made = generate_connector_part(id, &self.dock.series.options());
        match &made {
            Ok(part) => {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(t!(
                            "library.dock.series.makes",
                            number = part.part_number
                        ))
                        .strong(),
                    )
                    .truncate(),
                );
            }
            Err(errors) => {
                let said = errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                ui.label(fonts::caption(said).color(crate::theme::tokens(ui.ctx()).red));
            }
        }
        let ready = made.is_ok();
        let target = if self.tab == Tab::Pcb {
            "board"
        } else {
            "schematic"
        };
        let (label, tip) = if target == "board" {
            (
                t!("library.dock.series.place_board"),
                t!("library.dock.series.place_board.tip"),
            )
        } else {
            (
                t!("library.dock.series.place"),
                t!("library.dock.series.place.tip"),
            )
        };
        if button::primary_button(ui, None, &label, &tip, Size::Normal, ready).clicked()
            && let Ok(part) = made
        {
            self.dock_series_place(id, part, target);
        }
        ui.add_space(6.0);
    }

    /// Hand a generated part to `part.place`: it imports the symbol and footprint into the project
    /// library and places the part, all as one undo step.
    fn dock_series_place(
        &mut self,
        id: &str,
        part: pcbcraft_library::GeneratedConnectorPart,
        target: &str,
    ) {
        let note = ConnectorCatalog::embedded()
            .map(|c| c.verify_note.clone())
            .unwrap_or_default();
        let at = crate::canvas::snap(if target == "board" {
            self.pcb.center
        } else {
            self.schematic.center
        });
        let sheet = self.dock_sheet(target);
        let params = crate::library_dock::with_sheet(
            json!({
                "target": target,
                "position": at,
                "symbol": part.symbol,
                "footprint": part.footprint,
                "notice": t!("library.dock.series.notice", series = id, note = note),
                "value": part.part_number,
            }),
            sheet,
        );
        self.run("part.place", params);
        self.dock_after_place(target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_search_lists_every_catalog_series_with_real_limits() {
        let rows = series_rows("");
        let ids: Vec<_> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            ["TSW", "SSW", "BTH"],
            "the three catalog series, in catalog order"
        );
        assert!(rows[0].detail.contains("0.100 in pitch"), "{:?}", rows[0]);
        assert!(
            rows[0].detail.contains("1 to 80 positions"),
            "{:?}",
            rows[0]
        );
        assert!(
            rows[2].detail.contains("6 to 300 positions"),
            "{:?}",
            rows[2]
        );
    }

    #[test]
    fn every_catalog_series_opens_with_choices_inside_its_limits() -> Result<(), String> {
        for id in ["TSW", "SSW", "BTH"] {
            let mut state = SeriesState::default();
            state.start(id);
            assert_eq!(state.open.as_deref(), Some(id), "{id} opens");
            let spec = series_spec(id)?;
            assert!(
                (spec.limits.positions_per_row_min..=spec.limits.positions_per_row_max)
                    .contains(&state.positions),
                "{id} starts inside its position limits"
            );
            assert!(
                generate_connector_part(id, &state.options()).is_ok(),
                "{id} starts on a choice that makes a part"
            );
        }
        Ok(())
    }

    #[test]
    fn a_word_keeps_only_the_series_it_matches() {
        let rows = series_rows("TSW");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "TSW");
        assert!(series_rows("nothing like this").is_empty());
    }
}
