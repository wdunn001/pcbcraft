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
use crate::http::{bind_http, serve_listener, start_http};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};

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
    assert!(!init.headers.to_ascii_lowercase().contains("mcp-session-id"));
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
fn get_and_delete_get_405() -> Result<(), String> {
    let address = start()?;
    for method in ["GET", "DELETE", "PUT"] {
        let reply = send_raw(
            address,
            &format!("{method} /mcp HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {TOKEN}\r\n\r\n"),
        )?;
        assert_eq!(reply.status, 405, "{method}");
        assert!(reply.headers.contains("Allow: POST"));
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
        &format!("POST /mcp HTTP/1.1\r\nHost: x\r\n{auth}Content-Length: 2\r\n\r\n{{}}"),
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
