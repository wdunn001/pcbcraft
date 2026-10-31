# PCBCraft app design system

Status: accepted by the maintainer. Page by page layouts are in [page-layouts.md](page-layouts.md). The mockups are not committed; each one is attached to the GitHub issue listed in section 11. Where fork/main has not caught up yet, this page is the target and new work follows it.

The app is a dockable workbench with one frame for every page:

1. menu bar (brand, File, Edit, View, Help)
2. tab nav, work areas only: Files, Library, Schematic, Layout, Manufacture (3D is the 2D | 3D toggle in the Layout view card, not a tab)
3. tool bar, one row, changes per tab
4. left aside: Layers | Library tabs on Schematic and Layout, Outputs | Machines on Manufacture, actions on Files
5. centre work area (canvas or page)
6. right aside, tabs: Properties and Assistant
7. bottom sheet, collapsible, tabs: Findings, Output, Console
8. slim status bar

Each tab owns its regions. The contract is the `Workspace` trait in `crates/ui-egui/src/workspace.rs`: a workspace supplies its tool bar, its left and bottom region tabs, its centre and its default layout, and the layout is remembered per tab. What suits the Schematic is not what suits the Layout, so never assume one tab's regions on another. New work plugs into a region by adding a body arm for its region tab id and never edits the shell.

There is no left navigation rail and no other bar. Navigation is always tabs at the top of a region.

Where the old tabs went: Checks is the bottom sheet Findings tab (Run checks button, severity filter; the status bar counts open it). Rules, stackup, net classes, machines and title block are File > Project settings (a modal dialog with tabs at its top). Theme, language, units, keys and assistant backend are Edit > Preferences (modal dialog with tabs). Fabrication and CAM outputs are the Manufacture tab. Never add a tab for an action or a settings page.

## 1. Rules

1. No developer text in the UI. Not revision counters, key hint strings, crate or command names, scope paragraphs, "Tool: Pick", "rev 16". Key hints live in tooltips and in Help, Shortcuts. Long explanations live behind a disclosure ("About these checks") or a tooltip.
2. Every control has a visible plain label, or if it is an icon button, a tooltip and an accessible name. Labels are one to three words: Add, Apply, Hide, Mute, Save, Show. The effect goes in the tooltip.
3. A panel shows only what is relevant to the active tab. Layers tree: schematic objects on Schematic, board layers on Layout, output tree on Manufacture. Physical board setup (finish, thickness, substrate, plated edge, stackup) lives only in File > Project settings, Board and stackup.
4. Sub-views are tabs at the top of their region (page tabs, aside tabs, sheet tabs). Never a rail, never a button strip in the middle of a page.
5. Show state in text or shape as well as colour (warning triangle plus word, eye icon plus dimmed row).
6. Empty states explain what is missing and offer one button.
7. Title case is not used. Sentence case everywhere. Units are shown in the field (mm, deg), never in the label.
8. Y is up on every board view. The axis gizmo shows it. Rulers show 0 at the lower left.
9. Corners are cut at 45 degrees by default (board edge and routed corners). The Corners control in the Layout tool bar and Settings, Board offers 45, 90 and Free.
10. No names of other design tools appear in UI text or docs.
11. Tooltips explain. A tooltip says what the thing is and what it is for in plain words, not only its name (see [tooltips.md](tooltips.md)). Every Layers tree row has one.
12. Never draw a status dot, arrow or tick as a text character (a bullet, a check mark, a triangle). The bundled fonts may lack it and it shows as an empty box. Paint a shape or use the icon set. A test checks the font coverage of every non-ASCII UI string.
13. One command, many surfaces. The same action may sit in the tool bar, the right-click menu and the Properties of the selected object; all run one engine command and the tooltip or menu row names the shortcut.
14. Hover on anything actionable shows an affordance (outline, ring on pins and pads, net highlight, cursor, status text). It is static when Reduce motion is on.
15. A choice between two or a few settings is a segmented button group (section 5.4), never two loose buttons and never a drop-down.
16. All UI text comes from the string catalog (`crates/text/strings/en.txt`), never inline in code.

## 2. Colour tokens

UI roles map to the website palette. Canvas layer colours are a separate set and never used for chrome.

### UI roles

