# Flux patterns to adopt (recon of flux.ai, Oct 28 2026)

Recon notes from the live Flux app (pico-macro-keyboard example) and
docs.flux.ai. Each section ends with the pcbcraft plan. Nothing here is
binding; PRs stay one-feature-each and stack on the current tip.

## 1. Their shell layout (user decisions recorded)

- Top bar: project tab strip with Files / Schematic / Layout / 3D View
  document tabs, plus Share and account at the right.
- Left: AI chat drawer with a thin icon rail (home, lists, layers, and a
  live severity stack: 10+ errors, 2 warnings, 10+ info as colored counts).
- Middle-left floating Library panel: search box, filter icon row, rich
  part cards (thumbnail, name, description, tags, author, usage count,
  stars, MPN, stock range with freshness, price range).
- Right project drawer with stacked panels: Profile (inspector), Library,
  Objects (tree), View, Review.
- Bottom-right: collapsible "Project" accordion.

William's decisions for PCBCraft (saved to shared memory):
- The top menu keeps working like SolidCraft's (brand + File/Edit/Help),
  NOT like Flux's document-tab strip.
- No project accordion/modal in the corner; skip it.
- Any assistant/chat surface lives on the RIGHT and collapses to a single
  chat icon, never a left drawer.

## 2. Component library: where their data comes from

Data chain discovered from live network traffic:
- `app.flux.ai/ugc/set-claims` serves user-generated content "sets":
  curated packs (e.g. #CommonPartsLibrary) published by named authors.
- `app.flux.ai/accounts/auth/typesense` hands out a search-only key; part
  search runs against a Typesense cluster (fuzzy keyword + exact MPN
  search over ~1M records).
- Firestore (`graviton-mvp` project) is the live-sync store for projects
  and canonical part records.
- `shortcircuit-cdn.flux.ai` is the CDN for the official "Short Circuit"
  library assets (symbols, footprints, 3D models), auth-gated.
- Distributor data (MPN list, stock range + "2d ago" freshness, price
  breaks) rides on each part record.

Record shape shown in the UI: name, long description, value lists, tags,
author handle, usage count, stars, MPN variants, stock, price, thumbnail.
Ownership works like GitHub: every part belongs to a user or organization
with public / shared / org / yours visibility; parts can be forked and
published; JEP30 PartModel gives parametric parts.

PCBCraft plan:
- Adopt the card shape and the set/pack grouping in the Parts dock:
  each part shows plain description, tags, and origin; group library
  entries into named sets the user can toggle.
- Adopt "search by keyword or part number" wording and the owner filter
  idea later (local library has no authors yet; add `author`/`tags`
  fields to library records as a doc change when it pays rent).
- Skip Typesense/Firestore: our library is local files; keep a plain
  substring + fuzzy search in the UI.

## 3. Validator system (the pattern we want, re-skinned)

Their model, from docs + the live inspector:
- ONE mechanism: named, typed "layout rules" that both CONSTRAIN
  (guardrails) and MODIFY the design directly.
- Two application styles: object-specific rules (attached to one object,
  edited in the inspector) and selector-based rules (a selector like
  `#[U3]/1` or `fills(1.5mm)` matches many objects; edited in a Rules
  menu).
- Inheritance and precedence: the layout root holds system defaults;
  nets and classes override; objects override again. The inspector shows
  every value twice: **Specified** (what the user set) and **Computed**
  (the effective value after inheritance).
- Checks are mostly clamps on rules: every constructive rule (Hole Size,
  Corner Radius, Font Size, Trace Width...) has Min/Max clamping
  variants; violating a clamp IS the DRC finding.
- Real-time DRC + ERC with named checks, each with a description and a
  suggested fix: Overlapping Traces, Airwires (unrouted), Missing
  Footprints, Invalid Layer, Floating Copper, Multiple Fills per Layer,
  Component Overrides; ERC: unconnected power pins, missing decoupling,
  missing I2C pull-ups, single-pin nets.
- Results live in a **Review panel grouped by category** (e.g. Stackup
  Areas) plus a messages menu; the chat/agent can "run checks, fix,
  re-check" in a loop.
- Rules can be phrased in plain language and typed values:
  `0.5mm fills(1.5mm)` = default 0.5 mm but 1.5 mm against fills.

PCBCraft plan (William's decisions, saved to shared memory):
- Keep our existing typed engine rules; do NOT copy their selector
  language wholesale.
- Surface check results in TWO places: a footer status strip (error /
  warning / info counts, click to jump) and the Checks tab reorganized
  as a Review section grouped by category (Electrical, Copper,
  Manufacturing, Assembly).
- Add user-authored custom checks stored in the document: a name, a
  plain-language note, what to measure (pair clearance, part height,
  track width, copper-to-edge), the limit, and the severity. The engine
  evaluates them with the same reports the built-ins use.
- Inspector later gains the Specified vs Computed idea where we already
  have defaults (clearance per pair, board rules): show the effective
  value and where it came from.

## 4. Layer anatomy: the missing tree (top priority)

What Flux shows that we do not:
- An **Objects tree** (Objects panel) whose nodes expand down to every
  object on the board, each carrying full properties in the inspector,
  including a first-class **stackup table**: every layer with type
  (Signal / Dielectric / Solder Mask / Solder Paste / Overlay), material
  (Copper, Prepreg, Core, PbSn...), thickness, copper weight (oz) and
  dielectric constant, from named presets like "Standard 4 Layer".
- Silk screen, mask and paste layers are real objects with their own
  visibility and properties, not draw toggles.
- Nets appear as tree nodes you can attach rules to directly.

PCBCraft gap and plan (stacked PRs, in order):
1. **Board anatomy tree** in the Layers dock: expandable rows Board ->
   layer (Top copper, Bottom copper, Silk top, Silk bottom, Mask, Edge,
   Notes) -> kind group (wires, vias, pads, copper zones, holes,
   cut-outs, words, stricter areas, watched pairs) -> individual object
   with its key numbers. One click selects it on the canvas; each layer
   row gets an eye toggle for display. Pure doc traversal, no engine
   change. This is the "show each piece of a layer" fix.
2. **Stackup editor**: turn the Layers dock finish/thickness fields into
   a Flux-style stackup table (name, what it is, material, thickness,
   weight) with presets ("Plain two layer", "Standard four layer");
   needs a small engine addition to store named stackups on the board.
3. **Footer status strip**: counts of errors/warnings/info (the severity
   stack pattern), current tool, cursor position, zoom; clicking a count
   opens the Checks tab filtered to that level.
4. **Review re-skin**: Checks tab groups findings by category with the
   per-category counts, keeps Hide/Quiet controls per finding.
5. **Custom checks**: document-stored user rules + engine evaluation
   (typed command `checks.custom_add` with a bounded measure enum).
6. **Specified vs Computed** inspector rows where defaults exist.

## 5. Things deliberately NOT copied

- Firestore/Typesense/UGC cloud plumbing; our engine is local-first.
- The document-tab strip at top (SolidCraft menu wins).
- The corner project accordion (not needed).
- The left AI drawer (assistant, when it comes, docks right and
  collapses to an icon).
