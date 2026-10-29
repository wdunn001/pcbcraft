// Adapted from SolidCraft (MIT OR Apache-2.0), commit 32558d0a:
// crates/ui-egui/src/agent_chat/tests.rs (Script backend)

use crate::backend::{ChatBackend, Event, PromptPart};
use std::sync::{Arc, Mutex};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    }
}

/// A backend that records what the pane asks of it, and reports what the test pushes.
#[derive(Clone, Default)]
pub struct ScriptBackend {
    calls: Arc<Mutex<Vec<String>>>,
    events: Arc<Mutex<Vec<Event>>>,
    prompts: Arc<Mutex<Vec<Vec<PromptPart>>>>,
}

impl ScriptBackend {
    pub fn calls(&self) -> Vec<String> {
        lock(&self.calls).clone()
    }

    pub fn prompts(&self) -> Vec<Vec<PromptPart>> {
        lock(&self.prompts).clone()
    }

    pub fn report(&self, e: Event) {
        lock(&self.events).push(e);
    }
}

impl ChatBackend for ScriptBackend {
    fn poll(&mut self) -> Vec<Event> {
        std::mem::take(&mut *lock(&self.events))
    }

    fn prompt(&mut self, parts: &[PromptPart]) {
        let text = parts
            .iter()
            .find_map(|p| {
                if let PromptPart::Text(t) = p {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        lock(&self.calls).push(format!("prompt: {text}"));
        lock(&self.prompts).push(parts.to_vec());
    }

    fn cancel(&mut self) {
        lock(&self.calls).push("cancel".into());
    }

    fn new_session(&mut self) {
        lock(&self.calls).push("new_session".into());
    }

    fn set_model(&mut self, model: &str) {
        lock(&self.calls).push(format!("set_model: {model}"));
    }

    fn answer_permission(&mut self, key: u64, option: Option<&str>) {
        lock(&self.calls).push(format!("permission: {key} {option:?}"));
    }

    fn authenticate(&mut self, _: &str) {}

    fn set_mode(&mut self, _: &str) {}

    fn shutdown(&mut self, _: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Event, ToolStatus};

    #[test]
    fn script_backend_replays_events_and_records_prompts() {
        let mut backend = ScriptBackend::default();
        backend.prompt(&[PromptPart::Text("hello".into())]);
        backend.report(Event::Text {
            thought: false,
            text: "hi".into(),
        });
        backend.report(Event::Tool {
            id: "t1".into(),
            title: Some("run".into()),
            kind: None,
            status: Some(ToolStatus::Pending),
            output: None,
        });
        backend.report(Event::TurnEnded {
            reason: String::new(),
        });
        let events = backend.poll();
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], Event::Text { text, .. } if text == "hi"));
        assert!(matches!(&events[1], Event::Tool { id, .. } if id == "t1"));
        assert!(matches!(&events[2], Event::TurnEnded { reason } if reason.is_empty()));
        assert_eq!(backend.prompts().len(), 1);
        assert_eq!(backend.calls(), vec!["prompt: hello".to_string()]);
    }
}
