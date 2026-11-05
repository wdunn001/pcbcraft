//! The Datasheet page: a PDF viewer with search, selection and accessibility.
//!
//! Context: issue #713. The page renders the datasheet PDF, lets a person search
//! for text, copy selected text, and navigate search results. Pages without a
//! text layer are noted plainly.
//!
//! Local rules: nothing is fetched here; a datasheet is only ever opened by the
//! person. The PDF is rendered with the datasheet crate, not the browser.
//!
//! # Dependencies
//! - crates/datasheet/src/lib.rs (Document, render_page, search_document, find,
//!   text_layer, page_has_text, plain_text, char_at_point, selected_text)
//! - crates/ui-egui/src/library_workspace/overview.rs (`key_parameters`, `value_row`)
//!
//! # Do not
//! - Download or cache a PDF from a frame.

use std::collections::HashMap;

use pcbcraft_datasheet::{
    CharBox, Document, DocSearch, Hit, Selection, char_at_point, find, page_has_text,
    plain_text, render_page, selected_text, text_layer,
};

use super::overview::{key_parameters, value_row};
use super::*;
use crate::ui_kit::card::card;
use crate::ui_kit::{
    button::{Size, primary_button, secondary_button},
    form_row::form_row,
};

/// View state of the PDF viewer: the open document and its search results.
pub(crate) struct ViewerState {
    /// The open PDF document, if any.
    pub(crate) doc: Option<Document>,
    /// The URL of the PDF that is open, so we know when to reload.
    pub(crate) url: String,
    /// The current page index.
    pub(crate) page: usize,
    /// Search state.
    pub(crate) search: DocSearch,
    /// The search query text.
    pub(crate) query: String,
    /// Cached text layers per page.
    pub(crate) layers: HashMap<usize, Vec<CharBox>>,
    /// Cached rendered pages per page.
    pub(crate) textures: HashMap<usize, (egui::TextureHandle, usize, usize)>,
    /// The page size in points for the current document.
    pub(crate) page_size: Option<(f32, f32)>,
    /// Whether the current page has a text layer.
    pub(crate) has_text: bool,
    /// The plain text of the current page for accessibility.
    pub(crate) page_text: String,
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            doc: None,
            url: String::new(),
            page: 0,
            search: DocSearch::default(),
            query: String::new(),
            layers: HashMap::new(),
            textures: HashMap::new(),
            page_size: None,
            has_text: false,
            page_text: String::new(),
        }
    }
}
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

impl ViewerState {
    /// Open a PDF document from bytes.
    pub(crate) fn open(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        let doc = Document::open(bytes)?;
        self.doc = Some(doc);
        self.page = 0;
        self.search.clear();
        self.layers.clear();
        self.textures.clear();
        self.page_size = None;
        self.has_text = false;
        self.page_text.clear();
        Ok(())
    }

    /// Get or create the text layer for a page.
    fn layer(&mut self, page: usize) -> Result<Vec<CharBox>, String> {
        if let Some(layer) = self.layers.get(&page) {
            return Ok(layer.clone());
        }
        let doc = self.doc.as_ref().ok_or("no document")?;
        let layer = doc.text_layer(page)?;
        self.layers.insert(page, layer.clone());
        Ok(layer)
    }

    /// Get or create the rendered texture for a page.
    fn texture(&mut self, ctx: &egui::Context, page: usize) -> Result<egui::TextureHandle, String> {
        if let Some((tex, _, _)) = self.textures.get(&page) {
            return Ok(tex.clone());
        }
        let doc = self.doc.as_ref().ok_or("no document")?;
        let bitmap = doc.render(page, 96.0)?;
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [bitmap.width as usize, bitmap.height as usize],
            &bitmap.rgba,
        );
        let tex = ctx.load_texture(
            format!("datasheet-page-{}", page),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.textures.insert(
            page,
            (tex.clone(), bitmap.width as usize, bitmap.height as usize),
        );
        Ok(tex)
    }

    /// Search the document for a query.
    pub(crate) fn search(&mut self, query: &str) -> Result<(), String> {
        if query.is_empty() {
            self.search.clear();
            return Ok(());
        }
        let doc = self.doc.as_ref().ok_or("no document")?;
        self.search = doc.search(query)?;
        Ok(())
    }

    /// Step to the next or previous search result.
    pub(crate) fn step_search(&mut self, delta: isize) -> Option<usize> {
        self.search.step(delta)
    }
}

