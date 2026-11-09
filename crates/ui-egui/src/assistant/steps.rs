//! Step rows, step-by-step apply and next-step cards: the App side.
//!
//! Context: `mod.rs` receives every tool call of the model; this file turns each one into an
//! `Item::Step` (plain name, state, duration, one-line result), runs a batch one command at a
//! time on request, and turns the closing lines of a message into `Item::Suggestions`.
//! Rules: the step title and result are stored already in the user's language (the transcript
//! only keeps plain text); a result line is made from counts, never from the model's output;
//! a command that fails is shown with the engine's first line of reason.
//! Dependencies: `crates/assistant/src/steps.rs` (kinds, digests, suggestion parsing),
//! `crates/engine/src/dry_run.rs` (`ChangeSummary`).
//! Do not: draw anything here; `cards.rs` and `pane.rs` draw.

use super::{App, Waiting, error_result, text};
use pcbcraft_assistant::{Item, StepState, ToolRequest};
use serde_json::{Value, json};
use std::sync::mpsc::channel;

/// A batch applied one command at a time, waiting for the user between commands.
pub(super) struct Stepwise {
    /// The model's original call; its reply is sent once, when the last command ran or one stopped.
    pub(super) request: ToolRequest,
    pub(super) commands: Vec<pcbcraft_engine::Command>,
    pub(super) next: usize,
}

/// The one-line result of a change, from what its dry run reported.
pub(super) fn edit_line(s: &pcbcraft_engine::ChangeSummary) -> String {
    let count = |kind: &str| s.added_counts.get(kind).copied().unwrap_or(0);
    let (parts, tracks, vias) = (count("component"), count("track"), count("via"));
    let other = s
        .added_counts
        .values()
        .sum::<usize>()
        .saturating_sub(parts + tracks + vias);
    let mut added: Vec<String> = Vec::new();
    if parts > 0 {
        added.push(text::count_parts(parts));
    }
    if tracks > 0 {
        added.push(text::count_tracks(tracks));
    }
    if vias > 0 {
        added.push(text::count_vias(vias));
    }
    if other > 0 {
        added.push(text::count_other(other));
    }
    let mut pieces: Vec<String> = Vec::new();
    if !added.is_empty() {
        pieces.push(text::digest_added(&added.join(", ")));
    }
    let changed = s.changed.len() + s.moved.len();
    if changed > 0 {
        pieces.push(text::digest_changed().replace("{n}", &changed.to_string()));
    }
    if !s.removed.is_empty() {
        pieces.push(text::digest_removed().replace("{n}", &s.removed.len().to_string()));
    }
    if pieces.is_empty() {
        return text::digest_nothing();
    }
    pieces.join("; ")
}

/// "2.1" for 2150 bytes.
pub(super) fn kb(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / 1024.0)
}

impl App {
    /// Add a step row for call `id`.
    pub(super) fn step_push(&mut self, id: u64, tool: &str, args: &Value, state: StepState) {
        let kind = pcbcraft_assistant::step_kind(tool, args);
        self.assistant.transcript.push(Item::Step {
            id,
            title: text::step_name(kind),
            state,
            ms: 0,
            result: String::new(),
            commands: pcbcraft_assistant::commands_of(tool, args),
        });
    }

    /// Change the row of call `id`.
    pub(super) fn step_set(&mut self, id: u64, new: StepState, took: u64, line: String) {
        for item in self.assistant.transcript.items.iter_mut().rev() {
            if let Item::Step {
                id: x,
                state,
                ms,
                result,
                ..
            } = item
                && *x == id
            {
                *state = new;
                *ms = took;
                *result = line;
                return;
            }
        }
    }

    /// A step that never ran: refused, declined or stopped.
    pub(super) fn step_failed(&mut self, id: u64, tool: &str, args: &Value, reason: String) {
        let known = self
            .assistant
            .transcript
            .items
            .iter()
            .any(|i| matches!(i, Item::Step { id: x, .. } if *x == id));
        if !known {
            self.step_push(id, tool, args, StepState::Failed);
        }
        self.step_set(id, StepState::Failed, 0, reason);
    }

    /// Finish call `id` from its result.
    pub(super) fn step_done(
        &mut self,
        id: u64,
        tool: &str,
        result: &Value,
        took: u64,
        summary: Option<&pcbcraft_engine::ChangeSummary>,
    ) {
        let failed = result.get("isError").and_then(Value::as_bool) == Some(true);
        let line = if failed {
            text::digest_failed(&pcbcraft_assistant::error_line(result))
        } else if let Some(d) = pcbcraft_assistant::digest(tool, result) {
            text::digest(&d)
        } else if let Some(s) = summary {
            edit_line(s)
        } else {
            text::digest_done()
        };
        let state = if failed {
            StepState::Failed
        } else {
            StepState::Done
        };
        self.step_set(id, state, took, line);
    }

    /// What a call would change, found on a copy (None for a call that edits nothing or fails).
    pub(super) fn assistant_summary(
        &self,
        request: &ToolRequest,
    ) -> Option<pcbcraft_engine::ChangeSummary> {
        let commands = commands_for(request)?;
        let expected = request
            .args
            .get("expected_revision")
            .and_then(Value::as_u64);
        let session = self.session.lock().ok()?;
        session.dry_run(&commands, expected).ok().map(|d| d.summary)
    }

