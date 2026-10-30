//! Part sources in the library list: remote results appear beside local parts
//! as cards with a source badge, stock, a price break and the source's credit.
//! The one source today is the open library, behind `OpenLibrarySource`: it
//! lists parts (`PartSource`) and imports them (`CadSource`).
//!
//! # Rules
//! - Runtime reads are cache-only until the live transport lands: `CacheOnly`
//!   never touches the network. Tests inject a recorded fetcher through
//!   `App::use_open_source`.
//!
//! # Dependencies
//! - crates/library/src/open_library.rs
//! - crates/library/src/cad_cache.rs
//! - crates/library/src/sources.rs
use super::*;
use pcbcraft_library::cad::CadSource;
use pcbcraft_library::cad_cache::{CadCache, default_cache_dir};
use pcbcraft_library::open_library::{CardScope, FetchError, Fetcher, OpenLibrarySource};
use pcbcraft_library::sources::{PartResult, PartSource};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;

/// The transport used until the live one lands: it answers nothing, so only
/// files already kept on this computer are served.
///
/// Contract: crates/library/src/open_library.rs (Fetcher)
pub(super) struct CacheOnly;

/// Contract: crates/library/src/open_library.rs
impl Fetcher for CacheOnly {
    fn get(&self, _url: &str) -> Result<Vec<u8>, FetchError> {
        Err(FetchError::Offline(
            "Online parts are read from this computer only for now.".into(),
        ))
    }
}

/// An online search running on a worker.
struct Search {
    query: String,
    scope: CardScope,
    rx: Receiver<Result<Value, String>>,
}

#[derive(Default)]
pub(super) struct PartSources {
    source: Option<Arc<OpenLibrarySource>>,
    /// Set by the desktop app so the Offline toggle can stop all requests.
    offline: Option<Arc<AtomicBool>>,
    /// The search in flight, and the last answer with the query it was for.
    search: Option<Search>,
    answer: Option<(String, Result<Vec<PartResult>, String>)>,
    /// What the last answer was asked for, so switching Symbols and Footprints asks again.
    answer_scope: Option<CardScope>,
    notice: String,
}

impl PartSources {
    /// The open source, when it is switched on.
    pub(super) fn open(&self) -> Option<Arc<OpenLibrarySource>> {
        self.source.clone()
    }
}

/// One plain line about a result's stock.
pub(super) fn stock_line(part: &PartResult) -> String {
    let stock = if part.stock == 0 {
        "out of stock".to_string()
    } else {
        format!("{} in stock", part.stock)
    };
    let price = part.price_breaks.first().map(|b| {
        format!(
            "from {:.2} {} each at {}",
            b.unit_price, b.currency, b.quantity
        )
    });
    match price {
        Some(price) => format!("{stock} · {price}"),
        None => stock,
    }
}

