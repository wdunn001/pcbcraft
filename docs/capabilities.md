# What is implemented

Short inventory of behaviour on fork `main`. Open gaps are in
[ROADMAP.md](../ROADMAP.md). First-run steps:
[getting-started.md](getting-started.md).

## Window

One process, one window. Tabs: Schematic, PCB Layout, Libraries, Checks.
Project browser and properties stay docked. Dark default, optional light theme,
persistent accent and grid. Zoom about the cursor, middle-button pan. Periodic
project recovery, including on a normal close.

## Schematic

Built-in two-pin symbols (resistor, capacitor, LED, connector). Explicit logical
nets. Orthogonal pin-to-pin wires and free-form wires with editable vertices.
Local, global, hierarchical and power labels; junctions; no-connect flags.
Free graphics: line, rectangle, circle, arc, polygon, text. Multi-unit symbols
share one reference and footprint; units move independently.

Parts table graphics, dimensions, barcodes (Code 39), find-and-replace across
the project, named component properties, bill of materials grouping.

## Board

Front SMD and plated through-hole pads (circle, rectangle, roundrect, oval).
Straight, two-segment orthogonal and arc tracks. Plated vias, including stitch
grids. Poured zones with cutouts; unfilled zones are outline only. Rectangular
or polygonal outline, circular cutouts, Edge.Cuts export. Copper move, width
and layer edits update clearance findings. Step-and-repeat arrays, placement
locks, edge docking, castellated pad flags.

Net classes set default width and clearance. Per-net length targets, width
bounds, net-pair gaps, rectangular rule areas. Assembly variants with
do-not-populate sets and footprint swaps.

## Libraries

Search and preview installed symbol and footprint libraries, including inherited
definitions. Place, assign in bulk by reference wildcard, embed used geometry.
Project-library editors for pins, pads and drawing primitives. `.pcbcraft-lib`
round-trip. 3D model path is stored and exported; nothing renders the mesh yet.
Per-footprint clearance and mask/paste margin overrides are stored; only the
clearance is checked.

Pad and unit limits, unsupported styles, and fixture licenses:
[kicad-libraries.md](kicad-libraries.md).

## Checks and reports

Initial ERC and DRC with object IDs: shorts, clearance, board edge, cutout,
unrouted nets, pin assignment. Findings can be re-levelled or silenced per
code, per object, or per rectangle. Pad table, drill chart, board statistics,
impedance estimates from recorded stackup, net chain walk, library part
compare.

## Export

RS-274X Gerber (front/back copper, outline with cutouts), Excellon (including
plated/unplated split and oval slots), solder mask and paste, drill map,
fabrication readme, netlist, BOM CSV, DXF, pick-and-place. Silkscreen layers
are not written yet. PNG and SVG previews; plot report CSVs. Native project
is `.pcbcraft`.

## Engine and MCP

Typed commands with JSON schemas, atomic batches, stable IDs, revision guards,
32-step undo, validated saves. `pcbcraft-cli commands` lists the engine
surface. MCP exposes those commands plus library search, recent files, inspect,
export and rasters. Headless session or loopback bridge into the live desktop.
The shared UI compiles to WebAssembly; browser host file open/save/export is
not implemented.
