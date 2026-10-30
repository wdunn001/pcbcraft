//! Assistant conversation model: backend seam, transcript, and turn accounting (no UI).
mod api;
mod backend;
mod stream;
mod connections;
mod context;
mod describe;
mod gate;
mod guard;
mod http;
mod prompt;
mod script;
mod store;
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
    AuthKind, AuthMethod, ChatBackend, Event, HttpFn, HttpRequest, HttpStreamPart, ModelInfo,
    PermissionOption, deliver_streamed_body,
    PromptPart, Secrets, StartContext, ToolStatus, base64, content_blocks,
};
pub use connections::{Connection, PRESETS, Preset, config_for, key_id, new_id, normalize_url};
pub use context::{Context, MAX_NOTE, MAX_PICTURE_BYTES, Selection, Sent, note, picture};
pub use describe::summary_text;
pub use gate::{
    AMBIGUOUS_KEYS, FILE_EXTENSIONS, FILE_KEYS, Mode, Verdict, describe, edits_design, request,
    tool_key,
};
pub use guard::{Blocked, can_send};
pub use http::{MAX_BODY, plain_http, plain_request};
pub use prompt::SYSTEM_NOTE;
pub use script::{CountingHttp, ScriptBackend};
pub use store::{
    clear as clear_transcript, export_markdown, load as load_transcript, save as save_transcript,
    transcript_path,
};
pub use text::{MAX_PROMPT, check_prompt_length, prompt_too_long_reason};
pub use tool_names::{ToolNames, to_wire};
pub use transcript::{Item, MAX_ITEMS, Transcript};
pub use turn::{DEFAULT_TURN_LIMIT, MAX_REQUESTS_PER_TURN, ToolRequest, Turn};

#[cfg(test)]
mod api_tests;
