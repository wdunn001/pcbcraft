# PCBCraft

**A Rust EDA application, MCP-first, with one window and tabbed workspaces.**

GPL-3.0-or-later. Inspired by KiCad workflows and the SolidCraft / Craft application
architecture. Uses the pinned `craft-ui` library from `storytold/craft-libs`.

**Status: first working foundation, not a KiCad replacement yet.** The current
vertical slice is a flat schematic → assigned SMD PCB → basic routing → checks → KiCad
board export. The complete EDA roadmap is in [ROADMAP.md](ROADMAP.md).

## What works

- One native Rust window with Schematic, PCB Layout, Libraries and Checks tabs;
  project browser and properties remain docked in the same window.
- Dark default, light theme, persistent accent and grid preferences, zoom around
  the cursor, middle-button pan, symbol previews and drag-to-move.
- Periodic project recovery and recovery on normal close, using application storage.
- One-command alignment and distribution for groups of schematic symbols or
  board footprints: flush left/right/top/bottom, shared horizontal or vertical
  centre, even spacing along an axis, or snapping placements to the nearest
  multiple of a grid. Mirror a placed symbol or footprint left-to-right or
  top-to-bottom about its own origin; pins, pads, embedded library graphics
  and per-unit placements all flip together and two mirrors cancel exactly.
- Split an existing copper segment or drawn wire at any point on it; both
  halves keep the net, width and layer of the original. Cut the corner
  between two joined same-net track segments with a diagonal chamfer.
  Duplicate components
  at an offset with fresh ids and automatically unique reference labels,
  bringing the physical footprint when one is assigned. Placed copper
  segments and vias move after the fact, and the clearance findings follow
  the move. Reference designators and net names can be renamed at any time,
  validated unique against the rest of the design, and a netlist table can be
  imported in one step to assign pins to named nets by reference, or read back
  as a net-by-net pin table that also lists every unconnected pin. A router
  design session file on disk brings in the same assignments in one step,
  splitting its named nets into component-pin references. Components
  carry named properties (datasheet, manufacturer, custom notes) that are set,
  replaced or cleared one at a time and travel with a duplicate, and a bill of
  materials groups the design by value, kind and footprint with any requested
  properties as extra grouping columns, and the extra columns themselves can be
  defined once for the project (property key plus printed header) so every
  later bill of materials prints them; one call clears them again. One find-and-replace call sweeps any
  text across the whole project at once (references, values, properties, net
  names, board and graphic text), with a dry run that lists every hit first
  and refusals when a rename would collide or run out of room. Routed copper length is reported per
  net, with via counts and a list of nets that have no copper at all; a net
  class can demand a target routed length within a tolerance, and member nets
  outside that window are reported by the checks. A single net can also carry
  its own target and tolerance, which take priority over any class target. Any
  check code can be re-levelled between error and warning, or silenced
  entirely, when its findings are normal for a particular design; individual
  objects can also be excluded per code, so one accepted clearance silences
  just that finding instead of the whole check; whole rectangular areas can be
  excluded per code as well, so a work-in-progress corner can be held out of a
  check until it is finished. Net classes
  can also demand a minimum and maximum track width, and member tracks
  outside those bounds are reported by the checks. Components can be found by
  reference wildcard, exact value or kind, with each hit reporting whether it
  is placed on the board, and align or distribute can select its group with
  that same filter instead of an id list.
