# Roadmap and current status

The target is a complete Rust EDA application with MCP access to every feature.
This checklist records implementation, not an estimated parity percentage.
Each open line names the tracker issue that carries it. The program status epic,
wdunn001/pcbcraft#95, mirrors this list with what is in flight and the open
owner decisions. Issue numbers are current wdunn001/pcbcraft tracker numbers.
Milestone names here match the `milestone:` tags described in
CONTRIBUTING.md: M1 to M4 are `milestone:m1` to `milestone:m4`, M5 is
`milestone:m5`, M6 is `milestone:m6`. This list was last checked against main on
2026-11-06. A line is ticked only when the code and a test or story show it; an
issue can stay open after its line is ticked while follow-up slices remain.

## M0: MCP-first foundation (implemented)

- [x] Rust workspace and enforced dependency direction
- [x] Stable IDs, typed commands, discoverable schemas, revision guards
- [x] Transactional edits/batches, undo/redo, validated project persistence
- [x] MCP stdio and loopback bridge into the displayed session
- [x] Single-window tabbed editor, shared project and cross-tab component selection
- [x] Pinned craft-ui integration; dark/light/accent/grid preferences
- [x] Native and WebAssembly compilation gates

## M1: First circuit (implemented subset)

- [x] Four original two-pin symbol templates and generic SMD footprints
- [x] Explicit pin nets and orthogonal schematic wire presentation
- [x] Schematic-to-board reconciliation with retained footprint placement
- [x] Basic straight and orthogonal tracks, same-layer copper connectivity, airwires
- [x] Initial ERC/DRC with affected object references
- [x] The board is a free-form 2D shape: an outline of lines and arcs (#112) whose extent is read from the outline, with no board width or height field (`board.set_shape`, `board.set_outline`)
- [x] Each copper layer can follow the board shape or have a shape of its own (`board.layer_shape_set`, `board.layer_shape_clear`; story `a_newcomer_gives_the_top_layer_its_own_shape_and_edits_only_that`)
- [x] Cutouts, mounting holes and slots inside the outline, including on L-shaped boards
- [x] Board interchange subset export validated with an external checker
- [x] Read/search/preview installed symbol and footprint libraries
- [x] Embed placed single/multi-unit symbols and assigned front SMD/PTH footprints
- [x] Actual pad geometry in UI, previews, clearance checks and board export
- [x] Native project-library symbol/footprint editors with pins, pads and graphics
- [x] Multi-unit packages with independently movable schematic units and one footprint
- [x] Reusable native library collections, attribution, atomic saves and undoable load
- [x] Plated through-hole pads, circular/oval offset drills, front/back pad bridges
- [x] Library writers: `library.export` and footprint export that keeps pads, 3D model and extra nodes (#125)
- [ ] Interactive canvas library drawing (#124) and the Libraries tab Export actions (#289)
- [ ] Repeated common pins across symbol units (#126) (the model accepts common power pins across units and a test covers it; the story for a quad part is missing)
- [x] Place SMD parts on the bottom side: Flip to bottom and Flip to top from the Layout right-click menu (#129; `footprint.flip`, e2e `board_side`)
- [x] Graphical pin styles on project-library symbols (#127)
- [x] Unplated footprint holes and board mounting holes/slots (#128)
- [x] Explicit wire vertices, junctions, labels and no-connect flags (#132)
- [x] Buses: `wire.draw_bus`, `label.place_bus`, bus entries in the Add menu (#132)
- [x] Hierarchy ports with a direction, selectable on the page, with a direction picker in Properties
- [x] Power and ground ports move with their net: the wires that touch a port follow it and a port can be dragged from a pin (#1754)
- [ ] Hierarchy sheets and reusable blocks placed more than once (#130, #131, #205, #293, #294); importing a project into another design as a block (#1330)
- [x] Open a board and a schematic file made in another tool, with unknown-field preservation and round-trip fixtures (#134, #135, #287, #292); epic #133 stays open for further sources
- [ ] Complete pin-type electrical rules: the connection matrix is stored and editable in Project settings (`erc.matrix_set`); the full matrix (#136), its engine form (#207) and the Checks grid (#208) remain open
- [x] Project and net-class rule sets with fabrication presets (#137; `rules.preset_apply`, `netclass.set`)
- [ ] Rules tab fabrication preset picker (#210) and the remaining fabrication minimums (#295)
- [ ] Browser download/upload and project recovery (#138): open and save work in the browser (#267 verified, story set in `e2e/webfs.rs`); the recovery banner and drag and drop (#269) remain open
- [x] Configurable shortcut profiles and a complete default keymap (#139)
- [x] Schematic and board stay in sync automatically (#156; `project.set_auto_sync`, `board.sync`)

## M2: Serious board layout

- [x] Chamfered and custom polygon pad shapes with exact clearance outlines (#105)
- [x] Plated vias, stitch grids and editable copper pours with cutouts
- [x] Inner copper layer stackups (#141)
- [x] Interactive routing with obstacle avoidance, shove and track dragging (#106; stories in `e2e/route_drag.rs`: drag a route between pads, push a track aside, drag a track end)
- [x] Differential pair routing as one coupled gesture (#165) with skew tuning (`track.tune_skew`)
- [x] Length tuning and impedance-aware constraints (#107)
- [x] Copper zone thermal reliefs, islands, keepouts and the pour panel (#109; stories in `e2e/pours.rs`: thermal spokes, stray copper removal, starved pad, keepout)
- [x] Comprehensive clearance and manufacturing checks (#110, #318): hole spacing, mask bridges, silk line, copper sliver, minimum track radius and acute corners, each with a hint in the Results list
- [x] Checks on drawn machine layers against the copper and vias they cross (#101)
- [x] Design-intent electrical checks: power pins, decoupling and pull-ups (#158)
- [ ] Follow-ups to the design-intent checks: Checks tab UI (#327), ESD device placement check (#328), dedupe against the pin matrix (#330), decoupling distance by copper path (#189)
- [x] Many fabrication plots, drill, placement, BOM and named output jobs (#111), with fabrication-valid copper gerbers (#176)
- [ ] Everyday editing: copy and paste, box select, measure, groups and track cleanup (#166) (clipboard #254 and `track.cleanup` #255 are in; `object.group` and `board.select_net` exist; the box select, measure and filter interface #258 remains open)
- [x] Teardrop copper at pad and via junctions (`teardrop.add`, `teardrop.remove`)
- [x] Logos and reference images placed as bitmaps and traced to silk, mask or copper (#167)
- [x] Initial Gerber copper layer writer (pad flashes and tracks)
- [x] `route.auto`: route the unrouted connections of a net, a layer or the board with the existing router, in net-class order, one undo step, refusing a run that breaks a check or adds a second error of a code already present (#1116; `route.preview`, `route.settings`)
- [x] Placement: `placement.legalize` removes overlaps and keeps locked parts, the board edge and the grid, with preview and one undo (#1120); anchor and satellite placement for decoupling, crystal and edge parts (#1119; `layout.place_suggest`, `layout.place_apply`); `layout.report` with per-net routed and unrouted length

## M3: Sketch-driven board geometry and mechanical coordination

- [x] Line/arc outline sketch model inspired by a planar sketch workflow (#112)
- [x] Dimensions and named parameters (`dimension.add`, `parameter.set`) with expression-driven regeneration (#113)
- [ ] Constraints that follow a parameter change with undo, and the constraint interface (#113) (constraints are stored and validated; no test shows one following a parameter)
- [x] Unplated mounting holes, routed slots and rectangular edge notches (#114 subset; sketch-driven patterns remain in #112)
- [x] PCB/enclosure coordination, shared mounting datums and STEP exchange (#115)
- [x] Show an enclosure or assembly from the sibling Craft app around the board in 3D (#735; `board.enclosure.import`)
- [ ] Component bodies in STEP and enclosure model import (#145) (STEP part bodies are in; glTF, VRML and STEP model import remain open)
- [ ] Real 3D board tab with component bodies and inspection (#116) (box body size per footprint is in; model file loading #370 and the disk cache and load failure story #510 remain open; optional fetch of open library 3D models #726)

## M4: Complete workflows and distribution

- [ ] Simulation strategy compatible with the Rust-only constraint (#118) (`sim.dc` is in; decisions recorded for transient and AC, co-simulation, firmware execution, hardware in the loop and IDE link; the epics are listed under Analyze below)
- [ ] Full schematic/PCB/library editor workflows
- [x] Reference-project parity suite with golden outputs (#119)
- [x] Autosave/recovery, project migration, large-board responsiveness (#120; the ten thousand pad budget test runs under `cargo xtask bench` only)
- [ ] Native Windows/Linux/macOS packaging and browser distribution (#121), built by the CI/CD pipeline (#164) (nightly pre-releases with checksums are in #306; #297, #298, #299, #309, #311 remain open)
- [ ] Craft Launcher integration where authorized by its maintainers (#122)
- [x] Compare two versions of a design (#159; story `a_newcomer_compares_their_board_with_yesterdays`)
- [x] Open and inspect fabrication files (#160)
- [ ] Drawing frames and title blocks for schematics and drawings (#161) (frame model and plot output are in; the template editor is not confirmed; the title block is now a page Properties section)
- [ ] Accessibility: keyboard-only use, screen reader names and scalable text (#163) (a measured ratchet is in `e2e/a11y-ratchet.txt`; keyboard-only use and scalable text remain open)
- [ ] Text catalog and translations for every user-visible string (#173) (Spanish is in; the localization epic #446 holds the rest)
- [ ] Design calculators: track width, via, impedance, divider and unit conversion (#174) (the calc crate and `impedance.calc` exist; the in-app calculator pages remain open)

## M5: Inner copper layers

Epic #140. Tag: `milestone:m5`.

- [x] Layer model for any number of copper layers, with migration of existing designs (#141)
- [x] Route, pour and check on inner layers (#142; layer keys and picker, stories in `e2e/copper_layers.rs`, ground and power plane roles, pour checks that see vias only on the layers they touch)
- [ ] Fabrication outputs per copper layer (#143) (stack-accurate layer lists in DSN, CSV and GenCAD are in)
- [x] Blind, buried and micro vias (#144)

## M6: Manufacturing artifacts

Epic #146. Tag: `milestone:m6`. Artifacts are first-class project
objects with a stale marker and a preview, produced for fabrication houses and
for in-house machines.

- [ ] Artifact records with stale marker and preview (#147) (records, `artifact.stale` and the stale marker on each layer's files in the Layers tree are in; the Outputs view #215 and artifact commands #213 remain open)
- [x] Machine profiles (#148): `machine.*` commands and a Machines view in the Library tab with built-in and your own machines
- [x] Reflow ovens: the Puhui T-962A built-in oven, and named reflow profiles with stages checked against the oven's top temperature (#1646; `reflow.profile_add`, `reflow.profile_list`, `reflow.profile_remove`)
- [ ] Reflow profile list in the Machines tab and its stories (#1646)
- [x] Fabrication-house plots with attributes, drill split and drill map (#149)
- [x] BOM artifact (#150; artifact kind `bom`, `bom.export`, vendor formats, DNP column and merged identical parts)
- [ ] Assembly and fabrication drawings (#151) (`assembly_drawing` and the fabrication drawing with stackup, drill chart, impedance table and notes are in; goldens remain open)
- [ ] Fabrication-layer images at a chosen DPI with registration marks (#152) (pixel, invert and mirror tests remain open)
- [x] CNC isolation G-code (#153; `cam.isolation`, `cam.isolation_paths`, copper rubout, toolpath-closer-than-tool check)
- [ ] CNC drilling and outline G-code (#154) (Snapmaker drill and outline files are in; the tab and depth pass assertions remain open)
- [ ] Paste stencil and laser cut paths (#155) (stencil SVG and cut DXF are in; the per-pad golden and aperture reduction test remain open)
- [x] Plot drawn layers for laser, mill and printer output (#100)
- [x] Panel fabrication export with breakaway and tooling holes (#162 subset; panel settings on the project #247 and panel checks #249 remain open)

## Workbench and UX

User-interface work that cuts across milestones. These lines carry
`track:ui-design` and no `milestone:` tag.

- [x] Top menu bar, Start page, plain menu labels, canvas handles, pin and pad click wiring, stackup-first Layers, status strip, Review groups, custom checks, drawn machine layer model
- [x] Workbench layout: top menu bar, library left, assistant and Layers tree right, tool strip under the tabs (#123)
- [x] One vector-style Layers tree replaces the Parts, Nets and Layers docks (#98, #597)
- [x] Draw directly on your own layers from the canvas (#99)
- [x] Rules and Checks as top tabs, Project Settings with breathing room (#102)
- [x] Right-click context menu on the canvas (#103)
- [x] Library browser with search, source and kind filters and chips in the left dock (#104; stories in `e2e/library_dock.rs`)
- [x] Files page: a project files tree, a Board shape card that draws the real outline, read-only shape properties and no board width or height (story set in `e2e/files_shape.rs`)
- [x] Library sources live in the Library tab: a Sources page and a Collections page, with the old Libraries page and its Preferences tab removed (`e2e/library_sources.rs`)
- [x] Library tab: part data, prices at a chosen order quantity, compare, alternates and a part editor with pin to pad map (#699)
- [x] Publish and install library parts with `library.publish`, `library.install` and `library.list_publications`, with a publication scan that removes private lines before bundling (#989)
- [ ] In-app Publish, Browse and Install screens, update badges and Report (#989) and the portal service (#988); no screen calls the publish commands yet
- [x] Add tools show a translucent preview of the item under the pointer on the schematic and the layout (stories in `e2e/add_tool_preview.rs`)
- [x] Page Properties: the page name is the one title, the title block is a Properties section that shows what the sheet paints, and each page lists its power symbols, net labels and buses (#1753)
- [x] Online part sources: search distributor catalogs from the library (#157)
- [x] Find and place open-library footprints by name
- [x] Generate IPC-style land patterns from package parameters and read-only footprint checks (#168)
- [x] Parts table: a Parts tab lists every part in one grid to sort, filter and edit, with spreadsheet export, fill for several parts and merge of identical parts (#169; `e2e/parts_table.rs`)
- [ ] Schematic tool strip: wiring, labels, shapes, text and tables drawn on the sheet (#170) (labels, flags, text, line, rectangle and circle are in the Add menu; the sheet drawing end to end stories #304 and the strip UI #273 remain open)
- [x] Assistant pane backed by the typed command surface (#172), working in the browser with a key kept in that browser
- [ ] Assistant epic: bring the assistant structure of a sibling Craft app into PCBCraft (#185) (core crate, API backends, settings, edit gate, context and offline stories are in; knowledge retrieval #197, transcript #198, coding agent backend #200 and shared home decision #201 remain open)
- [x] Move the end-to-end stories out of the UI crate's lib.rs into `crates/ui-egui/tests/e2e/<area>.rs` (#219)
- [ ] Help reference: electrical engineering terms, formulas and calculators inside the app (issue to be filed; related to #174; `help.glossary` and `help.term` exist)
- Not planned: several designs open at once with a tab each. The owner decided on 2026-11-03 to keep one project open at a time; the schematic holds several pages. Reuse comes from importing a project as a block (#1330).

## Analyze tab

A sixth tab, Analyze, sits between Layout and Manufacture with ten pages laid out as the approved mockups show. Only Checks (design rules and electrical checks) and Net classes read the design. Every other page says plainly what data it is missing, draws no sample values and disables buttons that have nothing behind them. The simulation roadmap epics carry the label `sim-roadmap`.

- [x] Analyze tab shell with the page list, filter and Properties; Checks reads the real findings and Net classes reads the real classes (`e2e/analyze.rs`, #1446)
- [ ] Scope page: channels, timebase, triggers, cursors, measurements, math and FFT (#1360; the decision that the Scope lives on the Analyze tab with a strip in the Simulation tab is #1421)
- [ ] PDN impedance, Rails and Load step pages (#1358)
- [ ] Reflections and levels page (#1359)
- [ ] RF page and Antenna page (#1366)
- [ ] Firmware page (#1363)
- [ ] Hardware link page (#1364)
- [ ] IDE link page (#1365)
- [ ] Protocol decoder pages for UART, SPI, I2C and CAN (#1361)
- [ ] Eye diagram page (#1031)
- [ ] Placement, Impedance targets, Return path, Crosstalk and Rule sets pages read the checks that already exist in the engine (net impedance #654 and signal integrity findings #1021, crosstalk estimate #841); the pages do not read them yet

## Planned: browser providers

Planned, not shipped. Capabilities that look desktop only today (files and folders, network calls, stored keys, file pickers, sending to a machine) become providers with a native and a browser implementation, so every feature has a browser route. The epic is #1141, with the platform crate #1143, native providers #1144, browser providers #1145 (OPFS with an IndexedDB fallback, a fetch HTTP client, a browser secret store and file pickers), migration of the library, io and engine crates off std::fs (#1146), migration of the UI and MCP crates (#1147) and a lint and CI guard (#1148). Only a small browser HTTP slice for library fetches is on main; the platform crate is not.

No feature is considered complete solely because a command exists. Workflows need
usable UI, working MCP access, reference fixtures and meaningful verification.
