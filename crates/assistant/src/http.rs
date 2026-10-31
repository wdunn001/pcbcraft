//! Plain `http://` client for the API backend: std only, no TLS, no redirects.
//!
//! Context: [`crate::HttpFn`] is injected by the host. This is the host-independent
//! implementation for loopback and LAN endpoints; `https://` stays the app layer's job.
//! Local rules: every request has a deadline, the reply body has a size cap, a redirect
//! is returned as its status and never followed, no panics.

use crate::backend::{HttpFn, HttpRequest, HttpResponse, deliver_streamed_body};
use pcbcraft_clock::Instant;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

/// Largest reply body accepted, in bytes.
pub const MAX_BODY: usize = 8 * 1024 * 1024;
/// Largest reply header block accepted, in bytes.
const MAX_HEAD: usize = 64 * 1024;

/// The plain-HTTP implementation of [`HttpFn`].
///
/// What and why: lets the real backend run over a socket without a host app, so the
/// end-to-end tests exercise the same transport code.
///
/// # Rules
/// The `timeout_s` of the request is one deadline for connect, send and receive.
/// # Dependencies
/// `src/backend.rs` (`HttpRequest`, `HttpFn`).
/// # Do not
/// Follow redirects, add TLS, or put the request headers in an error text.
pub fn plain_http() -> HttpFn {
    Arc::new(plain_request)
}

/// Send one request over `http://` and return the status and the body.
///
/// # Rules
/// Errors are plain sentences. A reply cut short of its announced length is an error.
/// # Dependencies
/// `src/backend.rs`.
/// # Do not
/// Echo header values (they may hold a key).
pub fn plain_request(req: &HttpRequest) -> Result<HttpResponse, String> {
    let (host, port, path) = split_url(&req.url)?;
    let deadline = pcbcraft_clock::instant_now() + Duration::from_secs(req.timeout_s.max(1));
    let mut stream = connect(&host, port, deadline)?;
    let mut head = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/json\r\n",
        req.method, path, host
    );
    for (k, v) in &req.headers {
        if k.contains(['\r', '\n', ':']) || v.contains(['\r', '\n']) {
            return Err("A request header was not valid.".into());
        }
        let lower = k.to_ascii_lowercase();
        if matches!(lower.as_str(), "host" | "connection" | "content-length") {
            continue;
        }
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    let body = req.body.as_deref().unwrap_or(&[]);
    if req.body.is_some() {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    set_timeouts(&stream, deadline)?;
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(body))
        .map_err(|e| io_text(&e, "send the request"))?;
    let raw = read_all(&mut stream, deadline)?;
    let (status, body) = parse_reply(&raw)?;
    if let Some(out) = &req.stream_out {
        return deliver_streamed_body(out, status, &body);
    }
    Ok((status, body))
}

fn split_url(url: &str) -> Result<(String, u16, String), String> {
    let lower = url.to_ascii_lowercase();
    let rest = if lower.starts_with("http://") {
        &url[7..]
    } else if lower.starts_with("https://") {
        return Err("This connection cannot do https here; use an http address.".into());
    } else {
        return Err("The address must start with http://".into());
    };
    let (auth, path) = match rest.find(['/', '?']) {
        Some(i) if rest.as_bytes()[i] == b'/' => (&rest[..i], rest[i..].to_string()),
        Some(i) => (&rest[..i], format!("/{}", &rest[i..])),
        None => (rest, "/".to_string()),
    };
    if auth.is_empty() || auth.contains('@') {
        return Err("The address has no usable host.".into());
    }
    let (host, port) = match auth.rsplit_once(':') {
        Some((h, p)) if !p.contains(']') => {
            let port = p
                .parse::<u16>()
                .map_err(|_| "The port is not a number.".to_string())?;
            (h.to_string(), port)
        }
        _ => (auth.to_string(), 80),
    };
    if path.contains(['\r', '\n', ' ']) || host.contains(['\r', '\n', ' ']) {
        return Err("The address has characters that are not allowed.".into());
    }
    Ok((host, port, path))
}

fn connect(host: &str, port: u16, deadline: Instant) -> Result<TcpStream, String> {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let addrs = (bare, port)
        .to_socket_addrs()
        .map_err(|_| "The host name could not be found.".to_string())?;
    let mut last = "No address to connect to.".to_string();
    for a in addrs {
        let left = deadline.saturating_duration_since(pcbcraft_clock::instant_now());
        if left.is_zero() {
            return Err("The request timed out.".into());
        }
        match TcpStream::connect_timeout(&a, left) {
            Ok(s) => return Ok(s),
            Err(e) => last = io_text(&e, "connect"),
        }
    }
    Err(last)
}

fn set_timeouts(s: &TcpStream, deadline: Instant) -> Result<(), String> {
    let left = deadline.saturating_duration_since(pcbcraft_clock::instant_now());
    if left.is_zero() {
        return Err("The request timed out.".into());
    }
    s.set_read_timeout(Some(left))
        .and_then(|_| s.set_write_timeout(Some(left)))
        .map_err(|e| io_text(&e, "set up the connection"))
}

fn io_text(e: &std::io::Error, what: &str) -> String {
    use std::io::ErrorKind::{TimedOut, WouldBlock};
    if matches!(e.kind(), TimedOut | WouldBlock) {
        "The request timed out.".into()
    } else {
        format!("Could not {what}: {}.", e.kind())
    }
}

fn read_all(s: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        set_timeouts(s, deadline)?;
        match s.read(&mut buf) {
            Ok(0) => return Ok(out),
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if out.len() > MAX_BODY + MAX_HEAD {
                    return Err("The reply is larger than the limit.".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            // A reset after some bytes is a cut-off reply; parse_reply says so if short.
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset && !out.is_empty() => {
                return Ok(out);
            }
            Err(e) => return Err(io_text(&e, "read the reply")),
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn parse_reply(raw: &[u8]) -> Result<HttpResponse, String> {
    let end = find(raw, b"\r\n\r\n").ok_or("The reply ended before its headers did.")?;
    if end > MAX_HEAD {
        return Err("The reply headers are larger than the limit.".into());
    }
    let head = String::from_utf8_lossy(&raw[..end]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or("The reply was not HTTP.")?;
    let mut length: Option<usize> = None;
    let mut chunked = false;
    let mut headers: Vec<(String, String)> = Vec::new();
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
            if k == "content-length" {
                length = v.parse().ok();
            } else if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            }
            headers.push((k, v));
        }
    }
    let body = &raw[end + 4..];
    let body = if chunked {
        dechunk(body)?
    } else if let Some(n) = length {
        if body.len() < n {
            return Err("The reply was cut off before it finished.".into());
        }
        body[..n].to_vec()
    } else {
        body.to_vec()
    };
    if body.len() > MAX_BODY {
        return Err("The reply is larger than the limit.".into());
    }
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

fn dechunk(mut b: &[u8]) -> Result<Vec<u8>, String> {
    let cut = || "The reply was cut off before it finished.".to_string();
    let mut out = Vec::new();
    loop {
        let eol = find(b, b"\r\n").ok_or_else(cut)?;
        let size_text = String::from_utf8_lossy(&b[..eol]).into_owned();
        let size_text = size_text.split(';').next().unwrap_or("").trim().to_string();
        let size = usize::from_str_radix(&size_text, 16)
            .map_err(|_| "The reply had a bad chunk size.".to_string())?;
        b = &b[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if b.len() < size + 2 {
            return Err(cut());
        }
        out.extend_from_slice(&b[..size]);
        if out.len() > MAX_BODY {
            return Err("The reply is larger than the limit.".into());
        }
        b = &b[size + 2..];
    }
}
