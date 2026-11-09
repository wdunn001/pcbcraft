# Assistant backends

## Model endpoint backend

Connections in Settings store a URL and a key name. The key itself lives in a
`ModelKeyStore` that the shell supplies (`crates/assistant/src/provider.rs`). The
`pcbcraft-assistant` crate runs the HTTP tool loop and forwards calls through the
gate in the shell.

## Providers and key stores (desktop and browser)

The model side is two small traits, so the pane never asks which shell it runs in:
`ModelKeyStore` (get, set and clear a named key; a `Key` prints as "(key)") and
`ModelProvider` (send one chat request with the tool list, get the reply through a
callback). `CloudProvider` is the endpoint-with-a-key provider; it sends through a
`Transport`. `ProviderBackend` drives any provider one step per frame with no thread,
so it runs where threads do not exist.

| Shell | Keys | Requests | Web API behind it |
| --- | --- | --- | --- |
| Desktop | system keychain | blocking HTTP on a thread (`ApiBackend`, streams) | none |
| Browser | `localStorage` under `pcbcraft.assistant.key.<name>` | `fetch`, CORS mode, no credentials | Web Storage, Fetch |
| Tests | `MemKeys` | `MemProvider` or a canned `Transport` | none |

In the browser the key sits in page storage, where any script of the page can read
it. Settings says so in one line. The server must allow cross-origin requests from the
page; a server that does not shows the plain "did not answer" reason. Settings lists
"Local model in the browser (WebGPU)" and "WebMCP" as kinds that are not built yet.

Left for later slices: Test for a browser connection (it needs the same transport),
streaming replies in the browser (`fetch` body streams), the knowledge search tool,
keys in IndexedDB wrapped with Web Crypto, and the two kinds above. Heavy work such as
a local model belongs in a Web Worker so the page stays responsive.

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
