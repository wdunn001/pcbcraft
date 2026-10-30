//! End-to-end: the real `ApiBackend` against a loopback fake Anthropic Messages server.

mod support;

use pcbcraft_assistant::{
    ApiBackend, ApiConfig, ChatBackend, Event, HttpRequest, PromptPart, Protocol, StartContext,
    ToolRequest, plain_http, plain_request,
};
use serde_json::json;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use support::{FakeServer, Reply, assert_request_shape, first_tool_name};

fn cfg(url: &str, key: Option<&str>) -> ApiConfig {
    ApiConfig {
        protocol: Protocol::Anthropic,
        base_url: url.into(),
        model: "m".into(),
        key: key.map(str::to_string),
        system_note: String::new(),
    }
}

fn host() -> StartContext {
    let (tx, rx) = channel::<ToolRequest>();
    std::thread::spawn(move || {
        for r in rx {
            let _ = r
                .reply
                .send(json!({"content": [{"type": "text", "text": "ok"}], "isError": false}));
        }
    });
    StartContext {
        work_dir: std::path::PathBuf::new(),
        tools: Some(tx),
    }
}

fn run_until(b: &mut ApiBackend, done: impl Fn(&Event) -> bool) -> Result<Vec<Event>, String> {
    let end = Instant::now() + Duration::from_secs(15);
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

fn start(srv: &FakeServer, key: Option<&str>, ctx: StartContext) -> Result<ApiBackend, String> {
    let mut b = ApiBackend::start(cfg(&srv.url, key), ctx, plain_http())?;
    b.prompt(&[PromptPart::Text("go".into())]);
    Ok(b)
}

#[test]
fn a_two_call_tool_loop_runs_over_a_socket_and_sends_tool_use_ids_back() -> Result<(), String> {
    let name = first_tool_name()?;
    let srv = FakeServer::start(vec![
        Reply::anthropic_calls(&name, 1),
        Reply::anthropic_text("done"),
    ])?;
    let mut b = start(&srv, None, host())?;
    run_until(&mut b, ended)?;
    let posts = srv.posts();
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0].path, "/v1/messages");
    assert!(posts[0].body.get("system").is_some());
    assert!(
        posts[0]
            .headers
            .iter()
            .any(|(k, _)| k == "anthropic-version")
    );
    assert!(
        posts[0].body["tools"]
            .as_array()
            .is_some_and(|t| !t.is_empty())
    );
    let msgs = posts[1].body["messages"].as_array().ok_or("no messages")?;
    let tool_user = msgs
        .iter()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .ok_or("no user message with tool results")?;
    let blocks = tool_user["content"].as_array().ok_or("no content blocks")?;
    assert_eq!(blocks[0]["type"], "tool_result");
    assert_eq!(blocks[0]["tool_use_id"], "c0");
    for p in &posts {
        assert_request_shape(p)?;
    }
    Ok(())
}

#[test]
fn the_turn_stops_at_twenty_four_requests_over_a_socket() -> Result<(), String> {
    let name = first_tool_name()?;
    let srv = FakeServer::start(vec![Reply::anthropic_calls(&name, 1)])?;
    let mut b = start(&srv, None, host())?;
    let events = run_until(&mut b, ended)?;
    assert_eq!(srv.posts().len(), 24);
    assert!(
        matches!(events.last(), Some(Event::TurnEnded { reason }) if reason == "max_turn_requests")
    );
    Ok(())
}

#[test]
fn cancel_while_the_server_is_slow_ends_the_turn_as_cancelled() -> Result<(), String> {
    let srv = FakeServer::start(vec![Reply::anthropic_text("late").after(3000)])?;
    let mut b = start(&srv, None, host())?;
    let end = Instant::now() + Duration::from_secs(10);
    while srv.posts().is_empty() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    let t = Instant::now();
    b.cancel();
    let events = run_until(&mut b, ended)?;
    assert!(t.elapsed() < Duration::from_secs(2));
    assert!(matches!(events.last(), Some(Event::TurnEnded { reason }) if reason == "cancelled"));
    Ok(())
}

#[test]
fn a_401_is_a_plain_sentence_and_the_key_stays_out_of_it() -> Result<(), String> {
    let srv = FakeServer::start(vec![Reply::json(
        401,
        json!({"error": {"message": "invalid x-api-key"}}),
    )])?;
    let mut b = start(&srv, Some("sk-secret-123"), host())?;
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
    assert!(err.starts_with("HTTP 401"), "{err}");
    assert!(!err.contains("sk-secret-123"));
    let sent = srv.posts();
    assert!(
        sent[0]
            .headers
            .iter()
            .any(|(k, v)| k == "x-api-key" && v == "sk-secret-123")
    );
    assert!(!format!("{events:?}").contains("sk-secret-123"));
    Ok(())
}

#[test]
fn a_reply_that_is_not_json_ends_the_turn_without_a_crash() -> Result<(), String> {
    let srv = FakeServer::start(vec![Reply {
        status: 200,
        body: "<html>oops".into(),
        delay_ms: 0,
        declared_len: None,
    }])?;
    let mut b = start(&srv, None, host())?;
    let events = run_until(&mut b, ended)?;
    assert!(events.iter().any(|e| matches!(e, Event::Error(_))) || events.len() > 1);
    assert_eq!(srv.posts().len(), 1);
    Ok(())
}

#[test]
fn a_slow_server_hits_the_request_timeout() -> Result<(), String> {
    let srv = FakeServer::start(vec![Reply::anthropic_text("late").after(4000)])?;
    let req = HttpRequest {
        method: "POST",
        url: format!("{}/messages", srv.url),
        headers: vec![],
        body: Some(b"{}".to_vec()),
        timeout_s: 1,
    };
    let t = Instant::now();
    let err = plain_request(&req).err().ok_or("expected a timeout")?;
    assert!(err.contains("timed out"), "{err}");
    assert!(t.elapsed() < Duration::from_secs(3));
    Ok(())
}

#[test]
fn a_429_is_a_plain_busy_sentence() -> Result<(), String> {
    let srv = FakeServer::start(vec![Reply::json(
        429,
        json!({"error": {"message": "Rate limit"}}),
    )])?;
    let mut b = start(&srv, Some("sk-secret-429"), host())?;
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
    assert!(err.contains("HTTP 429"), "{err}");
    assert!(err.contains("the endpoint is busy"), "{err}");
    assert!(!err.contains("sk-secret-429"));
    Ok(())
}

#[test]
fn a_500_is_a_plain_endpoint_error_sentence() -> Result<(), String> {
    let srv = FakeServer::start(vec![Reply::json(
        500,
        json!({"error": {"message": "Internal"}}),
    )])?;
    let mut b = start(&srv, Some("sk-secret-500"), host())?;
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
    assert!(err.contains("HTTP 500"), "{err}");
    assert!(err.contains("the endpoint had an error"), "{err}");
    assert!(!err.contains("sk-secret-500"));
    Ok(())
}

#[test]
fn a_reply_cut_off_mid_body_ends_the_turn_with_a_plain_error() -> Result<(), String> {
    let full = json!({"content": [{"type": "text", "text": "half"}], "stop_reason": "end_turn"})
        .to_string();
    let srv = FakeServer::start(vec![Reply::cut(&full, full.len() / 2)])?;
    let mut b = start(&srv, None, host())?;
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
    assert!(
        err.contains("cut off") || err.contains("not valid JSON"),
        "{err}"
    );
    assert!(matches!(events.last(), Some(Event::TurnEnded { reason }) if reason == "error"));
    Ok(())
}
