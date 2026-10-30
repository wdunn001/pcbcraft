//! Browser transport for the open library source: queued GETs drained by
//! [`BrowserTransport::drain_one`], with the same host rules as the desktop client.
//!
//! # Rules
//! - Plain GET, no credentials. Answers over [`MAX_FILE_BYTES`] are refused.
//! - `Fetcher::get` returns [`FetchError::Failed`] with [`PENDING_MESSAGE`] while
//!   the queue has not finished; call `poll` and try again.
//! - Cross-origin reads against the library host were checked on 2026-10-30:
//!   anonymous GET on the GitLab project API returns `Access-Control-Allow-Origin: *`
//!   (see `recorded-cross-origin.txt` in this folder). If a future probe fails,
//!   [`online_disabled_message`] explains that only file-drop import remains.
//!
//! # Dependencies
//! - crates/library/src/open_library.rs
//! - crates/library/src/cad.rs
//! - crates/library/tests/fixtures/open_library/ (recorded answers for tests)
//!
//! # Do not
//! - Use `async`/`.await` inside `Fetcher::get` (no async-to-sync bridge).

use pcbcraft_library::cad::MAX_FILE_BYTES;
use pcbcraft_library::open_library::{FetchError, Fetcher};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

/// Hosts the open library source may read in the browser build.
pub const OPEN_LIBRARY_HOSTS: &[&str] = &["gitlab.com"];
/// Most redirects followed for one call.
pub const MAX_REDIRECTS: usize = 3;
/// Error text while a queued URL is still in flight.
pub const PENDING_MESSAGE: &str = "request pending";

const UNCHECKED: u8 = 0;
const ALLOWED: u8 = 1;
const REFUSED: u8 = 2;

static CROSS_ORIGIN: AtomicU8 = AtomicU8::new(UNCHECKED);
#[cfg(target_arch = "wasm32")]
static PROBE_STARTED: AtomicBool = AtomicBool::new(false);

/// Whether the library host allows cross-origin reads from the web build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossOriginStatus {
    /// The probe has not finished yet.
    Unknown,
    /// Anonymous GET succeeds with CORS headers.
    Allowed,
    /// The host blocks readable cross-origin answers.
    Refused,
}

impl CrossOriginStatus {
    fn from_raw(v: u8) -> Self {
        match v {
            ALLOWED => Self::Allowed,
            REFUSED => Self::Refused,
            _ => Self::Unknown,
        }
    }
}

/// Result of the cross-origin probe against the library host.
pub fn cross_origin_status() -> CrossOriginStatus {
    CrossOriginStatus::from_raw(CROSS_ORIGIN.load(Ordering::Acquire))
}

/// Plain explanation when online sources stay off because CORS failed.
pub fn online_disabled_message() -> String {
    pcbcraft_text::Catalog::english().get("web.online_cross_origin_refused")
}

/// Test hook: force the cross-origin outcome without a browser.
#[cfg(test)]
pub fn set_cross_origin_status_for_test(status: CrossOriginStatus) {
    let v = match status {
        CrossOriginStatus::Unknown => UNCHECKED,
        CrossOriginStatus::Allowed => ALLOWED,
        CrossOriginStatus::Refused => REFUSED,
    };
    CROSS_ORIGIN.store(v, Ordering::Release);
}

/// Folder of recorded open-library answers shared with the library crate tests.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/library/tests/fixtures/open_library")
}

