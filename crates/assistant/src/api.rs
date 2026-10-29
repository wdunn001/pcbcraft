// Adapted from SolidCraft (MIT OR Apache-2.0), commit 32558d0a:
// crates/ui-egui/src/agent_chat/api_client.rs
//! The API backend: a model behind an OpenAI-compatible chat completions endpoint or an
//! Anthropic-compatible Messages endpoint. It runs the tool loop itself. Each tool call goes out on
//! the [`StartContext::tools`] channel (the host gates it and runs it on the displayed session) and
//! the result goes back to the model. The host supplies the HTTP function; nothing here touches the
//! network by itself.

use crate::backend::{
    ChatBackend, Event, HttpFn, HttpRequest, ModelInfo, PromptPart, StartContext, ToolStatus,
};
use crate::prompt::SYSTEM_NOTE;
use crate::tool_names::{ToolNames, to_wire};
use crate::turn::ToolRequest;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

/// Model requests for one message, at most (each answer with tool calls is one).
pub const MAX_REQUESTS_PER_TURN: usize = 24;
/// Longest tool result handed back to the model.
pub const MAX_TOOL_TEXT: usize = 20_000;
/// Messages kept in the conversation (the system note stays).
pub const MAX_HISTORY: usize = 240;
const REQUEST_TIMEOUT_S: u64 = 300;
const TOOL_TIMEOUT: Duration = Duration::from_secs(330);
const CANCEL_POLL: Duration = Duration::from_millis(40);
const ANTHROPIC_MAX_TOKENS: u64 = 8192;
const ANTHROPIC_VERSION: &str = "2023-06-01";
const TEST_TIMEOUT_S: u64 = 60;
/// Tools the model is not offered. None today.
const HIDDEN_TOOLS: &[&str] = &[];

/// Which wire shape the endpoint speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    OpenAi,
    Anthropic,
}

/// Where the model is and how to reach it.
#[derive(Clone)]
pub struct ApiConfig {
    pub protocol: Protocol,
    pub base_url: String,
    pub model: String,
    pub key: Option<String>,
    /// Added after the standing system note (the design summary, later).
    pub system_note: String,
}

enum Cmd {
    Prompt(Vec<PromptPart>),
    NewChat,
    SetModel(String),
}

/// The running backend; the loop lives on a thread of its own.
pub struct ApiBackend {
    cmds: Option<Sender<Cmd>>,
    events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}

/// Whether a key may be sent to this address: HTTPS, or this computer.
pub fn key_safe_for(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    if lower.starts_with("https://") {
        return true;
    }
    let Some(rest) = lower.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or("");
    let name = if let Some(v6) = host.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    matches!(name, "localhost" | "127.0.0.1" | "::1")
}

impl ApiBackend {
    pub fn start(cfg: ApiConfig, ctx: StartContext, http: HttpFn) -> Result<ApiBackend, String> {
        if cfg.base_url.trim().is_empty() {
            return Err("Enter the address of the model first.".into());
        }
        if cfg.key.is_some() && !key_safe_for(&cfg.base_url) {
            return Err("A key is only sent over https, or to this computer.".into());
        }
        let (cmd_tx, cmd_rx) = channel::<Cmd>();
        let (ev_tx, events) = channel::<Event>();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let tool_list = pcbcraft_mcp::tool_list();
        let tool_names = ToolNames::from_list(&tool_list)?;
        let tools = match cfg.protocol {
            Protocol::OpenAi => openai_tools(&tool_list),
            Protocol::Anthropic => anthropic_tools(&tool_list),
        };
        let spawned = std::thread::Builder::new()
            .name("assistant-api".into())
            .spawn(move || {
                let mut w = Worker {
                    cfg,
                    http,
                    tool_sink: ctx.tools,
                    tools,
                    tool_names,
                    history: Vec::new(),
                    events: ev_tx,
                    cancel: stop,
                    requests_made: 0,
                };
                w.run(cmd_rx);
            });
        spawned.map_err(|e| format!("Could not start the assistant: {e}"))?;
        Ok(ApiBackend {
            cmds: Some(cmd_tx),
            events,
            cancel,
        })
    }
}

