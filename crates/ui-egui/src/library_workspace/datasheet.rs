//! The Datasheet page: the datasheet link, the key details and a PDF reader with search,
//! selectable text and a spoken description of the page.
//!
//! Context: issues #695 and #713. The page keeps what it always showed (the link, the key
//! details and a button that opens the datasheet in the browser) and adds a reader beneath.
//! The reader draws one page at a time, steps through pages, finds words across the whole
//! file, lets a person drag over text and copy it, and says plainly when a page has no text.
//!
//! Local rules: nothing is fetched and no file is read while painting. A PDF opens only when
//! the person presses Read here on a local PDF address, or from bytes handed to `open`. With
//! no document the page shows the link and the details only.
//!
//! # Dependencies
//! - crates/datasheet/src/lib.rs (`Document`, `DocSearch`, `Selection`, `char_at_point`,
//!   `selected_text`, `page_has_text`, `plain_text`)
//! - crates/ui-egui/src/library_workspace/overview.rs (`key_parameters`, `value_row`)
//! - crates/ui-egui/src/ui_kit/button.rs (`secondary_button`)
//! - crates/ui-egui/src/ui_kit/card.rs (`card`)
//! - crates/ui-egui/src/theme.rs (`Tokens`, `SPACE`)
//! - crates/text/strings/en.txt (`libws.ds.*`)
//!
//! # Do not
//! - Download, read or cache a PDF from a frame.
//! - Put a raw colour or font size here; use the theme tokens and the font roles.
use std::collections::HashMap;

use pcbcraft_datasheet::{
    CharBox, DocSearch, Document, Selection, char_at_point, page_has_text, plain_text,
    selected_text,
};

use super::datasheet_values::{ValuesCache, target};
use super::overview::{key_parameters, value_row};
use super::*;
use crate::a11y::{self, Named};
use crate::ui_kit::card::card;

/// Resolution a page is drawn at, in dots per inch.
const PAGE_DPI: f32 = 120.0;
/// How many page pictures stay in memory.
const TEXTURE_KEEP: usize = 6;
/// Longest search text, in characters.
const SEARCH_MAX: usize = 128;
/// Width of the search box.
const SEARCH_WIDTH: f32 = 180.0;

/// The reader: the open document, the page on show and everything cached for it.
#[derive(Default)]
pub(crate) struct ViewerState {
    /// The open document, when there is one.
    pub(crate) doc: Option<Document>,
    /// The datasheet address the document was read from; another address closes it.
    pub(crate) owner: String,
    /// The page on show, counted from zero.
    pub(crate) page: usize,
    /// The words typed in the search box.
    pub(crate) query: String,
    /// The last search result.
    pub(crate) search: DocSearch,
    /// True once a search ran for the words in the box.
    pub(crate) searched: bool,
    /// Text layers read so far, by page.
    pub(crate) layers: HashMap<usize, Vec<CharBox>>,
    /// Page pictures drawn so far, by page; `None` marks a page that would not draw.
    pub(crate) textures: HashMap<usize, Option<egui::TextureHandle>>,
    /// The dragged-over text: the page it is on and the span.
    pub(crate) selection: Option<(usize, Selection)>,
    /// What went wrong last, in the reader's own words.
    pub(crate) error: Option<String>,
    /// The values read from the datasheet, decoded from the project's extraction files.
    pub(crate) values: ValuesCache,
}

impl ViewerState {
    /// Open a PDF from bytes the person supplied.
    pub(crate) fn open(&mut self, source: &str, bytes: Vec<u8>) -> Result<(), String> {
        let doc = Document::open(bytes)?;
        self.close();
        self.doc = Some(doc);
        source.clone_into(&mut self.owner);
        Ok(())
    }

    /// Close the document and forget everything drawn from it.
    pub(crate) fn close(&mut self) {
        *self = Self::default();
    }

    /// True while a document is open.
    pub(crate) fn is_open(&self) -> bool {
        self.doc.is_some()
    }

