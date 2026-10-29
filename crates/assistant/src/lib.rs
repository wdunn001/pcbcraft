//! Assistant conversation model: backend seam, transcript, and turn accounting (no UI).
mod backend;
mod script;
mod text;
mod transcript;
mod turn;

pub use backend::{
    AuthMethod, AuthKind, ChatBackend, Event, HttpFn, HttpRequest, ModelInfo, PermissionOption, PromptPart, Secrets,
    StartContext, ToolStatus, base64, content_blocks,
};
pub use script::ScriptBackend;
pub use text::{check_prompt_length, prompt_too_long_reason, MAX_PROMPT};
pub use transcript::{Item, Transcript, MAX_ITEMS};
pub use turn::{Turn, ToolRequest, DEFAULT_TURN_LIMIT, MAX_REQUESTS_PER_TURN};
