# The Library tab

The Library tab is a work area of its own, the last tab, after Manufacture. It is where you read about
a part, compare parts, see what they cost and edit them. The Library panel stays in the left aside
of the Schematic and Layout tabs for finding and placing parts. Issues #695, #698 and #699.

## What is on screen

- **Left aside:** the same Library browser as everywhere else (search, source and kind filters,
  cards), a little wider. Picking a card, or pressing the open button that shows on a card in
  the other tabs, opens that part here. The "Browse component libraries" link at the end of the
  list opens this tab.
- **Centre:** one tab per open part (up to eight) and a Compare tab when two or more are open.
  A part has five pages: Overview, Datasheet, Vendors, 3D model and Edit.
- **Right aside:** the part's Properties. A part placed in the project edits its manufacturer,
  part number, lifecycle, compliance and supplier numbers there, one undo step per edit. A part
  that is not in the project is read-only and says how to make it editable.
- **Tool bar:** New part, Edit symbol, Edit footprint, Import, Refresh prices.

## Pages

| Page | Shows |
| --- | --- |
| Overview | Symbol, footprint and 3D previews, key parameters, lifecycle and compliance, description. A value no source gave reads "Not listed". |
| Datasheet | The datasheet link (opens in your browser), the details known for the part, and the ratings and dimensions read from the datasheet when the project holds an extraction for it. Pressing one opens the datasheet window on its page and flashes the spot it came from; the window opens only a PDF saved on this computer. A value with no recorded place has its button off and says so in its tooltip. |
| Vendors | Supplier, SKU, stock and price breaks from cached answers, each with the date it was taken, and the Sources card. |
| 3D model | The part on a small board in the board renderer, the model file, offset, rotation and scale, and the pads drawn over the model's outline. |
| Edit | The symbol and footprint editor of the Libraries page, a pin map that shows which footprint pad each symbol pin reaches, and a Pad assignments card with a pad picker for every pin and a role picker for every pad. |

## Prices come from cached answers

