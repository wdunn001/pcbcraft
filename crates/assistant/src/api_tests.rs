//! API backend tests with a fake HTTP function: nothing here touches the network.

use crate::api::*;
use crate::backend::{
    ChatBackend, Event, HttpFn, HttpRequest, PromptPart, StartContext, ToolStatus,
};
use crate::tool_names::{ToolNames, to_wire};
use crate::turn::ToolRequest;
use serde_json::{Value, json};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Log = Arc<Mutex<Vec<HttpRequest>>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    }
}

/// A fake endpoint: `GET` answers an empty model list, the Nth `POST` is answered by `reply`.
fn fake(reply: impl Fn(usize) -> (u16, Value) + Send + Sync + 'static) -> (HttpFn, Log) {
    let log: Log = Arc::default();
    let seen = log.clone();
    let http: HttpFn = Arc::new(move |r: &HttpRequest| {
        let n = {
            let mut l = lock(&seen);
            l.push(r.clone());
            l.iter().filter(|x| x.method == "POST").count()
        };
        if r.method == "GET" {
            return Ok((200, b"{\"data\":[]}".to_vec()));
        }
        let (status, body) = reply(n);
        Ok((status, body.to_string().into_bytes()))
    });
    (http, log)
}

fn posts(log: &Log) -> Vec<Value> {
    lock(log)
        .iter()
        .filter(|r| r.method == "POST")
        .map(|r| {
            serde_json::from_slice(r.body.as_deref().unwrap_or(b"null")).unwrap_or(Value::Null)
        })
        .collect()
}

fn cfg(protocol: Protocol) -> ApiConfig {
    ApiConfig {
        protocol,
        base_url: "http://127.0.0.1:1/v1".into(),
        model: "m".into(),
        key: None,
        system_note: String::new(),
    }
}

/// A host that answers every tool call with `text`; the names it was asked are kept.
fn host(text: &'static str) -> (StartContext, Arc<Mutex<Vec<String>>>) {
    let (tx, rx) = channel::<ToolRequest>();
    let names = Arc::new(Mutex::new(Vec::new()));
    let kept = names.clone();
    std::thread::spawn(move || {
        for r in rx {
            lock(&kept).push(r.name.clone());
            let _ = r
                .reply
                .send(json!({"content": [{"type": "text", "text": text}], "isError": false}));
        }
    });
    (
        StartContext {
            work_dir: std::path::PathBuf::new(),
            tools: Some(tx),
        },
        names,
    )
}

