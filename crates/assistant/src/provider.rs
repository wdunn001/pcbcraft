//! The model side of the assistant as small traits: where keys live and who answers a chat request.
//!
//! Context: the pane asks these traits and never asks which shell it runs in. The desktop
//! gives a system keychain; the browser gives page storage; tests give memory. A model is a
//! [`ModelProvider`]: a cloud endpoint with a key today, a browser-local model or a page tool
//! provider later.
//! Rules: a key is a [`Key`], whose Debug output is "(key)"; a request never blocks the caller,
//! it reports through a callback, so one code path serves threads and the single-threaded browser.
//! Do not: print, log or format a key value, or add a provider kind without a settings row.

use crate::api::{
    ANTHROPIC_MAX_TOKENS, ANTHROPIC_VERSION, Protocol, REQUEST_TIMEOUT_S, endpoint, http_error,
    key_safe_for, redact,
};
use crate::backend::{HttpFn, HttpRequest, HttpResponse};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

/// A secret key. Debug shows "(key)", never the value.
///
/// What and why: carries a key from a [`ModelKeyStore`] to the one place that puts it in a header,
/// so a stray `{:?}` cannot leak it.
///
/// # Rules
/// The value is reachable only through [`Key::expose`].
///
/// # Dependencies
/// - crates/assistant/src/provider.rs (the stores that return it)
///
/// # Do not
/// Derive Debug, Display or Serialize on it.
#[derive(Clone, PartialEq, Eq)]
pub struct Key(String);

impl Key {
    /// Wrap a key value.
    pub fn new(value: &str) -> Key {
        Key(value.to_string())
    }

    /// The value, for building one request header.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("(key)")
    }
}

/// Where a [`ModelKeyStore`] keeps its keys, so settings can say it in plain words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeptIn {
    /// The operating system keychain.
    SystemKeychain,
    /// Page storage of the browser: readable by anything running on this page.
    BrowserStorage,
    /// Memory only, gone when the app closes.
    Memory,
}

/// Keeps named keys for model providers.
///
/// What and why: one place for "where does the key live", so the desktop keychain, browser
/// storage and a test double all plug in the same way.
///
/// # Rules
/// `get` of a missing name is `Ok(None)`; `clear` of a missing name is `Ok(())`. Errors are
/// plain sentences and never hold a key.
///
/// # Implementations
/// - crates/assistant/src/provider.rs (MemKeys, for tests)
/// - apps/pcbcraft/src/host.rs (the system keychain)
/// - apps/pcbcraft-web/src/browser_keys.rs (browser storage)
/// - crates/ui-egui/src/e2e/assistant_browser.rs (page storage double, test only)
///
/// # Dependencies
/// - crates/assistant/src/connections.rs (key_id builds the names)
///
/// # Do not
/// Return a key as a plain String, or write one to a log.
pub trait ModelKeyStore: Send + Sync {
    /// The key stored under `name`.
    fn get(&self, name: &str) -> Result<Option<Key>, String>;
    /// Store `key` under `name`, replacing any earlier one.
    fn set(&self, name: &str, key: &str) -> Result<(), String>;
    /// Forget the key under `name`.
    fn clear(&self, name: &str) -> Result<(), String>;
    /// Where the keys live.
    fn kept_in(&self) -> KeptIn;
}

/// A [`ModelKeyStore`] in memory, for tests and for a session that must not persist anything.
///
/// # Rules
/// Debug lists the names only.
///
/// # Dependencies
/// - crates/assistant/src/provider.rs (ModelKeyStore)
///
/// # Do not
/// Use it where a key must survive a restart.
#[derive(Default)]
pub struct MemKeys(Mutex<BTreeMap<String, String>>);

impl std::fmt::Debug for MemKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<String> = self
            .0
            .lock()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        f.debug_struct("MemKeys").field("names", &names).finish()
    }
}

/// Contract: crates/assistant/src/provider.rs
impl ModelKeyStore for MemKeys {
    fn get(&self, name: &str) -> Result<Option<Key>, String> {
        let map = self.0.lock().map_err(|e| e.to_string())?;
        Ok(map.get(name).map(|v| Key::new(v)))
    }
    fn set(&self, name: &str, key: &str) -> Result<(), String> {
        let mut map = self.0.lock().map_err(|e| e.to_string())?;
        map.insert(name.into(), key.into());
        Ok(())
    }
    fn clear(&self, name: &str) -> Result<(), String> {
        let mut map = self.0.lock().map_err(|e| e.to_string())?;
        map.remove(name);
        Ok(())
    }
    fn kept_in(&self) -> KeptIn {
        KeptIn::Memory
    }
}

