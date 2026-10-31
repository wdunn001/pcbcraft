//! The ACP backend (desktop only): starts the agent the user picked as a child process and holds
//! the protocol's side of the conversation (initialize, the session, prompts, permission questions,
//! sign-in). The panel sees only [`Event`]s.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::process::{AgentProcess, FromAgent};
use super::protocol::{self, Incoming};
use crate::backend::{AuthMethod, ChatBackend, Event, PromptPart};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    Initialize,
    SessionNew,
    Prompt,
    Authenticate,
    SetMode,
    SetModel,
}

/// Variables the agent doesn't inherit: a control token set for the app's own `--control` port
/// would otherwise reach the agent and outrank the chat endpoint's own token file.
const ENV_REMOVE: &[&str] = &["PCBCRAFT_CONTROL_TOKEN"];

pub struct AcpBackend {
    proc: AgentProcess,
    cwd: std::path::PathBuf,
    bridge: Option<protocol::McpBridge>,
    next_id: u64,
    pending: HashMap<u64, Pending>,
    session: Option<String>,
    agent: String,
    images: bool,
    documents: bool,
    auth: Vec<AuthMethod>,
    /// Open permission questions: our key → the agent's request id.
    permissions: HashMap<u64, Value>,
    next_key: u64,
    exited: bool,
}

/// The CLI next to the desktop binary (`pcbcraft-cli` or `pcbcraft-cli.exe`).
pub fn resolve_cli(app_exe: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let dir = app_exe
        .parent()
        .ok_or_else(|| "pcbcraft-cli was not found beside the app".to_string())?;
    let name = if cfg!(windows) {
        "pcbcraft-cli.exe"
    } else {
        "pcbcraft-cli"
    };
    let cli = dir.join(name);
    if cli.is_file() {
        Ok(cli)
    } else {
        Err("pcbcraft-cli was not found beside the app".into())
    }
}

impl AcpBackend {
    /// Start `argv`, open the gated MCP bridge on `bridge_port`, and say hello (`initialize`).
    pub fn start(
        argv: &[String],
        work_dir: std::path::PathBuf,
        bridge_port: u16,
        bridge_token: String,
        app_exe: &std::path::Path,
    ) -> Result<AcpBackend, String> {
        let cli = resolve_cli(app_exe)?;
        let bridge = protocol::McpBridge {
            cli,
            port: bridge_port,
            token: bridge_token,
        };
        let proc = AgentProcess::spawn(argv, &work_dir, ENV_REMOVE, || {})?;
        let (cwd, bridge) = if protocol::runs_in_wsl(argv) {
            (protocol::wsl_path(&work_dir), Some(bridge.in_wsl()))
        } else {
            (work_dir, Some(bridge))
        };
        let mut b = AcpBackend {
            proc,
            cwd,
            bridge,
            next_id: 0,
            pending: HashMap::new(),
            session: None,
            agent: String::new(),
            images: false,
            documents: false,
            auth: Vec::new(),
            permissions: HashMap::new(),
            next_key: 0,
            exited: false,
        };
        b.request(
            Pending::Initialize,
            "initialize",
            protocol::initialize_params(env!("CARGO_PKG_VERSION")),
        );
        Ok(b)
    }

    fn request(&mut self, what: Pending, method: &str, params: Value) {
        self.next_id += 1;
        self.pending.insert(self.next_id, what);
        self.proc
            .send(&protocol::request(self.next_id, method, params));
    }

    fn new_session(&mut self) {
        let params = protocol::session_new_params(&self.cwd, self.bridge.as_ref());
        self.request(Pending::SessionNew, "session/new", params);
    }

