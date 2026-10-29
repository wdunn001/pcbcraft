# MCP and live control

## Getting started

Build the CLI, start MCP on stdio, and call `design.summary` and `inspect_design` before any edit.
For the same project the desktop shows, run the app with `--control` and connect
with `pcbcraft-cli mcp --connect`. Tool and command counts on fork `main` are in
[getting-started.md](getting-started.md).

```sh
cargo build -p pcbcraft-cli
cargo run -p pcbcraft-cli -- mcp
```

## Details

The executable `pcbcraft-cli mcp` implements a stdio MCP server. It negotiates
2025-11-25, 2025-06-18 or 2024-11-05 protocol versions, advertises tools, and returns
tool failures as `isError` responses. Messages are capped at 1 MiB. Until a
successful `initialize`, the stdio server answers every request except `initialize` and
`ping` with error -32002; notifications get no reply.

Example client configuration after `cargo build -p pcbcraft-cli`:

```json
{
  "mcpServers": {
    "pcbcraft": {
      "command": "/absolute/path/to/target/debug/pcbcraft-cli",
      "args": ["mcp"]
    }
  }
}
```

For a visible session, launch:

```sh
pcbcraft --sample --control 127.0.0.1:7991
pcbcraft-cli mcp --connect 127.0.0.1:7991
```

The bridge binds only to literal loopback addresses. It is trusted local automation
with filesystem access as the invoking user. It has no remote authentication.
Only tools/call requests are forwarded; tool discovery comes from the same registry.
Do not expose this port through forwarding to an untrusted network.

How the server measures against the specification, and the test list, are in
[mcp-conformance.md](mcp-conformance.md).

## Recommended agent flow

1. `design.summary`: a short, bounded view of the design (counts, board, parts, nets, findings). Read it first.
2. `inspect_design`: read current revision, stable IDs and capability limits when you need a full record.
3. `list_commands`: inspect schemas and millimetre/degree units.
4. `execute` or `batch`: supply `expected_revision` when coordinating concurrent edits.
5. `checks`: inspect scope, findings and object IDs; initial checks are not full ERC/DRC.
6. `screenshot`: obtain a PNG design preview of `schematic` or `pcb`.
7. `save` / `export`: use server-host paths. WSL paths differ from Windows paths.