    /// The Apply step by step button: run the batch one command at a time, asking before each.
    pub(crate) fn assistant_answer_each(&mut self, id: u64) {
        let Some(at) = self.assistant.waiting.iter().position(|w| w.id == id) else {
            return;
        };
        let held = self.assistant.waiting.remove(at);
        let commands = commands_for(&held.request).unwrap_or_default();
        for item in &mut self.assistant.transcript.items {
            if let Item::Approval { id: x, answer, .. } = item
                && *x == id
            {
                *answer = Some(true);
            }
        }
        self.step_set(id, StepState::Done, 0, text::apply_each());
        let stepwise = Stepwise {
            request: held.request,
            commands,
            next: 0,
        };
        self.stepwise_ask(stepwise);
    }

    /// Ask about the next command of a stepwise batch, or finish it.
    fn stepwise_ask(&mut self, st: Stepwise) {
        let Some(command) = st.commands.get(st.next).cloned() else {
            let note = format!(
                "All {} commands were applied one by one.",
                st.commands.len()
            );
            let _ = st
                .request
                .reply
                .send(json!({"content":[{"type":"text","text":note}],"isError":false}));
            return;
        };
        let args = json!({"command": command.command, "params": command.params});
        let (reply, _unused) = channel();
        let one = ToolRequest {
            name: "execute".into(),
            args: args.clone(),
            reply,
        };
        let summary = match self.assistant_summary(&one) {
            Some(s) => s,
            None => {
                let reason = format!(
                    "Command {} of {} would not apply.",
                    st.next + 1,
                    st.commands.len()
                );
                let _ = st.request.reply.send(error_result(&reason));
                return;
            }
        };
        let id = self.assistant.next_id;
        self.assistant.next_id += 1;
        let title = format!(
            "{}: {}",
            text::step_n(st.next + 1, st.commands.len()),
            pcbcraft_assistant::describe("execute", &args)
        );
        self.assistant.transcript.push(Item::Approval {
            id,
            title,
            detail: String::new(),
            summary: Some(summary),
            answer: None,
        });
        self.step_push(id, "execute", &args, StepState::Waiting);
        self.assistant.waiting.push(Waiting {
            id,
            key: pcbcraft_assistant::tool_key("execute", &args),
            request: one,
            each: Some(st),
        });
    }

    /// The answer to one command of a stepwise batch.
    pub(super) fn stepwise_answer(&mut self, held: Waiting, allow: bool) {
        let Some(mut st) = held.each else { return };
        let total = st.commands.len();
        if !allow {
            self.step_failed(
                held.id,
                "execute",
                &held.request.args,
                text::digest_declined(),
            );
            let note = format!(
                "The user stopped after {} of {total} commands; those {} stay applied.",
                st.next, st.next
            );
            let _ = st.request.reply.send(error_result(&note));
            return;
        }
        let (reply, answer) = channel();
        let call = ToolRequest {
            name: held.request.name.clone(),
            args: held.request.args.clone(),
            reply,
        };
        self.assistant_admit_and_run(call, true, held.id);
        let failed = answer
            .try_recv()
            .ok()
            .filter(|r| r.get("isError").and_then(Value::as_bool) == Some(true));
        if let Some(result) = failed {
            let note = format!(
                "Command {} of {total} failed: {}. The first {} stay applied.",
                st.next + 1,
                pcbcraft_assistant::error_line(&result),
                st.next
            );
            let _ = st.request.reply.send(error_result(&note));
            return;
        }
        st.next += 1;
        self.stepwise_ask(st);
    }

    /// End of a message: turn the closing "Next:" lines of its last text into cards.
    pub(super) fn assistant_suggestions(&mut self) {
        let items = &mut self.assistant.transcript.items;
        let from = items
            .iter()
            .rposition(|i| matches!(i, Item::User { .. }))
            .unwrap_or(0);
        let Some(at) = items
            .iter()
            .rposition(|i| matches!(i, Item::Assistant { .. }))
            .filter(|at| *at >= from)
        else {
            return;
        };
        let Item::Assistant { text } = &items[at] else {
            return;
        };
        let (body, cards) = pcbcraft_assistant::split_suggestions(text);
        if cards.is_empty() {
            return;
        }
        if body.is_empty() {
            items.remove(at);
        } else {
            items[at] = Item::Assistant { text: body };
        }
        self.assistant.transcript.push(Item::Suggestions(cards));
    }

    /// A next-step card was pressed: send its title as the next message.
    pub(crate) fn assistant_pick(&mut self, title: String) {
        if self.assistant.backend.is_none() {
            return;
        }
        self.assistant.input = title;
        self.assistant_send();
    }
}

/// The commands of an `execute` or `batch` call; `None` for any other tool.
fn commands_for(request: &ToolRequest) -> Option<Vec<pcbcraft_engine::Command>> {
    match request.name.as_str() {
        "execute" => Some(vec![pcbcraft_engine::Command {
            command: request
                .args
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            params: request.args.get("params").cloned().unwrap_or(Value::Null),
        }]),
        "batch" => serde_json::from_value(request.args.get("commands").cloned()?).ok(),
        _ => None,
    }
}
