//! The assistant pane's state and its loop with a chat backend.
//!
//! Context: the right aside's Assistant tab talks to a [`pcbcraft_assistant::ChatBackend`].
//! The backend asks for design tools over a channel; this module answers them on the displayed
//! session through `pcbcraft_mcp::call_tool`, the same typed command surface the MCP server uses.
//! Rules: nothing here opens a network connection (the host passes an `HttpFn`); keys never enter
//! the design, the transcript or any log; reads run at once, everything else waits for an answer.
//! Keep the pane body in `pane.rs` a self-contained function so the dock work can move it.

mod capture;
mod cards;
pub(crate) mod pane;
pub(crate) mod settings;
mod steps;
mod text;

use super::{App, Tab};
use pcbcraft_assistant::{
    ApiBackend, ChatBackend, Event, HttpFn, Item, PromptPart, Secrets, StartContext, StepState,
    ToolRequest, Transcript, Turn, Verdict,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

/// The tab label with the count of waiting questions.
pub(crate) fn tab_label(waiting: usize) -> String {
    text::tab_label(waiting)
}

/// Tooltip of the Assistant tab.
pub(crate) fn tab_tip() -> String {
    text::tab_tip()
}

/// What the desktop or web shell offers the assistant. `None` means the shell has no such thing.
#[derive(Clone, Default)]
pub struct Host {
    /// Sends one HTTP request. Absent in the browser build.
    pub http: Option<HttpFn>,
    /// The system keychain for model keys. Absent where the shell has none.
    pub secrets: Option<Arc<dyn Secrets>>,
}

/// A tool call held back until the user answers.
struct Waiting {
    id: u64,
    key: String,
    request: ToolRequest,
    /// Set while a batch is applied one command at a time.
    each: Option<steps::Stepwise>,
}

/// Everything the pane keeps between frames.
pub(crate) struct State {
    pub(crate) transcript: Transcript,
    pub(crate) input: String,
    /// Send a picture of the view with the next message.
    pub(crate) attach: bool,
    /// What the last message told the model, to send only changes next time.
    sent: Option<pcbcraft_assistant::Sent>,
    /// The text of the last context note (shown under "Show what was sent").
    pub(crate) last_note: String,
    /// Items in the saved file, to report only a change.
    saved_items: usize,
    /// Rows and height at the last frame, to scroll to the newest row when they change.
    pub(crate) seen: (usize, i32),
    /// Waiting for the answer to "Forget this conversation?".
    pub(crate) confirm_forget: bool,
    /// Open the Assistant part of Settings on the next frame.
    pub(crate) settings_open: bool,
    pub(crate) host: Host,
    backend: Option<Box<dyn ChatBackend>>,
    ready: bool,
    busy: bool,
    /// The last message was stopped by the user.
    stopped: bool,
    /// The last message ended because the endpoint failed.
    pub(crate) failed: bool,
    /// The last message sent, for Retry.
    last_prompt: String,
    turn: Option<Turn>,
    tools: (Sender<ToolRequest>, Receiver<ToolRequest>),
    waiting: Vec<Waiting>,
    next_id: u64,
    /// Id of the connection the running backend was started from.
    active: Option<String>,
    /// Set once a start was tried for the current settings, so a failure is not retried each frame.
    tried: bool,
    pub(crate) dialog: Option<settings::Dialog>,
    pub(crate) row_test: Option<(String, settings::SharedTest)>,
    /// History length right after the last message's changes were merged into one undo step.
    undo_len: Option<usize>,
    /// First step ids of the runs the user opened (a finished run is one line until opened).
    pub(crate) open_runs: std::collections::HashSet<u64>,
    /// Show the name of every changed object on the review card.
    pub(crate) list_all: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            transcript: Transcript::default(),
            input: String::new(),
            attach: false,
            sent: None,
            last_note: String::new(),
            saved_items: 0,
            seen: (0, 0),
            confirm_forget: false,
            settings_open: false,
            host: Host::default(),
            backend: None,
            ready: false,
            busy: false,
            stopped: false,
            failed: false,
            last_prompt: String::new(),
            turn: None,
            tools: channel(),
            waiting: Vec::new(),
            next_id: 1,
            active: None,
            tried: false,
            dialog: None,
            row_test: None,
            undo_len: None,
            open_runs: std::collections::HashSet::new(),
            list_all: false,
        }
    }
}

