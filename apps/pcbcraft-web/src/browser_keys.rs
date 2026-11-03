//! The browser's half of the assistant: where model keys live and how requests leave the page.
//!
//! Context: the assistant asks `ModelKeyStore` and `Transport` and never asks which shell it is in.
//! This file gives them their browser implementations.
//! Web APIs behind them: Web Storage (`localStorage`) for keys, and `fetch` for requests.
//!
//! # Rules
//! - A key lives under the name `pcbcraft.assistant.key.<name>` in `localStorage` of this origin.
//!   Any script of this page can read it; settings says so in one plain line.
//! - Requests use `fetch` in CORS mode with credentials omitted, so no cookie is sent or read.
//! - A reply over 8 MiB is refused. A request that fails before any status is reported in plain words.
//! - Errors never hold a header value, so a key cannot reach the status line.
//!
//! # Dependencies
//! - crates/assistant/src/provider.rs (ModelKeyStore, Transport)
//! - apps/pcbcraft-web/src/browser_fetch.rs (the same fetch pattern for the library)
//!
//! # Do not
//! - Log a key or a request header, or follow a redirect to another host with the key.
use pcbcraft_assistant::{Done, HttpRequest, HttpResponse, KeptIn, Key, ModelKeyStore, Transport};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

const PREFIX: &str = "pcbcraft.assistant.key.";
const MAX_REPLY: usize = 8 * 1024 * 1024;

/// Keys in `localStorage`.
///
/// # Rules
/// A storage that is blocked reports a plain error from every call.
///
/// # Dependencies
/// - crates/assistant/src/provider.rs (ModelKeyStore)
///
/// # Do not
/// Store anything but keys under the key prefix.
pub struct BrowserKeys;

fn storage() -> Result<web_sys::Storage, String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or_else(|| "This browser does not allow the page to keep a key.".to_string())
}

/// Contract: crates/assistant/src/provider.rs
impl ModelKeyStore for BrowserKeys {
    fn get(&self, name: &str) -> Result<Option<Key>, String> {
        let v = storage()?
            .get_item(&format!("{PREFIX}{name}"))
            .map_err(|_| "The browser would not read the key.".to_string())?;
        Ok(v.map(|v| Key::new(&v)))
    }
    fn set(&self, name: &str, key: &str) -> Result<(), String> {
        storage()?
            .set_item(&format!("{PREFIX}{name}"), key)
            .map_err(|_| "The browser would not keep the key.".to_string())
    }
    fn clear(&self, name: &str) -> Result<(), String> {
        storage()?
            .remove_item(&format!("{PREFIX}{name}"))
            .map_err(|_| "The browser would not forget the key.".to_string())
    }
    fn kept_in(&self) -> KeptIn {
        KeptIn::BrowserStorage
    }
}

/// Requests through the browser's `fetch`.
///
/// # Rules
/// The reply is read whole; a streamed request gets its body in one piece.
///
/// # Dependencies
/// - crates/assistant/src/provider.rs (Transport)
///
/// # Do not
/// Add an origin allowlist here; the person chose the address in settings.
pub struct BrowserTransport;

/// Contract: crates/assistant/src/provider.rs
impl Transport for BrowserTransport {
    fn send(&self, request: HttpRequest, done: Done<HttpResponse>) {
        wasm_bindgen_futures::spawn_local(async move {
            done(fetch(request).await);
        });
    }
}

async fn fetch(request: HttpRequest) -> Result<HttpResponse, String> {
    let failed = |_: JsValue| "The request could not be sent from this page.".to_string();
    let window = web_sys::window().ok_or("The browser window is not available.")?;
    let headers = web_sys::Headers::new().map_err(failed)?;
    let mut anthropic = false;
    for (k, v) in &request.headers {
        anthropic |= k.eq_ignore_ascii_case("anthropic-version");
        headers.set(k, v).map_err(failed)?;
    }
    if anthropic {
        // Required by that service before it accepts a call made straight from a page.
        headers
            .set("anthropic-dangerous-direct-browser-access", "true")
            .map_err(failed)?;
    }
    let init = web_sys::RequestInit::new();
    init.set_method(request.method);
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_credentials(web_sys::RequestCredentials::Omit);
    init.set_redirect(web_sys::RequestRedirect::Error);
    init.set_headers(&headers);
    if let Some(body) = &request.body {
        init.set_body(&js_sys::Uint8Array::from(body.as_slice()));
    }
    let reply = JsFuture::from(window.fetch_with_str_and_init(&request.url, &init))
        .await
        .map_err(|_| {
            if window.navigator().on_line() {
                "The model server did not answer. It may refuse requests from web pages, or the address is wrong.".to_string()
            } else {
                "The browser has no connection.".to_string()
            }
        })?;
    let response: web_sys::Response = reply.dyn_into().map_err(failed)?;
    let status = response.status();
    let buffer = response.array_buffer().map_err(failed)?;
    let buffer = JsFuture::from(buffer).await.map_err(failed)?;
    let body = js_sys::Uint8Array::new(&buffer).to_vec();
    if body.len() > MAX_REPLY {
        return Err("The reply is larger than the limit.".into());
    }
    Ok(HttpResponse::with_status(status, body))
}
