//! A chat backend that needs no thread: it drives any [`ModelProvider`] one step per poll.
//!
//! Context: the threaded API backend cannot run in the browser. This one keeps the turn as a
//! small state machine: send a request, wait for the reply, hand tool calls to the host, wait
//! for their answers, send the next request. The pane polls it every frame.
//! Rules: the same limits as the threaded backend (24 requests per message, bounded history),
//! tool calls go through the host gate, Stop drops the waiting request and keeps the history sound.
//! Do not: block, sleep or spawn here, or stream: replies arrive whole.

use crate::api::{
    MAX_HISTORY, MAX_REQUESTS_PER_TURN, Protocol, Reply, TOOL_TIMEOUT, anthropic_tools,
    openai_tools, reply, starts_a_turn, stop_note, tool_text, tool_title, user_message, user_text,
    without_calls,
};
use crate::backend::{ChatBackend, Event, PromptPart, StartContext, ToolStatus};
use crate::prompt::SYSTEM_NOTE;
use crate::provider::{ChatRequest, ModelProvider};
use crate::tool_names::ToolNames;
use crate::turn::ToolRequest;
use pcbcraft_clock::Instant;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

type Slot = Arc<Mutex<Option<Result<Value, String>>>>;

struct Running {
    call_id: String,
    tool_id: String,
    answer: Option<Receiver<Value>>,
    done: Option<(String, bool)>,
}

enum State {
    Idle,
    Sending {
        slot: Slot,
        sound: usize,
    },
    Tools {
        running: Vec<Running>,
        deadline: Instant,
    },
}

/// Drives a [`ModelProvider`] without threads.
///
/// What and why: lets the assistant work where only callbacks exist, the browser first of all,
/// while keeping the conversation rules of the threaded backend.
///
/// # Rules
/// All work happens inside `poll`, `prompt` and `cancel`; none of them block.
///
/// # Dependencies
/// - crates/assistant/src/provider.rs (ModelProvider)
/// - crates/assistant/src/api.rs (reply parsing, message shapes, tool lists)
///
/// # Do not
/// Run retrieval or streaming here; they stay on the threaded backend.
pub struct ProviderBackend {
    provider: Arc<dyn ModelProvider>,
    sink: Option<Sender<ToolRequest>>,
    tools: Vec<Value>,
    names: ToolNames,
    history: Vec<Value>,
    events: VecDeque<Event>,
    state: State,
    requests_made: usize,
    sent_this_turn: usize,
    notified: bool,
}

impl ProviderBackend {
    /// Start a backend on `provider`; tool calls go to `ctx.tools`.
    pub fn start(
        provider: Arc<dyn ModelProvider>,
        ctx: StartContext,
    ) -> Result<ProviderBackend, String> {
        let list = pcbcraft_mcp::tool_list();
        let names = ToolNames::from_list(&list)?;
        let tools = match provider.wire() {
            Protocol::OpenAi => openai_tools(&list),
            Protocol::Anthropic => anthropic_tools(&list),
        };
        Ok(ProviderBackend {
            provider,
            sink: ctx.tools,
            tools,
            names,
            history: Vec::new(),
            events: VecDeque::new(),
            state: State::Idle,
            requests_made: 0,
            sent_this_turn: 0,
            notified: false,
        })
    }

    fn emit(&mut self, e: Event) {
        self.events.push_back(e);
    }

    fn end(&mut self, reason: &str) {
        self.state = State::Idle;
        self.emit(Event::TurnEnded {
            reason: reason.into(),
        });
    }

    fn trim(&mut self) {
        let extra = self.history.len().saturating_sub(MAX_HISTORY);
        self.history.drain(..extra);
        let lead = self
            .history
            .iter()
            .take_while(|m| !starts_a_turn(m))
            .count();
        self.history.drain(..lead);
    }

    fn send_next(&mut self) {
        if self.sent_this_turn >= MAX_REQUESTS_PER_TURN {
            let wire = self.provider.wire();
            self.history.push(user_text(
                wire,
                "Stop calling tools now. Report what you did and what is left.",
            ));
            self.emit(Event::Text {
                thought: false,
                text: format!(
                    "\n(Stopped after {MAX_REQUESTS_PER_TURN} requests for one message.)"
                ),
            });
            self.end("max_turn_requests");
            return;
        }
        self.sent_this_turn += 1;
        self.emit(Event::Note(format!(
            "request {} of {MAX_REQUESTS_PER_TURN}",
            self.sent_this_turn
        )));
        let slot: Slot = Arc::new(Mutex::new(None));
        let put = slot.clone();
        self.state = State::Sending {
            slot,
            sound: self.history.len(),
        };
        self.provider.send(
            ChatRequest {
                system: SYSTEM_NOTE.to_string(),
                messages: self.history.clone(),
                tools: self.tools.clone(),
            },
            Box::new(move |r| {
                if let Ok(mut g) = put.lock() {
                    *g = Some(r);
                }
            }),
        );
    }

