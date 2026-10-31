//! Protocol conformance tests: each test walks one row of `docs/mcp-conformance.md`.
//!
//! Local rules: tests talk to `handle_rpc` and `serve` the way a client does, over JSON
//! values, with a fresh `DocumentHub`. Lifecycle-gate tests live in `lib.rs` (#477) and
//! annotation tests in `lib.rs` (#476); do not repeat them here.
//!
//! # Dependencies
//! `crates/mcp/src/lib.rs` (`handle_rpc`, `serve`, `tool_list`, `MAX_MESSAGE`),
//! `docs/mcp-conformance.md`.
//!
//! # Do not
//! Do not use `unwrap`, `expect` or `panic!`; return `Result<(), String>`.
use super::*;

fn hub() -> Result<DocumentHub, String> {
    DocumentHub::new(Session::default(), None)
}

fn initialize(version: &str) -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"t","version":"1"}}})
}

fn call(hub: &DocumentHub, request: Value) -> Result<Value, String> {
    handle_rpc(hub, request, None, None).ok_or_else(|| "expected a reply".to_string())
}

#[test]
fn initialize_answers_each_supported_version_and_falls_back_to_the_newest_for_an_unknown_one()
-> Result<(), String> {
    let hub = hub()?;
    for (asked, answered) in [
        ("2025-11-25", "2025-11-25"),
        ("2025-06-18", "2025-06-18"),
        ("2024-11-05", "2024-11-05"),
        ("1999-01-01", "2025-11-25"),
    ] {
        let reply = call(&hub, initialize(asked))?;
        assert_eq!(
            reply["result"]["protocolVersion"], answered,
            "asked {asked}"
        );
        assert_eq!(reply["result"]["serverInfo"]["name"], "pcbcraft");
    }
    Ok(())
}

#[test]
fn initialize_without_client_info_is_refused_with_invalid_params() -> Result<(), String> {
    let hub = hub()?;
    let reply = call(
        &hub,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{}}}),
    )?;
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    Ok(())
}

#[test]
fn a_notification_never_gets_a_reply() -> Result<(), String> {
    let hub = hub()?;
    for method in [
        "notifications/initialized",
        "notifications/cancelled",
        "nonsense",
    ] {
        let reply = handle_rpc(&hub, json!({"jsonrpc":"2.0","method":method}), None, None);
        assert!(reply.is_none(), "{method} got {reply:?}");
    }
    Ok(())
}

#[test]
fn cancelled_notification_for_an_unknown_request_is_ignored() -> Result<(), String> {
    let hub = hub()?;
    let cancel = json!({
        "jsonrpc":"2.0",
        "method":"notifications/cancelled",
        "params":{"requestId":424242}
    });
    assert!(handle_rpc(&hub, cancel.clone(), None).is_none());
    let initialize = initialize("2025-11-25");
    let ping = json!({"jsonrpc":"2.0","id":2,"method":"ping"});
    let input = format!("{initialize}\n{cancel}\n{ping}\n");
    let (replies, outcome) = serve_lines(input.as_bytes())?;
    assert!(outcome.is_ok(), "{outcome:?}");
    assert_eq!(replies.len(), 2, "cancel must not produce stdout");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(replies[1]["result"], json!({}));
    Ok(())
}

#[test]
fn ping_returns_an_empty_object() -> Result<(), String> {
    let reply = call(&hub()?, json!({"jsonrpc":"2.0","id":7,"method":"ping"}))?;
    assert_eq!(reply["id"], 7);
    assert_eq!(reply["result"], json!({}));
    Ok(())
}

#[test]
fn an_unknown_method_gets_method_not_found() -> Result<(), String> {
    let reply = call(
        &hub()?,
        json!({"jsonrpc":"2.0","id":1,"method":"no/such/method"}),
    )?;
    assert_eq!(reply["error"]["code"], -32601);
    Ok(())
}

#[test]
fn a_request_with_the_wrong_jsonrpc_version_or_a_missing_method_gets_invalid_request()
-> Result<(), String> {
    let hub = hub()?;
    for request in [
        json!({"jsonrpc":"1.0","id":1,"method":"ping"}),
        json!({"id":1,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":1}),
        json!({"jsonrpc":"2.0","id":[1],"method":"ping"}),
    ] {
        let reply = call(&hub, request.clone())?;
        assert_eq!(reply["error"]["code"], -32600, "{request}");
    }
    Ok(())
}

/// Run raw input lines through `serve` and return the parsed output lines.
fn serve_lines(input: &[u8]) -> Result<(Vec<Value>, Result<(), String>), String> {
    let hub = hub()?;
    let mut output = vec![];
    let mut reader = input;
    let outcome = serve(&mut reader, &mut output, &hub, None, None);
    let replies = String::from_utf8(output)
        .map_err(|e| e.to_string())?
        .lines()
        .map(|line| serde_json::from_str(line).map_err(|e| format!("stdout line is not JSON: {e}")))
        .collect::<Result<Vec<Value>, String>>()?;
    Ok((replies, outcome))
}

#[test]
fn a_json_array_request_is_rejected_because_batching_is_not_supported() -> Result<(), String> {
    let (replies, _) = serve_lines(b"[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}]\n")?;
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0]["error"]["code"], -32600, "{}", replies[0]);
    Ok(())
}

#[test]
fn broken_json_gets_parse_error_with_a_null_id() -> Result<(), String> {
    let (replies, _) = serve_lines(b"{not json\n")?;
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(replies[0]["id"], Value::Null);
    Ok(())
}

