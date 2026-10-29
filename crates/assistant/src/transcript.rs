// Adapted from SolidCraft (MIT OR Apache-2.0), commit 32558d0a:
// crates/ui-egui/src/agent_chat/mod.rs (conversation items and event handling)

use crate::backend::{Event, PermissionOption, ToolStatus};

/// Most entries kept in the conversation; older ones are dropped.
pub const MAX_ITEMS: usize = 2000;
/// Longest message sent (see [`crate::text::check_prompt_length`]).
pub const MAX_PROMPT: usize = 100_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    User {
        text: String,
    },
    Assistant {
        text: String,
    },
    Thought {
        text: String,
    },
    ToolCall {
        id: String,
        title: String,
        status: Option<ToolStatus>,
        content: Option<String>,
    },
    Plan(Vec<(String, String)>),
    Permission {
        key: u64,
        title: String,
        options: Vec<PermissionOption>,
        answer: Option<String>,
    },
    Approval {
        id: u64,
        title: String,
        answer: Option<bool>,
    },
    Queued {
        text: String,
    },
    Note {
        text: String,
        error: bool,
    },
    UndoStep {
        changes: u32,
        undone: bool,
    },
}

#[derive(Default)]
pub struct Transcript {
    pub items: Vec<Item>,
}

impl Transcript {
    pub fn push(&mut self, item: Item) {
        if self.items.len() >= MAX_ITEMS {
            self.items.drain(..MAX_ITEMS / 10);
        }
        self.items.push(item);
    }

    /// User and assistant lines, in order (for tests).
    pub fn texts(&self) -> Vec<String> {
        self.items
            .iter()
            .filter_map(|i| match i {
                Item::User { text } | Item::Assistant { text } | Item::Queued { text } => {
                    Some(text.clone())
                }
                _ => None,
            })
            .collect()
    }

    pub fn waiting(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i, Item::Permission { answer: None, .. }))
            .count()
    }

    /// Answer the agent's permission question at `index` (`None`: cancelled).
    pub fn answer(&mut self, index: usize, option: Option<&str>) {
        let Some(Item::Permission {
            options, answer, ..
        }) = self.items.get_mut(index)
        else {
            return;
        };
        if answer.is_some() {
            return;
        }
        let label = option
            .and_then(|o| options.iter().find(|x| x.id == o))
            .map_or_else(|| "Cancelled".to_string(), |o| o.name.clone());
        *answer = Some(label);
    }

    /// Apply one backend event to the transcript (core streaming and tool updates).
    pub fn on_event(&mut self, e: Event) {
        match e {
            Event::Text { thought, text } => {
                if thought {
                    if let Some(Item::Thought { text: last }) = self.items.last_mut() {
                        last.push_str(&text);
                    } else {
                        self.push(Item::Thought { text });
                    }
                } else if let Some(Item::Assistant { text: last }) = self.items.last_mut() {
                    last.push_str(&text);
                } else {
                    self.push(Item::Assistant { text });
                }
            }
            Event::Tool {
                id,
                title,
                status,
                output,
                ..
            } => {
                let existing = self
                    .items
                    .iter_mut()
                    .rev()
                    .find(|i| matches!(i, Item::ToolCall { id: x, .. } if *x == id));
                if let Some(Item::ToolCall {
                    title: t,
                    status: s,
                    content: c,
                    ..
                }) = existing
                {
                    if let Some(v) = title {
                        *t = v;
                    }
                    if status.is_some() {
                        *s = status;
                    }
                    if output.is_some() {
                        *c = output;
                    }
                } else {
                    self.push(Item::ToolCall {
                        id,
                        title: title.unwrap_or_default(),
                        status,
                        content: output,
                    });
                }
            }
            Event::Plan(entries) => {
                let turn = self
                    .items
                    .iter()
                    .rposition(|i| matches!(i, Item::User { .. }))
                    .unwrap_or(0);
                if let Some(Item::Plan(p)) = self
                    .items
                    .iter_mut()
                    .skip(turn)
                    .rev()
                    .find(|i| matches!(i, Item::Plan(_)))
                {
                    *p = entries;
                } else {
                    self.push(Item::Plan(entries));
                }
            }
            Event::Permission {
                key,
                title,
                options,
            } => {
                self.push(Item::Permission {
                    key,
                    title,
                    options,
                    answer: None,
                });
            }
            Event::Error(text) => self.push(Item::Note { text, error: true }),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Event;

    fn chat() -> Transcript {
        Transcript::default()
    }

    #[test]
    fn streamed_text_joins_and_tool_updates_change_their_call() {
        let mut c = chat();
        for e in [
            Event::Text {
                thought: false,
                text: "Mak".into(),
            },
            Event::Text {
                thought: false,
                text: "ing a box".into(),
            },
            Event::Tool {
                id: "t".into(),
                title: Some("batch".into()),
                kind: Some("other".into()),
                status: Some(ToolStatus::Pending),
                output: None,
            },
            Event::Tool {
                id: "t".into(),
                title: None,
                kind: None,
                status: Some(ToolStatus::Completed),
                output: Some("ok".into()),
            },
        ] {
            c.on_event(e);
        }
        assert_eq!(c.items.len(), 2);
        assert!(matches!(&c.items[0], Item::Assistant { text } if text == "Making a box"));
        assert!(
            matches!(&c.items[1], Item::ToolCall { title, status: Some(ToolStatus::Completed), content: Some(o), .. } if title == "batch" && o == "ok")
        );
    }

    #[test]
    fn a_plan_update_replaces_the_turns_plan() {
        let mut c = chat();
        c.push(Item::User { text: "go".into() });
        c.on_event(Event::Plan(vec![("a".into(), "pending".into())]));
        c.on_event(Event::Text {
            thought: false,
            text: "working".into(),
        });
        c.on_event(Event::Plan(vec![("a".into(), "completed".into())]));
        let plans: Vec<_> = c
            .items
            .iter()
            .filter(|i| matches!(i, Item::Plan(_)))
            .collect();
        assert_eq!(plans.len(), 1);
        assert!(matches!(plans[0], Item::Plan(p) if p[0].1 == "completed"));
    }

    #[test]
    fn a_permission_is_answered_once() {
        let mut c = chat();
        let opts = vec![PermissionOption {
            id: "allow".into(),
            name: "Allow".into(),
            kind: "allow_once".into(),
        }];
        c.push(Item::Permission {
            key: 5,
            title: "batch".into(),
            options: opts,
            answer: None,
        });
        assert_eq!(c.waiting(), 1);
        c.answer(0, Some("allow"));
        c.answer(0, None);
        assert!(matches!(&c.items[0], Item::Permission { answer: Some(a), .. } if a == "Allow"));
        assert_eq!(c.waiting(), 0);
    }

    #[test]
    fn the_transcript_drops_its_oldest_item_past_the_cap() {
        let mut c = chat();
        for i in 0..MAX_ITEMS {
            c.push(Item::Note {
                text: format!("n{i}"),
                error: false,
            });
        }
        assert_eq!(c.items.len(), MAX_ITEMS);
        c.push(Item::Note {
            text: "extra".into(),
            error: false,
        });
        assert_eq!(c.items.len(), MAX_ITEMS - MAX_ITEMS / 10 + 1);
        assert!(
            !c.items
                .iter()
                .any(|i| matches!(i, Item::Note { text, .. } if text == "n0"))
        );
    }
}