    /// Show another page and drop the text selection.
    pub(crate) fn go_to(&mut self, page: usize) {
        let last = self
            .doc
            .as_ref()
            .map_or(0, |d| d.page_count().saturating_sub(1));
        self.page = page.min(last);
        self.selection = None;
        self.error = None;
    }

    /// Search the whole document for the words in the box and show the first result.
    pub(crate) fn run_search(&mut self) {
        let words = self.query.trim().to_owned();
        self.selection = None;
        if words.is_empty() {
            self.search.clear();
            self.searched = false;
            return;
        }
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
        match doc.search(&words) {
            Ok(found) => {
                let first = found.active_hit().map(|h| h.page);
                self.search = found;
                self.searched = true;
                self.error = None;
                if let Some(page) = first {
                    self.go_to(page);
                }
            }
            Err(e) => {
                self.search.clear();
                self.searched = false;
                self.error = Some(e);
            }
        }
    }

    /// Step to the next result (`1`) or the previous one (`-1`) and show its page.
    pub(crate) fn step_result(&mut self, delta: isize) {
        if let Some(page) = self.search.step(delta) {
            self.go_to(page);
        }
    }

    /// The words under the selection, in reading order.
    pub(crate) fn selected(&self) -> String {
        match (self.selection, self.layers.get(&self.page)) {
            (Some((page, sel)), Some(layer)) if page == self.page => selected_text(layer, sel),
            _ => String::new(),
        }
    }

    /// Read the text layer of a page once.
    fn ensure_layer(&mut self, page: usize) {
        if self.layers.contains_key(&page) {
            return;
        }
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
        match doc.text_layer(page) {
            Ok(layer) => {
                self.layers.insert(page, layer);
            }
            Err(e) => {
                self.layers.insert(page, Vec::new());
                self.error = Some(e);
            }
        }
    }

    /// Draw a page to a picture once and keep it.
    fn ensure_texture(&mut self, ctx: &egui::Context, page: usize) {
        if self.textures.contains_key(&page) {
            return;
        }
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
        if self.textures.len() >= TEXTURE_KEEP {
            self.textures.clear();
        }
        match doc.render(page, PAGE_DPI) {
            Ok(bitmap) => {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [bitmap.width as usize, bitmap.height as usize],
                    &bitmap.rgba,
                );
                let handle = ctx.load_texture(
                    format!("library-datasheet-page-{page}"),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                self.textures.insert(page, Some(handle));
            }
            Err(e) => {
                self.textures.insert(page, None);
                self.error = Some(e);
            }
        }
    }

    /// Read the local PDF the person asked for and open it.
    fn read_local(&mut self, url: &str) {
        let Some((_, bytes)) = crate::datasheet::read_file_url(url) else {
            self.error = Some(t!("libws.ds.read_failed"));
            return;
        };
        if let Err(e) = self.open(url, bytes) {
            self.error = Some(t!("libws.ds.open_failed", reason = e));
        }
    }

    /// The row with the page controls and the copy button.
    fn page_bar(&mut self, ui: &mut egui::Ui, pages: usize, t: &theme::Tokens) {
        ui.horizontal_wrapped(|ui| {
            if secondary_button(
                ui,
                None,
                &t!("libws.ds.prev_page"),
                &t!("libws.ds.prev_page.tip"),
                Size::Small,
                self.page > 0,
            )
            .clicked()
            {
                self.go_to(self.page.saturating_sub(1));
            }
            ui.label(
                fonts::text(
                    fonts::Role::Body,
                    t!("libws.ds.page_of", page = self.page + 1, pages = pages),
                )
                .color(t.muted),
            );
            if secondary_button(
                ui,
                None,
                &t!("libws.ds.next_page"),
                &t!("libws.ds.next_page.tip"),
                Size::Small,
                self.page + 1 < pages,
            )
            .clicked()
            {
                self.go_to(self.page + 1);
            }
            let chosen = self.selected();
            if secondary_button(
                ui,
                None,
                &t!("libws.ds.copy"),
                &t!("libws.ds.copy.tip"),
                Size::Small,
                !chosen.is_empty(),
            )
            .clicked()
            {
                ui.ctx().copy_text(chosen);
            }
        });
    }

