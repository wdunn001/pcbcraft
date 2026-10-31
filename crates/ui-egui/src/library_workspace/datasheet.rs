//! The Datasheet page: the datasheet link and the details read for the part.
//!
//! Context: issue #695 and the datasheet reader note (#678 and children). The page that renders
//! the PDF itself, with page navigation, search and a text layer, is a separate piece of work
//! (a page-render trait over a pure-Rust renderer, a cache of the PDF on this computer). Until
//! it lands this page shows what is known now: the link, the key details, and a button that
//! opens the datasheet in the person's browser.
//!
//! Local rules: nothing is fetched here; a datasheet is only ever opened by the person.
//!
//! # Dependencies
//! - crates/ui-egui/src/library_workspace/overview.rs (`key_parameters`, `value_row`)
//!
//! # Do not
//! - Download or cache a PDF from a frame.
use super::overview::{key_parameters, value_row};
use super::*;
use crate::ui_kit::card::card;

impl App {
    /// The Datasheet page.
    pub(super) fn library_datasheet_page(&mut self, ui: &mut egui::Ui, f: &PartFacts) {
        let t = theme::tokens(ui.ctx());
        card(
            ui,
            &t!("libws.ds.title"),
            None,
            |_| {},
            |ui| {
                match &f.datasheet {
                    Some(url) => {
                        ui.label(fonts::mono(url).color(t.text));
                        if secondary_button(
                            ui,
                            None,
                            &t!("library.dock.datasheet"),
                            &t!("library.dock.datasheet.tip"),
                            Size::Normal,
                            url.starts_with("http://") || url.starts_with("https://"),
                        )
                        .clicked()
                        {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url.clone()));
                        }
                    }
                    None => {
                        ui.label(
                            fonts::text(fonts::Role::Body, t!("libws.ds.no_link")).color(t.muted),
                        );
                    }
                }
                ui.add_space(4.0);
                ui.label(fonts::caption(t!("libws.ds.reader_later")).color(t.muted));
            },
        );
        card(
            ui,
            &t!("libws.ds.details"),
            None,
            |_| {},
            |ui| {
                for (label, value) in key_parameters(f) {
                    value_row(ui, &label, value);
                }
            },
        );
    }
}
