//! Request-shape checks on recorded wire bodies.

mod support;

use serde_json::json;
use support::{Seen, assert_request_shape};

#[test]
fn a_tool_message_without_tool_call_id_fails_the_request_shape_check() -> Result<(), String> {
    let good = Seen {
        method: "POST".into(),
        path: "/v1/chat/completions".into(),
        headers: Vec::new(),
        body: json!({"messages": [
            {"role": "tool", "tool_call_id": "c0", "content": "ok"}
        ]}),
    };
    assert_request_shape(&good)?;

    let bad = Seen {
        method: "POST".into(),
        path: "/v1/chat/completions".into(),
        headers: Vec::new(),
        body: json!({"messages": [
            {"role": "tool", "content": "ok"}
        ]}),
    };
    let err = assert_request_shape(&bad).err().ok_or("expected shape failure")?;
    assert!(err.contains("tool_call_id"), "{err}");

    let bad_anthropic = Seen {
        method: "POST".into(),
        path: "/v1/messages".into(),
        headers: Vec::new(),
        body: json!({"messages": [
            {"role": "user", "content": [{"type": "tool_result", "content": "ok"}]}
        ]}),
    };
    let err = assert_request_shape(&bad_anthropic)
        .err()
        .ok_or("expected shape failure")?;
    assert!(err.contains("tool_use_id"), "{err}");
    Ok(())
}
