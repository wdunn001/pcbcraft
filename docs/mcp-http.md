# Connect a client over HTTP

`pcbcraft-cli mcp --http` serves the Model Context Protocol over the Streamable HTTP
transport (revision 2025-11-25; 2025-06-18 and 2024-11-05 clients also work). Use it for
clients that want a URL instead of a command to launch. Clients that can start a program
should keep using stdio (`pcbcraft-cli mcp`, see [mcp.md](mcp.md)).

## Start the server

```sh
pcbcraft-cli mcp --http 127.0.0.1:7992 --token-file ~/.pcbcraft-mcp-token
```

The endpoint is `http://127.0.0.1:7992/mcp`. The server prints the endpoint on stderr. It
makes a new random bearer token each run; with `--token-file` the token is written to that
file (mode 0600) and not printed, otherwise it is printed on stderr. Stdout is never used.
Add `--project FILE` to open a design first.

## Connect

Any client that speaks Streamable HTTP needs the URL and an `Authorization` header:

```json
{
  "mcpServers": {
    "pcbcraft": {
      "url": "http://127.0.0.1:7992/mcp",
      "headers": { "Authorization": "Bearer <the token>" }
    }
  }
}
```

To try it by hand:

```sh
TOKEN=$(cat ~/.pcbcraft-mcp-token)
curl -si http://127.0.0.1:7992/mcp -H "Authorization: Bearer $TOKEN" \
  -H "Accept: application/json, text/event-stream" -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"curl","version":"1"}}}'
```

The reply carries an `MCP-Session-Id` header. Send it back on later requests, then send
`notifications/initialized`, then `tools/list`, `resources/list`, `prompts/list` and
`tools/call` as usual. After initialize, clients should also send
`MCP-Protocol-Version: 2025-11-25`.

## Sessions

`initialize` issues an `MCP-Session-Id`. A request with an id the server does not know gets
404; start over with `initialize`. `DELETE /mcp` with the header ends the session (200). A
request with no session header still works, so simple clients are not forced to track one.
All sessions share the one open design.

## Progress over an event stream

A `tools/call` that carries `_meta.progressToken` and an `Accept` header listing
`text/event-stream` is answered as an event stream. The events are `notifications/progress`
(progress 0 queued, 1 running, 2 done, total 2) and then the JSON-RPC result. Without a token
or without that Accept entry the reply is one JSON object. GET answers 405: the server never
pushes anything on its own.

The progress is coarse on purpose. The tools run to completion as one step; there is no
per-file progress inside an export.

## Cancel a call

Send `notifications/cancelled` with the `requestId` of the call (and the same session header).
The server answers 202 and:

- a call still waiting its turn never runs;
- a call that is already running is not interrupted. The waiting request ends with error
  -32800 "request cancelled" and the tool's late result is dropped.

So cancelling an export can leave the files it had already written, and cancelling an edit that
already started does not undo it (use `undo`). Calls run one at a time against the one design.
Stdio does not support cancellation: it reads one message at a time, so the cancel is only
read after the call returns, and the notification is ignored.

## Resources and prompts

| URI | Content |
| --- | --- |
| `pcbcraft://design` | The open project, same as `inspect_design` |
| `pcbcraft://summary` | Counts, board, nets, parts and findings; same as `design.summary` |
| `pcbcraft://checks` | The checks findings, run when read |
| `pcbcraft://commands` | Every engine command with its schema |
| `pcbcraft://design/preview/{view}` | PNG of `schematic` or `pcb` |

Prompts: `review_board` (review the design, optional `focus`), `fix_findings` (work through the
checks findings), `fix_unrouted_nets`, `prepare_fabrication`. A `layout.report` resource is not
offered yet because that report is not on `main`.

`inspect_design` and `checks` declare an `outputSchema`, so clients can validate their
`structuredContent`.

## Security

- The server binds a literal loopback address only (`127.0.0.1` or `::1`). Names such as
  `localhost` are refused.
- Every request needs the bearer token (401 otherwise).
- A request whose `Origin` is present and is not `http://127.0.0.1`, `http://localhost` or
  `http://[::1]` (any port) gets 403. A request whose `Host` is not one of those gets 403 too.
  Together these stop a web page from driving the server through DNS rebinding.
- Bodies are capped at 1 MiB (413); at most 32 connections are served at once (503).

The tools can write files as you. Treat the token like a password.

### Binding another address

Only for a setup you have thought through, such as a client on another machine:

```sh
pcbcraft-cli mcp --http 192.0.2.10:7992 --allow-remote --allow-origin https://app.example
```

`--allow-remote` is required for any non-loopback address, and the token stays required. With it
the `Host` check is off, and only loopback origins plus each `--allow-origin` (exact match, no
wildcards) pass the Origin check. The server prints a warning. It speaks plain HTTP, so put it
behind TLS and a firewall rule; do not forward the port to the internet. The OAuth flow of the
authorization specification is not implemented.
