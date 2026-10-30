# Architecture and decisions

## Getting started

Read [getting-started.md](getting-started.md) for clone and run commands, then
[capabilities.md](capabilities.md) for the user-visible slice. This page
explains why the crates are layered and where mutations must go. Agents must
follow [AGENTS.md](../AGENTS.md) invariants before editing Rust.

## Details

PCBCraft follows the Craft application structure, with a headless design engine
that is the sole mutation boundary. The delivered application is Rust-only.
GPL-3.0-or-later.

| Layer | Crates | Responsibility |
|---|---|---|
| L0 | geom | Bounded millimetre geometry and distance operations |
| L1 | doc, library | Stable-ID project model; bounded pure Rust library parsers/DTOs |
| L2 | rules, io | Connectivity and initial checks; native project, board interchange and SVG export |
| L3 | engine | Typed command registry, transactions, revisions, undo/redo |
| L4 | assistant, mcp, ui-egui | Assistant backend seam and transcript; MCP over stdio / live bridge; thin tabbed Rust UI using craft-ui |
| Apps | pcbcraft, pcbcraft-cli, pcbcraft-web | Desktop, headless and browser entry points |

### MCP-first

The command registry publishes JSON schemas and descriptions. `execute` and
`batch` mutate a candidate document, validate it, and commit once. Failures preserve
the document, revision and history. Stable IDs survive edits, save/open and undo.
Optional expected revisions reject stale agent mutations. Undo branches never reuse
an allocated ID. Read tools expose actual project state and current limitations.

Headless MCP owns a session. Live MCP forwards calls over a loopback-only JSON-lines
bridge into the exact session displayed by the desktop. The bridge is local trusted
automation, not a remotely exposed service; do not bind to all interfaces.

The desktop may keep up to eight designs open at once. Shared tab title and dirty
rules live in `pcbcraft_engine::design_document`; the live bridge routes tools through
`DocumentHub` in `pcbcraft_mcp`. Per-design workbench tabs, selection and tool state
stay on each UI document slot in `ui-egui` (see issue #268).

### Single-window flow

Schematic, PCB, Libraries and Checks are workspace tabs in one window. Browser,
inspector and command entry are inline panes. 3D is reserved for a future real renderer.
Switching tabs carries the selected component into its corresponding board footprint.
Properties apply through commands, and checks can select the affected item.

### Initial model

Pins use explicit logical nets. Wires are schematic presentations between those pins.
Built-in original templates have generic SMD footprints. Installed KiCad libraries
are read directly; used resolved definitions are embedded in projects. Reference
test fixtures retain the separate KiCad library license. PCB sync reconciles
assigned footprints and pin nets, skipping unassigned imported components.
Copper connectivity joins touching same-net tracks and pads, separated by layer.
Plated through-hole pads bridge only their own physical pad across front/back
copper; a trace entirely within a circular/oval drilled opening is not copper
contact. Drill containment is checked against the actual rounded pad shape.
Initial clearance checks use capsule tracks and actual SMD/PTH pad outlines
with bounded chord error. Connectivity uses conservative inscribed pad bounds.
Library queries run off the GUI thread without locking the project.

Plated vias, stitch grids and editable copper pours (with cutouts and fill control)
are implemented on front and back copper. Schematic drawn wires with explicit
vertices, junctions, labels and no-connect flags are implemented. Inner copper
layers, hierarchy and comprehensive electrical rules remain future work.

### Pad shapes

`FootprintPad.shape` is one of circle, rect, oval, roundrect, chamfered or custom;
`shape_kind()` gives the typed view. A chamfered pad cuts its selected corners by
a ratio in (0, 0.5] of the shorter side. A custom pad is one simple polygon of 3 to
256 pad-local vertices that must contain the rectangular anchor given by `size`;
that anchor's inscribed disc is the conservative connectivity contact. Clearance,
board edge, cutout and drill containment checks use the exact chamfer or polygon
outline. KiCad import maps rect/roundrect pads with zero rounding plus chamfer
corners to chamfered, and custom pads whose primitives are one filled zero-width
polygon containing the anchor to custom; other custom primitives, chamfered rounded
rectangles and trapezoids are reported unsupported.

Fabrication writers emit front and back copper Gerbers (pad flashes and tracks),
a board outline layer, soldermask and paste layers, split plated and unplated
Excellon drill files, a drill map with a per-tool legend, a fabrication readme,
and separate positive-ink plots for drawn machine layers. Named output jobs can
rerun gerbers, netlist, bill of materials, placement, DXF, panel, drawn-layer
plots (`layer_plots`) and fabrication-layer PNG rasters (`raster`). Silkscreen
layers derived only from pad geometry are not generated yet.
The initial KiCad writer targets a small documented subset and must be checked with
KiCad CLI; there is no general KiCad importer yet.

### Packages, units and authoring

One Component owns all physical pins/nets and one board footprint. Its legacy
position/rotation represents unit 1; secondary SymbolUnitPlacement records provide
independent unit positions/rotations. Pin.unit selects its placement, so movement
and rotation update wire endpoints without changing physical pin identity.
Rendering views carry unit suffixes while retaining the underlying component ID.
Repeated physical pin numbers across units are rejected in this initial policy.

ProjectLibrary stores editable symbol/footprint DTOs and attribution notices.
Upsert/remove/load are typed engine transactions; placed definitions remain
embedded snapshots. The inline editor changes a draft and saves through commands.
Native collection save is an atomic MCP file operation, not a batch side effect.
Collections carry format/version plus the project-library data; unknown fields
are rejected. Original KiCad nodes remain provenance/unknown metadata, not a
regenerated KiCad library. Lossless library writing is future work.

## Limits

The engine exposes **306** commands today; MCP advertises **359** tools including
session and export helpers. Not every command has a polished UI workflow yet.
Browser WASM builds compile, but host file open/save/export is still planned.
Routing modes beyond orthogonal are optional and layer-local; they are not a full
interactive router.

## See also

- [mcp.md](mcp.md) for tool names, batches and the live bridge
- [validation.md](validation.md) for evidence on the initial vertical slice
- [ROADMAP.md](../ROADMAP.md) for milestone checkboxes
