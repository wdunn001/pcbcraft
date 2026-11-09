//! Tests for the Streamable HTTP transport, with a plain `TcpStream` as the client.
//!
//! Local rules: every test binds `127.0.0.1:0` and runs the accept loop on its own thread;
//! the thread ends with the test process. Requests are written by hand so header
//! handling is tested as a client would see it.
//!
//! # Dependencies
//! `crates/mcp/src/http.rs`, `crates/mcp/src/lib.rs` (`DocumentHub`).
//!
//! # Do not
//! Do not use `unwrap`, `expect` or `panic!`; return `Result<(), String>`.
use super::*;
use crate::http::{
    HttpOptions, Server, bind_http, bind_http_with, serve_listener, serve_server, start_http,
};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;

const TOKEN: &str = "test-token";

struct Reply {
    status: u16,
    headers: String,
    body: String,
}

fn start() -> Result<SocketAddr, String> {
    let hub = DocumentHub::new(Session::sample().map_err(|e| e.to_string())?, None)?;
    let listener = bind_http("127.0.0.1:0")?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    std::thread::spawn(move || serve_listener(&hub, &listener, TOKEN));
    Ok(address)
}

fn send_raw(address: SocketAddr, text: &str) -> Result<Reply, String> {
    let mut stream = TcpStream::connect(address).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    stream
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw).map_err(|e| e.to_string())?;
    let (head, body) = raw.split_once("\r\n\r\n").ok_or("no header end")?;
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or("no status")?;
    Ok(Reply {
        status,
        headers: head.to_string(),
        body: body.to_string(),
    })
}

/// A POST with the given extra header lines (each ending in CRLF) and body.
fn post(address: SocketAddr, extra: &str, body: &str) -> Result<Reply, String> {
    send_raw(
        address,
        &format!(
            "POST /mcp HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\n{extra}Content-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    )
}

fn authed(address: SocketAddr, body: &Value) -> Result<Reply, String> {
    post(
        address,
        &format!("Authorization: Bearer {TOKEN}\r\n"),
        &body.to_string(),
    )
}

#[test]
fn a_test_client_initializes_lists_tools_and_calls_inspect_design_over_http() -> Result<(), String>
{
    let address = start()?;
    let init = authed(
        address,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}),
    )?;
    assert_eq!(init.status, 200, "{}", init.body);
    assert!(init.headers.contains("Content-Type: application/json"));
    assert!(init.headers.to_ascii_lowercase().contains("mcp-session-id"));
    let init: Value = serde_json::from_str(&init.body).map_err(|e| e.to_string())?;
    assert_eq!(init["result"]["serverInfo"]["name"], "pcbcraft");

    let tools = authed(
        address,
        &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )?;
    let tools: Value = serde_json::from_str(&tools.body).map_err(|e| e.to_string())?;
    assert!(
        tools["result"]["tools"]
            .as_array()
            .is_some_and(|t| t.len() > 20)
    );

    let call = authed(
        address,
        &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"inspect_design","arguments":{}}}),
    )?;
    let call: Value = serde_json::from_str(&call.body).map_err(|e| e.to_string())?;
    assert_eq!(call["result"]["isError"], false, "{call}");
    assert!(call["result"]["structuredContent"]["revision"].is_number());
    Ok(())
}

#[test]
fn a_http_client_lists_tools_resources_and_prompts() -> Result<(), String> {
    let address = start()?;
    for (method, key) in [
        ("tools/list", "tools"),
        ("resources/list", "resources"),
        ("prompts/list", "prompts"),
    ] {
        let reply = authed(address, &json!({"jsonrpc":"2.0","id":1,"method":method}))?;
        assert_eq!(reply.status, 200, "{method}");
        let reply: Value = serde_json::from_str(&reply.body).map_err(|e| e.to_string())?;
        assert!(
            reply["result"][key]
                .as_array()
                .is_some_and(|l| !l.is_empty()),
            "{method}"
        );
    }
    Ok(())
}

#[test]
fn a_request_without_the_token_gets_401() -> Result<(), String> {
    let address = start()?;
    let body = json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string();
    let none = post(address, "", &body)?;
    assert_eq!(none.status, 401);
    assert!(none.headers.contains("WWW-Authenticate: Bearer"));
    let wrong = post(address, "Authorization: Bearer nope\r\n", &body)?;
    assert_eq!(wrong.status, 401);
    let other_scheme = post(address, &format!("Authorization: Basic {TOKEN}\r\n"), &body)?;
    assert_eq!(other_scheme.status, 401);
    Ok(())
}

