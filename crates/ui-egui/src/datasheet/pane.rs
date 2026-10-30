//! Floating datasheet window with search, selection, and highlights.

use crate::a11y::{self, Named};
use crate::ui_kit::button::{Size, secondary_button};
use crate::theme;
use egui::{Color32, Context, Id, Pos2, Rect, Sense, TextureHandle, TextureOptions, Vec2};
use pcbcraft_datasheet::{CharBox, DocSearch, Document, Selection, char_at_point, selected_text};
use pcbcraft_text::t;

const SEARCH_MAX: usize = 128;

/// State for the in-app datasheet reader.
pub(crate) struct Pane {
    open: bool,
    title: String,
    doc: Option<Document>,
    page: usize,
    search_draft: String,
    search: DocSearch,
    layer: Vec<CharBox>,
    layer_page: usize,
    layer_error: Option<String>,
    selection: Option<Selection>,
    texture: Option<(usize, TextureHandle)>,
    page_rect: Rect,
    page_response_id: Id,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            open: false,
            title: String::new(),
            doc: None,
            page: 0,
            search_draft: String::new(),
            search: DocSearch::default(),
            layer: Vec::new(),
            layer_page: usize::MAX,
            layer_error: None,
            selection: None,
            texture: None,
            page_rect: Rect::NOTHING,
            page_response_id: Id::new("datasheet-page-unassigned"),
        }
    }
}

impl Pane {
    /// True while the reader window is open.
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Open a PDF from bytes (tests and local files).
    pub(crate) fn open_bytes(&mut self, title: String, bytes: Vec<u8>, ctx: &Context) -> String {
        match Document::open(bytes) {
            Ok(doc) => {
                self.doc = Some(doc);
                self.title = title;
                self.page = 0;
                self.search.clear();
                self.search_draft.clear();
                self.selection = None;
                self.texture = None;
                self.layer.clear();
                self.layer_page = usize::MAX;
                self.layer_error = None;
                self.open = true;
                ctx.request_repaint();
                String::new()
            }
            Err(e) => e,
        }
    }

    /// Try to open a local `file://` PDF; returns true when the pane took the file.
    pub(crate) fn open_file_url(&mut self, url: &str, ctx: &Context) -> bool {
        let Some(path) = url.strip_prefix("file://") else {
            return false;
        };
        let path = path.trim_start_matches('/');
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Datasheet")
            .to_owned();
        self.open_bytes(name, bytes, ctx);
        true
    }

    fn reload_layer(&mut self) {
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
        if self.layer_page == self.page {
            return;
        }
        self.layer_page = self.page;
        self.layer.clear();
        self.layer_error = None;
        self.selection = None;
        match doc.text_layer(self.page) {
            Ok(layer) => self.layer = layer,
            Err(e) => self.layer_error = Some(e),
        }
    }

    fn run_search(&mut self) {
        let draft = self.search_draft.trim();
        if draft.is_empty() {
            self.search.clear();
            return;
        }
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
        match doc.search(draft) {
            Ok(found) => {
                self.page = found.active_hit().map(|h| h.page).unwrap_or(self.page);
                self.search = found;
                self.texture = None;
                self.layer_page = usize::MAX;
            }
            Err(e) => {
                self.search.clear();
                self.layer_error = Some(e);
            }
        }
    }

    fn page_to_screen(&self, rect: Rect, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        let Some(doc) = self.doc.as_ref() else {
            return Rect::NOTHING;
        };
        let Ok((pw, ph)) = doc.page_size(self.page) else {
            return Rect::NOTHING;
        };
        if pw <= 0.0 || ph <= 0.0 {
            return Rect::NOTHING;
        }
        let sx = rect.width() / pw;
        let sy = rect.height() / ph;
        Rect::from_min_max(
            rect.min + Vec2::new(x0 * sx, y0 * sy),
            rect.min + Vec2::new(x1 * sx, y1 * sy),
        )
    }

    fn screen_to_page(&self, rect: Rect, pos: Pos2) -> (f32, f32) {
        let Some(doc) = self.doc.as_ref() else {
            return (0.0, 0.0);
        };
        let Ok((pw, ph)) = doc.page_size(self.page) else {
            return (0.0, 0.0);
        };
        let rel = pos - rect.min;
        let sx = pw / rect.width().max(1.0);
        let sy = ph / rect.height().max(1.0);
        (rel.x * sx, rel.y * sy)
    }