    fn handle(&mut self, msg: Incoming, out: &mut Vec<Event>) {
        match msg {
            Incoming::Reply { id, result } => {
                let Some(what) = self.pending.remove(&id) else {
                    return;
                };
                match (what, result) {
                    (Pending::Initialize, Ok(v)) => {
                        self.agent = protocol::agent_info(&v);
                        self.images = protocol::takes_images(&v);
                        self.documents = protocol::takes_documents(&v);
                        self.auth = protocol::auth_methods(&v);
                        self.new_session();
                    }
                    (Pending::SessionNew, Ok(v)) => {
                        match v.get("sessionId").and_then(Value::as_str) {
                            Some(s) => {
                                self.session = Some(s.to_string());
                                if let Some((current, available)) = protocol::modes(&v) {
                                    out.push(Event::Modes { current, available });
                                }
                                if let Some((current, available)) = protocol::models(&v) {
                                    out.push(Event::Models { current, available });
                                }
                                out.push(Event::Documents(self.documents));
                                out.push(Event::Ready {
                                    agent: self.agent.clone(),
                                    images: self.images,
                                });
                            }
                            None => out.push(Event::Error("session/new: no sessionId".into())),
                        }
                    }
                    (Pending::Prompt, Ok(v)) => {
                        let reason = v.get("stopReason").and_then(Value::as_str).unwrap_or("");
                        out.push(Event::TurnEnded {
                            reason: if reason == "end_turn" {
                                String::new()
                            } else {
                                reason.to_string()
                            },
                        });
                    }
                    // Signed in: try the session again.
                    (Pending::Authenticate, Ok(_)) => self.new_session(),
                    (Pending::SetMode | Pending::SetModel, Ok(_)) => {}
                    (what, Err((code, message))) => {
                        let auth = code == protocol::AUTH_REQUIRED
                            || message.to_ascii_lowercase().contains("auth");
                        if auth {
                            self.session = None;
                            if what == Pending::Prompt {
                                out.push(Event::TurnEnded {
                                    reason: "auth".into(),
                                });
                            }
                            out.push(Event::AuthRequired {
                                methods: self.auth.clone(),
                            });
                        } else {
                            if what == Pending::Prompt {
                                out.push(Event::TurnEnded {
                                    reason: "error".into(),
                                });
                            }
                            out.push(Event::Error(message));
                        }
                    }
                }
            }
            Incoming::Text { thought, text } => out.push(Event::Text { thought, text }),
            Incoming::Tool {
                id,
                title,
                kind,
                status,
                output,
            } => out.push(Event::Tool {
                id,
                title,
                kind,
                status,
                output,
            }),
            Incoming::Plan(p) => out.push(Event::Plan(p)),
            Incoming::Permission {
                rpc,
                title,
                options,
            } => {
                self.next_key += 1;
                self.permissions.insert(self.next_key, rpc);
                out.push(Event::Permission {
                    key: self.next_key,
                    title,
                    options,
                });
            }
            Incoming::AuthStatus(s) => out.push(Event::AuthStatus(s)),
            Incoming::ModeChanged(m) => out.push(Event::Modes {
                current: m,
                available: Vec::new(),
            }),
            Incoming::ModelChanged(m) => out.push(Event::Models {
                current: m,
                available: Vec::new(),
            }),
            Incoming::Unsupported { rpc, method } => {
                self.proc.send(&protocol::reply_error(
                    rpc,
                    protocol::METHOD_NOT_FOUND,
                    &format!("PCBCraft does not serve `{method}`"),
                ));
            }
            Incoming::Ignored => {}
        }
    }
}

impl ChatBackend for AcpBackend {
    fn poll(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        if self.exited {
            return out;
        }
        for _ in 0..512 {
            match self.proc.from_agent.try_recv() {
                Ok(FromAgent::Line(l)) => {
                    if let Some(m) = protocol::parse(&l) {
                        self.handle(m, &mut out);
                    }
                }
                Ok(FromAgent::Exited(tail)) => {
                    self.exited = true;
                    self.session = None;
                    self.pending.clear();
                    out.push(Event::Exited { tail });
                    break;
                }
                Err(_) => break,
            }
        }
        out
    }

