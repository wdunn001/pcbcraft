//! Loopback MCP bridge for the assistant gate: `tools/call` waits on the host.
use crate::{read_message, rpc_error, tool_list, MAX_MESSAGE};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::Sender;
use std::time::Duration;

/// One tool call the desktop assistant gate must answer.
#[derive(Debug)]
pub struct ToolRequest {
    pub name: String,
    pub args: Value,
    pub reply: Sender<Value>,
}

const UNAUTHORIZED: i64 = -32001;
const TOOL_TIMEOUT: Duration = Duration::from_secs(300);

/// Compare two secrets without stopping at the first differing byte.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for index in 0..a.len().max(b.len()) {
        diff |=
            usize::from(a.get(index).copied().unwrap_or(0) ^ b.get(index).copied().unwrap_or(0));
    }
    diff == 0
}

fn strip_token(mut request: Value) -> (Option<String>, Value) {
    let token = request
        .get("token")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(obj) = request.as_object_mut() {
        obj.remove("token");
    }
    (token, request)
}

fn handle_gated(request: Value, expected: &str, tx: &Sender<ToolRequest>) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let (token, request) = strip_token(request);
    let authorized = token.is_some_and(|t| same_secret(&t, expected));
    if !authorized {
        return Some(rpc_error(id, UNAUTHORIZED, "unauthorized"));
    }
    let method = request.get("method").and_then(Value::as_str);
    let id = match request.get("id").cloned() {
        Some(id) => id,
        None => return None,
    };
    match method {
        Some("tools/list") => Some(json!({"jsonrpc":"2.0","id":id,"result": tool_list()})),
        Some("tools/call") => {
            let params = request.get("params").cloned().unwrap_or(json!({}));
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "missing tool name"));
            };
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let (reply_tx, reply_rx) = std::sync::mpsc::channel();
            if tx
                .send(ToolRequest {
                    name: name.to_string(),
                    args,
                    reply: reply_tx,
                })
                .is_err()
            {
                return Some(rpc_error(id, -32603, "bridge closed"));
            }
            match reply_rx.recv_timeout(TOOL_TIMEOUT) {
                Ok(result) => Some(json!({"jsonrpc":"2.0","id":id,"result": result})),
                Err(_) => Some(rpc_error(id, -32603, "tool call timed out")),
            }
        }
        Some(_) => Some(rpc_error(id, -32601, "method not found")),
        None => Some(rpc_error(id, -32600, "invalid JSON-RPC request")),
    }
}

/// Bind `127.0.0.1:0` and serve gated MCP tool calls on a background thread.
pub fn start_gated_bridge(tx: Sender<ToolRequest>, token: String) -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let expected = std::sync::Arc::new(token);
    std::thread::Builder::new()
        .name("pcbcraft-gated-bridge".into())
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                if stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .is_err()
                {
                    continue;
                }
                if let Ok(reader) = stream.try_clone() {
                    let mut reader = BufReader::new(reader);
                    if let Ok(Some(line)) = read_message(&mut reader) {
                        let response = match serde_json::from_str::<Value>(&line) {
                            Ok(request) => handle_gated(request, &expected, &tx),
                            Err(_) => Some(rpc_error(Value::Null, -32700, "parse error")),
                        };
                        if let Some(response) = response {
                            let _ = serde_json::to_writer(&mut stream, &response);
                            let _ = stream.write_all(b"\n");
                        }
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(port)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Instant;

    #[test]
    fn a_prompt_round_trips_through_the_bridge_with_the_token_and_without_it_is_refused() {
        let (tx, rx) = channel::<ToolRequest>();
        let token = "abc123";
        let port = start_gated_bridge(tx, token.into()).expect("bind");
        let address = format!("127.0.0.1:{port}");
        let call = json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"tools/call",
            "params":{"name":"design.summary","arguments":{}},
            "token":"wrong"
        });
        let bad = crate::forward(&address, &call, None).expect("connect");
        assert_eq!(bad["error"]["code"], UNAUTHORIZED);

        let call = json!({
            "jsonrpc":"2.0",
            "id":2,
            "method":"tools/call",
            "params":{"name":"design.summary","arguments":{}},
            "token":token
        });
        std::thread::spawn(move || {
            let req = rx.recv_timeout(Duration::from_secs(5)).expect("tool");
            assert_eq!(req.name, "design.summary");
            let _ = req.reply.send(json!({"content":[{"type":"text","text":"ok"}],"isError":false}));
        });
        let good = crate::forward(&address, &call, Some(token)).expect("connect");
        assert_eq!(good["result"]["content"][0]["text"], "ok");
    }

    #[test]
    fn tools_list_works_with_the_token() {
        let (tx, _rx) = channel::<ToolRequest>();
        let token = "list-token";
        let port = start_gated_bridge(tx, token.into()).expect("bind");
        let address = format!("127.0.0.1:{port}");
        let list = json!({"jsonrpc":"2.0","id":1,"method":"tools/list","token":token});
        let reply = crate::forward(&address, &list, Some(token)).expect("connect");
        assert!(reply["result"]["tools"].is_array());
    }

    #[test]
    fn bridge_refuses_a_call_when_nobody_is_listening() {
        let (tx, rx) = channel::<ToolRequest>();
        let token = "lonely";
        let port = start_gated_bridge(tx, token.into()).expect("bind");
        drop(rx);
        let address = format!("127.0.0.1:{port}");
        let call = json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"tools/call",
            "params":{"name":"design.summary","arguments":{}},
            "token":token
        });
        let reply = crate::forward(&address, &call, Some(token)).expect("connect");
        assert_eq!(reply["error"]["code"], -32603);
    }

    #[test]
    fn same_secret_rejects_different_lengths() {
        assert!(!same_secret("a", "ab"));
        assert!(same_secret("xy", "xy"));
    }

    #[test]
    fn gated_bridge_starts_on_loopback() {
        let (tx, _rx) = channel::<ToolRequest>();
        let start = Instant::now();
        let port = start_gated_bridge(tx, "t".repeat(64)).expect("bind");
        assert!(port > 0);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
