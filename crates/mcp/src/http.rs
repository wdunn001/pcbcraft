//! MCP Streamable HTTP transport: one endpoint, bearer token, Origin check, sessions, SSE progress.
//!
//! Local rules: the bind is loopback unless `bind_http_with(.., true)` (CLI `--allow-remote`);
//! every request needs the token, and a non-loopback `Origin` or `Host` gets 403 unless the
//! operator opted in. `initialize` issues an `MCP-Session-Id`; an unknown id is 404, no id is
//! accepted. A `tools/call` with a `progressToken` that accepts `text/event-stream` streams
//! progress then the result. `notifications/cancelled` stops a queued call and ends the wait of
//! a running one; a running tool is not interrupted (`docs/mcp-http.md`). Tool calls take turns.
//!
//! # Dependencies
//! `crates/mcp/src/lib.rs` (`handle_rpc`, `loopback`, `rpc_error`, `SUPPORTED_VERSIONS`),
//! `apps/pcbcraft-cli/src/main.rs` (`mcp --http`), `docs/mcp-http.md`.
//!
//! # Do not
//! Do not write the token to stdout or a log line, skip the token, or make a remote bind implicit.
use crate::{DocumentHub, MAX_MESSAGE, SUPPORTED_VERSIONS, handle_rpc, loopback, rpc_error};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

const MAX_HEADER_BYTES: u64 = 16 * 1024;
const ENDPOINT: &str = "/mcp";
const MAX_CONNECTIONS: usize = 32;
const MAX_SESSIONS: usize = 64;
const KEEPALIVE: Duration = Duration::from_secs(15);
/// JSON-RPC code for a cancelled request.
const CANCELLED: i64 = -32800;

/// Who may connect, besides the bearer token.
///
/// # Rules
/// `allow_remote` only lifts the bind and `Host` limits; the token stays required.
/// `allow_origins` entries match the whole `Origin` header, case-insensitively.
///
/// # Do not
/// Do not put `*` in `allow_origins`; the CLI refuses it.
#[derive(Clone)]
pub struct HttpOptions {
    /// The bearer token every request must present.
    pub token: String,
    /// Lifts the loopback-only `Host` check; the bind itself is decided by `bind_http_with`.
    pub allow_remote: bool,
    /// Extra exact `Origin` values that pass besides loopback origins.
    pub allow_origins: Vec<String>,
}

impl HttpOptions {
    /// Loopback only, no extra origins.
    pub fn loopback(token: &str) -> Self {
        Self {
            token: token.to_string(),
            allow_remote: false,
            allow_origins: vec![],
        }
    }
}

/// The shared state of one HTTP endpoint: the hub, the options, sessions and running calls.
///
/// # Rules
/// `turn` serialises tool calls so one design session is edited by one call at a time.
///
/// # Dependencies
/// `crates/mcp/src/document_hub.rs` (`DocumentHub`).
///
/// # Do not
/// Do not hold `turn` while writing to a socket.
pub struct Server {
    hub: DocumentHub,
    options: HttpOptions,
    sessions: Mutex<VecDeque<String>>,
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
    turn: Mutex<()>,
    connections: AtomicUsize,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Server {
    /// A server over `hub` with no sessions and no running calls.
    pub fn new(hub: DocumentHub, options: HttpOptions) -> Self {
        Self {
            hub,
            options,
            sessions: Mutex::new(VecDeque::new()),
            running: Mutex::new(HashMap::new()),
            turn: Mutex::new(()),
            connections: AtomicUsize::new(0),
        }
    }

    /// Hold the tool-call turn, so tests can queue a call behind a busy one.
    #[cfg(test)]
    pub(crate) fn hold_turn(&self) -> MutexGuard<'_, ()> {
        lock(&self.turn)
    }

    /// How many tool calls are registered as running or queued.
    #[cfg(test)]
    pub(crate) fn running_calls(&self) -> usize {
        lock(&self.running).len()
    }

    fn open_session(&self) -> Result<String, String> {
        let id = new_token()?;
        let mut sessions = lock(&self.sessions);
        if sessions.len() >= MAX_SESSIONS {
            sessions.pop_front();
        }
        sessions.push_back(id.clone());
        Ok(id)
    }