/// The word shown beside the state dot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Off,
    Starting,
    Ready,
    Working,
    Waiting,
    Stopped,
}

impl State {
    /// A start context for a backend that runs its own tool loop; tool calls come back to `poll`.
    pub(crate) fn start_context(&self, work_dir: std::path::PathBuf) -> StartContext {
        StartContext {
            work_dir,
            tools: Some(self.tools.0.clone()),
        }
    }

    pub(crate) fn phase(&self) -> Phase {
        if self.backend.is_none() {
            Phase::Off
        } else if !self.waiting.is_empty() {
            Phase::Waiting
        } else if self.ready && self.stopped {
            Phase::Stopped
        } else if self.busy {
            Phase::Working
        } else if self.ready {
            Phase::Ready
        } else {
            Phase::Starting
        }
    }

    /// Index of the Undo step that may still be pressed: the newest one, while nothing newer happened.
    pub(crate) fn live_undo(&self, history_len: usize) -> Option<usize> {
        let at = self
            .transcript
            .items
            .iter()
            .rposition(|i| matches!(i, Item::UndoStep { .. }))?;
        let fresh = matches!(
            self.transcript.items.get(at),
            Some(Item::UndoStep { undone: false, .. })
        ) && self.undo_len == Some(history_len);
        fresh.then_some(at)
    }

    /// Questions waiting for an answer (the count on the collapsed tab).
    pub(crate) fn waiting(&self) -> usize {
        self.waiting.len()
    }
}

/// A tool result that tells the model why nothing ran.
fn error_result(reason: &str) -> Value {
    json!({"isError": true, "content": [{"type": "text", "text": reason}]})
}

/// True when the assistant is switched on in Settings.
pub(crate) fn shown(app: &App) -> bool {
    app.preferences.assistant_enabled
}

impl App {
    /// Once per frame: connect when a default connection is ready, apply what the backend
    /// reported, and draw the Add connection window.
    pub(crate) fn assistant_frame(&mut self, ctx: &egui::Context) {
        if shown(self) {
            self.assistant_autoconnect();
            self.assistant_poll();
            settings::dialog(self, ctx);
        }
    }

    /// Start the default connection's backend, once per change of settings. Nothing starts
    /// without a saved connection and a way to send requests.
    fn assistant_autoconnect(&mut self) {
        if self.assistant.backend.is_some() || self.assistant.tried {
            return;
        }
        let Some(http) = self.assistant.host.http.clone() else {
            return;
        };
        let Some(connection) = self
            .preferences
            .assistant_connections
            .iter()
            .find(|c| c.id == self.preferences.assistant_default)
            .cloned()
        else {
            return;
        };
        self.assistant.tried = true;
        let key = if connection.has_key {
            self.assistant.host.secrets.as_ref().and_then(|s| {
                s.get(&pcbcraft_assistant::key_id(&connection.id))
                    .ok()
                    .flatten()
            })
        } else {
            None
        };
        let config = pcbcraft_assistant::config_for(&connection, key, String::new());
        let context = self.assistant.start_context(std::path::PathBuf::new());
        match ApiBackend::start(config, context, http) {
            Ok(backend) => {
                self.assistant_use(Box::new(backend));
                self.assistant.active = Some(connection.id);
                self.assistant.ready = true;
            }
            Err(reason) => self.assistant.transcript.push(Item::Note {
                text: reason,
                error: true,
            }),
        }
    }

    /// Drop the running backend so the next frame starts the default connection again.
    pub(crate) fn assistant_reconnect(&mut self) {
        self.assistant_stop();
        if let Some(mut b) = self.assistant.backend.take() {
            b.shutdown(false);
        }
        self.assistant.active = None;
        self.assistant.ready = false;
        self.assistant.busy = false;
        self.assistant.tried = false;
    }

    /// Give the assistant what the shell offers (an HTTP sender, a keychain).
    pub fn with_host(mut self, host: Host) -> Self {
        self.assistant.host = host;
        self
    }