fn load_index(base: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = std::fs::read_to_string(base.join("index.json")).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

enum Backend {
    Recorded {
        index: BTreeMap<String, String>,
        base: PathBuf,
    },
    #[cfg(target_arch = "wasm32")]
    Live,
}

struct Inner {
    backend: Backend,
    offline: Arc<AtomicBool>,
    queue: Mutex<Vec<String>>,
    done: Mutex<BTreeMap<String, Result<Vec<u8>, FetchError>>>,
    #[cfg(target_arch = "wasm32")]
    flight: Mutex<Option<LiveFlight>>,
}

#[cfg(target_arch = "wasm32")]
struct LiveFlight {
    url: String,
    future: wasm_bindgen_futures::JsFuture,
}

/// Queued browser GETs for the open library hosts.
///
/// # Rules
/// - One in-flight fetch at a time on wasm; tests drain the whole queue in `poll`.
///
/// # Do not
/// - Share this between unrelated host allowlists.
pub struct BrowserTransport {
    inner: Arc<Inner>,
}

impl BrowserTransport {
    /// Replay recorded answers (tests and local checks).
    pub fn recorded(index: BTreeMap<String, String>, base: PathBuf) -> Self {
        Self {
            inner: Arc::new(Inner {
                backend: Backend::Recorded { index, base },
                offline: Arc::new(AtomicBool::new(false)),
                queue: Mutex::new(Vec::new()),
                done: Mutex::new(BTreeMap::new()),
                #[cfg(target_arch = "wasm32")]
                flight: Mutex::new(None),
            }),
        }
    }

    /// Recorded transport using the shared library fixtures.
    pub fn from_fixtures() -> Result<Self, String> {
        let base = fixtures_dir();
        let index = load_index(&base)?;
        Ok(Self::recorded(index, base))
    }

    /// Live browser fetch queue for wasm builds.
    #[cfg(target_arch = "wasm32")]
    pub fn live(offline: Arc<AtomicBool>) -> Self {
        Self {
            inner: Arc::new(Inner {
                backend: Backend::Live,
                offline,
                queue: Mutex::new(Vec::new()),
                done: Mutex::new(BTreeMap::new()),
                flight: Mutex::new(None),
            }),
        }
    }

    /// Drain at most one queued request (or advance the in-flight wasm fetch).
    pub fn poll(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            if cross_origin_status() == CrossOriginStatus::Unknown {
                start_cross_origin_probe();
            }
            self.poll_live();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.drain_recorded_one();
        }
    }

    fn drain_recorded_one(&self) {
        let Backend::Recorded { index, base } = &self.inner.backend else {
            return;
        };
        let url = {
            let mut q = match self.inner.queue.lock() {
                Ok(q) => q,
                Err(_) => return,
            };
            if q.is_empty() {
                return;
            }
            q.remove(0)
        };
        let answer = read_recorded(index, base, &url);
        if let Ok(mut done) = self.inner.done.lock() {
            done.insert(url, answer);
        }
    }

    fn get_once(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        if self.inner.offline.load(Ordering::Acquire) {
            return Err(FetchError::Offline(
                "Online libraries are switched off. Saved parts still open.".into(),
            ));
        }
        if cross_origin_status() == CrossOriginStatus::Refused {
            return Err(FetchError::Offline(online_disabled_message()));
        }
        check_url(url)?;
        if let Ok(done) = self.inner.done.lock()
            && let Some(answer) = done.get(url)
        {
            return answer.clone();
        }
        let mut q = self
            .inner
            .queue
            .lock()
            .map_err(|_| FetchError::Failed("transport is busy".into()))?;
        if !q.iter().any(|u| u == url) {
            q.push(url.to_owned());
        }
        Err(FetchError::Failed(PENDING_MESSAGE.into()))
    }

    #[cfg(target_arch = "wasm32")]
    fn poll_live(&self) {
        use std::task::{Context, Poll, Waker};
        use wasm_bindgen::JsCast;
        use web_sys::{Request, RequestCredentials, RequestInit, RequestMode, Response};

        let mut flight_slot = match self.inner.flight.lock() {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(flight) = flight_slot.as_mut() {
            let waker = Waker::noop();
            let mut cx = Context::from_waker(&waker);
            match flight.future.poll(&mut cx) {
                Poll::Pending => return,
                Poll::Ready(Ok(js)) => {
                    let url = flight.url.clone();
                    flight_slot.take();
                    let answer = match js.dyn_into::<Response>() {
                        Ok(resp) => map_response(resp),
                        Err(_) => Err(FetchError::Failed(
                            "The browser returned an unexpected answer.".into(),
                        )),
                    };
                    if let Ok(mut done) = self.inner.done.lock() {
                        done.insert(url, answer);
                    }
                }
                Poll::Ready(Err(_)) => {
                    let url = flight.url.clone();
                    flight_slot.take();
                    if let Ok(mut done) = self.inner.done.lock() {
                        done.insert(
                            url,
                            Err(FetchError::Failed(
                                "The browser could not finish the request.".into(),
                            )),
                        );
                    }
                }
            }
            return;
        }

        let url = {
            let mut q = match self.inner.queue.lock() {
                Ok(q) => q,
                Err(_) => return,
            };
            if q.is_empty() {
                return;
            }
            q.remove(0)
        };

        let Some(window) = web_sys::window() else {
            store_done(&self.inner, &url, Err(FetchError::Failed("no window".into())));
            return;
        };
        let Ok(init) = RequestInit::new() else {
            store_done(
                &self.inner,
                &url,
                Err(FetchError::Failed("bad request".into())),
            );
            return;
        };
        init.set_method("GET");
        init.set_mode(RequestMode::Cors);
        init.set_credentials(RequestCredentials::Omit);
        let Ok(request) = Request::new_with_str_and_init(&url, &init) else {
            store_done(
                &self.inner,
                &url,
                Err(FetchError::Failed("bad request".into())),
            );
            return;
        };
        let Ok(promise) = window.fetch_with_request(&request) else {
            store_done(
                &self.inner,
                &url,
                Err(FetchError::Failed("fetch refused".into())),
            );
            return;
        };
        *flight_slot = Some(LiveFlight {
            url,
            future: wasm_bindgen_futures::JsFuture::from(promise),
        });
    }
}

#[cfg(target_arch = "wasm32")]
fn store_done(inner: &Inner, url: &str, answer: Result<Vec<u8>, FetchError>) {
    if let Ok(mut done) = inner.done.lock() {
        done.insert(url.to_owned(), answer);
    }
}

#[cfg(target_arch = "wasm32")]
fn map_response(resp: web_sys::Response) -> Result<Vec<u8>, FetchError> {
    use js_sys::Uint8Array;
    use std::task::{Context, Poll, Waker};
    use wasm_bindgen_futures::JsFuture;

    let status = resp.status();
    match status {
        200..=299 => {
            let promise = resp.array_buffer();
            let mut future = JsFuture::from(promise);
            let waker = Waker::noop();
            let mut cx = Context::from_waker(&waker);
            match future.poll(&mut cx) {
                Poll::Ready(Ok(buf)) => {
                    let array = Uint8Array::new(&buf);
                    let len = array.length() as usize;
                    if len > MAX_FILE_BYTES {
                        return Err(FetchError::Failed(
                            "The file is larger than the size limit, so it was not read.".into(),
                        ));
                    }
                    let mut out = vec![0u8; len];
                    array.copy_to(&mut out);
                    Ok(out)
                }
                Poll::Ready(Err(_)) => Err(FetchError::Failed(
                    "The answer could not be read.".into(),
                )),
                Poll::Pending => Err(FetchError::Failed(PENDING_MESSAGE.into())),
            }
        }
        404 | 410 => Err(FetchError::NotFound),
        429 => {
            let wait = resp
                .headers()
                .get("Retry-After")
                .ok()
                .flatten()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(30)
                .min(3600);
            Err(FetchError::RateLimited {
                retry_after_secs: wait,
            })
        }
        other => Err(FetchError::Failed(format!("The host answered {other}."))),
    }
}

#[cfg(target_arch = "wasm32")]
fn start_cross_origin_probe() {
    use pcbcraft_library::open_library::OpenLibraryConfig;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, RequestCredentials, RequestInit, RequestMode, Response};

    if PROBE_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let cfg = OpenLibraryConfig::default();
    let url = format!(
        "{}/projects/{}/repository/commits/{}",
        cfg.api_base,
        encode_path(&cfg.symbols_project),
        encode_path(&cfg.branch)
    );
    let Some(window) = web_sys::window() else {
        CROSS_ORIGIN.store(REFUSED, Ordering::Release);
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(init) = RequestInit::new() else {
            CROSS_ORIGIN.store(REFUSED, Ordering::Release);
            return;
        };
        init.set_method("GET");
        init.set_mode(RequestMode::Cors);
        init.set_credentials(RequestCredentials::Omit);
        let Ok(request) = Request::new_with_str_and_init(&url, &init) else {
            CROSS_ORIGIN.store(REFUSED, Ordering::Release);
            return;
        };
        let Ok(promise) = window.fetch_with_request(&request) else {
            CROSS_ORIGIN.store(REFUSED, Ordering::Release);
            return;
        };
        let outcome = match JsFuture::from(promise).await {
            Ok(v) => v
                .dyn_into::<Response>()
                .map(|r| r.ok())
                .unwrap_or(false),
            Err(_) => false,
        };
        CROSS_ORIGIN.store(
            if outcome { ALLOWED } else { REFUSED },
            Ordering::Release,
        );
    });
}

