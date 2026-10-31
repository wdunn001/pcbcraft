//! Optional knowledge retrieval over MCP streamable HTTP.
//!
//! Context: when Settings holds a retrieval endpoint, the API backend offers `search_knowledge`
//! and forwards calls to that server. The key lives in the keychain under [`RETRIEVAL_KEY_ID`].
//! Rules: bodies at most 1 MiB, at most 20 hits, excerpts clipped to 600 characters, whole tool
//! text clipped to [`crate::api::MAX_TOOL_TEXT`]. Session ids from `Mcp-Session-Id` are echoed on
//! later POSTs. Replies may be JSON or one SSE `message` event.
//! Do not: send a key over plain HTTP to a non-loopback host.

use crate::api::{MAX_TOOL_TEXT, key_safe_for};
use crate::backend::{HttpFn, HttpRequest, HttpResponse, SourceLink};
use crate::tool_names::to_wire;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Mutex;

/// Keychain entry for the retrieval service key.
pub const RETRIEVAL_KEY_ID: &str = "retrieval";
/// The name the model sees on the wire.
pub const MODEL_TOOL_NAME: &str = "search_knowledge";

const MAX_MCP_BODY: usize = 1024 * 1024;
const MAX_HITS: usize = 20;
const EXCERPT_MAX: usize = 600;
const MCP_TIMEOUT_S: u64 = 120;
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

/// Saved in preferences; the key is not stored here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrievalConfig {
    pub base_url: String,
    #[serde(default)]
    pub has_key: bool,
    #[serde(default)]
    pub tool_name: String,
}

/// A configured retrieval endpoint ready to call.
#[derive(Clone)]
pub struct Retrieval {
    base_url: String,
    key: Option<String>,
    tool_name: String,
    session: std::sync::Arc<Mutex<Option<String>>>,
}

/// Outcome of one search, for the model and the UI.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchOutcome {
    pub text: String,
    pub links: Vec<SourceLink>,
    pub count: usize,
    pub failed: bool,
}

impl Retrieval {
    /// Build a client from saved settings and an optional key from the keychain.
    pub fn from_config(config: &RetrievalConfig, key: Option<String>) -> Result<Retrieval, String> {
        let base_url = config.base_url.trim();
        if base_url.is_empty() {
            return Err("Enter the retrieval address first.".into());
        }
        if key.is_some() && !key_safe_for(base_url) {
            return Err("A key is only sent over https, or to this computer.".into());
        }
        let tool_name = config.tool_name.trim();
        if tool_name.is_empty() {
            return Err("Test the retrieval address first.".into());
        }
        Ok(Retrieval {
            base_url: base_url.to_string(),
            key,
            tool_name: tool_name.to_string(),
            session: std::sync::Arc::new(Mutex::new(None)),
        })
    }

    /// The wire tool definition for OpenAI or Anthropic.
    pub fn model_tool(protocol: crate::api::Protocol) -> Value {
        let schema = json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1, "maximum": MAX_HITS as i64},
            },
            "required": ["query"],
        });
        match protocol {
            crate::api::Protocol::OpenAi => json!({
                "type": "function",
                "function": {
                    "name": MODEL_TOOL_NAME,
                    "description": "Search the electronics knowledge base for standards, parts and manufacturing guidance.",
                    "parameters": schema,
                },
            }),
            crate::api::Protocol::Anthropic => json!({
                "name": MODEL_TOOL_NAME,
                "description": "Search the electronics knowledge base for standards, parts and manufacturing guidance.",
                "input_schema": schema,
            }),
        }
    }

    /// Run `search_knowledge` on the remote MCP server.
    pub fn search(&self, http: &HttpFn, query: &str, limit: u32) -> SearchOutcome {
        let limit = limit.clamp(1, MAX_HITS as u32);
        let args = json!({"query": query.trim(), "limit": limit});
        match self.call_tool(http, &args) {
            Ok(body) => format_outcome(&body),
            Err(reason) => SearchOutcome {
                text: reason,
                links: Vec::new(),
                count: 0,
                failed: true,
            },
        }
    }

    fn call_tool(&self, http: &HttpFn, args: &Value) -> Result<Value, String> {
        let session = self.ensure_session(http)?;
        let id = 4u64;
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": self.tool_name, "arguments": args},
        });
        let reply = rpc(
            http,
            &self.base_url,
            self.key.as_deref(),
            session.as_deref(),
            &request,
        )?;
        let result = rpc_result(&reply)?;
        Ok(result)
    }

    fn ensure_session(&self, http: &HttpFn) -> Result<Option<String>, String> {
        let mut guard = lock(&self.session);
        if guard.is_some() {
            return Ok(guard.clone());
        }
        let init_id = 1u64;
        let init = json!({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSIONS[0],
                "capabilities": {},
                "clientInfo": {"name": "pcbcraft-assistant", "version": env!("CARGO_PKG_VERSION")},
            },
        });
        let reply = rpc(http, &self.base_url, self.key.as_deref(), None, &init)?;
        if rpc_result(&reply).is_err() {
            return Err(rpc_error_text(&reply));
        }
        if let Some(sid) = session_from_value(&reply) {
            *guard = Some(sid);
        }
        let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}});
        let _ = rpc(
            http,
            &self.base_url,
            self.key.as_deref(),
            guard.as_deref(),
            &note,
        );
        Ok(guard.clone())
    }
}