    /// Use `backend` for the conversation. It reports `Ready` when it can take a message.
    pub(crate) fn assistant_use(&mut self, backend: Box<dyn ChatBackend>) {
        self.assistant.backend = Some(backend);
        self.assistant.ready = false;
        self.assistant.busy = false;
    }

    /// Show or hide the Assistant tab (Ctrl+Shift+A).
    pub(crate) fn assistant_toggle(&mut self) {
        if !shown(self) {
            return;
        }
        self.preferences.aside_lower_tab = u8::from(self.preferences.aside_lower_tab != 1);
    }

    /// Send the typed message, or queue it while the model is busy. Empty text is ignored.
    pub(crate) fn assistant_send(&mut self) {
        let text = self.assistant.input.trim().to_string();
        if text.is_empty() || self.assistant.backend.is_none() {
            return;
        }
        if let Err(reason) = pcbcraft_assistant::check_prompt_length(text.chars().count()) {
            self.assistant.transcript.push(Item::Note {
                text: reason,
                error: true,
            });
            return;
        }
        self.assistant.input.clear();
        if self.assistant.busy || !self.assistant.ready {
            self.assistant.transcript.push(Item::Queued { text });
            return;
        }
        self.assistant_prompt(text);
    }

    fn assistant_prompt(&mut self, text: String) {
        self.assistant.stopped = false;
        self.assistant.failed = false;
        self.assistant.last_prompt = text.clone();
        let (revision, history) = self
            .session
            .lock()
            .map_or((0, 0), |s| (s.revision, s.history_len()));
        self.assistant.turn = Some(Turn::new(revision, history, text.clone()));
        self.assistant
            .transcript
            .push(Item::User { text: text.clone() });
        self.assistant.busy = true;
        let parts = self.assistant_parts(&text);
        if let Some(b) = self.assistant.backend.as_mut() {
            b.prompt(&parts);
        }
    }

    /// The parts of a message: the context note (whole the first time, then only changes) with
    /// the user's words, and a picture of the view when Attach view is on.
    fn assistant_parts(&mut self, text: &str) -> Vec<PromptPart> {
        let context = self.assistant_context();
        let note = pcbcraft_assistant::note(&context, self.assistant.sent.as_ref());
        self.assistant.sent = Some(context.sent());
        self.assistant.last_note = note.clone();
        let body = if note.is_empty() {
            text.to_string()
        } else {
            format!("{note}\n\n{text}")
        };
        self.assistant.transcript.push(Item::Note {
            text: format!("context: {} bytes", note.len()),
            error: false,
        });
        let mut parts = vec![PromptPart::Text(body)];
        if self.assistant.attach {
            let view = if matches!(self.tab, Tab::Pcb) {
                "pcb"
            } else {
                "schematic"
            };
            let reply =
                pcbcraft_mcp::call_tool(&self.session, "screenshot", &json!({"view": view}));
            let data = reply.pointer("/content/0/data").and_then(Value::as_str);
            let takes = self
                .preferences
                .assistant_connections
                .iter()
                .find(|c| Some(&c.id) == self.assistant.active.as_ref())
                .is_some_and(|c| c.images);
            match pcbcraft_assistant::picture(takes, data) {
                Ok(part) => parts.push(part),
                Err(reason) => self.assistant.transcript.push(Item::Note {
                    text: reason,
                    error: false,
                }),
            }
        }
        parts
    }

    /// What the model should know now: the design summary, the view and the selection.
    fn assistant_context(&self) -> pcbcraft_assistant::Context {
        let summary = self.session.lock().map_or(Value::Null, |s| s.summary());
        let tab = match self.tab {
            Tab::Schematic => "schematic",
            Tab::Pcb => "pcb",
            Tab::Board3d => "3d",
            Tab::Rules => "rules",
            Tab::Checks => "checks",
            Tab::Libraries => "libraries",
            Tab::PlotFiles => "output files",
            Tab::Files => "files",
            Tab::Library => "library",
            Tab::Settings => "settings",
        };
        let selection = self.selected.map(|id| {
            let part = self.project.component(id).or_else(|| {
                self.project
                    .footprint(id)
                    .and_then(|f| self.project.component(f.component))
            });
            let mut nets: Vec<String> = Vec::new();
            if let Some(c) = part {
                for pin in &c.pins {
                    if let Some(n) = pin
                        .net
                        .and_then(|n| self.project.nets.iter().find(|x| x.id == n))
                        && !nets.contains(&n.name)
                    {
                        nets.push(n.name.clone());
                    }
                }
            }
            pcbcraft_assistant::Selection {
                kind: part.map_or_else(|| "object".to_string(), |c| c.kind.clone()),
                id,
                reference: part.map(|c| c.reference.clone()).unwrap_or_default(),
                value: part.map(|c| c.value.clone()).unwrap_or_default(),
                net: nets.join(", "),
                position: part.map(|c| (c.position.x, c.position.y)),
                layer: String::new(),
            }
        });
        pcbcraft_assistant::Context {
            tab,
            selection,
            summary,
        }
    }

