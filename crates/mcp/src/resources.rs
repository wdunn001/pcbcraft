//! MCP resources: the design, the command list and the checks, read without editing.
//!
//! Local rules: every read goes through `DocumentHub::call_tool`, so the active design is
//! used and nothing is edited. The scheme is `pcbcraft://`. There is no subscription and no
//! list-changed notice, because the server never pushes messages.
//!
//! # Dependencies
//! `crates/mcp/src/lib.rs` (`handle_rpc` dispatches `resources/*`),
//! `crates/mcp/src/document_hub.rs` (`DocumentHub::call_tool`).
//!
//! # Do not
//! Do not add a resource that edits the design or reads a host path. Do not return secrets.
use crate::DocumentHub;
use serde_json::{Value, json};

const PREVIEW_PREFIX: &str = "pcbcraft://design/preview/";
const VIEWS: [&str; 2] = ["schematic", "pcb"];

/// A failed read: JSON-RPC code, message and optional data.
pub(crate) struct ReadError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

fn annotations(priority: f64) -> Value {
    json!({"audience":["user","assistant"],"priority":priority})
}

/// The fixed resources `resources/list` returns.
pub(crate) fn list() -> Value {
    json!({"resources":[
        {"uri":"pcbcraft://design","name":"design","title":"Design","description":"The canonical project with stable ids, revision and history; same content as inspect_design","mimeType":"application/json","annotations":annotations(1.0)},
        {"uri":"pcbcraft://commands","name":"commands","title":"Commands","description":"Every engine command with its input schema; same content as list_commands","mimeType":"application/json","annotations":annotations(0.6)},
        {"uri":"pcbcraft://checks","name":"checks","title":"Checks","description":"The result of running the checks now; reading it edits nothing","mimeType":"application/json","annotations":annotations(0.8)}
    ]})
}

/// The resource templates `resources/templates/list` returns.
pub(crate) fn templates() -> Value {
    json!({"resourceTemplates":[
        {"uriTemplate":"pcbcraft://design/preview/{view}","name":"design_preview","title":"Design preview","description":"PNG preview of the schematic or the pcb view","mimeType":"image/png","annotations":annotations(0.4)}
    ]})
}

fn not_found(uri: &str) -> ReadError {
    ReadError {
        code: -32002,
        message: "Resource not found".into(),
        data: Some(json!({"uri":uri})),
    }
}

fn run_tool(hub: &DocumentHub, tool: &str, args: &Value) -> Result<Value, ReadError> {
    let reply = hub.call_tool(tool, args);
    if reply["isError"] == true {
        return Err(ReadError {
            code: -32603,
            message: reply["content"][0]["text"]
                .as_str()
                .unwrap_or("resource read failed")
                .to_string(),
            data: None,
        });
    }
    Ok(reply)
}

fn json_contents(uri: &str, reply: &Value) -> Value {
    json!({"contents":[{"uri":uri,"mimeType":"application/json","text":reply["structuredContent"].to_string()}]})
}

