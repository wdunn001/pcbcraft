//! The Digi-Key setup card in the Library dock. A person makes a free key on the distributor's
//! developer site, pastes the client id and secret into the card, and the app keeps them
//! through the key store the host gave the window.
//!
//! # Rules
//! - The key is read and written through the `KeyStore` the host gave the window. A project
//!   file never holds it.
//! - The card is drawn only when the host gave a key store; a build without one shows nothing.
//! - Test key runs against the typed values in a throwaway store and saves nothing.
//! - The secret field is masked, is emptied after Save and is never logged.
//! - The card says where the key is stored, using the store's own warning when it has one.
//!
//! # Dependencies
//! - crates/library/src/credentials.rs
//! - crates/library/src/digikey.rs
//! - crates/library/src/sources.rs
//! - crates/ui-egui/src/sources.rs
//!
//! # Do not
//! - Derive `Debug` on the card state; it holds a secret while it is typed.
use super::*;
use pcbcraft_library::credentials::{KeyStore, MemoryKeyStore, keys};
use pcbcraft_library::digikey::DigiKeySource;
use pcbcraft_library::http_client::HttpClient;
use pcbcraft_library::sources::{PartResult, PartSource, SourceError};
use std::sync::Arc;
use std::sync::mpsc::Receiver;

/// The search that proves a key works.
const TEST_QUERY: &str = "resistor";

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

/// The Digi-Key card state.
#[derive(Default)]
pub(super) struct Distributors {
    access: Option<(Arc<dyn HttpClient>, Arc<dyn KeyStore>)>,
    /// Keys were stored or removed since `stored` was last read.
    dirty: bool,
    stored: bool,
    client_id: String,
    client_secret: String,
    status: KeyStatus,
    /// Opens or closes the card on the next frame, then clears.
    force_open: Option<bool>,
}

/// Whether both Digi-Key values are stored and not blank.
///
/// # Rules
/// - A key store that cannot be read counts as no key.
fn keys_stored(store: &dyn KeyStore) -> bool {
    let has = |name: &str| {
        store
            .get(name)
            .ok()
            .flatten()
            .is_some_and(|v| !v.trim().is_empty())
    };
    has(keys::DIGIKEY_CLIENT_ID) && has(keys::DIGIKEY_CLIENT_SECRET)
}

/// Judge one test search: any answer from the source, even an empty one, means the key works.
fn judge(result: Result<Vec<PartResult>, SourceError>) -> Result<Value, String> {
    match result {
        Ok(_) | Err(SourceError::NotFound(_)) => Ok(Value::Null),
        Err(e) => Err(e.to_string()),
    }
}

impl App {
    /// Give the window the transport and key store for the Digi-Key card. The
    /// desktop app calls this; a build without them shows no card.
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

    /// Read again whether a key is stored, after Save or Remove.
    fn distributors_refresh(&mut self) {
        let d = &mut self.distributors;
        if !d.dirty {
            return;
        }
        d.dirty = false;
        if let Some((_, store)) = &d.access {
            d.stored = keys_stored(store.as_ref());
        }
    }

    /// Draws the Digi-Key card. The part list gets nothing from here yet.
    ///
    /// # Rules
    /// - Nothing is drawn when the host gave no key store.
    pub(super) fn distributor_parts(&mut self, ui: &mut egui::Ui) -> Vec<(PartResult, String)> {
        if self.distributors.access.is_some() {
            self.distributors_refresh();
            self.digikey_card(ui);
        }
        Vec::new()
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
        self.distributors.dirty = true;
        self.distributors_refresh();
        self.distributors.force_open = Some(true);
    }

    /// Where the Digi-Key card stands, for a failing story to print.
    #[cfg(test)]
    pub(super) fn distributors_state(&self) -> String {
        let d = &self.distributors;
        format!("stored: {}, saving needed: {}", d.stored, d.dirty)
    }
}