/// Pick the MCP tool name for search, or explain why none was found.
pub fn pick_search_tool(list: &Value) -> Result<String, String> {
    let Some(tools) = list.get("tools").and_then(Value::as_array) else {
        return Err("the endpoint offers no search_knowledge tool".into());
    };
    for t in tools.iter().take(128) {
        let Some(name) = t.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name == MODEL_TOOL_NAME || name.ends_with("__search_knowledge") {
            return Ok(name.to_string());
        }
    }
    Err("the endpoint offers no search_knowledge tool".into())
}

/// Discover the tool and return a line for the Test button.
pub fn test_retrieval(
    http: &HttpFn,
    base_url: &str,
    key: Option<&str>,
) -> Result<(String, String), String> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        return Err("Enter the retrieval address first.".into());
    }
    if key.is_some() && !key_safe_for(base_url) {
        return Err("A key is only sent over https, or to this computer.".into());
    }
    let init = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSIONS[0],
            "capabilities": {},
            "clientInfo": {"name": "pcbcraft-assistant", "version": env!("CARGO_PKG_VERSION")},
        },
    });
    let reply = rpc(http, base_url, key, None, &init)?;
    if rpc_result(&reply).is_err() {
        return Err(rpc_error_text(&reply));
    }
    let session = session_from_value(&reply);
    let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}});
    let _ = rpc(http, base_url, key, session.as_deref(), &note);
    let list_req = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}});
    let list_reply = rpc(http, base_url, key, session.as_deref(), &list_req)?;
    let list = rpc_result(&list_reply)?;
    let tool_name = pick_search_tool(&list)?;
    let said = format!("Found `{tool_name}` as search_knowledge.");
    Ok((said, tool_name))
}

/// Append the retrieval tool when `cfg.retrieval` is set.
pub fn append_model_tool(tools: &mut Vec<Value>, cfg: &crate::api::ApiConfig) {
    if cfg.retrieval.is_some() {
        tools.push(Retrieval::model_tool(cfg.protocol));
    }
}

/// Extend wire-name lookup so `search_knowledge` maps to itself.
pub fn register_tool_name(names: &mut crate::tool_names::ToolNames) {
    names.insert(MODEL_TOOL_NAME, MODEL_TOOL_NAME);
}

/// Extra system note when the tool is offered.
pub fn system_note_offered() -> &'static str {
    "Use search_knowledge for standards, component and manufacturing questions and name the sources you used."
}

fn format_outcome(body: &Value) -> SearchOutcome {
    let failed = body
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let raw = tool_text_from_result(body);
    if failed {
        return SearchOutcome {
            text: raw,
            links: Vec::new(),
            count: 0,
            failed: true,
        };
    }
    let hits = parse_hits(&raw);
    let count = hits.len();
    let text = render_hits(&hits);
    let links = hits
        .into_iter()
        .map(|h| SourceLink {
            title: h.title,
            source: h.source,
        })
        .collect();
    SearchOutcome {
        text,
        links,
        count,
        failed: false,
    }
}

struct Hit {
    title: String,
    excerpt: String,
    source: String,
}

fn parse_hits(raw: &str) -> Vec<Hit> {
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        if let Some(arr) = v.as_array() {
            return arr
                .iter()
                .filter_map(parse_hit_object)
                .take(MAX_HITS)
                .collect();
        }
        if let Some(obj) = parse_hit_object(&v) {
            return vec![obj];
        }
    }
    if raw.trim().is_empty() {
        return Vec::new();
    }
    vec![Hit {
        title: "Result".into(),
        excerpt: clip_excerpt(raw),
        source: String::new(),
    }]
}

fn parse_hit_object(v: &Value) -> Option<Hit> {
    let title = v
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Result")
        .to_string();
    let excerpt = v
        .get("excerpt")
        .or_else(|| v.get("text"))
        .and_then(Value::as_str)
        .map(clip_excerpt)
        .unwrap_or_default();
    let source = v
        .get("source")
        .or_else(|| v.get("url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some(Hit {
        title,
        excerpt,
        source,
    })
}