    fn prompt(&mut self, parts: &[PromptPart]) {
        if let Some(s) = self.session.clone() {
            self.request(
                Pending::Prompt,
                "session/prompt",
                protocol::prompt_params(&s, parts),
            );
        }
    }

    fn cancel(&mut self) {
        // Open permission questions are answered "cancelled" first, as the protocol asks.
        for (_, rpc) in self.permissions.drain() {
            self.proc.send(&protocol::permission_reply(rpc, None));
        }
        if let Some(s) = &self.session {
            self.proc.send(&protocol::notification(
                "session/cancel",
                json!({"sessionId": s}),
            ));
        }
    }

    fn new_session(&mut self) {
        if self.pending.values().any(|p| *p == Pending::Prompt) {
            self.cancel();
        }
        self.session = None;
        self.new_session();
    }

    fn answer_permission(&mut self, key: u64, option: Option<&str>) {
        if let Some(rpc) = self.permissions.remove(&key) {
            self.proc.send(&protocol::permission_reply(rpc, option));
        }
    }

    fn authenticate(&mut self, method: &str) {
        self.request(
            Pending::Authenticate,
            "authenticate",
            json!({"methodId": method}),
        );
    }

    fn set_mode(&mut self, mode: &str) {
        if let Some(s) = self.session.clone() {
            self.request(
                Pending::SetMode,
                "session/set_mode",
                json!({"sessionId": s, "modeId": mode}),
            );
        }
    }

    fn set_model(&mut self, model: &str) {
        if let Some(s) = self.session.clone() {
            self.request(
                Pending::SetModel,
                "session/set_model",
                protocol::set_model_params(&s, model),
            );
        }
    }

    fn pid(&self) -> Option<u32> {
        self.proc.pid()
    }

    fn shutdown(&mut self, wait: bool) {
        self.proc.shutdown(wait);
    }
}

/// Test connection: start the agent, say hello, open a session without tools, end it. What it
/// answered (its name and version), or what went wrong (it isn't signed in, it didn't answer).
pub fn test_agent(argv: &[String], cwd: &std::path::Path) -> Result<String, String> {
    let mut p = AgentProcess::spawn(argv, cwd, ENV_REMOVE, || {})?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let wait_for = |p: &AgentProcess, id: u64| -> Result<Value, String> {
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match p.from_agent.recv_timeout(left) {
                Ok(FromAgent::Line(l)) => {
                    if let Some(Incoming::Reply { id: got, result }) = protocol::parse(&l)
                        && got == id
                    {
                        return result.map_err(|(code, m)| {
                            if code == protocol::AUTH_REQUIRED
                                || m.to_ascii_lowercase().contains("auth")
                            {
                                "Sign in to the agent first".to_string()
                            } else {
                                m
                            }
                        });
                    }
                }
                Ok(FromAgent::Exited(tail)) => {
                    return Err(tail
                        .last()
                        .cloned()
                        .unwrap_or_else(|| "The agent ended".to_string()));
                }
                Err(_) => return Err("The agent did not answer".to_string()),
            }
        }
    };
    p.send(&protocol::request(
        1,
        "initialize",
        protocol::initialize_params(env!("CARGO_PKG_VERSION")),
    ));
    let init = wait_for(&p, 1);
    let session_cwd = if protocol::runs_in_wsl(argv) {
        protocol::wsl_path(cwd)
    } else {
        cwd.to_path_buf()
    };
    let result = init.and_then(|v| {
        p.send(&protocol::request(
            2,
            "session/new",
            protocol::session_new_params(&session_cwd, None),
        ));
        wait_for(&p, 2).map(|_| {
            let who = protocol::agent_info(&v);
            if who.is_empty() {
                "Connected".to_string()
            } else {
                format!("Connected to {who}")
            }
        })
    });
    p.shutdown(true);
    result
}