    fn has_session(&self, id: &str) -> bool {
        lock(&self.sessions).iter().any(|known| known == id)
    }

    fn close_session(&self, id: &str) -> bool {
        let mut sessions = lock(&self.sessions);
        let before = sessions.len();
        sessions.retain(|known| known != id);
        sessions.len() != before
    }

    fn cancel(&self, session: &str, request_id: &Value) {
        if let Some(flag) = lock(&self.running).get(&call_key(session, request_id)) {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

fn call_key(session: &str, id: &Value) -> String {
    format!("{session}|{id}")
}

/// A parsed request head. Header names are lowercased.
struct Head {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
}

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// A refusal: status code, reason, extra header lines and the JSON body.
struct Refusal {
    code: u16,
    reason: &'static str,
    extra: &'static str,
    body: Value,
}

fn refuse(code: u16, reason: &'static str, message: &str) -> Refusal {
    Refusal {
        code,
        reason,
        extra: "",
        body: json!({"error":message}),
    }
}

/// Compare two secrets without stopping at the first differing byte.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for index in 0..a.len().max(b.len()) {
        diff |=
            usize::from(a.get(index).copied().unwrap_or(0) ^ b.get(index).copied().unwrap_or(0));
    }
    diff == 0
}

/// True for `http://127.0.0.1`, `http://localhost` or `http://[::1]` with an optional port.
///
/// The origin is parsed whole, never prefix-matched, so `http://localhost.evil.test` fails.
fn loopback_origin(origin: &str) -> bool {
    let Some(rest) = origin.strip_prefix("http://") else {
        return false;
    };
    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) if !host.ends_with(']') || host.starts_with('[') => (host, Some(port)),
        _ => (rest, None),
    };
    let port_ok =
        port.is_none_or(|p| (1..=5).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()));
    port_ok
        && ["127.0.0.1", "localhost", "[::1]"]
            .iter()
            .any(|h| host.eq_ignore_ascii_case(h))
}

fn accepts_json(accept: &str) -> bool {
    accept.split(',').any(|item| {
        let kind = item.split(';').next().unwrap_or("").trim();
        ["application/json", "application/*", "*/*"]
            .iter()
            .any(|ok| kind.eq_ignore_ascii_case(ok))
    })
}

fn read_head(reader: &mut BufReader<&TcpStream>) -> Result<Head, Refusal> {
    let mut budget = MAX_HEADER_BYTES;
    let mut next_line = |reader: &mut BufReader<&TcpStream>| -> Result<String, Refusal> {
        let mut bytes = vec![];
        let count = reader
            .take(budget)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| refuse(400, "Bad Request", "could not read the request"))?;
        budget = budget.saturating_sub(count as u64);
        if !bytes.ends_with(b"\n") {
            return Err(refuse(
                431,
                "Request Header Fields Too Large",
                "headers are too long",
            ));
        }
        String::from_utf8(bytes)
            .map(|line| line.trim_end().to_string())
            .map_err(|_| refuse(400, "Bad Request", "headers must be UTF-8"))
    };
    let request_line = next_line(reader)?;
    let mut parts = request_line.split(' ');
    let (Some(method), Some(path), Some(_version)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(refuse(400, "Bad Request", "malformed request line"));
    };
    let mut headers = vec![];
    loop {
        let line = next_line(reader)?;
        if line.is_empty() {
            break;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| refuse(400, "Bad Request", "malformed header"))?;
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
    Ok(Head {
        method: method.to_string(),
        path: path.to_string(),
        headers,
    })
}

fn accepts_stream(accept: &str) -> bool {
    accept.split(',').any(|item| {
        let kind = item.split(';').next().unwrap_or("").trim();
        kind.eq_ignore_ascii_case("text/event-stream")
    })
}

/// True when the `Origin` is a loopback origin or one the operator listed.
fn origin_allowed(server: &Server, origin: &str) -> bool {
    loopback_origin(origin)
        || server
            .options
            .allow_origins
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(origin))
}

