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
//! The section shows when a kind switch is on that allows connectors and the search is empty,
//! or when the typed words match a series (its prefix, its maker or its description).
//!
//! # Dependencies
//! - crates/library/src/connector_catalog.rs (`ConnectorCatalog`, one listing per series)
//! - crates/library/src/connector_series_data.rs (`series_spec`: position and row limits)
//! - crates/ui-egui/src/library_dock.rs (calls `dock_series_section` under the filters)
//!
//! # Do not
//! - Invent a series, a count or a drawing the catalog does not hold.
//! - Show the section when no kind switch is on and the search is empty; the dock stays as it was.
use crate::ui_kit::rows::list_row_high;
use crate::{App, fonts, t};
use pcbcraft_library::connector_catalog::ConnectorCatalog;
use pcbcraft_library::listing::PartFilter;

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

impl App {
    /// The series rows the dock shows now, none when the section is not wanted.
    pub(crate) fn dock_series_wanted(&self, query: &str) -> Vec<SeriesRow> {
        if self.dock.tag.is_some() || !self.kind_allows("connector") {
            return Vec::new();
        }
        if query.is_empty() && self.dock.kinds.is_empty() {
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
            list_row_high(ui, ROW, false, &name, &tip, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add(egui::Label::new(egui::RichText::new(&name).strong()).truncate());
                    ui.add(
                        egui::Label::new(
                            fonts::caption(format!("{}. {}", row.summary, row.detail)).color(muted),
                        )
                        .truncate(),
                    );
                });
            });
        }
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.add_space(6.0);
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
    fn a_word_keeps_only_the_series_it_matches() {
        let rows = series_rows("TSW");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "TSW");
        assert!(series_rows("nothing like this").is_empty());
    }
}