No live supplier connection exists yet, so Vendors says so in one plain notice and shows cached
values with their date. Refresh prices changes nothing until a connection is set up and says why.
Cached answers are read from `supplier-quotes.json` next to the online cache folder
(`pcbcraft/supplier-quotes.json` under the user's cache directory), in the format of the recorded
sample sources in `crates/library/src/sources.rs` (`FixtureSource`). Tests use a recorded fixture
and never touch the network.

Supplier numbers typed on a placed part show as rows with no price and no date until an answer is
cached. Money is never converted between currencies.

## Compare

The Compare tab puts the open parts side by side: package, pin count, value, tolerance, rated
voltage, power, temperature range, lifecycle, RoHS, REACH, price at 1, 100 or 1000 pieces,
stock and datasheet link. A row differs when its cells are not all equal; a missing value counts
as different from a value and is marked "Not listed" with an icon. Highlight differences and Only
differences are switches. The rule is `rows` in `crates/ui-egui/src/library_workspace/compare.rs`.

## 3D model

The model is edited on the footprint in the project's library with `library.footprint.set_model`
(offset in millimetres, rotation in degrees, scale), one undo step per change. A footprint that is
not in the project's library shows its model read-only. A part with no model file shows the
generated body.

## Parts from the open library

With Open library switched on, type a chip number such as STM32F103R8 in the part search and
click the card. The part is placed with its pins, the package its symbol names, and these fields
when the symbol states them: Manufacturer, MPN, Datasheet and Description. A chip number also
finds its family library, so the library name is not needed.

On the Schematic tab a card places the part on the page that was open when the card was clicked.
The placement commands `part.place` and `library.place_online` take a `sheet` page id for this.
Without one the part goes on Main.

- Many chips are stored as a small symbol that inherits its pins from a parent symbol in another
  file of the same library. Both files are fetched and both are named in the project's library
  notices (`read_symbol_with_parents` in `crates/library/src/symbol_inherit.rs`).
- Manufacturer is filled only when the symbol has it; it is never guessed. MPN is the symbol's
  own part number property, or the symbol value when the symbol links a datasheet. A generic
  symbol such as a resistor gets neither (`sourcing_fields` in
  `crates/library/src/symbol_sourcing.rs`).
- Pin alternate functions are kept in the symbol's source text and no longer stop a part from
  being placed.

## Your own Digi-Key key

Below the part search the Library dock has a Digi-Key key card. It lists the steps to make a free
developer key on the Digi-Key site and has fields for the client id and the client secret. The
secret field is masked.

- Save key keeps both values on this computer through the key store in
  `crates/library/src/credentials.rs`: the system keychain, or a plain file in the settings folder
  when the keychain cannot be opened. The card says which one is in use. A project file never holds
  the key.
- Test key runs one search with the typed values and shows Connected, or the reason the key was
  rejected. It saves nothing.
- Remove key deletes both values and returns the card to its first state.
- The card is drawn by `crates/ui-egui/src/distributors.rs`. The desktop app gives the window its
  HTTPS client and key store in `apps/pcbcraft/src/distributor_http.rs`.

## Searching Digi-Key

With a key stored, the part search in the dock also asks Digi-Key. Its cards are listed first and
name Digi-Key as their source, with stock and the first price break.

- With no key stored the dock builds no Digi-Key source and shows no error.
- Answers are kept on disk in the app's own cache folder, each with the date it was taken, and the
  dock shows that date under the cards (Digi-Key answer from 2026-11-11). An answer younger than the
  source's cache window is used again, so the same words do not ask again, even after a restart. An
  older one is asked for again. Answers older than the window are deleted when the app starts.
- Nothing secret is stored: an answer that names a key, secret or token is shown but never kept, each
  source keeps at most 128 answers, and saving or removing the key empties the cache.
- The source's own pacing applies, and a pause the distributor asks for is shown in plain words.
- The Offline switch stops Digi-Key requests. The dock then shows the saved answer with its date
  (Saved Digi-Key answer from 2026-11-11), or says no saved answer exists for a search never made.
  Prices are cached values and may have changed since their date.
- This is the desktop app only; the browser build has no distributor sources, so it keeps no answers.
- A Digi-Key card lists stock and price. It has no symbol or footprint, so clicking it places nothing.
- The sources are built by `configured_sources` in `crates/ui-egui/src/distributors.rs`, which is
  where the other distributors join the list.

## Make a resistor or capacitor from a value

The Library panel in the left aside shows a Resistor generator card and a Capacitor generator card
under the connector series (issue #757). Opening a card gives a form: the series (E6, E12, E24 or
E96), the chip size (0402, 0603, 0805 or 1206), a range of values (one decade) and a value from that
series. A line reads "Makes" followed by the part number and the value. Place on schematic, or Place
on board on the Layout tab, adds the part with its symbol and footprint to the project in one undo
step. Only values of the chosen series can be picked, and the tolerance follows the series (E6 20,
E12 10, E24 5, E96 1 percent). Resistors run from 1 ohm to just under 10 Mohm and capacitors from
1 pF to just under 100 uF. The CLI and MCP do not have a command for this yet.

## Pad assignments

The Edit page of a part in the project library ends with a Pad assignments card (issue #759). Each
symbol pin has a picker for the footprint pad it drives, and each footprint pad has a picker for its
role: electrical, mechanical, thermal or no connect. A pad chosen for a pin stays with any other pin
that already drives it, so a pad given to two pins is listed under the card title as a problem until
one pin is moved; a pad that no pin drives while its role is electrical is listed the same way. The
problems update after every pick, and each pick is one undo step. The card needs a project library
part whose symbol names a project library footprint; any other part says to copy it into the project
library first. The CLI and MCP reach the same edits through `library.pin_pad_assign` (name, pin, pad,
and `only` to keep other pins' pads) and `library.pin_pad_role` (name, pad, role). A refused call,
for an unknown part, pin, pad or role, changes nothing.

## Not built yet

- Rendering the datasheet PDF in the app (page navigation, zoom, search, selectable text).
- Symbol and footprint canvases side by side on the Edit page; today one editor shows one at a time.
- Auto-align of the model to the pads, and snapping.
- Joining the open library part to a distributor listing for stock and price by part number.

## Machines

The Library page has two tabs in its left panel, Parts and Machines. The Machines tab holds the machine library. Its left panel has a search box, a Kind list (All kinds, Pick and place, Laser, Mill, UV printer, Reflow oven), a Source list (Built in, My machines, This project, Shared folder) with a count on each row, and Show hidden machines. Add machine and Import are pinned at the bottom of the panel. The centre is a two column grid of machine cards. The right panel shows the selected machine with a Selected machine section and a Summary section. The library holds every machine a person can use, shown as cards with a source badge:

- Built in: ships with the program. A built-in machine can be hidden (Show hidden machines lists it again) but never changed or deleted. To change one, Duplicate it and edit the copy.
- My machines: files in the `machines` folder beside the preferences, shared by every project on this computer.
- This project: the copy a project keeps. A project always holds its own copy of each machine it uses, so its output does not change when the library does.

Clicking a kind or source row narrows the cards, and the search box matches name, maker, model and id. Clicking a card selects it. A card offers Edit (off for now) and Duplicate. The right panel offers Add to this project, Edit a copy, Export (off for now), Hide or Show (built in), Save to my machines (project copies), Remove (my machines) and, when the library machine has a newer version than the project copy, Update from library. The tool bar has Add machine, Duplicate, Import (off for now), Export (off for now), Check for updates and Sort by Name (off for now). Import, Export, Edit and Sort stay off until the engine has commands for them. The Shared folder row lists nothing until a shared folder exists.

Add machine opens a window to start from a library machine or a blank machine of one kind, name it, and choose whether it goes on the project and whether it is kept in My machines. The blank kinds are Pick and place, Laser (galvo), Laser (gantry), Mill, UV printer, Reflow oven and Other machine.

A Reflow oven records its soldering area (the work area), rated power, top temperature, a plain description of the heating and the number of ready-made profiles. The built-in Puhui T-962A uses the figures in the maker's manual: 300 x 320 mm, 1500 W, up to 280 C, eight profiles and a 1 to 8 minute cycle. Its mains line and outside size are unverified and the machine note says so. Reflow profiles themselves are not modelled yet (issue 1646). Other machine records a name, maker, model, work area and a note for any machine that fits no kind. A machine with no output offers no job file in Manufacture.

In Manufacture, the Pick and place file row has a Machine dropdown fed by the library. Write job file is available when the machine's declared output dialect has a writer; it does not depend on a machine name. The commands behind all of this are `machine.library_list`, `machine.add`, `machine.install`, `machine.duplicate`, `machine.update_from_library`, `machine.library_save`, `machine.library_hide` and `machine.library_remove`; the job file command is `placement_job.export` (`placement_yy1.export` still works for one release).