- Original resistor, capacitor, LED and two-pin connector symbols; explicit
  logical nets, pin-to-pin orthogonal wire presentations and free-form drawn
  wires with explicit vertices that may be bound to a net and edited after
  the fact (insert or remove any vertex, slide the whole wire with its
  junctions, or drag a single vertex), and free-standing
  schematic/board graphics (lines, rectangles, circles, arcs, polygons and text);
  local/global/hierarchical/power labels, junctions and no-connect flags that
  suppress the matching ERC warning, all movable after placement (graphics by
  delta, labels to an absolute position, junctions only onto a vertex of
  their net), rotatable (graphics in 90-degree steps about their centre or
  a chosen pivot, labels by any angle) and retargetable one vertex at a time,
  and any label can change kind or point at a different net in place, a
  junction only following copper that touches it,
  with their layer, stroke width and text all still editable; plated
  through-hole vias that join front
  and back copper, including stitch grids that fill a rectangular area with a
  via lattice; poured copper zones as closed polygons on either layer, with
  cutouts that remove copper from a zone; a moved zone shifts its outline and
  every cutout together, a poured zone can be switched to the other copper
  layer without moving a single vertex, any single corner of a zone or of
  one of its cutouts can be retargeted by index while the outline stays
  closed, and corners can be inserted or removed one at a time. A zone can
  also be taken out of the copper: an unfilled zone renders as a bare dashed
  outline, conducts to nothing and demands nothing, and pouring it back in
  brings its findings straight with it.
- Connectivity stays editable: pins can be taken back out of a net, which clears
  the matching placed pads too, and a placed track, via or copper zone can be
  switched to another net without deleting and redrawing it.
- Background search and geometry previews of installed KiCad symbol/footprint
  libraries, including inherited definitions. Place supported symbols with
  independently movable schematic units within one physical package, assign
  matching front SMD or plated through-hole footprints, in bulk with a
  reference wildcard, and embed used definitions.
- Symbol and footprint editors in the Libraries tab: editable pins, unit membership,
  pad/drill dimensions, and line/rectangle/circle/arc/polyline/text graphics. Save
  undoable project-library definitions and reusable `.pcbcraft-lib` collections.
- 3D model references: footprints read their model path and placement transform
  from library files, and one command each sets or clears that reference on a
  project-library footprint; the reference travels into board exports. Placed
  instances keep their embedded copy until reassigned, and nothing renders the
  model yet.
- Per-footprint design rule overrides: footprints read their local copper
  clearance and solder mask/paste margin overrides from library files, and one
  command each sets or clears them on a project-library footprint. The local
  clearance tightens clearance checks for that footprint's pads; the margins
  are recorded and exported without being checked.
- Schematic-to-PCB sync retains existing placements and reconciles pad nets.
  A pad table report lists every board pad by component reference with its net,
  position, shape, size and drill, for the whole board or one component. A
  drill chart report groups every drilled via and through-hole pad by size,
  round diameters and oval width/height counted separately, with the board
  thickness, for checking a fab's minimum-drill limits before ordering.
- Real circle/rectangle/roundrect/oval front SMD and plated through-hole pads,
  circular/oval offset drills, plus generic starter pads;
  straight or two-segment orthogonal tracks;
  airwires from actual same-layer copper connectivity.
- Copper sizes stay editable after placement: change a track's width or a via's
  pad and drill, reposition a placed footprint pad by its local offset (shaped
  pads keep their library outline, plain round pads can be resized too), move a
  track to the other copper layer without touching its
  ends, drag either end of a track to shorten, lengthen or re-aim it, and the
  clearance and width-bound findings follow immediately. Tracks can also curve:
  an arc through start, a point on the curve and end routes real copper - the
  clearance checks sample the arc, statistics count its length, and Gerber,
  DXF and preview output draw the true curve. Any placed board
  object (track, via, footprint, zone with its cutouts, circular cutout or
  text label) can also be slid by a plain dx/dy distance in one call.
- Editable rectangular board width/height, or a free-form closed outline traced
  from 3 to 64 vertices with the rectangle sized to its bounding box, and
  dimensioned circular cutouts, exported as Edge.Cuts. Cutouts can be nudged and
  resized afterwards as long as
  they stay inside the outline, and the copper-to-cutout checks follow.
  Constrained/parametric outline sketches are planned.
- Free text labels on either copper side with a glyph height in millimetres,
  for assembly notes and markings; turn them to any angle about their anchor,
  move, re-edit, and delete them like any other object, and they appear in
  previews and saved projects. The angle is carried in saved projects and
  exported previews; the desktop canvas still draws labels horizontally.
- Step-and-repeat arrays: clone a placed footprint and its component into a
  grid of up to 64x64 copies at fixed spacing in one call, with numbered
  references, collision refusal and a single undo for the whole array.
