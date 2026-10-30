//! MCP prompts: three user-controlled templates that name tools and hold no secrets.
//!
//! Local rules: a prompt text only names tools from `tool_list` and never contains a path
//! or a credential. Argument values are inserted as plain text, clipped to 200
//! characters. `focus` must be one of a fixed set; anything else is invalid params.
//!
//! # Dependencies
//! `crates/mcp/src/lib.rs` (`handle_rpc` dispatches `prompts/*`; `tool_list` is the tool
//! registry the prompts refer to).
//!
//! # Do not
//! Do not name a tool that is not in `tool_list`. Do not put file paths in a prompt.
use serde_json::{Value, json};

const MAX_ARGUMENT_CHARS: usize = 200;
const FOCUS: [&str; 3] = ["placement", "power", "signal"];

/// A failed prompt request: always JSON-RPC -32602.
pub(crate) struct PromptError(pub String);

/// The prompts `prompts/list` returns, with their arguments.
pub(crate) fn list() -> Value {
    json!({"prompts":[
        {"name":"review_board","title":"Review the board","description":"Read the design and its checks, then review it without editing","arguments":[{"name":"focus","description":"placement, power or signal","required":false}]},
        {"name":"fix_unrouted_nets","title":"Fix unrouted nets","description":"Route the nets the checks report as unrouted, then check again","arguments":[]},
        {"name":"prepare_fabrication","title":"Prepare fabrication files","description":"Run the checks and, if they pass, write the fabrication outputs","arguments":[{"name":"fab","description":"short note on the board house or process","required":false}]}
    ]})
}

fn clip(text: &str) -> String {
    text.chars().take(MAX_ARGUMENT_CHARS).collect()
}

fn argument<'a>(arguments: &'a Value, key: &str) -> Result<Option<&'a str>, PromptError> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(PromptError(format!("{key} must be a string"))),
    }
}

/// Build the messages of one prompt.
///
/// # Rules
/// An unknown name or a `focus` outside `placement`, `power`, `signal` is an error.
///
/// # Dependencies
/// `crates/mcp/src/lib.rs` (`tool_list`, the tools named in the texts).
///
/// # Do not
/// Do not insert an argument without `clip`.
pub(crate) fn get(name: &str, arguments: &Value) -> Result<Value, PromptError> {
    let (description, text) = match name {
        "review_board" => {
            let focus = argument(arguments, "focus")?;
            if let Some(focus) = focus
                && !FOCUS.contains(&focus)
            {
                return Err(PromptError("focus must be placement, power or signal".into()));
            }
            let scope = focus.map_or_else(
                || "the whole board".to_string(),
                |f| format!("{f} only"),
            );
            (
                "Review the board without editing it",
                format!("Call inspect_design, then call checks. Review {scope}. Report findings by object id, most serious first. Do not edit the design."),
            )
        }
        "fix_unrouted_nets" => (
            "Route the unrouted nets",
            "Call checks and list the unrouted nets. Route them with execute or batch, using expected_revision from inspect_design. Then call checks again and report what is left.".to_string(),
        ),
        "prepare_fabrication" => {
            let note = argument(arguments, "fab")?.map(clip);
            let note = note.map_or_else(String::new, |n| format!(" Board house or process: {n}."));
            (
                "Check the board and write fabrication files",
                format!("Call checks. If any errors remain, stop and report them.{note} Otherwise write the outputs with gerber.export, bom.write and placement.write, and report where each file went."),
            )
        }
        _ => return Err(PromptError(format!("unknown prompt: {}", clip(name)))),
    };
    Ok(
        json!({"description":description,"messages":[{"role":"user","content":{"type":"text","text":text}}]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DocumentHub, handle_rpc};
    use pcbcraft_engine::Session;

    fn rpc(method: &str, params: Value) -> Result<Value, String> {
        let hub = DocumentHub::new(Session::default(), None)?;
        handle_rpc(
            &hub,
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}),
            None,
            None,
        )
        .ok_or_else(|| "expected a reply".to_string())
    }

    fn text_of(reply: &Value) -> String {
        reply["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn prompts_list_names_three_prompts_with_their_arguments() -> Result<(), String> {
        let reply = rpc("prompts/list", json!({}))?;
        let prompts = reply["result"]["prompts"].as_array().ok_or("prompts")?;
        let names: Vec<&str> = prompts.iter().filter_map(|p| p["name"].as_str()).collect();
        assert_eq!(
            names,
            ["review_board", "fix_unrouted_nets", "prepare_fabrication"]
        );
        assert_eq!(prompts[0]["arguments"][0]["name"], "focus");
        assert_eq!(prompts[1]["arguments"], json!([]));
        assert_eq!(prompts[2]["arguments"][0]["name"], "fab");
        Ok(())
    }

    #[test]
    fn review_board_with_focus_power_mentions_power_and_both_tools() -> Result<(), String> {
        let reply = rpc(
            "prompts/get",
            json!({"name":"review_board","arguments":{"focus":"power"}}),
        )?;
        let text = text_of(&reply);
        assert!(text.contains("power only"), "{text}");
        assert!(
            text.contains("inspect_design") && text.contains("checks"),
            "{text}"
        );
        assert_eq!(reply["result"]["messages"][0]["role"], "user");
        assert_eq!(reply["result"]["messages"][0]["content"]["type"], "text");
        assert!(reply["result"]["description"].is_string());
        Ok(())
    }

    #[test]
    fn an_unknown_prompt_or_a_bad_focus_gives_invalid_params() -> Result<(), String> {
        for params in [
            json!({"name":"no_such_prompt"}),
            json!({"name":"review_board","arguments":{"focus":"thermal"}}),
            json!({"name":"review_board","arguments":{"focus":5}}),
            json!({}),
        ] {
            let reply = rpc("prompts/get", params.clone())?;
            assert_eq!(reply["error"]["code"], -32602, "{params}");
        }
        Ok(())
    }

    #[test]
    fn every_tool_a_prompt_names_exists_in_the_tool_list() -> Result<(), String> {
        let tools = crate::tool_list();
        let names: Vec<&str> = tools["tools"]
            .as_array()
            .ok_or("tools")?
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        let mut named = 0;
        for prompt in ["review_board", "fix_unrouted_nets", "prepare_fabrication"] {
            let text = text_of(&rpc("prompts/get", json!({"name":prompt}))?);
            for word in text
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
                .map(|w| w.trim_matches('.'))
            {
                // A tool name has an underscore or a dot, or is one of these single words.
                let looks_like_tool = word.contains('_')
                    || word.contains('.')
                    || word == "checks"
                    || word == "execute"
                    || word == "batch";
                if looks_like_tool && !word.is_empty() && word != "expected_revision" {
                    assert!(
                        names.contains(&word),
                        "{prompt} names {word}, which is not a tool"
                    );
                    named += 1;
                }
            }
        }
        assert!(named >= 8, "the prompts name several tools");
        Ok(())
    }

    #[test]
    fn a_long_argument_is_clipped() -> Result<(), String> {
        let long = "x".repeat(500);
        let reply = rpc(
            "prompts/get",
            json!({"name":"prepare_fabrication","arguments":{"fab":long}}),
        )?;
        let text = text_of(&reply);
        assert!(text.contains(&"x".repeat(200)));
        assert!(!text.contains(&"x".repeat(201)));
        Ok(())
    }
}
