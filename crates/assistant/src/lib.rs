//! Assistant core: backend trait, events, transcript and turn model.
//!
//! This crate defines the contract between the UI and a chat backend. It has
//! no I/O of its own; the host injects HTTP and secrets through traits so the
//! same code runs on desktop and in the browser.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// One piece of a user prompt.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum PromptPart {
    Text(String),
    Image { data: Vec<u8>, mime: String },
}

/// Context sent with every prompt so the backend knows what design it is
/// working on and what the user has selected.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct StartContext {
    pub design_summary: String,
    pub selection_note: Option<String>,
}

/// How the backend should handle tool calls.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Mode {
    /// Ask the user before every tool call.
    Ask,
    /// Run read-only tools, ask before edits.
    Edits,
    /// Run all tools without asking.
    All,
}

/// An HTTP request the backend wants the host to make.
#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// The host's response to an HTTP request.
#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// The host's HTTP function, injected at startup.
pub type HttpFn = Box<dyn Fn(HttpRequest) -> Result<HttpResponse, String> + Send + Sync>;

/// The host's secrets store, injected at startup.
pub trait Secrets: Send + Sync {
    fn get(&self, service: &str, key: &str) -> Option<String>;
    fn set(&self, service: &str, key: &str, value: &str) -> Result<(), String>;
    fn delete(&self, service: &str, key: &str) -> Result<(), String>;
}

/// Events the backend emits to the UI.
#[derive(Clone, Debug)]
pub enum Event {
    /// A chunk of assistant text.
    TextChunk { turn_id: u64, text: String },
    /// A chunk of assistant thinking.
    ThoughtChunk { turn_id: u64, text: String },
    /// The backend started a tool call.
    ToolCallStarted {
        turn_id: u64,
        call_id: u64,
        tool: String,
        args: serde_json::Value,
    },
    /// The backend finished a tool call.
    ToolCallUpdated {
        turn_id: u64,
        call_id: u64,
        result: serde_json::Value,
    },
    /// The backend proposed a plan.
    Plan { turn_id: u64, steps: Vec<String> },
    /// The backend wants permission to run a tool.
    PermissionRequest {
        turn_id: u64,
        request_id: u64,
        tool: String,
        description: String,
    },
    /// The backend needs authentication.
    AuthNeeded { message: String },
    /// A turn ended.
    TurnEnded {
        turn_id: u64,
        ok: bool,
        error: Option<String>,
    },
}

/// The chat backend contract.
pub trait ChatBackend: Send + Sync {
    /// Poll for new events.
    fn poll(&self) -> Vec<Event>;
    /// Send a prompt.
    fn prompt(&self, parts: Vec<PromptPart>, context: StartContext);
    /// Cancel the current turn.
    fn cancel(&self);
    /// Start a new session.
    fn new_session(&self);
    /// Answer a permission request.
    fn answer_permission(&self, request_id: u64, allow: bool, always: bool);
    /// Provide an authentication token.
    fn authenticate(&self, token: String);
    /// Set the model.
    fn set_model(&self, model: String);
    /// Set the mode.
    fn set_mode(&self, mode: Mode);
    /// Get the backend's process ID, if any.
    fn pid(&self) -> Option<u32>;
    /// Shut down the backend.
    fn shutdown(&self);
}

/// One item in the transcript.
#[derive(Clone, Debug)]
pub struct Item {
    pub id: u64,
    pub kind: ItemKind,
    pub turn_id: u64,
    pub created: u64,
}

/// The kind of transcript item.
#[derive(Clone, Debug)]
pub enum ItemKind {
    UserMessage { parts: Vec<PromptPart> },
    AssistantText { text: String },
    AssistantThought { text: String },
    ToolCall {
        call_id: u64,
        tool: String,
        args: serde_json::Value,
        result: Option<serde_json::Value>,
    },
    Plan { steps: Vec<String> },
    PermissionRequest {
        request_id: u64,
        tool: String,
        description: String,
        verdict: Option<Verdict>,
    },
    TurnEnd { ok: bool, error: Option<String> },
}

/// The user's verdict on a permission request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    Allow,
    AlwaysAllow,
    Deny,
}

/// State for one turn.
#[derive(Clone, Debug)]
pub struct Turn {
    pub id: u64,
    pub started: u64,
    pub ended: Option<u64>,
    pub ok: Option<bool>,
    pub error: Option<String>,
    pub items: Vec<u64>,
}

/// A queued message waiting for the backend to be ready.
#[derive(Clone, Debug)]
pub struct Pending {
    pub parts: Vec<PromptPart>,
    pub context: StartContext,
}

/// The assistant's transcript and turn state.
pub struct AgentChat {
    pub items: Vec<Item>,
    pub turns: HashMap<u64, Turn>,
    pub pending: Vec<Pending>,
    pub backend: Option<Box<dyn ChatBackend>>,
    next_item_id: AtomicU64,
    next_turn_id: AtomicU64,
    next_request_id: AtomicU64,
}

