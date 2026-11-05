//! The Alternates page: second sources and alternates for the open part.
//!
//! Context: issue #719. A second source is another maker's part number for the same slot.
//! An alternate is another project-library symbol that may replace this one.
//!
//! Local rules: second sources are keyed by manufacturer name; alternates by symbol name.
//! At most eight of each. Every add or remove is one engine command, one undo step.
//!
//! # Dependencies
//! - crates/library/src/part_substitutes.rs (`PartSubstitutes`, `SecondSource`, `AlternatePart`)
//! - crates/engine/src/authoring.rs (`library.part.set_substitute`)
use super::*;
use crate::targets::text_field;
use crate::ui_kit::button::{Size, secondary_button};

impl App {
    /// The Alternates page.
    pub(super) fn library_alternates_page(&mut self, ui: &mut egui::Ui, part_card: &Card) {
        let t = theme::tokens(ui.ctx());
        let facts = self.library_facts(ui, part_card);
        let name = facts.name.clone();

        // Substitutes are kept on parts of the project library only.
        if !self.project.library.symbols.iter().any(|s| s.name == name) {
            ui.label(
                fonts::text(fonts::Role::Body, t!("libws.alternates.project_only")).color(t.muted),
            );
            return;
        }

        // Read substitutes from the project library
        let subs = self
            .project
            .library
            .part_substitutes
            .get(&name)
            .cloned()
            .unwrap_or_default();

        // Second sources section
        ui.label(fonts::title(t!("libws.alternates.second_sources")).color(t.text));
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.add(text_field(&mut self.libws.alt_manufacturer))
                .named(&t!("libws.alternates.manufacturer"))
                .on_hover_text(t!("libws.alternates.manufacturer.tip"));
            ui.add(text_field(&mut self.libws.alt_mpn))
                .named(&t!("libws.alternates.mpn"))
                .on_hover_text(t!("libws.alternates.mpn.tip"));
            if secondary_button(
                ui,
                None,
                &t!("libws.alternates.add_source"),
                &t!("libws.alternates.add_source.tip"),
                Size::Small,
                !self.libws.alt_manufacturer.trim().is_empty(),
            )
            .clicked()
            {
                let manufacturer = self.libws.alt_manufacturer.trim().to_owned();
                let mpn = self.libws.alt_mpn.trim().to_owned();
                self.tool_call(
                    "library.part.set_substitute",
                    json!({
                        "name": name,
                        "kind": "second_source",
                        "manufacturer": manufacturer,
                        "mpn": mpn
                    }),
                );
                self.libws.alt_manufacturer.clear();
                self.libws.alt_mpn.clear();
                self.refresh();
            }
        });
        ui.add_space(8.0);

        if subs.second_sources.is_empty() {
            ui.label(
                fonts::text(fonts::Role::Body, t!("libws.alternates.no_sources")).color(t.muted),
            );
        } else {
            for src in subs.second_sources.iter() {
                ui.horizontal(|ui| {
                    ui.label(fonts::strong(&src.manufacturer).color(t.text));
                    ui.label(fonts::text(fonts::Role::Body, &src.mpn).color(t.muted));
                    if !src.note.is_empty() {
                        ui.label(fonts::caption(&src.note).color(t.muted));
                    }
                    if secondary_button(
                        ui,
                        None,
                        &t!("libws.alternates.remove"),
                        &t!("libws.alternates.remove.tip"),
                        Size::Small,
                        true,
                    )
                    .clicked()
                    {
                        self.tool_call(
                            "library.part.set_substitute",
                            json!({
                                "name": name,
                                "kind": "second_source",
                                "manufacturer": src.manufacturer,
                                "mpn": null
                            }),
                        );
                        self.refresh();
                    }
                });
            }
        }
        ui.add_space(16.0);

        // Alternates section
        ui.label(fonts::title(t!("libws.alternates.alternates")).color(t.text));
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.add(text_field(&mut self.libws.alt_symbol))
                .named(&t!("libws.alternates.symbol"))
                .on_hover_text(t!("libws.alternates.symbol.tip"));
            if secondary_button(
                ui,
                None,
                &t!("libws.alternates.add_alternate"),
                &t!("libws.alternates.add_alternate.tip"),
                Size::Small,
                !self.libws.alt_symbol.trim().is_empty(),
            )
            .clicked()
            {
                let symbol = self.libws.alt_symbol.trim().to_owned();
                self.tool_call(
                    "library.part.set_substitute",
                    json!({
                        "name": name,
                        "kind": "alternate",
                        "symbol": symbol
                    }),
                );
                self.libws.alt_symbol.clear();
                self.refresh();
            }
        });
        ui.add_space(8.0);

        if subs.alternates.is_empty() {
            ui.label(
                fonts::text(fonts::Role::Body, t!("libws.alternates.no_alternates")).color(t.muted),
            );
        } else {
            for alt in subs.alternates.iter() {
                ui.horizontal(|ui| {
                    ui.label(fonts::strong(&alt.symbol).color(t.text));
                    if !alt.footprint.is_empty() {
                        ui.label(fonts::text(fonts::Role::Body, &alt.footprint).color(t.muted));
                    }
                    if !alt.note.is_empty() {
                        ui.label(fonts::caption(&alt.note).color(t.muted));
                    }
                    if secondary_button(
                        ui,
                        None,
                        &t!("libws.alternates.remove"),
                        &t!("libws.alternates.remove.tip"),
                        Size::Small,
                        true,
                    )
                    .clicked()
                    {
                        self.tool_call(
                            "library.part.set_substitute",
                            json!({
                                "name": name,
                                "kind": "alternate",
                                "symbol": alt.symbol,
                                "clear": true
                            }),
                        );
                        self.refresh();
                    }
                });
            }
        }
    }
}
