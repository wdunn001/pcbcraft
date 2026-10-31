//! Assistant conversation model: backend seam, transcript, and turn accounting (no UI).
mod acp;
mod api;
mod backend;
mod connections;
mod context;
mod describe;
mod gate;
mod guard;
mod http;
mod prompt;
mod retrieval;
mod script;
mod steps;
mod store;
mod stream;
mod text;
mod tool_names;
mod transcript;
mod turn;

pub use acp::{
    AcpBackend, Detection as AcpDetection, Found as AcpFound, detect as detect_acp_agents,
    resolve_cli, test_agent,
};
pub use api::{
    ApiBackend, ApiConfig, MAX_HISTORY, MAX_REQUESTS_PER_TURN as MAX_API_REQUESTS_PER_TURN,
    MAX_TOOL_TEXT, Protocol, anthropic_model_list, anthropic_tools, endpoint, http_error,
    key_safe_for, model_list, openai_tools, test_anthropic_endpoint, test_endpoint,
};
pub use backend::{
AuthKind, AuthMethod, ChatBackend, Event, HttpFn, HttpRequest, HttpResponse, HttpStreamPart, ModelInfo,
    PermissionOption, PromptPart, Secrets, SourceLink, StartContext, ToolStatus, base64,
    content_blocks, deliver_streamed_body,
};
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
pub use retrieval::{
    MODEL_TOOL_NAME, RETRIEVAL_KEY_ID, Retrieval, RetrievalConfig, SearchOutcome,
    append_model_tool, pick_search_tool, register_tool_name, system_note_offered, test_retrieval,
};
pub use script::{CountingHttp, ScriptBackend};
pub use steps::{
    Digest, MAX_SUGGESTIONS, StepKind, StepState, Suggestion, command_kind, commands_of, digest,
    error_line, split_suggestions, step_kind,
};
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