/// A one-shot result callback; it runs once, on whichever thread or task finished the work.
pub type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// Sends one HTTP request and reports the reply through a callback.
///
/// What and why: the one seam where "how bytes leave the machine" differs. A thread and a
/// socket on the desktop, the browser's fetch on the web, a canned reply in tests.
///
/// # Rules
/// `done` is called exactly once. The call itself returns at once.
///
/// # Implementations
/// - crates/assistant/src/provider.rs (ThreadTransport)
/// - apps/pcbcraft-web/src/browser_keys.rs (BrowserTransport, with fetch)
/// - crates/ui-egui/src/e2e/assistant_browser.rs (a canned fetch, test only)
///
/// # Dependencies
/// - crates/assistant/src/backend.rs (HttpRequest, HttpResponse)
///
/// # Do not
/// Follow redirects or put a header value in an error.
pub trait Transport: Send + Sync {
    /// Start `request`; `done` gets the reply or a plain error.
    fn send(&self, request: HttpRequest, done: Done<HttpResponse>);
}

/// A [`Transport`] that runs a blocking [`HttpFn`] on a thread of its own.
///
/// # Rules
/// A thread that cannot start reports an error through `done`, never a panic.
///
/// # Dependencies
/// - crates/assistant/src/backend.rs (HttpFn)
///
/// # Do not
/// Use it where threads do not exist (the browser build).
pub struct ThreadTransport(pub HttpFn);

/// Contract: crates/assistant/src/provider.rs
impl Transport for ThreadTransport {
    fn send(&self, request: HttpRequest, done: Done<HttpResponse>) {
        let http = self.0.clone();
        let slot = Arc::new(Mutex::new(Some(done)));
        let held = slot.clone();
        let started = std::thread::Builder::new()
            .name("assistant-send".into())
            .spawn(move || {
                if let Some(done) = take(&held) {
                    done(http(&request));
                }
            });
        if started.is_err()
            && let Some(done) = take(&slot)
        {
            done(Err("Could not start the request.".into()));
        }
    }
}

fn take(slot: &Mutex<Option<Done<HttpResponse>>>) -> Option<Done<HttpResponse>> {
    match slot.lock() {
        Ok(mut g) => g.take(),
        Err(e) => e.into_inner().take(),
    }
}

/// The kinds of model the settings pane offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    /// A hosted or local HTTP endpoint, with a key when it needs one.
    Cloud,
    /// A model that runs in the browser on the graphics card. Not built yet.
    WebGpu,
    /// A model reached through the page tool interface of the browser. Not built yet.
    WebMcp,
}

impl ProviderKind {
    /// Every kind, in the order settings lists them.
    pub const ALL: [ProviderKind; 3] = [
        ProviderKind::Cloud,
        ProviderKind::WebGpu,
        ProviderKind::WebMcp,
    ];

    /// True when this kind can be added today.
    pub fn is_built(self) -> bool {
        self == ProviderKind::Cloud
    }
}

/// One chat request: the system text, the conversation so far and the tool list.
///
/// # Rules
/// `messages` and `tools` are in the wire format the provider reports from `wire`.
///
/// # Dependencies
/// - crates/assistant/src/api.rs (openai_tools, anthropic_tools build `tools`)
///
/// # Do not
/// Put a key in it.
#[derive(Clone, Debug)]
pub struct ChatRequest {
    /// Instructions for the model.
    pub system: String,
    /// The conversation, oldest first.
    pub messages: Vec<Value>,
    /// The tools the model may call.
    pub tools: Vec<Value>,
}

/// Answers one chat request.
///
/// What and why: the single interface for every way to reach a model. The reply is the
/// provider's JSON answer in its `wire` format, already checked for an error status.
///
/// # Rules
/// `done` is called exactly once. A provider that is not built answers with a plain error.
///
/// # Implementations
/// - crates/assistant/src/provider.rs (CloudProvider, MemProvider)
///
/// # Dependencies
/// - crates/assistant/src/provider_backend.rs (the backend that drives it)
///
/// # Do not
/// Block the caller or stream partial replies through this call.
pub trait ModelProvider: Send + Sync {
    /// Which kind this is.
    fn kind(&self) -> ProviderKind;
    /// The wire format of requests and replies.
    fn wire(&self) -> Protocol;
    /// Name shown as the agent.
    fn label(&self) -> String;
    /// Send `request`; report the reply through `done`.
    fn send(&self, request: ChatRequest, done: Done<Value>);
}