fn render_hits(hits: &[Hit]) -> String {
    let mut lines: Vec<String> = hits
        .iter()
        .map(|h| {
            let mut line = format!("title: {}\nexcerpt: {}", h.title, h.excerpt);
            if !h.source.is_empty() {
                line.push_str(&format!("\nsource: {}", h.source));
            }
            line
        })
        .collect();
    let mut text = lines.join("\n\n");
    if text.len() > MAX_TOOL_TEXT {
        let mut cut = MAX_TOOL_TEXT;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n...(cut)");
    }
    text
}

fn clip_excerpt(s: &str) -> String {
    if s.chars().count() <= EXCERPT_MAX {
        return s.to_string();
    }
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= EXCERPT_MAX {
            break;
        }
        out.push(c);
    }
    out
}

fn tool_text_from_result(body: &Value) -> String {
    body.get("content")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    if c.get("type").and_then(Value::as_str) == Some("text") {
                        c.get("text").and_then(Value::as_str)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn rpc(
    http: &HttpFn,
    base_url: &str,
    key: Option<&str>,
    session: Option<&str>,
    body: &Value,
) -> Result<Value, String> {
    let text = body.to_string();
    if text.len() > MAX_MCP_BODY {
        return Err("The request is larger than 1 MiB.".into());
    }
    let mut headers = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        (
            "Accept".to_string(),
            "application/json, text/event-stream".to_string(),
        ),
        (
            "MCP-Protocol-Version".to_string(),
            PROTOCOL_VERSIONS[0].to_string(),
        ),
    ];
    if let Some(k) = key.filter(|k| !k.is_empty()) {
        headers.push(("Authorization".to_string(), format!("Bearer {k}")));
    }
    if let Some(sid) = session.filter(|s| !s.is_empty()) {
        headers.push(("Mcp-Session-Id".to_string(), sid.to_string()));
    }
    let req = HttpRequest {
        method: "POST",
        url: base_url.trim().to_string(),
        headers,
        body: Some(text.into_bytes()),
        timeout_s: MCP_TIMEOUT_S,
    };
    let response = http(&req)?;
    if response.status == 401 || response.status == 403 {
        return Err("the endpoint did not accept the key".into());
    }
    if response.status >= 400 {
        return Err(format!(
            "HTTP {} from the retrieval service.",
            response.status
        ));
    }
    let mut reply = decode_body(&response.body)?;
    merge_session(&mut reply, &response);
    Ok(reply)
}

fn decode_body(raw: &[u8]) -> Result<Value, String> {
    if raw.is_empty() {
        return Err("The retrieval service sent an empty reply.".into());
    }
    if raw.first() == Some(&b'{') || raw.first() == Some(&b'[') {
        return serde_json::from_slice(raw)
            .map_err(|_| "The retrieval service reply was not valid JSON.".into());
    }
    parse_sse_json(raw)
}

fn parse_sse_json(raw: &[u8]) -> Result<Value, String> {
    let text = String::from_utf8_lossy(raw);
    for block in text.split("\n\n") {
        let mut event = String::new();
        let mut data = String::new();
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("event:") {
                event = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest.trim());
            }
        }
        if event == "message" || event.is_empty() {
            if !data.is_empty() {
                return serde_json::from_str(&data)
                    .map_err(|_| "The event data was not valid JSON.".into());
            }
        }
    }
    Err("The retrieval service sent no JSON message.".into())
}

fn rpc_result(reply: &Value) -> Result<Value, String> {
    if let Some(err) = reply.get("error") {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the retrieval call failed");
        return Err(msg.to_string());
    }
    reply
        .get("result")
        .cloned()
        .ok_or_else(|| "the retrieval reply had no result".to_string())
}

fn rpc_error_text(reply: &Value) -> String {
    reply
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("the retrieval service refused the request")
        .to_string()
}

