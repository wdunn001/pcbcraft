//! Loopback control bridge tests for `start_bridge`, `forward`, and `tool_list` oracles.
//!
//! Local rules: bind `127.0.0.1:0`, one JSON line per TCP connection, no panics.
//! Return `Result<(), String>` instead of panicking.
//!
//! # Dependencies
//! `crates/mcp/src/lib.rs` (`start_bridge`, `forward`, `tool_list`, `handle_rpc`).
//!
//! # Do not
//! Do not bind non-loopback addresses. Do not leave listeners running after the test.
use super::*;
use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::net::{TcpListener, TcpStream};

fn free_loopback_port() -> Result<String, String> {
    let address = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| e.to_string())?
        .local_addr()
        .map_err(|e| e.to_string())?;
    Ok(address.to_string())
}

fn tool_by_name(name: &str) -> Result<Value, String> {
    tool_list()["tools"]
        .as_array()
        .ok_or("tools is an array")?
        .iter()
        .find(|t| t["name"] == name)
        .cloned()
        .ok_or_else(|| format!("no tool named {name}"))
}

#[test]
fn forward_refuses_addresses_that_are_not_literal_loopback() -> Result<(), String> {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"ping"});
    for address in [
        "192.168.1.1:7991",
        "0.0.0.0:7991",
        "example.com:80",
        "localhost:7991",
    ] {
        let err = forward(address, &request)
            .err()
            .ok_or_else(|| format!("{address} should be refused"))?;
        assert!(
            err.contains("loopback") || err.contains("127.0.0.1:7991"),
            "{address}: {err}"
        );
    }
    Ok(())
}

#[test]
fn start_bridge_refuses_addresses_that_are_not_literal_loopback() -> Result<(), String> {
    let hub = DocumentHub::new(Session::default(), None)?;
    for address in ["192.168.1.1:7991", "0.0.0.0:0", "localhost:0"] {
        assert!(
            start_bridge(hub.clone(), address).is_err(),
            "{address} should be refused"
        );
    }
    Ok(())
}

#[test]
fn start_bridge_and_forward_carry_ping_and_tool_calls_on_one_connection_each() -> Result<(), String>
{
    let hub = DocumentHub::new(Session::default(), None)?;
    let address = free_loopback_port()?;
    start_bridge(hub, &address)?;

    let ping = forward(&address, &json!({"jsonrpc":"2.0","id":7,"method":"ping"}))?;
    assert_eq!(ping["jsonrpc"], "2.0");
    assert_eq!(ping["id"], 7);
    assert_eq!(ping["result"], json!({}));

    let inspect = forward(
        &address,
        &json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"inspect_design","arguments":{}}}),
    )?;
    assert_eq!(inspect["id"], 8);
    assert_eq!(inspect["result"]["isError"], false);
    assert_eq!(inspect["result"]["structuredContent"]["revision"], 0);
    Ok(())
}

#[test]
fn the_bridge_returns_parse_error_when_the_client_sends_invalid_json() -> Result<(), String> {
    let hub = DocumentHub::new(Session::default(), None)?;
    let address = free_loopback_port()?;
    start_bridge(hub, &address)?;

    let mut stream = TcpStream::connect(&address).map_err(|e| e.to_string())?;
    stream
        .write_all(b"{not json\n")
        .map_err(|e| e.to_string())?;
    let mut reader = std::io::BufReader::new(stream);
    let line = read_message(&mut reader)?.ok_or("bridge closed without a reply")?;
    let reply: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    assert_eq!(reply["error"]["code"], -32700);
    assert_eq!(reply["id"], Value::Null);
    Ok(())
}

#[test]
fn forward_fails_when_nothing_is_listening_on_the_loopback_port() -> Result<(), String> {
    let address = free_loopback_port()?;
    assert!(
        forward(&address, &json!({"jsonrpc":"2.0","id":1,"method":"ping"}),).is_err(),
        "closed port {address}"
    );
    Ok(())
}

#[test]
fn tool_list_names_are_unique_and_include_every_engine_command_with_its_schema()
-> Result<(), String> {
    let list = tool_list()["tools"]
        .as_array()
        .ok_or("tools is an array")?
        .clone();
    assert!(
        list.len() > 50,
        "the registry lists many export and edit tools"
    );
    let mut names = HashSet::new();
    for tool in &list {
        let name = tool["name"]
            .as_str()
            .ok_or("every tool has a string name")?;
        assert!(names.insert(name.to_string()), "duplicate name {name}");
    }
    for spec in pcbcraft_engine::commands() {
        let tool = tool_by_name(spec.id)?;
        assert_eq!(tool["inputSchema"], spec.input_schema, "{} schema", spec.id);
        assert_eq!(
            tool["description"].as_str(),
            Some(spec.description),
            "{} description",
            spec.id
        );
    }
    Ok(())
}

#[test]
fn tool_list_titles_capitalize_only_the_first_character_and_replace_dots_and_underscores()
-> Result<(), String> {
    for (name, want) in [
        ("execute", "Execute"),
        ("gerber.export", "Gerber export"),
        ("library.save", "Library save"),
        ("output_job.package", "Output job package"),
        ("board_mesh", "Board mesh"),
    ] {
        assert_eq!(tool_by_name(name)?["title"].as_str(), Some(want), "{name}");
    }
    Ok(())
}

#[test]
fn tool_list_marks_each_read_only_discovery_tool_with_the_read_only_annotation()
-> Result<(), String> {
    const READ_ONLY: [&str; 15] = [
        "inspect_design",
        "list_commands",
        "checks",
        "board_mesh",
        "screenshot",
        "library.catalog",
        "library.search",
        "library.list",
        "library.inspect_symbol",
        "library.inspect_footprint",
        "library.compare",
        "library.sources",
        "design.recents",
        "design.list_open",
        "design.activate",
    ];
    for name in READ_ONLY {
        let tool = tool_by_name(name)?;
        assert_eq!(tool["annotations"]["readOnlyHint"], true, "{name}");
        assert_eq!(tool["annotations"]["destructiveHint"], false, "{name}");
        assert_eq!(tool["annotations"]["idempotentHint"], true, "{name}");
        assert_eq!(tool["annotations"]["openWorldHint"], false, "{name}");
    }
    Ok(())
}