#[test]
fn every_tool_name_is_one_to_128_letters_digits_underscore_hyphen_or_dot() -> Result<(), String> {
    let tools = tool_list();
    let list = tools["tools"].as_array().ok_or("tools is an array")?;
    assert!(!list.is_empty());
    for tool in list {
        let name = tool["name"].as_str().ok_or("tool has a name")?;
        assert!((1..=128).contains(&name.len()), "{name}");
        assert!(
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')),
            "{name}"
        );
    }
    Ok(())
}

// Changes with #483 and #484 only if they add tools; resources and prompts are not tools.
#[test]
fn an_unknown_tool_name_is_a_protocol_error_but_bad_arguments_are_a_tool_error_with_is_error_true()
-> Result<(), String> {
    let hub = hub()?;
    let unknown = call(
        &hub,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"no_such_tool","arguments":{}}}),
    )?;
    assert_eq!(unknown["error"]["code"], -32602, "{unknown}");
    let missing = call(
        &hub,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"arguments":{}}}),
    )?;
    assert_eq!(missing["error"]["code"], -32602, "{missing}");
    let bad = call(
        &hub,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"open","arguments":{}}}),
    )?;
    assert!(bad.get("error").is_none(), "{bad}");
    assert_eq!(bad["result"]["isError"], true, "{bad}");
    Ok(())
}

#[test]
fn a_tool_result_carries_text_content_and_a_structured_copy() -> Result<(), String> {
    let reply = call(
        &hub()?,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"inspect_design","arguments":{}}}),
    )?;
    let result = &reply["result"];
    assert_eq!(result["isError"], false);
    assert_eq!(result["content"][0]["type"], "text");
    let text = result["content"][0]["text"]
        .as_str()
        .ok_or("text content")?;
    let parsed: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    assert_eq!(parsed, result["structuredContent"]);
    Ok(())
}

#[test]
fn a_message_over_one_mebibyte_is_refused_and_stdout_stays_json_only() -> Result<(), String> {
    let mut input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n".to_vec();
    input.extend(std::iter::repeat_n(
        b'a',
        usize::try_from(MAX_MESSAGE).map_err(|e| e.to_string())? + 10,
    ));
    input.push(b'\n');
    let (replies, outcome) = serve_lines(&input)?;
    assert!(outcome.is_err_and(|e| e.contains("1 MiB")));
    assert_eq!(
        replies.len(),
        1,
        "only the ping before the oversized line was answered"
    );
    assert_eq!(replies[0]["result"], json!({}));
    Ok(())
}

// Resources (#483) and prompts (#484) are advertised; logging and completion are not.
#[test]
fn capabilities_advertise_only_tools_and_resources_and_prompts_calls_are_method_not_found()
-> Result<(), String> {
    let hub = hub()?;
    let reply = call(&hub, initialize("2025-11-25"))?;
    let capabilities = reply["result"]["capabilities"]
        .as_object()
        .ok_or("capabilities is an object")?;
    assert_eq!(
        capabilities.keys().collect::<Vec<_>>(),
        ["prompts", "resources", "tools"]
    );
    for method in ["logging/setLevel", "completion/complete"] {
        let reply = call(&hub, json!({"jsonrpc":"2.0","id":2,"method":method}))?;
        assert_eq!(reply["error"]["code"], -32601, "{method}");
    }
    Ok(())
}

fn output_schema(name: &str) -> Result<Value, String> {
    tool_list()["tools"]
        .as_array()
        .and_then(|tools| tools.iter().find(|tool| tool["name"] == name))
        .map(|tool| tool["outputSchema"].clone())
        .filter(|schema| schema.is_object())
        .ok_or_else(|| format!("{name} has no outputSchema"))
}

#[test]
fn inspect_design_result_validates_against_its_output_schema() -> Result<(), String> {
    let hub = DocumentHub::new(Session::sample().map_err(|e| e.to_string())?, None)?;
    let result = hub.call_tool("inspect_design", &json!({}));
    validate_schema(
        &result["structuredContent"],
        &output_schema("inspect_design")?,
        "structuredContent",
    )?;
    let schema = output_schema("inspect_design")?;
    for key in ["revision", "project", "history", "capabilities"] {
        assert!(
            schema["required"]
                .as_array()
                .is_some_and(|r| r.iter().any(|k| k == key))
        );
    }
    // The schema must be able to fail: a result without a revision is refused.
    let mut broken = result["structuredContent"].clone();
    if let Some(object) = broken.as_object_mut() {
        object.remove("revision");
    }
    assert!(validate_schema(&broken, &schema, "structuredContent").is_err());
    Ok(())
}

#[test]
fn checks_result_validates_against_its_output_schema() -> Result<(), String> {
    let hub = DocumentHub::new(Session::sample().map_err(|e| e.to_string())?, None)?;
    let schema = output_schema("checks")?;
    for args in [json!({}), json!({"copper_only":true})] {
        let result = hub.call_tool("checks", &args);
        validate_schema(&result["structuredContent"], &schema, "structuredContent")?;
    }
    assert!(validate_schema(&json!({"erc":[]}), &schema, "structuredContent").is_err());
    Ok(())
}

#[test]
fn only_inspect_design_and_checks_carry_an_output_schema() {
    let names: Vec<String> = tool_list()["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|tool| tool.get("outputSchema").is_some())
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect();
    assert_eq!(names, ["inspect_design", "checks"]);
}
