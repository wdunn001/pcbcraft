//! The Datasheet page: the cached PDF read in the app, beside the details known for the part.
//!
//! Context: issues #695 and #678, spike #723. The left side is the reader (`reader.rs`): page
//! navigation, zoom, search with highlights, selectable text and Copy. The right side is the
//! key details and the pinout. The PDF is opened only from the cache: a person fetches it
//! (Get datasheet, one fetch of the part's own link under a size cap and a time limit) or
//! chooses a file; the page never fetches by itself.
//!
//! Local rules: a link that is not http or https is never fetched; a file that is not a PDF is
//! refused with a plain sentence; the cache copy is shown with the date it was stored. Opening
//! the link in the browser is always offered.
//!
//! # Dependencies
//! - crates/ui-egui/src/library_workspace/reader.rs
//! - crates/library/src/datasheet/cache.rs (`DatasheetCache`)
//! - crates/library/src/open_library.rs (`Fetcher`, the transport the app already has)
//!
//! # Do not
//! - Fetch or read a file from a frame other than on a button press.
use super::overview::{key_parameters, value_row};
use super::reader::{Reader, ZOOMS};
use super::*;
use crate::ui_kit::button::{ghost_button, icon_button};
use crate::ui_kit::card::card;
use crate::ui_kit::icons as kit;
use pcbcraft_library::datasheet::cache::{DatasheetCache, Source};
use pcbcraft_library::datasheet::{MAX_PDF_BYTES, check_pdf};
use pcbcraft_library::open_library::Fetcher;
use std::sync::mpsc::Receiver;

/// A fetch running on a worker: the address and where its answer arrives.
pub(crate) struct Download {
    url: String,
    rx: Receiver<Result<Vec<u8>, String>>,
}

impl App {
    /// Hand the Datasheet page a transport for Get datasheet. The desktop app does; without
    /// one the page offers only a chosen file and the browser link.
    pub fn with_datasheet_fetcher(mut self, fetcher: Box<dyn Fetcher>) -> Self {
        self.libws.fetcher = Some(std::sync::Arc::from(fetcher));
        self
    }

    /// Keep cached datasheets in `dir` (tests).
    #[cfg(test)]
    pub(crate) fn use_datasheet_cache(&mut self, dir: std::path::PathBuf) {
        self.libws.ds_cache = Some(DatasheetCache::new(dir));
        self.libws.ds_cache_ready = true;
    }

    /// The cache folder, found the first time it is needed.
    pub(super) fn datasheet_cache(&mut self) -> Option<DatasheetCache> {
        if !self.libws.ds_cache_ready {
            self.libws.ds_cache_ready = true;
            self.libws.ds_cache = DatasheetCache::default_dir().map(DatasheetCache::new);
        }
        self.libws.ds_cache.clone()
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }

    /// Start fetching the part's datasheet on a worker. One press, one fetch.
    fn datasheet_get(&mut self, ctx: &egui::Context, url: &str) {
        let Some(fetcher) = self.libws.fetcher.clone() else {
            self.libws.ds_note = t!("libws.ds.no_connection");
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let (ctx, address) = (ctx.clone(), url.to_owned());
        let spawned = std::thread::Builder::new()
            .name("pcbcraft-datasheet-fetch".into())
            .spawn(move || {
                let got = fetcher
                    .get(&address)
                    .map_err(|e| match e {
                        pcbcraft_library::open_library::FetchError::Offline(w)
                        | pcbcraft_library::open_library::FetchError::Failed(w) => w,
                        pcbcraft_library::open_library::FetchError::NotFound => {
                            "The host has no such file.".to_owned()
                        }
                        pcbcraft_library::open_library::FetchError::RateLimited { .. } => {
                            "The host asked for a pause. Try again in a minute.".to_owned()
                        }
                    })
                    .and_then(|b| check_pdf(&b).map(|()| b).map_err(|e| e.to_string()));
                let _ = tx.send(got);
                ctx.request_repaint();
            });
        match spawned {
            Ok(_) => {
                self.libws.ds_note.clear();
                self.libws.download = Some(Download {
                    url: url.to_owned(),
                    rx,
                });
            }
            Err(e) => self.libws.ds_note = e.to_string(),
        }
    }

    /// Take a finished fetch: store it in the cache and open it.
    fn datasheet_poll_download(&mut self, ctx: &egui::Context) {
        let Some(d) = &self.libws.download else {
            return;
        };
        match d.rx.try_recv() {
            Ok(result) => {
                let url = d.url.clone();
                self.libws.download = None;
                match result {
                    Ok(bytes) => self.datasheet_keep(ctx, &url, bytes, Source::Downloaded),
                    Err(why) => self.libws.ds_note = why,
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.libws.download = None,
        }
    }

    /// Store bytes in the cache (when there is one) and open the reader on them.
    fn datasheet_keep(&mut self, ctx: &egui::Context, url: &str, bytes: Vec<u8>, source: Source) {
        if let Some(cache) = self.datasheet_cache()
            && let Err(e) = cache.put(url, &bytes, source, Self::now_secs())
        {
            self.libws.ds_note = e.to_string();
            return;
        }
        self.libws.reader = Some(Reader::open(ctx, url, bytes));
    }

    /// Read a PDF the person chose and keep it as the datasheet for `url`.
    #[cfg(not(target_arch = "wasm32"))]
    fn datasheet_use_file(&mut self, ctx: &egui::Context, url: &str, path: &str) {
        let read = std::fs::metadata(path)
            .map_err(|e| e.to_string())
            .and_then(|m| {
                if m.len() as usize > MAX_PDF_BYTES {
                    Err(t!("libws.ds.too_big"))
                } else {
                    std::fs::read(path).map_err(|e| e.to_string())
                }
            })
            .and_then(|b| check_pdf(&b).map(|()| b).map_err(|e| e.to_string()));
        match read {
            Ok(bytes) => self.datasheet_keep(ctx, url, bytes, Source::UserFile),
            Err(why) => self.libws.ds_note = why,
        }
    }

    /// The Datasheet page.
    pub(super) fn library_datasheet_page(
        &mut self,
        ui: &mut egui::Ui,
        card_of: &Card,
        f: &PartFacts,
    ) {
        let t = theme::tokens(ui.ctx());
        let ctx = ui.ctx().clone();
        self.datasheet_poll_download(&ctx);
        let url = f.datasheet.clone();
        if self
            .libws
            .reader
            .as_ref()
            .is_some_and(|r| Some(&r.url) != url.as_ref())
        {
            self.libws.reader = None;
        }
        if self.libws.reader.is_none()
            && self.libws.download.is_none()
            && let Some(url) = &url
            && let Some(cache) = self.datasheet_cache()
            && let Some((_, bytes)) = cache.get(url)
        {
            self.libws.reader = Some(Reader::open(&ctx, url, bytes));
        }
        if let Some(r) = self.libws.reader.as_mut() {
            r.poll(&ctx);
        }
        let pins = self.library_card_pins(card_of);
        let stored = url
            .as_ref()
            .and_then(|u| self.datasheet_cache().and_then(|c| c.info(u)));
        let wide = ui.available_width();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(wide * 0.62, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    self.datasheet_left(
                        ui,
                        url.as_deref(),
                        stored.as_ref().map(|d| (d.stored_at, d.source)),
                    )
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.y = theme::SPACE[3];
                    card(
                        ui,
                        &t!("libws.ds.details"),
                        Some(&t!("libws.ds.read_from")),
                        |_| {},
                        |ui| {
                            for (label, value) in key_parameters(f) {
                                value_row(ui, &label, value);
                            }
                        },
                    );
                    card(
                        ui,
                        &t!("libws.ds.pinout"),
                        None,
                        |_| {},
                        |ui| {
                            if pins.is_empty() {
                                ui.label(
                                    fonts::text(fonts::Role::Body, t!("libws.ds.no_pins"))
                                        .color(t.muted),
                                );
                            }
                            for (number, name, kind) in &pins {
                                ui.horizontal(|ui| {
                                    ui.label(fonts::mono(number).color(t.text));
                                    ui.label(fonts::text(fonts::Role::Body, name).color(t.text));
                                    ui.label(fonts::text(fonts::Role::Body, kind).color(t.muted));
                                });
                            }
                        },
                    );
                },
            );
        });
    }

