//! Distributor part sources in the Library dock, built from the keys a person stored.
//! Today the one distributor is Digi-Key. The dock asks every configured source
//! beside the open library, and a setup card in the dock takes the person's own key.
//!
//! # Rules
//! - A source exists only while both of its stored values are non-empty. With no key the
//!   dock builds no source and shows no error.
//! - The key is read and written through the `KeyStore` the host gave the window. A project
//!   file never holds it.
//! - One source object lives as long as its keys do, so its request pacing carries across
//!   searches. Every search goes through the dated answer cache (`PartSourceCache`): a fresh
//!   answer is reused, an old one is asked again, and each answer shows the date it was taken.
//! - The cache folder comes from the app's own cache location; with none, answers stay in memory.
//!   The desktop build only: the browser build has no distributor sources at all.
//! - Saving or removing the key empties the cache, so one account's answers never reach another.
//! - Test key runs against the typed values in a throwaway store and saves nothing.
//! - The secret field is masked, is emptied after Save and is never logged.
//! - The Offline switch stops every request here and answers from the saved answers instead,
//!   with their date; a search never made says there is nothing saved.
//!
//! # Dependencies
//! - crates/library/src/credentials.rs
//! - crates/library/src/digikey.rs
//! - crates/library/src/sources.rs
//! - crates/engine/src/source_cache.rs
//! - crates/ui-egui/src/sources.rs
//!
//! # Do not
//! - Derive `Debug` on the card state; it holds a secret while it is typed.
use super::*;
use crate::library_workspace::facts::date_text;
use pcbcraft_engine::source_cache::{CachedSearch, PartSourceCache, default_response_cache_dir};
use pcbcraft_library::credentials::{KeyStore, MemoryKeyStore, keys};
use pcbcraft_library::digikey::DigiKeySource;
use pcbcraft_library::http_client::HttpClient;
use pcbcraft_library::sources::{PartResult, PartSource, SourceError};
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

/// The search that proves a key works.
const TEST_QUERY: &str = "resistor";

/// A pacing wait this short is waited out once on the search worker instead of shown as an error.
const SHORT_WAIT_SECS: u64 = 2;

/// What the key test last said.
#[derive(Default)]
enum KeyStatus {
    #[default]
    Idle,
    Testing(Receiver<Result<Value, String>>),
    Connected,
    Rejected(String),
    Saved,
    SaveFailed(String),
    Removed,
}

/// A search over the configured sources, and what it answered.
struct Asked {
    query: String,
    offline: bool,
    rx: Receiver<Result<Value, String>>,
}

/// Parts and one-line notes from all configured sources for one query.
struct Answer {
    query: String,
    /// The Offline switch was on when this was asked.
    offline: bool,
    parts: Vec<PartResult>,
    notes: Vec<String>,
}

/// The Digi-Key card and the sources built from stored keys.
#[derive(Default)]
pub(super) struct Distributors {
    access: Option<(Arc<dyn HttpClient>, Arc<dyn KeyStore>)>,
    sources: Vec<Arc<dyn PartSource>>,
    cache: Arc<Mutex<PartSourceCache>>,
    /// Keys were stored or removed since the sources were built.
    dirty: bool,
    stored: bool,
    asked: Option<Asked>,
    answer: Option<Answer>,
    client_id: String,
    client_secret: String,
    status: KeyStatus,
    /// Opens or closes the card on the next frame, then clears.
    force_open: Option<bool>,
}

/// The sources the stored keys switch on, in the order the dock lists them.
///
/// # Rules
/// - Digi-Key is on when both of its values are stored and not blank.
/// - A key store that cannot be read counts as no key.
pub(super) fn configured_sources(
    keys_in: &dyn KeyStore,
    http: &Arc<dyn HttpClient>,
    store: &Arc<dyn KeyStore>,
) -> Vec<Arc<dyn PartSource>> {
    let has = |name: &str| {
        keys_in
            .get(name)
            .ok()
            .flatten()
            .is_some_and(|v| !v.trim().is_empty())
    };
    let mut sources: Vec<Arc<dyn PartSource>> = Vec::new();
    if has(keys::DIGIKEY_CLIENT_ID) && has(keys::DIGIKEY_CLIENT_SECRET) {
        sources.push(Arc::new(DigiKeySource::new(http.clone(), store.clone())));
    }
    sources
}