impl ChatBackend for ApiBackend {
    fn poll(&mut self) -> Vec<Event> {
        self.events.try_iter().take(512).collect()
    }
    fn prompt(&mut self, parts: &[PromptPart]) {
        self.cancel.store(false, Ordering::Relaxed);
        if let Some(c) = &self.cmds {
            let _ = c.send(Cmd::Prompt(parts.to_vec()));
        }
    }
    fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    fn set_model(&mut self, model: &str) {
        if let Some(c) = &self.cmds {
            let _ = c.send(Cmd::SetModel(model.to_string()));
        }
    }
    fn new_session(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(c) = &self.cmds {
            let _ = c.send(Cmd::NewChat);
        }
    }
    fn answer_permission(&mut self, _key: u64, _option: Option<&str>) {}
    fn authenticate(&mut self, _method: &str) {}
    fn set_mode(&mut self, _mode: &str) {}
    fn shutdown(&mut self, _wait: bool) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cmds = None;
    }
}

impl Drop for ApiBackend {
    fn drop(&mut self) {
        self.shutdown(false);
    }
}

struct Worker {
    cfg: ApiConfig,
    http: HttpFn,
    tool_sink: Option<Sender<ToolRequest>>,
    tools: Vec<Value>,
    tool_names: ToolNames,
    history: Vec<Value>,
    events: Sender<Event>,
    cancel: Arc<AtomicBool>,
    requests_made: usize,
}

impl Worker {
    fn emit(&self, e: Event) {
        let _ = self.events.send(e);
    }

    fn agent_name(&self) -> String {
        format!("API · {}", self.cfg.model)
    }