    /// The row with the search box, Find, the result count and the result steppers.
    fn search_bar(&mut self, ui: &mut egui::Ui, t: &theme::Tokens) {
        ui.horizontal_wrapped(|ui| {
            let field = ui
                .add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(t!("libws.ds.search"))
                        .desired_width(SEARCH_WIDTH),
                )
                .named(&t!("libws.ds.search"))
                .on_hover_text(t!("libws.ds.search.tip"));
            if field.changed() && self.query.chars().count() > SEARCH_MAX {
                self.query = self.query.chars().take(SEARCH_MAX).collect();
            }
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let find = secondary_button(
                ui,
                None,
                &t!("libws.ds.find"),
                &t!("libws.ds.find.tip"),
                Size::Small,
                !self.query.trim().is_empty(),
            );
            if find.clicked() || enter {
                self.run_search();
            }
            let count = self.search.count();
            if count > 0 {
                ui.label(
                    fonts::text(
                        fonts::Role::Body,
                        t!(
                            "libws.ds.result_of",
                            n = self.search.active + 1,
                            total = count
                        ),
                    )
                    .color(t.muted),
                );
                if secondary_button(
                    ui,
                    None,
                    &t!("libws.ds.prev"),
                    &t!("libws.ds.prev.tip"),
                    Size::Small,
                    true,
                )
                .clicked()
                {
                    self.step_result(-1);
                }
                if secondary_button(
                    ui,
                    None,
                    &t!("libws.ds.next"),
                    &t!("libws.ds.next.tip"),
                    Size::Small,
                    true,
                )
                .clicked()
                {
                    self.step_result(1);
                }
            } else if self.searched {
                ui.label(fonts::text(fonts::Role::Body, t!("libws.ds.no_results")).color(t.muted));
            }
        });
    }

    /// The page picture with the search marks, the selection and the drag handling.
    fn page_body(&mut self, ui: &mut egui::Ui, t: &theme::Tokens) {
        let page = self.page;
        self.ensure_layer(page);
        self.ensure_texture(ui.ctx(), page);
        let Some((pw, ph)) = self.doc.as_ref().and_then(|d| d.page_size(page).ok()) else {
            return;
        };
        if pw <= 0.0 || ph <= 0.0 {
            return;
        }
        let has_text = self.layers.get(&page).is_some_and(|l| page_has_text(l));
        if !has_text {
            ui.label(fonts::text(fonts::Role::Body, t!("libws.ds.no_text")).color(t.muted));
        }
        let width = ui.available_width();
        let size = egui::vec2(width, width * ph / pw);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let to_page = |pos: egui::Pos2| {
            let rel = pos - rect.min;
            (
                rel.x * pw / rect.width().max(1.0),
                rel.y * ph / rect.height().max(1.0),
            )
        };
        let to_screen = |x0: f32, y0: f32, x1: f32, y1: f32| {
            let (sx, sy) = (rect.width() / pw, rect.height() / ph);
            egui::Rect::from_min_max(
                rect.min + egui::vec2(x0 * sx, y0 * sy),
                rect.min + egui::vec2(x1 * sx, y1 * sy),
            )
        };
        if let Some(Some(tex)) = self.textures.get(&page) {
            ui.painter().image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        let painter = ui.painter_at(rect);
        for (i, hit) in self.search.hits.iter().enumerate() {
            if hit.page != page {
                continue;
            }
            let fill = if i == self.search.active {
                t.copper.gamma_multiply(0.6)
            } else {
                t.gold.gamma_multiply(0.5)
            };
            let b = &hit.bounds;
            painter.rect_filled(to_screen(b.x0, b.y0, b.x1, b.y1), 0.0, fill);
        }
        if let (Some((sel_page, sel)), Some(layer)) = (self.selection, self.layers.get(&page))
            && sel_page == page
            && !layer.is_empty()
        {
            let (lo, hi) = sel.range(layer.len());
            for c in layer[lo..=hi].iter().filter(|c| !c.ch.is_whitespace()) {
                painter.rect_filled(
                    to_screen(c.x0, c.y0, c.x1, c.y1),
                    0.0,
                    t.cyan.gamma_multiply(0.4),
                );
            }
        }
        if has_text {
            let layer = self.layers.get(&page).map(Vec::as_slice).unwrap_or(&[]);
            if response.drag_started()
                && let Some(origin) = ui.input(|i| i.pointer.press_origin())
            {
                let (x, y) = to_page(origin);
                self.selection = char_at_point(layer, x, y).map(|at| {
                    (
                        page,
                        Selection {
                            anchor: at,
                            focus: at,
                        },
                    )
                });
            }
            if response.dragged()
                && let Some(pos) = response.interact_pointer_pos()
                && let Some((_, sel)) = self.selection.as_mut()
            {
                let (x, y) = to_page(pos);
                if let Some(at) = char_at_point(layer, x, y) {
                    sel.focus = at;
                }
            }
        }
        if response.clicked() {
            self.selection = None;
        }
        let layer = self.layers.get(&page);
        ui.ctx().accesskit_node_builder(response.id, |node| {
            if let Some(layer) = layer {
                node.set_description(plain_text(layer));
            }
        });
        a11y::name_control(&response, &t!("libws.ds.page_area"), None);
        response.on_hover_text(t!("libws.ds.page_area.tip"));
    }

    /// The reader card: controls above, the page below.
    fn show(&mut self, ui: &mut egui::Ui, t: &theme::Tokens) {
        let Some(pages) = self.doc.as_ref().map(Document::page_count) else {
            return;
        };
        let mut close = false;
        card(
            ui,
            &t!("libws.ds.reader"),
            None,
            |ui| {
                close = secondary_button(
                    ui,
                    None,
                    &t!("libws.ds.close"),
                    &t!("libws.ds.close.tip"),
                    Size::Small,
                    true,
                )
                .clicked();
            },
            |ui| {
                self.page_bar(ui, pages, t);
                self.search_bar(ui, t);
                if let Some(problem) = self.error.clone() {
                    ui.label(
                        fonts::text(fonts::Role::Body, t!("libws.ds.problem", reason = problem))
                            .color(t.red),
                    );
                }
                ui.add_space(theme::SPACE[1]);
                self.page_body(ui, t);
            },
        );
        if close {
            self.close();
        }
    }
}