| Role | Dark | Light | Use |
|---|---|---|---|
| ground | #0A1411 | #F4F3EC | window, menu bar, tab nav, centre work area behind cards |
| panel | #111F1A | #FFFFFF | asides, cards, tool bar, bottom sheet, floating canvas controls |
| raised | #182A23 | #E9E8DF | inputs, segmented track, chips, hovered rows |
| hover | #1F352C | #DEDDD2 | hover on raised surfaces |
| line | #2C4A3E | #B9C4BC | decorative dividers (never the only border of a control) |
| edge | #5E8576 | #667B70 | borders of inputs and buttons |
| text | #E8F0EB | #101C17 | body text, focus ring |
| muted | #9DB3A8 | #44574E | secondary text, idle icons, captions |
| copper | #E39A5B | #8F4A12 | primary action, active tab bar, selection, active tool |
| mask | #4FCB8C | #14663F | success, no problems |
| gold | #E6BE5A | #7A5A00 | warning |
| cyan | #5CC8E8 | #0B6785 | links, pin numbers, information |
| red | #FF8A80 | #B3261E | error |
| onacc | #0A1411 | #FFFFFF | text on copper |

Tints used for selected rows and active tools: copper at 16 percent alpha (dark) and 12 percent (light). Same pattern for warning, error and success badges.