- Reference renumbering: reassign designators per letter prefix in placement
  order (R1, R2, ... from the top left), a dry run to preview the mapping, and
  any collision with a part outside the renumbered group refuses the whole call.
- Board statistics reports: object counts, routed copper length per layer,
  drilled-hole totals and board area in one call, plus a recorded surface
  finish (HASL, ENIG, OSP...) that travels with the saved project.
- Impedance estimates from the stackup: record the dielectric thickness and
  substrate permittivity, then ask what impedance each routed net's widest
  track gives and what width would hit a target ohm value.
- Per-layer fabrication notes: record each board layer's name, role (copper,
  core, prepreg, soldermask, silkscreen, paste or custom) and thickness; a
  repeated name replaces its note, notes stay ordered by name, and one call
  clears them all. The record travels with the saved project.
- Attached files: attach text files (licence notes, datasheet excerpts) to the
  project by name, replacing contents by re-attaching and detaching by name;
  up to 32 files of 256 KiB each, ordered by name, stored verbatim so they
  travel with every saved design.
- Machining notes: record whether board edges are plated and the drill and
  rout tolerances the board is held to (each within 1 mm); partial updates
  keep the other notes, and one call clears them all. The record travels with
  the saved project for fabrication packets.
- Batch editing: update up to 128 placed board objects in one undo step -
  tracks by net, layer and width; vias by net, diameter and drill; labels by
  text, layer and size. A bad entry refuses the whole batch.
- Assembly variants: define named alternate builds of the design, each with an
  optional note and a do-not-populate set of components; ids join or leave the
  set on request, an empty list clears it, and up to 16 variants travel with
  the saved project. A variant can also swap in a different footprint for any
  component, and a bill of materials printed for a variant skips its
  do-not-populate parts and shows those swaps in the footprint column.
- Net-pair clearances: require a minimum copper gap between two named nets,
  stricter than either class or the board default when larger; the same pair
  replaces its rule, one call removes it, and the clearance check honours it
  immediately.
- Rule areas: define a rectangle on the board that demands a stricter minimum
  copper gap for copper whose midpoints lie inside it, optionally narrowed to
  one net and one copper layer; the check honours it immediately and one call
  removes the area again.
- Barcodes: draw a Code 39 barcode (up to 48 characters of digits, letters and
  punctuation, case-folded) as real bar graphics on either layer, with
  selectable height and module width; the whole symbol lands and undoes as one
  step and stays editable like any other graphic.
- Castellated pads: flag any placed footprint pad castellated - plated and cut
  at a board edge so another board can be soldered onto it - and clear the
  flag again; the pad table reports which pads carry it, and the record
  travels with the saved project.
- Parts table: draw a bill-of-materials style table on the schematic as
  ordinary graphics - one row per component sorted by reference, columns for
  reference and value or up to three of your own properties, with a row cap
  that reports truncation - so the drawing captures the design as it stands.
- Edge connectors: dock any placed footprint flush against the left, right,
  top or bottom board edge - its copper lands exactly on the edge line,
  rotation preserved - so card-edge connections snap into place and undo as
  one step.
- Placement locks: lock any placed footprint in place; while locked it refuses
  move, rotate, mirror, array and edge docking, so approved placements survive
  later editing until you unlock them again.
- Graphic groups: bundle two to 512 drawn graphics into one group and shift the
  whole group by an offset in a single undoable step; dissolving a group
  leaves its graphics exactly where they are, and each graphic belongs to at
  most one group.
- Project metadata: record title, revision, company, notes or any other
  lower-case-keyed entry on the project, replace and clear them by key, and
  carry them with the saved design so exported documentation and handoff
  packets agree with the drawing.
- Dimensions: draw a linear dimension between any two points - witness lines,
  dimension line offset to either side, and the exact distance in millimetres
  as text - or a radius/diameter dimension of a circle: witness circle, leader
  through the centre, and an R or diameter label - on the schematic or either
  copper side; the pieces arrive grouped,
  so one translate moves the whole annotation and one undo removes it.
- Design queries: report the bounding box of all placed copper per side
  (tracks by width, via and pad extents, zone outlines), and list object
  IDs by kind with optional net and copper-side filters.
