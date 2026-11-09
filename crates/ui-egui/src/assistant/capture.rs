//! Scripted Assistant states for `--capture` images (no model, no network).
//!
//! Context: the screenshot job and the design review need the pane in each state of a run: steps
//! running, steps finished, the review card, the undo card and the next-step cards. The review
//! and undo states go through the real gate and the real engine on the loaded design, so their
//! figures are the engine's own; only the model is absent.
//! Rules: used only when `--assistant <state>` is given with `--capture`; nothing here is reachable
//! from normal use; the conversation is never saved (the design has no transcript path here).
//! Dependencies: `mod.rs` (the gate, `assistant_receive`, `assistant_answer`).
//! Do not: add a state that fakes a figure; use the real flow or leave the state out.

use super::App;
use pcbcraft_assistant::{
    ChatBackend, Event, Item, PromptPart, StepState, Suggestion, ToolRequest, Turn,
};
use serde_json::json;
use std::sync::mpsc::channel;

/// A backend that never answers: the pane shows the scripted conversation as it is.
struct Idle;

impl ChatBackend for Idle {
    fn poll(&mut self) -> Vec<Event> {
        Vec::new()
    }
    fn prompt(&mut self, _: &[PromptPart]) {}
    fn cancel(&mut self) {}
    fn new_session(&mut self) {}
    fn answer_permission(&mut self, _: u64, _: Option<&str>) {}
    fn authenticate(&mut self, _: &str) {}
    fn set_model(&mut self, _: &str) {}
    fn set_mode(&mut self, _: &str) {}
    fn shutdown(&mut self, _: bool) {}
}

const PROMPT: &str = "Make a 60 by 40 mm board that lights a green LED from 5 V. I will solder it by hand. Two layers.";

fn step(id: u64, title: &str, state: StepState, ms: u64, result: &str, commands: &[&str]) -> Item {
    Item::Step {
        id,
        title: title.into(),
        state,
        ms,
        result: result.into(),
        commands: commands.iter().map(|c| (*c).to_string()).collect(),
    }
}

impl App {
    /// Put the pane in a scripted state: `run`, `run-done`, `run-open`, `review`, `applied` or
    /// `next`. Any other word leaves the pane as it was.
    pub fn with_capture_assistant(mut self, state: Option<String>) -> Self {
        let Some(state) = state else { return self };
        self.preferences.assistant_enabled = true;
        self.preferences.aside_lower_tab = 1;
        self.assistant_use(Box::new(Idle));
        self.assistant.ready = true;
        let user = Item::User {
            text: PROMPT.into(),
        };
        match state.as_str() {
            "run" => {
                self.assistant.busy = true;
                self.assistant.transcript.items = vec![
                    user,
                    Item::Assistant {
                        text: "Plan approved. Running it now.".into(),
                    },
                    step(
                        1,
                        "Choosing parts",
                        StepState::Done,
                        4000,
                        "Found parts for 6 of 6 searches",
                        &["library.search"],
                    ),
                    step(
                        2,
                        "Placing",
                        StepState::Done,
                        3000,
                        "Placed 3 parts inside the outline, 0 overlaps",
                        &["component.add"],
                    ),
                    step(
                        3,
                        "Routing",
                        StepState::Running,
                        0,
                        "Kept all existing copper; 3 nets now need routing",
                        &["track.add"],
                    ),
                    step(4, "Checking", StepState::Waiting, 0, "", &["checks"]),
                ];
            }
            "run-done" | "run-open" => {
                self.assistant.transcript.items = vec![
                    user,
                    step(
                        1,
                        "Choosing parts",
                        StepState::Done,
                        4000,
                        "Found parts for 6 of 6 searches",
                        &["library.search"],
                    ),
                    step(
                        2,
                        "Placing",
                        StepState::Done,
                        3000,
                        "Placed 3 parts inside the outline, 0 overlaps",
                        &["component.add"],
                    ),
                    step(
                        3,
                        "Routing",
                        StepState::Done,
                        7000,
                        "Kept all existing copper; 3 nets now need routing",
                        &["track.add"],
                    ),
                    step(
                        4,
                        "Checking",
                        StepState::Done,
                        5000,
                        "No rule problems; 0 connections still need routing",
                        &["checks"],
                    ),
                ];
                if state == "run-open" {
                    self.assistant.open_runs.insert(1);
                }
            }
            "review" | "applied" => {
                self.assistant.transcript.items = vec![user];
                let (revision, history) = self
                    .session
                    .lock()
                    .map_or((0, 0), |s| (s.revision, s.history_len()));
                self.assistant.turn = Some(Turn::new(revision, history, PROMPT.into()));
                self.assistant.busy = true;
                let part = |r: &str, x: f64| {
                    json!({"command":"component.add","params":{
                        "kind":"resistor","reference":r,"value":"330","position":{"x":x,"y":30.0}}})
                };
                let track = |x0: f64, x1: f64| {
                    json!({"command":"track.add","params":{
                        "net":4,"start":{"x":x0,"y":10.0},"end":{"x":x1,"y":10.0},"width":0.25}})
                };
                let args = json!({"commands":[
                    part("R2", 20.0), part("R3", 30.0), part("R4", 40.0),
                    json!({"command":"net.create","params":{"name":"SENSE"}}),
                    track(10.0, 30.0), track(30.0, 51.4),
                ]});
                let (reply, _unused) = channel();
                self.assistant_receive(ToolRequest {
                    name: "batch".into(),
                    args,
                    reply,
                });
                if state == "applied" {
                    let id = self.assistant.waiting.first().map(|w| w.id);
                    if let Some(id) = id {
                        self.assistant_answer(id, true, false);
                    }
                    self.assistant.transcript.push(Item::Assistant {
                        text: "Applied. The board has the new parts and tracks.".into(),
                    });
                    self.assistant_finish_turn();
                }
            }
            "next" => {
                self.assistant.transcript.items = vec![
                    Item::Assistant {
                        text: "Both fixes are applied and the board passes every check.".into(),
                    },
                    Item::UndoStep {
                        changes: 2,
                        undone: false,
                    },
                    Item::Suggestions(vec![
                        Suggestion {
                            title: "Make shop files".into(),
                            detail: "Gerber and drill files, ready to send.".into(),
                            recommended: true,
                        },
                        Suggestion {
                            title: "Mark the LED polarity".into(),
                            detail: "Draw a plus sign next to the anode on the silkscreen.".into(),
                            recommended: false,
                        },
                        Suggestion {
                            title: "Add a smoothing capacitor".into(),
                            detail: "A 10 uF part near J1 steadies the 5 V input.".into(),
                            recommended: false,
                        },
                    ]),
                ];
            }
            _ => {}
        }
        self
    }
}