/// Judge one test search: any answer from the source, even an empty one, means the key works.
fn judge(result: Result<Vec<PartResult>, SourceError>) -> Result<Value, String> {
    match result {
        Ok(_) | Err(SourceError::NotFound(_)) => Ok(Value::Null),
        Err(e) => Err(e.to_string()),
    }
}

impl App {
    /// Give the window the transport and key store for distributor sources. The
    /// desktop app calls this; a build without them shows no distributor card.
    pub fn with_distributors(
        mut self,
        http: Arc<dyn HttpClient>,
        store: Arc<dyn KeyStore>,
    ) -> Self {
        self.distributors.access = Some((http, store));
        self.distributors.dirty = true;
        self.distributors_refresh();
        self.distributors.force_open = Some(!self.distributors.stored);
        self
    }

    /// Rebuild the source list when the stored keys changed.
    fn distributors_refresh(&mut self) {
        let d = &mut self.distributors;
        if !d.dirty {
            return;
        }
        d.dirty = false;
        let Some((http, store)) = d.access.clone() else {
            return;
        };
        d.sources = configured_sources(store.as_ref(), &http, &store);
        d.stored = !d.sources.is_empty();
        d.asked = None;
        d.answer = None;
        self.distributors_expire();
    }

    /// Keep the saved answers in `dir`, or in memory alone with `None`. Call after `with_distributors`.
    ///
    /// # Rules
    /// - Answers older than their source's window are deleted from the folder right away.
    pub fn with_distributor_cache_dir(mut self, dir: Option<std::path::PathBuf>) -> Self {
        let cache = match dir {
            Some(dir) => PartSourceCache::open(&dir),
            None => PartSourceCache::in_memory(),
        };
        self.distributors.cache = Arc::new(Mutex::new(cache));
        self.distributors_expire();
        self
    }

    /// Keep the saved answers in the app's own cache folder.
    pub fn with_default_distributor_cache(self) -> Self {
        self.with_distributor_cache_dir(default_response_cache_dir())
    }

    /// Delete saved answers that are past their source's window.
    fn distributors_expire(&self) {
        let now = pcbcraft_clock::unix_secs();
        if let Ok(cache) = self.distributors.cache.lock() {
            for source in &self.distributors.sources {
                // A folder that cannot be cleaned only keeps old files; the search still dates them.
                let _ = cache.purge_expired(source.as_ref(), now);
            }
        }
    }

    /// Empty every saved answer, after the key changed.
    fn distributors_forget(&self) {
        if let Ok(mut cache) = self.distributors.cache.lock() {
            let _ = cache.purge();
        }
    }

