//! The browser transport for online CAD sources: a [`Fetcher`] that never
//! blocks. The first ask for an address hands it to a [`Dispatch`] (the web app
//! starts a browser fetch) and answers [`FetchError::Pending`]; the web app
//! calls [`WebTransport::answer`] when the browser replies, and the next ask
//! gets the bytes. The library crate stays free of browser code, so this file
//! is tested natively with recorded answers.
//!
//! # Rules
//! - Plain GET over HTTPS to an allowlisted host; no credentials, no user info in the address.
//! - At most [`MAX_IN_FLIGHT`] requests are out at once; the rest wait their turn.
//! - Answers are kept for a short while so the caller can ask again; the source's own cache is the long-term copy.
//! - Offline answers before anything is dispatched.
//! - When the browser refuses cross-origin reads the transport turns itself off and keeps the plain explanation in [`WebTransport::blocked`].
//!
//! # Dependencies
//! - crates/library/src/open_library.rs (`Fetcher`, `FetchError`)
//! - crates/library/src/cad.rs (the size cap)
//! - apps/pcbcraft-web/src/browser_fetch.rs (the one `Dispatch` that talks to the browser)
use crate::cad::MAX_FILE_BYTES;
use crate::open_library::{FetchError, Fetcher};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Requests the browser may have out at once.
pub const MAX_IN_FLIGHT: usize = 2;
/// Answers kept for the caller to ask again.
const MAX_KEPT_ANSWERS: usize = 256;
/// Bytes of answers kept in all.
const MAX_KEPT_BYTES: usize = 32 * 1024 * 1024;

/// Starts one browser request. The web app implements it; tests record.
///
/// # Rules
/// - Return at once; the reply comes later through [`WebTransport::answer`] or [`WebTransport::refuse_cross_origin`].
///
/// # Do not
/// - Block, and do not call back into the transport on the same stack frame while it holds a lock (it does not).
///
/// # Implementations
/// - `apps/pcbcraft-web/src/browser_fetch.rs`
/// - `crates/ui-egui/src/e2e/online.rs`
pub trait Dispatch: Send + Sync {
    /// Begin a GET for `url`.
    fn start(&self, url: &str);
}

#[derive(Default)]
struct State {
    answers: BTreeMap<String, Result<Vec<u8>, FetchError>>,
    order: VecDeque<String>,
    kept_bytes: usize,
    asked: BTreeSet<String>,
    waiting: VecDeque<String>,
    in_flight: usize,
    blocked: Option<String>,
}

/// A non-blocking [`Fetcher`] driven by the browser.
///
/// # Rules
/// - Contract: crates/library/src/open_library.rs (`Fetcher`). `get` never blocks; it answers [`FetchError::Pending`] until the reply is in.
/// - The caller repeats its work while [`WebTransport::waiting`] is above zero.
///
/// # Dependencies
/// - crates/library/src/open_library.rs
///
/// # Do not
/// - Hold the state lock while calling the [`Dispatch`].
pub struct WebTransport {
    dispatch: Box<dyn Dispatch>,
    allowed_hosts: Vec<String>,
    offline: Arc<AtomicBool>,
    pending_served: AtomicU64,
    state: Mutex<State>,
}

impl WebTransport {
    /// A transport that asks `dispatch` for each new address on `allowed_hosts`.
    pub fn new(
        dispatch: Box<dyn Dispatch>,
        allowed_hosts: &[&str],
        offline: Arc<AtomicBool>,
    ) -> Self {
        Self {
            dispatch,
            allowed_hosts: allowed_hosts
                .iter()
                .map(|h| (*h).to_ascii_lowercase())
                .collect(),
            offline,
            pending_served: AtomicU64::new(0),
            state: Mutex::new(State::default()),
        }
    }

    /// Requests sent or queued that have no answer yet.
    pub fn waiting(&self) -> usize {
        self.state
            .lock()
            .map(|s| s.in_flight + s.waiting.len())
            .unwrap_or(0)
    }

    /// How many asks have been answered [`FetchError::Pending`] so far. A caller
    /// that sees this change across one piece of work knows that work was partial.
    pub fn pending_served(&self) -> u64 {
        self.pending_served.load(Ordering::SeqCst)
    }