    /// Stop the model; every question waiting for the user is declined.
    pub(crate) fn assistant_stop(&mut self) {
        let waiting: Vec<u64> = self.assistant.waiting.iter().map(|w| w.id).collect();
        for id in waiting {
            self.assistant_answer(id, false, false);
        }
        if self.assistant.busy {
            self.assistant.stopped = true;
        }
        if let Some(b) = self.assistant.backend.as_mut() {
            b.cancel();
        }
    }

    /// Send the last message again after the endpoint failed.
    pub(crate) fn assistant_retry(&mut self) {
        let text = self.assistant.last_prompt.clone();
        if self.assistant.failed && !text.is_empty() && !self.assistant.busy {
            self.assistant_prompt(text);
        }
    }

    /// Start a fresh conversation.
    pub(crate) fn assistant_new_chat(&mut self) {
        self.assistant_stop();
        self.assistant.transcript.items.clear();
        self.assistant.sent = None;
        self.assistant.busy = false;
        if let Some(b) = self.assistant.backend.as_mut() {
            b.new_session();
        }
    }

    /// Answer the approval card `id`: run the held call, or tell the model it was declined.
    pub(crate) fn assistant_answer(&mut self, id: u64, allow: bool, always: bool) {
        let Some(at) = self.assistant.waiting.iter().position(|w| w.id == id) else {
            return;
        };
        let held = self.assistant.waiting.remove(at);
        if held.each.is_some() {
            for item in &mut self.assistant.transcript.items {
                if let Item::Approval { id: x, answer, .. } = item
                    && *x == id
                {
                    *answer = Some(allow);
                }
            }
            self.stepwise_answer(held, allow);
            return;
        }
        if allow && always {
            let id = self.assistant.active.clone().unwrap_or_default();
            let keys = self
                .preferences
                .assistant_always_allow
                .entry(id)
                .or_default();
            if !keys.contains(&held.key) {
                keys.push(held.key.clone());
            }
        }
        for item in &mut self.assistant.transcript.items {
            if let Item::Approval { id: x, answer, .. } = item
                && *x == id
            {
                *answer = Some(allow);
            }
        }
        if allow {
            let change = pcbcraft_assistant::edits_design(&held.request.name, &held.request.args);
            self.assistant_admit_and_run(held.request, change, held.id);
        } else {
            self.step_failed(
                held.id,
                &held.request.name,
                &held.request.args,
                text::digest_declined(),
            );
            let _ = held.request.reply.send(error_result(&text::denied_note()));
        }
    }

    /// Count the call against the turn's limits, then run it, or tell the model to stop.
    fn assistant_admit_and_run(&mut self, request: ToolRequest, change: bool, call: u64) {
        let limit = self.preferences.assistant_turn_limit;
        let admitted = match self.assistant.turn.as_mut() {
            Some(turn) => turn.admit(change, limit),
            None => Ok(()),
        };
        if let Err(stop) = admitted {
            self.step_failed(call, &request.name, &request.args, stop.to_string());
            let _ = request.reply.send(error_result(stop));
            return;
        }
        let known = self
            .assistant
            .transcript
            .items
            .iter()
            .any(|i| matches!(i, Item::Step { id, .. } if *id == call));
        if known {
            self.step_set(call, StepState::Running, 0, String::new());
        } else {
            self.step_push(call, &request.name, &request.args, StepState::Running);
        }
        let summary = if change {
            self.assistant_summary(&request)
        } else {
            None
        };
        let started = std::time::Instant::now();
        let result = pcbcraft_mcp::call_tool(&self.session, &request.name, &request.args);
        let took = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.refresh();
        self.step_done(call, &request.name, &result, took, summary.as_ref());
        let _ = request.reply.send(result);
    }