impl App {
    /// The Datasheet page: the link, the key details, then the reader when a PDF is open.
    pub(super) fn library_datasheet_page(&mut self, ui: &mut egui::Ui, f: &PartFacts) {
        let t = theme::tokens(ui.ctx());
        let viewer = &mut self.libws.datasheet;
        if viewer.is_open() && f.datasheet.as_deref() != Some(viewer.owner.as_str()) {
            viewer.close();
        }
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
                        let lower = url.to_ascii_lowercase();
                        let local_pdf = lower.starts_with("file://") && lower.ends_with(".pdf");
                        if !viewer.is_open()
                            && secondary_button(
                                ui,
                                None,
                                &t!("libws.ds.read"),
                                &t!("libws.ds.read.tip"),
                                Size::Normal,
                                local_pdf,
                            )
                            .clicked()
                        {
                            viewer.read_local(url);
                        }
                    }
                    None => {
                        ui.label(
                            fonts::text(fonts::Role::Body, t!("libws.ds.no_link")).color(t.muted),
                        );
                    }
                }
                if !viewer.is_open()
                    && let Some(problem) = viewer.error.clone()
                {
                    ui.label(
                        fonts::text(fonts::Role::Body, t!("libws.ds.problem", reason = problem))
                            .color(t.red),
                    );
                }
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
        let groups = viewer.values.groups(&self.project.files, f).to_vec();
        let mut jump = None;
        if !groups.is_empty() {
            card(
                ui,
                &t!("libws.ds.values"),
                None,
                |_| {},
                |ui| {
                    for group in &groups {
                        ui.label(
                            fonts::text(fonts::Role::Body, group.title.clone()).color(t.muted),
                        );
                        ui.horizontal_wrapped(|ui| {
                            for row in &group.rows {
                                let place = target(&row.source);
                                let tip = if place.is_some() {
                                    t!("libws.ds.value_button.tip")
                                } else {
                                    t!("libws.ds.value_nosource.tip")
                                };
                                let pressed = secondary_button(
                                    ui,
                                    None,
                                    &row.label,
                                    &tip,
                                    Size::Small,
                                    place.is_some(),
                                )
                                .clicked();
                                if pressed {
                                    jump = place;
                                }
                            }
                        });
                    }
                },
            );
        }
        viewer.show(ui, &t);
        if let Some((page, rect)) = jump {
            self.show_value_source(ui.ctx(), f.datasheet.as_deref(), page, rect);
        }
        self.library_manufacturer_page(ui, f);
    }
}