#[test]
fn a_foreign_origin_gets_403_and_an_absent_origin_is_allowed() -> Result<(), String> {
    let address = start()?;
    let body = json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string();
    let auth = format!("Authorization: Bearer {TOKEN}\r\n");
    for origin in [
        "https://example.com",
        "http://localhost.evil.test",
        "http://127.0.0.1.evil.test:80",
        "http://127.0.0.1:80x",
        "null",
    ] {
        let reply = post(address, &format!("{auth}Origin: {origin}\r\n"), &body)?;
        assert_eq!(reply.status, 403, "{origin}");
    }
    for origin in [
        "http://127.0.0.1:3000",
        "http://localhost:8080",
        "http://localhost",
    ] {
        let reply = post(address, &format!("{auth}Origin: {origin}\r\n"), &body)?;
        assert_eq!(reply.status, 200, "{origin}");
    }
    assert_eq!(post(address, &auth, &body)?.status, 200, "absent Origin");
    Ok(())
}

#[test]
fn an_unsupported_protocol_version_header_gets_400() -> Result<(), String> {
    let address = start()?;
    let body = json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string();
    let auth = format!("Authorization: Bearer {TOKEN}\r\n");
    let bad = post(
        address,
        &format!("{auth}MCP-Protocol-Version: 1999-01-01\r\n"),
        &body,
    )?;
    assert_eq!(bad.status, 400);
    let good = post(
        address,
        &format!("{auth}MCP-Protocol-Version: 2025-06-18\r\n"),
        &body,
    )?;
    assert_eq!(good.status, 200);
    Ok(())
}

#[test]
fn get_and_put_get_405() -> Result<(), String> {
    let address = start()?;
    for method in ["GET", "PUT"] {
        let reply = send_raw(
            address,
            &format!("{method} /mcp HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {TOKEN}\r\n\r\n"),
        )?;
        assert_eq!(reply.status, 405, "{method}");
        assert!(reply.headers.contains("Allow: POST, DELETE"));
    }
    let other_path = send_raw(address, "GET /elsewhere HTTP/1.1\r\nHost: x\r\n\r\n")?;
    assert_eq!(other_path.status, 404);
    Ok(())
}

#[test]
fn a_notification_gets_202_with_no_body() -> Result<(), String> {
    let address = start()?;
    let reply = authed(
        address,
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )?;
    assert_eq!(reply.status, 202);
    assert!(reply.body.is_empty());
    let response = authed(address, &json!({"jsonrpc":"2.0","id":9,"result":{}}))?;
    assert_eq!(response.status, 202, "a client response is accepted too");
    Ok(())
}

#[test]
fn a_body_over_one_mebibyte_gets_413() -> Result<(), String> {
    let address = start()?;
    let length = MAX_MESSAGE + 1;
    let reply = send_raw(
        address,
        &format!(
            "POST /mcp HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {TOKEN}\r\nAccept: application/json\r\nContent-Length: {length}\r\n\r\n"
        ),
    )?;
    assert_eq!(reply.status, 413);
    Ok(())
}

#[test]
fn malformed_bodies_and_missing_accept_are_refused() -> Result<(), String> {
    let address = start()?;
    let auth = format!("Authorization: Bearer {TOKEN}\r\n");
    assert_eq!(post(address, &auth, "{broken")?.status, 400);
    let batch = post(
        address,
        &auth,
        "[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}]",
    )?;
    assert_eq!(batch.status, 400);
    assert!(batch.body.contains("-32600"));
    let no_accept = send_raw(
        address,
        &format!("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}Content-Length: 2\r\n\r\n{{}}"),
    )?;
    assert_eq!(no_accept.status, 406);
    Ok(())
}

#[test]
fn a_non_loopback_address_is_refused_before_binding() {
    for address in [
        "0.0.0.0:0",
        "localhost:0",
        "192.168.1.5:0",
        "[::]:0",
        "example.com:80",
    ] {
        assert!(bind_http(address).is_err(), "{address}");
    }
}

