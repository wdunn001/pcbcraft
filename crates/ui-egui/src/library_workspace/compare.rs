//! The Compare page: two or more open parts side by side, with what differs highlighted.
//!
//! Context: issues #699 and #713. The table is built by `rows`, a pure function over
//! `PartFacts`, so the diff rule is tested without a window. The page draws it.
//!
//! Local rules: a row differs when its cells are not all equal, and a missing value counts as
//! different from a value (the person is told "Not listed", never shown a blank). A row where
//! every part lacks the value does not differ. Differences are shown with a word and an icon,
//! never colour alone. Prices are cached values at the chosen quantity and carry their date in
//! the note under the table.
//!
//! # Dependencies
//! - crates/ui-egui/src/library_workspace/facts.rs
//! - crates/ui-egui/src/library_workspace/overview.rs (`key_parameters`, `lifecycle_words`)
//!
//! # Do not
//! - Convert a price between currencies.
use super::facts::{PartFacts, date_text, price_text};
use super::overview::{flag_words, key_parameters, lifecycle_words};
use super::*;
use crate::ui_kit::checkbox::checkbox;
use crate::ui_kit::segmented::{Segment, segmented};

/// The order quantities the page offers.
pub(crate) const QUANTITIES: [u32; 3] = [1, 100, 1000];

/// One row of the compare table.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompareRow {
    /// What the row measures.
    pub(crate) label: String,
    /// One cell per part, `None` for a value no source gave.
    pub(crate) values: Vec<Option<String>>,
    /// True when the cells are not all equal.
    pub(crate) differs: bool,
}

/// The rows of the table for `parts` at an order of `quantity` pieces.
///
/// # Rules
/// - Fixed order: identity, electrical, environment, commercial.
/// - `differs` is true when the cells are not all equal; all-missing is equal.
pub(crate) fn rows(parts: &[PartFacts], quantity: u32) -> Vec<CompareRow> {
    let mut table: Vec<(String, Vec<Option<String>>)> = Vec::new();
    let params: Vec<Vec<(String, Option<String>)>> = parts.iter().map(key_parameters).collect();
    if let Some(first) = params.first() {
        for (i, (label, _)) in first.iter().enumerate() {
            table.push((
                label.clone(),
                params.iter().map(|p| p[i].1.clone()).collect(),
            ));
        }
    }
    let each = |f: &dyn Fn(&PartFacts) -> Option<String>| parts.iter().map(f).collect::<Vec<_>>();
    table.push((t!("libws.life.lifecycle"), each(&|p| lifecycle_words(p))));
    table.push((
        t!("props.sourcing.rohs"),
        each(&|p| flag_words(p.rohs.or(p.listing.as_ref().and_then(|l| l.rohs)))),
    ));
    table.push((t!("props.sourcing.reach"), each(&|p| flag_words(p.reach))));
    table.push((
        t!("libws.compare.price", quantity = quantity),
        each(&|p| p.unit_price_at(quantity).map(|(v, c)| price_text(v, &c))),
    ));
    table.push((
        t!("libws.vendors.stock"),
        each(&|p| p.stock_total().map(|n| n.to_string())),
    ));
    table.push((
        t!("libws.compare.datasheet"),
        each(&|p| p.datasheet.clone()),
    ));
    table
        .into_iter()
        .map(|(label, values)| {
            let differs = values.iter().any(|v| *v != values[0]);
            CompareRow {
                label,
                values,
                differs,
            }
        })
        .collect()
}