    /// The Digi-Key card and the results of the configured sources for the typed query.
    ///
    /// # Rules
    /// - Nothing is drawn when the host gave no key store.
    /// - No search runs with no key, offline, or for a query shorter than two letters.
    pub(super) fn distributor_parts(&mut self, ui: &mut egui::Ui) -> Vec<(PartResult, String)> {
        if self.distributors.access.is_none() {
            return Vec::new();
        }
        self.distributors_refresh();
        self.digikey_card(ui);
        if self.distributors.sources.is_empty() {
            return Vec::new();
        }
        let muted = self.muted();
        let note = |ui: &mut egui::Ui, text: &str| {
            ui.label(crate::fonts::caption(text).color(muted));
        };
        let offline = self
            .part_sources
            .offline_flag()
            .is_some_and(|f| f.load(Ordering::SeqCst));
        if offline {
            note(ui, &t!("library.dock.digikey.offline"));
        }
        let query = self.part_search.trim().to_owned();
        if query.chars().count() < 2 {
            return Vec::new();
        }
        if let Some(asked) = self.distributors.asked.take() {
            match asked.rx.try_recv() {
                Ok(done) => {
                    self.distributors.answer = Some(answer_of(asked.query, asked.offline, done));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.distributors.asked = Some(asked);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }
        let answered = self
            .distributors
            .answer
            .as_ref()
            .is_some_and(|a| a.query == query && a.offline == offline);
        let stale_ask = self
            .distributors
            .asked
            .as_ref()
            .is_some_and(|a| a.offline != offline);
        if stale_ask {
            // The switch moved while a search ran; its answer is for the other mode.
            self.distributors.asked = None;
        }
        if !answered && self.distributors.asked.is_none() {
            let sources = self.distributors.sources.clone();
            let cache = self.distributors.cache.clone();
            let asked = query.clone();
            let rx = library_dock::spawn_read(ui.ctx(), move || {
                Ok(search_all(&sources, &cache, &asked, offline))
            });
            self.distributors.asked = Some(Asked {
                query: query.clone(),
                offline,
                rx,
            });
        }
        if self.distributors.asked.is_some() {
            note(ui, &t!("library.dock.digikey.searching"));
        }
        let Some(answer) = self.distributors.answer.as_ref() else {
            return Vec::new();
        };
        for line in &answer.notes {
            note(ui, line);
        }
        if answer.parts.is_empty() && answer.notes.is_empty() && answer.query == query {
            note(
                ui,
                &t!("library.dock.digikey.nothing", query = answer.query),
            );
        }
        let credit = self
            .distributors
            .sources
            .first()
            .map(|s| s.attribution().text)
            .unwrap_or_default();
        answer
            .parts
            .iter()
            .map(|p| (p.clone(), credit.clone()))
            .collect()
    }

    /// The setup card: steps, the two fields, Save, Test and Remove, and where the key lives.
    fn digikey_card(&mut self, ui: &mut egui::Ui) {
        let Some((http, store)) = self.distributors.access.clone() else {
            return;
        };
        let muted = self.muted();
        let setup = DigiKeySource::new(http.clone(), store.clone()).setup();
        // A finished test replaces the "testing" line.
        if let KeyStatus::Testing(rx) = &self.distributors.status {
            match rx.try_recv() {
                Ok(Ok(_)) => self.distributors.status = KeyStatus::Connected,
                Ok(Err(why)) => self.distributors.status = KeyStatus::Rejected(why),
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.distributors.status = KeyStatus::Rejected(String::new());
                }
            }
        }
        let open = self.distributors.force_open.take();
        egui::CollapsingHeader::new(t!("library.dock.digikey.title"))
            .id_salt("digikey_card")
            .default_open(!self.distributors.stored)
            .open(open)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(t!("library.dock.digikey.steps")).strong());
                for (n, step) in setup.steps.iter().enumerate() {
                    ui.label(format!("{}. {step}", n + 1));
                }
                ui.hyperlink_to(t!("library.dock.digikey.portal"), &setup.portal_url)
                    .on_hover_text(t!("library.dock.digikey.portal.tip"));
                for (label, secret) in [
                    (t!("library.dock.digikey.client_id"), false),
                    (t!("library.dock.digikey.client_secret"), true),
                ] {
                    ui.label(&label);
                    let value = if secret {
                        &mut self.distributors.client_secret
                    } else {
                        &mut self.distributors.client_id
                    };
                    let field = ui.add(
                        egui::TextEdit::singleline(value)
                            .password(secret)
                            .desired_width(f32::INFINITY),
                    );
                    let enabled = field.enabled();
                    field.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::TextEdit,
                            enabled,
                            label.clone(),
                        )
                    });
                }
                ui.horizontal(|ui| {
                    if ui
                        .button(t!("library.dock.digikey.save"))
                        .on_hover_text(t!("library.dock.digikey.save.tip"))
                        .clicked()
                    {
                        self.digikey_save(store.as_ref());
                    }
                    if ui
                        .button(t!("library.dock.digikey.test"))
                        .on_hover_text(t!("library.dock.digikey.test.tip"))
                        .clicked()
                    {
                        self.digikey_test(ui.ctx(), http.clone(), store.as_ref());
                    }
                    if self.distributors.stored
                        && ui
                            .button(t!("library.dock.digikey.remove"))
                            .on_hover_text(t!("library.dock.digikey.remove.tip"))
                            .clicked()
                    {
                        self.digikey_remove(store.as_ref());
                    }
                });
                let line = match &self.distributors.status {
                    KeyStatus::Idle if self.distributors.stored => {
                        t!("library.dock.digikey.stored_now")
                    }
                    KeyStatus::Idle => t!("library.dock.digikey.no_key"),
                    KeyStatus::Testing(_) => t!("library.dock.digikey.testing"),
                    KeyStatus::Connected => t!("library.dock.digikey.connected"),
                    KeyStatus::Rejected(why) => t!("library.dock.digikey.rejected", why = why),
                    KeyStatus::Saved => t!("library.dock.digikey.saved"),
                    KeyStatus::SaveFailed(why) => t!("library.dock.digikey.save_failed", why = why),
                    KeyStatus::Removed => t!("library.dock.digikey.removed"),
                };
                ui.label(line);
                let where_kept = match store.storage_warning() {
                    Some(warning) => t!("library.dock.digikey.where_file", warning = warning),
                    None => t!("library.dock.digikey.where_keychain"),
                };
                ui.label(crate::fonts::caption(where_kept).color(muted));
            });
    }

    /// Store both typed values, then empty the secret field.
    fn digikey_save(&mut self, store: &dyn KeyStore) {
        let (id, secret) = (
            self.distributors.client_id.trim().to_owned(),
            self.distributors.client_secret.trim().to_owned(),
        );
        if id.is_empty() || secret.is_empty() {
            self.distributors.status = KeyStatus::SaveFailed(t!("library.dock.digikey.need_both"));
            return;
        }
        let saved = store
            .set(keys::DIGIKEY_CLIENT_ID, &id)
            .and_then(|()| store.set(keys::DIGIKEY_CLIENT_SECRET, &secret));
        self.distributors.status = match saved {
            Ok(()) => KeyStatus::Saved,
            Err(e) => KeyStatus::SaveFailed(e.to_string()),
        };
        if matches!(self.distributors.status, KeyStatus::Saved) {
            self.distributors.client_id.clear();
            self.distributors.client_secret.clear();
            self.distributors_forget();
            self.distributors.dirty = true;
            self.distributors_refresh();
        }
    }

    /// Run one search with the typed values, or the stored ones when the fields are empty.
    fn digikey_test(
        &mut self,
        ctx: &egui::Context,
        http: Arc<dyn HttpClient>,
        store: &dyn KeyStore,
    ) {
        let typed = |s: &str| Some(s.trim().to_owned()).filter(|v| !v.is_empty());
        let id = typed(&self.distributors.client_id)
            .or_else(|| store.get(keys::DIGIKEY_CLIENT_ID).ok().flatten());
        let secret = typed(&self.distributors.client_secret)
            .or_else(|| store.get(keys::DIGIKEY_CLIENT_SECRET).ok().flatten());
        let (Some(id), Some(secret)) = (id, secret) else {
            self.distributors.status = KeyStatus::Rejected(t!("library.dock.digikey.need_both"));
            return;
        };
        let rx = library_dock::spawn_read(ctx, move || {
            let temp: Arc<dyn KeyStore> = Arc::new(MemoryKeyStore::default());
            temp.set(keys::DIGIKEY_CLIENT_ID, &id)
                .and_then(|()| temp.set(keys::DIGIKEY_CLIENT_SECRET, &secret))
                .map_err(|e| e.to_string())?;
            let source = DigiKeySource::new(http, temp);
            judge(source.search(TEST_QUERY))
        });
        self.distributors.status = KeyStatus::Testing(rx);
    }

    /// Delete the stored values and return the card to its first state.
    fn digikey_remove(&mut self, store: &dyn KeyStore) {
        let gone = store
            .delete(keys::DIGIKEY_CLIENT_ID)
            .and_then(|()| store.delete(keys::DIGIKEY_CLIENT_SECRET));
        self.distributors.client_id.clear();
        self.distributors.client_secret.clear();
        self.distributors.status = match gone {
            Ok(()) => KeyStatus::Removed,
            Err(e) => KeyStatus::SaveFailed(e.to_string()),
        };
        self.distributors_forget();
        self.distributors.dirty = true;
        self.distributors_refresh();
        self.distributors.force_open = Some(true);
    }

    /// True when no search is wanted or the wanted one has answered.
    #[cfg(test)]
    pub(super) fn distributors_settled(&self) -> bool {
        let d = &self.distributors;
        let query = self.part_search.trim();
        let offline = self
            .part_sources
            .offline_flag()
            .is_some_and(|f| f.load(Ordering::SeqCst));
        d.sources.is_empty()
            || query.chars().count() < 2
            || (d.asked.is_none()
                && d.answer
                    .as_ref()
                    .is_some_and(|a| a.query == query && a.offline == offline))
    }

    /// Where the Digi-Key card stands, for a failing story to print.
    #[cfg(test)]
    pub(super) fn distributors_state(&self) -> String {
        let d = &self.distributors;
        format!(
            "stored: {}, sources: {}, search running: {}, answered: {}",
            d.stored,
            d.sources.len(),
            d.asked.is_some(),
            d.answer.as_ref().map_or(0, |a| a.parts.len())
        )
    }
}