fn read_recorded(
    index: &BTreeMap<String, String>,
    base: &Path,
    url: &str,
) -> Result<Vec<u8>, FetchError> {
    let file = index
        .get(url)
        .ok_or_else(|| FetchError::Failed(format!("not recorded: {url}")))?;
    std::fs::read(base.join(file)).map_err(|e| FetchError::Failed(e.to_string()))
}

/// Wraps a [`BrowserTransport`] so synchronous open-library reads can loop `poll`.
pub struct PollingFetcher {
    inner: Arc<BrowserTransport>,
}

impl PollingFetcher {
    /// A fetcher that polls the shared transport until each URL settles.
    pub fn new(inner: Arc<BrowserTransport>) -> Self {
        Self { inner }
    }
}

impl Fetcher for PollingFetcher {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        const MAX_ROUNDS: u32 = 20_000;
        for _ in 0..MAX_ROUNDS {
            match self.inner.get_once(url) {
                Ok(bytes) => return Ok(bytes),
                Err(FetchError::Failed(m)) if m == PENDING_MESSAGE => {
                    self.inner.poll();
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err(FetchError::Failed(
            "The online library request did not finish.".into(),
        ))
    }

    fn poll(&self) {
        self.inner.poll();
    }
}

fn encode_path(text: &str) -> String {
    text.split('/')
        .map(|part| {
            part.bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("%2F")
}

fn check_url(url: &str) -> Result<(), FetchError> {
    let parts = Parts::parse(url)?;
    if parts.scheme != "https" {
        return Err(FetchError::Failed(
            "Only secure addresses are used for online libraries.".into(),
        ));
    }
    if !OPEN_LIBRARY_HOSTS.iter().any(|h| *h == parts.host) {
        return Err(FetchError::Failed(format!(
            "{} is not one of the online library sites.",
            parts.host
        )));
    }
    Ok(())
}

struct Parts {
    scheme: String,
    host: String,
}

impl Parts {
    fn parse(url: &str) -> Result<Self, FetchError> {
        let bad = || FetchError::Failed("That is not a valid web address.".into());
        let (scheme, rest) = url.split_once("://").ok_or_else(bad)?;
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        if authority.is_empty() || authority.contains('@') || url.contains(char::is_whitespace) {
            return Err(bad());
        }
        let host = match authority.rsplit_once(':') {
            Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
            _ => authority,
        };
        Ok(Self {
            scheme: scheme.to_ascii_lowercase(),
            host: host.to_ascii_lowercase(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_library::cad::{CadKind, CadRef, CadSource};
    use pcbcraft_library::cad_cache::CadCache;
    use pcbcraft_library::open_library::OpenLibrarySource;
    use pcbcraft_library::sources::PartSource;

    #[test]
    fn web_transport_serves_recorded_answers() -> Result<(), String> {
        set_cross_origin_status_for_test(CrossOriginStatus::Allowed);
        let transport = Arc::new(BrowserTransport::from_fixtures()?);
        let fetcher = PollingFetcher::new(Arc::clone(&transport));
        let source =
            OpenLibrarySource::new(Box::new(fetcher), CadCache::in_memory()).with_clock(Box::new(
                || 1_000,
            ));
        let libs = source
            .list_libraries(CadKind::Symbol)
            .map_err(|e| format!("{e:?}"))?;
        assert!(libs.contains(&"Device".to_owned()));
        let cards = PartSource::search(&source, "resistor").map_err(|e| format!("{e:?}"))?;
        assert!(cards.iter().any(|c| c.mpn == "R"));
        let r = CadRef::new(CadKind::Symbol, "Device", "R").map_err(|e| format!("{e:?}"))?;
        let file = source.fetch_symbol(&r).map_err(|e| format!("{e:?}"))?;
        assert!(!file.bytes.is_empty());
        set_cross_origin_status_for_test(CrossOriginStatus::Unknown);
        Ok(())
    }

    #[test]
    fn web_build_explains_when_cross_origin_reads_are_refused() {
        set_cross_origin_status_for_test(CrossOriginStatus::Refused);
        let catalog = pcbcraft_text::Catalog::english();
        assert!(
            catalog.defines("web.online_cross_origin_refused"),
            "catalog key missing"
        );
        let msg = online_disabled_message();
        assert_eq!(msg, catalog.get("web.online_cross_origin_refused"));
        assert!(
            msg.to_ascii_lowercase().contains("drop"),
            "mentions file-drop import: {msg}"
        );
        set_cross_origin_status_for_test(CrossOriginStatus::Unknown);
    }
}
