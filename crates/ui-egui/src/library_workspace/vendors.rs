//! The Vendors page: cached prices and stock per supplier, and where the data comes from.
//!
//! Context: issues #695 and #699. No live supplier connection exists yet, so every price here
//! is a cached answer and carries its date. With no connection the page says so in one plain
//! notice. Supplier numbers typed on a placed part show as rows with no price.
//!
//! Local rules: money is never converted; a missing value reads "Not listed"; nothing here
//! reads the network. Supplier numbers edit through the Sourcing rows of Properties
//! (`supplier_rows`), so every edit is one undo step.
//!
//! # Dependencies
//! - crates/ui-egui/src/library_workspace/facts.rs
//! - crates/ui-egui/src/sourcing_panel.rs (`supplier_rows`)
//! - crates/ui-egui/src/library.rs (`Browser` sources)
//!
//! # Do not
//! - Fetch prices from a frame.
use super::facts::{PartFacts, date_text, price_text};
use super::*;
use crate::ui_kit::button::ghost_button;
use crate::ui_kit::card::card;
use crate::ui_kit::chip::BadgeKind;

/// The break quantities the table shows as columns.
const BREAKS: [u32; 4] = [1, 10, 100, 1000];

impl App {
    /// The Vendors page.
    pub(super) fn library_vendors_page(&mut self, ui: &mut egui::Ui, _card: &Card, f: &PartFacts) {
        let t = theme::tokens(ui.ctx());
        if !self.supplier_connected() {
            egui::Frame::NONE
                .stroke(egui::Stroke::new(theme::BORDER, t.gold))
                .corner_radius(egui::CornerRadius::same(theme::RADIUS_CARD))
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::hover());
                        (crate::ui_kit::icons::WARNING.0)(ui.painter(), rect, t.gold);
                        ui.vertical(|ui| {
                            ui.label(
                                fonts::strong(t!("libws.vendors.no_connection")).color(t.text),
                            );
                            let dated = f.quotes.iter().map(|q| q.retrieved_at).max();
                            let line = match dated {
                                Some(at) => t!("libws.vendors.cached_from", date = date_text(at)),
                                None => t!("libws.vendors.nothing_cached"),
                            };
                            ui.label(fonts::text(fonts::Role::Body, line).color(t.muted));
                        });
                    });
                });
        }
        let rows = f.vendor_rows();
        card(
            ui,
            &t!("libws.vendors.title"),
            None,
            |ui| {
                if secondary_button(
                    ui,
                    None,
                    &t!("libws.refresh_prices"),
                    &t!("libws.refresh_prices.tip"),
                    Size::Small,
                    true,
                )
                .clicked()
                {
                    self.message = t!("libws.refresh_no_connection");
                }
            },
            |ui| {
                if rows.is_empty() {
                    ui.label(
                        fonts::text(fonts::Role::Body, t!("libws.vendors.none")).color(t.muted),
                    );
                    return;
                }
                egui::Grid::new("library-vendor-table")
                    .num_columns(4 + BREAKS.len())
                    .spacing([16.0, 8.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for head in [
                            t!("libws.vendors.supplier"),
                            t!("libws.vendors.sku"),
                            t!("libws.vendors.stock"),
                        ] {
                            ui.label(fonts::section_caption(head).color(t.muted));
                        }
                        for q in BREAKS {
                            ui.label(fonts::section_caption(q.to_string()).color(t.muted));
                        }
                        ui.label(
                            fonts::section_caption(t!("libws.vendors.updated")).color(t.muted),
                        );
                        ui.end_row();
                        for r in &rows {
                            ui.label(fonts::strong(&r.supplier).color(t.text));
                            ui.label(fonts::mono(&r.sku).color(t.text));
                            ui.label(
                                fonts::mono(
                                    r.stock
                                        .map_or_else(|| t!("libws.not_listed"), |n| n.to_string()),
                                )
                                .color(t.text),
                            );
                            for q in BREAKS {
                                let at = r
                                    .breaks
                                    .iter()
                                    .filter(|b| b.quantity <= q)
                                    .max_by_key(|b| b.quantity)
                                    .filter(|b| b.quantity == q || q == 1);
                                let text = match at {
                                    Some(b) => price_text(b.unit_price, &b.currency),
                                    None => "\u{2013}".to_owned(),
                                };
                                ui.label(fonts::mono(text).color(t.text));
                            }
                            let when = r.as_of.map_or_else(|| t!("libws.vendors.never"), date_text);
                            ui.label(fonts::text(fonts::Role::Body, when).color(t.muted));
                            ui.end_row();
                        }
                    });
                ui.add_space(4.0);
                ui.label(fonts::caption(t!("libws.vendors.note")).color(t.muted));
            },
        );
        self.library_sources_card(ui, f);
    }

    /// The Sources card: the supplier numbers of a placed part, and the library folders the
    /// browser reads.
    fn library_sources_card(&mut self, ui: &mut egui::Ui, f: &PartFacts) {
        let t = theme::tokens(ui.ctx());
        card(
            ui,
            &t!("libws.sources.title"),
            Some(&t!("libws.sources.caption")),
            |_| {},
            |ui| {
                ui.label(fonts::strong(t!("libws.sources.suppliers")).color(t.text));
                match f
                    .component
                    .and_then(|id| self.project.component(id).cloned())
                {
                    Some(component) => self.supplier_rows(ui, &component),
                    None => {
                        ui.label(
                            fonts::text(fonts::Role::Body, t!("libws.sources.place_first"))
                                .color(t.muted),
                        );
                    }
                }
                ui.add_space(8.0);
                ui.label(fonts::strong(t!("libws.sources.libraries")).color(t.text));
                self.library_folder_sources(ui);
            },
        );
    }

    /// The library folders in use, with a box to add one and a button to remove an added one.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/library.rs (`Browser::request`, the same requests the Libraries page makes)
    fn library_folder_sources(&mut self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        self.library.poll();
        if self.library.sources.is_none() && self.library.pending.is_none() {
            self.library.request(ui.ctx(), "library.sources", json!({}));
        }
        let list: Vec<(String, String, String)> = self
            .library
            .sources
            .as_ref()
            .and_then(|v| v.get("sources"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|s| {
                        let get = |k: &str| {
                            s.get(k)
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned()
                        };
                        (get("name"), get("kind"), get("path"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (name, kind, path) in &list {
            ui.horizontal(|ui| {
                ui.label(fonts::text(fonts::Role::Body, name).color(t.text));
                let badge_kind = if kind == "added" {
                    BadgeKind::Accent
                } else {
                    BadgeKind::Neutral
                };
                crate::ui_kit::chip::badge(
                    ui,
                    &t!("libws.sources.in_use"),
                    badge_kind,
                    &t!("libws.sources.in_use.tip"),
                );
                if kind == "added"
                    && ghost_button(
                        ui,
                        None,
                        &t!("libws.sources.remove"),
                        &t!("libws.sources.remove.tip", name = name),
                        Size::Small,
                        self.library.pending.is_none(),
                    )
                    .clicked()
                {
                    self.library
                        .request(ui.ctx(), "library.remove_folder", json!({"path": path}));
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add(
                crate::targets::text_field(&mut self.library.new_source)
                    .hint_text(t!("library.collection_browser.folder_to_add_as_a"))
                    .desired_width(260.0),
            )
            .named(&t!("library.collection_browser.folder_to_add_as_a"))
            .on_hover_text(t!("library.collection_browser.folder_to_add_as_a.tip"));
            let ready =
                !self.library.new_source.trim().is_empty() && self.library.pending.is_none();
            if secondary_button(
                ui,
                None,
                &t!("libws.sources.add"),
                &t!("library.collection_browser.add_folder.tip"),
                Size::Small,
                ready,
            )
            .clicked()
            {
                let path = self.library.new_source.trim().to_owned();
                self.library
                    .request(ui.ctx(), "library.add_folder", json!({"path": path}));
            }
        });
        if !self.library.notice.is_empty() {
            ui.label(fonts::caption(&self.library.notice).color(t.muted));
        }
        if !self.library.error.is_empty() {
            ui.label(fonts::caption(&self.library.error).color(t.red));
        }
    }
}
