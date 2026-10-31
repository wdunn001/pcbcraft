# Assistant backends

## Model endpoint backend

Connections in Settings store a URL and API key in the system keychain. The
`pcbcraft-assistant` crate runs the HTTP tool loop and forwards calls through the
gate in the desktop shell.

## Coding agent backend (ACP)

Desktop only. The user picks a program that speaks the
[Agent Client Protocol](https://agentclientprotocol.com) on stdin and stdout.
PCBCraft starts it as a child process, declares that file and terminal access stay
off, and gives the agent one stdio MCP server:

`pcbcraft-cli mcp --connect 127.0.0.1:PORT --token TOKEN` named `pcbcraft-app`.

Implementation lives in `crates/assistant/src/acp/` (protocol, process management,
discovery, and `AcpBackend`). The gated loopback bridge is in `crates/mcp`
(`start_gated_bridge`).

### Packaging

Release packages must ship `pcbcraft-cli` next to the `pcbcraft` binary. If the
CLI is missing, the ACP backend refuses with: `pcbcraft-cli was not found beside
the app`.

Flatpak and other sandboxes need filesystem access to the agent install and login
folders, plus network, same as other desktop agents that spawn Node-based adapters.