    /// The plain explanation, once the browser has refused cross-origin reads.
    pub fn blocked(&self) -> Option<String> {
        self.state.lock().ok().and_then(|s| s.blocked.clone())
    }

    /// Record the browser's reply to a request it was asked to start.
    ///
    /// # Rules
    /// - A body over [`MAX_FILE_BYTES`] is refused.
    /// - The next queued request starts after this one is stored.
    pub fn answer(&self, url: &str, reply: Result<Vec<u8>, FetchError>) {
        let reply = match reply {
            Ok(body) if body.len() > MAX_FILE_BYTES => {
                Err(FetchError::Failed("That file is too large to open.".into()))
            }
            other => other,
        };
        let next = {
            let Ok(mut s) = self.state.lock() else {
                return;
            };
            s.in_flight = s.in_flight.saturating_sub(1);
            if let Ok(body) = &reply {
                s.kept_bytes += body.len();
            }
            s.order.push_back(url.to_owned());
            s.answers.insert(url.to_owned(), reply);
            while s.order.len() > MAX_KEPT_ANSWERS || s.kept_bytes > MAX_KEPT_BYTES {
                let Some(old) = s.order.pop_front() else {
                    break;
                };
                if let Some(Ok(body)) = s.answers.remove(&old) {
                    s.kept_bytes = s.kept_bytes.saturating_sub(body.len());
                }
                s.asked.remove(&old);
            }
            let next = if s.in_flight < MAX_IN_FLIGHT {
                s.waiting.pop_front()
            } else {
                None
            };
            if next.is_some() {
                s.in_flight += 1;
            }
            next
        };
        if let Some(url) = next {
            self.dispatch.start(&url);
        }
    }

    /// The browser could not read the host at all, which is what a refused
    /// cross-origin read looks like from the page. The transport turns itself
    /// off and every later ask explains why.
    pub fn refuse_cross_origin(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.blocked = Some(
                "This browser does not let the page read the open library host, so online parts are off here. \
                 Drop a library file onto the window to import it, or use the desktop app."
                    .into(),
            );
            s.waiting.clear();
            s.in_flight = 0;
        }
    }

    fn check(&self, url: &str) -> Result<(), FetchError> {
        let bad = || FetchError::Failed("That is not a valid web address.".into());
        let (scheme, rest) = url.split_once("://").ok_or_else(bad)?;
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        if !scheme.eq_ignore_ascii_case("https")
            || authority.is_empty()
            || authority.contains('@')
            || url.contains(char::is_whitespace)
        {
            return Err(bad());
        }
        let host = authority.to_ascii_lowercase();
        if self.allowed_hosts.contains(&host) {
            Ok(())
        } else {
            Err(FetchError::Failed(
                "That host is not one of the open library hosts.".into(),
            ))
        }
    }
}

/// Contract: crates/library/src/open_library.rs
impl Fetcher for WebTransport {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        if self.offline.load(Ordering::SeqCst) {
            return Err(FetchError::Offline(
                "online sources are switched off".into(),
            ));
        }
        self.check(url)?;
        let start = {
            let mut s = self
                .state
                .lock()
                .map_err(|_| FetchError::Failed("The browser transport is busy.".into()))?;
            if let Some(why) = &s.blocked {
                return Err(FetchError::Failed(why.clone()));
            }
            if let Some(reply) = s.answers.get(url) {
                return reply.clone();
            }
            if !s.asked.insert(url.to_owned()) {
                self.pending_served.fetch_add(1, Ordering::SeqCst);
                return Err(FetchError::Pending);
            }
            if s.in_flight < MAX_IN_FLIGHT {
                s.in_flight += 1;
                true
            } else {
                s.waiting.push_back(url.to_owned());
                false
            }
        };
        self.pending_served.fetch_add(1, Ordering::SeqCst);
        if start {
            self.dispatch.start(url);
        }
        Err(FetchError::Pending)
    }
}

/// Contract: crates/library/src/open_library.rs
impl Fetcher for Arc<WebTransport> {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        self.as_ref().get(url)
    }
}
