---
type: feature
area: layout
issues: [62]
user_visible: true
docs: [crates/engine/src/lib.rs]
summary: Route and check copper on inner layers, including through vias that change layer.
---

- Copper checks, airwires and clearance now key copper by its copper layer, so a through via conducts on every copper layer of its span while a pad and a zone conduct on their own layer.
- `track.route` takes `layer` and `end_layer` and places a through via at the bend in the same undo step when they differ, sized from the net class or the board default.
- `zone.place` accepts any copper layer of the current stackup including a plane layer.
- The canvas paints the active copper layer last at full colour and dims every other copper layer, so inner copper never hides the layer being routed.