/// Ask one source through the answer cache. A pacing wait of a second or two is waited out once.
fn ask(
    cache: &Mutex<PartSourceCache>,
    source: &dyn PartSource,
    query: &str,
    offline: bool,
) -> Result<CachedSearch, SourceError> {
    let mut waited = false;
    loop {
        let now = pcbcraft_clock::unix_secs();
        let found = match cache.lock() {
            Ok(mut c) => {
                c.set_offline(offline);
                c.search(source, query, now)
            }
            Err(_) => Err(SourceError::Unavailable("the answer cache failed".into())),
        };
        match found {
            Err(SourceError::RateLimited { retry_after_secs })
                if !waited && !offline && retry_after_secs <= SHORT_WAIT_SECS =>
            {
                waited = true;
                std::thread::sleep(std::time::Duration::from_secs(retry_after_secs));
            }
            other => return other,
        }
    }
}

/// The line that dates an answer: the oldest price in it, online or saved.
fn dated_note(found: &CachedSearch, offline: bool) -> Option<String> {
    let taken = found.results.iter().map(|p| p.retrieved_at).min()?;
    let date = date_text(taken);
    Some(if offline {
        t!("library.dock.digikey.saved_from", date = date)
    } else {
        t!("library.dock.digikey.dated", date = date)
    })
}

