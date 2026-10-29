# Workbench layout and patterns (Oct 28 2026)

Design notes for the PCBCraft shell, library dock, checks, and board
anatomy. Each section states owner decisions and the planned implementation.
Nothing here is binding; pull requests stay one-feature-each and stack on
the current tip.

## 1. Shell layout (owner decisions)

Target workbench regions:

- **Top:** application menu with brand mark, **File**, **Edit**, and **Help**.
  No document-tab strip across the full width for Files / Schematic / Layout /
  3D View; view switching stays in the existing workbench tab model.
- **Left:** parts library dock with search, filters, and rich part cards
  (thumbnail, name, description, tags, origin, usage metadata where we store
  it).
- **Center:** schematic or board canvas with the usual tool strip.
- **Right:** stacked dock panels (inspector, objects tree, checks, assistant
  when enabled). Any assistant or chat surface lives on the **right** and
  collapses to a single chat icon, not a left drawer.
- **Bottom:** footer status strip (see section 4).

William's decisions (saved to shared memory):

- Keep **File / Edit / Help** at the top; do not replace the menu with a
  project document-tab strip.
- No project accordion or modal in the corner; skip it.
- Assistant docks right and collapses to an icon.

## 2. Component library (local-first)

Card shape and grouping in the Parts dock:

- Each part shows plain description, tags, and origin.
- Group library entries into named **sets** the user can toggle (curated
  packs without cloud sync).
- Search by keyword or part number; owner and visibility filters can follow
  once local records carry `author` and `tags` fields.

PCBCraft stays local-first:

- Library data lives in project and on-disk files, not a hosted search index
  or realtime document store.
- The UI keeps substring and fuzzy search over the installed library.

## 3. Rules, checks, and review

Engine model (what we keep and extend):

- One mechanism: named, typed layout rules that constrain the design and, where
  appropriate, modify it through commands.
- Object-specific rules attach to one object and edit in the inspector;
  board-level and class rules override defaults with clear precedence.
- The inspector shows values twice where inheritance matters: **Specified**
  (what the user set) and **Computed** (effective value after defaults and
  overrides).
- Checks clamp or evaluate those rules: clearance, geometry limits, routing
  completeness, footprint assignment, layer validity, copper pours, and ERC
  items such as unconnected power, missing decoupling, and single-pin nets.
- Results belong in a **Review** presentation grouped by category, plus quick
  access from the footer counts.

PCBCraft plan (owner decisions, saved to shared memory):

- Keep existing typed engine rules; do not introduce a second selector
  language parallel to commands.
- Surface findings in **two** places: footer status strip (error / warning /
  info counts, click to jump) and the Checks tab reorganized as Review sections
  grouped by category (Electrical, Copper, Manufacturing, Assembly).
- Add user-authored **custom checks** stored in the document: name, plain
  language note, what to measure (pair clearance, part height, track width,
  copper-to-edge), limit, and severity. The engine evaluates them with the
  same reports as built-ins (`checks.custom_add` with a bounded measure enum).
- Inspector rows gain Specified vs Computed where defaults already exist
  (clearance per pair, board rules).

## 4. Layer anatomy, stackup, and footer (priority stack)

Gaps today:

- No expandable **objects tree** from board down to individual items with
  one-click canvas selection.
- Stackup fields are not yet a full table with presets.
- No footer strip tying tool, cursor, zoom, and severity counts to the Checks
  tab.

Planned work (stacked pull requests, in order):

1. **Board anatomy tree** in the Layers dock: Board → layer (top copper, bottom
   copper, silk, mask, edge, notes) → kind group (wires, vias, pads, zones,
   holes, cut-outs, text, rule areas, watched pairs) → individual object with
   key numbers. One click selects on the canvas; each layer row gets a
   visibility toggle. Pure document traversal, no engine change.
2. **Stackup editor:** turn finish/thickness fields into a table (name, layer
   kind, material, thickness, copper weight) with presets such as plain two
   layer and standard four layer; store named stackups on the board in the
   engine.
3. **Footer status strip:** error / warning / info counts, current tool,
   cursor position, zoom; clicking a count opens Checks filtered to that level.
4. **Review re-skin:** Checks tab groups findings by category with per-category
   counts; keep Hide and Quiet per finding.
5. **Custom checks:** document-stored user rules plus engine evaluation.
6. **Specified vs Computed** inspector rows where defaults exist.

Tree targets for stackup rows include layer type (signal, dielectric, solder
mask, solder paste, overlay), material, thickness, copper weight, and
dielectric constant from named presets. Silk, mask, and paste behave as real
layers with visibility and properties, not only draw toggles. Nets appear as
tree nodes that can carry rules.

## 5. Deliberately out of scope for these notes

- Cloud library sync, hosted search, or UGC claim endpoints; local files win.
- A full-width document-tab strip instead of File / Edit / Help.
- Corner project accordion.
- Left-docked assistant drawer.
