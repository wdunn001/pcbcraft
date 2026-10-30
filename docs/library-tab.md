# The Library tab

The Library tab is a work area of its own, between Files and Schematic. It is where you read about
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
| Datasheet | The cached PDF read in the app (page navigation, zoom, search with highlights, selectable text, Copy) beside the key details and pinout. Get datasheet fetches the part's own link once (https, 25 MB, 30 seconds, `%PDF-` header check) or you choose a file; the copy stays on this computer in the datasheet cache and is never uploaded or saved in the project. |
| Vendors | Supplier, SKU, stock and price breaks from cached answers, each with the date it was taken, and the Sources card. |
| 3D model | The part on a small board in the board renderer, the model file, offset, rotation and scale, and the pads drawn over the model's outline. |
| Edit | The symbol and footprint editor of the Libraries page, and a pin map that shows which footprint pad each symbol pin reaches. |

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

## Not built yet

- Datasheet reader: only the current page is drawn (no continuous scroll), text of the first 300 pages is searched, no thumbnails, outline or annotations, a link that redirects to another site must be opened in the browser, and values are not yet extracted from the PDF itself.
- Symbol and footprint canvases side by side on the Edit page; today one editor shows one at a time.
- Auto-align of the model to the pads, and snapping.