- One-call fabrication output: write RS-274X Gerber front and back copper
  layers, a board outline layer with circular cutouts, and an Excellon drill
  map into any directory. Poured zones, tracks, vias and pads flash with their
  real shapes and drills, and the export never edits the project. Oval holes
  are written as spanned slot hits that follow pad and footprint rotation,
  measured from a drill origin that can be recorded and reset. A plain-text
  netlist, one line per net with its component pins, and a
  bill-of-materials CSV with identical parts collapsed into one row export
  the same way, alongside an ASCII DXF drawing of copper, pads, zones,
  graphics and labels on named layers and a pick-and-place centroid file that
  lists every placed part by reference with position, rotation, side and
  package. Solder mask, silkscreen and paste
  layers are not generated yet.
- Net chain tracing: start at any pin and follow the series path through
  two-pin parts, reporting each hop's component, reference, pin and net, and
  why the walk stopped - an open end, a branching net, a many-pin part or a
  loop back to a visited pin.
- Library part comparison: line up any two symbols or any two footprints and
  get pin/pad counts, the numbers unique to each, shared counts and matching
  recommendations, straight from the library files with no project edits.
- Initial pin assignment/net-population checks and copper short, clearance,
  board-edge, cutout and unrouted checks with affected object IDs.
- Named net classes: group nets once and every clearance check between their
  copper uses the strictest requirement involved, while new tracks and routes
  on a classified net start at that class's width unless a width is given. A
  class can be retuned later or deleted outright, which returns its nets to the
  board default rules.
- 272 engine commands, discoverable schemas, atomic batches, stable IDs,
  revision guards, 32-step undo/redo and validated atomic project saves.
- 324 MCP tools: every engine command plus library discovery/search/inspection,
  source management for adding library folders from anywhere, recent designs,
  design inspection, checks, persistence,
  export, PNG raster previews saved to disk or returned inline, SVG vector
  previews, and plot report CSVs. Headless or bridged into the actual running desktop.
- Rust browser entry point; the shared UI and engine compile to WebAssembly.
  Browser host-file open/save/export are not implemented yet.

## Run on Linux / WSL

Install Rust 1.95+ and window-system development libraries. On Ubuntu:

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
  libx11-dev libxi-dev libxrandr-dev libxcursor-dev libgl1-mesa-dev
rustup target add wasm32-unknown-unknown
cargo run -p pcbcraft -- --sample --control 127.0.0.1:7991
cargo run -p pcbcraft -- --project led.pcbcraft --tab pcb
```

WSLg can display the Linux app on Windows. For automated UI capture without a
desktop, use Xvfb with `--capture`:

```sh
xvfb-run -a -s '-screen 0 1600x1100x24' \
  env -u WAYLAND_DISPLAY WGPU_BACKEND=vulkan \
  cargo run -p pcbcraft -- --sample --capture /tmp/pcbcraft.png
