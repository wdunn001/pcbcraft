//! End-to-end: the real `ApiBackend` and the real plain HTTP function against a loopback
//! fake OpenAI-compatible server. No external network.

mod support;

use pcbcraft_assistant::{
    ApiBackend, ApiConfig, ChatBackend, Event, HttpRequest, PromptPart, Protocol, StartContext,
    ToolRequest, plain_http, plain_request,
};
use serde_json::json;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use support::{
    FakeServer, Reply, assert_request_shape, execute_wire_name, first_tool_name,
};

fn cfg(url: &str, key: Option<&str>) -> ApiConfig {
    ApiConfig {
        protocol: Protocol::OpenAi,
        base_url: url.into(),
        model: "m".into(),
        key: key.map(str::to_string),
        system_note: String::new(),
        retrieval: None,
    }
}

/// A host that answers every tool call with "ok".
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
    let end = pcbcraft_clock::instant_now() + Duration::from_secs(15);
    let mut all = Vec::new();
    while pcbcraft_clock::instant_now() < end {
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
fn a_two_call_tool_loop_runs_over_a_socket_and_sends_tool_ids_back() -> Result<(), String> {
    let name = first_tool_name()?;
    let srv = FakeServer::start(vec![Reply::calls(&name, 1), Reply::text("done")])?;
    let mut b = start(&srv, None, host())?;
    run_until(&mut b, ended)?;
    let posts = srv.posts();
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0].path, "/v1/chat/completions");
    assert!(
        posts[0].body["tools"]
            .as_array()
            .is_some_and(|t| !t.is_empty())
    );
    let msgs = posts[1].body["messages"].as_array().ok_or("no messages")?;
    let roles: Vec<&str> = msgs.iter().filter_map(|m| m["role"].as_str()).collect();
    assert_eq!(roles, vec!["system", "user", "assistant", "tool"]);
    assert_eq!(msgs[3]["tool_call_id"], "c0");
    for p in srv.posts() {
        assert_request_shape(p)?;
    }
    Ok(())
}

#[test]
fn the_turn_stops_at_twenty_four_requests_over_a_socket() -> Result<(), String> {
    let name = first_tool_name()?;
    let srv = FakeServer::start(vec![Reply::calls(&name, 1)])?;
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
    let srv = FakeServer::start(vec![Reply::text("late").after(3000)])?;
    let mut b = start(&srv, None, host())?;
    let end = pcbcraft_clock::instant_now() + Duration::from_secs(10);
    while srv.posts().is_empty() && pcbcraft_clock::instant_now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    let t = pcbcraft_clock::instant_now();
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
        json!({"error": {"message": "Incorrect API key provided"}}),
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
            .any(|(k, v)| k == "authorization" && v == "Bearer sk-secret-123")
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
    let srv = FakeServer::start(vec![Reply::text("late").after(4000)])?;
    let req = HttpRequest {
        method: "POST",
        url: format!("{}/chat/completions", srv.url),
        headers: vec![],
        body: Some(b"{}".to_vec()),
        timeout_s: 1,
        stream_out: None,
    };
    let t = pcbcraft_clock::instant_now();
    let err = plain_request(&req).err().ok_or("expected a timeout")?;
    assert!(err.contains("timed out"), "{err}");
    assert!(t.elapsed() < Duration::from_secs(3));
    Ok(())
}

#[test]
fn the_plain_function_refuses_https_and_does_not_follow_redirects() -> Result<(), String> {
    let mk = |url: String| HttpRequest {
        method: "GET",
        url,
        headers: vec![],
        body: None,
        timeout_s: 5,
        stream_out: None,
    };
    assert!(plain_request(&mk("https://127.0.0.1:1/".into())).is_err());
    let srv = FakeServer::start(vec![])?;
    let _ = srv;
    let redirect = FakeServer::start(vec![Reply::json(302, json!({}))])?;
    let mut r = mk(format!("{}/x", redirect.url));
    r.method = "POST";
    r.body = Some(b"{}".to_vec());
    assert_eq!(plain_request(&r)?.0, 302);
    assert_eq!(redirect.posts().len(), 1);
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
    let full = json!({"choices": [{"message": {"role": "assistant", "content": "half"}}]}).to_string();
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

#[test]
fn a_finished_turn_is_saved_and_loads_back_the_same() -> Result<(), String> {
    let srv = FakeServer::start(vec![Reply::text("saved line")])?;
    let mut b = start(&srv, None, host())?;
    let events = run_until(&mut b, ended)?;
    let mut transcript = pcbcraft_assistant::Transcript::default();
    transcript.push(pcbcraft_assistant::Item::User {
        text: "go".into(),
    });
    for e in events {
        transcript.on_event(e);
    }
    let path = std::env::temp_dir().join(format!(
        "pcbcraft-assistant-wire-{}-transcript.jsonl",
        std::process::id()
    );
    let _ = std::fs::remove_file(&path);
    pcbcraft_assistant::save_transcript(&transcript.items, &path)?;
    let (loaded, skipped) = pcbcraft_assistant::load_transcript(&path);
    assert_eq!(skipped, 0);
    assert_eq!(loaded.len(), transcript.items.len());
    assert!(
        loaded.iter().any(|i| matches!(i, pcbcraft_assistant::Item::Assistant { text } if text == "saved line"))
    );
    let _ = std::fs::remove_file(&path);
    Ok(())
}

#[test]
fn after_apply_the_design_state_is_read_back_through_shared() -> Result<(), String> {
    use pcbcraft_assistant::to_wire;
    use pcbcraft_engine::Session;
    use pcbcraft_mcp::{call_tool, shared};

    let wire = execute_wire_name();
    assert!(!wire.contains('.'));
    let args = json!({"command":"component.add","params":{"kind":"resistor","reference":"R1","value":"10k","position":[10.0,10.0]}});
    let arg_text = args.to_string();
    let first = Reply::json(
        200,
        json!({"choices": [{"message": {"role": "assistant", "content": null, "tool_calls": [
            {"id": "c0", "type": "function", "function": {"name": wire, "arguments": arg_text}}
        ]}}]}),
    );
    let srv = FakeServer::start(vec![first, Reply::text("done")])?;
    let session = shared(Session::default());
    let (tx, rx) = channel::<ToolRequest>();
    let handle = session.clone();
    std::thread::spawn(move || {
        for r in rx {
            let out = call_tool(&handle, &r.name, &r.args);
            let _ = r.reply.send(out);
        }
    });
    let ctx = StartContext {
        work_dir: std::path::PathBuf::new(),
        tools: Some(tx),
    };
    let mut b = start(&srv, None, ctx)?;
    run_until(&mut b, ended)?;
    let inspect = session
        .lock()
        .map_err(|_| "session lock".to_string())?
        .inspect();
    let refs: Vec<&str> = inspect["project"]["components"]
        .as_array()
        .ok_or("no components")?
        .iter()
        .filter_map(|c| c["reference"].as_str())
        .collect();
    assert!(refs.iter().any(|r| *r == "R1"), "{refs:?}");
    let sent_name = srv.posts()[0].body["tools"]
        .as_array()
        .and_then(|t| t.iter().find(|x| x["function"]["name"] == wire))
        .ok_or("execute not in tools")?;
    assert_eq!(sent_name["function"]["name"], to_wire("execute"));
    for p in srv.posts() {
        assert_request_shape(p)?;
    }
    Ok(())
}