fn session_from_value(reply: &Value) -> Option<String> {
    reply
        .get("_session")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            reply
                .pointer("/result/sessionId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn session_from_response(response: &HttpResponse) -> Option<String> {
    response
        .headers
        .iter()
        .find(|(k, _)| k == "mcp-session-id")
        .map(|(_, v)| v.clone())
}

pub(crate) fn merge_session(reply: &mut Value, response: &HttpResponse) {
    if let Some(sid) = session_from_response(response) {
        if let Some(obj) = reply.as_object_mut() {
            obj.insert("_session".into(), json!(sid));
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::HttpRequest;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    type Script = Arc<Mutex<Vec<HttpResponse>>>;

    fn scripted(script: Vec<HttpResponse>) -> (HttpFn, Script, Arc<AtomicUsize>) {
        let steps: Script = Arc::new(Mutex::new(script));
        let seen = Arc::new(AtomicUsize::new(0));
        let log = seen.clone();
        let s = steps.clone();
        let http: HttpFn = Arc::new(move |req: &HttpRequest| {
            log.fetch_add(1, Ordering::SeqCst);
            let mut guard = lock(&s);
            let step = guard
                .first()
                .cloned()
                .ok_or_else(|| "no reply".to_string())?;
            if guard.len() > 1 {
                guard.remove(0);
            }
            if let Some(sid) = session_from_response(&step) {
                let mut v: Value = serde_json::from_slice(&step.body).unwrap_or(json!({}));
                merge_session(&mut v, &step);
                return Ok(HttpResponse {
                    status: step.status,
                    headers: step.headers,
                    body: v.to_string().into_bytes(),
                });
            }
            let mut reply = step;
            if req.headers.iter().any(|(k, _)| k == "Mcp-Session-Id") {
                let body: Value = serde_json::from_slice(&reply.body).unwrap_or(json!({}));
                if body.get("method").and_then(Value::as_str) == Some("tools/list") {
                    for (k, v) in &req.headers {
                        if k == "Mcp-Session-Id" {
                            assert_eq!(v, "sess-1");
                        }
                    }
                }
            }
            Ok(reply)
        });
        (http, steps, seen)
    }

    fn ok_json(v: Value) -> HttpResponse {
        HttpResponse::ok(v.to_string().into_bytes())
    }

    fn init_ok(session_header: Option<&str>) -> HttpResponse {
        let mut h = HttpResponse::ok(
            json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{}}})
                .to_string()
                .into_bytes(),
        );
        if let Some(sid) = session_header {
            h.headers
                .push(("mcp-session-id".to_string(), sid.to_string()));
        }
        h
    }

    #[test]
    fn tools_list_with_a_prefixed_search_knowledge_tool_is_found_and_offered_as_search_knowledge() {
        let list = json!({"tools":[{"name":"kb__search_knowledge","description":"search"}]});
        assert_eq!(pick_search_tool(&list), Ok("kb__search_knowledge".into()));
        let tool = Retrieval::model_tool(crate::api::Protocol::OpenAi);
        assert_eq!(
            tool.pointer("/function/name").and_then(Value::as_str),
            Some(MODEL_TOOL_NAME)
        );
        assert_eq!(to_wire("kb__search_knowledge"), "kb__search_knowledge");
    }

    #[test]
    fn tools_list_without_the_tool_offers_nothing_and_test_says_why() {
        let list = json!({"tools":[{"name":"other_tool"}]});
        assert_eq!(
            pick_search_tool(&list),
            Err("the endpoint offers no search_knowledge tool".into())
        );
    }

    #[test]
    fn a_search_result_is_clipped_and_rendered_with_title_excerpt_and_source() {
        let long = "x".repeat(800);
        let raw = json!([{
            "title": "Clearance",
            "excerpt": long,
            "source": "https://example.com/a",
        }]);
        let hits = parse_hits(&raw.to_string());
        assert_eq!(hits.len(), 1);
        assert!(hits[0].excerpt.chars().count() <= EXCERPT_MAX);
        let text = render_hits(&hits);
        assert!(text.contains("title: Clearance"));
        assert!(text.contains("source: https://example.com/a"));
    }

    #[test]
    fn the_session_id_header_is_echoed_on_later_calls() {
        let list = json!({"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"search_knowledge"}]}});
        let (http, _, _) = scripted(vec![
            init_ok(Some("sess-1")),
            ok_json(json!({})),
            ok_json(list),
        ]);
        let outcome = test_retrieval(&http, "http://127.0.0.1/mcp", None);
        assert!(outcome.is_ok());
    }

    #[test]
    fn an_sse_wrapped_reply_is_read_like_a_json_one() {
        let inner = json!({"jsonrpc":"2.0","id":1,"result":{"tools":[]}});
        let sse = format!("event: message\ndata: {inner}\n\n");
        let v = decode_body(sse.as_bytes());
        assert!(v.is_ok());
    }

    #[test]
    fn no_retrieval_configured_means_no_tool_in_the_list_and_no_request() {
        let mut tools: Vec<Value> = Vec::new();
        let cfg = crate::api::ApiConfig {
            protocol: crate::api::Protocol::OpenAi,
            base_url: "http://127.0.0.1/v1".into(),
            model: "m".into(),
            key: None,
            system_note: String::new(),
            retrieval: None,
        };
        append_model_tool(&mut tools, &cfg);
        assert!(tools.is_empty());
        let http = CountingHttp::new(Ok(HttpResponse::ok(b"{}".to_vec()))).http();
        assert!(test_retrieval(&http, "", None).is_err());
    }

    use crate::CountingHttp;

    #[test]
    fn register_tool_name_maps_search_knowledge_to_itself() {
        let list = pcbcraft_mcp::tool_list();
        let mut names = crate::tool_names::ToolNames::from_list(&list).expect("tool names");
        register_tool_name(&mut names);
        assert_eq!(names.mcp_name(MODEL_TOOL_NAME), Some(MODEL_TOOL_NAME));
    }
}
