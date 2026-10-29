//! Assistant conversation model: backend seam, transcript, and turn accounting (no UI).
mod api;
mod backend;
mod prompt;
mod script;
mod text;
mod tool_names;
mod transcript;
mod turn;

pub use api::{
    ApiBackend, ApiConfig, MAX_HISTORY, MAX_REQUESTS_PER_TURN as MAX_API_REQUESTS_PER_TURN,
    MAX_TOOL_TEXT, Protocol, anthropic_model_list, anthropic_tools, endpoint, http_error,
    key_safe_for, model_list, openai_tools, test_anthropic_endpoint, test_endpoint,
};
pub use backend::{
    AuthKind, AuthMethod, ChatBackend, Event, HttpFn, HttpRequest, ModelInfo, PermissionOption,
    PromptPart, Secrets, StartContext, ToolStatus, base64, content_blocks,
};
pub use prompt::SYSTEM_NOTE;
pub use script::ScriptBackend;
pub use text::{MAX_PROMPT, check_prompt_length, prompt_too_long_reason};
pub use tool_names::{ToolNames, to_wire};
pub use transcript::{Item, MAX_ITEMS, Transcript};
pub use turn::{DEFAULT_TURN_LIMIT, MAX_REQUESTS_PER_TURN, ToolRequest, Turn};

#[cfg(test)]
mod api_tests;