    fn run(&mut self, cmds: Receiver<Cmd>) {
        self.emit(Event::Ready {
            agent: self.agent_name(),
            images: true,
        });
        let listed = match self.cfg.protocol {
            Protocol::OpenAi => model_list(&self.http, &self.cfg.base_url, self.cfg.key.as_deref()),
            Protocol::Anthropic => {
                anthropic_model_list(&self.http, &self.cfg.base_url, self.cfg.key.as_deref())
            }
        };
        if let Ok(mut available) = listed {
            if !self.cfg.model.is_empty() && !available.iter().any(|m| m.id == self.cfg.model) {
                available.insert(
                    0,
                    ModelInfo {
                        id: self.cfg.model.clone(),
                        name: self.cfg.model.clone(),
                        description: String::new(),
                    },
                );
            }
            if !available.is_empty() {
                self.emit(Event::Models {
                    current: self.cfg.model.clone(),
                    available,
                });
            }
        }
        for cmd in cmds {
            match cmd {
                Cmd::Prompt(parts) => self.turn(parts),
                Cmd::SetModel(m) => self.cfg.model = m,
                Cmd::NewChat => {
                    self.history.clear();
                    self.cancel.store(false, Ordering::Relaxed);
                    self.emit(Event::Ready {
                        agent: self.agent_name(),
                        images: true,
                    });
                }
            }
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn turn(&mut self, parts: Vec<PromptPart>) {
        self.history.push(user_message(self.cfg.protocol, &parts));
        self.trim_history();
        for n in 1..=MAX_REQUESTS_PER_TURN {
            // The history here is complete: a cancelled request goes back to it.
            let sound = self.history.len();
            if self.cancelled() {
                self.emit(Event::TurnEnded {
                    reason: "cancelled".into(),
                });
                return;
            }
            self.emit(Event::Note(format!(
                "request {n} of {MAX_REQUESTS_PER_TURN}"
            )));
            let result = self.request();
            if self.cancelled() {
                self.history.truncate(sound);
                self.emit(Event::TurnEnded {
                    reason: "cancelled".into(),
                });
                return;
            }
            let reply = match result {
                Ok(r) => r,
                Err(e) => {
                    self.emit(Event::Error(e));
                    self.emit(Event::TurnEnded {
                        reason: "error".into(),
                    });
                    return;
                }
            };
            for (thought, text) in reply.text {
                self.emit(Event::Text { thought, text });
            }
            self.history.push(reply.assistant);
            if reply.calls.is_empty() {
                self.emit(Event::TurnEnded {
                    reason: String::new(),
                });
                return;
            }
            let mut results = Vec::new();
            for c in reply.calls.iter().take(32) {
                // The model uses the wire name; the host, the permission prompts and the UI use the MCP name.
                let mcp = self.tool_names.mcp_name(&c.name).map(str::to_string);
                let shown_name = mcp.as_deref().unwrap_or(&c.name);
                let args = c.arguments.clone().unwrap_or_else(|_| json!({}));
                self.requests_made += 1;
                let tool_id = format!("{}-{}", c.id, self.requests_made);
                self.emit(Event::Tool {
                    id: tool_id.clone(),
                    title: Some(tool_title(shown_name, &args)),
                    kind: Some(String::new()),
                    status: Some(ToolStatus::InProgress),
                    output: None,
                });
                let (text, failed) = if self.cancelled() {
                    ("cancelled by the user".to_string(), true)
                } else if let Err(e) = &c.arguments {
                    (format!("Not run: {e}"), true)
                } else if let Some(mcp) = &mcp {
                    match self.call_tool(mcp, args) {
                        Ok(r) => tool_text(&r),
                        Err(e) => (e, true),
                    }
                } else {
                    (format!("unknown tool `{}`", c.name), true)
                };
                let shown: String = text.chars().take(2000).collect();
                self.emit(Event::Tool {
                    id: tool_id,
                    title: None,
                    kind: None,
                    status: Some(if failed {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Completed
                    }),
                    output: Some(shown),
                });
                match self.cfg.protocol {
                    Protocol::OpenAi => self.history.push(json!({"role": "tool", "tool_call_id": c.id, "content": text})),
                    Protocol::Anthropic => results.push(json!({"type": "tool_result", "tool_use_id": c.id, "content": text, "is_error": failed})),
                }
            }
            if self.cfg.protocol == Protocol::Anthropic && !results.is_empty() {
                self.history
                    .push(json!({"role": "user", "content": results}));
            }
        }
        // The cap is reached: tell the model to stop and report, and show the user why.
        self.history.push(user_text(
            self.cfg.protocol,
            "Stop calling tools now. Report what you did and what is left.",
        ));
        self.emit(Event::Text {
            thought: false,
            text: "\n(Stopped after 24 requests for one message.)".into(),
        });
        self.emit(Event::TurnEnded {
            reason: "max_turn_requests".into(),
        });
    }

    /// Send one tool call to the host and wait for its answer, looking at Stop while it waits.
    fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        let Some(sink) = &self.tool_sink else {
            return Err("The assistant has no design tools in this session.".into());
        };
        let (reply, answer) = channel();
        sink.send(ToolRequest {
            name: name.to_string(),
            args,
            reply,
        })
        .map_err(|_| "The design tools stopped.".to_string())?;
        let deadline = std::time::Instant::now() + TOOL_TIMEOUT;
        loop {
            if self.cancelled() {
                return Err("cancelled by the user".into());
            }
            match answer.recv_timeout(CANCEL_POLL) {
                Ok(v) => return Ok(v),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                    if std::time::Instant::now() < deadline => {}
                Err(_) => return Err("The design tools did not answer.".into()),
            }
        }
    }

    /// Keep the conversation bounded and starting at a user message.
    fn trim_history(&mut self) {
        let extra = self.history.len().saturating_sub(MAX_HISTORY);
        self.history.drain(..extra);
        let lead = self
            .history
            .iter()
            .take_while(|m| !starts_a_turn(m))
            .count();
        self.history.drain(..lead);
    }

    fn request(&self) -> Result<Reply, String> {
        let body = self.body();
        let (status, v) = self.post(&body)?;
        if status != 200 {
            return Err(http_error(status, &v));
        }
        Ok(reply(self.cfg.protocol, &v))
    }

    fn system_text(&self) -> String {
        if self.cfg.system_note.trim().is_empty() {
            SYSTEM_NOTE.to_string()
        } else {
            format!("{SYSTEM_NOTE}\n\n{}", self.cfg.system_note)
        }
    }

    fn body(&self) -> Value {
        match self.cfg.protocol {
            Protocol::OpenAi => {
                let mut messages = Vec::with_capacity(self.history.len() + 1);
                messages.push(json!({"role": "system", "content": self.system_text()}));
                messages.extend(self.history.iter().cloned());
                let mut body = json!({"model": self.cfg.model, "messages": messages});
                if !self.tools.is_empty() {
                    body["tools"] = Value::Array(self.tools.clone());
                }
                body
            }
            Protocol::Anthropic => {
                let mut body = json!({
                    "model": self.cfg.model,
                    "max_tokens": ANTHROPIC_MAX_TOKENS,
                    "system": self.system_text(),
                    "messages": &self.history,
                });
                if !self.tools.is_empty() {
                    body["tools"] = Value::Array(self.tools.clone());
                }
                body
            }
        }
    }

    fn post(&self, body: &Value) -> Result<(u16, Value), String> {
        let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        let url = match self.cfg.protocol {
            Protocol::OpenAi => format!(
                "{}/chat/completions",
                self.cfg.base_url.trim_end_matches('/')
            ),
            Protocol::Anthropic => {
                headers.push((
                    "anthropic-version".to_string(),
                    ANTHROPIC_VERSION.to_string(),
                ));
                endpoint(&self.cfg.base_url, "messages")
            }
        };
        if let Some(k) = &self.cfg.key {
            match self.cfg.protocol {
                Protocol::OpenAi => headers.push(("Authorization".into(), format!("Bearer {k}"))),
                Protocol::Anthropic => headers.push(("x-api-key".into(), k.clone())),
            }
        }
        let req = HttpRequest {
            method: "POST",
            url,
            headers,
            body: Some(body.to_string().into_bytes()),
            timeout_s: REQUEST_TIMEOUT_S,
        };
        // The request runs on its own thread so Stop does not wait for the model.
        let (tx, rx) = channel();
        let http = self.http.clone();
        std::thread::spawn(move || {
            let _ = tx.send(http(&req));
        });
        // streaming: follow-up
        let (status, bytes) = loop {
            if self.cancelled() {
                return Err("cancelled by the user".into());
            }
            match rx.recv_timeout(CANCEL_POLL) {
                Ok(r) => break r.map_err(|e| redact(&e, self.cfg.key.as_deref()))?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err("The request ended without an answer.".into()),
            }
        };
        Ok((
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        ))
    }
}

/// An error text with the key taken out, if it ever got in.
fn redact(text: &str, key: Option<&str>) -> String {
    match key.filter(|k| !k.is_empty()) {
        Some(k) => text.replace(k, "(key)"),
        None => text.to_string(),
    }
}

/// An endpoint's refusal as a plain sentence: the status and the body's `error.message`. The key is
/// never part of it.
pub fn http_error(status: u16, body: &Value) -> String {
    let detail = body
        .pointer("/error/message")
        .or_else(|| body.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(300)
        .collect::<String>();
    let what = match status {
        401 | 403 => "the endpoint did not accept the key",
        404 => "the endpoint has nothing at that address",
        429 => "the endpoint is busy, try again soon",
        _ => "",
    };
    format!(
        "HTTP {status}{}{}",
        if what.is_empty() {
            String::new()
        } else {
            format!(": {what}")
        },
        if detail.is_empty() {
            String::new()
        } else {
            format!(" ({detail})")
        }
    )
}

fn tool_arguments(raw: Option<&Value>) -> Result<Value, String> {
    let text = match raw {
        None | Some(Value::Null) => return Ok(json!({})),
        Some(v @ Value::Object(_)) => return Ok(v.clone()),
        Some(Value::String(s)) => s.trim(),
        Some(_) => return Err("the tool arguments must be a JSON object".into()),
    };
    if text.is_empty() {
        return Ok(json!({}));
    }
    match serde_json::from_str::<Value>(text) {
        Ok(v @ Value::Object(_)) => Ok(v),
        Ok(_) => Err("the tool arguments must be a JSON object".into()),
        Err(e) => Err(format!(
            "the tool arguments were not valid JSON ({e}); send the call again with a JSON object"
        )),
    }
}

fn user_text(protocol: Protocol, text: &str) -> Value {
    user_message(protocol, &[PromptPart::Text(text.to_string())])
}

fn user_message(protocol: Protocol, parts: &[PromptPart]) -> Value {
    let pictures = parts
        .iter()
        .any(|p| matches!(p, PromptPart::Picture { .. }));
    if !pictures {
        let text: Vec<&str> = parts
            .iter()
            .filter_map(|p| {
                if let PromptPart::Text(t) = p {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
        return json!({"role": "user", "content": text.join("\n")});
    }
    let content: Vec<Value> = parts
        .iter()
        .filter_map(|p| match (protocol, p) {
            (_, PromptPart::Text(t)) => Some(json!({"type": "text", "text": t})),
            (Protocol::OpenAi, PromptPart::Picture { mime, base64 }) => {
                Some(json!({"type": "image_url", "image_url": {"url": format!("data:{mime};base64,{base64}")}}))
            }
            (Protocol::Anthropic, PromptPart::Picture { mime, base64 }) => {
                Some(json!({"type": "image", "source": {"type": "base64", "media_type": mime, "data": base64}}))
            }
            (_, PromptPart::Document { .. }) => None,
        })
        .collect();
    json!({"role": "user", "content": content})
}

fn starts_a_turn(message: &Value) -> bool {
    message["role"] == "user"
        && !message["content"]
            .as_array()
            .is_some_and(|blocks| blocks.iter().any(|b| b["type"] == "tool_result"))
}

#[derive(Debug)]
struct Call {
    id: String,
    name: String,
    arguments: Result<Value, String>,
}

#[derive(Debug)]
struct Reply {
    text: Vec<(bool, String)>,
    calls: Vec<Call>,
    assistant: Value,
}

fn reply(protocol: Protocol, body: &Value) -> Reply {
    match protocol {
        Protocol::OpenAi => openai_reply(body),
        Protocol::Anthropic => anthropic_reply(body),
    }
}

fn openai_reply(body: &Value) -> Reply {
    let message = body
        .pointer("/choices/0/message")
        .cloned()
        .unwrap_or(Value::Null);
    let text = message
        .get("content")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .map(|t| vec![(false, t.to_string())])
        .unwrap_or_default();
    let raw: Vec<Value> = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|a| a.iter().take(32).cloned().collect())
        .unwrap_or_default();
    let calls = raw
        .iter()
        .enumerate()
        .map(|(i, c)| Call {
            id: c
                .get("id")
                .and_then(Value::as_str)
                .map_or_else(|| format!("call-{i}"), str::to_string),
            name: c
                .pointer("/function/name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            arguments: tool_arguments(c.pointer("/function/arguments")),
        })
        .collect();
    let mut assistant = json!({"role": "assistant", "content": message.get("content").cloned().unwrap_or(Value::Null)});
    if !raw.is_empty() {
        assistant["tool_calls"] = Value::Array(raw);
    }
    Reply {
        text,
        calls,
        assistant,
    }
}

fn anthropic_reply(body: &Value) -> Reply {
    let content = body.get("content").cloned().unwrap_or(Value::Null);
    let mut text = Vec::new();
    let mut calls = Vec::new();
    let assistant = match &content {
        Value::Array(blocks) => {
            let mut kept = Vec::new();
            for (i, block) in blocks.iter().take(128).enumerate() {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(t) = block
                            .get("text")
                            .and_then(Value::as_str)
                            .filter(|t| !t.trim().is_empty())
                        {
                            text.push((false, t.to_string()));
                        }
                        kept.push(block.clone());
                    }
                    Some("thinking") => {
                        let thought = block.get("thinking").or_else(|| block.get("text"));
                        if let Some(t) = thought
                            .and_then(Value::as_str)
                            .filter(|t| !t.trim().is_empty())
                        {
                            text.push((true, t.to_string()));
                        }
                        kept.push(block.clone());
                    }
                    Some("tool_use") if calls.len() < 32 => {
                        calls.push(Call {
                            id: block
                                .get("id")
                                .and_then(Value::as_str)
                                .map_or_else(|| format!("toolu-{i}"), str::to_string),
                            name: block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            arguments: match block.get("input") {
                                None | Some(Value::Null) => Ok(json!({})),
                                Some(v @ Value::Object(_)) => Ok(v.clone()),
                                Some(_) => Err("the tool arguments must be a JSON object".into()),
                            },
                        });
                        kept.push(block.clone());
                    }
                    Some("tool_use") => {}
                    _ => kept.push(block.clone()),
                }
            }
            json!({"role": "assistant", "content": kept})
        }
        Value::String(s) => {
            if !s.trim().is_empty() {
                text.push((false, s.clone()));
            }
            json!({"role": "assistant", "content": content.clone()})
        }
        _ => json!({"role": "assistant", "content": content.clone()}),
    };
    Reply {
        text,
        calls,
        assistant,
    }
}

pub(crate) fn tool_entries(list: &Value) -> impl Iterator<Item = (&str, &Value)> {
    list.get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|a| a.iter().take(128))
        .filter_map(|t| {
            let name = t.get("name")?.as_str()?;
            (!HIDDEN_TOOLS.contains(&name)).then_some((name, t))
        })
}

fn schema_of(t: &Value) -> Value {
    t.get("inputSchema")
        .cloned()
        .filter(Value::is_object)
        .unwrap_or(json!({"type": "object", "properties": {}}))
}

/// The tool list (MCP `tools/list` shape) as OpenAI function tools. Names go out through
/// [`to_wire`] (see `crates/assistant/src/tool_names.rs`).
pub fn openai_tools(list: &Value) -> Vec<Value> {
    tool_entries(list)
        .map(|(name, t)| {
            json!({"type": "function", "function": {
                "name": to_wire(name),
                "description": t.get("description").and_then(Value::as_str).unwrap_or(""),
                "parameters": schema_of(t),
            }})
        })
        .collect()
}

/// The tool list as Anthropic tools, names through [`to_wire`].
pub fn anthropic_tools(list: &Value) -> Vec<Value> {
    tool_entries(list)
        .map(|(name, t)| {
            json!({
                "name": to_wire(name),
                "description": t.get("description").and_then(Value::as_str).unwrap_or(""),
                "input_schema": schema_of(t),
            })
        })
        .collect()
}

/// `{base}/v1/{path}`, whether the base ends in `/v1` or not.
pub fn endpoint(base_url: &str, path: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.to_ascii_lowercase().ends_with("/v1") {
        format!("{base}/{path}")
    } else {
        format!("{base}/v1/{path}")
    }
}

fn anthropic_headers(base_url: &str, key: Option<&str>) -> Result<Vec<(String, String)>, String> {
    let mut headers = vec![(
        "anthropic-version".to_string(),
        ANTHROPIC_VERSION.to_string(),
    )];
    if let Some(k) = key {
        if !key_safe_for(base_url) {
            return Err("A key is only sent over https, or to this computer.".into());
        }
        headers.push(("x-api-key".to_string(), k.to_string()));
    }
    Ok(headers)
}

fn parse_models(v: &Value, name_field: Option<&str>) -> Vec<ModelInfo> {
    v.get("data")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .take(500)
                .filter_map(|m| {
                    let id = m.get("id").and_then(Value::as_str)?.to_string();
                    let name = name_field
                        .and_then(|f| m.get(f))
                        .and_then(Value::as_str)
                        .map_or_else(|| id.clone(), str::to_string);
                    Some(ModelInfo {
                        id,
                        name,
                        description: String::new(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn get_json(
    http: &HttpFn,
    url: String,
    headers: Vec<(String, String)>,
    key: Option<&str>,
) -> Result<Value, String> {
    let req = HttpRequest {
        method: "GET",
        url,
        headers,
        body: None,
        timeout_s: 15,
    };
    let (status, bytes) = http(&req).map_err(|e| redact(&e, key))?;
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if status != 200 {
        return Err(http_error(status, &v));
    }
    Ok(v)
}

fn bearer_headers(base_url: &str, key: Option<&str>) -> Result<Vec<(String, String)>, String> {
    let mut headers = Vec::new();
    if let Some(k) = key {
        if !key_safe_for(base_url) {
            return Err("A key is only sent over https, or to this computer.".into());
        }
        headers.push(("Authorization".to_string(), format!("Bearer {k}")));
    }
    Ok(headers)
}

/// `GET {base}/models` of an OpenAI-compatible endpoint.
pub fn model_list(
    http: &HttpFn,
    base_url: &str,
    key: Option<&str>,
) -> Result<Vec<ModelInfo>, String> {
    let v = get_json(
        http,
        format!("{}/models", base_url.trim_end_matches('/')),
        bearer_headers(base_url, key)?,
        key,
    )?;
    Ok(parse_models(&v, None))
}

/// Ask an OpenAI-compatible endpoint for its models: how many, and whether `model` is one.
pub fn test_endpoint(
    http: &HttpFn,
    base_url: &str,
    key: Option<&str>,
    model: &str,
) -> Result<String, String> {
    let models = model_list(http, base_url, key)?;
    let found = model.is_empty() || models.iter().any(|m| m.id == model);
    Ok(if found {
        format!("Connected. The endpoint lists {} models.", models.len())
    } else {
        format!("Connected, but the endpoint does not list the model {model}.")
    })
}

/// The models of an Anthropic-compatible endpoint (some have no such list).
pub fn anthropic_model_list(
    http: &HttpFn,
    base_url: &str,
    key: Option<&str>,
) -> Result<Vec<ModelInfo>, String> {
    let v = get_json(
        http,
        endpoint(base_url, "models"),
        anthropic_headers(base_url, key)?,
        key,
    )?;
    Ok(parse_models(&v, Some("display_name")))
}

/// Test an Anthropic-compatible endpoint with the smallest message there is.
pub fn test_anthropic_endpoint(
    http: &HttpFn,
    base_url: &str,
    key: Option<&str>,
    model: &str,
) -> Result<String, String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("Choose a model first.".into());
    }
    let mut headers = anthropic_headers(base_url, key)?;
    headers.push(("Content-Type".to_string(), "application/json".to_string()));
    let body = json!({"model": model, "max_tokens": 16, "messages": [{"role": "user", "content": "Hello"}]});
    let req = HttpRequest {
        method: "POST",
        url: endpoint(base_url, "messages"),
        headers,
        body: Some(body.to_string().into_bytes()),
        timeout_s: TEST_TIMEOUT_S,
    };
    let (status, bytes) = http(&req).map_err(|e| redact(&e, key))?;
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if status != 200 {
        return Err(http_error(status, &v));
    }
    let said: String = v
        .get("content")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    Ok(if said.trim().is_empty() {
        format!("The model {model} answered.")
    } else {
        said
    })
}

/// A tool call's result as text for the model, and whether it failed. Long text is clipped.
pub fn tool_text(r: &Value) -> (String, bool) {
    let failed = r.get("isError").and_then(Value::as_bool).unwrap_or(false);
    let parts: Vec<String> = r
        .get("content")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .take(16)
                .map(|c| match c.get("type").and_then(Value::as_str) {
                    Some("text") => c
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    Some("image") => "[a picture of the view]".to_string(),
                    _ => String::new(),
                })
                .collect()
        })
        .unwrap_or_default();
    let mut text = parts.join("\n");
    if text.len() > MAX_TOOL_TEXT {
        let mut cut = MAX_TOOL_TEXT;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n...(cut)");
    }
    (text, failed)
}

fn tool_title(name: &str, args: &Value) -> String {
    match name {
        "execute" => format!(
            "execute: {}",
            args.get("command").and_then(Value::as_str).unwrap_or("?")
        ),
        "batch" => {
            let ids: Vec<&str> = args
                .get("commands")
                .and_then(Value::as_array)
                .map(|c| {
                    c.iter()
                        .filter_map(|s| s.get("command").and_then(Value::as_str))
                        .take(4)
                        .collect()
                })
                .unwrap_or_default();
            format!("batch: {}", ids.join(", "))
        }
        _ => name.to_string(),
    }
}