impl App {
    /// Open the datasheet reader window on a page and flash the rectangle a value came from.
    ///
    /// # Rules
    /// - A reader window with no document first reads the part's own datasheet, which must be a
    ///   local PDF; with no way to open it the person is told so and nothing else changes.
    fn show_value_source(
        &mut self,
        ctx: &egui::Context,
        datasheet: Option<&str>,
        page: usize,
        rect: [f32; 4],
    ) {
        if !self.datasheet_pane.has_document() {
            let opened = datasheet.is_some_and(|url| self.datasheet_pane.open_file_url(url, ctx));
            if !opened {
                self.message = t!("libws.ds.source_unavailable");
                return;
            }
        }
        let now = ctx.input(|i| i.time);
        self.datasheet_pane.show_source(page, rect, now);
    }
}

impl App {
    /// Manufacturer page and import for user-downloaded Samtec files.
    pub(super) fn library_manufacturer_page(&mut self, ui: &mut egui::Ui, f: &PartFacts) {
        if f.manufacturer != "Samtec" || f.mpn.is_empty() {
            return;
        }
        let t = theme::tokens(ui.ctx());
        let url = match pcbcraft_library::samtec::manufacturer_page_url(&f.mpn) {
            Ok(u) => u,
            Err(_) => return,
        };
        card(
            ui,
            &t!("libws.manufacturer.title"),
            None,
            |_| {},
            |ui| {
                ui.label(fonts::mono(&url).color(t.text));
                if secondary_button(
                    ui,
                    None,
                    &t!("library.dock.manufacturer_page"),
                    &t!("library.dock.manufacturer_page.tip"),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
                ui.separator();
                ui.label(
                    fonts::text(fonts::Role::Body, t!("libws.manufacturer.drop_tip"))
                        .color(t.muted),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        crate::targets::text_field(&mut self.libws.manufacturer_path)
                            .hint_text(t!("libws.manufacturer.path_hint"))
                            .desired_width(300.0),
                    )
                    .named(&t!("libws.manufacturer.path_hint"))
                    .on_hover_text(t!("libws.manufacturer.path_tip"));
                    let path = self.libws.manufacturer_path.trim().to_owned();
                    if secondary_button(
                        ui,
                        None,
                        &t!("libws.manufacturer.import"),
                        &t!("libws.manufacturer.import.tip"),
                        Size::Normal,
                        !path.is_empty(),
                    )
                    .clicked()
                    {
                        let params = json!({
                            "manufacturer": "samtec",
                            "mpn": f.mpn,
                            "path": path
                        });
                        self.run("library.import_manufacturer_file", params);
                    }
                });
            },
        );
    }
}
