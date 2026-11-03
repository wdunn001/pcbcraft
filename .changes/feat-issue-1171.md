# Port direction picker

Ports are drawn as monospace `[name]` text on schematic pages but cannot be
clicked or configured. This change makes ports selectable and adds a signal
direction picker (Input, Output, Both, Passive) to the Properties panel.

**What changed:**
- Ports are now hit-testable on the schematic canvas (8 px tolerance, zoom-aware).
- Clicking a port in Select mode selects it.
- The Properties panel shows a Port section with the port name (read-only) and a
  Direction segmented picker.
- The `schematic.set_port_direction` engine command sets the direction on a port.
- The `Port` document type gains a `direction` field (defaults to "bidirectional").
- Undo/redo works for direction changes.

**Files changed:**
- crates/doc/src/lib.rs - added `direction` field to `Port`
- crates/engine/src/lib.rs - added `schematic.set_port_direction` command
- crates/ui-egui/src/canvas/gestures.rs - added `Hit::Port` and hit detection
- crates/ui-egui/src/tool_modifiers.rs - added `Target::Port`
- crates/ui-egui/src/schematic_port.rs - added `port_under`, `select_port_hit`, `port_controls`
- crates/ui-egui/src/properties.rs - call `port_controls` for selected objects
- crates/text/strings/en.txt - added port direction strings
- crates/ui-egui/src/e2e/schematic_labels.rs - added `a_newcomer_sets_a_port_direction` story

**Acceptance:**
- `a_newcomer_sets_a_port_direction`: place a port, select it, set direction to Output,
  verify it is stored, undo and verify it returns to bidirectional.
