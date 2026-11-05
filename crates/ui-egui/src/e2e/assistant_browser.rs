//! Stories for the assistant in the browser build: a key in page storage and an answer from a
//! model, with no network (the page's fetch is replaced by a canned transport).
use super::super::*;
use super::support::*;
use pcbcraft_assistant::{
    Done, HttpRequest, HttpResponse, KeptIn, Key, MemKeys, ModelKeyStore, Transport,
};
use std::sync::{Arc, Mutex};

/// Stands in for the page's fetch: answers every request with one canned chat reply.
#[derive(Default)]
struct CannedFetch {
    seen: Mutex<Vec<HttpRequest>>,
}

/// Contract: crates/assistant/src/provider.rs
impl Transport for CannedFetch {
    fn send(&self, request: HttpRequest, done: Done<HttpResponse>) {
        if let Ok(mut s) = self.seen.lock() {
            s.push(request);
        }
        let reply = br#"{"choices":[{"message":{"role":"assistant","content":"Hello from the page"},"finish_reason":"stop"}]}"#;
        done(Ok(HttpResponse::ok(reply.to_vec())));
    }
}

/// Memory that says it is page storage, as the browser's key store does.
#[derive(Default)]
struct PageKeys(MemKeys);

/// Contract: crates/assistant/src/provider.rs
impl ModelKeyStore for PageKeys {
    fn get(&self, name: &str) -> Result<Option<Key>, String> {
        self.0.get(name)
    }
    fn set(&self, name: &str, key: &str) -> Result<(), String> {
        self.0.set(name, key)
    }
    fn clear(&self, name: &str) -> Result<(), String> {
        self.0.clear(name)
    }
    fn kept_in(&self) -> KeptIn {
        KeptIn::BrowserStorage
    }
}

fn browser_app(fetch: Arc<CannedFetch>, keys: Arc<PageKeys>) -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None).with_host(Host {
        http: None,
        transport: Some(fetch),
        keys: Some(keys),
    });
    app.browser = true;
    app.refresh();
    app.preferences.aside_lower_tab = 1;
    app.open_preferences(crate::preferences::TAB_ASSISTANT);
    app
}

fn add_connection_with_key(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    settle(ctx, app);
    authoring::tests::tap(ctx, app, "Add connection")?;
    settle(ctx, app);
    if let Some(d) = app.assistant.dialog.as_mut() {
        d.set_key("test-key");
    }
    settle(ctx, app);
    authoring::tests::tap(ctx, app, "Add")?;
    settle(ctx, app);
    Ok(())
}

#[test]
fn a_newcomer_adds_a_key_and_the_assistant_answers_in_the_browser() -> Result<(), String> {
    let fetch = Arc::new(CannedFetch::default());
    let keys = Arc::new(PageKeys::default());
    let mut app = browser_app(fetch.clone(), keys.clone());
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    authoring::tests::tap(&ctx, &mut app, "Add connection")?;
    settle(&ctx, &mut app);
    assert!(
        paints_text(&ctx, &mut app, "kept in this browser's storage")?,
        "the one line warning is shown next to the key field"
    );
    assert!(
        !paints_text(&ctx, &mut app, "needs the desktop app")?,
        "nothing refuses the assistant in the browser"
    );
    if let Some(d) = app.assistant.dialog.as_mut() {
        d.set_key("test-key");
    }
    authoring::tests::tap(&ctx, &mut app, "Add")?;
    settle(&ctx, &mut app);
    assert_eq!(app.preferences.assistant_connections.len(), 1);
    assert!(app.preferences.assistant_connections[0].has_key);
    let prefs = serde_json::to_string(&app.preferences).map_err(|e| e.to_string())?;
    assert!(
        !prefs.contains("test-key"),
        "the key is not in the preferences"
    );
    authoring::tests::tap(&ctx, &mut app, "Done")?;
    app.change_tab(Tab::Pcb);
    settle(&ctx, &mut app);
    app.assistant.input = "hello".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    let mut shown = false;
    for _ in 0..40 {
        if paints_text(&ctx, &mut app, "Hello from the page")? {
            shown = true;
            break;
        }
    }
    assert!(shown, "the answer is on screen");
    let seen = fetch.seen.lock().map_err(|e| e.to_string())?;
    assert_eq!(seen.len(), 1, "one request left the page");
    assert!(
        seen[0]
            .headers
            .contains(&("Authorization".into(), "Bearer test-key".into())),
        "the stored key was sent"
    );
    Ok(())
}

#[test]
fn a_newcomer_clears_their_key() -> Result<(), String> {
    let fetch = Arc::new(CannedFetch::default());
    let keys = Arc::new(PageKeys::default());
    let mut app = browser_app(fetch, keys.clone());
    let ctx = egui::Context::default();
    add_connection_with_key(&ctx, &mut app)?;
    let id = app.preferences.assistant_connections[0].id.clone();
    let name = pcbcraft_assistant::key_id(&id);
    assert!(keys.get(&name)?.is_some(), "the key was stored");
    assert!(paints_text(&ctx, &mut app, "key stored")?);
    authoring::tests::tap(&ctx, &mut app, "Edit")?;
    settle(&ctx, &mut app);
    authoring::tests::tap(&ctx, &mut app, "Clear key")?;
    settle(&ctx, &mut app);
    assert!(keys.get(&name)?.is_none(), "the key is gone from the store");
    assert!(!app.preferences.assistant_connections[0].has_key);
    assert!(
        paints_text(&ctx, &mut app, "no key")?,
        "the row says there is no key"
    );
    assert_eq!(
        app.preferences.assistant_connections.len(),
        1,
        "the connection stays"
    );
    Ok(())
}

fn settle(ctx: &egui::Context, app: &mut App) {
    for _ in 0..4 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}