Every engine command is also a directly callable MCP tool, such as `component.add`
or `board.cutout.add`. Use the generic `execute` wrapper for revision guards.

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"example","version":"1"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"execute","arguments":{"command":"component.add","params":{"kind":"resistor","position":{"x":30,"y":20}},"expected_revision":0}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"inspect_design","arguments":{}}}
```

Atomic batch example:

```json
{"commands":[{"command":"board.resize","params":{"width":80,"height":50}},{"command":"board.cutout.add","params":{"center":{"x":5,"y":5},"radius":1.6}}],"expected_revision":1}
```

The entire batch is one undo step. Failure leaves document, revision and history
unchanged. New/open/sample replacements are undoable. IDs are not recycled after undo.

`screenshot` renders a headless design preview; it does not capture desktop chrome.
To capture the actual UI, use the desktop's `--capture /absolute/path.png` option.

## Library workflow

`library.catalog` discovers host folders. `library.search` accepts `path`, `kind`
(`symbol` or `footprint`), optional `query`, and `limit` (1–100). A symbol path is
a `.kicad_sym` file; a footprint search path is a `.pretty` directory. Results
report search and directory-scan truncation. `library.inspect_symbol` accepts
`path` and `name`; `library.inspect_footprint` accepts a `.kicad_mod` `path`.
Read tools return resolved geometry and unsupported features without changing or
locking the project. Symbol catalogs cache two files using length/mtime; restart
to refresh external replacements deliberately preserving both metadata fields.

`component.place_library` accepts `path`, `symbol`, `position` and optional
`reference`/`value`. `footprint.assign` accepts component `id` and footprint `path`.
The engine validates supported placement and exact numbered-pin/pad correspondence,
embeds the used definitions, and makes assignment undoable. `board.sync` returns
`unassigned_components` rather than inventing packages for imported symbols.
These commands also work in atomic batches and the live desktop bridge.

`component.move` and `component.rotate` accept optional `unit` (default 1) for
independent schematic unit placement. All package pins still use the same
component ID and physical pin number. `track.route` accepts `layer` (`F.Cu` default
or `B.Cu`) and requires copper on that layer at both endpoints. Plated through-hole
pads connect their own front/back copper; two different pads are never bridged.
`track.route_pair` takes four pad refs (`from_p`, `from_n`, `to_p`, `to_n`) for a
declared differential pair, optional `width`, and `layer` (default `F.Cu`). It
routes both nets with coupled orthogonal L geometry and refuses when the pair gap
cannot be kept.

`library.inspect_symbol` and `library.inspect_footprint` accept `include_source`.
When true, up to 512 KiB of original nodes are returned for editing; larger source
nodes report `source_truncated`. The GUI disables copying truncated definitions.
Original files are never modified by placement or editing.

## Native library authoring

`library.symbol.upsert` / `library.footprint.upsert` accept a full validated
`definition` DTO and optional attribution `notice`. Inspect a source definition
with `include_source: true`, or construct one using the fields documented in
docs/kicad-libraries.md. `library.symbol.remove` / `library.footprint.remove` accept
`name`. `component.place_custom` accepts `name`, `position`, optional reference/value;
`footprint.assign_custom` accepts component `id` and footprint `name`.

`library.save {path}` atomically writes a `.pcbcraft-lib` collection without
changing history. `library.load {path}` validates and replaces the project-library
collection as one undoable edit. Collections carry `format: "pcbcraft-library"`,
`version: 1`, and `library` containing symbols, footprints and notices. These
operations retain original nodes/notices and reject unknown native fields.
Placed components retain embedded copies when library definitions are edited or
removed; edits do not silently rewrite existing circuits.

```sh
pcbcraft-cli tool library.catalog
pcbcraft-cli tool library.search --args '{"path":"/usr/share/kicad/symbols/Device.kicad_sym","kind":"symbol","query":"resistor","limit":20}'
```

## Fabrication and output jobs

`gerber.export` writes front and back copper, outline and Excellon drill into a
directory without editing the project. `output_job.add` records a named recipe
(listing export kinds such as `gerbers`, `netlist`, `bom`, `placement`, `dxf`,
`panel`, `layer_plots` and `raster`). `output_job.run` writes every file in that
recipe into one folder after layout changes. Drawn silk, mask and machine layers
in `layer_plots` become their own Gerber, DXF, SVG and HP-GL files as positive
ink, not negative soldermask from pads. The `raster` kind writes six high-DPI
PNGs (top and bottom copper, mask and legend). `panel.export` duplicates
fabrication outputs for every copy when a panel grid is enabled.

## Limits

MCP messages are capped at 1 MiB. The live bridge is loopback-only trusted
automation with no remote authentication. Initial checks exposed through `checks`
are not full ERC/DRC. Preview `export` supports PNG and SVG for schematic/PCB
views; it does not write fabrication BMP rasters (those PNGs come from output
jobs).

## See also

- [architecture.md](architecture.md) for the mutation boundary and crate layers
- [kicad-libraries.md](kicad-libraries.md) for library DTO fields used in authoring
- README **CLI and MCP** for copy-paste sample commands

## Tool hints

Every tool in `tools/list` carries a `title` and an `annotations` block with the four
MCP hints, so a client can decide when to ask the user first.

| Kind | Tools | readOnly | destructive | idempotent |
| --- | --- | --- | --- | --- |
| Read only | `inspect_design`, `list_commands`, `checks`, `board_mesh`, `screenshot`, `library.catalog`, `library.search`, `library.inspect_*`, `library.compare`, `library.sources`, `design.recents`, `design.list_open`, `design.activate` | yes | no | yes |
| Writes files outside the project | `*.write`, `*.export`, `export`, `save`, `library.save`, `library.export_*`, `output_job.package` | no | no | yes |
| Replaces the whole design | `new_design`, `sample_design`, `open` | no | yes | no |
| Everything else (`execute`, `batch`, `undo`, `redo`, engine commands, `library.add_folder`, `library.remove_folder`) | edits that can be undone | no | no | no |

`openWorldHint` is false for every tool. The title is the tool name with dots and
underscores turned into spaces. The table lives in `annotations_for` in
`crates/mcp/src/lib.rs`.

## Streamable HTTP

`pcbcraft-cli mcp --http 127.0.0.1:PORT` serves MCP over the Streamable HTTP transport
(revision 2025-11-25) on one endpoint, `http://127.0.0.1:PORT/mcp`. It prints the endpoint
and a per-run random bearer token to stderr; stdout is never used. With
`--token-file PATH` the token goes to that file (mode 0600) and is not printed.