    /// The pins of a part for the pinout card: number, name and kind.
    fn library_card_pins(&self, card: &Card) -> Vec<(String, String, String)> {
        match card {
            Card::Project(d) => d
                .pins
                .iter()
                .filter(|p| p.unit <= 1)
                .map(|p| (p.number.clone(), p.name.clone(), p.electrical_type.clone()))
                .collect(),
            Card::BuiltIn(kind, _) => pcbcraft_doc::component_template(
                kind,
                0,
                String::new(),
                pcbcraft_geom::Point::new(0.0, 0.0),
            )
            .map(|p| {
                p.pins
                    .iter()
                    .map(|q| (q.number.clone(), q.name.clone(), q.electrical_type.clone()))
                    .collect()
            })
            .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// The reader card, or what to do to get a datasheet into it.
    fn datasheet_left(
        &mut self,
        ui: &mut egui::Ui,
        url: Option<&str>,
        stored: Option<(u64, Source)>,
    ) {
        let t = theme::tokens(ui.ctx());
        let ctx = ui.ctx().clone();
        let Some(url) = url else {
            card(
                ui,
                &t!("libws.ds.title"),
                None,
                |_| {},
                |ui| {
                    ui.label(fonts::text(fonts::Role::Body, t!("libws.ds.no_link")).color(t.muted));
                },
            );
            return;
        };
        let reading = self.libws.reader.as_ref().is_some_and(|r| r.info.is_some());
        if !reading {
            card(
                ui,
                &t!("libws.ds.title"),
                None,
                |_| {},
                |ui| {
                    ui.label(fonts::mono(url).color(t.text));
                    if let Some(why) = self.libws.reader.as_ref().and_then(|r| r.failed.clone()) {
                        ui.label(fonts::text(fonts::Role::Body, why).color(t.red));
                    } else if self.libws.download.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                fonts::text(fonts::Role::Body, t!("libws.ds.fetching"))
                                    .color(t.muted),
                            );
                        });
                    } else if self.libws.reader.is_some() {
                        ui.label(
                            fonts::text(fonts::Role::Body, t!("libws.ds.opening")).color(t.muted),
                        );
                    } else {
                        ui.label(
                            fonts::text(fonts::Role::Body, t!("libws.ds.not_cached"))
                                .color(t.muted),
                        );
                    }
                    if !self.libws.ds_note.is_empty() {
                        ui.label(
                            fonts::text(fonts::Role::Body, self.libws.ds_note.clone()).color(t.red),
                        );
                    }
                    ui.horizontal(|ui| {
                        let web = url.starts_with("http://") || url.starts_with("https://");
                        if secondary_button(
                            ui,
                            None,
                            &t!("libws.ds.get"),
                            &t!("libws.ds.get.tip"),
                            Size::Normal,
                            web && self.libws.download.is_none() && self.libws.fetcher.is_some(),
                        )
                        .clicked()
                        {
                            self.datasheet_get(&ctx, url);
                        }
                        if secondary_button(
                            ui,
                            None,
                            &t!("library.dock.datasheet"),
                            &t!("library.dock.datasheet.tip"),
                            Size::Normal,
                            web,
                        )
                        .clicked()
                        {
                            ctx.open_url(egui::OpenUrl::new_tab(url.to_owned()));
                        }
                    });
                    if self.libws.fetcher.is_none() {
                        ui.label(fonts::caption(t!("libws.ds.no_connection")).color(t.muted));
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    ui.horizontal(|ui| {
                        ui.add(
                            crate::targets::text_field(&mut self.libws.ds_path)
                                .hint_text(t!("libws.ds.path_hint"))
                                .desired_width(260.0),
                        )
                        .named(&t!("libws.ds.path"))
                        .on_hover_text(t!("libws.ds.path.tip"));
                        let ready = !self.libws.ds_path.trim().is_empty();
                        if secondary_button(
                            ui,
                            None,
                            &t!("libws.ds.use_file"),
                            &t!("libws.ds.use_file.tip"),
                            Size::Normal,
                            ready,
                        )
                        .clicked()
                        {
                            let path = self.libws.ds_path.trim().to_owned();
                            self.datasheet_use_file(&ctx, url, &path);
                        }
                    });
                },
            );
            return;
        }
        let Some(reader) = self.libws.reader.as_mut() else {
            return;
        };
        reader.refresh_hits();
        card(
            ui,
            &t!("libws.ds.title"),
            None,
            |_| {},
            |ui| {
                Self::reader_bar(ui, reader);
                ui.add_space(4.0);
                egui::ScrollArea::both()
                    .id_salt("library-datasheet-scroll")
                    .max_height((ui.available_height() - 36.0).max(240.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.centered_and_justified(|ui| reader.show_page(ui, &t));
                    });
                let when = stored.map_or_else(String::new, |(at, source)| {
                    let how = match source {
                        Source::Downloaded => t!("libws.ds.downloaded"),
                        Source::UserFile => t!("libws.ds.chosen"),
                    };
                    t!(
                        "libws.ds.cached_line",
                        date = super::facts::date_text(at),
                        how = how
                    )
                });
                let pages = reader.info.as_ref().map_or(0, |i| i.sizes.len());
                ui.label(
                    fonts::caption(format!(
                        "{when} \u{b7} {}",
                        t!("libws.ds.pages", count = pages)
                    ))
                    .color(t.muted),
                );
            },
        );
    }

    /// Page navigation, zoom, search and copy.
    fn reader_bar(ui: &mut egui::Ui, r: &mut Reader) {
        let t = theme::tokens(ui.ctx());
        let pages = r.info.as_ref().map_or(1, |i| i.sizes.len());
        ui.horizontal_wrapped(|ui| {
            if icon_button(
                ui,
                kit::CHEVRON_UP,
                &t!("libws.ds.prev"),
                &t!("libws.ds.prev.tip"),
                Size::Small,
                r.page > 0,
            )
            .clicked()
            {
                r.go_to(r.page - 1);
            }
            let key = ui.id().with("reader-page");
            let mut text: String = ui
                .data_mut(|d| d.get_temp(key))
                .unwrap_or_else(|| (r.page + 1).to_string());
            let shown = (r.page + 1).to_string();
            if !ui.memory(|m| m.focused().is_some()) && text != shown {
                text = shown;
            }
            let resp = ui
                .add(crate::targets::text_field(&mut text).desired_width(44.0))
                .named(&t!("libws.ds.page_field"))
                .on_hover_text(t!("libws.ds.page_field.tip"));
            if resp.lost_focus()
                && let Ok(n) = text.trim().parse::<usize>()
            {
                r.go_to(n.saturating_sub(1));
                text = (r.page + 1).to_string();
            }
            ui.data_mut(|d| d.insert_temp(key, text));
            ui.label(fonts::mono(t!("libws.ds.of", count = pages)).color(t.muted));
            if icon_button(
                ui,
                kit::CHEVRON_DOWN,
                &t!("libws.ds.next"),
                &t!("libws.ds.next.tip"),
                Size::Small,
                r.page + 1 < pages,
            )
            .clicked()
            {
                r.go_to(r.page + 1);
            }
            ui.separator();
            let at = ZOOMS.iter().position(|z| *z == r.zoom).unwrap_or(2);
            if icon_button(
                ui,
                kit::MINUS,
                &t!("libws.ds.zoom_out"),
                &t!("libws.ds.zoom_out.tip"),
                Size::Small,
                at > 0,
            )
            .clicked()
            {
                r.zoom = ZOOMS[at - 1];
            }
            ui.label(fonts::mono(format!("{} %", r.zoom)).color(t.text));
            if icon_button(
                ui,
                kit::ZOOM_IN,
                &t!("libws.ds.zoom_in"),
                &t!("libws.ds.zoom_in.tip"),
                Size::Small,
                at + 1 < ZOOMS.len(),
            )
            .clicked()
            {
                r.zoom = ZOOMS[at + 1];
            }
            if icon_button(
                ui,
                kit::FIT,
                &t!("libws.ds.fit"),
                &t!("libws.ds.fit.tip"),
                Size::Small,
                true,
            )
            .clicked()
            {
                r.zoom = 100;
            }
            ui.separator();
            let search = ui
                .add(
                    crate::targets::text_field(&mut r.query)
                        .hint_text(t!("libws.ds.search_hint"))
                        .desired_width(150.0),
                )
                .named(&t!("libws.ds.search"))
                .on_hover_text(t!("libws.ds.search.tip"));
            let enter = search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            r.refresh_hits();
            if !r.query.trim().is_empty() {
                let count = if r.hits.is_empty() {
                    t!("libws.ds.no_hits")
                } else {
                    t!(
                        "libws.ds.hit_count",
                        at = r.hit_at + 1,
                        count = r.hits.len()
                    )
                };
                ui.label(fonts::mono(count).color(t.muted));
            }
            let has_hits = !r.hits.is_empty();
            if enter && has_hits {
                r.step_hit(true);
            }
            if icon_button(
                ui,
                kit::CHEVRON_UP,
                &t!("libws.ds.prev_hit"),
                &t!("libws.ds.prev_hit.tip"),
                Size::Small,
                has_hits,
            )
            .clicked()
            {
                r.step_hit(false);
            }
            if icon_button(
                ui,
                kit::CHEVRON_DOWN,
                &t!("libws.ds.next_hit"),
                &t!("libws.ds.next_hit.tip"),
                Size::Small,
                has_hits,
            )
            .clicked()
            {
                r.step_hit(true);
            }
            let selected = r.selected_text();
            if ghost_button(
                ui,
                None,
                &t!("libws.ds.copy"),
                &t!("libws.ds.copy.tip"),
                Size::Small,
                selected.is_some(),
            )
            .clicked()
                && let Some(text) = selected
            {
                ui.ctx().copy_text(text);
            }
        });
    }
}