    fn on_reply(&mut self, got: Result<Value, String>) {
        let wire = self.provider.wire();
        let r: Reply = match got {
            Ok(v) => reply(wire, &v),
            Err(e) => {
                self.emit(Event::Error(e));
                self.end("error");
                return;
            }
        };
        for (thought, text) in r.text {
            self.emit(Event::Text { thought, text });
        }
        let stop = r.stop.clone().unwrap_or_default();
        let cut = matches!(stop.as_str(), "length" | "max_tokens");
        let note = stop_note(&stop);
        if note.is_some() && (r.calls.is_empty() || cut) {
            let kept = if r.calls.is_empty() {
                r.assistant
            } else {
                without_calls(r.assistant)
            };
            self.history.push(kept);
            if let Some(n) = note {
                self.emit(Event::Note(n.to_string()));
            }
            self.end(&stop);
            return;
        }
        self.history.push(r.assistant);
        if r.calls.is_empty() {
            self.end("");
            return;
        }
        let mut running = Vec::new();
        for c in r.calls.iter().take(32) {
            let mcp = self.names.mcp_name(&c.name).map(str::to_string);
            let shown = mcp.clone().unwrap_or_else(|| c.name.clone());
            let args = c.arguments.clone().unwrap_or_else(|_| json!({}));
            self.requests_made += 1;
            let tool_id = format!("{}-{}", c.id, self.requests_made);
            self.emit(Event::Tool {
                id: tool_id.clone(),
                title: Some(tool_title(&shown, &args)),
                kind: Some(String::new()),
                status: Some(ToolStatus::InProgress),
                output: None,
            });
            let mut run = Running {
                call_id: c.id.clone(),
                tool_id,
                answer: None,
                done: None,
            };
            if let Err(e) = &c.arguments {
                run.done = Some((format!("Not run: {e}"), true));
            } else if let (Some(name), Some(sink)) = (mcp, &self.sink) {
                let (reply_tx, answer) = channel();
                if sink
                    .send(ToolRequest {
                        name,
                        args,
                        reply: reply_tx,
                    })
                    .is_ok()
                {
                    run.answer = Some(answer);
                } else {
                    run.done = Some(("The design tools stopped.".into(), true));
                }
            } else {
                run.done = Some((format!("unknown tool `{}`", c.name), true));
            }
            running.push(run);
        }
        self.state = State::Tools {
            running,
            deadline: pcbcraft_clock::instant_now() + TOOL_TIMEOUT,
        };
    }

    fn step_tools(&mut self) {
        let State::Tools {
            mut running,
            deadline,
        } = std::mem::replace(&mut self.state, State::Idle)
        else {
            return;
        };
        let late = pcbcraft_clock::instant_now() >= deadline;
        let mut finished = Vec::new();
        for run in &mut running {
            if run.done.is_none()
                && let Some(rx) = &run.answer
            {
                match rx.try_recv() {
                    Ok(v) => run.done = Some(tool_text(&v)),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        run.done = Some(("The design tools did not answer.".into(), true));
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) if late => {
                        run.done = Some(("The design tools did not answer.".into(), true));
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
                if let Some((text, failed)) = &run.done {
                    finished.push((run.tool_id.clone(), text.clone(), *failed));
                }
            }
        }
        for (id, text, failed) in finished {
            let shown: String = text.chars().take(2000).collect();
            self.emit(Event::Tool {
                id,
                title: None,
                kind: None,
                status: Some(if failed {
                    ToolStatus::Failed
                } else {
                    ToolStatus::Completed
                }),
                output: Some(shown),
            });
        }
        if running.iter().any(|r| r.done.is_none()) {
            self.state = State::Tools { running, deadline };
            return;
        }
        let wire = self.provider.wire();
        let mut blocks = Vec::new();
        for run in running {
            let (text, failed) = run.done.unwrap_or_default();
            match wire {
                Protocol::OpenAi => self
                    .history
                    .push(json!({"role": "tool", "tool_call_id": run.call_id, "content": text})),
                Protocol::Anthropic => blocks.push(json!({"type": "tool_result", "tool_use_id": run.call_id, "content": text, "is_error": failed})),
            }
        }
        if !blocks.is_empty() {
            self.history
                .push(json!({"role": "user", "content": blocks}));
        }
        self.send_next();
    }
}

/// Contract: crates/assistant/src/backend.rs
impl ChatBackend for ProviderBackend {
    fn poll(&mut self) -> Vec<Event> {
        if !self.notified {
            self.notified = true;
            let agent = self.provider.label();
            self.emit(Event::Ready {
                agent,
                images: true,
            });
        }
        if let State::Sending { slot, .. } = &self.state {
            let got = match slot.lock() {
                Ok(mut g) => g.take(),
                Err(e) => e.into_inner().take(),
            };
            if let Some(r) = got {
                self.on_reply(r);
            }
        }
        if matches!(self.state, State::Tools { .. }) {
            self.step_tools();
        }
        self.events.drain(..).collect()
    }