```

Windows can also build natively with `cargo run -p pcbcraft -- --sample`.

## First circuit

Load **LED example**, or press **A** to select a symbol and click to place it.
Press **W**, then click two pin circles to connect them. A new net is created
atomically if neither pin is assigned; conflicting existing nets are rejected.
Click **Sync to PCB** to generate footprints. Press **X** and click two same-net
pads to route between them. Open **Checks** to inspect remaining findings. Save
the native `.pcbcraft` project or export its board to `.kicad_pcb`.

The route tool is a simple orthogonal pad-to-pad operation. It does not avoid
obstacles; inspect its clearance findings. Footprint movement does not drag tracks.
Wire crossings do not implicitly join logical nets.

## KiCad libraries

Install KiCad's symbol and footprint libraries, then open **Libraries**. Search a
symbol, preview it and choose **Place in schematic**. Select its placed component,
choose **Assign footprint in Libraries**, and assign a package whose numbered pads
match its pins. **Sync to PCB** retains placement and reports symbols that still
need footprints. The project embeds used geometry, so reopening needs no library
installation. The desktop discovers Linux, Windows and Ubuntu WSL library folders;
custom paths and `KICAD10_SYMBOL_DIR` / `KICAD10_FOOTPRINT_DIR` are supported.

Multi-unit symbols share one component/reference, physical pin map and footprint;
each unit can move/rotate independently. Select its drawing or choose the unit in
Properties. All units, including separate power units, are initially placed.
Placement supports up to 32 units, 64 unique physical pins, and front SMD/plated
through-hole circle/rect/roundrect/oval pads. Repeated common pin numbers across
units, unsupported graphical pin styles, NPTH mechanical holes, back SMD placement,
3D models, footprint copper graphics and general KiCad project import remain planned.
Rounded-pad clearance uses sampled outlines with a small conservative error bound;
connectivity uses inscribed pad bounds and can conservatively flag corner contacts.
See [library formats, limits and provenance](docs/kicad-libraries.md).

Choose **Edit a copy**, **New symbol**, or **New footprint** inside Libraries to
author a definition. Edit pins/pads and drawing primitives with a live preview,
then save to **Project library**. Save/load a `.pcbcraft-lib` collection to reuse
it elsewhere. Original nodes and attribution notices are retained for imported
copies; native libraries reject unknown data fields rather than discarding them.
Library edits do not silently change placed definitions. Reassign a revised
footprint or place a revised symbol explicitly. KiCad library writing and advanced
interactive library drawing remain planned.

The portable `examples/dual-amplifier.pcbcraft` demonstrates three LM358 units,
one edited DIP8 package with an offset oval drill, and front/back routing.

## Keyboard defaults

| Shortcut | Action |
|---|---|
| A | Inline add-symbol chooser (schematic) |
| W | Wire between pins (schematic) |
| X | Route between pads (PCB) |
| M | Move selection, then click destination |
| R | Rotate selection 90° |
| E | Show properties |
| Delete | Delete selection |
| Escape | Cancel tool / placement preview |
| Ctrl+Z / Ctrl+Y | Undo / redo (Ctrl+Shift+Z also redoes) |
| Ctrl+S | Save to the inline project path |
| Ctrl+K | Inline command panel |
| Home | Fit the active canvas |

These are the initial KiCad-style defaults, not a complete configurable KiCad keymap.

## CLI and MCP

```sh
cargo run -p pcbcraft-cli -- commands
cargo run -p pcbcraft-cli -- sample --out led.pcbcraft
cargo run -p pcbcraft-cli -- run examples/led-indicator.json --save led.pcbcraft --export led.kicad_pcb
cargo run -p pcbcraft-cli -- checks --project led.pcbcraft
cargo run -p pcbcraft-cli -- snapshot --sample --view pcb --out led.png
cargo run -p pcbcraft-cli -- mcp
cargo run -p pcbcraft-cli -- mcp --connect 127.0.0.1:7991
```

See [docs/mcp.md](docs/mcp.md) for client configuration and request examples.
MCP uses JSON-RPC over newline-delimited stdio. Logs/errors go to stderr.

## Development

```sh
cargo xtask ci  # formatting, clippy, tests, UI/engine/browser wasm checks
cargo build -p pcbcraft -p pcbcraft-cli
# Browser: install Trunk, then:
cd apps/pcbcraft-web && trunk serve
```

Keep build products outside the mounted Windows tree in WSL for better filesystem
performance, e.g. `CARGO_TARGET_DIR=$HOME/.cache/pcbcraft-target`.

Architecture: [docs/architecture.md](docs/architecture.md). Agent instructions:
[AGENTS.md](AGENTS.md). Provenance: [ATTRIBUTION.md](ATTRIBUTION.md).
How work flows through the tracker, from issue to merge: [CONTRIBUTING.md](CONTRIBUTING.md).

## License

PCBCraft is licensed under GNU GPL version 3 or later; see [LICENSE](LICENSE).
Third-party dependencies retain their own licenses and notices. No KiCad source
implementation has been copied or ported in this initial version. KiCad is an
independent project; PCBCraft is not affiliated with or endorsed by the KiCad team.
KiCad library fixtures retain CC-BY-SA 4.0 with the design-output exception; the
data license is separate from the Rust application's GPL license.
