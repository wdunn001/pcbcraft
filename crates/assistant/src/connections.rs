//! Saved model connections and the checks on their addresses.
//!
//! Context: Settings keeps a list of [`Connection`]s in the preferences file. A connection never
//! holds its key: the key lives in the system keychain under [`key_id`], and `has_key` only
//! records that one is stored.
//! Rules: adapted from SolidCraft (MIT OR Apache-2.0), commit 32558d0a,
//! `crates/ui-egui/src/agent_chat/agents.rs` (presets, URL checks, ids). An address has a scheme,
//! no query or fragment, no trailing slash, and at most 2048 bytes.
//! Do not: write a key into a [`Connection`], a log line or the design file.

use crate::api::{ApiConfig, Protocol};
use serde::{Deserialize, Serialize};

/// Longest address kept.
const MAX_URL: usize = 2048;

/// One saved model endpoint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    /// Stable id, also the keychain entry name (see `key_id`).
    pub id: String,
    /// The name shown in lists.
    pub name: String,
    /// The wire format it uses.
    pub protocol: Protocol,
    /// The server address, as `normalize_url` leaves it.
    pub base_url: String,
    /// The model name to ask for.
    pub model: String,
    /// True when a key is stored in the keychain for this connection.
    #[serde(default)]
    pub has_key: bool,
    /// True when the model takes pictures (Attach view sends one).
    #[serde(default)]
    pub images: bool,
}

/// A starting point offered by the Add connection dialog.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    /// The name shown in the preset list.
    pub name: &'static str,
    /// The address it fills in (empty for a custom one).
    pub base_url: &'static str,
    /// The wire format it uses.
    pub protocol: Protocol,
}

/// Local servers first, then a custom address of each protocol.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Local model server",
        base_url: "http://localhost:11434/v1",
        protocol: Protocol::OpenAi,
    },
    Preset {
        name: "Second local server",
        base_url: "http://localhost:1234/v1",
        protocol: Protocol::OpenAi,
    },
    Preset {
        name: "Custom OpenAI-compatible address",
        base_url: "",
        protocol: Protocol::OpenAi,
    },
    Preset {
        name: "Custom Anthropic-compatible address",
        base_url: "",
        protocol: Protocol::Anthropic,
    },
];

/// Check and tidy an address: a scheme is required, a query or fragment is refused, a trailing
/// slash is dropped.
pub fn normalize_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.len() > MAX_URL {
        return Err(format!("The address is longer than {MAX_URL} bytes."));
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("The address must start with http:// or https://.".into());
    }
    if url.contains('?') || url.contains('#') {
        return Err("The address must not contain a query or a fragment.".into());
    }
    let tidy = url.trim_end_matches('/');
    if tidy.len() <= "https://".len() && tidy.ends_with("//") {
        return Err("The address has no host.".into());
    }
    Ok(tidy.to_string())
}

/// A fresh id starting with `prefix` that no connection in `existing` has.
pub fn new_id(existing: &[Connection], prefix: &str) -> String {
    (1u32..)
        .map(|n| format!("{prefix}-{n}"))
        .find(|id| !existing.iter().any(|c| &c.id == id))
        .unwrap_or_else(|| format!("{prefix}-0"))
}

/// The keychain entry name for a connection's key.
pub fn key_id(connection_id: &str) -> String {
    format!("connection:{connection_id}")
}

/// The settings the API backend starts with for `connection` and its key.
pub fn config_for(connection: &Connection, key: Option<String>, system_note: String) -> ApiConfig {
    ApiConfig {
        protocol: connection.protocol,
        base_url: connection.base_url.clone(),
        model: connection.model.clone(),
        key,
        system_note,
        retrieval: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_url_rejects_queries_fragments_and_missing_schemes() {
        assert!(normalize_url("localhost:11434/v1").is_err());
        assert!(normalize_url("http://a/v1?x=1").is_err());
        assert!(normalize_url("http://a/v1#top").is_err());
        assert_eq!(
            normalize_url(" http://localhost:11434/v1/ "),
            Ok("http://localhost:11434/v1".into())
        );
        assert!(normalize_url(&format!("https://{}", "a".repeat(2100))).is_err());
    }

    #[test]
    fn key_safe_for_allows_https_anywhere_and_http_on_loopback_only() {
        use crate::api::key_safe_for;
        assert!(key_safe_for("https://api.example.com/v1"));
        assert!(key_safe_for("http://localhost:11434/v1"));
        assert!(key_safe_for("http://127.0.0.1:1234/v1"));
        assert!(!key_safe_for("http://10.0.0.5:1234/v1"));
    }

    #[test]
    fn new_id_skips_ids_in_use() {
        let c = |id: &str| Connection {
            id: id.into(),
            name: String::new(),
            protocol: Protocol::OpenAi,
            base_url: String::new(),
            model: String::new(),
            has_key: false,
            images: false,
        };
        assert_eq!(new_id(&[c("api-1"), c("api-2")], "api"), "api-3");
    }

    #[test]
    fn a_connection_serializes_without_any_key_text() -> Result<(), String> {
        let c = Connection {
            id: "api-1".into(),
            name: "Home".into(),
            protocol: Protocol::OpenAi,
            base_url: "http://localhost:11434/v1".into(),
            model: "m".into(),
            has_key: true,
            images: false,
        };
        let text = serde_json::to_string(&c).map_err(|e| e.to_string())?;
        assert!(text.contains("\"has_key\":true"));
        assert!(!text.contains("sk-secret"));
        Ok(())
    }
}
