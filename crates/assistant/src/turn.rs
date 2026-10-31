// Adapted from SolidCraft (MIT OR Apache-2.0), commit 32558d0a:
// crates/ui-egui/src/agent_chat/mod.rs (turn accounting)

use pcbcraft_engine::Session;
use serde_json::Value;

/// Changes the agent may make in one turn; the seeded default when the user turns the limit on.
pub const DEFAULT_TURN_LIMIT: u32 = 24;
/// Hard stop on tool-loop iterations per user message.
pub const MAX_REQUESTS_PER_TURN: u32 = 400;

/// One in-process tool call the host runs on behalf of a backend.
#[cfg(not(target_arch = "wasm32"))]
pub use pcbcraft_mcp::ToolRequest;
#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct ToolRequest {
    pub name: String,
    pub args: serde_json::Value,
    pub reply: std::sync::mpsc::Sender<serde_json::Value>,
}

/// The message being answered: what undo and the limits need.
#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    pub started_revision: u64,
    pub history_len_at_start: usize,
    pub changes: u32,
    pub requests: u32,
    pub message: String,
}

/// Told to the model when a limit is reached.
const STOP_TEXT: &str = "Stop calling tools now. Report what you did and what is left.";

impl Turn {
    /// Count one tool request; `change` is true when it edits the design. Returns the text to give
    /// the model instead of running the tool once `change_limit` changes or
    /// [`MAX_REQUESTS_PER_TURN`] requests are used.
    pub fn admit(&mut self, change: bool, change_limit: u32) -> Result<(), &'static str> {
        if self.requests >= MAX_REQUESTS_PER_TURN || (change && self.changes >= change_limit) {
            return Err(STOP_TEXT);
        }
        self.requests += 1;
        if change {
            self.changes += 1;
        }
        Ok(())
    }

    /// End the turn: merge everything it changed into one undo step. Returns the number of
    /// changes and the history length right after the merge (the Undo button is live while the
    /// history still has that length), or `None` when the message changed nothing.
    pub fn finish(&self, session: &mut Session) -> Option<(u32, usize)> {
        if self.changes == 0 || session.history_len() <= self.history_len_at_start {
            return None;
        }
        session.squash_undo_since(self.history_len_at_start);
        Some((self.changes, session.history_len()))
    }

    pub fn new(started_revision: u64, history_len_at_start: usize, message: String) -> Self {
        Self {
            started_revision,
            history_len_at_start,
            changes: 0,
            requests: 0,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_engine::Command;
    use serde_json::json;

    fn add(reference: &str) -> Command {
        Command {
            command: "component.add".into(),
            params: json!({"kind":"resistor","reference":reference,"value":"1k","position":{"x":5.0,"y":5.0}}),
        }
    }

    fn work(session: &mut Session, turn: &mut Turn, n: usize) -> Result<(), String> {
        for i in 0..n {
            turn.admit(true, 24).map_err(str::to_string)?;
            session
                .batch(&[add(&format!("R{i}"))], None)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    #[test]
    fn a_messages_changes_undo_as_one_step() -> Result<(), String> {
        let mut s = Session::default();
        s.batch(&[add("R9")], None).map_err(|e| e.to_string())?;
        let mut turn = Turn::new(s.revision, s.history_len(), "add three".into());
        work(&mut s, &mut turn, 3)?;
        let (changes, len) = turn.finish(&mut s).ok_or("no undo step")?;
        assert_eq!(changes, 3);
        assert_eq!(s.history_len(), len);
        s.undo().map_err(|e| e.to_string())?;
        assert_eq!(
            s.project.components.len(),
            1,
            "only the earlier part is left"
        );
        Ok(())
    }

    #[test]
    fn an_interrupted_turns_changes_undo_as_one_step() -> Result<(), String> {
        let mut s = Session::default();
        let mut turn = Turn::new(s.revision, s.history_len(), "add many".into());
        work(&mut s, &mut turn, 2)?;
        // Stop pressed: the turn ends early but still merges what it did.
        assert!(turn.finish(&mut s).is_some());
        s.undo().map_err(|e| e.to_string())?;
        assert!(s.project.components.is_empty());
        Ok(())
    }

    #[test]
    fn merging_finds_the_turns_start_when_the_history_was_trimmed() -> Result<(), String> {
        let mut s = Session::default();
        for i in 0..40 {
            s.batch(&[add(&format!("P{i}"))], None)
                .map_err(|e| e.to_string())?;
        }
        let before = s.history_len();
        let mut turn = Turn::new(s.revision, before, "more".into());
        work(&mut s, &mut turn, 3)?;
        // The history stayed at its cap, so the start is gone: nothing is merged.
        let kept = s.history_len();
        let _ = turn.finish(&mut s);
        assert_eq!(s.history_len(), kept, "a trimmed history is left as it was");
        Ok(())
    }

    #[test]
    fn the_turn_limit_tells_the_model_to_stop() {
        let mut turn = Turn::new(0, 0, "go".into());
        for _ in 0..2 {
            assert!(turn.admit(true, 2).is_ok());
        }
        assert!(turn.admit(true, 2).is_err());
        assert!(
            turn.admit(false, 2).is_ok(),
            "reads still run past the change limit"
        );
    }
}