/// Read one resource by uri.
///
/// # Rules
/// Unknown uri is -32002 with the uri in `data`; a preview view other than `schematic`
/// or `pcb` is -32602.
///
/// # Dependencies
/// `crates/mcp/src/document_hub.rs` (`DocumentHub::call_tool`).
///
/// # Do not
/// Do not call a tool that changes the revision.
pub(crate) fn read(hub: &DocumentHub, uri: &str) -> Result<Value, ReadError> {
    match uri {
        "pcbcraft://design" => Ok(json_contents(
            uri,
            &run_tool(hub, "inspect_design", &json!({}))?,
        )),
        "pcbcraft://commands" => Ok(json_contents(
            uri,
            &run_tool(hub, "list_commands", &json!({}))?,
        )),
        "pcbcraft://checks" => Ok(json_contents(uri, &run_tool(hub, "checks", &json!({}))?)),
        _ => {
            let Some(view) = uri.strip_prefix(PREVIEW_PREFIX) else {
                return Err(not_found(uri));
            };
            if !VIEWS.contains(&view) {
                return Err(ReadError {
                    code: -32602,
                    message: "view must be schematic or pcb".into(),
                    data: Some(json!({"uri":uri})),
                });
            }
            let reply = run_tool(hub, "screenshot", &json!({"view":view}))?;
            let blob = reply["content"][0]["data"].as_str().unwrap_or_default();
            Ok(json!({"contents":[{"uri":uri,"mimeType":"image/png","blob":blob}]}))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle_rpc;
    use base64::Engine as _;
    use pcbcraft_engine::Session;

    fn sample_hub() -> Result<DocumentHub, String> {
        DocumentHub::new(Session::sample().map_err(|e| e.to_string())?, None)
    }

    fn rpc(hub: &DocumentHub, method: &str, params: Value) -> Result<Value, String> {
        handle_rpc(
            hub,
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}),
            None,
        )
        .ok_or_else(|| "expected a reply".to_string())
    }

    fn revision(hub: &DocumentHub) -> Value {
        hub.call_tool("inspect_design", &json!({}))["structuredContent"]["revision"].clone()
    }

    #[test]
    fn resources_list_names_the_design_commands_and_checks_resources() -> Result<(), String> {
        let hub = sample_hub()?;
        let reply = rpc(&hub, "resources/list", json!({}))?;
        let uris: Vec<&str> = reply["result"]["resources"]
            .as_array()
            .ok_or("resources is an array")?
            .iter()
            .filter_map(|r| r["uri"].as_str())
            .collect();
        assert_eq!(
            uris,
            [
                "pcbcraft://design",
                "pcbcraft://commands",
                "pcbcraft://checks"
            ]
        );
        for resource in reply["result"]["resources"]
            .as_array()
            .into_iter()
            .flatten()
        {
            for key in ["name", "title", "mimeType"] {
                assert!(resource[key].is_string(), "{resource} lacks {key}");
            }
            assert!(resource["annotations"]["audience"].is_array());
        }
        let templates = rpc(&hub, "resources/templates/list", json!({}))?;
        assert_eq!(
            templates["result"]["resourceTemplates"][0]["uriTemplate"],
            "pcbcraft://design/preview/{view}"
        );
        Ok(())
    }

    #[test]
    fn reading_the_design_resource_equals_inspect_design() -> Result<(), String> {
        let hub = sample_hub()?;
        let reply = rpc(&hub, "resources/read", json!({"uri":"pcbcraft://design"}))?;
        let content = &reply["result"]["contents"][0];
        assert_eq!(content["uri"], "pcbcraft://design");
        assert_eq!(content["mimeType"], "application/json");
        let read: Value = serde_json::from_str(content["text"].as_str().ok_or("text")?)
            .map_err(|e| e.to_string())?;
        assert_eq!(
            read,
            hub.call_tool("inspect_design", &json!({}))["structuredContent"]
        );
        let commands = rpc(&hub, "resources/read", json!({"uri":"pcbcraft://commands"}))?;
        assert!(commands["result"]["contents"][0]["text"].is_string());
        let checks = rpc(&hub, "resources/read", json!({"uri":"pcbcraft://checks"}))?;
        assert!(
            checks["result"]["contents"][0]["text"].is_string(),
            "{checks}"
        );
        Ok(())
    }

    #[test]
    fn reading_an_unknown_uri_gives_resource_not_found_with_the_uri() -> Result<(), String> {
        let hub = sample_hub()?;
        for uri in [
            "pcbcraft://nothing",
            "file:///etc/passwd",
            "pcbcraft://design/other",
        ] {
            let reply = rpc(&hub, "resources/read", json!({"uri":uri}))?;
            assert_eq!(reply["error"]["code"], -32002, "{uri}");
            assert_eq!(reply["error"]["data"]["uri"], uri);
        }
        let missing = rpc(&hub, "resources/read", json!({}))?;
        assert_eq!(missing["error"]["code"], -32602);
        Ok(())
    }

    #[test]
    fn the_preview_template_returns_a_png_blob_for_pcb_and_schematic_and_refuses_other_views()
    -> Result<(), String> {
        let hub = sample_hub()?;
        for view in ["pcb", "schematic"] {
            let uri = format!("pcbcraft://design/preview/{view}");
            let reply = rpc(&hub, "resources/read", json!({"uri":uri}))?;
            let content = &reply["result"]["contents"][0];
            assert_eq!(content["mimeType"], "image/png", "{reply}");
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(content["blob"].as_str().ok_or("blob")?)
                .map_err(|e| e.to_string())?;
            assert!(bytes.starts_with(b"\x89PNG"), "{view} is a PNG");
        }
        let bad = rpc(
            &hub,
            "resources/read",
            json!({"uri":"pcbcraft://design/preview/3d"}),
        )?;
        assert_eq!(bad["error"]["code"], -32602);
        Ok(())
    }

    #[test]
    fn reading_resources_does_not_change_the_revision() -> Result<(), String> {
        let hub = sample_hub()?;
        let before = revision(&hub);
        for uri in [
            "pcbcraft://design",
            "pcbcraft://commands",
            "pcbcraft://checks",
            "pcbcraft://design/preview/pcb",
        ] {
            rpc(&hub, "resources/read", json!({"uri":uri}))?;
        }
        assert_eq!(revision(&hub), before);
        Ok(())
    }
}