impl App {
    /// The remote half of the part list. Local parts are drawn by the caller.
    /// The online part source behind the Online toggle: the sample source
    /// until a live one is wired in, asked for the typed query. Returns the
    /// results with the source's credit line, and draws the notes a person
    /// needs when there are none (source off, nothing typed, error).
    ///
    /// # Rules
    /// - Reads go through the cached source; nothing is fetched without a query.
    /// - In the browser build part sources are not available.
    pub(super) fn online_parts(&mut self, ui: &mut egui::Ui) -> Vec<(PartResult, String)> {
        let muted = self.muted();
        let note = |ui: &mut egui::Ui, text: &str| {
            ui.label(egui::RichText::new(text).small().color(muted));
        };
        #[cfg(target_arch = "wasm32")]
        {
            note(ui, "Part sources are available in the desktop app.");
            Vec::new()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.part_sources.source.is_none() {
                note(
                    ui,
                    "Part sources are off. Only parts on this computer are listed.",
                );
                if ui
                    .button("Turn on Open library")
                    .on_hover_text(
                        "Search the open symbol and footprint libraries. Parts you have opened before work offline. No account needed.",
                    )
                    .clicked()
                {
                    self.use_open_source(Box::new(CacheOnly), None);
                }
                if !self.part_sources.notice.is_empty() {
                    ui.label(egui::RichText::new(&self.part_sources.notice).small());
                }
                return Vec::new();
            }
            let query = self.part_search.trim().to_owned();
            let scope = match self.dock.view {
                library_dock::ViewKind::Symbols => CardScope::Symbols,
                library_dock::ViewKind::Footprints => CardScope::Footprints,
                library_dock::ViewKind::Both => CardScope::Both,
            };
            if query.chars().count() < 2 {
                note(ui, "Type a part number or keyword to search the sources.");
                return Vec::new();
            }
            let Some(source) = self.part_sources.source.clone() else {
                return Vec::new();
            };
            // Collect a finished search, then start the next one when the box changed.
            if let Some(search) = self.part_sources.search.take() {
                match search.rx.try_recv() {
                    Ok(result) => {
                        let parts = result.and_then(|v| {
                            serde_json::from_value::<Vec<PartResult>>(v).map_err(|e| e.to_string())
                        });
                        self.part_sources.answer = Some((search.query, parts));
                        self.part_sources.answer_scope = Some(search.scope);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        self.part_sources.search = Some(search);
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
                }
            }
            let answered = self.part_sources.answer_scope == Some(scope)
                && self
                    .part_sources
                    .answer
                    .as_ref()
                    .is_some_and(|(q, _)| *q == query);
            if !answered && self.part_sources.search.is_none() {
                let (worker, asked) = (source.clone(), query.clone());
                let reply = library_dock::spawn_read(ui.ctx(), move || {
                    worker
                        .search_cards(scope, &asked)
                        .map_err(|e| e.to_string())
                        .and_then(|p| serde_json::to_value(p).map_err(|e| e.to_string()))
                });
                self.part_sources.search = Some(Search {
                    query,
                    scope,
                    rx: reply,
                });
            }
            if self.part_sources.search.is_some() {
                note(ui, "Searching the open library...");
            }
            let name = PartSource::name(source.as_ref()).to_owned();
            let credit = format!(
                "{}. {}",
                source.license().name,
                CadSource::attribution(source.as_ref()).text
            );
            match &self.part_sources.answer {
                None => Vec::new(),
                Some((_, Err(e))) => {
                    note(ui, e);
                    Vec::new()
                }
                Some((q, Ok(parts))) if parts.is_empty() => {
                    note(ui, &format!("{name} has nothing for \"{q}\"."));
                    Vec::new()
                }
                Some((_, Ok(parts))) => parts.iter().map(|p| (p.clone(), credit.clone())).collect(),
            }
        }
    }
    /// Switch the open source on over `fetcher`, keeping files in `cache_dir`
    /// (the default folder when `None`, memory when there is none).
    pub(super) fn use_open_source(
        &mut self,
        fetcher: Box<dyn Fetcher>,
        cache_dir: Option<std::path::PathBuf>,
    ) {
        let cache = match cache_dir.or_else(default_cache_dir) {
            Some(dir) => CadCache::open(&dir),
            None => CadCache::in_memory(),
        };
        self.part_sources.source = Some(Arc::new(OpenLibrarySource::new(fetcher, cache)));
        self.part_sources.search = None;
        self.part_sources.answer = None;
        self.part_sources.answer_scope = None;
    }
    /// Hand the open source a live transport and its offline switch; the
    /// Online source is then on from the start. Called by the desktop app.
    pub fn with_online_fetcher(
        mut self,
        fetcher: Box<dyn Fetcher>,
        offline: Arc<AtomicBool>,
    ) -> Self {
        self.use_open_source(fetcher, None);
        self.part_sources.offline = Some(offline);
        self
    }
    /// True when no online search is wanted or the one wanted has answered;
    /// a `--capture` image waits for it.
    pub(super) fn online_settled(&self) -> bool {
        if self.dock_capture_placing() && self.frames < 900 {
            return false;
        }
        let query = self.part_search.trim();
        self.part_sources.source.is_none()
            || query.chars().count() < 2
            || (self.part_sources.search.is_none()
                && self.part_sources.answer_scope.is_some()
                && self.part_sources.answer.as_ref().is_some_and(|(q, parts)| {
                    q == query
                        && (parts.as_ref().is_ok_and(Vec::is_empty)
                            || parts.is_err()
                            || self.dock_online_read())
                }))
    }
    /// The Offline and Clear online cache items of the filter overflow menu.
    pub(super) fn online_menu(&mut self, ui: &mut egui::Ui) {
        if let Some(flag) = self.part_sources.offline.clone() {
            let off = flag.load(Ordering::SeqCst);
            if ui
                .selectable_label(off, t!("library.dock.offline"))
                .on_hover_text(t!("library.dock.offline.tip"))
                .clicked()
            {
                flag.store(!off, Ordering::SeqCst);
                self.part_sources.answer = None;
                ui.close();
            }
        }
        if let Some(source) = self.part_sources.source.clone()
            && ui
                .button(t!("library.dock.clear_cache"))
                .on_hover_text(t!("library.dock.clear_cache.tip"))
                .clicked()
        {
            self.message = match source.cache().purge() {
                Ok(()) => t!("library.dock.cache_cleared"),
                Err(e) => e.to_string(),
            };
            self.part_sources.answer = None;
            ui.close();
        }
        if self.part_sources.source.is_some() {
            let mut on = self.preferences.fetch_open_library_3d;
            if ui
                .checkbox(&mut on, t!("library.dock.fetch_open_3d"))
                .on_hover_text(t!("library.dock.fetch_open_3d.tip"))
                .changed()
            {
                self.preferences.fetch_open_library_3d = on;
                ui.close();
            }
        }
    }
    /// After a source part is placed, write its maker and numbers onto the
    /// new component in one step.
    pub(super) fn attach_pending_source(&mut self, component: Id) {
        let Some(part) = self.pending_source.take() else {
            return;
        };
        if !matches!(self.tool, Tool::Place(_)) {
            return;
        }
        self.run(
            "part.attach_source",
            json!({"id":component,"manufacturer":part.manufacturer,"mpn":part.mpn,
                   "datasheet":part.datasheet_url,"distributor":part.distributor,"sku":part.sku}),
        );
    }
}