impl AgentChat {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            turns: HashMap::new(),
            pending: Vec::new(),
            backend: None,
            next_item_id: AtomicU64::new(1),
            next_turn_id: AtomicU64::new(1),
            next_request_id: AtomicU64::new(1),
        }
    }

    fn next_item_id(&self) -> u64 {
        self.next_item_id.fetch_add(1, Ordering::Relaxed)
    }

    fn next_turn_id(&self) -> u64 {
        self.next_turn_id.fetch_add(1, Ordering::Relaxed)
    }

    fn next_request_id(&self) -> u64 {
        self.next_request_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Queue a message. If the backend is ready, send it immediately.
    pub fn send(&mut self, parts: Vec<PromptPart>, context: StartContext) {
        if let Some(backend) = &self.backend {
            let turn_id = self.next_turn_id();
            let item_id = self.next_item_id();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            self.items.push(Item {
                id: item_id,
                kind: ItemKind::UserMessage { parts: parts.clone() },
                turn_id,
                created: now,
            });

            self.turns.insert(
                turn_id,
                Turn {
                    id: turn_id,
                    started: now,
                    ended: None,
                    ok: None,
                    error: None,
                    items: vec![item_id],
                },
            );

            backend.prompt(parts, context);
        } else {
            self.pending.push(Pending { parts, context });
        }
    }

    /// Send all pending messages now that the backend is ready.
    pub fn send_pending(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        for p in pending {
            self.send(p.parts, p.context);
        }
    }

    /// Cancel the current turn.
    pub fn cancel(&self) {
        if let Some(backend) = &self.backend {
            backend.cancel();
        }
    }

    /// Answer a permission request.
    pub fn answer_permission(&self, request_id: u64, allow: bool, always: bool) {
        if let Some(backend) = &self.backend {
            backend.answer_permission(request_id, allow, always);
        }
    }

    /// Approve a permission request.
    pub fn approve(&self, request_id: u64) {
        self.answer_permission(request_id, true, false);
    }

    /// Always approve this kind of request.
    pub fn approve_always(&self, request_id: u64) {
        self.answer_permission(request_id, true, true);
    }

    /// Poll the backend for events and update the transcript.
    pub fn poll(&mut self) {
        if let Some(backend) = &self.backend {
            for event in backend.poll() {
                self.handle_event(event);
            }
        }
    }

    fn handle_event(&mut self, event: Event) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        match event {
            Event::TextChunk { turn_id, text } => {
                let item_id = self.next_item_id();
                self.items.push(Item {
                    id: item_id,
                    kind: ItemKind::AssistantText { text },
                    turn_id,
                    created: now,
                });
                if let Some(turn) = self.turns.get_mut(&turn_id) {
                    turn.items.push(item_id);
                }
            }
            Event::ThoughtChunk { turn_id, text } => {
                let item_id = self.next_item_id();
                self.items.push(Item {
                    id: item_id,
                    kind: ItemKind::AssistantThought { text },
                    turn_id,
                    created: now,
                });
                if let Some(turn) = self.turns.get_mut(&turn_id) {
                    turn.items.push(item_id);
                }
            }
            Event::ToolCallStarted {
                turn_id,
                call_id,
                tool,
                args,
            } => {
                let item_id = self.next_item_id();
                self.items.push(Item {
                    id: item_id,
                    kind: ItemKind::ToolCall {
                        call_id,
                        tool,
                        args,
                        result: None,
                    },
                    turn_id,
                    created: now,
                });
                if let Some(turn) = self.turns.get_mut(&turn_id) {
                    turn.items.push(item_id);
                }
            }
            Event::ToolCallUpdated {
                turn_id,
                call_id,
                result,
            } => {
                if let Some(item) = self.items.iter_mut().rev().find(|i| {
                    matches!(
                        &i.kind,
                        ItemKind::ToolCall { call_id: c, .. } if *c == call_id
                    )
                }) {
                    if let ItemKind::ToolCall { result: r, .. } = &mut item.kind {
                        *r = Some(result);
                    }
                }
            }
            Event::Plan { turn_id, steps } => {
                let item_id = self.next_item_id();
                self.items.push(Item {
                    id: item_id,
                    kind: ItemKind::Plan { steps },
                    turn_id,
                    created: now,
                });
                if let Some(turn) = self.turns.get_mut(&turn_id) {
                    turn.items.push(item_id);
                }
            }
            Event::PermissionRequest {
                turn_id,
                request_id,
                tool,
                description,
            } => {
                let item_id = self.next_item_id();
                self.items.push(Item {
                    id: item_id,
                    kind: ItemKind::PermissionRequest {
                        request_id,
                        tool,
                        description,
                        verdict: None,
                    },
                    turn_id,
                    created: now,
                });
                if let Some(turn) = self.turns.get_mut(&turn_id) {
                    turn.items.push(item_id);
                }
            }
            Event::AuthNeeded { message } => {
                // Auth needed is a global event, not tied to a turn.
                let item_id = self.next_item_id();
                self.items.push(Item {
                    id: item_id,
                    kind: ItemKind::AssistantText { text: message },
                    turn_id: 0,
                    created: now,
                });
            }
            Event::TurnEnded {
                turn_id,
                ok,
                error,
            } => {
                let item_id = self.next_item_id();
                self.items.push(Item {
                    id: item_id,
                    kind: ItemKind::TurnEnd { ok, error: error.clone() },
                    turn_id,
                    created: now,
                });
                if let Some(turn) = self.turns.get_mut(&turn_id) {
                    turn.items.push(item_id);
                    turn.ended = Some(now);
                    turn.ok = Some(ok);
                    turn.error = error;
                }
            }
        }
    }

    /// Connect to a backend.
    pub fn connect(&mut self, backend: Box<dyn ChatBackend>) {
        self.backend = Some(backend);
        self.send_pending();
    }

    /// Start a new chat session.
    pub fn new_chat(&mut self) {
        if let Some(backend) = &self.backend {
            backend.new_session();
        }
        self.items.clear();
        self.turns.clear();
        self.pending.clear();
    }

    /// Set the backend's model.
    pub fn set_model(&self, model: String) {
        if let Some(backend) = &self.backend {
            backend.set_model(model);
        }
    }

    /// Set the backend's mode.
    pub fn set_mode(&self, mode: Mode) {
        if let Some(backend) = &self.backend {
            backend.set_mode(mode);
        }
    }

    /// Authenticate with the backend.
    pub fn authenticate(&self, token: String) {
        if let Some(backend) = &self.backend {
            backend.authenticate(token);
        }
    }

    /// Get the backend's process ID.
    pub fn pid(&self) -> Option<u32> {
        self.backend.as_ref().and_then(|b| b.pid())
    }

    /// Shut down the backend.
    pub fn shutdown(&mut self) {
        if let Some(backend) = &self.backend {
            backend.shutdown();
        }
        self.backend = None;
    }

    /// Get the current status string.
    pub fn status(&self) -> String {
        if self.backend.is_none() {
            "disconnected".to_string()
        } else if !self.pending.is_empty() {
            "queued".to_string()
        } else {
            "ready".to_string()
        }
    }
}

