# Tool modifiers and tool keys

Shift, Ctrl and Alt change what the active tool does. On macOS the primary key is Cmd and Alt is Option; Ctrl on a Mac is not a modifier here. Every key below can be changed in Edit > Preferences > Keys, and changes apply at once and are saved.

The resolver lives in `crates/ui-egui/src/tool_modifiers.rs`. Canvas code never reads Shift, Ctrl or Alt itself; a scan test (`no_raw_modifier_reads_outside_resolver`) enforces it. The idea follows the owner's sibling app SolidCraft.

## Where you can see them

- The tool bar tooltip of each tool lists its key and modifiers.
- The right-click menu ends with the modifier line for the active tool.
- The status bar shows what a click does right now for the thing under the pointer, such as "Click: start wire from R1.2" or "Ctrl+click: delete connection", and the effect of every held key, such as "Shift: 45° only".
- The cursor and an animated outline on the hovered object show what can be acted on. With Reduce motion on, the outline is static.

## What you press on decides (Select tool)

| Press on | Click | Drag |
| --- | --- | --- |
| Schematic pin | start a wire | start a wire to another pin |
| Board pad | start a route | start a route to another pad |
| Track end | select | pull the end out with clearance |
| Wire or track segment | select | move the segment |
| Part | select | move the part |
| Empty canvas | clear selection | box select |
| Net label | select (double-click renames the net) | box select |

## Hotkeys (remappable)

W wire (schematic), X route (board), V via (board), T text (board), Esc back to Select.

## Modifiers

| Tool | Shift | Ctrl or Cmd | Alt or Option |
| --- | --- | --- | --- |
| Select (click) | add to selection | toggle in selection | pick what is underneath |
| Select (drag) | one axis only | copy | no snap |
| Wire | 45° preview | no snap; click a node or junction to delete the connection | other corner in the preview |
| Route | other corner style | no snap; click a track end or via to remove it | shove or walk around |
| Move | one axis only | copy | no snap |
| Draw line, text, Measure | 45° only | no snap | none |
| Place, Via, Hole, Cutout, Fiducial | none | no snap | none |
| Mouse wheel | pan sideways | fine zoom | none |

Known limit: the engine's wire connect command has no corner option, so Shift and Alt in the Wire tool change the preview only.
