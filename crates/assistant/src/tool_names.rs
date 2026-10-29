//! Wire-safe tool names for the hosted model endpoints.
//!
//! Context: MCP tool names are namespaced with dots (`library.save`), but the OpenAI function name
//! pattern (`^[a-zA-Z0-9_-]{1,64}$`) and the Anthropic one (`^[a-zA-Z0-9_-]{1,128}$`) exclude the dot.
//! The map lives at the API edge: [`to_wire`] makes a name legal on the way out and [`ToolNames`]
//! turns the name the model used back into the MCP name on the way in.
//!
//! Local rules: inbound lookup is by table, never by string replacement, so a name the model made up
//! is unknown. Two MCP names that share a wire name are an error, never a silent merge.

use serde_json::Value;
use std::collections::BTreeMap;

/// Longest wire name; the stricter of the two hosted patterns.
const WIRE_MAX: usize = 64;
/// Characters kept from a name that is too long, before `_` and four hex digits.
const CLIPPED_KEEP: usize = WIRE_MAX - 5;

/// An MCP tool name as a legal wire name.
///
/// Every `.` becomes `__`, any other character outside `[A-Za-z0-9_-]` becomes `_`, and a result
/// over 64 characters is cut to 59 plus `_` and four hex digits of an FNV-1a hash of the original
/// name, so the same name always gives the same answer and the 64 limit always holds.
///
/// # Rules
/// - Legal names without a dot come back unchanged.
///
/// # Dependencies
/// - `crates/assistant/src/api.rs` writes it into the tool lists.
///
/// # Do not
/// - Use this to go back to the MCP name; [`ToolNames::mcp_name`] does that.
pub fn to_wire(name: &str) -> String {
    let mut wire = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '.' => wire.push_str("__"),
            c if c.is_ascii_alphanumeric() || c == '_' || c == '-' => wire.push(c),
            _ => wire.push('_'),
        }
    }
    if wire.len() <= WIRE_MAX {
        return wire;
    }
    let kept: String = wire.chars().take(CLIPPED_KEEP).collect();
    format!("{kept}_{:04x}", fnv1a(name) & 0xffff)
}

/// FNV-1a over the bytes of `text`, folded to 32 bits.
fn fnv1a(text: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for b in text.bytes() {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// The table from the wire name the model uses to the MCP name the host runs.
///
/// # Dependencies
/// - `crates/assistant/src/api.rs` builds it in `ApiBackend::start` and reads it in `Worker::turn`.
///
/// # Do not
/// - Add names by hand; the table comes from the tool list only.
#[derive(Debug, Default)]
pub struct ToolNames {
    to_mcp: BTreeMap<String, String>,
}

impl ToolNames {
    /// The table for a tool list (MCP `tools/list` shape, hidden tools left out).
    ///
    /// Errors with a plain sentence naming both tools when two different MCP names share a wire name.
    pub fn from_list(list: &Value) -> Result<ToolNames, String> {
        let mut to_mcp: BTreeMap<String, String> = BTreeMap::new();
        for (name, _) in crate::api::tool_entries(list) {
            let wire = to_wire(name);
            if let Some(other) = to_mcp.get(&wire) {
                if other != name {
                    return Err(format!(
                        "The tools `{other}` and `{name}` would share the name `{wire}` on the wire."
                    ));
                }
                continue;
            }
            to_mcp.insert(wire, name.to_string());
        }
        Ok(ToolNames { to_mcp })
    }

    /// The MCP name for a wire name, or `None` for a name the model made up.
    pub fn mcp_name(&self, wire: &str) -> Option<&str> {
        self.to_mcp.get(wire).map(String::as_str)
    }
}