/// Ask every source, each through the shared dated answer cache, and gather parts and notes.
fn search_all(
    sources: &[Arc<dyn PartSource>],
    cache: &Mutex<PartSourceCache>,
    query: &str,
    offline: bool,
) -> Value {
    let mut parts: Vec<PartResult> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for source in sources {
        match ask(cache, source.as_ref(), query, offline) {
            Ok(mut found) => {
                notes.extend(dated_note(&found, offline));
                parts.append(&mut found.results);
            }
            Err(SourceError::NotFound(_)) => {}
            Err(SourceError::Unavailable(_)) if offline => {
                notes.push(t!("library.dock.digikey.nothing_stored", query = query));
            }
            Err(e) => notes.push(t!("library.dock.digikey.failed", why = e.to_string())),
        }
    }
    serde_json::json!({"parts": parts, "notes": notes})
}

/// Read a worker's answer back into parts and notes.
fn answer_of(query: String, offline: bool, done: Result<Value, String>) -> Answer {
    match done {
        Ok(v) => Answer {
            query,
            offline,
            parts: serde_json::from_value(v["parts"].clone()).unwrap_or_default(),
            notes: serde_json::from_value(v["notes"].clone()).unwrap_or_default(),
        },
        Err(why) => Answer {
            query,
            offline,
            parts: Vec::new(),
            notes: vec![t!("library.dock.digikey.failed", why = why)],
        },
    }
}