    /// The gate's decision for one call from the model.
    fn assistant_receive(&mut self, request: ToolRequest) {
        let key = pcbcraft_assistant::tool_key(&request.name, &request.args);
        let undo_left = self
            .assistant
            .turn
            .as_ref()
            .map_or(0, |t| t.changes as usize);
        let id = self.assistant.active.clone().unwrap_or_default();
        let mode = self
            .preferences
            .assistant_modes
            .get(&id)
            .copied()
            .unwrap_or(self.preferences.assistant_mode);
        let always = self
            .preferences
            .assistant_always_allow
            .get(&id)
            .cloned()
            .unwrap_or_default();
        let verdict =
            pcbcraft_assistant::request(&request.name, &request.args, mode, undo_left, &always);
        let call = self.assistant.next_id;
        self.assistant.next_id += 1;
        match verdict {
            Verdict::Run { change } => self.assistant_admit_and_run(request, change, call),
            Verdict::Refuse(reason) => {
                self.assistant.transcript.push(Item::Note {
                    text: reason.clone(),
                    error: true,
                });
                self.step_failed(call, &request.name, &request.args, reason.clone());
                let _ = request.reply.send(error_result(&reason));
            }
            Verdict::Ask(_) => {
                let (detail, summary) = match self.assistant_preview(&request) {
                    Ok(preview) => preview,
                    Err(reason) => {
                        self.assistant.transcript.push(Item::Note {
                            text: reason.clone(),
                            error: true,
                        });
                        self.step_failed(call, &request.name, &request.args, reason.clone());
                        let _ = request.reply.send(error_result(&reason));
                        return;
                    }
                };
                let id = call;
                self.assistant.transcript.push(Item::Approval {
                    id,
                    title: pcbcraft_assistant::describe(&request.name, &request.args),
                    detail,
                    summary,
                    answer: None,
                });
                self.step_push(id, &request.name, &request.args, StepState::Waiting);
                self.assistant.waiting.push(Waiting {
                    id,
                    key,
                    request,
                    each: None,
                });
            }
        }
    }

    /// For `execute` and `batch`: run them on a copy and say what they would change.
    /// A call the engine refuses on the copy is refused with its reason.
    fn assistant_preview(
        &self,
        request: &ToolRequest,
    ) -> Result<(String, Option<pcbcraft_engine::ChangeSummary>), String> {
        let commands: Vec<pcbcraft_engine::Command> = match request.name.as_str() {
            "execute" => vec![pcbcraft_engine::Command {
                command: request
                    .args
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                params: request.args.get("params").cloned().unwrap_or(Value::Null),
            }],
            "batch" => {
                serde_json::from_value(request.args.get("commands").cloned().unwrap_or(Value::Null))
                    .map_err(|e| e.to_string())?
            }
            _ => return Ok((String::new(), None)),
        };
        let expected = request
            .args
            .get("expected_revision")
            .and_then(Value::as_u64);
        let session = self
            .session
            .lock()
            .map_err(|_| "the design is busy".to_string())?;
        let dry = session
            .dry_run(&commands, expected)
            .map_err(|e| e.to_string())?;
        let line = pcbcraft_assistant::summary_text(&dry.summary, session.revision);
        Ok((line, Some(dry.summary)))
    }

    /// End the message: merge what it changed into one undo step and show the Undo row.
    fn assistant_finish_turn(&mut self) {
        let Some(turn) = self.assistant.turn.take() else {
            return;
        };
        self.assistant.busy = false;
        let finished = self
            .session
            .lock()
            .ok()
            .and_then(|mut s| turn.finish(&mut s));
        if let Some((changes, len)) = finished {
            self.assistant.undo_len = Some(len);
            self.assistant.transcript.push(Item::UndoStep {
                changes,
                undone: false,
            });
        }
        self.assistant_suggestions();
        self.assistant_save();
    }