impl Default for AgentChat {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestBackend {
        events: std::sync::Mutex<Vec<Event>>,
    }

    impl ChatBackend for TestBackend {
        fn poll(&self) -> Vec<Event> {
            let mut e = self.events.lock().unwrap_or_default();
            std::mem::take(&mut e)
        }
        fn prompt(&self, _parts: Vec<PromptPart>, _context: StartContext) {}
        fn cancel(&self) {}
        fn new_session(&self) {}
        fn answer_permission(&self, _request_id: u64, _allow: bool, _always: bool) {}
        fn authenticate(&self, _token: String) {}
        fn set_model(&self, _model: String) {}
        fn set_mode(&self, _mode: Mode) {}
        fn pid(&self) -> Option<u32> { None }
        fn shutdown(&self) {}
    }

    #[test]
    fn a_newcomer_can_create_an_assistant() {
        let chat = AgentChat::new();
        assert_eq!(chat.status(), "disconnected");
    }

    #[test]
    fn a_newcomer_can_queue_a_message() {
        let mut chat = AgentChat::new();
        chat.send(
            vec![PromptPart::Text("hello".to_string())],
            StartContext::default(),
        );
        assert_eq!(chat.pending.len(), 1);
    }

    #[test]
    fn a_newcomer_can_connect_and_send() {
        let mut chat = AgentChat::new();
        chat.send(
            vec![PromptPart::Text("hello".to_string())],
            StartContext::default(),
        );
        chat.connect(Box::new(TestBackend {
            events: std::sync::Mutex::new(Vec::new()),
        }));
        assert_eq!(chat.status(), "ready");
        assert_eq!(chat.pending.len(), 0);
        assert_eq!(chat.items.len(), 1);
    }

    #[test]
    fn a_newcomer_can_handle_events() {
        let mut chat = AgentChat::new();
        chat.connect(Box::new(TestBackend {
            events: std::sync::Mutex::new(Vec::new()),
        }));
        chat.handle_event(Event::TextChunk {
            turn_id: 1,
            text: "hello".to_string(),
        });
        assert_eq!(chat.items.len(), 1);
    }

    #[test]
    fn a_newcomer_can_start_a_new_chat() {
        let mut chat = AgentChat::new();
        chat.connect(Box::new(TestBackend {
            events: std::sync::Mutex::new(Vec::new()),
        }));
        chat.handle_event(Event::TextChunk {
            turn_id: 1,
            text: "hello".to_string(),
        });
        chat.new_chat();
        assert_eq!(chat.items.len(), 0);
    }
}
