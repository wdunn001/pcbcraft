//! Stories for the Digi-Key setup card in the Library dock.
//! Every answer comes from the recordings next to the library crate's tests, and the
//! keys are fake placeholders held in memory; no story reaches a host.
use super::super::*;
use super::support::*;
use pcbcraft_library::credentials::{KeyStore, MemoryKeyStore, keys};
use pcbcraft_library::http_client::{HttpClient, HttpError, HttpRequest, HttpResponse};
use std::collections::BTreeMap;
use std::sync::Arc;

const FIXTURES: &str = "../library/tests/fixtures/sources";
const FAKE_ID: &str = "fake-client-id-0001";
const FAKE_SECRET: &str = "fake-client-secret-0001";

fn recorded(name: &str) -> Result<Vec<u8>, String> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURES)
        .join(name);
    std::fs::read(path).map_err(|e| e.to_string())
}

/// A distributor that answers from the recordings. The token call is refused with
/// `token_status` when that is not 200, and a search answers `search_status`.
struct Recorded {
    token: Vec<u8>,
    search: Vec<u8>,
    token_status: u16,
    search_status: u16,
}

impl Recorded {
    fn good() -> Result<Self, String> {
        Ok(Self {
            token: recorded("digikey_token.json")?,
            search: recorded("digikey_search.json")?,
            token_status: 200,
            search_status: 200,
        })
    }
}

impl HttpClient for Recorded {
    fn call(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let (status, body) = if request.url.ends_with("/oauth2/token") {
            (self.token_status, self.token.clone())
        } else if request.url.ends_with("/search/keyword") {
            (self.search_status, self.search.clone())
        } else {
            return Err(HttpError::Failed(format!("not recorded: {}", request.url)));
        };
        Ok(HttpResponse {
            status,
            headers: BTreeMap::new(),
            body,
        })
    }
}

fn dock(http: Arc<Recorded>, store: Arc<MemoryKeyStore>) -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app =
        App::create(shared, Preferences::default(), None, None).with_distributors(http, store);
    app.refresh();
    app.region().left_tab = "library".to_owned();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Schematic);
    (ctx, app)
}

fn press(ctx: &egui::Context, app: &mut App, name: &str) -> Result<(), String> {
    let _tall = TallScope::enter();
    let all = interactive_rects(ctx, app);
    let pos = all
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, r)| r.center())
        .ok_or_else(|| {
            format!(
                "{name} is not on screen: {:?}",
                all.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()
            )
        })?;
    tap_at(ctx, app, pos)
}

fn type_in(ctx: &egui::Context, app: &mut App, name: &str, text: &str) -> Result<(), String> {
    press(ctx, app, name)?;
    frame(
        ctx,
        app,
        vec![egui::Event::Text(text.to_owned())],
        egui::Modifiers::NONE,
    );
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

fn stored(store: &MemoryKeyStore) -> bool {
    store.get(keys::DIGIKEY_CLIENT_ID).ok().flatten().is_some()
        && store
            .get(keys::DIGIKEY_CLIENT_SECRET)
            .ok()
            .flatten()
            .is_some()
}

#[test]
fn a_newcomer_follows_the_card_and_adds_a_digikey_key() -> Result<(), String> {
    let store = Arc::new(MemoryKeyStore::default());
    let (ctx, mut app) = dock(Arc::new(Recorded::good()?), store.clone());
    for text in [
        "Digi-Key key",
        "Create a free developer account",
        "Register an app and copy the client id",
        "No key is stored yet.",
        "system keychain",
    ] {
        assert!(paints_text(&ctx, &mut app, text)?, "the card shows: {text}");
    }
    assert!(!stored(&store), "nothing is stored before Save");
    type_in(&ctx, &mut app, "TextInput Client id", FAKE_ID)?;
    type_in(&ctx, &mut app, "TextInput Client secret", FAKE_SECRET)?;
    assert!(
        !paints_text(&ctx, &mut app, FAKE_SECRET)?,
        "the secret is masked on screen"
    );
    press(&ctx, &mut app, "Button Test key")?;
    assert!(
        waits_for_text(&ctx, &mut app, "Connected. Digi-Key accepted the key.")?,
        "a good key reads Connected: {}",
        app.distributors_state()
    );
    assert!(!stored(&store), "Test key saves nothing");
    press(&ctx, &mut app, "Button Save key")?;
    assert!(
        paints_text(&ctx, &mut app, "Key saved.")?,
        "Save says it kept the key"
    );
    assert_eq!(
        store
            .get(keys::DIGIKEY_CLIENT_ID)
            .map_err(|e| e.to_string())?,
        Some(FAKE_ID.to_owned())
    );
    assert_eq!(
        store
            .get(keys::DIGIKEY_CLIENT_SECRET)
            .map_err(|e| e.to_string())?,
        Some(FAKE_SECRET.to_owned())
    );
    press(&ctx, &mut app, "Button Remove key")?;
    assert!(!stored(&store), "Remove deletes both values");
    assert!(
        paints_text(&ctx, &mut app, "Key removed.")?,
        "Remove says so"
    );
    assert!(
        paints_text(&ctx, &mut app, "Create a free developer account")?,
        "after Remove the card shows its steps again"
    );
    assert!(
        interactive_rects(&ctx, &mut app)
            .iter()
            .all(|(n, _)| n != "Button Remove key"),
        "with no key there is nothing to remove"
    );
    Ok(())
}

#[test]
fn a_newcomer_enters_a_wrong_key_and_reads_why_it_failed() -> Result<(), String> {
    let store = Arc::new(MemoryKeyStore::default());
    let mut http = Recorded::good()?;
    http.token_status = 401;
    let (ctx, mut app) = dock(Arc::new(http), store.clone());
    press(&ctx, &mut app, "Button Test key")?;
    assert!(
        paints_text(&ctx, &mut app, "Enter both the client id")?,
        "an empty card asks for both values"
    );
    type_in(&ctx, &mut app, "TextInput Client id", FAKE_ID)?;
    type_in(
        &ctx,
        &mut app,
        "TextInput Client secret",
        "fake-wrong-secret",
    )?;
    press(&ctx, &mut app, "Button Test key")?;
    assert!(
        waits_for_text(
            &ctx,
            &mut app,
            "Digi-Key did not accept the key: This service has not approved the key yet"
        )?,
        "the rejected key is explained: {}",
        app.distributors_state()
    );
    assert!(!stored(&store), "a rejected test stores nothing");
    Ok(())
}