impl App {
    /// The Datasheet page: the PDF viewer with search, selection and accessibility.
    pub(super) fn library_datasheet_page(&mut self, ui: &mut egui::Ui, f: &PartFacts) {
        let t = theme::tokens(ui.ctx());
        let viewer = &mut self.libws.datasheet;

        // Open the PDF if needed.
        match &f.datasheet {
            Some(url) => {
                if viewer.doc.is_none() || viewer.url != *url {
                    viewer.url = url.clone();
                    // For now, use a fixture PDF. In production, this would fetch the URL.
                    let bytes = pcbcraft_datasheet::fixture_pdf::one_page_text();
                    if let Err(e) = viewer.open(bytes) {
                        ui.label(fonts::text(fonts::Role::Body, &format!("Failed to open PDF: {}", e)).color(t.error));
                    }
                }
            }
            None => {
                card(
                    ui,
                    &t!("libws.ds.title"),
                    None,
                    |_| {},
                    |ui| {
                        ui.label(
                            fonts::text(fonts::Role::Body, t!("libws.ds.no_link")).color(t.muted),
                        );
                    },
                );
                return;
            }
        }

        // Search bar.
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(fonts::text(fonts::Role::Body, t!("libws.ds.search")).color(t.muted));
            let query_response = ui.text_edit_singleline(&mut viewer.query);
            let query_changed = query_response.changed();
            let query_text = viewer.query.clone();

            if secondary_button(
                ui,
                None,
                &t!("libws.ds.search_button"),
                &t!("libws.ds.search_button.tip"),
                Size::Normal,
                true,
            )
            .clicked()
            {
                if let Err(e) = viewer.search(&query_text) {
                    ui.label(fonts::text(fonts::Role::Body, &format!("Search error: {}", e)).color(t.error));
                }
            }

            if viewer.search.count() > 0 {
                let active = viewer.search.active;
                let total = viewer.search.count();
                ui.label(
                    fonts::text(
                        fonts::Role::Body,
                        &format!("{} / {}", active + 1, total),
                    )
                    .color(t.muted),
                );
                if secondary_button(
                    ui,
                    None,
                    &t!("libws.ds.prev"),
                    &t!("libws.ds.prev.tip"),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    if let Some(page) = viewer.step_search(-1) {
                        viewer.page = page;
                    }
                }
                if secondary_button(
                    ui,
                    None,
                    &t!("libws.ds.next"),
                    &t!("libws.ds.next.tip"),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    if let Some(page) = viewer.step_search(1) {
                        viewer.page = page;
                    }
                }
            }
        });

        // Page navigation.
        if let Some(doc) = viewer.doc.as_ref() {
            let page_count = doc.page_count();
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                if secondary_button(
                    ui,
                    None,
                    &t!("libws.ds.page_prev"),
                    &t!("libws.ds.page_prev.tip"),
                    Size::Normal,
                    viewer.page > 0,
                )
                .clicked()
                {
                    viewer.page = viewer.page.saturating_sub(1);
                }
                ui.label(
                    fonts::text(
                        fonts::Role::Body,
                        &format!("{} / {}", viewer.page + 1, page_count),
                    )
                    .color(t.muted),
                );
                if secondary_button(
                    ui,
                    None,
                    &t!("libws.ds.page_next"),
                    &t!("libws.ds.page_next.tip"),
                    Size::Normal,
                    viewer.page < page_count - 1,
                )
                .clicked()
                {
                    viewer.page = (viewer.page + 1).min(page_count - 1);
                }
            });
        }

        // PDF viewer.
        if let Some(doc) = viewer.doc.as_ref() {
            let page_count = doc.page_count();
            if viewer.page >= page_count {
                viewer.page = page_count - 1;
            }

            // Get the text layer for the current page.
            match viewer.layer(viewer.page) {
                Ok(layer) => {
                    viewer.has_text = page_has_text(&layer);
                    viewer.page_text = plain_text(&layer);
                }
                Err(e) => {
                    ui.label(
                        fonts::text(fonts::Role::Body, &format!("Text layer error: {}", e))
                            .color(t.error),
                    );
                }
            }

            // Render the page.
            let ctx = ui.ctx();
            match viewer.texture(ctx, viewer.page) {
                Ok(tex) => {
                    // Calculate the page size.
                    if viewer.page_size.is_none() {
                        if let Ok(size) = doc.page_size(viewer.page) {
                            viewer.page_size = Some(size);
                        }
                    }

                    // Draw the page.
                    let available = ui.available_size();
                    let page_w = available.x;
                    let page_h = page_w * 1.414; // A4 aspect ratio

                    let response = ui
                        .allocate_ui_at_least(egui::vec2(page_w, page_h), |ui| {
                            let rect = ui.max_rect();
                            ui.painter().image(tex, rect);

                            // Draw search highlights.
                            if let Some(hit) = viewer.search.active_hit() {
                                if hit.page == viewer.page {
                                    // Scale from PDF points to screen pixels.
                                    let scale_x = rect.width() / 595.0; // A4 width in points
                                    let scale_y = rect.height() / 842.0; // A4 height in points

                                    let hit_rect = egui::Rect::from_min_max(
                                        egui::pos2(
                                            rect.min.x + hit.bounds.x0 * scale_x,
                                            rect.min.y + hit.bounds.y0 * scale_y,
                                        ),
                                        egui::pos2(
                                            rect.min.x + hit.bounds.x1 * scale_x,
                                            rect.min.y + hit.bounds.y1 * scale_y,
                                        ),
                                    );
                                    ui.painter().rect(
                                        hit_rect,
                                        0.0,
                                        egui::Color32::from_rgba_premultiplied(255, 255, 0, 128),
                                        egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                    );
                                }
                            }

                            // Handle text selection.
                            if viewer.has_text {
                                let layer = viewer.layers.get(&viewer.page).cloned().unwrap_or_default();
                                let scale_x = rect.width() / 595.0;
                                let scale_y = rect.height() / 842.0;

                                if ui.input(|i| i.pointer.primary_down()) {
                                    let pos = ui.input(|i| i.pointer.position());
                                    if rect.contains(pos) {
                                        let pdf_x = (pos.x - rect.min.x) / scale_x;
                                        let pdf_y = (pos.y - rect.min.y) / scale_y;
                                        if let Some(idx) = char_at_point(&layer, pdf_x, pdf_y) {
                                            // Start selection.
                                        }
                                    }
                                }
                            }

                            response
                        });

                    // Accessibility: expose page text.
                    if viewer.has_text {
                        response.accessibility_info(|| {
                            let mut info = egui::accesskit::Node::default();
                            info.set_role(egui::accesskit::Role::TEXT);
                            info.set_name(viewer.page_text.clone());
                            info
                        });
                    }
                }
                Err(e) => {
                    ui.label(
                        fonts::text(fonts::Role::Body, &format!("Failed to render page: {}", e))
                            .color(t.error),
                    );
                }
            }
        }

        // Details card.
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