    /// Called once per frame: apply what the backend reported and answer its tool calls.
    pub(crate) fn assistant_poll(&mut self) {
        let events = self
            .assistant
            .backend
            .as_mut()
            .map(|b| b.poll())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Ready { .. } => self.assistant.ready = true,
                Event::TurnEnded { reason } => {
                    self.assistant.failed = reason == "error";
                    self.assistant_finish_turn();
                }
                Event::Exited { .. } => {
                    self.assistant_finish_turn();
                    self.assistant.backend = None;
                    self.assistant.ready = false;
                }
                other => self.assistant.transcript.on_event(other),
            }
        }
        while let Ok(request) = self.assistant.tools.1.try_recv() {
            self.assistant_receive(request);
        }
        if !self.assistant.busy
            && self.assistant.ready
            && let Some(at) = self
                .assistant
                .transcript
                .items
                .iter()
                .position(|i| matches!(i, Item::Queued { .. }))
            && let Item::Queued { text } = self.assistant.transcript.items.remove(at)
        {
            self.assistant_prompt(text);
        }
    }

    /// A design was opened or replaced: questions about the old one are declined. Opening a
    /// file brings back that design's saved conversation; a new or example design starts empty.
    pub(crate) fn assistant_design_opened(&mut self, name: &str) {
        let waiting: Vec<u64> = self.assistant.waiting.iter().map(|w| w.id).collect();
        for id in waiting {
            self.assistant_answer(id, false, false);
        }
        if name == "open" {
            self.assistant_load();
        } else if !self.assistant.transcript.items.is_empty() {
            self.assistant.transcript.push(Item::Note {
                text: text::new_design_note(),
                error: false,
            });
        }
    }

    fn transcript_file(&self) -> Option<std::path::PathBuf> {
        pcbcraft_assistant::transcript_path(&self.user_config(), Some(self.path.as_str()))
    }

    /// Write this design's conversation outside the design file. Does nothing for a design
    /// without a path or an empty conversation.
    pub(crate) fn assistant_save(&mut self) {
        let Some(file) = self.transcript_file() else {
            return;
        };
        if self.assistant.transcript.items.is_empty() {
            return;
        }
        match pcbcraft_assistant::save_transcript(&self.assistant.transcript.items, &file) {
            Ok(n) if n != self.assistant.saved_items => {
                self.assistant.saved_items = n;
                self.message = text::transcript_saved(n);
            }
            Ok(_) => {}
            Err(e) => self.message = e,
        }
    }

    /// Replace the conversation with the one saved for the current design (empty when none).
    pub(crate) fn assistant_load(&mut self) {
        let items = self
            .transcript_file()
            .map(|f| pcbcraft_assistant::load_transcript(&f).0)
            .unwrap_or_default();
        self.assistant.saved_items = items.len();
        self.assistant.transcript.items = items;
    }

    /// Forget the conversation and its file; the design is untouched.
    pub(crate) fn assistant_forget(&mut self) {
        self.assistant.transcript.items.clear();
        self.assistant.saved_items = 0;
        if let Some(file) = self.transcript_file() {
            pcbcraft_assistant::clear_transcript(&file);
        }
    }

    /// Save the conversation as Markdown beside the design (or in the home folder when the
    /// design has no path yet) and say where it went.
    pub(crate) fn assistant_export(&mut self) {
        let name = self.project.name.clone();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let markdown =
            pcbcraft_assistant::export_markdown(&self.assistant.transcript.items, &name, now);
        let folder = std::path::Path::new(&self.path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(std::env::temp_dir);
        let target = folder.join(format!(
            "{}-assistant.md",
            name.replace(['/', '\\', ' '], "-")
        ));
        self.message = match std::fs::write(&target, markdown) {
            Ok(()) => text::exported(&target.display().to_string()),
            Err(e) => e.to_string(),
        };
    }

    /// Undo the last message's changes with one `undo` (the Undo button of a finished message).
    pub(crate) fn assistant_undo(&mut self) {
        let _: Value = pcbcraft_mcp::call_tool(&self.session, "undo", &json!({}));
        for item in self.assistant.transcript.items.iter_mut().rev() {
            if let Item::UndoStep { undone, .. } = item {
                *undone = true;
                break;
            }
        }
        self.refresh();
    }
}