#[test]
fn the_token_is_never_written_to_stdout() -> Result<(), String> {
    // start_http takes one writer for notes and no other output handle, so stdout is out of reach.
    let mut notes = vec![];
    let (listener, token) = start_http("127.0.0.1:0", None, &mut notes)?;
    let text = String::from_utf8(notes).map_err(|e| e.to_string())?;
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(text.contains(&format!("Bearer token: {token}")));
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    assert!(text.contains(&format!("MCP endpoint: http://127.0.0.1:{port}/mcp")));

    let file = std::env::temp_dir().join(format!("pcbcraft-token-{}.txt", std::process::id()));
    let mut notes = vec![];
    let (_listener, token) = start_http("127.0.0.1:0", Some(&file), &mut notes)?;
    let text = String::from_utf8(notes).map_err(|e| e.to_string())?;
    assert!(
        !text.contains(&token),
        "the token is not printed when a file is given"
    );
    assert_eq!(
        std::fs::read_to_string(&file).map_err(|e| e.to_string())?,
        token
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let _ = std::fs::remove_file(&file);
    Ok(())
}

// ---- sessions, event streams, cancellation and the remote opt-in ----

fn initialize_body() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"1"}}})
}

fn header_value(reply: &Reply, name: &str) -> Option<String> {
    reply.headers.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

fn json_of(reply: &Reply) -> Result<Value, String> {
    serde_json::from_str(&reply.body).map_err(|e| format!("{e}: {}", reply.body))
}

/// The JSON payloads of the events in a `text/event-stream` body.
fn events_of(reply: &Reply) -> Result<Vec<Value>, String> {
    reply
        .body
        .split("\n\n")
        .filter_map(|block| {
            block
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .map(|data| serde_json::from_str(data).map_err(|e| e.to_string()))
        })
        .collect()
}

fn start_server() -> Result<(SocketAddr, Arc<Server>), String> {
    let hub = DocumentHub::new(Session::sample().map_err(|e| e.to_string())?, None)?;
    let listener = bind_http("127.0.0.1:0")?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    let server = Arc::new(Server::new(hub, HttpOptions::loopback(TOKEN)));
    let running = Arc::clone(&server);
    std::thread::spawn(move || serve_server(running, &listener));
    Ok((address, server))
}

/// A request that carries a session id and extra accept types.
fn session_post(address: SocketAddr, session: &str, body: &Value) -> Result<Reply, String> {
    post(
        address,
        &format!("Authorization: Bearer {TOKEN}\r\nMCP-Session-Id: {session}\r\n"),
        &body.to_string(),
    )
}

#[test]
fn initialize_issues_a_session_id_that_later_requests_and_delete_use() -> Result<(), String> {
    let address = start()?;
    let init = authed(address, &initialize_body())?;
    let session = header_value(&init, "MCP-Session-Id").ok_or("no session id")?;
    assert_eq!(session.len(), 64);
    let ping = json!({"jsonrpc":"2.0","id":2,"method":"ping"});
    assert_eq!(session_post(address, &session, &ping)?.status, 200);
    assert_eq!(
        session_post(address, "not-a-session", &ping)?.status,
        404,
        "an unknown id means: initialize again"
    );
    let delete = |id: &str| {
        let extra = if id.is_empty() {
            String::new()
        } else {
            format!("MCP-Session-Id: {id}\r\n")
        };
        send_raw(
            address,
            &format!(
                "DELETE /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {TOKEN}\r\n{extra}\r\n"
            ),
        )
    };
    assert_eq!(delete("")?.status, 400);
    assert_eq!(delete(&session)?.status, 200);
    assert_eq!(delete(&session)?.status, 404);
    assert_eq!(session_post(address, &session, &ping)?.status, 404);
    Ok(())
}

#[test]
fn resources_read_and_prompts_get_work_over_http() -> Result<(), String> {
    let address = start()?;
    let read = |uri: &str| -> Result<Value, String> {
        json_of(&authed(
            address,
            &json!({"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":uri}}),
        )?)
    };
    let design = read("pcbcraft://design")?;
    let text = design["result"]["contents"][0]["text"]
        .as_str()
        .ok_or("design text")?;
    let design: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    assert!(design["revision"].is_number() && design["project"].is_object());
    let summary = read("pcbcraft://summary")?;
    assert!(summary["result"]["contents"][0]["text"].is_string());
    let checks = read("pcbcraft://checks")?;
    let text = checks["result"]["contents"][0]["text"]
        .as_str()
        .ok_or("checks text")?;
    let checks: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    assert!(checks["erc"].is_array() && checks["drc"].is_array());
    let missing = read("pcbcraft://nothing")?;
    assert_eq!(missing["error"]["code"], -32002);
    assert_eq!(missing["error"]["data"]["uri"], "pcbcraft://nothing");

    for name in ["review_board", "fix_findings"] {
        let prompt = json_of(&authed(
            address,
            &json!({"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":name}}),
        )?)?;
        assert_eq!(prompt["result"]["messages"][0]["role"], "user", "{name}");
        assert!(
            prompt["result"]["messages"][0]["content"]["text"]
                .as_str()
                .is_some_and(|t| t.contains("checks"))
        );
    }
    let unknown = json_of(&authed(
        address,
        &json!({"jsonrpc":"2.0","id":3,"method":"prompts/get","params":{"name":"nope"}}),
    )?)?;
    assert_eq!(unknown["error"]["code"], -32602);
    Ok(())
}

