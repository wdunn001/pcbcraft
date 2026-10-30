//! Agent Client Protocol backend (desktop only).
//!
//! Adapted from SolidCraft (MIT OR Apache-2.0), commit 32558d0a:
//! `crates/ui-egui/src/agent_chat/{acp.rs,acp_client.rs,process.rs,discover.rs}`.
mod client;
mod discover;
mod process;
mod protocol;

pub use client::{AcpBackend, resolve_cli, test_agent};
pub use discover::{detect, Detection, Found, Login, Missing, KNOWN};
pub use protocol::{split_command, BRIDGE_NAME, McpBridge, PROTOCOL_VERSION};