/// A cloud endpoint: base address, model name and a key read from a [`ModelKeyStore`] per request.
///
/// # Rules
/// The key goes only to https or this computer. Error text has the key taken out.
///
/// # Dependencies
/// - crates/assistant/src/api.rs (key_safe_for, endpoint, http_error)
///
/// # Do not
/// Cache the key past one request, or send it to any other address.
pub struct CloudProvider {
    protocol: Protocol,
    base_url: String,
    model: String,
    key_name: Option<String>,
    keys: Arc<dyn ModelKeyStore>,
    transport: Arc<dyn Transport>,
}

impl CloudProvider {
    /// A provider for `base_url` and `model`; `key_name` names its key in `keys`, if it has one.
    pub fn new(
        protocol: Protocol,
        base_url: &str,
        model: &str,
        key_name: Option<String>,
        keys: Arc<dyn ModelKeyStore>,
        transport: Arc<dyn Transport>,
    ) -> CloudProvider {
        CloudProvider {
            protocol,
            base_url: base_url.to_string(),
            model: model.to_string(),
            key_name,
            keys,
            transport,
        }
    }

    fn body(&self, request: &ChatRequest) -> Value {
        let mut body = match self.protocol {
            Protocol::OpenAi => {
                let mut messages = vec![json!({"role": "system", "content": request.system})];
                messages.extend(request.messages.iter().cloned());
                json!({"model": self.model, "messages": messages})
            }
            Protocol::Anthropic => json!({
                "model": self.model,
                "max_tokens": ANTHROPIC_MAX_TOKENS,
                "system": request.system,
                "messages": request.messages,
            }),
        };
        if !request.tools.is_empty() {
            body["tools"] = Value::Array(request.tools.clone());
        }
        body
    }
}

/// Contract: crates/assistant/src/provider.rs
impl ModelProvider for CloudProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Cloud
    }
    fn wire(&self) -> Protocol {
        self.protocol
    }
    fn label(&self) -> String {
        format!("API · {}", self.model)
    }
    fn send(&self, request: ChatRequest, done: Done<Value>) {
        let key = match &self.key_name {
            Some(name) => match self.keys.get(name) {
                Ok(k) => k,
                Err(e) => return done(Err(e)),
            },
            None => None,
        };
        if key.is_some() && !key_safe_for(&self.base_url) {
            return done(Err(
                "A key is only sent over https, or to this computer.".into()
            ));
        }
        let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        let url = match self.protocol {
            Protocol::OpenAi => format!("{}/chat/completions", self.base_url.trim_end_matches('/')),
            Protocol::Anthropic => {
                headers.push(("anthropic-version".into(), ANTHROPIC_VERSION.into()));
                endpoint(&self.base_url, "messages")
            }
        };
        if let Some(k) = &key {
            match self.protocol {
                Protocol::OpenAi => {
                    headers.push(("Authorization".into(), format!("Bearer {}", k.expose())));
                }
                Protocol::Anthropic => headers.push(("x-api-key".into(), k.expose().to_string())),
            }
        }
        let http = HttpRequest {
            method: "POST",
            url,
            headers,
            body: Some(self.body(&request).to_string().into_bytes()),
            timeout_s: REQUEST_TIMEOUT_S,
            stream_out: None,
        };
        let secret = key.map(|k| k.expose().to_string());
        self.transport.send(
            http,
            Box::new(move |result| {
                done(match result {
                    Err(e) => Err(redact(&e, secret.as_deref())),
                    Ok(r) => {
                        let v = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
                        if r.status == 200 {
                            Ok(v)
                        } else {
                            Err(http_error(r.status, &v))
                        }
                    }
                });
            }),
        );
    }
}

/// A [`ModelProvider`] that returns queued replies, for tests. No network.
///
/// # Rules
/// It answers at once, in order; with the queue empty it answers with an error. It keeps every
/// request it got.
///
/// # Dependencies
/// - crates/assistant/src/provider.rs (ModelProvider)
///
/// # Do not
/// Use it outside tests and demos.
pub struct MemProvider {
    protocol: Protocol,
    replies: Mutex<VecDeque<Result<Value, String>>>,
    seen: Mutex<Vec<ChatRequest>>,
}