Contrast: all text pairs are checked at AA. Copper text on its own tint is checked in the contrast gate. Light copper (#8F4A12) on white is 6.65.

### Canvas layer colours (separate set, drawn by the painter, not by egui Visuals)

| Role | Dark | Light |
|---|---|---|
| canvas background | #07100D | #F1F0E6 |
| grid minor, major | #12231C, #1B3329 | #E1E0D3, #CFCEBF |
| board substrate fill | #0E3B2D | #CFE3D6 |
| board edge | #E6BE5A | #7A5A00 |
| top copper | #E8834A | #C2571A |
| bottom copper | #4FA3E0 | #1F6FB5 |
| silkscreen | #EDEBDD | #2A2A24 |
| courtyard | #B58AE0 | #8A52B8 |
| ratsnest | #E8F0EB at 70 percent | #3A4A42 |
| selection | #5CC8E8 | #0B6785 |
| schematic wire | #4FCB8C | #14663F |
| schematic symbol body | #E6BE5A | #7A5A00 |
| schematic pin number | #5CC8E8 | #0B6785 |
| schematic sheet fill | #0B1713 | #FBFAF3 |

Net colours in the Layers tree (VCC, LED, GND) are assigned from the canvas set and stay stable per net.

## 3. Type

Fonts: IBM Plex Sans (UI, Regular 400, Medium 500, SemiBold 600) and JetBrains Mono Regular (numbers, references, net names, file names). Both are OFL, already used by the website. The mockups embed the same files.

Five sizes:

| Name | Size / line | Weight | Use | egui TextStyle |
|---|---|---|---|---|
| Caption | 11 / 16 | 400, or 600 uppercase with 0.06 em tracking for section captions | section captions, hints, table heads, badges | Small |
| Mono | 12 / 16 | 400 | values, references, net names, file names, status readouts | Monospace |
| Body | 13 / 20 | 400 body, 500 buttons and tabs, 600 active tab and strong text | everything else | Body and Button |
| Title | 16 / 24 | 600 | card titles | Heading |
| Display | 22 / 28 | 600 | one per page, the page name | custom `TextStyle::Name("display")` |

Rules: no other sizes. Do not use `RichText::small()` or `.heading()` ad hoc; use the named styles.

## 4. Spacing, radius, strokes

- Grid: 4 px. Scale: 4, 8, 12, 16, 24, 32. Aside content padding 12 on every panel, Properties included (no content touches the aside edge). Page padding 24 by 32. Card padding 16. Gap between cards 16. Gap inside button groups 4 or 8.
- Heights: menu bar 32, tab nav 40, tool bar 40, aside tab strip 40, panel header 36, list row 28 (tree), 36 (list), 48 (finding), control 28 (24 small, 32 large), status bar 24.
- Widths: left aside 320 (resizable 240 to 480), right aside 340 (resizable 280 to 480).
- Radius: 4 for controls and chips, 6 for cards and floating canvas controls, 2 for segmented items. Nothing else. The brand chamfer is not copied into widgets.
- Strokes: 1 px borders. Selection and focus 2 px. No shadows, no gradients, no glow (egui flat look).
- Icons: 16 px, 1.4 px stroke, drawn from the project's own icon set (`crates/ui-egui/src/ui_kit/icons.rs`).

## 5. Components

Common states: rest, hover, active (pressed or selected), disabled, focus. Focus is a 2 px ring in `text` colour, 2 px outside the control, drawn by our own helper (see Focus below).

### 5.1 Primary button
Copper fill, `onacc` text, 600, height 28, padding 12, radius 4. Hover: brighten 10 percent. Active: darken 8 percent. Disabled: 45 percent opacity, no hover. One primary per region (page header, tool bar, aside footer).
egui: custom `fn primary_button(ui, icon, label) -> Response` using `ui.allocate_response` and `painter.rect_filled`; set `ui.style_mut().visuals.widgets.*` is not enough because Button fill is shared with secondary.

### 5.2 Secondary button
`raised` fill, 1 px `edge` border, `text` colour, 500. Hover: `hover` fill. Active: `raised` with `copper` border. Disabled as above. Ghost variant (no fill, no border) for Hide, Mute, Reset view, tertiary actions in rows.
egui: the default `Button` after Visuals are set (section 6). Ghost: `Button::new(..).frame(false)` plus hover fill from `widgets.hovered.weak_bg_fill`.

### 5.3 Icon button
28 by 28, icon 16, `muted` icon, no frame. Hover: `raised` fill, `text` icon. Active: `copper` icon. Always has `.on_hover_text(label)` and an `AccessKit` label. Used for add, more, zoom, collapse.

### 5.4 Segmented control
Track: `raised`, 1 px `edge`, radius 4, padding 2. Items: 500 `muted`; selected item: copper fill, `onacc` text, 600; hover: `hover` fill and `text`. Use for every choice between 2 to 4 mutually exclusive options that change a setting or a view: 2D and 3D, Top and Bottom, mm and mil, 45, 90 and Free, the Library filter Symbols, Footprints and Both. Not for navigation.
egui: `segmented` in `crates/ui-egui/src/ui_kit/segmented.rs`.

### 5.5 Tabs
Three levels, one component with a size parameter:
- tab nav (40 high, 16 px side padding)
- aside tabs and page tabs (40 high, 14 px padding)
- sheet tabs (36 high, 12 px padding)
Rest: `muted`, 500. Hover: `text` plus `raised` fill. Active: `text`, 600, 2 px `copper` bar along the bottom edge inset 6 to 8 px. Optional count badge: 18 px pill, `gold` on gold tint for findings, `muted` on `raised` for neutral counts. Disabled tabs are not shown; irrelevant tabs are hidden.
egui: custom `tab_bar(ui, &mut current, &[Tab{label, badge}])`. Do not use `SelectableLabel`. With egui_dock, hide the dock's own tab bars on asides and render this component through `TabViewer::tab_ui`.

### 5.6 Panel header
36 high, 12 side padding, Title 16/600 or caption, optional icon buttons right aligned, 1 px `line` bottom border. Used above lists and trees inside an aside or card. Property panels use a 48 high variant with an icon, a mono reference and a caption line (part type).

### 5.7 List row and tree row
List row: 36 high, 12 side padding, optional leading icon, label 13, trailing meta in Mono or chips, 1 px divider at 55 percent `line`. Hover: `raised`. Selected: copper tint plus a 3 px `copper` bar on the left edge. Tree row: 28 high, indent 16 per level, chevron 16, swatch or icon, name, count in Mono, lock cell 24, eye cell 24 (see layers tree in the page spec). Dimmed row (layer hidden or empty): name and swatch at 45 percent.
egui: `fn list_row(ui, selected, add_contents) -> Response` using `ui.allocate_exact_size`, painting the background first, then `ui.allocate_ui_at_rect` for the content. Trees use `egui::CollapsingState` for open state but our own row painter.

### 5.8 Finding row
48 high minimum. Severity icon (warn triangle in `gold`, error circle in `red`, ok circle in `mask`), a strong one-line title ("VCC is not fully routed"), a muted detail line (where, which layer), a Mono chip for the net or part, then Show (secondary small), Hide and Mute (ghost small). Selected: as list row. Compact variant (36 high, one line) in the bottom sheet.

### 5.9 Form row with label
Two columns: label column 200 on pages (104 in the right aside), control column fills. Label 13 `text`; optional hint under it, Caption `muted`. Row min height 40 (36 in aside) with a 55 percent `line` divider. Validation message goes under the control in `red` with an error icon.
egui: `egui::Grid` with fixed first column width, or a helper `form_row(ui, label, hint, |ui| ..)` that uses `ui.horizontal` inside a 2 column `StripBuilder`.

### 5.10 Number field with unit
`raised` fill, 1 px `edge`, radius 4, height 28, Mono 12, value left, unit right in `muted`. Width fixed per use (96 to 140). Focus: border `text` plus ring. Error: border `red`. Up and down arrow keys step by the grid, Shift steps by 10, Esc reverts. Always shows the unit.
egui: wrap `TextEdit::singleline` in a frame with our paint, parse through `number_entry.rs` (exists today), draw the unit with `painter.text` at the right edge.

### 5.11 Dropdown (select)
Same frame as the number field, label left, chevron right, popup via `egui::ComboBox` styled with Visuals (`menu_corner_radius`, `window_fill` = panel).

### 5.12 Empty state
Centered column: 40 px circle on `raised` with a muted icon, strong one-line statement, one muted sentence, one secondary button. Example: "No fabrication files yet" with "Go to Plot files". Never a bare sentence.

### 5.13 Status bar item
24 high, 10 px padding, 12 px text, `muted`. Icon plus text. Hover: `raised` and `text`. Left: document state (Ready, Saved, Checks done). Right: errors count, warnings count, X Y readout (Mono), units, zoom. Nothing else. Items that open something are buttons with tooltips.

### 5.14 Tool bar button (secondary toolbar)
28 high, 10 px padding, icon 16 plus label 13, radius 4. Rest: `text`. Hover: `raised`. Active tool: copper tint fill, 1 px `copper` border, `copper` text, 600. Toggles that are not tools (Preview on Manufacture) use the same look with an eye icon. The icon and the label are centred vertically in the button with equal side padding; a layout test checks this on every tab. Groups separated by a 1 px divider with 8 px margin. Context options (grid, width, corners) are placed after the group divider with a `muted` label.

### 5.15 Chip
20 high, Mono 12, `raised` fill, 1 px `line`, radius 4. For nets, pins, file kinds.

### 5.16 Card
`panel` fill, 1 px `line`, radius 6. Header 44 high with Title and optional caption and actions; body padding 8 by 16.

### 5.17 Floating canvas control
`panel` fill, 1 px `line`, radius 6, padding 4. Holds zoom, fit, view side. Positioned 16 px from the canvas edge. `egui::Area` with `Order::Foreground`, anchored.

### 5.18 Layers tree (the centrepiece)
Specified in [page-layouts.md](page-layouts.md), Layout, and summarised in section 11.

### Focus
Every interactive widget draws a 2 px ring in `text` colour, outset 2, when `response.has_focus()` and the last input was the keyboard (`ctx.input(|i| i.focus_visible)` style check). Helper: `fn focus_ring(ui, &Response)` called by every custom widget. Do not rely on `Visuals.selection.stroke` for this.

## 6. egui implementation notes (egui 0.36)

`crates/ui-egui/src/theme.rs` holds `pub struct Tokens` (one per theme, plain `Color32` fields named as in section 2) and `pub fn apply(ctx, &Tokens)`. The mapping below is the intent.

**Fonts** (`ctx.set_fonts`): register `plex-regular`, `plex-medium`, `plex-semibold` (static instances; egui cannot select weights from a variable font) as `FontFamily::Name`, put `plex-regular` first in `Proportional`, `jetbrains-mono` first in `Monospace`. Keep the existing fallbacks for CJK and Arabic scripts behind them.

**Text styles**: `style.text_styles` = Small 11, Body 13, Button 13, Monospace 12, Heading 16, plus `Name("display")` 22. Heading uses the semibold family.

**Spacing**: `style.spacing.item_spacing = vec2(8,4)`, `button_padding = vec2(12,4)`, `interact_size = vec2(28,28)`, `window_margin = Margin::same(16)`, `menu_margin = Margin::same(4)`, `indent = 16`, `icon_width = 16`, `tooltip_width = 360`. Panels use `Frame::new().inner_margin(...)` per component, not the global margin.

**Visuals** (`style.visuals`): `dark_mode`, `override_text_color = Some(text)`, `panel_fill = panel`, `window_fill = panel`, `extreme_bg_color = raised` (TextEdit background), `faint_bg_color = raised` at low alpha (striped tables), `code_bg_color = raised`, `hyperlink_color = cyan`, `warn_fg_color = gold`, `error_fg_color = red`, `selection.bg_fill = copper tint`, `selection.stroke = Stroke::new(1.0, copper)`, `window_corner_radius = CornerRadius::same(6)`, `menu_corner_radius = same(4)`, `window_stroke = Stroke::new(1.0, line)`, `window_shadow = Shadow::NONE`, `popup_shadow = Shadow::NONE`, `striped = false`.
`widgets.noninteractive`: `bg_fill = panel`, `bg_stroke = Stroke(1, line)`, `fg_stroke = Stroke(1, text)`, `corner_radius = 4`.
`widgets.inactive`: `bg_fill = raised`, `weak_bg_fill = raised`, `bg_stroke = Stroke(1, edge)`, `fg_stroke = Stroke(1, text)`, `corner_radius = 4`, `expansion = 0`.
`widgets.hovered`: `weak_bg_fill = hover`, `bg_fill = hover`, `bg_stroke = Stroke(1, edge)`, `fg_stroke = Stroke(1, text)`, `expansion = 0`.
`widgets.active`: `weak_bg_fill = raised`, `bg_stroke = Stroke(1, copper)`, `fg_stroke = Stroke(1, text)`, `expansion = 0`.
`widgets.open`: same as active (open combo box).
Set `expansion = 0` on all so hover does not grow widgets.

**Light theme**: same function with the light `Tokens`; `dark_mode = false`. Switch with `ctx.set_visuals` and store the choice in Preferences (already exists: `Preferences.dark`). The canvas painter reads a separate `CanvasColors` struct, also per theme, so Layers tree swatches and canvas share one source.

**Small widget helpers** (module `ui_kit`): `primary_button`, `secondary_button`, `ghost_button`, `icon_button`, `segmented`, `tab_bar`, `list_row`, `finding_row`, `form_row`, `number_field`, `select`, `empty_state`, `status_item`, `card`, `panel_header`, `chip`, `badge`, `focus_ring`. Every helper takes a `label: &str` and a `tip: &str` and returns a `Response` with `widget_info` set (accessible name). A test walks the AccessKit tree and fails on an unlabeled interactive node (the existing `accessible-name-exemptions.txt` e2e does this today; the exemption list must shrink to zero).

**Dock**: `egui_dock::DockArea` with `Style::from_egui(ctx.style())` then override `tab_bar.bg_fill`, `tab.active.*` and `separator.*` from tokens. The centre area uses the dock's central surface, asides and bottom sheet are dock nodes with fixed splits: left 320, right 340, bottom 232 (36 collapsed). Tab nav, menu bar, tool bar and status bar are plain `TopBottomPanel`s outside the dock. Check the egui_dock release against the egui version before relying on any API named here.

**Icons**: ship as SVG files under `crates/ui-egui/assets/icons/` and load with `egui::Image` plus `include_image!` (needs the `svg` loader), tinted with `.tint(color)`; or as `epaint` paths in code. 

**Motion**: none beyond egui defaults; respect the existing reduce motion preference.

## 7. Dialogs

Project settings and Preferences are modal dialogs: dimmed scrim, 1 px `edge` border, radius 6, header (title, close button), a tab row at the top of the dialog (never a side list), scrolling body of cards and form rows, footer with a left note and right-aligned buttons. Project settings footer: Cancel, Save. Preferences applies at once, footer: Reset to defaults, Done. Build them on `crates/ui-egui/src/ui_kit/dialog.rs`, not a second implementation.

## 8. Menus and context menus

Menu panel: `panel` fill, 1 px `edge` border, radius 4, items 28 high with 12 px padding, shortcut hint in Mono 11 `muted` at the right, 1 px dividers between groups, hovered item on `raised`.

**Right-click menus** exist on canvas objects and empty canvas, tree rows, library cards and findings rows. Every one is built through `crates/ui-egui/src/ui_kit/menu.rs` (radius 6, soft shadow, icon column, shortcut at the right, destructive group last); no other code draws a context menu. Each row runs the same command as the tool bar or menu bar and names its shortcut.

**File menu:**
- New project, Open (opens a project), Open recent (submenu).
- Save and Save as write the whole project (native `.pcbcraft`, or another whole-project format).
- Import (submenu) brings things into the project. Export (submenu) makes things from the project (bill of materials, plot files, PDF, STEP, packages).
- Import and Export submenus list Recent (up to five, remembering the format and its options), then Common, then More…. Every entry opens the same dialog; a specific entry opens it preconfigured. The format lists come from the one format registry in `crates/io/src/formats.rs`, so never add a menu item per format. Packages (named multi-format bundles for a vendor or fab) are a group inside Export.
- Project settings, Close project.

**Edit menu:** Undo, Redo, Cut, Copy, Paste, Delete, Renumber parts, Update layout from schematic, Preferences.

## 9. Companion app alignment

PCBCraft pins the shared UI library of the companion CAD app. Anything that library already ships (theme tokens, dialog containers, slider, row helpers, icon ramp) is reused, not rebuilt; the tokens here are the PCBCraft values fed into that token structure. Where a number here differs from the shared library, the library wins. The Manufacture tab follows the companion app's Manufacture workspace model: outputs tree, machine profiles, preview, per-output properties. 3D models (board and component) import from and export to the companion app's projects.

## 10. Manufacture tab components

- Output row: 44 high, two lines (name, machine in muted 11), status badge (Current in `mask` tint, Stale in `gold` tint, Not made in neutral), eye cell for preview. Layer sub-rows under Gerber files use a colour swatch and an eye.
- Preview floats: Show in preview checklist (colour swatch per layer or tool) at the upper left, estimate card (time, path length) at the upper right, zoom and Fit at the lower right.
- Tool colours in toolpath previews: V bit cyan, drill gold, end mill orange (canvas colours, fixed per tool role). Rapid moves dashed `muted`.
- Findings from manufacturability checks appear in the bottom sheet Findings tab, not in the page.

## 11. Layers tree, Layout view card, tabs, and the mockup table

**Layers tree.** Each row is a physical board layer with sublayers (silkscreen, copper nets, components). Groups on Layout: Copper, Silkscreen, Mask and paste, Board, User layers, Parts, Nets. Selecting a row sets what the drawing tools draw on (Line on silk is silkscreen, on copper a track, on the edge the outline); there is no layer drop-down in the tool bar. Wires and nets are one thing: a net row expands to its wires, tracks and vias, with no separate Wires group. Top-level Components and Nets branches list every part and net across layers. Adding and editing layers happens in the tree. Rows lock and hide with the lock and eye cells; locking rows is how you limit what can be picked, so there are no selection-filter chips (Parts, Copper, Graphics, Text) in the tool bar. Sheet category rows on Schematic (Symbols, Power ports, Drawing, Title block) have working eye, lock, Properties and right-click; fixed categories cannot be removed and empty ones are hidden by default. Hovering a layer or sublayer previews it (everything above goes semi-transparent); clicking pins that view until you click the same row again, press Esc or press Reset. Page Up and Page Down step through sublayers in physical order.

**Layout view card.** Floating at the top right of the Layout canvas: 2D | 3D, Top | Bottom, rotate 90 degrees left, rotate 90 degrees right, reset. Bottom is the flip: the view from below, mirrored, with reversed z-order. The rotate and reset buttons are icon buttons in the same style as the 3D controls. There is no "From below" button. The canvas paints layers in physical z-order from the viewer's side, with the active layer at full strength, layers above it semi-transparent and layers behind it dimmed.

**Panel tabs.** The left aside keeps Layers | Library tabs on Schematic and Layout: the Library panel finds and places parts. The Library tab in the tab nav is a separate full workspace (part browser, datasheet reader, vendor prices, source editing, symbol and footprint editors, compare view).

**Files tab.** Its own workspace: the left aside lists New, Open, Save, Save as, Import, Export, Project settings, Preferences and Close; the centre shows Recent projects. The Library never appears there.

**Mockup table.** Accepted mockups are attached to these issues on wdunn001/pcbcraft and are not committed. Open the issue to see the image.

| Area | Issue |
| --- | --- |
| Library tab: part browser and data | #699 |
| Library tab: datasheet reader | #695 |
| Library tab: 3D viewport | #702 |
| Library tab: compare and part tabs | #713 |
| Library tab: part editor | #722 |
| Library filter segmented control | #895 |
| Shell frame (menu bar, tab nav, status bar) | #607 |
| Tool bar per tab | #608 |
| Right aside Properties and Assistant | #609 |
| Bottom sheet | #610 |
| Files tab | #612 |
| Schematic page | #613 |
| Layout page and view card | #614 |
| 3D view | #615 |
| Project settings dialog | #617 |
| Preferences dialog | #618 |
| Manufacture tab | #621 |
| Layers tree | #597 |
| Workspace contract | #639 |
| UI kit (buttons, segmented, tabs) | #602, #603 |
| Right-click menus | #682 |