#[test]
fn a_call_with_a_progress_token_streams_progress_then_the_result() -> Result<(), String> {
    let address = start()?;
    let body = json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"checks","arguments":{},"_meta":{"progressToken":"p1"}}});
    let reply = authed(address, &body)?;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(reply.headers.contains("Content-Type: text/event-stream"));
    let events = events_of(&reply)?;
    let progress: Vec<&Value> = events
        .iter()
        .filter(|e| e["method"] == "notifications/progress")
        .collect();
    assert!(progress.len() >= 2, "{events:?}");
    let mut last = -1.0;
    for event in &progress {
        assert_eq!(event["params"]["progressToken"], "p1");
        let value = event["params"]["progress"].as_f64().ok_or("progress")?;
        assert!(value > last, "progress must increase");
        last = value;
    }
    let result = events.last().ok_or("no events")?;
    assert_eq!(result["id"], 5);
    assert_eq!(result["result"]["isError"], false);
    assert!(result["result"]["structuredContent"]["airwires"].is_array());

    // Without a token, or without text/event-stream in Accept, the reply is plain JSON.
    let plain = authed(
        address,
        &json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"checks","arguments":{}}}),
    )?;
    assert!(plain.headers.contains("Content-Type: application/json"));
    let json_only = send_raw(
        address,
        &format!(
            "POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {TOKEN}\r\nAccept: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.to_string().len()
        ),
    )?;
    assert!(json_only.headers.contains("Content-Type: application/json"));
    Ok(())
}

/// Send a tools/call on its own thread so the test can cancel it from outside.
fn spawn_call(address: SocketAddr, body: Value) -> std::thread::JoinHandle<Result<Reply, String>> {
    std::thread::spawn(move || authed(address, &body))
}

fn wait_until(what: &str, mut ready: impl FnMut() -> bool) -> Result<(), String> {
    for _ in 0..200 {
        if ready() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Err(format!("timed out waiting for {what}"))
}

fn revision_of(address: SocketAddr) -> Result<u64, String> {
    let reply = json_of(&authed(
        address,
        &json!({"jsonrpc":"2.0","id":900,"method":"tools/call","params":{"name":"inspect_design","arguments":{}}}),
    )?)?;
    reply["result"]["structuredContent"]["revision"]
        .as_u64()
        .ok_or_else(|| "no revision".to_string())
}

#[test]
fn cancelling_a_queued_call_stops_it_before_it_runs() -> Result<(), String> {
    let (address, server) = start_server()?;
    let before = revision_of(address)?;
    let turn = server.hold_turn();
    let edit = json!({"jsonrpc":"2.0","id":77,"method":"tools/call","params":{"name":"execute","arguments":{"command":"component.add","params":{"kind":"resistor","position":{"x":30,"y":20}}}}});
    let call = spawn_call(address, edit);
    wait_until("the call to queue", || server.running_calls() == 1)?;
    let cancel = authed(
        address,
        &json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":77,"reason":"user"}}),
    )?;
    assert_eq!(cancel.status, 202);
    let reply = call.join().map_err(|_| "call thread failed")??;
    let reply = json_of(&reply)?;
    assert_eq!(reply["id"], 77);
    assert_eq!(reply["error"]["code"], -32800, "{reply}");
    drop(turn);
    wait_until("the call to leave the registry", || {
        server.running_calls() == 0
    })?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(
        revision_of(address)?,
        before,
        "the cancelled edit never ran"
    );
    Ok(())
}

#[test]
fn cancelling_a_streamed_call_ends_the_stream_with_a_cancelled_error() -> Result<(), String> {
    let (address, server) = start_server()?;
    let turn = server.hold_turn();
    let body = json!({"jsonrpc":"2.0","id":"w1","method":"tools/call","params":{"name":"report.write","arguments":{"path":"/nonexistent/never-written"},"_meta":{"progressToken":7}}});
    let call = spawn_call(address, body);
    wait_until("the call to queue", || server.running_calls() == 1)?;
    authed(
        address,
        &json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"w1"}}),
    )?;
    let reply = call.join().map_err(|_| "call thread failed")??;
    let events = events_of(&reply)?;
    let last = events.last().ok_or("no events")?;
    assert_eq!(last["error"]["code"], -32800, "{events:?}");
    assert_eq!(events[0]["params"]["progressToken"], 7);
    drop(turn);
    Ok(())
}

