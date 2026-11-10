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

## rules.stitch_distance_mm and the return path check

`rules.stitch_distance_mm {"distance_mm": 2.0}` sets how close a ground stitching via must sit to a layer change on a fast net, from 0.1 to 50 mm. The `return_path` check reports a layer change that has no ground via within that distance, and the finding names the distance in its message. A new design uses 2 mm, and that default is not written to the saved file, so a file saved before this setting existed loads with 2 mm. The change is one undo step. A value outside the range, or one that is not a number, is refused and the design stays as it was.

## pad.add, pad.edit and pad.remove

These commands add, change and remove single copper pads from the canvases. The Layout Pad tool, Add pad in the right-click menu of a placed part and the Pad item of the Schematic strip send them.

- `pad.add` with `position` makes a free pad: a one-pad part (kind `test_point`, reference `TP1`, `TP2` and so on) at that board position. With `schematic_position` the part also sits on that sheet, and a board position is optional; the board pad shows on the same net. With `footprint` it adds a pad to that placed part instead, at `offset` (local) or `position` (board), and the part gains a pin with the same number.
- Options: `number`, `shape` (circle, rect, oval, roundrect), `width`, `height`, `drill`, `side` (top, bottom or through) and `net`. A surface pad has no drill; a through-hole pad needs one smaller than the copper.
- `pad.edit` changes one pad of a placed part by its current number; the pin follows the number and the net. `release_net` takes the pad off its net.
- `pad.remove` removes one pad and its pin. The only pad of a free pad part removes the part. Any other part keeps at least one pad.

### Pad kinds

`kind` picks what the pad is for. In Layout, the Pad tool and the Pads section of Properties have a Kind list.

- `plain` is the ordinary pad: open mask, paste printed.
- `exposed` is bare copper for contacts such as test pads, card edge fingers and spring contacts. The mask is open and no paste is printed. It is a surface pad (top or bottom), not a through-hole pad.
- `notched` is a rectangle with one corner or edge cut out. Give `notch_place` (`corner_nw`, `corner_ne`, `corner_se`, `corner_sw`, `edge_n`, `edge_e`, `edge_s` or `edge_w`), `notch_width` and `notch_depth`; a missing value is a quarter of the pad. An edge notch is centred and stays under half the pad deep; a corner notch must leave 0.05 mm of the pad in width or height. `paste` (`full` or `none`) belongs to a notched pad. The shape is stored as the pad's polygon, so saving, loading, undo and the Gerber copper, mask and paste outputs follow it.

The kind is read back from the stored layers and polygon, `pad.table` lists it, and resizing a notched pad scales its notch with it. The checks hold a notched pad to the board edge by its real outline. Two further checks cover the new kinds: `mask_web` (warning) when the mask between an exposed or notched pad and its neighbour is thinner than 0.1 mm, and `notch_feature` (error) when a notch or the copper arm beside it is thinner than the board's minimum track width (0.1 mm without fabrication rules).

Half pads (clipped by the board edge) and castellated hole rows are not part of this slice. The `footprint.set_castellated` flag stays as it was.

A free pad is an ordinary part, so clearance checks, net connectivity, Gerber, drill and placement output, undo and saving treat it like any other pad.

## Hiding and picking a layer

The eye on a layer row hides everything that belongs to that layer: its tracks, zones, pads, part outlines and courtyards, reference and value labels, text, drawn board shapes, the vias that only reach hidden layers, and the ratsnest lines whose ends are all on hidden layers. A part mounted on a hidden side goes as one thing. Click the name of a layer row to pick it: its elements stay at full strength, everything else dims, and the hint says what the layer holds ("Top copper: 6 tracks, 3 pads, 6 silkscreen items, 2 vias shown"). Click the name again to clear it. Only the arrow opens or closes a layer row. The focus button still dims everything else without picking the row.

## Via tool

Arm Via and click anywhere inside the board outline to drop a via. The via takes the net of the track, pad or via under the cursor; on bare board it takes the net picked in the tool options, and with Auto it has no net. A via with no net is reported by the checks ("Via with no net") until you give it one in Properties. The options hold the Net, the via type (through, blind or buried, with the layers it joins on boards that allow them), the size and drill (they start from the net class or the fabrication rules), and Snap. A ghost via follows the cursor in the colours of the layers it joins and turns red, with the reason beside it, where the via would sit outside the board, too close to the edge, with too thin a copper ring, or too close to other copper. The status bar says what was placed or why not. The options sit in the right-hand panel for now.

## Route on any copper layer

The Layers tree is the one place to pick the layer. Click the row of a copper layer (Top, each inner layer by the name the stackup gives it, or Bottom) and it gets the Active tag; new tracks, routes and pours go there. The `]` and `[` keys step to the next and previous copper layer and wrap at the ends. Adding or removing an inner pair keeps you on the bottom if you were on the bottom.

Commands that name a layer take `F.Cu`, `B.Cu` (always the last copper layer), `In1.Cu` and so on, or a stackup row name. The checks and the unrouted-connection list read every copper layer, and a plated through-hole pad joins a track on any of them.

A route can start or finish on a via as well as on a pad, by click or by drag. A board needs this once its surface-mount pads have escape vias beside them. Such a pad holds copper on one side only. The run has to leave from the via instead. A via carries its own net. Where both ends of a route are vias, that net travels to the route command with them. Whichever of the two the pointer is nearer wins. A pad does not win on being a pad. An escape via sits a fraction of a millimetre from the pad it serves. A pad-first rule would swallow every via worth aiming at. Differential pairs are named by their two pads. Pair routing therefore stays pad to pad.

A route is refused outright when it starts or finishes on a pad that has no copper on the layer being routed. A surface-mount pad holds copper on one face. Routing from one onto an inner layer or the back is therefore refused, with the pad and the layer named. The refusal is drawn on the canvas beside the pointer, in red. A refused route leaves no copper behind. The gesture otherwise looks like it simply vanished. The reason stays until a command succeeds. To take such a pad to another layer, run a short stub on the pad's own layer, place a via, and carry on from the via.

A track whose end lies on a pad of its own net, where the pad has no copper on the track's layer, is reported as an error by the check named Track on a layer its pad does not reach. The finding names the net, the track layer, the pad and the layers the pad does have. A surface-mount pad has copper on one layer only, so a bottom track that ends on a front pad is the usual case. A plated through-hole pad has copper on every layer and a via joins layers, so ends on either of those are never reported. The route command reports such a connection as failed and keeps no copper for it.

## Inner layer roles

When a board has inner copper layers, the designer can mark each one as a ground plane or a power plane, or leave it as signal routing. This is set in the Layers panel by selecting a copper layer and choosing its role. A pour drawn on a layer marked as a plane is understood as a plane pour in the checks and in the stackup report, so the user can tell at a glance that a layer carries power or ground. Only inner copper layers can carry a role; the top and bottom layers are always signal. The role travels with the layer when the stackup is reordered.
