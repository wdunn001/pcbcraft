//! MCP Streamable HTTP transport: one loopback endpoint, POST only, plain JSON replies.
//!
//! Local rules: the listener only ever binds an address that `loopback` accepts, every
//! request needs the per-run bearer token, and a browser `Origin` that is not a loopback
//! origin is refused (DNS rebinding guard). The transport is stateless: no session id is
//! issued, so the lifecycle gate of `serve` does not apply. No event stream: GET and DELETE
//! answer 405. Requests run one at a time (one design session) through `handle_rpc`.
//!
//! # Dependencies
//! `crates/mcp/src/lib.rs` (`handle_rpc`, `loopback`, `rpc_error`, `MAX_MESSAGE`,
//! `SUPPORTED_VERSIONS`), `apps/pcbcraft-cli/src/main.rs` (`mcp --http`).
//!
//! # Do not
//! Do not add a way to bind a non-loopback address. Do not write the token to stdout or
//! to a log line. Do not add SSE or session ids here without an issue.
use crate::{DocumentHub, MAX_MESSAGE, SUPPORTED_VERSIONS, handle_rpc, loopback, rpc_error};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

const MAX_HEADER_BYTES: u64 = 16 * 1024;
const ENDPOINT: &str = "/mcp";

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

/// Check the head in order and return the declared body length.
fn check_head(head: &Head, token: &str) -> Result<usize, Refusal> {
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
    if head.method != "POST" {
        return Err(Refusal {
            extra: "Allow: POST\r\n",
            ..refuse(
                405,
                "Method Not Allowed",
                "use POST; there is no event stream",
            )
        });
    }
    if head.header("origin").is_some_and(|o| !loopback_origin(o)) {
        return Err(refuse(403, "Forbidden", "origin is not allowed"));
    }
    let bearer = head
        .header("authorization")
        .and_then(|value| value.strip_prefix("Bearer "));
    if !bearer.is_some_and(|given| same_secret(given.trim(), token)) {
        return Err(Refusal {
            extra: "WWW-Authenticate: Bearer\r\n",
            ..refuse(401, "Unauthorized", "a bearer token is required")
        });
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

/// Turn one parsed body into a status and an optional JSON reply.
fn dispatch(hub: &DocumentHub, body: &[u8]) -> (u16, &'static str, Option<Value>) {
    match serde_json::from_slice::<Value>(body) {
        Err(_) => (
            400,
            "Bad Request",
            Some(rpc_error(Value::Null, -32700, "parse error")),
        ),
        Ok(Value::Array(_)) => (
            400,
            "Bad Request",
            Some(rpc_error(Value::Null, -32600, "batching is not supported")),
        ),
        Ok(message) => {
            let is_client_response = message.get("method").is_none()
                && (message.get("result").is_some() || message.get("error").is_some());
            if is_client_response {
                return (202, "Accepted", None);
            }
            match handle_rpc(hub, message, None) {
                Some(reply) => (200, "OK", Some(reply)),
                None => (202, "Accepted", None),
            }
        }
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

fn handle_connection(hub: &DocumentHub, mut stream: TcpStream, token: &str) {
    let timeout = Some(Duration::from_secs(3));
    if stream.set_read_timeout(timeout).is_err() || stream.set_write_timeout(timeout).is_err() {
        return;
    }
    let Ok(clone) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(&clone);
    let outcome = read_head(&mut reader).and_then(|head| {
        let length = check_head(&head, token)?;
        let mut body = vec![0u8; length];
        reader
            .read_exact(&mut body)
            .map_err(|_| refuse(400, "Bad Request", "body shorter than Content-Length"))?;
        Ok(body)
    });
    match outcome {
        Ok(body) => {
            let (code, reason, reply) = dispatch(hub, &body);
            respond(&mut stream, code, reason, "", reply.as_ref());
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
    TcpListener::bind(loopback(address)?).map_err(|e| e.to_string())
}

/// Serve requests on an already bound listener until the process ends.
///
/// One request at a time, on the calling thread. Every request is checked against the
/// bearer `token` before its body is read.
///
/// # Dependencies
/// `crates/mcp/src/lib.rs` (`handle_rpc`).
///
/// # Do not
/// Do not log the token or any request body.
pub fn serve_listener(hub: &DocumentHub, listener: &TcpListener, token: &str) {
    for stream in listener.incoming().flatten() {
        handle_connection(hub, stream, token);
    }
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
/// Without it the token is printed to `notes`. The caller passes stderr, so stdout stays
/// free for nothing at all in this mode and the token never reaches it.
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
    let listener = bind_http(address)?;
    let token = new_token()?;
    let local = listener.local_addr().map_err(|e| e.to_string())?;
    writeln!(notes, "MCP endpoint: http://{local}{ENDPOINT}").map_err(|e| e.to_string())?;
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
