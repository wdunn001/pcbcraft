// Adapted from SolidCraft (MIT OR Apache-2.0), commit 32558d0a:
// crates/ui-egui/src/agent_chat/backend.rs

use serde_json::Value;
use std::sync::Arc;

/// One part of a message to the agent.
#[derive(Clone, Debug, PartialEq)]
pub enum PromptPart {
    Text(String),
    /// A picture of the design (PNG), sent only to agents that take images.
    Picture {
        mime: String,
        base64: String,
    },
    /// A file the agent reads as a document (a PDF), sent only to agents that take documents.
    Document {
        name: String,
        /// Where it is on this machine (a `file://` link).
        uri: String,
        mime: String,
        base64: String,
    },
}

/// What a backend reports to the pane.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Whether the agent takes documents (PDFs) in a message; sent before [`Event::Ready`].
    Documents(bool),
    /// Ready for a message. `agent`: its name and version; `images`: it takes pictures.
    Ready { agent: String, images: bool },
    /// Text it streams (`thought`: its reasoning, shown dimmed).
    Text { thought: bool, text: String },
    /// A new tool call, or news about one.
    Tool {
        id: String,
        title: Option<String>,
        kind: Option<String>,
        status: Option<ToolStatus>,
        output: Option<String>,
    },
    /// A short progress note (such as `request 2 of 24`).
    Note(String),
    /// Its plan, replaced whole: each entry with its status.
    Plan(Vec<(String, String)>),
    /// It asks before running something of its own; answered with [`ChatBackend::answer_permission`].
    Permission {
        key: u64,
        title: String,
        options: Vec<PermissionOption>,
    },
    /// It needs the user to sign in; `methods` are its own ways to do so.
    AuthRequired { methods: Vec<AuthMethod> },
    /// What it says about its login.
    AuthStatus(String),
    /// Its own modes and the current one.
    Modes {
        current: String,
        available: Vec<(String, String)>,
    },
    /// Its models (empty `available`: only the current one changed).
    Models {
        current: String,
        available: Vec<ModelInfo>,
    },
    /// The message is answered (`reason`: why it stopped, empty for a normal end).
    TurnEnded { reason: String },
    /// Something failed; the backend is still running unless [`Event::Exited`] follows.
    Error(String),
    /// It ended; its last error output.
    Exited { tail: Vec<String> },
}

/// What a backend gets to start.
pub struct StartContext {
    /// Its working folder: an empty folder of the app's own, not the user's files.
    pub work_dir: std::path::PathBuf,
    /// Where in-process tool calls go, for backends that run their own tool loop.
    pub tools: Option<std::sync::mpsc::Sender<super::turn::ToolRequest>>,
}

/// A chat backend. Methods never block the frame; replies arrive through [`ChatBackend::poll`].
pub trait ChatBackend {
    /// What arrived since the last call.
    fn poll(&mut self) -> Vec<Event>;
    /// Send a message (the pane only sends one after [`Event::Ready`] and while no turn runs).
    fn prompt(&mut self, parts: &[PromptPart]);
    /// Stop the message being answered; open permission questions count as cancelled.
    fn cancel(&mut self);
    /// Start a fresh conversation on the running backend (no restart); [`Event::Ready`] follows.
    fn new_session(&mut self);
    /// Answer an [`Event::Permission`] with one of its options (`None`: cancelled).
    fn answer_permission(&mut self, key: u64, option: Option<&str>);
    /// Ask the backend to sign in by itself with one of its methods (it opens the browser).
    fn authenticate(&mut self, method: &str);
    /// Use one of the models it reported in [`Event::Models`].
    fn set_model(&mut self, model: &str);
    /// Switch to one of the modes it reported in [`Event::Modes`].
    fn set_mode(&mut self, mode: &str);
    /// Its process, for status and tests.
    fn pid(&self) -> Option<u32> {
        None
    }
    /// End it and everything it started. `wait`: block until done (quitting).
    fn shutdown(&mut self, wait: bool);
}

/// One HTTP request of the API backend (the desktop host sends it; nothing else in the chat
/// reaches the network).
#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout_s: u64,
}

/// Sends an [`HttpRequest`]: the status and the body, or why it failed.
pub type HttpFn = Arc<dyn Fn(&HttpRequest) -> Result<(u16, Vec<u8>), String> + Send + Sync>;

/// The system keychain (the desktop host's), for API keys: never in the preferences.
pub trait Secrets: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<String>, String>;
    fn set(&self, id: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}

/// One way to log in that the agent offers.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthMethod {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: AuthKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AuthKind {
    Agent,
    Terminal {
        args: Vec<String>,
        command: Option<(String, Vec<String>)>,
    },
    EnvVar {
        vars: Vec<String>,
    },
}

/// One model the agent offers.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// A tool call's state as the agent reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// One choice of a permission request.
#[derive(Clone, Debug, PartialEq)]
pub struct PermissionOption {
    pub id: String,
    pub name: String,
    /// `allow_once`, `allow_always`, `reject_once` or `reject_always`.
    pub kind: String,
}

/// Standard base64 with padding (RFC 4648), for pictures in messages.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                let idx = ((n >> shift) & 63) as usize;
                let ch = T.get(idx).copied().unwrap_or(b'A');
                out.push(char::from(ch));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A message's parts as ACP content blocks.
pub fn content_blocks(parts: &[PromptPart]) -> Vec<Value> {
    parts
        .iter()
        .map(|p| match p {
            PromptPart::Text(t) => serde_json::json!({"type": "text", "text": t}),
            PromptPart::Picture { mime, base64 } => {
                serde_json::json!({"type": "image", "mimeType": mime, "data": base64})
            }
            PromptPart::Document { uri, mime, base64, .. } => {
                serde_json::json!({"type": "resource", "resource": {"uri": uri, "mimeType": mime, "blob": base64}})
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_examples() {
        for (i, o) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(i.as_bytes()), o);
        }
    }

    #[test]
    fn pictures_go_as_image_blocks() {
        let b = content_blocks(&[
            PromptPart::Text("hi".into()),
            PromptPart::Picture {
                mime: "image/png".into(),
                base64: "AA==".into(),
            },
        ]);
        assert_eq!(b[1]["type"], "image");
        assert_eq!(b[1]["mimeType"], "image/png");
    }

    #[test]
    fn documents_go_as_embedded_resources() {
        let doc = PromptPart::Document {
            name: "brief.pdf".into(),
            uri: "file:///a/brief.pdf".into(),
            mime: "application/pdf".into(),
            base64: "AA==".into(),
        };
        let b = content_blocks(&[doc]);
        assert_eq!(b[0]["type"], "resource");
        assert_eq!(b[0]["resource"]["uri"], "file:///a/brief.pdf");
        assert_eq!(b[0]["resource"]["mimeType"], "application/pdf");
        assert_eq!(b[0]["resource"]["blob"], "AA==");
    }
}