#[test]
fn cancelling_an_unknown_request_is_ignored_and_a_reused_id_is_refused() -> Result<(), String> {
    let (address, server) = start_server()?;
    let cancel = authed(
        address,
        &json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":12345}}),
    )?;
    assert_eq!(cancel.status, 202);
    assert_eq!(revision_of(address)?, revision_of(address)?);

    let turn = server.hold_turn();
    let call = |id: u64| json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"inspect_design","arguments":{}}});
    let first = spawn_call(address, call(40));
    wait_until("the call to queue", || server.running_calls() == 1)?;
    let again = authed(address, &call(40))?;
    assert_eq!(again.status, 400, "the id is still in use");
    drop(turn);
    let first = json_of(&first.join().map_err(|_| "call thread failed")??)?;
    assert_eq!(first["result"]["isError"], false);
    Ok(())
}

#[test]
fn a_foreign_host_header_is_refused_by_default() -> Result<(), String> {
    let address = start()?;
    let body = json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string();
    let reply = send_raw(
        address,
        &format!(
            "POST /mcp HTTP/1.1\r\nHost: rebind.example:{}\r\nAuthorization: Bearer {TOKEN}\r\nAccept: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            address.port(),
            body.len()
        ),
    )?;
    assert_eq!(reply.status, 403, "DNS rebinding sends a foreign Host");
    Ok(())
}

#[test]
fn the_default_server_refuses_a_listed_foreign_origin_and_never_binds_elsewhere()
-> Result<(), String> {
    // Default options: the origin list is empty, so nothing but loopback passes.
    let address = start()?;
    let body = json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string();
    let auth = format!("Authorization: Bearer {TOKEN}\r\n");
    for origin in [
        "https://app.example",
        "http://192.168.1.20:3000",
        "http://[::2]",
    ] {
        assert_eq!(
            post(address, &format!("{auth}Origin: {origin}\r\n"), &body)?.status,
            403,
            "{origin}"
        );
    }
    let refusal = bind_http("0.0.0.0:0")
        .err()
        .ok_or("0.0.0.0 must be refused")?;
    assert!(refusal.contains("--allow-remote"), "{refusal}");
    assert!(
        bind_http_with("localhost:0", true).is_err(),
        "names are never resolved"
    );
    Ok(())
}

#[test]
fn the_remote_opt_in_lifts_host_and_listed_origins_but_keeps_the_token() -> Result<(), String> {
    let hub = DocumentHub::new(Session::sample().map_err(|e| e.to_string())?, None)?;
    // The test binds loopback; only the policy under test is the opt-in.
    let listener = bind_http_with("127.0.0.1:0", true)?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    let options = HttpOptions {
        token: TOKEN.into(),
        allow_remote: true,
        allow_origins: vec!["https://app.example".into()],
    };
    let server = Arc::new(Server::new(hub, options));
    std::thread::spawn(move || serve_server(server, &listener));
    let body = json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string();
    let send = |extra: &str| {
        send_raw(
            address,
            &format!(
                "POST /mcp HTTP/1.1\r\nHost: files.example:7992\r\nAccept: application/json\r\n{extra}Content-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        )
    };
    let auth = format!("Authorization: Bearer {TOKEN}\r\n");
    assert_eq!(
        send(&auth)?.status,
        200,
        "foreign Host is fine after the opt-in"
    );
    assert_eq!(
        send(&format!("{auth}Origin: https://app.example\r\n"))?.status,
        200
    );
    assert_eq!(
        send(&format!("{auth}Origin: https://other.example\r\n"))?.status,
        403
    );
    assert_eq!(send("")?.status, 401, "the token is still required");
    Ok(())
}