```sh
pcbcraft-cli mcp --http 127.0.0.1:7992 --token-file ~/.pcbcraft-mcp-token
curl -s http://127.0.0.1:7992/mcp -H "Authorization: Bearer $(cat ~/.pcbcraft-mcp-token)" \
  -H "Accept: application/json, text/event-stream" -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

What the listener does and does not do:

- It binds only a literal loopback address (`127.0.0.1` or `::1`). There is no flag to
  bind elsewhere, and names such as `localhost` are refused.
- Every request needs `Authorization: Bearer <token>` (401 otherwise). A browser `Origin`
  header that is not `http://127.0.0.1`, `http://localhost` or `http://[::1]` (any port)
  gets 403; a request with no `Origin` is allowed.
- POST only. GET and DELETE get 405 (there is no event stream), other paths 404, bodies over
  1 MiB 413, a wrong `MCP-Protocol-Version` 400, a missing JSON `Accept` 406.
- A request gets 200 with one JSON object. A notification or a client response gets 202 with
  no body. A JSON array (batching) gets 400.
- It is stateless: no `MCP-Session-Id` is issued, so the initialize gate of the stdio server
  does not apply. Requests run one at a time against one design session.
- It reuses `handle_rpc`, the same dispatcher as stdio and the `--control` bridge.

## Resources

The server advertises `resources` (no subscribe, no list-changed notice) and answers
`resources/list`, `resources/templates/list` and `resources/read`. Reading never edits the
design; it goes through the active design like the tools do.

| URI | Type | Content |
| --- | --- | --- |
| `pcbcraft://design` | JSON | Same as `inspect_design` |
| `pcbcraft://commands` | JSON | Same as `list_commands` |
| `pcbcraft://checks` | JSON | Runs `checks` when read; there is no stored last result |
| `pcbcraft://design/preview/{view}` | PNG blob | `schematic` or `pcb`, same renderer as `screenshot` |

An unknown URI is error -32002 "Resource not found" with the URI in `data`. A preview view
other than `schematic` or `pcb` is -32602.

## Prompts

The server advertises `prompts` (no list-changed notice) and answers `prompts/list` and
`prompts/get`. Prompts are templates the user picks; each only names tools from `tools/list`.

| Prompt | Argument | What it asks the model to do |
| --- | --- | --- |
| `review_board` | `focus`: `placement`, `power` or `signal` (optional) | Call `inspect_design` and `checks`, review, report by object id, edit nothing |
| `fix_unrouted_nets` | none | Call `checks`, route the unrouted nets with `execute` or `batch`, check again |
| `prepare_fabrication` | `fab`: short text (optional) | Run `checks`, stop on errors, else write outputs with `gerber.export`, `bom.write`, `placement.write` |

An unknown prompt name or a `focus` outside the list is error -32602. Argument text is
inserted as plain text and clipped to 200 characters.
