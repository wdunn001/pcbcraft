// Adapted from SolidCraft (MIT OR Apache-2.0), commit 32558d0a:
// crates/ui-egui/src/agent_chat/mod.rs (turn accounting)

use serde_json::Value;
use std::sync::mpsc::Sender;

/// Changes the agent may make in one turn; the seeded default when the user turns the limit on.
pub const DEFAULT_TURN_LIMIT: u32 = 24;
/// Hard stop on tool-loop iterations per user message.
pub const MAX_REQUESTS_PER_TURN: u32 = 400;

/// One in-process tool call the host runs on behalf of a backend.
#[derive(Debug)]
pub struct ToolRequest {
    pub name: String,
    pub args: Value,
    pub reply: Sender<Value>,
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

impl Turn {
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