/// Poll until `done` matches an event, giving up after ten seconds.
fn run_until(b: &mut ApiBackend, done: impl Fn(&Event) -> bool) -> Result<Vec<Event>, String> {
    let end = Instant::now() + Duration::from_secs(10);
    let mut all = Vec::new();
    while Instant::now() < end {
        for e in b.poll() {
            let stop = done(&e);
            all.push(e);
            if stop {
                return Ok(all);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(format!("no end of turn; saw {all:?}"))
}

fn ended(e: &Event) -> bool {
    matches!(e, Event::TurnEnded { .. })
}

fn say(b: &mut ApiBackend, t: &str) {
    b.prompt(&[PromptPart::Text(t.into())]);
}

fn openai_text(t: &str) -> Value {
    json!({"choices": [{"message": {"role": "assistant", "content": t}}]})
}

fn first_mcp_name() -> Result<String, String> {
    let list = pcbcraft_mcp::tool_list();
    list["tools"][0]["name"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "no tools".to_string())
}

/// The first tool's name as a model sees and uses it.
fn first_tool_name() -> Result<String, String> {
    first_mcp_name().map(|n| to_wire(&n))
}

fn openai_call_of(name: &str, n: usize) -> Value {
    let calls: Vec<Value> =
        (0..n).map(|i| json!({"id": format!("c{i}"), "type": "function", "function": {"name": name, "arguments": "{}"}})).collect();
    json!({"choices": [{"message": {"role": "assistant", "content": null, "tool_calls": calls}}]})
}

#[test]
fn an_openai_reply_with_two_tool_calls_runs_both_through_the_tools_channel_and_sends_their_results_back()
-> Result<(), String> {
    let name = first_tool_name()?;
    let mcp = first_mcp_name()?;
    let n2 = name.clone();
    let (http, log) = fake(move |n| {
        (
            200,
            if n == 1 {
                openai_call_of(&n2, 2)
            } else {
                openai_text("done")
            },
        )
    });
    let (ctx, names) = host("ok");
    let mut b = ApiBackend::start(cfg(Protocol::OpenAi), ctx, http)?;
    say(&mut b, "look");
    run_until(&mut b, ended)?;
    assert_eq!(*lock(&names), vec![mcp.clone(), mcp]);
    let sent = posts(&log);
    assert_eq!(sent.len(), 2);
    let tool_msgs = sent[1]["messages"]
        .as_array()
        .ok_or("no messages")?
        .iter()
        .filter(|m| m["role"] == "tool")
        .count();
    assert_eq!(tool_msgs, 2);
    assert_eq!(sent[0]["messages"][0]["role"], "system");
    Ok(())
}

#[test]
fn an_anthropic_reply_sends_all_tool_results_in_one_user_message() -> Result<(), String> {
    let name = first_tool_name()?;
    let n2 = name.clone();
    let (http, log) = fake(move |n| {
        if n == 1 {
            (
                200,
                json!({"content": [
                {"type": "tool_use", "id": "a", "name": n2, "input": {}},
                {"type": "tool_use", "id": "b", "name": n2, "input": {}}]}),
            )
        } else {
            (200, json!({"content": [{"type": "text", "text": "done"}]}))
        }
    });
    let (ctx, _) = host("ok");
    let mut b = ApiBackend::start(cfg(Protocol::Anthropic), ctx, http)?;
    say(&mut b, "look");
    run_until(&mut b, ended)?;
    let sent = posts(&log);
    let results = sent[1]["messages"]
        .as_array()
        .ok_or("no messages")?
        .iter()
        .filter(|m| m["role"] == "user" && m["content"].is_array())
        .count();
    assert_eq!(results, 1);
    assert_eq!(
        sent[1]["messages"][2]["content"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        sent[0]["system"].as_str().map(|s| s.contains("PCBCraft")),
        Some(true)
    );
    let versioned = lock(&log).iter().filter(|r| r.method == "POST").all(|r| {
        r.headers
            .iter()
            .any(|(k, v)| k == "anthropic-version" && v == "2023-06-01")
    });
    assert!(versioned);
    Ok(())
}

#[test]
fn the_turn_stops_after_twenty_four_requests_and_tells_the_model() -> Result<(), String> {
    let name = first_tool_name()?;
    let (http, log) = fake(move |_| (200, openai_call_of(&name, 1)));
    let (ctx, _) = host("ok");
    let mut b = ApiBackend::start(cfg(Protocol::OpenAi), ctx, http)?;
    say(&mut b, "loop");
    let events = run_until(&mut b, ended)?;
    assert_eq!(posts(&log).len(), 24);
    assert!(
        matches!(events.last(), Some(Event::TurnEnded { reason }) if reason == "max_turn_requests")
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Note(n) if n == "request 24 of 24"))
    );
    // The next message carries the stop note, so the model reports instead of carrying on.
    say(&mut b, "again");
    let end = Instant::now() + Duration::from_secs(10);
    while posts(&log).len() < 25 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    let sent = posts(&log);
    let last = sent.last().ok_or("no request")?.to_string();
    assert!(last.contains("Stop calling tools now"));
    Ok(())
}

#[test]
fn a_tool_result_longer_than_the_cap_is_clipped_with_a_note() {
    let long = "x".repeat(MAX_TOOL_TEXT + 500);
    let (text, failed) =
        tool_text(&json!({"content": [{"type": "text", "text": long}], "isError": false}));
    assert!(!failed);
    assert!(text.len() < MAX_TOOL_TEXT + 20);
    assert!(text.ends_with("(cut)"));
}

#[test]
fn cancel_between_requests_ends_the_turn_with_reason_cancelled() -> Result<(), String> {
    let name = first_tool_name()?;
    let (http, log) = fake(move |_| (200, openai_call_of(&name, 1)));
    let (tx, rx) = channel::<ToolRequest>();
    let (seen_tx, seen_rx) = channel::<()>();
    std::thread::spawn(move || {
        for r in rx {
            let _ = seen_tx.send(());
            std::thread::sleep(Duration::from_millis(150));
            let _ = r
                .reply
                .send(json!({"content": [{"type": "text", "text": "late"}], "isError": false}));
        }
    });
    let ctx = StartContext {
        work_dir: std::path::PathBuf::new(),
        tools: Some(tx),
    };
    let mut b = ApiBackend::start(cfg(Protocol::OpenAi), ctx, http)?;
    say(&mut b, "go");
    seen_rx
        .recv_timeout(Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    b.cancel();
    let events = run_until(&mut b, ended)?;
    assert!(matches!(events.last(), Some(Event::TurnEnded { reason }) if reason == "cancelled"));
    assert_eq!(posts(&log).len(), 1);
    Ok(())
}

#[test]
fn a_401_reply_becomes_a_plain_sentence_without_the_key_in_it() -> Result<(), String> {
    let (http, _) = fake(|_| {
        (
            401,
            json!({"error": {"message": "Incorrect API key provided"}}),
        )
    });
    let (ctx, _) = host("ok");
    let mut c = cfg(Protocol::OpenAi);
    c.key = Some("sk-secret-123".into());
    let mut b = ApiBackend::start(c, ctx, http)?;
    say(&mut b, "hi");
    let events = run_until(&mut b, ended)?;
    let err = events
        .iter()
        .find_map(|e| {
            if let Event::Error(t) = e {
                Some(t.clone())
            } else {
                None
            }
        })
        .ok_or("no error")?;
    assert!(err.starts_with("HTTP 401"));
    assert!(err.contains("Incorrect API key provided"));
    assert!(!err.contains("sk-secret-123"));
    let body = http_error(401, &json!({"error": {"message": "bad"}}));
    assert!(!body.contains("sk-"));
    Ok(())
}

#[test]
fn a_key_never_goes_to_a_plain_http_address_off_this_computer() {
    assert!(key_safe_for("https://api.example.com/v1"));
    assert!(key_safe_for("http://localhost:11434/v1"));
    assert!(key_safe_for("http://127.0.0.1:8080"));
    assert!(!key_safe_for("http://192.168.1.5/v1"));
    assert!(!key_safe_for("http://localhost.evil.example/v1"));
    assert!(!key_safe_for("http://localhost@evil.example/v1"));
}

#[test]
fn the_tool_list_maps_every_registry_command_to_a_function_tool_with_its_schema()
-> Result<(), String> {
    let list = pcbcraft_mcp::tool_list();
    let source = list["tools"].as_array().ok_or("no tools")?;
    let open = openai_tools(&list);
    let anth = anthropic_tools(&list);
    assert_eq!(open.len(), source.len().min(128));
    assert_eq!(anth.len(), open.len());
    for (i, t) in open.iter().enumerate() {
        assert_eq!(t["type"], "function");
        assert_eq!(
            t["function"]["name"].as_str(),
            source[i]["name"].as_str().map(to_wire).as_deref()
        );
        assert_eq!(t["function"]["parameters"], source[i]["inputSchema"]);
        assert_eq!(anth[i]["input_schema"], source[i]["inputSchema"]);
    }
    Ok(())
}

#[test]
fn model_list_reads_the_data_ids() -> Result<(), String> {
    let (http, _) = fake(|_| (200, Value::Null));
    let http2: HttpFn = Arc::new(move |r: &HttpRequest| {
        assert_eq!(r.url, "http://h/v1/models");
        http(r)?;
        Ok((
            200,
            br#"{"data":[{"id":"a"},{"id":"b"},{"nope":1}]}"#.to_vec(),
        ))
    });
    let models = model_list(&http2, "http://h/v1", None)?;
    assert_eq!(
        models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    Ok(())
}

#[test]
fn test_anthropic_endpoint_returns_the_model_text_on_200_and_the_error_sentence_otherwise()
-> Result<(), String> {
    let ok: HttpFn = Arc::new(|r: &HttpRequest| {
        assert_eq!(r.url, "https://h/v1/messages");
        Ok((
            200,
            br#"{"content":[{"type":"text","text":"Hi there"}]}"#.to_vec(),
        ))
    });
    assert_eq!(
        test_anthropic_endpoint(&ok, "https://h", None, "m")?,
        "Hi there"
    );
    let bad: HttpFn = Arc::new(|_| Ok((404, br#"{"error":{"message":"no such model"}}"#.to_vec())));
    let err = test_anthropic_endpoint(&bad, "https://h", None, "m")
        .err()
        .ok_or("expected an error")?;
    assert!(err.starts_with("HTTP 404"));
    assert!(err.contains("no such model"));
    Ok(())
}

#[test]
fn a_302_reply_is_an_error_and_no_second_request_follows() -> Result<(), String> {
    let (http, log) = fake(|_| (302, json!({})));
    let (ctx, _) = host("ok");
    let mut b = ApiBackend::start(cfg(Protocol::OpenAi), ctx, http)?;
    say(&mut b, "hi");
    let events = run_until(&mut b, ended)?;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Error(t) if t.starts_with("HTTP 302")))
    );
    assert_eq!(posts(&log).len(), 1);
    Ok(())
}

fn every_name_in(body: &Value, protocol: Protocol) -> Vec<String> {
    let tools = body["tools"].as_array().cloned().unwrap_or_default();
    tools
        .iter()
        .filter_map(|t| match protocol {
            Protocol::OpenAi => t["function"]["name"].as_str(),
            Protocol::Anthropic => t["name"].as_str(),
        })
        .map(str::to_string)
        .collect()
}

fn is_hosted_legal(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[test]
fn every_outgoing_tool_name_matches_the_hosted_name_pattern() -> Result<(), String> {
    for protocol in [Protocol::OpenAi, Protocol::Anthropic] {
        let (http, log) = fake(move |_| match protocol {
            Protocol::OpenAi => (200, openai_text("hi")),
            Protocol::Anthropic => (200, json!({"content": [{"type": "text", "text": "hi"}]})),
        });
        let (ctx, _) = host("ok");
        let mut b = ApiBackend::start(cfg(protocol), ctx, http)?;
        say(&mut b, "hello");
        run_until(&mut b, ended)?;
        let sent = posts(&log);
        let names = every_name_in(sent.first().ok_or("no request")?, protocol);
        assert!(!names.is_empty());
        assert!(
            names.iter().any(|n| n.contains("__")),
            "no dotted tool seen"
        );
        if let Some(bad) = names.iter().find(|n| !is_hosted_legal(n)) {
            return Err(format!("{protocol:?} sent the illegal name {bad}"));
        }
    }
    Ok(())
}

#[test]
fn two_different_mcp_tool_names_never_share_a_wire_name() -> Result<(), String> {
    ToolNames::from_list(&pcbcraft_mcp::tool_list())?;
    let clash = json!({"tools": [{"name": "a.b"}, {"name": "a__b"}]});
    let err = ToolNames::from_list(&clash)
        .err()
        .ok_or("no collision found")?;
    assert!(err.contains("a.b") && err.contains("a__b"));
    Ok(())
}

#[test]
fn a_reply_that_calls_the_wire_name_runs_the_dotted_tool() -> Result<(), String> {
    for protocol in [Protocol::OpenAi, Protocol::Anthropic] {
        let wire = to_wire("library.search");
        assert_ne!(wire, "library.search");
        let (http, _) = fake(move |n| match (n, protocol) {
            (1, Protocol::OpenAi) => (200, openai_call_of(&wire, 1)),
            (1, Protocol::Anthropic) => (
                200,
                json!({"content": [{"type": "tool_use", "id": "a", "name": wire, "input": {}}]}),
            ),
            (_, Protocol::OpenAi) => (200, openai_text("done")),
            (_, Protocol::Anthropic) => {
                (200, json!({"content": [{"type": "text", "text": "done"}]}))
            }
        });
        let (ctx, names) = host("ok");
        let mut b = ApiBackend::start(cfg(protocol), ctx, http)?;
        say(&mut b, "find");
        let events = run_until(&mut b, ended)?;
        assert_eq!(*lock(&names), vec!["library.search".to_string()]);
        let titled = events
            .iter()
            .any(|e| matches!(e, Event::Tool { title: Some(t), .. } if t == "library.search"));
        assert!(titled, "the UI should show the MCP name");
    }
    Ok(())
}

#[test]
fn the_assistant_message_in_history_keeps_the_wire_name() -> Result<(), String> {
    let wire = to_wire("library.search");
    let used = wire.clone();
    let (http, log) = fake(move |n| {
        if n == 1 {
            (200, openai_call_of(&used, 1))
        } else {
            (200, openai_text("done"))
        }
    });
    let (ctx, _) = host("ok");
    let mut b = ApiBackend::start(cfg(Protocol::OpenAi), ctx, http)?;
    say(&mut b, "find");
    run_until(&mut b, ended)?;
    let sent = posts(&log);
    let second = sent.get(1).ok_or("no second request")?;
    let carried = second["messages"]
        .as_array()
        .ok_or("no messages")?
        .iter()
        .find(|m| m["role"] == "assistant")
        .and_then(|m| m["tool_calls"][0]["function"]["name"].as_str());
    assert_eq!(carried, Some(wire.as_str()));
    Ok(())
}

#[test]
fn a_model_that_invents_a_name_gets_unknown_tool() -> Result<(), String> {
    let (http, _) = fake(|n| {
        if n == 1 {
            (200, openai_call_of("library__does_not_exist", 1))
        } else {
            (200, openai_text("done"))
        }
    });
    let (ctx, names) = host("ok");
    let mut b = ApiBackend::start(cfg(Protocol::OpenAi), ctx, http)?;
    say(&mut b, "go");
    let events = run_until(&mut b, ended)?;
    assert!(lock(&names).is_empty());
    let failed = events.iter().any(|e| {
        matches!(e, Event::Tool { status: Some(ToolStatus::Failed), output: Some(o), .. }
            if o.starts_with("unknown tool"))
    });
    assert!(failed);
    Ok(())
}

#[test]
fn to_wire_keeps_legal_names_and_clips_long_ones_the_same_way_each_time() {
    assert_eq!(to_wire("batch"), "batch");
    assert_eq!(to_wire("a-b_c9"), "a-b_c9");
    assert_eq!(to_wire("library.save"), "library__save");
    assert_eq!(to_wire("a b"), "a_b");
    let long = format!("{}.tail", "x".repeat(80));
    let once = to_wire(&long);
    assert_eq!(once.len(), 64);
    assert_eq!(once, to_wire(&long));
    assert!(is_hosted_legal(&once));
    let other = format!("{}.tail2", "x".repeat(80));
    assert_ne!(once, to_wire(&other));
}
