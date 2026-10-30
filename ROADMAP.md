# Roadmap and current status

The target is a complete Rust EDA application with MCP access to every feature.
This checklist records implementation, not an estimated parity percentage.
Each open line names the tracker issue that carries it. The program status epic,
tangency-labs/pcbcraft#352, mirrors this list with what is in flight and the open
owner decisions. Milestone names here match the `milestone:` tags described in
CONTRIBUTING.md: M1 to M4 are `milestone:m1` to `milestone:m4`, M5 is
`milestone:m5`, M6 is `milestone:m6`.

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
- [x] Rectangular outline dimensions and circular cutouts
- [x] Board interchange subset export validated with an external checker
- [x] Read/search/preview installed symbol and footprint libraries
- [x] Embed placed single/multi-unit symbols and assigned front SMD/PTH footprints
- [x] Actual pad geometry in UI, previews, clearance checks and board export
- [x] Native project-library symbol/footprint editors with pins, pads and graphics
- [x] Multi-unit packages with independently movable schematic units and one footprint
- [x] Reusable native library collections, attribution, atomic saves and undoable load
- [x] Plated through-hole pads, circular/oval offset drills, front/back pad bridges
- [ ] Interactive canvas library drawing (#300) and library writers (#301) (#300 subset on fork main; #459, #460 remain open)
- [ ] Repeated common pins (#302) and back SMD placement (#305)
- [x] Graphical pin styles on project-library symbols (#303 subset on fork main)
- [x] Unplated footprint holes and board mounting holes/slots (#304 subset on fork main)
- [x] Explicit wire vertices, junctions, labels and no-connect flags (#308 subset on fork main)
- [ ] Buses (#308) and hierarchy (#306, #307) (#308 subset on fork main; #379, #380, #461, #462, #464, #465 remain open)
- [ ] General import of boards and schematics with unknown-field preservation and round-trip fixtures (#309, #310, #311) (#309 subset on fork main; #310, #311, #458, #463 remain open)
- [ ] Complete pin-type electrical rules (#312) and project/net-class rules with fabrication presets (#313) (#312 subset on fork main; #381, #382, #384, #466, #467 remain open)
- [ ] Browser download/upload and project recovery (#314) (#314 subset on fork main; #438, #440 remain open)
- [x] Configurable shortcut profiles and a complete default keymap (#315)
- [ ] Schematic and board stay in sync automatically (#332)

## M2: Serious board layout

- [x] Chamfered and custom polygon pad shapes with exact clearance outlines (#281)
- [x] Plated vias, stitch grids and editable copper pours with cutouts (thermal reliefs and keepouts #284 remain open)
- [x] Inner copper layer stackups (#317)
- [ ] Interactive 45° routing, obstacle avoidance, shove and track dragging (#282) (#282 subset on fork main; the drag interface remains open)
- [x] Differential pair routing as one coupled gesture (#341 subset on fork main; skew tuning #407 remains open)
- [x] Length tuning and impedance-aware constraints (#283)
- [ ] Copper zone thermal reliefs, islands, keepouts and robust polygon operations (#284) (#284 subset on fork main; the pour panel and keepout interface remain open)
- [ ] Comprehensive clearance and manufacturing checks (#285) (#285 subset on fork main; #489 remains open)
- [x] Checks on drawn machine layers against the copper and vias they cross (#277)
- [ ] Design-intent electrical checks: power pins, decoupling and pull-ups (#334) (#334 subset on fork main; #492, #498, #499, #501 remain open)
- [x] Many fabrication plots, drill, placement, BOM and named output jobs (#286 subset on fork main; fabrication-valid copper gerbers #351 remain open)
- [ ] Everyday editing: copy and paste, box select, measure, groups and track cleanup (#166) (#166 subset on fork main; #254, #255, #256, #257, #258 remain open)
- [x] Teardrop copper at pad and via junctions (`teardrop.add`, `teardrop.remove`)
- [x] Logos and reference images placed as bitmaps and traced to silk, mask or copper (#343)
- [x] Initial Gerber copper layer writer (pad flashes and tracks)

## M3: Sketch-driven board geometry and mechanical coordination

- [ ] Line/arc outline sketch model inspired by a planar sketch workflow (#287)
- [x] Dimensions, constraints, named parameters and expression-driven regeneration (#288)
- [x] Unplated mounting holes, routed slots and rectangular edge notches (#289 subset on fork main; sketch-driven patterns remain in #287)
- [x] PCB/enclosure coordination, shared mounting datums and STEP exchange (#290)
- [ ] Component bodies in STEP and enclosure model import (#321) (#321 subset on fork main; STEP part bodies are in, glTF, VRML and STEP model import remain open)
- [ ] Real 3D board tab with component bodies and inspection (#291) (#291 subset on fork main; #542, #670 remain open)

## M4: Complete workflows and distribution

- [ ] Simulation strategy compatible with the Rust-only constraint (#292)
- [ ] Full schematic/PCB/library editor workflows
- [x] Reference-project parity suite with golden outputs (#293)
- [ ] Autosave/recovery, project migration, large-board responsiveness (#294)
- [ ] Native Windows/Linux/macOS packaging and browser distribution (#295), built by the CI/CD pipeline (#340) (#295 subset on fork main; #468, #469, #470, #477, #480, #482 remain open)
- [ ] Craft Launcher integration where authorized by its maintainers (#296)
- [ ] Compare two versions of a design (#335)
- [x] Open and inspect fabrication files (#336)
- [ ] Drawing frames and title blocks for schematics and drawings (#337) (#337 subset on fork main; #430, #431 remain open)
- [ ] Accessibility: keyboard-only use, screen reader names and scalable text (#339) (#339 subset on fork main; keyboard-only use and scalable text remain open)
- [ ] Text catalog and translations for every user-visible string (#349) (#349 subset on fork main; #604 remains open)
- [ ] Design calculators: track width, via, impedance, divider and unit conversion (#350)

## M5: Inner copper layers

Epic #316. Tag: `milestone:m5`.

- [x] Layer model for any number of copper layers, with migration of existing designs (#317)
- [ ] Route, pour and check on inner layers (#318) (#318 subset on fork main; #385, #386 remain open)
- [ ] Fabrication outputs per copper layer (#319)
- [x] Blind, buried and micro vias (#320)

## M6: Manufacturing artifacts

Epic #322. Tag: `milestone:m6`. Artifacts are first-class project
objects with a stale marker and a preview, produced for fabrication houses and
for in-house machines.

- [ ] Artifact records with stale marker and preview (#323) (#323 subset on fork main; #387, #388 remain open)
- [ ] Machine profiles (#324) (#324 subset on fork main; #357, #390 remain open)
- [x] Fabrication-house plots with attributes, drill split and drill map (#325)
- [ ] BOM artifact (#326)
- [ ] Assembly and fabrication drawings (#327)
- [x] Fabrication-layer PNG rasters via output jobs (#328 subset on fork main; UV-specific workflows and drawn-layer BMP export remain open)
- [ ] CNC isolation G-code (#329)
- [ ] CNC drilling and outline G-code (#330)
- [ ] Paste stencil and laser cut paths (#331)
- [x] Plot drawn layers for laser, mill and printer output (#276 on fork main)
- [x] Panel fabrication export with breakaway and tooling holes (#338 subset on fork main; interactive panel UI #417-#419 remain open)

## Workbench and UX

User-interface work that cuts across milestones. These lines carry
`track:ui-design` and no `milestone:` tag.

- [x] Top menu bar, Start page, plain menu labels, canvas handles, pin and pad click wiring, stackup-first Layers, status strip, Review groups, custom checks, unified library sources and sets, drawn machine layer model
- [x] Workbench layout: top menu bar, library left, assistant and Layers tree right, tool strip under the tabs (#299)
- [ ] One vector-style Layers tree replaces the Parts, Nets and Layers docks (#274) (scope continues in #756)
- [x] Draw directly on your own layers from the canvas (#275)
- [x] Rules and Checks as top tabs, Project Settings with breathing room (#278)
- [x] Right-click context menu on the canvas (#279)
- [ ] Library browser with filters in the left dock (#280)
- [x] Online part sources: search distributor catalogs from the library (#333)
- [x] Find and place open-library footprints by name (#76)
- [x] Generate IPC-style land patterns from package parameters and read-only footprint checks (#344 subset on fork main)
- [ ] Parts table: edit every part's fields in one grid (#345) (#345 subset on fork main; #435, #436 remain open)
- [ ] Schematic tool strip: wiring, labels, shapes, text and tables drawn on the sheet (#346) (#346 subset on fork main; #443, #444, #445, #475 remain open)
- [ ] Several designs open at once, one document tab each (#347) (#347 subset on fork main; #439, #441, #442 remain open)
- [x] Assistant pane backed by the typed command surface (#348)
- [ ] Assistant epic: bring the assistant structure of a sibling Craft app into PCBCraft (#360), with its core crate (#366), backends (#367, #374), settings (#368), edit gate (#369), context (#370), knowledge retrieval (#371), transcript (#372), offline behaviour and story suite (#373) and shared home decision (#375) (#360 subset on fork main; #371, #374, #375 remain open)
- [x] Move the end-to-end stories out of the UI crate's lib.rs into `crates/ui-egui/tests/e2e/<area>.rs` (#392)
- [ ] Help reference: electrical engineering terms, formulas and calculators inside the app (issue to be filed; related to #350)

No feature is considered complete solely because a command exists. Workflows need
usable UI, working MCP access, reference fixtures and meaningful verification.
