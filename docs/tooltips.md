# Tooltip standard

Every control in the app has a tooltip, and a good tooltip answers the questions a first-time user has when the pointer rests on it. This page says what goes in one. The check that every control has a tooltip at all is the unit test `every_control_hovers_a_plain_sentence_for_a_first_time_user` in `crates/ui-egui/src/lib.rs`. The quality lint in #618 mirrors the checklist at the end of this page.

## The four parts

A tooltip is at most two short lines: two sentences, about 220 characters. It carries up to four parts, in this order.

1. **What.** One plain sentence saying what the control does. Always present.
2. **Why or when.** A short second sentence saying when to use it or what it is for. Present unless the control is obvious from its label and its what (Zoom in, Close).
3. **Shortcut.** The key that does the same thing, when one exists. Read it from the shortcut table (`shortcut_rows()` in `crates/ui-egui/src/lib.rs` today, the keymap from #315 once it lands) and never type a key by hand, so the tooltip cannot drift from the real binding.
4. **Limits.** What the control refuses or caps, when it has bounds: a range, a unit, a required selection, a file it will not overwrite. Say the limit in the same units the field shows.

If all four will not fit in two lines, cut the why, not the limit.

## Wording

- Plain sentences a person who has never seen the app can follow. Start the what with a verb: "Write the design to its file", not "Saving".
- Do not repeat the label. A tooltip on "Save" that says "Save" teaches nothing.
- No jargon without a glossary link. Link a glossary entry when one exists (#398); until then, explain the word in the sentence or pick a plainer one.
- No em-dashes, no fancy dashes, and no names of other design tools or their file brands.
- Use the unit the user sees ("millimetres"), not an internal one.

## Where the text lives

- Controls backed by an engine command take their sentence from the command registry through `command_tip(id)` in `crates/ui-egui/src/lib.rs`. The registry description is the what. The control adds the why, the shortcut and the limits in the same call.
- Every other tooltip is a literal next to `on_hover_text`. After the text catalog lands (#349) it is the catalog entry named after the control key plus `.tip`, and the literal goes away. Write new tooltips in the same call shape so the move is mechanical.

## Disabled controls

A disabled control uses `on_disabled_hover_text` and says what to do to enable it, in the form "Nothing to X yet: do Y first." Never leave a disabled control silent, and never explain only why it is off.

## Examples

Each example uses a control that exists in the app. Where today's text falls short, both are shown and the corrected one is the target.

**A button: Save** (`crates/ui-egui/src/lib.rs`, File menu)

Today: "Write the design to its file on disk. (Ctrl+S works too.)"
Good: "Write the design to its file on disk. Ctrl+S does the same."
The what and the shortcut are present. The shortcut comes from the table, not from a typed string.

**A toggle: hide the library dock** (`crates/ui-egui/src/lib.rs`, Library dock header)

Today: "Close the library dock."
Good: "Close the library dock to give the canvas more room. Open it again with the Lib toggle in the top bar."
Today's text is the what only. The corrected text adds the why and the way back.

**A text field: part search** (`crates/ui-egui/src/lib.rs`, Add a part)

Today: "Type any part name; the list below narrows as you type."
This one already meets the standard: the what, and the effect the user will see. No shortcut and no bounds, so there is nothing to add.

**A canvas handle: the 3D board** (`crates/ui-egui/src/board3d.rs`)

Today: "3D board. Drag to turn it, right-drag or Shift-drag to move it, scroll to zoom, click a part to select it."
Good: "Drag to turn the board. Right-drag or Shift-drag moves it; scroll zooms."
A canvas control has several gestures, so the what lists them. Keep the gestures that matter and drop the rest to the help page. The label "3D board" is dropped because the tab already says it.

**A disabled control: Undo** (`crates/ui-egui/src/lib.rs`, toolbar)

Enabled: "Step back one action; every edit here is one undoable step."
Disabled: "Nothing to undo yet: make an edit first."
The disabled text says what to do to enable the button. The same pattern applies to any action that needs a selection: "Select at least two parts to line them up."

## Checklist

A tooltip passes when all of these hold. #618 turns each into a lint rule.

1. At most two lines, about 220 characters.
2. The what is a plain sentence that does not just repeat the label.
3. A second sentence gives the why or when, unless the control is on the short obvious list.
4. A control with a shortcut shows it, read from the shortcut table.
5. A control with bounds states the limit.
6. A disabled state says how to enable the control.
7. No em-dashes, no tool brand names, no unexplained jargon.