impl App {
    /// The compare table of every open part.
    pub(super) fn library_compare_page(&mut self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        let cards: Vec<Card> = self.libws.parts.iter().map(|p| p.card.clone()).collect();
        let facts: Vec<PartFacts> = cards.iter().map(|c| self.library_facts(ui, c)).collect();
        let quantity = QUANTITIES[self.libws.compare_qty.min(QUANTITIES.len() - 1)];
        let all = rows(&facts, quantity);
        let differing = all.iter().filter(|r| r.differs).count();
        let missing = all
            .iter()
            .filter(|r| r.differs)
            .flat_map(|r| r.values.iter())
            .filter(|v| v.is_none())
            .count();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(fonts::display(t!("libws.compare.title")).color(t.text));
                ui.label(
                    fonts::text(
                        fonts::Role::Body,
                        t!(
                            "libws.compare.summary",
                            differing = differing,
                            total = all.len(),
                            missing = missing
                        ),
                    )
                    .color(t.muted),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let labels: Vec<String> = QUANTITIES.iter().map(|q| q.to_string()).collect();
                let tips: Vec<String> = QUANTITIES
                    .iter()
                    .map(|q| t!("libws.compare.pieces.tip", count = q))
                    .collect();
                let segments: Vec<Segment<'_>> = labels
                    .iter()
                    .zip(&tips)
                    .map(|(label, tip)| Segment { label, tip })
                    .collect();
                segmented(
                    ui,
                    "library-compare-qty",
                    &mut self.libws.compare_qty,
                    &segments,
                );
                checkbox(
                    ui,
                    &mut self.libws.compare_only_diff,
                    &t!("libws.compare.only"),
                    &t!("libws.compare.only.tip"),
                );
                checkbox(
                    ui,
                    &mut self.libws.compare_highlight,
                    &t!("libws.compare.highlight"),
                    &t!("libws.compare.highlight.tip"),
                );
            });
        });
        let only = self.libws.compare_only_diff;
        let highlight = self.libws.compare_highlight;
        crate::ui_kit::card::card(
            ui,
            &t!("libws.compare.parts"),
            None,
            |_| {},
            |ui| {
                egui::Grid::new("library-compare-table")
                    .num_columns(1 + facts.len())
                    .spacing([24.0, 8.0])
                    .min_col_width(160.0)
                    .show(ui, |ui| {
                        ui.label(
                            fonts::section_caption(t!("libws.compare.parameter")).color(t.muted),
                        );
                        for f in &facts {
                            ui.label(fonts::strong(&f.name).color(t.text));
                        }
                        ui.end_row();
                        for row in all.iter().filter(|r| !only || r.differs) {
                            ui.horizontal(|ui| {
                                ui.label(fonts::text(fonts::Role::Body, &row.label).color(t.muted));
                                if row.differs && highlight {
                                    crate::ui_kit::chip::badge(
                                        ui,
                                        &t!("libws.compare.differs"),
                                        crate::ui_kit::chip::BadgeKind::Warning,
                                        &t!("libws.compare.differs.tip"),
                                    );
                                }
                            });
                            for v in &row.values {
                                compare_cell(ui, v.as_deref(), row.differs && highlight, &t);
                            }
                            ui.end_row();
                        }
                    });
                ui.add_space(4.0);
                let dated = facts
                    .iter()
                    .flat_map(|f| f.quotes.iter().map(|q| q.retrieved_at))
                    .max();
                let note = match dated {
                    Some(at) => t!("libws.compare.note_dated", date = date_text(at)),
                    None => t!("libws.compare.note"),
                };
                ui.label(fonts::caption(note).color(t.muted));
            },
        );
    }
}

/// One cell: the value, or "Not listed" with a warning icon; a differing cell is bold.
fn compare_cell(ui: &mut egui::Ui, value: Option<&str>, mark: bool, t: &theme::Tokens) {
    match value {
        Some(v) => {
            let text = if mark {
                fonts::strong(v)
            } else {
                fonts::mono(v)
            };
            ui.label(text.color(t.text));
        }
        None => {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(14.0), egui::Sense::hover());
                (crate::ui_kit::icons::WARNING.0)(
                    ui.painter(),
                    rect,
                    if mark { t.gold } else { t.muted },
                );
                ui.label(fonts::text(fonts::Role::Body, t!("libws.not_listed")).color(t.muted));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_library::sources::{PartResult, PriceBreak};

    fn part(name: &str, package: &str, price: f64, stock: u64) -> PartFacts {
        PartFacts {
            name: name.into(),
            package: Some(package.into()),
            pins: Some(2),
            quotes: vec![PartResult {
                source: "recorded".into(),
                mpn: name.into(),
                manufacturer: String::new(),
                description: String::new(),
                kind: String::new(),
                distributor: "Supplier A".into(),
                sku: "A".into(),
                stock,
                price_breaks: vec![PriceBreak {
                    quantity: 1,
                    unit_price: price,
                    currency: "USD".into(),
                }],
                lifecycle: String::new(),
                datasheet_url: String::new(),
                product_url: String::new(),
                retrieved_at: 1_792_540_800,
            }],
            ..Default::default()
        }
    }

    fn row<'a>(all: &'a [CompareRow], label: &str) -> &'a CompareRow {
        all.iter().find(|r| r.label == label).unwrap_or(&all[0])
    }

    #[test]
    fn rows_differ_when_the_cells_are_not_all_equal() {
        let all = rows(&[part("A", "1206", 0.1, 5), part("B", "0603", 0.1, 9)], 1);
        assert!(row(&all, "Package").differs, "1206 and 0603 differ");
        assert!(!row(&all, "Pin count").differs, "both have two pins");
        assert!(
            !row(&all, "Price at 1").differs,
            "the same price at the same quantity does not differ"
        );
        assert!(row(&all, "Stock").differs, "5 and 9 differ");
    }

    #[test]
    fn a_missing_value_differs_from_a_value_and_all_missing_is_equal() {
        let mut b = part("B", "1206", 0.1, 5);
        b.pins = None;
        let all = rows(&[part("A", "1206", 0.1, 5), b], 1);
        let pins = row(&all, "Pin count");
        assert!(pins.differs && pins.values[1].is_none());
        let both = rows(&[PartFacts::default(), PartFacts::default()], 1);
        assert!(
            both.iter().all(|r| !r.differs),
            "two parts that both lack a value do not differ on it"
        );
    }

    #[test]
    fn the_price_row_follows_the_asked_quantity() {
        let a = part("A", "1206", 0.1, 5);
        let at_1000 = rows(std::slice::from_ref(&a), 1000);
        let price = row(&at_1000, "Price at 1000");
        assert_eq!(price.values[0].as_deref(), Some("0.100 USD"));
    }
}