/// Check the head in order and return the declared body length (0 for DELETE).
fn check_head(head: &Head, server: &Server) -> Result<usize, Refusal> {
    let length = match head.header("content-length") {
        Some(text) => Some(
            text.parse::<u64>()
                .map_err(|_| refuse(400, "Bad Request", "bad Content-Length"))?,
        ),
        None => None,
    };
    if length.is_some_and(|n| n > MAX_MESSAGE) {
        return Err(refuse(413, "Content Too Large", "body exceeds 1 MiB"));
    }
    if head.header("transfer-encoding").is_some() {
        return Err(refuse(
            400,
            "Bad Request",
            "chunked bodies are not supported",
        ));
    }
    if head.path != ENDPOINT {
        return Err(refuse(404, "Not Found", "the endpoint is /mcp"));
    }
    if head.method != "POST" && head.method != "DELETE" {
        return Err(Refusal {
            extra: "Allow: POST, DELETE\r\n",
            ..refuse(
                405,
                "Method Not Allowed",
                "use POST; the server never pushes events on its own",
            )
        });
    }
    if !server.options.allow_remote
        && head
            .header("host")
            .is_some_and(|host| !loopback_origin(&format!("http://{host}")))
    {
        return Err(refuse(403, "Forbidden", "host is not allowed"));
    }
    if head
        .header("origin")
        .is_some_and(|o| !origin_allowed(server, o))
    {
        return Err(refuse(403, "Forbidden", "origin is not allowed"));
    }
    let bearer = head
        .header("authorization")
        .and_then(|value| value.strip_prefix("Bearer "));
    if !bearer.is_some_and(|given| same_secret(given.trim(), &server.options.token)) {
        return Err(Refusal {
            extra: "WWW-Authenticate: Bearer\r\n",
            ..refuse(401, "Unauthorized", "a bearer token is required")
        });
    }
    if head.method == "DELETE" {
        return Ok(0);
    }
    if !head.header("accept").is_some_and(accepts_json) {
        return Err(refuse(
            406,
            "Not Acceptable",
            "Accept must list application/json",
        ));
    }
    if head
        .header("mcp-protocol-version")
        .is_some_and(|v| !SUPPORTED_VERSIONS.contains(&v))
    {
        return Err(refuse(
            400,
            "Bad Request",
            "unsupported MCP-Protocol-Version",
        ));
    }
    let length =
        length.ok_or_else(|| refuse(411, "Length Required", "Content-Length is required"))?;
    usize::try_from(length).map_err(|_| refuse(413, "Content Too Large", "body exceeds 1 MiB"))
}

/// What a request produced: a plain reply, or nothing because it was streamed already.
struct Reply {
    code: u16,
    reason: &'static str,
    extra: String,
    body: Option<Value>,
}

fn reply(code: u16, reason: &'static str, body: Option<Value>) -> Reply {
    Reply {
        code,
        reason,
        extra: String::new(),
        body,
    }
}

