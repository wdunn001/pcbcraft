//! Test-only fake OpenAI-compatible server on 127.0.0.1:0, plus small helpers.
//!
//! Context: `FakeServer` replays a script of replies, one per request, and records what it
//! received. Local rules: loopback only, every wait has a timeout, no panics in helpers.
#![allow(dead_code)]

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One scripted reply.
///
/// # Do not
/// Put a real key in a reply body.
#[derive(Clone)]
pub struct Reply {
    pub status: u16,
    pub body: String,
    pub delay_ms: u64,
    /// When set, the HTTP header uses this length but only `body` bytes are sent (cut-off reply).
    pub declared_len: Option<usize>,
}

impl Reply {
    /// A JSON reply with `status`.
    pub fn json(status: u16, v: Value) -> Reply {
        Reply {
            status,
            body: v.to_string(),
            delay_ms: 0,
            declared_len: None,
        }
    }
    /// A 200 chat completion with plain text.
    pub fn text(t: &str) -> Reply {
        Reply::json(
            200,
            json!({"choices": [{"message": {"role": "assistant", "content": t}}]}),
        )
    }
    /// A 200 chat completion asking for `n` calls of tool `name` with empty arguments.
    pub fn calls(name: &str, n: usize) -> Reply {
        let calls: Vec<Value> = (0..n)
            .map(|i| {
                json!({"id": format!("c{i}"), "type": "function",
                "function": {"name": name, "arguments": "{}"}})
            })
            .collect();
        Reply::json(
            200,
            json!({"choices": [{"message": {"role": "assistant",
            "content": null, "tool_calls": calls}}]}),
        )
    }
    /// The same reply after waiting `ms` milliseconds.
    pub fn after(mut self, ms: u64) -> Reply {
        self.delay_ms = ms;
        self
    }
    /// A 200 Anthropic message with plain text.
    pub fn anthropic_text(t: &str) -> Reply {
        Reply::json(
            200,
            json!({"content": [{"type": "text", "text": t}], "stop_reason": "end_turn"}),
        )
    }
    /// A 200 Anthropic message asking for `n` tool uses of `name` with empty input.
    pub fn anthropic_calls(name: &str, n: usize) -> Reply {
        let blocks: Vec<Value> = (0..n)
            .map(|i| json!({"type": "tool_use", "id": format!("c{i}"), "name": name, "input": {}}))
            .collect();
        Reply::json(200, json!({"content": blocks, "stop_reason": "tool_use"}))
    }
    /// A reply whose headers promise `full_len` bytes but the connection sends only `send_len`.
    ///
    /// # Do not
    /// Use this for a body that is meant to parse as complete JSON when fully sent.
    pub fn cut(full_body: &str, send_len: usize) -> Reply {
        let send = full_body.len().min(send_len);
        Reply {
            status: 200,
            body: full_body[..send].to_string(),
            delay_ms: 0,
            declared_len: Some(full_body.len()),
        }
    }
}

/// What the server saw for one request.
#[derive(Clone, Debug)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

/// The fake server; it stops when dropped.
pub struct FakeServer {
    pub url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    }
}

impl FakeServer {
    /// Bind a loopback port and answer POSTs from `script` in order (the last reply repeats).
    /// `GET` answers an empty model list.
    pub fn start(script: Vec<Reply>) -> Result<FakeServer, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let log = seen.clone();
        std::thread::spawn(move || {
            let mut posts = 0usize;
            for conn in listener.incoming() {
                let Ok(conn) = conn else { continue };
                let Some(req) = read_request(&conn) else {
                    continue;
                };
                let is_post = req.method == "POST";
                lock(&log).push(req);
                let reply = if is_post {
                    let r = script.get(posts).or(script.last()).cloned();
                    posts += 1;
                    r
                } else {
                    Some(Reply::json(200, json!({"data": []})))
                };
                if let Some(r) = reply {
                    std::thread::spawn(move || answer(conn, r));
                }
            }
        });
        Ok(FakeServer {
            url: format!("http://127.0.0.1:{port}/v1"),
            seen,
        })
    }

    /// Every request so far.
    pub fn requests(&self) -> Vec<Seen> {
        lock(&self.seen).clone()
    }

    /// The JSON bodies of the POST requests so far.
    pub fn posts(&self) -> Vec<Seen> {
        self.requests()
            .into_iter()
            .filter(|r| r.method == "POST")
            .collect()
    }
}

fn answer(mut c: TcpStream, r: Reply) {
    std::thread::sleep(Duration::from_millis(r.delay_ms));
    let length = r.declared_len.unwrap_or_else(|| r.body.len());
    let head = format!(
        "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        r.status, length
    );
    let _ = c.write_all(head.as_bytes());
    let _ = c.write_all(r.body.as_bytes());
}

fn read_request(mut c: &TcpStream) -> Option<Seen> {
    let _ = c.set_read_timeout(Some(Duration::from_secs(10)));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        let n = c.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let path = first.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let len = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[end + 4..].to_vec();
    while body.len() < len {
        let n = c.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
    Some(Seen {
        method,
        path,
        headers,
        body,
    })
}

/// The name of the first registry tool, so scripted calls are real ones.
pub fn first_tool_name() -> Result<String, String> {
    pcbcraft_mcp::tool_list()["tools"][0]["name"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "no tools".to_string())
}

/// The wire name of the `execute` tool (places components through the engine).
pub fn execute_wire_name() -> String {
    pcbcraft_assistant::to_wire("execute")
}

/// Check one recorded POST for legal tool ids and wire-safe tool names.
///
/// Context: the backend must send OpenAI `tool_call_id` and Anthropic `tool_use_id` on every tool
/// result, and tool names on the wire must not contain dots.
/// Local rules: plain `Err` sentences, no panics.
pub fn assert_request_shape(seen: &Seen) -> Result<(), String> {
    if seen.method != "POST" {
        return Ok(());
    }
    let body = &seen.body;
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        for t in tools {
            let name = t
                .get("name")
                .or_else(|| t.pointer("/function/name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if name.contains('.') {
                return Err(format!("tool name on the wire had a dot: {name}"));
            }
        }
    }
    let Some(msgs) = body.get("messages").and_then(Value::as_array) else {
        return Ok(());
    };
    for m in msgs {
        if m.get("role").and_then(Value::as_str) == Some("tool")
            && m.get("tool_call_id").and_then(Value::as_str).is_none()
        {
            return Err("a tool message had no tool_call_id".into());
        }
        let content = m.get("content");
        if let Some(Value::Array(blocks)) = content {
            for b in blocks {
                if b.get("type").and_then(Value::as_str) == Some("tool_result")
                    && b.get("tool_use_id").and_then(Value::as_str).is_none()
                {
                    return Err("a tool_result block had no tool_use_id".into());
                }
            }
        }
    }
    Ok(())
}