impl MemProvider {
    /// A provider speaking `protocol` that answers with `replies`, oldest first.
    pub fn new(protocol: Protocol, replies: Vec<Result<Value, String>>) -> MemProvider {
        MemProvider {
            protocol,
            replies: Mutex::new(replies.into()),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// A reply of plain text in the OpenAI wire format.
    pub fn text_reply(text: &str) -> Value {
        json!({"choices": [{"message": {"role": "assistant", "content": text}, "finish_reason": "stop"}]})
    }

    /// How many requests it has received.
    pub fn requests(&self) -> usize {
        self.seen.lock().map_or(0, |s| s.len())
    }
}

/// Contract: crates/assistant/src/provider.rs
impl ModelProvider for MemProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Cloud
    }
    fn wire(&self) -> Protocol {
        self.protocol
    }
    fn label(&self) -> String {
        "test model".into()
    }
    fn send(&self, request: ChatRequest, done: Done<Value>) {
        if let Ok(mut s) = self.seen.lock() {
            s.push(request);
        }
        let next = self.replies.lock().ok().and_then(|mut q| q.pop_front());
        done(next.unwrap_or_else(|| Err("No canned reply left.".into())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Canned(Mutex<Vec<HttpRequest>>, u16, &'static str);

    /// Contract: crates/assistant/src/provider.rs
    impl Transport for Canned {
        fn send(&self, request: HttpRequest, done: Done<HttpResponse>) {
            if let Ok(mut v) = self.0.lock() {
                v.push(request);
            }
            done(Ok(HttpResponse::with_status(
                self.1,
                self.2.as_bytes().to_vec(),
            )));
        }
    }

    fn cloud(url: &str, status: u16, body: &'static str) -> (CloudProvider, Arc<Canned>) {
        let keys = Arc::new(MemKeys::default());
        let _ = keys.set("k", "test-key");
        let t = Arc::new(Canned(Mutex::new(Vec::new()), status, body));
        let p = CloudProvider::new(
            Protocol::OpenAi,
            url,
            "m",
            Some("k".into()),
            keys,
            t.clone(),
        );
        (p, t)
    }

    fn ask(p: &CloudProvider) -> Result<Value, String> {
        let out = Arc::new(Mutex::new(None));
        let sink = out.clone();
        p.send(
            ChatRequest {
                system: "s".into(),
                messages: vec![json!({"role": "user", "content": "hi"})],
                tools: vec![],
            },
            Box::new(move |r| {
                if let Ok(mut g) = sink.lock() {
                    *g = Some(r);
                }
            }),
        );
        let got = out.lock().map_err(|e| e.to_string())?.take();
        got.unwrap_or_else(|| Err("no answer".into()))
    }

    #[test]
    fn a_key_never_shows_in_debug_output() {
        let keys = MemKeys::default();
        assert!(keys.set("a", "test-key").is_ok());
        assert!(!format!("{keys:?}").contains("test-key"));
        assert_eq!(format!("{:?}", Key::new("test-key")), "(key)");
    }

    #[test]
    fn the_cloud_provider_sends_the_key_as_a_bearer_header() {
        let (p, t) = cloud("https://example.test/v1", 200, "{\"ok\":true}");
        assert!(ask(&p).is_ok());
        let sent = t.0.lock().map(|v| v[0].headers.clone()).unwrap_or_default();
        assert!(sent.contains(&("Authorization".into(), "Bearer test-key".into())));
    }

    #[test]
    fn a_key_is_refused_for_plain_http_on_another_computer() {
        let (p, t) = cloud("http://10.0.0.5/v1", 200, "{}");
        assert!(ask(&p).is_err());
        assert_eq!(t.0.lock().map_or(1, |v| v.len()), 0);
    }

    #[test]
    fn an_error_status_becomes_a_plain_sentence() {
        let (p, _) = cloud(
            "https://example.test/v1",
            401,
            "{\"error\":{\"message\":\"bad\"}}",
        );
        let e = ask(&p).err().unwrap_or_default();
        assert!(e.contains("did not accept the key"), "{e}");
        assert!(!e.contains("test-key"));
    }

    #[test]
    fn the_memory_provider_answers_in_order_and_counts() {
        let p = MemProvider::new(Protocol::OpenAi, vec![Ok(json!(1)), Err("x".into())]);
        let r = ChatRequest {
            system: String::new(),
            messages: vec![],
            tools: vec![],
        };
        let got = Arc::new(Mutex::new(Vec::new()));
        for _ in 0..2 {
            let g = got.clone();
            p.send(
                r.clone(),
                Box::new(move |v| {
                    if let Ok(mut g) = g.lock() {
                        g.push(v);
                    }
                }),
            );
        }
        assert_eq!(got.lock().map_or(0, |g| g.len()), 2);
        assert_eq!(p.requests(), 2);
    }
}
