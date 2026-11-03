# PCBCraft page-by-page layout

What each work area holds, region by region. The tokens, components and rules are in [design-system.md](design-system.md); this page does not repeat them. The mockups are attached to the issues in the table at the end of that page.

Frame (1920 by 1240 reference): menu bar 32, tab nav 40, tool bar 40, body, status bar 24. Body = left aside 320, centre, right aside 340. The bottom sheet sits under the centre only (232 open, 36 collapsed). There is no left rail.

Each tab owns its regions through the `Workspace` trait in `crates/ui-egui/src/workspace.rs`: its tool bar, left aside tabs, centre, bottom sheet tabs and default layout. The user's layout is remembered per tab and View > Reset layout restores the defaults. The Workspace contract today lists Files, Schematic, Layout and Manufacture; the Library tab joins it as its own workspace.

## 1. Shell

- Menu bar: brand mark and name, then File, Edit, View, Help. Nothing else (no project title, no Save and Undo buttons, no version; those are menu items and Help, About).
- Tab nav: Files, Schematic, Layout, Manufacture, Library. These are work areas only; settings and one-off dialogs are not places you work in. The active tab has a 2 px copper bar.
- Tool bar: one row, changes per tab (listed per page). No layer drop-down and no selection-filter chips on any tab; the layer is picked in the Layers tree and picking is limited by locking rows. Buttons centre their icon and label.
- Status bar: document state left; error and warning counts (a click opens the bottom sheet on Findings), cursor X Y, units and zoom right. Nothing else.
- Bottom sheet: Findings (Run checks, severity filter, rows), Output (progress and results of the last run), Console (developer detail lives here), Parts (one row per part with reference, value, footprint, maker and part number; header buttons sort, a filter box per column narrows the rows, and a cell edit is one undo step; the boxes at the left pick rows, Shift picks a range, and Fill writes one typed value into a chosen column of every picked row as one undo step; Export saves the grid as a spreadsheet file with the visible columns, order and filter; Group identical merges parts the bill of materials prints as one line and adds a Quantity column). Collapsed it is a 36 px tab strip with a count.
- Right aside: Properties | Assistant on every tab that has one. Content padding is 12 on all sides.

## 2. Pages

### Files
Its own workspace. Left aside: New, Open, Save, Save as, Import, Export, Project settings, Preferences, Close. Centre: Recent projects by default; project name in display size, stat cards (Parts, Nets, Board, Checks) and the project files table follow. Right aside: project Properties (name, units, author, board size, layers). Never the Library here.

### Library
A full workspace for parts. Part browser with search and the Symbols | Footprints | Both segmented filter; part pages that open as tabs; a datasheet reader per component; vendor prices and supplier sources with editing; create and edit parts with symbol and footprint editors and 3D models; a Compare view that diffs the specs of two to five parts. A link that used to open settings opens the part in this tab instead.

### Schematic
Tool bar: Select | Add (Label, Power, Bus, Port, No connect, Pad; placing repeats until Esc) | Measure. While Label is armed a small panel over the sheet takes the net name to type: a name already in use joins that net and a new name makes one, so two labels of one name share a net, and a design that opens with nets of one name has them merged with a notice and one undo step. While Power is armed the panel takes the net (such as 3V3) and a style: ground, bar, arrow or negative; the power checks treat that net as driven. There is no Wire, Place part or Move button: a press on a pin starts a wire, library cards drag onto the sheet, and a drag moves a part. Left aside Layers | Library. The Layers tab is the sheet tree: the first page (Main until renamed) with Symbols, Net labels, Power ports, Buses, Text and notes, Drawing and Title block, then Parts and Nets. Net labels and wires belong to their net. Centre: a fitted sheet with zone border, grid, symbols with pin numbers, junctions, power symbols and title block. Each schematic page is its own page: a new page is empty, draws nothing on other pages and selects only its own parts. A page has one name, shown in the tab strip, the title block next to "Sheet 1 of 2", the Layers tree and Properties; rename it in any of them. Right aside: Properties of the selection. Bottom sheet collapsed with the findings count.

### Layout
Tool bar: Select, Route, Via, Zone, Keepout, Outline, Text, Measure | Corners (45, 90, Free), Width. The layout syncs from the schematic automatically; Edit > Update layout from schematic runs it by hand.

Left aside Layers | Library. The Layers tree is the centrepiece:
- search field, New layer, Show all, Hide others
- Copper (Top and Bottom copper with pads, tracks, zones), Silkscreen, Mask and paste, Board (outline locked, mounting holes), Courtyards, User layers
- Parts (each expands to pads) and Nets (colour, pad count; a net expands to its wires, tracks and vias)
- every row: chevron, swatch or icon, name, count, lock, eye; hidden and empty rows dim; every row has a tooltip that explains what it is

Selecting a row sets what the tools draw on. Hover previews a layer; a click pins it until you click again, press Esc or Reset. Page Up and Page Down step through sublayers. The full rules are in design-system.md, section 11.

Centre: board with chamfered corners, Y up with rulers and axis gizmo, dimensions, ratsnest, selection handles. The Layout view card at the top right holds 2D | 3D, Top | Bottom, rotate left, rotate right and reset. On 3D the canvas shows the board and parts with a soft shadow, the tool bar offers Orbit, Pick part, Measure and Export 3D model, and the Layers tree eyes drive visibility. Right aside: Properties. Board settings (the board row of the Layers tree) holds Board size, a Shape section (preset buttons for rectangle, rounded rectangle, circle, ellipse, polygon, L shape and chamfered rectangle, the sizes of the picked preset, and the live area and size of the outline), then the stackup. Bottom sheet open on Findings.
Draw shape: the right aside opens with a Drawing on chip (layer, purpose, one sentence on what a figure there becomes). The status bar names the shape, the layer and its purpose before the first click, and while a line, box or circle is pending the canvas shows its length and angle, width and height, or radius beside the pointer.

### Manufacture
A CAM and outputs workspace. Tool bar: Generate all (primary), Add output, Panelize | Machine picker, Run, Export | Preview toggle | stale count. Left aside Outputs | Machines. The outputs tree is grouped by kind: Fabrication (Gerber with a row per layer, Drill, Drill map, netlist), Assembly (parts list, placement, assembly drawing), CAM jobs (isolation milling, drilling, outline, laser, stencil, print, pick and place). Each row: name, machine, badge (Current, Stale, Not made), eye. Centre: preview of the selected output with the Show in preview checklist and the estimate card. Right aside: Properties of the output (name, machine, file name, side, tool table, feeds, depths, passes; Generate, Export, Run on machine). Bottom sheet: Output log and Findings.

## 3. Dialogs and menus

- Project settings (File menu): tabs Board and stackup, Design rules, Net classes, Machines, Title block.
- Preferences (Edit menu): tabs Appearance, Language, Units, Keys, Assistant.
- File menu with Import and Export submenus, the Edit menu and every right-click menu: design-system.md, section 8.

## 4. Region relevance

| Region | Files | Schematic | Layout (2D and 3D) | Manufacture | Library |
| --- | --- | --- | --- | --- | --- |
| Left aside | File actions | Layers or Library | Layers or Library | Outputs or Machines | Part browser |
| Right aside | Project properties | Part properties | Part properties | Output properties | Part properties |
| Bottom sheet | collapsed | collapsed | Findings open | Output log | collapsed |