fn respond(stream: &mut TcpStream, code: u16, reason: &str, extra: &str, body: Option<&Value>) {
    let text = body.map(Value::to_string).unwrap_or_default();
    let kind = if body.is_some() {
        "Content-Type: application/json\r\n"
    } else {
        ""
    };
    let head = format!(
        "HTTP/1.1 {code} {reason}\r\n{kind}{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
        text.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(text.as_bytes());
    let _ = stream.flush();
}

/// Write one server-sent event; false when the client has gone.
fn send_event(stream: &mut TcpStream, id: usize, data: &Value) -> bool {
    let text = format!("id: {id}\nevent: message\ndata: {data}\n\n");
    stream.write_all(text.as_bytes()).is_ok() && stream.flush().is_ok()
}

fn progress_event(token: &Value, progress: u32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":token,"progress":progress,"total":2,"message":message}})
}

/// Messages from the worker thread that runs one tool call.
enum Work {
    Progress(u32, &'static str),
    Done(Option<Value>),
}

/// Run one `tools/call` and answer it, streaming progress when the client asked for it.
///
/// # Rules
/// The call waits for the turn lock on a worker thread. A cancel before the turn is granted
/// means the tool never runs. A cancel while it runs ends the wait with -32800; the tool
/// finishes in the background and its result is dropped.
///
/// # Dependencies
/// `crates/mcp/src/lib.rs` (`handle_rpc`).
///
/// # Do not
/// Do not claim an edit was undone after a cancel; only file-writing and read tools are safe
/// to abandon.
fn run_call(
    server: &Arc<Server>,
    stream: &mut TcpStream,
    message: Value,
    session: &str,
    wants_stream: bool,
) -> Option<Reply> {
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let token = message
        .pointer("/params/_meta/progressToken")
        .filter(|t| t.is_string() || t.is_number())
        .cloned();
    let key = call_key(session, &id);
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut running = lock(&server.running);
        if running.contains_key(&key) {
            return Some(reply(
                400,
                "Bad Request",
                Some(rpc_error(id, -32600, "request id is already in use")),
            ));
        }
        running.insert(key.clone(), Arc::clone(&flag));
    }
    let (sender, receiver) = channel();
    let worker = {
        let (server, flag) = (Arc::clone(server), Arc::clone(&flag));
        let sender = sender.clone();
        std::thread::Builder::new()
            .name("pcbcraft-mcp-call".into())
            .spawn(move || {
                let _turn = lock(&server.turn);
                if flag.load(Ordering::SeqCst) {
                    let _ = sender.send(Work::Done(None));
                    return;
                }
                let _ = sender.send(Work::Progress(1, "running"));
                let _ = sender.send(Work::Done(handle_rpc(&server.hub, message, None)));
            })
    };
    drop(sender);
    let streaming = wants_stream && token.is_some();
    let mut event = 0usize;
    let mut open = true;
    if streaming {
        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
        open = stream.write_all(head.as_bytes()).is_ok();
        if let (true, Some(token)) = (open, token.as_ref()) {
            open = send_event(stream, event, &progress_event(token, 0, "queued"));
            event += 1;
        }
    }
    let mut last_write = Instant::now();
    let outcome = if worker.is_err() {
        Some(rpc_error(id.clone(), -32603, "could not start the call"))
    } else {
        loop {
            match receiver.recv_timeout(Duration::from_millis(25)) {
                Ok(Work::Progress(step, text)) => {
                    if let (true, Some(token)) = (streaming && open, token.as_ref()) {
                        open = send_event(stream, event, &progress_event(token, step, text));
                        event += 1;
                        last_write = Instant::now();
                    }
                }
                Ok(Work::Done(Some(result))) if !flag.load(Ordering::SeqCst) => {
                    if let (true, Some(token)) = (streaming && open, token.as_ref()) {
                        open = send_event(stream, event, &progress_event(token, 2, "done"));
                        event += 1;
                    }
                    break Some(result);
                }
                Ok(Work::Done(_)) | Err(RecvTimeoutError::Disconnected) => {
                    break if flag.load(Ordering::SeqCst) {
                        Some(rpc_error(id.clone(), CANCELLED, "request cancelled"))
                    } else {
                        Some(rpc_error(id.clone(), -32603, "the call produced no reply"))
                    };
                }
                Err(RecvTimeoutError::Timeout) => {
                    if flag.load(Ordering::SeqCst) {
                        break Some(rpc_error(id.clone(), CANCELLED, "request cancelled"));
                    }
                    if streaming && open && last_write.elapsed() >= KEEPALIVE {
                        open =
                            stream.write_all(b": keepalive\n\n").is_ok() && stream.flush().is_ok();
                        last_write = Instant::now();
                    }
                }
            }
        }
    };
    lock(&server.running).remove(&key);
    if streaming {
        if let (true, Some(result)) = (open, outcome.as_ref()) {
            send_event(stream, event, result);
        }
        return None;
    }
    outcome.map(|result| reply(200, "OK", Some(result)))
}

/// Turn one parsed POST body into a reply (or stream it).
fn dispatch(
    server: &Arc<Server>,
    stream: &mut TcpStream,
    head: &Head,
    body: &[u8],
) -> Option<Reply> {
    let message = match serde_json::from_slice::<Value>(body) {
        Err(_) => {
            return Some(reply(
                400,
                "Bad Request",
                Some(rpc_error(Value::Null, -32700, "parse error")),
            ));
        }
        Ok(Value::Array(_)) => {
            return Some(reply(
                400,
                "Bad Request",
                Some(rpc_error(Value::Null, -32600, "batching is not supported")),
            ));
        }
        Ok(message) => message,
    };
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let method = method.as_str();
    if method.is_empty() && (message.get("result").is_some() || message.get("error").is_some()) {
        return Some(reply(202, "Accepted", None));
    }
    let session = head.header("mcp-session-id").unwrap_or("");
    if method != "initialize" && !session.is_empty() && !server.has_session(session) {
        return Some(reply(
            404,
            "Not Found",
            Some(json!({"error":"unknown session; send initialize again"})),
        ));
    }
    if method == "notifications/cancelled" {
        if let Some(request_id) = message.pointer("/params/requestId") {
            server.cancel(session, request_id);
        }
        return Some(reply(202, "Accepted", None));
    }
    let wants_stream = head.header("accept").is_some_and(accepts_stream);
    if method == "tools/call" && message.get("id").is_some() {
        return run_call(server, stream, message, session, wants_stream);
    }
    match handle_rpc(&server.hub, message, None) {
        Some(result) => {
            let mut out = reply(200, "OK", None);
            if method == "initialize" && result.get("result").is_some() {
                match server.open_session() {
                    Ok(id) => out.extra = format!("MCP-Session-Id: {id}\r\n"),
                    Err(_) => {
                        return Some(reply(
                            500,
                            "Internal Server Error",
                            Some(json!({"error":"no random bytes for a session id"})),
                        ));
                    }
                }
            }
            out.body = Some(result);
            Some(out)
        }
        None => Some(reply(202, "Accepted", None)),
    }
}

fn handle_connection(server: Arc<Server>, mut stream: TcpStream) {
    let timeout = Some(Duration::from_secs(3));
    if stream.set_read_timeout(timeout).is_err() || stream.set_write_timeout(timeout).is_err() {
        return;
    }
    let Ok(clone) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(&clone);
    let outcome = read_head(&mut reader).and_then(|head| {
        let length = check_head(&head, &server)?;
        let mut body = vec![0u8; length];
        reader
            .read_exact(&mut body)
            .map_err(|_| refuse(400, "Bad Request", "body shorter than Content-Length"))?;
        Ok((head, body))
    });
    match outcome {
        Ok((head, body)) => {
            let answer = if head.method == "DELETE" {
                let session = head.header("mcp-session-id").unwrap_or("");
                Some(if session.is_empty() {
                    reply(
                        400,
                        "Bad Request",
                        Some(json!({"error":"MCP-Session-Id is required"})),
                    )
                } else if server.close_session(session) {
                    reply(200, "OK", None)
                } else {
                    reply(404, "Not Found", Some(json!({"error":"unknown session"})))
                })
            } else {
                dispatch(&server, &mut stream, &head, &body)
            };
            if let Some(answer) = answer {
                respond(
                    &mut stream,
                    answer.code,
                    answer.reason,
                    &answer.extra,
                    answer.body.as_ref(),
                );
            }
        }
        Err(refusal) => respond(
            &mut stream,
            refusal.code,
            refusal.reason,
            refusal.extra,
            Some(&refusal.body),
        ),
    }
}

/// Bind a loopback address; `127.0.0.1:0` picks a free port.
///
/// # Rules
/// Only `loopback` decides which address is acceptable (literal loopback IP and port).
///
/// # Dependencies
/// `crates/mcp/src/lib.rs` (`loopback`).
///
/// # Do not
/// Do not resolve names; `localhost` is refused on purpose.
pub fn bind_http(address: &str) -> Result<TcpListener, String> {
    bind_http_with(address, false)
}

/// Bind `address`; with `allow_remote` any literal IP and port is accepted.
///
/// # Rules
/// `allow_remote` must come from an explicit operator flag, never from a default. Names are
/// never resolved.
///
/// # Dependencies
/// `crates/mcp/src/lib.rs` (`loopback`), `apps/pcbcraft-cli/src/main.rs` (`--allow-remote`).
///
/// # Do not
/// Do not call this with `true` unless the token is also required (`Server` always does).
pub fn bind_http_with(address: &str, allow_remote: bool) -> Result<TcpListener, String> {
    let socket = if allow_remote {
        address
            .parse::<SocketAddr>()
            .map_err(|_| "use a literal IP and port, e.g. 192.0.2.10:7992".to_string())?
    } else {
        loopback(address).map_err(|e| {
            format!("{e}; binding another address needs the explicit --allow-remote flag")
        })?
    };
    TcpListener::bind(socket).map_err(|e| e.to_string())
}

/// Serve requests on an already bound listener until the process ends.
///
/// Each connection runs on its own thread (at most 32 at once; more get 503), tool calls
/// take turns. Every request is checked against the bearer token before its body is read.
///
/// # Dependencies
/// `crates/mcp/src/lib.rs` (`handle_rpc`).
///
/// # Do not
/// Do not log the token or any request body.
pub fn serve_server(server: Arc<Server>, listener: &TcpListener) {
    for mut stream in listener.incoming().flatten() {
        if server.connections.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            server.connections.fetch_sub(1, Ordering::SeqCst);
            respond(
                &mut stream,
                503,
                "Service Unavailable",
                "",
                Some(&json!({"error":"too many connections"})),
            );
            continue;
        }
        let shared = Arc::clone(&server);
        let spawned = std::thread::Builder::new()
            .name("pcbcraft-mcp-http".into())
            .spawn(move || {
                handle_connection(Arc::clone(&shared), stream);
                shared.connections.fetch_sub(1, Ordering::SeqCst);
            });
        if spawned.is_err() {
            server.connections.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Serve on a bound listener with loopback-only options and one bearer `token`.
pub fn serve_listener(hub: &DocumentHub, listener: &TcpListener, token: &str) {
    let server = Server::new(hub.clone(), HttpOptions::loopback(token));
    serve_server(Arc::new(server), listener);
}

/// Bind `address` and serve MCP over HTTP until the process is killed.
///
/// # Rules
/// The address goes through `bind_http`, so only a literal loopback address works.
///
/// # Dependencies
/// `crates/mcp/src/lib.rs` (`DocumentHub`).
///
/// # Do not
/// Do not add a flag that skips the token.
pub fn serve_http(hub: DocumentHub, address: &str, token: &str) -> Result<(), String> {
    let listener = bind_http(address)?;
    serve_listener(&hub, &listener, token);
    Ok(())
}

/// Make 32 random bytes as 64 hex characters.
pub fn new_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("no random bytes: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Bind, make a token, and tell the user where to connect, on `notes` only.
///
/// With `token_file` the token goes to that file (mode 0600 on Unix) and is not printed.
/// Without it the token is printed to `notes`. The caller passes stderr, so the token never
/// reaches stdout.
///
/// # Dependencies
/// `apps/pcbcraft-cli/src/main.rs` passes `std::io::stderr()`.
///
/// # Do not
/// Do not accept a stdout writer here, and do not echo the token when a file is given.
pub fn start_http(
    address: &str,
    token_file: Option<&Path>,
    notes: &mut impl Write,
) -> Result<(TcpListener, String), String> {
    start_http_with(address, false, token_file, notes)
}

/// Like `start_http`, with `allow_remote` for a non-loopback bind and a warning on `notes`.
///
/// # Do not
/// Do not skip the warning: a remote bind exposes file-writing tools to the network.
pub fn start_http_with(
    address: &str,
    allow_remote: bool,
    token_file: Option<&Path>,
    notes: &mut impl Write,
) -> Result<(TcpListener, String), String> {
    let listener = bind_http_with(address, allow_remote)?;
    let token = new_token()?;
    let local = listener.local_addr().map_err(|e| e.to_string())?;
    writeln!(notes, "MCP endpoint: http://{local}{ENDPOINT}").map_err(|e| e.to_string())?;
    if allow_remote && !local.ip().is_loopback() {
        writeln!(
            notes,
            "Warning: this endpoint is reachable from other machines and can write files as you. Keep the token secret and put it behind TLS."
        )
        .map_err(|e| e.to_string())?;
    }
    match token_file {
        Some(path) => {
            write_private(path, &token)?;
            writeln!(notes, "Bearer token written to {}", path.display())
                .map_err(|e| e.to_string())?;
        }
        None => writeln!(notes, "Bearer token: {token}").map_err(|e| e.to_string())?,
    }
    Ok((listener, token))
}

fn write_private(path: &Path, token: &str) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(token.as_bytes()).map_err(|e| e.to_string())
}