    fn paint_highlights(&self, ui: &egui::Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        let search_col = Color32::from_rgba_unmultiplied(255, 220, 60, 120);
        let active_col = Color32::from_rgba_unmultiplied(255, 140, 0, 160);
        let sel_col = Color32::from_rgba_unmultiplied(80, 160, 255, 140);
        for (i, hit) in self.search.hits.iter().enumerate() {
            if hit.page != self.page {
                continue;
            }
            let r = self.page_to_screen(rect, hit.bounds.x0, hit.bounds.y0, hit.bounds.x1, hit.bounds.y1);
            let col = if i == self.search.active {
                active_col
            } else {
                search_col
            };
            painter.rect_filled(r, 0.0, col);
        }
        if let Some(sel) = self.selection {
            let (lo, hi) = sel.range(self.layer.len());
            for c in &self.layer[lo..=hi] {
                if c.ch.is_whitespace() {
                    continue;
                }
                let r = self.page_to_screen(rect, c.x0, c.y0, c.x1, c.y1);
                painter.rect_filled(r, 0.0, sel_col);
            }
        }
    }

    fn ensure_texture(&mut self, ctx: &Context) {
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
        if self
            .texture
            .as_ref()
            .is_some_and(|(p, _)| *p == self.page)
        {
            return;
        }
        let Ok(bmp) = doc.render(self.page, 120.0) else {
            return;
        };
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [bmp.width as usize, bmp.height as usize],
            &bmp.rgba,
        );
        let handle = ctx.load_texture(
            format!("datasheet-page-{}", self.page),
            image,
            TextureOptions::LINEAR,
        );
        self.texture = Some((self.page, handle));
    }

    fn copy_selection(&self, ctx: &Context) {
        let Some(sel) = self.selection else {
            return;
        };
        let text = selected_text(&self.layer, sel);
        if !text.is_empty() {
            ctx.copy_text(text);
        }
    }

    /// Draw the reader window when open.
    pub(crate) fn ui(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let title = if self.title.is_empty() {
            t!("datasheet.pane.title")
        } else {
            self.title.clone()
        };
        egui::Window::new(title)
            .open(&mut open)
            .default_size([720.0, 560.0])
            .show(ctx, |ui| {
                self.toolbar(ui);
                ui.separator();
                self.page_body(ui, ctx);
            });
        self.open = open;
        if !self.open {
            self.doc = None;
            self.texture = None;
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let pages = self.doc.as_ref().map(|d| d.page_count()).unwrap_or(0);
            if pages > 0 {
                let prev = secondary_button(
                    ui,
                    None,
                    "◀",
                    &t!("datasheet.pane.prev_page.tip"),
                    Size::Small,
                    self.page > 0,
                );
                if prev.clicked() && self.page > 0 {
                    self.page -= 1;
                    self.texture = None;
                    self.layer_page = usize::MAX;
                }
                ui.label(t!("datasheet.pane.page", page = self.page + 1, pages = pages));
                let next = secondary_button(
                    ui,
                    None,
                    "▶",
                    &t!("datasheet.pane.next_page.tip"),
                    Size::Small,
                    self.page + 1 < pages,
                );
                if next.clicked() && self.page + 1 < pages {
                    self.page += 1;
                    self.texture = None;
                    self.layer_page = usize::MAX;
                }
            }
            ui.separator();
            let search = ui
                .add(
                    egui::TextEdit::singleline(&mut self.search_draft)
                        .hint_text(t!("datasheet.pane.search"))
                        .desired_width(180.0),
                )
                .named(t!("datasheet.pane.search"));
            if search.changed() && self.search_draft.len() > SEARCH_MAX {
                self.search_draft.truncate(SEARCH_MAX);
            }
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) && search.has_focus() {
                self.run_search();
            }
            if secondary_button(
                ui,
                None,
                &t!("datasheet.pane.find"),
                &t!("datasheet.pane.search.tip"),
                Size::Small,
                !self.search_draft.trim().is_empty(),
            )
            .clicked()
            {
                self.run_search();
            }
            let count = self.search.count();
            if count > 0 {
                ui.label(t!("datasheet.pane.matches", count = count));
                if secondary_button(
                    ui,
                    None,
                    &t!("datasheet.pane.prev"),
                    &t!("datasheet.pane.prev.tip"),
                    Size::Small,
                    true,
                )
                .clicked()
                    && let Some(p) = self.search.step(-1)
                {
                    self.page = p;
                    self.texture = None;
                    self.layer_page = usize::MAX;
                }
                if secondary_button(
                    ui,
                    None,
                    &t!("datasheet.pane.next"),
                    &t!("datasheet.pane.next.tip"),
                    Size::Small,
                    true,
                )
                .clicked()
                    && let Some(p) = self.search.step(1)
                {
                    self.page = p;
                    self.texture = None;
                    self.layer_page = usize::MAX;
                }
            } else if !self.search.query.is_empty() {
                ui.label(t!("datasheet.pane.no_matches"));
            }
        });
    }

    fn page_body(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        self.reload_layer();
        if let Some(err) = self.layer_error.as_ref() {
            ui.colored_label(theme::tokens(ui.ctx()).red, err);
        }
        let has_text = pcbcraft_datasheet::page_has_text(&self.layer);
        if !has_text && self.layer_error.is_none() {
            ui.label(t!("datasheet.pane.no_text"));
        }
        if let Some(doc) = self.doc.as_ref() {
            if let Ok(label) = doc.page_plain_text(self.page) {
                if !label.is_empty() {
                    ui.ctx().accesskit_node_builder(self.page_response_id, |node| {
                        node.set_label(label);
                    });
                }
            }
        }
        self.ensure_texture(ctx);
        let tex = self.texture.as_ref().map(|(_, h)| h.clone());
        let available = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
        self.page_rect = rect;
        self.page_response_id = response.id;
        if let Some(tex) = tex {
            ui.painter().image(
                tex.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        self.paint_highlights(ui, rect);
        if ui.input(|i| i.key_pressed(egui::Key::C) && i.modifiers.command) {
            self.copy_selection(ctx);
        }
        if response.drag_started() {
            let (x, y) = self.screen_to_page(rect, response.interact_pointer_pos().unwrap_or(rect.min));
            if let Some(idx) = char_at_point(&self.layer, x, y) {
                self.selection = Some(Selection {
                    anchor: idx,
                    focus: idx,
                });
            }
        }
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let (x, y) = self.screen_to_page(rect, pos);
                if let Some(focus) = char_at_point(&self.layer, x, y) {
                    if let Some(sel) = &mut self.selection {
                        sel.focus = focus;
                    }
                }
            }
        }
        if response.clicked() {
            let (x, y) = self.screen_to_page(rect, response.interact_pointer_pos().unwrap_or(rect.min));
            if let Some(idx) = char_at_point(&self.layer, x, y) {
                self.selection = Some(Selection {
                    anchor: idx,
                    focus: idx,
                });
            }
        }
        if response.double_clicked() {
            self.copy_selection(ctx);
        }
        a11y::name_control(&response, &t!("datasheet.pane.page_area"), None);
    }

    #[cfg(test)]
    pub(crate) fn open_fixture(
        &mut self,
        ctx: &Context,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        let err = self.open_bytes("Fixture".into(), bytes, ctx);
        if err.is_empty() {
            Ok(())
        } else {
            Err(err)
        }
    }

    #[cfg(test)]
    pub(crate) fn search_in_ui(&mut self, needle: &str) {
        self.search_draft = needle.to_owned();
        self.run_search();
    }

    #[cfg(test)]
    pub(crate) fn match_count(&self) -> usize {
        self.search.count()
    }

    #[cfg(test)]
    pub(crate) fn select_all_on_page(&mut self) {
        if self.layer.is_empty() {
            return;
        }
        self.selection = Some(Selection {
            anchor: 0,
            focus: self.layer.len().saturating_sub(1),
        });
    }

    #[cfg(test)]
    pub(crate) fn copied_selection(&self) -> String {
        self.selection
            .map(|sel| selected_text(&self.layer, sel))
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn page_shows_no_text_message(&self) -> bool {
        !pcbcraft_datasheet::page_has_text(&self.layer)
    }
}
