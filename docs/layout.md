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

## pad.add, pad.edit and pad.remove

These commands add, change and remove single copper pads from the canvases. The Layout Pad tool, Add pad in the right-click menu of a placed part and the Pad item of the Schematic strip send them.

- `pad.add` with `position` makes a free pad: a one-pad part (kind `test_point`, reference `TP1`, `TP2` and so on) at that board position. With `schematic_position` the part also sits on that sheet, and a board position is optional; the board pad shows on the same net. With `footprint` it adds a pad to that placed part instead, at `offset` (local) or `position` (board), and the part gains a pin with the same number.
- Options: `number`, `shape` (circle, rect, oval, roundrect), `width`, `height`, `drill`, `side` (top, bottom or through) and `net`. A surface pad has no drill; a through-hole pad needs one smaller than the copper.
- `pad.edit` changes one pad of a placed part by its current number; the pin follows the number and the net. `release_net` takes the pad off its net.
- `pad.remove` removes one pad and its pin. The only pad of a free pad part removes the part. Any other part keeps at least one pad.

A free pad is an ordinary part, so clearance checks, net connectivity, Gerber, drill and placement output, undo and saving treat it like any other pad.
