# Layout commands

Read-only layout commands share one `scope` object and return plain JSON in a fixed order.

## Scope

`{"layers": ["F.Cu"], "nets": [4], "components": [1], "respect_locked": true}`

- Give at least one of `layers`, `nets`, `components`. An empty scope is refused; there is no implicit whole board. To mean everything, list every copper layer.
- Lists that are given narrow the scope together: a part must match each one. A part matches a layer by the side it is mounted on, a net when one of its pads is on it, a component by its id.
- Each list holds at most 1024 entries. Unknown layers, nets and components are refused.
- `respect_locked` (default true) marks locked parts as not movable. Parts outside the scope are obstacles and are never moved.

## layout.report

Input: `{"scope": {...}}`. Changes nothing, adds no undo step, same input gives the same bytes. Numbers are millimetres rounded to 0.0001.

Output keys:

- `parts`: in-scope parts sorted by reference: `footprint`, `component`, `reference`, `x`, `y`, `rotation`, `side`, `locked`, `movable`, `box` (`min_x`, `min_y`, `max_x`, `max_y` around pads and drawn graphics).
- `gaps`: for each in-scope part, its nearest neighbour on the same side and the box-to-box gap (0 when touching or overlapping).
- `alignment_groups`: rows (`axis` "y") and columns (`axis` "x") of two or more parts whose centres agree within 0.05 mm.
- `nets`: for each net with a pad on an in-scope part: `pads`, `hpwl_mm` (half-perimeter of the pad box), `ratsnest_mm` (spanning tree over pads, ignores copper), `unrouted_mm` (airwires still open after existing copper).
- `totals`: the three wirelength sums over `nets`.
- `off_board`: parts whose box leaves the board outline.
- `overlaps`: pairs on the same side whose boxes overlap, where at least one is in scope.

The wirelength functions live in `crates/engine/src/wirelength.rs` (`hpwl`, `ratsnest_length`, `net_pad_points`) so placers can reuse them.

## rules.decoupling_set and missing_decoupling

`rules.decoupling_set {"max_distance_mm": 3.0, "suggested_value": "100 nF"}` (at least one field; distance 0.1 to 50 mm) stores the settings on the project. The `missing_decoupling` check then reports, for each power input pin of a part that is not a capacitor, either no capacitor on the net, or the nearest placed capacitor being farther than the distance (pad centre to pad centre). Ground-named nets are skipped. The finding names the part, pin, net and the value to add. Severity follows `rules.severity` like every code (default warning).