    fn prompt(&mut self, parts: &[PromptPart]) {
        if !matches!(self.state, State::Idle) {
            return;
        }
        self.history.push(user_message(self.provider.wire(), parts));
        self.trim();
        self.sent_this_turn = 0;
        self.send_next();
    }

    fn cancel(&mut self) {
        let was = std::mem::replace(&mut self.state, State::Idle);
        match was {
            State::Idle => {}
            State::Sending { sound, .. } => {
                self.history.truncate(sound);
                self.emit(Event::TurnEnded {
                    reason: "cancelled".into(),
                });
            }
            State::Tools { running, .. } => {
                // The assistant message with its calls is in the history; give each call an answer.
                let wire = self.provider.wire();
                let mut blocks = Vec::new();
                for run in running {
                    match wire {
                        Protocol::OpenAi => self.history.push(json!({"role": "tool", "tool_call_id": run.call_id, "content": "cancelled by the user"})),
                        Protocol::Anthropic => blocks.push(json!({"type": "tool_result", "tool_use_id": run.call_id, "content": "cancelled by the user", "is_error": true})),
                    }
                }
                if !blocks.is_empty() {
                    self.history
                        .push(json!({"role": "user", "content": blocks}));
                }
                self.emit(Event::TurnEnded {
                    reason: "cancelled".into(),
                });
            }
        }
    }

    fn new_session(&mut self) {
        self.state = State::Idle;
        self.history.clear();
        let agent = self.provider.label();
        self.emit(Event::Ready {
            agent,
            images: true,
        });
    }

    fn answer_permission(&mut self, _key: u64, _option: Option<&str>) {}
    fn authenticate(&mut self, _method: &str) {}
    fn set_model(&mut self, _model: &str) {}
    fn set_mode(&mut self, _mode: &str) {}
    fn shutdown(&mut self, _wait: bool) {
        self.state = State::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MemProvider;

    fn run(replies: Vec<Result<Value, String>>) -> (Vec<Event>, usize) {
        let p = Arc::new(MemProvider::new(Protocol::OpenAi, replies));
        let Ok(mut b) = ProviderBackend::start(
            p.clone(),
            StartContext {
                work_dir: Default::default(),
                tools: None,
            },
        ) else {
            return (vec![], 0);
        };
        let mut out = b.poll();
        b.prompt(&[PromptPart::Text("hi".into())]);
        for _ in 0..3 {
            out.extend(b.poll());
        }
        (out, p.requests())
    }

    #[test]
    fn a_canned_reply_becomes_text_and_ends_the_turn() {
        let (ev, n) = run(vec![Ok(MemProvider::text_reply("Hello there"))]);
        assert_eq!(n, 1);
        assert!(
            ev.iter()
                .any(|e| matches!(e, Event::Text { text, .. } if text == "Hello there"))
        );
        assert!(
            ev.iter()
                .any(|e| matches!(e, Event::TurnEnded { reason } if reason.is_empty()))
        );
    }

    #[test]
    fn a_failed_request_is_reported_as_an_error() {
        let (ev, _) = run(vec![Err("offline".into())]);
        assert!(
            ev.iter()
                .any(|e| matches!(e, Event::Error(t) if t == "offline"))
        );
    }
}
