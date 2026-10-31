//! The browser's half of the online part transport: starts a plain `fetch` for
//! each address `WebTransport` hands over and reports the reply back to it.
//!
//! # Rules
//! - GET only, no credentials, no cookies; the page's own origin is never sent as data.
//! - A fetch that fails while the browser says it is online is treated as a refused cross-origin read:
//!   the page cannot tell the two apart, and the explanation tells the person what to do either way.
//! - The status codes map as the contract in crates/library/src/open_library.rs says: 404 not found, 429 paused.
//!
//! # Dependencies
//! - crates/library/src/web_transport.rs
//! - crates/library/src/open_library.rs
//!
//! # Do not
//! - Add hosts here; the allowlist is [`HOSTS`] and matches the desktop list in apps/pcbcraft/src/online_http.rs.
use pcbcraft_library::open_library::FetchError;
use pcbcraft_library::web_transport::{Dispatch, WebTransport};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock, Weak};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// Hosts the open library source reads from.
const HOSTS: &[&str] = &["gitlab.com"];
/// Seconds to wait when the host asks for a pause and names no time.
const DEFAULT_PAUSE_SECS: u64 = 30;

/// Starts browser fetches for one transport.
///
/// Contract: crates/library/src/web_transport.rs
struct BrowserDispatch {
    transport: Arc<OnceLock<Weak<WebTransport>>>,
}

/// Contract: crates/library/src/web_transport.rs
impl Dispatch for BrowserDispatch {
    fn start(&self, url: &str) {
        let slot = self.transport.clone();
        let url = url.to_owned();
        wasm_bindgen_futures::spawn_local(async move {
            let reply = fetch(&url).await;
            let Some(transport) = slot.get().and_then(Weak::upgrade) else {
                return;
            };
            match reply {
                Ok(body) => transport.answer(&url, Ok(body)),
                Err(Reply::Refused) => transport.refuse_cross_origin(),
                Err(Reply::Error(e)) => transport.answer(&url, Err(e)),
            }
        });
    }
}

enum Reply {
    /// The browser would not read the host at all.
    Refused,
    Error(FetchError),
}

/// The transport the web app hands to the Library tab.
pub fn transport(offline: Arc<AtomicBool>) -> Arc<WebTransport> {
    let slot = Arc::new(OnceLock::new());
    let transport = Arc::new(WebTransport::new(
        Box::new(BrowserDispatch {
            transport: slot.clone(),
        }),
        HOSTS,
        offline,
    ));
    let _ = slot.set(Arc::downgrade(&transport));
    transport
}

async fn fetch(url: &str) -> Result<Vec<u8>, Reply> {
    let window = web_sys::window().ok_or(Reply::Error(FetchError::Failed(
        "The browser window is not available.".into(),
    )))?;
    let init = web_sys::RequestInit::new();
    init.set_method("GET");
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_credentials(web_sys::RequestCredentials::Omit);
    let promise = window.fetch_with_str_and_init(url, &init);
    let response: web_sys::Response = match JsFuture::from(promise).await {
        Ok(value) => value.dyn_into().map_err(|_| {
            Reply::Error(FetchError::Failed("The browser sent an odd answer.".into()))
        })?,
        Err(_) if !window.navigator().on_line() => {
            return Err(Reply::Error(FetchError::Offline(
                "the browser has no connection".into(),
            )));
        }
        Err(_) => return Err(Reply::Refused),
    };
    match response.status() {
        200..=299 => {}
        404 => return Err(Reply::Error(FetchError::NotFound)),
        429 => {
            let wait = response
                .headers()
                .get("retry-after")
                .ok()
                .flatten()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_PAUSE_SECS);
            return Err(Reply::Error(FetchError::RateLimited {
                retry_after_secs: wait,
            }));
        }
        code => {
            return Err(Reply::Error(FetchError::Failed(format!(
                "The host answered with status {code}."
            ))));
        }
    }
    let buffer = response
        .array_buffer()
        .map_err(|_| Reply::Error(FetchError::Failed("The answer could not be read.".into())))?;
    let buffer: JsValue = JsFuture::from(buffer)
        .await
        .map_err(|_| Reply::Error(FetchError::Failed("The answer could not be read.".into())))?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}
