//! Project-library editing commands: import, pin and pad edits, artwork, metadata,
//! geometry, and placing library parts on the schematic.
//!
//! Context: `commands()` registers MCP ids; `run` dispatches each id to validation and
//! mutation on `Project::library`. Geometry helpers below `run` are shared by several arms.
//!
//! # Rules
//! - Library names are 1 to 128 bytes; pin and pad numbers are 1 to 32 bytes unless noted.
//! - After each mutation the touched definition and `Project::library` are validated again.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs
//! - crates/library/src/lib.rs
//! - crates/geom/src/lib.rs

use super::*;
use pcbcraft_library::{
    FootprintDefinition, FootprintPad, Graphic, LandPatternDensity, LandPatternParams,
    PartSubstitutes, SymbolDefinition, SymbolPin, UnitGraphic, compare_one_part,
    MAX_PART_COMPARE_OTHERS, generate_footprint,
};

pub(super) fn commands() -> Vec<CommandSpec> {
    [
        ("library.symbol.upsert","Create or replace a project-library symbol; existing placed definitions remain embedded",schema(json!({"definition":{"type":"object"},"notice":{"type":"string"}}),&["definition"])),
        ("library.footprint.upsert","Create or replace a project-library footprint",schema(json!({"definition":{"type":"object"},"notice":{"type":"string"}}),&["definition"])),
        ("library.symbol.remove","Remove a project-library symbol; placed instances retain their embedded definition",schema(json!({"name":{"type":"string"}}),&["name"])),
        ("library.footprint.remove","Remove a project-library footprint; placed instances retain their embedded definition",schema(json!({"name":{"type":"string"}}),&["name"])),
        ("library.load","Load and validate a .pcbcraft-lib collection into the project as one undoable transaction",schema(json!({"path":{"type":"string"}}),&["path"])),
        ("library.import_footprint_file","Read one footprint from a footprint library text file on disk and add or replace it in the project library as one undoable transaction; an optional notice is kept with the project library",schema(json!({"path":{"type":"string","minLength":1,"maxLength":1024},"notice":{"type":"string"}}),&["path"])),
        ("library.import_symbol_file","Read the symbols from a symbol library text file on disk and add or replace them in the project library as one undoable transaction; an optional notice is kept with the project library",schema(json!({"path":{"type":"string","minLength":1,"maxLength":1024},"notice":{"type":"string"}}),&["path"])),
        ("library.import_footprint_dir","Read every footprint library text file in a library folder on disk and add or replace them in the project library as one undoable transaction, so a whole vendor folder arrives in one step",schema(json!({"path":{"type":"string","minLength":1,"maxLength":1024}}),&["path"])),
        ("library.import_symbol_dir","Read every symbol library text file in a library folder on disk and add or replace them in the project library as one undoable transaction, so a whole vendor symbol folder arrives in one step",schema(json!({"path":{"type":"string","minLength":1,"maxLength":1024}}),&["path"])),
        ("library.import_part","Copy a part's symbol and, when it has one, its footprint into the project library as one undoable step, keeping the licence notice; the symbol is linked to the footprint by name",schema(json!({"symbol":{"type":"object","description":"a symbol definition as library readers emit it"},"footprint":{"type":"object","description":"a footprint definition as library readers emit it"},"notice":{"type":"string","description":"licence or attribution line kept with the project library"}}),&["symbol"])),
        ("part.place","Import a part if it is not in the project library yet and place it as one undoable step. On the schematic the symbol goes at the position; on the board the footprint goes at the position and the part joins the schematic parked below the drawing, not yet wired",schema(json!({"target":{"type":"string","enum":["schematic","board"]},"position":point_schema(),"name":{"type":"string","description":"a symbol already in the project library"},"kind":{"type":"string","description":"a built-in part kind such as resistor"},"symbol":{"type":"object"},"footprint":{"type":"object"},"notice":{"type":"string"},"reference":{"type":"string"},"value":{"type":"string"}}),&["target","position"])),
        ("component.place_custom","Place a symbol from the editable project library, optionally turned to a starting rotation",schema(json!({"name":{"type":"string"},"position":point_schema(),"reference":{"type":"string"},"value":{"type":"string"},"rotation":{"type":"number","description":"starting rotation in degrees"}}),&["name","position"])),
        ("footprint.assign_custom","Assign a project-library footprint whose pad numbers match the component pins",schema(json!({"id":id_schema(),"name":{"type":"string"}}),&["id","name"])),
        ("library.footprint.set_model","Attach or replace a 3D model reference on a project-library footprint: a file path with an optional offset, scale and rotation in millimetres and degrees; placed instances keep their embedded copy",schema(json!({"name":{"type":"string"},"path":{"type":"string","maxLength":1024},"offset":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},"scale":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},"rotate":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}}),&["name","path"])),
        ("library.footprint.attach_starter_model","Attach a bundled starter 3D model to a project-library footprint when one matches its name or an explicit starter id, materializing the file into the user cache and recording the licence notice",schema(json!({"name":{"type":"string"},"id":{"type":"string","description":"starter model id; omit to match by footprint name"}}),&["name"])),
        ("library.footprint.clear_model","Remove the 3D model reference from a project-library footprint",schema(json!({"name":{"type":"string"}}),&["name"])),
        ("library.footprint.model_auto_align","Fit a footprint's 3D model to its pads from a triangle list in model space: tries quarter turns and an optional mirror, reports the residual in millimetres, and when apply is true writes the new offset, rotation and scale as one undoable step",schema(json!({"name":{"type":"string"},"positions":{"type":"array","items":{"type":"number"},"description":"model vertex positions as x,y,z triples in millimetres"},"indices":{"type":"array","items":{"type":"integer"},"description":"triangle corner indices, three per face"},"allow_flip":{"type":"boolean","description":"when true, also try mirroring the model about Y"},"apply":{"type":"boolean","description":"when false, preview only; default true"}}),&["name","positions"])),
        ("library.footprint.set_rules","Set design rule overrides on a project-library footprint: a local copper clearance and solder mask/paste margin shifts in millimetres; the clearance tightens clearance checks while the margins are recorded and exported",schema(json!({"name":{"type":"string"},"clearance":{"type":"number"},"mask_margin":{"type":"number"},"paste_margin":{"type":"number"}}),&["name"])),
        ("library.footprint.clear_rules","Remove the design rule overrides from a project-library footprint",schema(json!({"name":{"type":"string"}}),&["name"])),
        ("library.footprint.renumber_pads","Renumber pads on a project-library footprint with an ordered old-to-new map; embedded copies, their placed pads and matching component pins follow, and a target number already held by an untouched pad is refused",schema(json!({"name":{"type":"string"},"renames":{"type":"array","maxItems":512,"items":{"type":"object","properties":{"old":{"type":"string","maxLength":32},"new":{"type":"string","maxLength":32}},"required":["old","new"],"additionalProperties":false}}}),&["name","renames"])),
        ("library.footprint.generate_courtyard","Generate a courtyard rectangle on a project-library footprint: the outline of its pads and artwork grown by a margin, replacing any courtyard the footprint already carries",schema(json!({"name":{"type":"string"},"margin":{"type":"number","minimum":0,"maximum":5,"description":"ring beyond the outermost copper and artwork, millimetres"}}),&["name","margin"])),
        ("library.footprint.generate","Build an IPC-style land pattern from a bounded family name and package parameters, then create or replace that footprint in the project library",schema(json!({"family":{"type":"string","enum":["chip_2terminal","sot","soic","qfp","qfn","bga","dip","header_single","header_double","radial","axial"]},"density":{"type":"string","enum":["A","B","C"],"default":"B"},"name":{"type":"string","minLength":1,"maxLength":128},"description":{"type":"string"},"keywords":{"type":"string"},"pitch":{"type":"number"},"pin_count":{"type":"integer","minimum":1,"maximum":256},"body_x":{"type":"number"},"body_y":{"type":"number"},"body_z":{"type":"number"},"rows":{"type":"integer","minimum":2,"maximum":50},"cols":{"type":"integer","minimum":2,"maximum":50},"row_spacing":{"type":"number"},"terminal_width":{"type":"number"},"thermal":{"type":"boolean"}}),&["family","name"])),
        ("library.footprint.check","Read-only library-quality check on a project-library footprint by name or on an inline definition; returns structured findings",schema(json!({"name":{"type":"string"},"definition":{"type":"object"}}),&[])),
        ("library.footprint.pad_set","Add or replace one pad on a project-library footprint by its number, so custom packages gain exposed pads, thermal vias or extra terminals without rewriting the whole definition; placed instances keep their embedded copy until footprint.refresh brings the change across",schema(json!({"name":{"type":"string"},"pad":{"type":"object","description":"a full pad: number, shape, size, position, layers and optional drill, as library readers emit them"}}),&["name","pad"])),
        ("library.footprint.pad_remove","Remove one pad from a project-library footprint by its number; the footprint keeps at least one pad, and placed instances keep their embedded copy until footprint.refresh brings the change across",schema(json!({"name":{"type":"string"},"pad":{"type":"string","description":"pad number, exactly as stored"}}),&["name","pad"])),
        ("library.footprint.pad_move","Slide one pad of a project-library footprint by an offset, as one undoable step, moving its copper and its drill together while size, shape and layers stay put, so a single pad landing in the wrong place can be nudged into position without rewriting the package; placed instances keep their embedded copy until footprint.refresh brings the change across",schema(json!({"name":{"type":"string"},"pad":{"type":"string","description":"pad number, exactly as stored"},"delta":point_schema()}),&["name","pad","delta"])),
        ("library.footprint.pad_rotate","Turn one pad of a project-library footprint about the package origin by any angle, as one undoable step, swinging the pad's position, facing and drill offset together, so a single pad can be re-aimed from the origin without turning the whole package; placed instances keep their embedded copy until footprint.refresh brings the change across",schema(json!({"name":{"type":"string"},"pad":{"type":"string","description":"pad number, exactly as stored"},"degrees":{"type":"number","description":"counter-clockwise turn in degrees about the footprint origin"}}),&["name","pad","degrees"])),
        ("library.footprint.pad_mirror","Flip one pad of a project-library footprint about the vertical axis, as one undoable step, mirroring its position, facing and drill offset together, so a single off-centre pad can be moved to the opposite side without flipping the whole package; placed instances keep their embedded copy until footprint.refresh brings the change across",schema(json!({"name":{"type":"string"},"pad":{"type":"string","description":"pad number, exactly as stored"},"axis":{"type":"string","enum":["y"],"description":"mirror axis; only y, the vertical axis, is supported today"}}),&["name","pad","axis"])),
        ("library.footprint.duplicate","Copy a project-library footprint under a new name as one undoable step, the starting point for a custom variant of a vendor package; the original and every placed copy stay untouched",schema(json!({"name":{"type":"string"},"new_name":{"type":"string","description":"name for the copy, which must not exist yet"}}),&["name","new_name"])),
        ("library.symbol.duplicate","Copy a project-library symbol under a new name as one undoable step, the starting point for a custom variant of a vendor symbol; the original and every placed copy stay untouched",schema(json!({"name":{"type":"string"},"new_name":{"type":"string","description":"name for the copy, which must not exist yet"}}),&["name","new_name"])),
        ("library.symbol.pin_set","Add or replace one pin on a project-library symbol by its number, so custom symbols gain extra terminals, corrected pin names or repositioned leads without rewriting the whole definition; placed instances keep their embedded copy until symbol.refresh brings the change across",schema(json!({"name":{"type":"string"},"pin":{"type":"object","description":"a full pin: number, name, position, angle, length, electrical type, unit, body style, graphical_style and visible, as library readers emit them"}}),&["name","pin"])),
        ("library.symbol.pin_remove","Remove one pin from a project-library symbol by its number; the symbol keeps at least one pin, and placed instances keep their embedded copy until symbol.refresh brings the change across",schema(json!({"name":{"type":"string"},"pin":{"type":"string","description":"pin number, exactly as stored"}}),&["name","pin"])),
        ("library.symbol.pin_move","Slide one pin of a project-library symbol by an offset, as one undoable step, so a single lead landing in the wrong place can be nudged into position without touching its name, length or electrical type and without moving the rest of the symbol; placed instances keep their embedded copy until symbol.refresh brings the change across",schema(json!({"name":{"type":"string"},"pin":{"type":"string","description":"pin number, exactly as stored"},"delta":point_schema()}),&["name","pin","delta"])),
        ("library.symbol.pin_rotate","Turn one pin of a project-library symbol about the symbol origin by any angle, as one undoable step, swinging the pin's position and its facing together so a single lead can be re-aimed from the origin without rewriting the symbol; placed instances keep their embedded copy until symbol.refresh brings the change across",schema(json!({"name":{"type":"string"},"pin":{"type":"string","description":"pin number, exactly as stored"},"degrees":{"type":"number","description":"counter-clockwise turn in degrees about the symbol origin"}}),&["name","pin","degrees"])),
        ("library.symbol.pin_mirror","Flip one pin of a project-library symbol about the vertical axis, as one undoable step, mirroring its position and turning its facing to match, so a single left-facing lead can become a right-facing one without rewriting the symbol; placed instances keep their embedded copy until symbol.refresh brings the change across",schema(json!({"name":{"type":"string"},"pin":{"type":"string","description":"pin number, exactly as stored"},"axis":{"type":"string","enum":["y"],"description":"mirror axis; only y, the vertical axis, is supported today"}}),&["name","pin","axis"])),
        ("library.symbol.renumber_pins","Renumber pins on a project-library symbol with an ordered old-to-new map; embedded copies and their placed pins follow, no-connect flags on those pins follow too, and a target number already held by an untouched pin is refused",schema(json!({"name":{"type":"string"},"renames":{"type":"array","maxItems":512,"items":{"type":"object","properties":{"old":{"type":"string","maxLength":32},"new":{"type":"string","maxLength":32}},"required":["old","new"],"additionalProperties":false}}}),&["name","renames"])),
        ("library.footprint.graphic_add","Draw one graphic (line, rectangle, circle, arc, polyline or text) onto a project-library footprint as one undoable step, the way a package gains its silk outline, courtyard art or documentation text; it travels with every embedded copy once the package is assigned or refreshed",schema(json!({"name":{"type":"string"},"graphic":{"type":"object","description":"a graphic primitive as library readers emit it: kind with its points, width, layer and optional fill"}}),&["name","graphic"])),
        ("library.footprint.graphic_clear","Remove artwork from a project-library footprint in one undoable step: give a layer to clear just that layer's graphics, or omit it to clear every graphic, so stale silk and fab art can be wiped before redrawing",schema(json!({"name":{"type":"string"},"layer":{"type":"string","description":"layer to clear; omit to clear all graphics"}}),&["name"])),
        ("library.symbol.graphic_add","Draw one graphic (line, rectangle, circle, arc, polyline or text) onto one unit of a project-library symbol as one undoable step, the way a symbol gains body art, pin decorations or documentation text; it travels with every embedded copy until symbol.refresh spreads it",schema(json!({"name":{"type":"string"},"unit":{"type":"integer","description":"symbol unit the graphic belongs to; defaults to 1"},"body_style":{"type":"integer","description":"body style the graphic belongs to; defaults to 1"},"graphic":{"type":"object","description":"a graphic primitive as library readers emit it: kind with its points, width, layer and optional fill"}}),&["name","graphic"])),
        ("library.symbol.graphic_clear","Remove one unit's artwork from a project-library symbol in one undoable step, so stale symbol art can be wiped before redrawing; the unit keeps its pins",schema(json!({"name":{"type":"string"},"unit":{"type":"integer","description":"symbol unit whose graphics clear; defaults to 1"}}),&["name"])),
        ("library.footprint.graphic_set","Replace one graphic on a project-library footprint by its position in the artwork list, as one undoable step, so a single stale silk or fab element can be corrected without clearing and redrawing the rest",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the footprint's artwork list"},"graphic":{"type":"object","description":"the replacement graphic primitive, as library readers emit it"}}),&["name","index","graphic"])),
        ("library.footprint.graphic_remove","Remove one graphic from a project-library footprint by its position in the artwork list, as one undoable step, so a single stale silk or fab element can be deleted without clearing the rest",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the footprint's artwork list"}}),&["name","index"])),
        ("library.symbol.graphic_set","Replace one graphic on a project-library symbol by its position in the artwork list, as one undoable step, keeping the unit it belongs to, so a single wrong symbol element can be corrected without clearing and redrawing the rest",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the symbol's artwork list"},"graphic":{"type":"object","description":"the replacement graphic primitive, as library readers emit it"}}),&["name","index","graphic"])),
        ("library.symbol.graphic_remove","Remove one graphic from a project-library symbol by its position in the artwork list, as one undoable step, so a single wrong symbol element can be deleted without clearing the rest",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the symbol's artwork list"}}),&["name","index"])),
        ("library.footprint.graphic_move","Slide one graphic on a project-library footprint by an offset, as one undoable step, so a single misplaced silk or fab element can be nudged into place without touching the rest of the package",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the footprint's artwork list"},"delta":point_schema()}),&["name","index","delta"])),
        ("library.symbol.graphic_move","Slide one graphic on a project-library symbol by an offset, as one undoable step, so a single misplaced body line or pin decoration can be nudged into place without touching the rest of the symbol",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the symbol's artwork list"},"delta":point_schema()}),&["name","index","delta"])),
        ("library.footprint.graphic_rotate","Turn one graphic on a project-library footprint about the origin by any angle, as one undoable step, so a single slanted or wrongly aimed drawing element can be re-aimed without turning the whole package",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the footprint's artwork list"},"degrees":{"type":"number","description":"counter-clockwise turn in degrees about the footprint origin"}}),&["name","index","degrees"])),
        ("library.symbol.graphic_rotate","Turn one graphic on a project-library symbol about the origin by any angle, as one undoable step, so a single slanted or wrongly aimed drawing element can be re-aimed without turning the whole symbol",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the symbol's artwork list"},"degrees":{"type":"number","description":"counter-clockwise turn in degrees about the symbol origin"}}),&["name","index","degrees"])),
        ("library.footprint.graphic_mirror","Flip one graphic on a project-library footprint about the vertical axis, as one undoable step, turning its text to follow, so a single mirrored drawing element can be corrected without flipping the whole package",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the footprint's artwork list"},"axis":{"type":"string","enum":["y"],"description":"mirror axis; only y, the vertical axis, is supported today"}}),&["name","index","axis"])),
        ("library.symbol.graphic_mirror","Flip one graphic on a project-library symbol about the vertical axis, as one undoable step, turning its text to follow, so a single mirrored drawing element can be corrected without flipping the whole symbol",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the symbol's artwork list"},"axis":{"type":"string","enum":["y"],"description":"mirror axis; only y, the vertical axis, is supported today"}}),&["name","index","axis"])),
        ("library.footprint.graphic_scale","Resize one graphic on a project-library footprint about the origin by one factor, as one undoable step, scaling its vertices, stroke and text size together, so a single drawing element made at the wrong scale is rescued without touching the rest of the package",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the footprint's artwork list"},"factor":{"type":"number","description":"uniform resize factor about the footprint origin, above 0 and up to 1000"}}),&["name","index","factor"])),
        ("library.symbol.graphic_scale","Resize one graphic on a project-library symbol about the origin by one factor, as one undoable step, scaling its vertices, stroke and text size together, so a single drawing element made at the wrong scale is rescued without touching the rest of the symbol",schema(json!({"name":{"type":"string"},"index":{"type":"integer","description":"zero-based position in the symbol's artwork list"},"factor":{"type":"number","description":"uniform resize factor about the symbol origin, above 0 and up to 1000"}}),&["name","index","factor"])),
        ("library.symbol.set_properties","Retype a project-library symbol's reference prefix, value, description, keywords, linked footprint or set name one field at a time as one undoable step, so imported vendor symbols get cleaned up without rewriting the definition; placed instances keep their embedded copy until symbol.refresh brings the change across",schema(json!({"name":{"type":"string"},"reference":{"type":"string","description":"reference prefix pattern the symbol suggests, e.g. U*"},"value":{"type":"string"},"description":{"type":"string"},"keywords":{"type":"string","description":"space separated search words"},"footprint":{"type":"string","description":"default footprint name the symbol suggests"},"set":{"type":"string","description":"name of the collection this part belongs to; empty clears it, and the parts list groups parts by set"}}),&["name"])),
        ("library.part.set_substitute","Add, replace or remove one second source (maker and part number) or one alternate (another project-library symbol) on a project-library part as one undoable step; pass null for mpn to remove a second source, or set clear true with symbol naming the alternate to remove",schema(json!({"name":{"type":"string","description":"project-library symbol name of the primary part"},"kind":{"type":"string","enum":["second_source","alternate"]},"manufacturer":{"type":"string","maxLength":256},"mpn":{"type":["string","null"],"maxLength":256},"symbol":{"type":"string","maxLength":256,"description":"project-library symbol name of the alternate part"},"footprint":{"type":"string","maxLength":256},"note":{"type":"string","maxLength":256},"clear":{"type":"boolean","description":"when true with kind alternate, remove the alternate named by symbol"}}),&["name","kind"])),
        ("library.part.compare","Read-only comparison of one baseline project-library part against up to two others: typed symbol fields with same flags, pin number diffs, and footprint pad diffs when both footprints exist",schema(json!({"baseline":{"type":"string","maxLength":128,"description":"project-library symbol name used as the reference row"},"others":{"type":"array","minItems":1,"maxItems":2,"items":{"type":"string","maxLength":128}},"footprint_overrides":{"type":"object","additionalProperties":{"type":"string","maxLength":256},"description":"optional footprint name per other symbol, e.g. from an alternate record"}}),&["baseline","others"])),
        ("library.footprint.set_properties","Retype a project-library footprint's description or keywords one field at a time as one undoable step, so imported packages get cleaned up for search and documentation without rewriting the definition; placed instances keep their embedded copy until footprint.refresh brings the change across",schema(json!({"name":{"type":"string"},"description":{"type":"string"},"keywords":{"type":"string","description":"space separated search words"}}),&["name"])),
        ("library.symbol.rename","Rename a project-library symbol in place as one undoable step, keeping every pin and graphic exactly as it was, so a typo at the source never needs a delete and re-upsert; placed instances keep the old embedded name",schema(json!({"name":{"type":"string"},"new_name":{"type":"string","minLength":1,"maxLength":128}}),&["name","new_name"])),
        ("library.footprint.rename","Rename a project-library footprint in place as one undoable step, keeping every pad and graphic exactly as it was, so a typo at the source never needs a delete and re-upsert; placed instances keep the old embedded name",schema(json!({"name":{"type":"string"},"new_name":{"type":"string","minLength":1,"maxLength":128}}),&["name","new_name"])),
        ("library.symbol.set_extends","Point a project-library symbol at the parent symbol it inherits from as one undoable step, recording the inheritance link the library readers use so derived symbols document where their pins and text come from",schema(json!({"name":{"type":"string"},"extends":{"type":"string","minLength":1,"maxLength":128,"description":"parent symbol name within the project library"}}),&["name","extends"])),
        ("library.symbol.clear_extends","Drop a project-library symbol's inheritance link in one undoable step, so a symbol that was copying another can stand on its own without rewriting the definition",schema(json!({"name":{"type":"string"}}),&["name"])),
        ("library.symbol.rotate","Turn a project-library symbol about its origin by any angle as one undoable step, swinging every pin and every graphic together, so an imported symbol facing the wrong way can be re-aimed without rewriting the definition",schema(json!({"name":{"type":"string"},"degrees":{"type":"number","description":"counter-clockwise turn in degrees about the symbol origin"}}),&["name","degrees"])),
        ("library.symbol.mirror","Flip a project-library symbol about the vertical axis as one undoable step, mirroring every pin and graphic and turning each graphic so its face follows, so a left-facing symbol becomes a right-facing one without rewriting the definition",schema(json!({"name":{"type":"string"},"axis":{"type":"string","enum":["y"],"description":"mirror axis; only y, the vertical axis, is supported today"}}),&["name","axis"])),
        ("library.footprint.rotate","Turn a project-library footprint about its origin by any angle as one undoable step, swinging every pad and graphic together, so an imported package can be re-aimed without rewriting the definition",schema(json!({"name":{"type":"string"},"degrees":{"type":"number","description":"counter-clockwise turn in degrees about the footprint origin"}}),&["name","degrees"])),
        ("library.footprint.mirror","Flip a project-library footprint about the vertical axis as one undoable step, mirroring every pad and graphic and turning each so its face follows, so a package can be re-aimed without rewriting the definition",schema(json!({"name":{"type":"string"},"axis":{"type":"string","enum":["y"],"description":"mirror axis; only y, the vertical axis, is supported today"}}),&["name","axis"])),
        ("library.footprint.recenter","Slide a project-library footprint's whole shape by an offset as one undoable step, moving every pad, graphic and model anchor together so the origin lands where the package should be centred, without rewriting the definition",schema(json!({"name":{"type":"string"},"delta":point_schema()}),&["name","delta"])),
        ("library.symbol.recenter","Slide a project-library symbol's whole shape by an offset as one undoable step, moving every pin and graphic together so the origin lands where the symbol should be centred, without rewriting the definition",schema(json!({"name":{"type":"string"},"delta":point_schema()}),&["name","delta"])),
        ("library.footprint.scale","Grow or shrink a project-library footprint about its origin by one factor as one undoable step, scaling every pad, drill, graphic and model anchor together, so a package imported at the wrong unit scale is rescued without rewriting the definition",schema(json!({"name":{"type":"string"},"factor":{"type":"number","description":"uniform scale factor, e.g. 0.0254 to turn a hundred-mils drawing into millimetres"}}),&["name","factor"])),
        ("library.symbol.scale","Grow or shrink a project-library symbol about its origin by one factor as one undoable step, scaling every pin position, pin length and graphic together, so a symbol imported at the wrong grid scale is resized without rewriting the definition",schema(json!({"name":{"type":"string"},"factor":{"type":"number","description":"uniform scale factor"}}),&["name","factor"])),
        ("library.footprint.adopt","Copy a placed component's embedded footprint into the project library under a chosen name as one undoable step, so a package that arrived on a part becomes editable and reusable; adopting the same name again replaces that library entry",schema(json!({"id":id_schema(),"name":{"type":"string","description":"name to store the package under"}}),&["id","name"])),
        ("library.symbol.adopt","Copy a placed component's embedded symbol into the project library under a chosen name as one undoable step, so a symbol that arrived on a part becomes editable and reusable; adopting the same name again replaces that library entry",schema(json!({"id":id_schema(),"name":{"type":"string","description":"name to store the symbol under"}}),&["id","name"])),
    ].into_iter().map(|(id,description,input_schema)|CommandSpec {id,description,input_schema}).collect()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportPartParams {
    symbol: SymbolDefinition,
    footprint: Option<FootprintDefinition>,
    notice: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlacePartParams {
    target: String,
    position: Point,
    name: Option<String>,
    /// A built-in part kind such as `resistor`, instead of a library symbol.
    kind: Option<String>,
    symbol: Option<SymbolDefinition>,
    footprint: Option<FootprintDefinition>,
    notice: Option<String>,
    reference: Option<String>,
    value: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolParams {
    definition: SymbolDefinition,
    notice: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PartCompareParams {
    baseline: String,
    others: Vec<String>,
    #[serde(default)]
    footprint_overrides: std::collections::BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetPartSubstituteParams {
    name: String,
    kind: String,
    #[serde(default)]
    manufacturer: Option<String>,
    #[serde(default)]
    mpn: Option<Value>,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    footprint: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    clear: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PadRename {
    old: String,
    new: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenumberParams {
    name: String,
    renames: Vec<PadRename>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CourtyardParams {
    name: String,
    margin: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FootprintCheckParams {
    name: Option<String>,
    definition: Option<FootprintDefinition>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GenerateFootprintCommandParams {
    family: String,
    #[serde(default = "default_density")]
    density: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    keywords: String,
    #[serde(default)]
    pitch: f64,
    #[serde(default)]
    pin_count: u32,
    #[serde(default)]
    body_x: f64,
    #[serde(default)]
    body_y: f64,
    #[serde(default)]
    body_z: f64,
    #[serde(default)]
    rows: u32,
    #[serde(default)]
    cols: u32,
    #[serde(default)]
    row_spacing: f64,
    #[serde(default)]
    terminal_width: f64,
    #[serde(default)]
    thermal: bool,
}

fn default_density() -> String {
    "B".into()
}

fn land_pattern_params_from_command(a: &GenerateFootprintCommandParams) -> LandPatternParams {
    let mut params = LandPatternParams {
        name: a.name.clone(),
        description: a.description.clone(),
        keywords: a.keywords.clone(),
        pitch: a.pitch,
        pin_count: a.pin_count,
        body_length: a.body_x,
        body_width: a.body_y,
        row_spacing: a.row_spacing,
        lead_width: a.terminal_width,
        lead_diameter: 0.0,
        grid_rows: a.rows,
        grid_cols: a.cols,
        thermal_size: if a.thermal {
            a.body_x.min(a.body_y) * 0.4
        } else {
            0.0
        },
    };
    if params.body_length <= 0.0 && a.body_z > 0.0 {
        params.body_length = a.body_z;
    }
    params
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FootprintParams {
    definition: FootprintDefinition,
    notice: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathParams {
    path: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportFileParams {
    path: String,
    notice: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdoptParams {
    id: Id,
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DuplicateParams {
    name: String,
    new_name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PadSetParams {
    name: String,
    pad: FootprintPad,
}
/// Input for `library.footprint.scale` and `library.symbol.scale`; scales every pad or
/// pin, artwork, and footprint model anchor together about the origin.
///
/// # Rules
/// - Name must be 1 to 128 bytes; factor must be finite, positive, and at most 1000.
/// - Scaled coordinates must pass `Point::valid`.
///
/// # Dependencies
/// - crates/geom/src/lib.rs
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScaleParams {
    name: String,
    factor: f64,
}

/// Input for `library.footprint.recenter` and `library.symbol.recenter`; slides the whole
/// definition by one offset in millimetres.
///
/// # Rules
/// - Name must be 1 to 128 bytes; delta must be finite with each axis within 1000 mm.
/// - Shifted coordinates must pass `Point::valid`.
///
/// # Dependencies
/// - crates/geom/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecenterParams {
    name: String,
    delta: Point,
}

/// Input for `library.symbol.rotate`, `library.symbol.mirror`, `library.footprint.rotate`,
/// and `library.footprint.mirror`; turns or flips the whole library definition.
///
/// # Rules
/// - Name must be 1 to 128 bytes; rotation needs finite degrees within 100000 and no axis.
/// - Mirror needs axis `"y"` and must not also send degrees.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GeometryParams {
    name: String,
    #[serde(default)]
    degrees: Option<f64>,
    #[serde(default)]
    axis: Option<String>,
}

/// Input for `library.symbol.set_extends` and `library.symbol.clear_extends`; records or
/// clears the parent symbol name on a project-library symbol.
///
/// # Rules
/// - Symbol name must be 1 to 128 bytes; set_extends needs a parent name in the library.
/// - A symbol cannot inherit from itself.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolExtendsParams {
    name: String,
    #[serde(default)]
    extends: Option<String>,
}

/// Input for `library.symbol.rename` and `library.footprint.rename`; changes the library
/// entry name in place while keeping pins or pads and graphics unchanged.
///
/// # Rules
/// - Current and new names must each be 1 to 128 bytes; the new name must not collide.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameParams {
    name: String,
    new_name: String,
}

/// Input for `library.symbol.set_properties` and `library.footprint.set_properties`; updates
/// searchable metadata one field at a time.
///
/// # Rules
/// - Name must be 1 to 128 bytes; at least one property field must be present.
/// - Text fields are capped at 128 KiB; footprints accept description and keywords only.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetPropertyParams {
    name: String,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    keywords: Option<String>,
    #[serde(default)]
    footprint: Option<String>,
    #[serde(default)]
    set: Option<String>,
}
/// Input for `library.footprint.graphic_add`; appends one graphic primitive to a footprint.
///
/// # Rules
/// - Footprint name must be 1 to 128 bytes; the graphic layer must pass `graphic_layer`.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicAddParams {
    name: String,
    graphic: Graphic,
}
/// Input for `library.footprint.graphic_set` and `library.symbol.graphic_set`; replaces one
/// artwork entry by zero-based index.
///
/// # Rules
/// - Name must be 1 to 128 bytes; index must address an existing graphic.
/// - Replacement graphic layer must pass `graphic_layer`.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicSetParams {
    name: String,
    index: usize,
    graphic: Graphic,
}
/// Input for `library.footprint.graphic_remove` and `library.symbol.graphic_remove`; drops
/// one artwork entry by zero-based index.
///
/// # Rules
/// - Name must be 1 to 128 bytes; index must address an existing graphic.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicRemoveParams {
    name: String,
    index: usize,
}
/// Input for `library.footprint.graphic_move` and `library.symbol.graphic_move`; slides one
/// graphic by a delta in millimetres.
///
/// # Rules
/// - Name must be 1 to 128 bytes; delta must be finite with each axis within 1000 mm.
/// - Shifted vertices must pass `Point::valid`.
///
/// # Dependencies
/// - crates/geom/src/lib.rs
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicMoveParams {
    name: String,
    index: usize,
    delta: Point,
}
/// Input for `library.footprint.graphic_rotate`, `library.symbol.graphic_rotate`,
/// `library.footprint.graphic_mirror`, and `library.symbol.graphic_mirror`.
///
/// # Rules
/// - Name must be 1 to 128 bytes; index must address an existing graphic.
/// - Rotate needs finite degrees within 100000; mirror needs axis `"y"`.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicTurnParams {
    name: String,
    index: usize,
    #[serde(default)]
    degrees: Option<f64>,
    #[serde(default)]
    axis: Option<String>,
}
/// Input for `library.footprint.graphic_scale` and `library.symbol.graphic_scale`; resizes
/// one graphic about the origin.
///
/// # Rules
/// - Name must be 1 to 128 bytes; factor must be finite, positive, and at most 1000.
///
/// # Dependencies
/// - crates/geom/src/lib.rs
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicScaleParams {
    name: String,
    index: usize,
    factor: f64,
}
/// Input for `library.footprint.graphic_clear`; removes footprint artwork on one layer or all layers.
///
/// # Rules
/// - Footprint name must be 1 to 128 bytes; an optional layer must not be empty.
/// - Refuses when no graphics would be removed.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicClearParams {
    name: String,
    #[serde(default)]
    layer: Option<String>,
}
/// Input for `library.symbol.graphic_add`; appends one graphic to a symbol unit and body style.
///
/// # Rules
/// - Symbol name must be 1 to 128 bytes; unit and body_style default to 1 when omitted.
/// - Graphic layer must pass `graphic_layer`.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolGraphicAddParams {
    name: String,
    #[serde(default)]
    unit: Option<u32>,
    #[serde(default)]
    body_style: Option<u32>,
    graphic: Graphic,
}
/// Input for `library.symbol.graphic_clear`; removes all graphics on one symbol unit.
///
/// # Rules
/// - Symbol name must be 1 to 128 bytes; unit defaults to 1 when omitted.
/// - Refuses when the unit has no graphics to remove.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolGraphicClearParams {
    name: String,
    #[serde(default)]
    unit: Option<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinSetParams {
    name: String,
    pin: SymbolPin,
}
/// Input for `library.symbol.pin_remove`; drops one pin by number from a project-library symbol.
///
/// # Rules
/// - Symbol name must be 1 to 128 bytes; pin number must be 1 to 32 bytes.
/// - The symbol must keep at least one pin after removal.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinRemoveParams {
    name: String,
    pin: String,
}
/// Input for `library.symbol.pin_move`; slides one pin by a delta without changing its metadata.
///
/// # Rules
/// - Symbol and pin number bounds match pin_remove; delta must be finite within 1000 mm per axis.
/// - Moved position must pass `Point::valid`.
///
/// # Dependencies
/// - crates/geom/src/lib.rs
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinMoveParams {
    name: String,
    pin: String,
    delta: Point,
}
/// Input for `library.symbol.pin_rotate` and `library.symbol.pin_mirror`; turns or flips one pin.
///
/// # Rules
/// - Symbol and pin number bounds match pin_remove; rotate needs finite degrees within 100000.
/// - Mirror needs axis `"y"` and must not also send degrees.
///
/// # Dependencies
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinTurnParams {
    name: String,
    pin: String,
    #[serde(default)]
    degrees: Option<f64>,
    #[serde(default)]
    axis: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PadRemoveParams {
    name: String,
    pad: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PadMoveParams {
    name: String,
    pad: String,
    delta: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PadTurnParams {
    name: String,
    pad: String,
    #[serde(default)]
    degrees: Option<f64>,
    #[serde(default)]
    axis: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirParams {
    path: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModelParams {
    pub(super) name: String,
    pub(super) path: String,
    #[serde(default)]
    pub(super) offset: Option<[f64; 3]>,
    #[serde(default)]
    pub(super) scale: Option<[f64; 3]>,
    #[serde(default)]
    pub(super) rotate: Option<[f64; 3]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelAutoAlignParams {
    name: String,
    positions: Vec<f64>,
    #[serde(default)]
    indices: Vec<u32>,
    #[serde(default)]
    allow_flip: Option<bool>,
    #[serde(default)]
    apply: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesParams {
    name: String,
    #[serde(default)]
    clearance: Option<f64>,
    #[serde(default)]
    mask_margin: Option<f64>,
    #[serde(default)]
    paste_margin: Option<f64>,
}
/// Input for `component.place_custom`; places a project-library symbol on the schematic.
///
/// # Rules
/// - The symbol name must exist in `Project::library`.
/// - Optional rotation is applied via `turn_placed` after placement.
///
/// # Dependencies
/// - crates/engine/src/lib.rs
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Place {
    name: String,
    position: Point,
    reference: Option<String>,
    value: Option<String>,
    #[serde(default)]
    rotation: Option<f64>,
}
/// Input for `footprint.assign_custom`; binds a project-library footprint to one component.
///
/// # Rules
/// - The footprint name must exist in `Project::library`.
/// - Pad numbers must match the component pins exactly or the arm refuses.
///
/// # Dependencies
/// - crates/engine/src/lib.rs
/// - crates/library/src/lib.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Assign {
    id: Id,
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachStarterModel {
    name: String,
    id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    format: String,
    version: u32,
    library: ProjectLibrary,
}

/// Validate model transforms and build the model reference that
/// `library.footprint.set_model` stores; `library.import_online` shares it.
///
/// # Rules
/// - The path is 1 to 1024 bytes; offsets and rotations are finite and at most 100000 in size; scales are positive.
/// # Do not
/// - Touch the project; this only builds the value.
pub(super) fn build_model(a: &ModelParams) -> Result<pcbcraft_library::Model3D, Error> {
    if a.path.is_empty() || a.path.len() > 1024 {
        return Err(Error::Invalid("model path must be 1 to 1024 bytes".into()));
    }
    let sane = |v: &Option<[f64; 3]>, positive: bool| {
        v.is_none_or(|t| {
            t.iter()
                .all(|x| x.is_finite() && x.abs() <= 100_000.0 && (!positive || *x > 0.0))
        })
    };
    if !sane(&a.offset, false) || !sane(&a.rotate, false) || !sane(&a.scale, true) {
        return Err(Error::Invalid(
            "model transforms must be finite, scales positive".into(),
        ));
    }
    let offset = a.offset.unwrap_or([0.0; 3]);
    let scale = a.scale.unwrap_or([1.0; 3]);
    let rotate = a.rotate.unwrap_or([0.0; 3]);
    let raw_source = format!(
        "(model \"{}\" (offset (xyz {} {} {})) (scale (xyz {} {} {})) (rotate (xyz {} {} {})))",
        a.path,
        offset[0],
        offset[1],
        offset[2],
        scale[0],
        scale[1],
        scale[2],
        rotate[0],
        rotate[1],
        rotate[2]
    );
    Ok(pcbcraft_library::Model3D {
        path: a.path.clone(),
        offset,
        scale,
        rotate,
        raw_source,
    })
}
pub(super) fn run(
    p: &mut Project,
    next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    if !commands().iter().any(|c| c.id == command) {
        return None;
    }
    Some((|| match command {
        "library.symbol.upsert" => {
            let a: SymbolParams = parse(params)?;
            a.definition
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let name = a.definition.name.clone();
            if let Some(notice) = a.notice
                && !p.library.notices.contains(&notice)
            {
                p.library.notices.push(notice);
            }
            if let Some(def) = p.library.symbols.iter_mut().find(|s| s.name == name) {
                *def = a.definition;
            } else {
                p.library.symbols.push(a.definition);
            }
            p.library.symbols.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(json!({"name":name}))
        }
        "library.footprint.upsert" => {
            let a: FootprintParams = parse(params)?;
            a.definition
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let name = a.definition.name.clone();
            if let Some(notice) = a.notice
                && !p.library.notices.contains(&notice)
            {
                p.library.notices.push(notice);
            }
            if let Some(def) = p.library.footprints.iter_mut().find(|s| s.name == name) {
                *def = a.definition;
            } else {
                p.library.footprints.push(a.definition);
            }
            p.library.footprints.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(json!({"name":name}))
        }
        "library.symbol.remove" | "library.footprint.remove" => {
            let a: Name = parse(params)?;
            if command == "library.symbol.remove" {
                let before = p.library.symbols.len();
                p.library.symbols.retain(|s| s.name != a.name);
                if before == p.library.symbols.len() {
                    return Err(missing());
                }
                p.library.part_substitutes.remove(&a.name);
                for subs in p.library.part_substitutes.values_mut() {
                    subs.alternates.retain(|alt| alt.symbol != a.name);
                }
                p.library
                    .part_substitutes
                    .retain(|_, subs| !subs.is_empty());
            } else {
                let before = p.library.footprints.len();
                p.library.footprints.retain(|s| s.name != a.name);
                if before == p.library.footprints.len() {
                    return Err(missing());
                }
                for subs in p.library.part_substitutes.values_mut() {
                    for alt in &mut subs.alternates {
                        if alt.footprint == a.name {
                            alt.footprint.clear();
                        }
                    }
                }
            }
            Ok(json!({"name":a.name}))
        }
        "library.footprint.set_model" => {
            let a: ModelParams = parse(params)?;
            let model = build_model(&a)?;
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            def.model = Some(model);
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(json!({"name": a.name, "path": def.model.as_ref().map(|m| m.path.clone())}))
        }
        "library.footprint.attach_starter_model" => {
            let a: AttachStarterModel = parse(params)?;
            let starter = match a.id.as_deref() {
                Some(id) => pcbcraft_library::starter_models::by_id(id)
                    .ok_or_else(|| Error::Invalid("unknown starter model id".into()))?,
                None => pcbcraft_library::starter_models::for_footprint(&a.name)
                    .ok_or_else(|| {
                        Error::Invalid(
                            "no bundled starter model matches this footprint name".into(),
                        )
                    })?,
            };
            let cache = pcbcraft_library::cad_cache::default_cache_dir().ok_or_else(|| {
                Error::Invalid("a user cache directory is required to store starter models".into())
            })?;
            let path = pcbcraft_io::materialize(starter, &cache)
                .map_err(|e| Error::Invalid(e))?;
            let path_text = path.to_string_lossy().into_owned();
            let model = build_model(&ModelParams {
                name: a.name.clone(),
                path: path_text.clone(),
                offset: None,
                scale: None,
                rotate: None,
            })?;
            let notice = starter.notice.to_string();
            if !notice.is_empty() && !p.library.notices.contains(&notice) {
                p.library.notices.push(notice);
            }
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            def.model = Some(model);
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(json!({
                "name": a.name,
                "starter": starter.id,
                "path": path_text,
            }))
        }
        "library.footprint.clear_model" => {
            let a: Name = parse(params)?;
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            if def.model.is_none() {
                return Err(missing());
            }
            def.model = None;
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(json!({"name": a.name}))
        }
        "library.footprint.model_auto_align" => {
            let a: ModelAutoAlignParams = parse(params)?;
            if a.positions.len() % 3 != 0 || a.positions.is_empty() {
                return Err(Error::Invalid(
                    "positions must be a non-empty list of x,y,z triples".into(),
                ));
            }
            if a.positions.len() / 3 > pcbcraft_io::MAX_MODEL_VERTICES {
                return Err(Error::Invalid("the model has too many vertices".into()));
            }
            if !a.indices.is_empty() && a.indices.len() % 3 != 0 {
                return Err(Error::Invalid(
                    "indices must come in groups of three".into(),
                ));
            }
            let positions: Vec<[f64; 3]> = a
                .positions
                .chunks(3)
                .map(|c| [c[0], c[1], c[2]])
                .collect();
            if positions.iter().any(|p| p.iter().any(|v| !v.is_finite())) {
                return Err(Error::Invalid("positions must be finite numbers".into()));
            }
            let mesh = pcbcraft_io::LoadedModel {
                positions,
                indices: a.indices,
                color: None,
            };
            let def = p
                .library
                .footprints
                .iter()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            let current = def.model.clone().ok_or_else(|| {
                Error::Invalid("the footprint has no model reference yet".into())
            })?;
            let allow_flip = a.allow_flip.unwrap_or(true);
            let out = pcbcraft_io::auto_align_model(&mesh, def, &current, allow_flip);
            let apply = a.apply.unwrap_or(true);
            if apply {
                let def = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|s| s.name == a.name)
                    .ok_or_else(missing)?;
                def.model = Some(out.model.clone());
                def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            }
            Ok(json!({
                "name": a.name,
                "applied": apply,
                "residual_mm": out.residual_mm,
                "coarse": out.coarse,
                "bbox_fallback": out.bbox_fallback,
                "offset": out.model.offset,
                "scale": out.model.scale,
                "rotate": out.model.rotate,
            }))
        }
        "library.footprint.set_rules" => {
            let a: RulesParams = parse(params)?;
            if a.clearance.is_none() && a.mask_margin.is_none() && a.paste_margin.is_none() {
                return Err(Error::Invalid("set at least one rule override".into()));
            }
            let bounded = |v: &Option<f64>| v.is_none_or(|x| x.is_finite() && x.abs() <= 100.0);
            if !bounded(&a.clearance) || !bounded(&a.mask_margin) || !bounded(&a.paste_margin) {
                return Err(Error::Invalid(
                    "rule overrides must be finite and at most 100 mm".into(),
                ));
            }
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            def.rules = Some(pcbcraft_library::FootprintRules {
                clearance: a.clearance,
                mask_margin: a.mask_margin,
                paste_margin: a.paste_margin,
            });
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(json!({"name": a.name, "rules": def.rules}))
        }
        "library.footprint.clear_rules" => {
            let a: Name = parse(params)?;
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            if def.rules.is_none() {
                return Err(missing());
            }
            def.rules = None;
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(json!({"name": a.name}))
        }
        "library.footprint.generate" => {
            let a: GenerateFootprintCommandParams = parse(params)?;
            let density = LandPatternDensity::parse(&a.density).ok_or_else(|| {
                Error::Invalid("density must be A, B or C (most, nominal or least)".into())
            })?;
            if !a.pitch.is_finite() || !(0.1..=25.4).contains(&a.pitch) {
                return Err(Error::Invalid(
                    "pitch must be between 0.1 and 25.4 millimetres".into(),
                ));
            }
            let lp = land_pattern_params_from_command(&a);
            let definition = generate_footprint(&a.family, density, &lp).map_err(Error::Invalid)?;
            let name = definition.name.clone();
            if let Some(def) = p.library.footprints.iter_mut().find(|s| s.name == name) {
                *def = definition;
            } else {
                p.library.footprints.push(definition);
            }
            p.library.footprints.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(json!({"name": name}))
        }
        "library.part.compare" => {
            let a: PartCompareParams = parse(params)?;
            if a.baseline.is_empty() || a.baseline.len() > 128 {
                return Err(Error::Invalid(
                    "baseline must be 1 to 128 bytes".into(),
                ));
            }
            if a.others.is_empty() || a.others.len() > MAX_PART_COMPARE_OTHERS {
                return Err(Error::Invalid(format!(
                    "others must list 1 to {MAX_PART_COMPARE_OTHERS} symbol names"
                )));
            }
            for name in &a.others {
                if name.is_empty() || name.len() > 128 {
                    return Err(Error::Invalid(
                        "each other name must be 1 to 128 bytes".into(),
                    ));
                }
                if name == &a.baseline {
                    return Err(Error::Invalid(
                        "baseline and an other part must differ".into(),
                    ));
                }
            }
            let baseline_sym = p
                .library
                .symbols
                .iter()
                .find(|s| s.name == a.baseline)
                .cloned()
                .ok_or_else(|| {
                    Error::Invalid(format!(
                        "no project-library symbol named {}",
                        a.baseline
                    ))
                })?;
            let resolve_footprint = |name: &str| {
                p.library
                    .footprints
                    .iter()
                    .find(|f| f.name == name)
                    .cloned()
            };
            let mut compared = Vec::with_capacity(a.others.len());
            for other_name in &a.others {
                let other_sym = p
                    .library
                    .symbols
                    .iter()
                    .find(|s| s.name == *other_name)
                    .cloned()
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "no project-library symbol named {other_name}"
                        ))
                    })?;
                let other_fp = a.footprint_overrides.get(other_name).map(String::as_str);
                compared.push(compare_one_part(
                    &baseline_sym,
                    &other_sym,
                    None,
                    other_fp,
                    &resolve_footprint,
                ));
            }
            Ok(json!({"baseline": a.baseline, "compared": compared}))
        }
        "library.footprint.check" => {
            let a: FootprintCheckParams = parse(params)?;
            let def = if let Some(def) = a.definition {
                def
            } else if let Some(name) = a.name {
                p.library
                    .footprints
                    .iter()
                    .find(|s| s.name == name)
                    .cloned()
                    .ok_or_else(missing)?
            } else {
                return Err(Error::Invalid(
                    "give a footprint name or an inline definition".into(),
                ));
            };
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            let findings = pcbcraft_library::check_footprint(&def);
            Ok(json!({"findings": findings}))
        }
        "library.footprint.generate_courtyard" => {
            let a: CourtyardParams = parse(params)?;
            if !a.margin.is_finite() || !(0.0..=5.0).contains(&a.margin) {
                return Err(Error::Invalid(
                    "the courtyard margin must be 0 to 5 millimetres".into(),
                ));
            }
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            let mut min = Point::new(f64::MAX, f64::MAX);
            let mut max = Point::new(f64::MIN, f64::MIN);
            let grow = |p: &mut Point, q: &mut Point, at: Point, radius: f64| {
                if !at.valid() || !radius.is_finite() || radius < 0.0 {
                    return;
                }
                p.x = p.x.min(at.x - radius);
                p.y = p.y.min(at.y - radius);
                q.x = q.x.max(at.x + radius);
                q.y = q.y.max(at.y + radius);
            };
            for pad in &def.pads {
                let half_x = pad.size.x / 2.0;
                let half_y = pad.size.y / 2.0;
                let (c, s) = (
                    pad.rotation.to_radians().cos(),
                    pad.rotation.to_radians().sin(),
                );
                let reach = (half_x * c).abs() + (half_y * s).abs();
                let reach_y = (half_x * s).abs() + (half_y * c).abs();
                grow(&mut min, &mut max, pad.position, reach.max(reach_y));
            }
            for graphic in &def.graphics {
                if matches!(
                    graphic,
                    pcbcraft_library::Graphic::Rectangle { layer, .. }
                        if layer == "pcbcraft.courtyard"
                ) {
                    continue;
                }
                let (points, radius): (Vec<Point>, f64) = match graphic {
                    pcbcraft_library::Graphic::Line {
                        start, end, width, ..
                    } => (vec![*start, *end], *width / 2.0),
                    pcbcraft_library::Graphic::Rectangle {
                        start, end, width, ..
                    } => (vec![*start, *end], *width / 2.0),
                    pcbcraft_library::Graphic::Circle {
                        center,
                        radius,
                        width,
                        ..
                    } => (vec![*center], *radius + width / 2.0),
                    pcbcraft_library::Graphic::Arc {
                        start,
                        mid,
                        end,
                        width,
                        ..
                    } => (vec![*start, *mid, *end], *width / 2.0),
                    pcbcraft_library::Graphic::Polyline { points, width, .. } => {
                        (points.clone(), *width / 2.0)
                    }
                    _ => (vec![], 0.0),
                };
                for at in points {
                    grow(&mut min, &mut max, at, radius);
                }
            }
            if min.x > max.x || min.y > max.y {
                return Err(Error::Invalid(
                    "this footprint has no pads or artwork to surround".into(),
                ));
            }
            let before = def.graphics.len();
            def.graphics
                .retain(|g| !matches!(g, pcbcraft_library::Graphic::Rectangle { layer, .. } if layer == "pcbcraft.courtyard"));
            def.graphics.push(pcbcraft_library::Graphic::Rectangle {
                start: Point::new(min.x - a.margin, min.y - a.margin),
                end: Point::new(max.x + a.margin, max.y + a.margin),
                width: 0.1,
                fill: String::new(),
                layer: "pcbcraft.courtyard".into(),
            });
            if let Err(e) = def.validate() {
                def.graphics.truncate(before);
                return Err(Error::Invalid(e.to_string()));
            }
            Ok(json!({
                "name": a.name,
                "courtyard": [
                    min.x - a.margin,
                    min.y - a.margin,
                    max.x + a.margin,
                    max.y + a.margin
                ],
                "margin": a.margin
            }))
        }
        "library.footprint.renumber_pads" => {
            let a: RenumberParams = parse(params)?;
            if a.renames.is_empty() || a.renames.len() > 512 {
                return Err(Error::Invalid(
                    "pad renumbering needs 1 to 512 old-new pairs".into(),
                ));
            }
            for r in &a.renames {
                if r.old.is_empty() || r.new.is_empty() || r.old.len() > 32 || r.new.len() > 32 {
                    return Err(Error::Invalid("pad numbers must be 1 to 32 bytes".into()));
                }
            }
            let mut sources: Vec<&str> = a.renames.iter().map(|r| r.old.as_str()).collect();
            sources.sort_unstable();
            sources.dedup();
            if sources.len() != a.renames.len() {
                return Err(Error::Invalid(
                    "each old pad number may appear once in the map".into(),
                ));
            }
            let targets: Vec<&str> = a.renames.iter().map(|r| r.new.as_str()).collect();
            let mut dedup: Vec<&str> = targets.clone();
            dedup.sort_unstable();
            dedup.dedup();
            if dedup.len() != targets.len() {
                return Err(Error::Invalid(
                    "two pads cannot be renumbered to the same number".into(),
                ));
            }
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            for r in &a.renames {
                if !def.pads.iter().any(|pad| pad.number == r.old) {
                    return Err(Error::Invalid(format!(
                        "this footprint has no pad {}",
                        r.old
                    )));
                }
            }
            for target in &targets {
                if !a.renames.iter().any(|r| r.old == **target)
                    && def.pads.iter().any(|pad| pad.number == **target)
                {
                    return Err(Error::Invalid(format!(
                        "pad number {target} is already used by a pad this map leaves alone"
                    )));
                }
            }
            let touched: Vec<(usize, String)> = def
                .pads
                .iter()
                .enumerate()
                .filter(|(_, pad)| a.renames.iter().any(|r| r.old == pad.number))
                .map(|(i, pad)| (i, pad.number.clone()))
                .collect();
            for &(i, _) in &touched {
                def.pads[i].number = String::new();
            }
            for (i, old) in &touched {
                if let Some(r) = a.renames.iter().find(|r| r.old == *old) {
                    def.pads[*i].number = r.new.clone();
                }
            }
            if def.pads.iter().any(|pad| pad.number.is_empty()) {
                return Err(Error::Invalid(
                    "every renamed pad needs a new number".into(),
                ));
            }
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            let affected: Vec<Id> = p
                .components
                .iter()
                .filter(|c| {
                    c.library_footprint
                        .as_ref()
                        .is_some_and(|f| f.definition.name == a.name)
                })
                .map(|c| c.id)
                .collect();
            for c in &mut p.components {
                if !affected.contains(&c.id) {
                    continue;
                }
                if let Some(embedded) = &mut c.library_footprint {
                    for pad in &mut embedded.definition.pads {
                        if let Some(r) = a.renames.iter().find(|r| r.old == pad.number) {
                            pad.number = r.new.clone();
                        }
                    }
                }
                for pin in &mut c.pins {
                    if let Some(r) = a.renames.iter().find(|r| r.old == pin.number) {
                        pin.number = r.new.clone();
                    }
                }
            }
            for f in &mut p.board.footprints {
                if !affected.contains(&f.component) {
                    continue;
                }
                for pad in &mut f.pads {
                    if let Some(r) = a.renames.iter().find(|r| r.old == pad.number) {
                        pad.number = r.new.clone();
                    }
                    let geometry_new = pad.geometry.as_ref().and_then(|geometry| {
                        a.renames
                            .iter()
                            .find(|r| r.old == geometry.number)
                            .map(|r| r.new.clone())
                    });
                    if let (Some(geometry), Some(new)) =
                        (pad.geometry.as_mut(), geometry_new.as_deref())
                    {
                        geometry.number = new.to_string();
                    }
                }
            }
            Ok(json!({
                "name": a.name,
                "renamed": touched.len(),
                "components": affected.len()
            }))
        }
        "library.load" => {
            let a: PathParams = parse(params)?;
            let text = pcbcraft_io::read_text(std::path::Path::new(&a.path))?;
            let file: LibraryFile =
                serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
            if file.format != "pcbcraft-library" || file.version != 1 {
                return Err(Error::Invalid(
                    "unsupported library file format/version".into(),
                ));
            }
            file.library.validate()?;
            p.library = file.library;
            Ok(json!({"symbols":p.library.symbols.len(),"footprints":p.library.footprints.len()}))
        }
        "library.import_footprint_file" | "library.import_symbol_file" => {
            let a: ImportFileParams = parse(params)?;
            let text =
                pcbcraft_io::read_text(std::path::Path::new(&a.path)).map_err(Error::Invalid)?;
            let mut imported: Vec<String> = Vec::new();
            if command == "library.import_footprint_file" {
                let def = pcbcraft_library::read_footprint(&text)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
                imported.push(def.name.clone());
                import_footprint(p, def);
            } else {
                let defs = pcbcraft_library::read_symbols(&text)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                if defs.is_empty() {
                    return Err(Error::Invalid("the library file defines no symbols".into()));
                }
                for def in defs {
                    def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
                    imported.push(def.name.clone());
                    import_symbol(p, def);
                }
            }
            if let Some(notice) = a.notice
                && !p.library.notices.contains(&notice)
            {
                p.library.notices.push(notice);
            }
            p.library.validate()?;
            Ok(json!({"names": imported}))
        }
        "library.import_footprint_dir" => {
            let a: DirParams = parse(params)?;
            let dir = std::path::Path::new(&a.path);
            let entries = std::fs::read_dir(dir).map_err(|e| Error::Invalid(e.to_string()))?;
            let mut files: Vec<std::path::PathBuf> = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|e| Error::Invalid(e.to_string()))?;
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("kicad_mod"))
                {
                    files.push(path);
                }
            }
            files.sort();
            if files.is_empty() {
                return Err(Error::Invalid(
                    "the folder holds no footprint library text files".into(),
                ));
            }
            if files.len() > 256 {
                return Err(Error::Invalid(
                    "a library folder may hold at most 256 footprint files".into(),
                ));
            }
            let mut imported: Vec<String> = Vec::new();
            for path in &files {
                let text = pcbcraft_io::read_text(path).map_err(Error::Invalid)?;
                let def = pcbcraft_library::read_footprint(&text)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
                imported.push(def.name.clone());
                import_footprint(p, def);
            }
            p.library.validate()?;
            Ok(json!({"imported": imported.len(), "names": imported}))
        }
        "library.import_symbol_dir" => {
            let a: DirParams = parse(params)?;
            let dir = std::path::Path::new(&a.path);
            let entries = std::fs::read_dir(dir).map_err(|e| Error::Invalid(e.to_string()))?;
            let mut files: Vec<std::path::PathBuf> = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|e| Error::Invalid(e.to_string()))?;
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("kicad_sym"))
                {
                    files.push(path);
                }
            }
            files.sort();
            if files.is_empty() {
                return Err(Error::Invalid(
                    "the folder holds no symbol library text files".into(),
                ));
            }
            if files.len() > 256 {
                return Err(Error::Invalid(
                    "a library folder may hold at most 256 symbol files".into(),
                ));
            }
            let mut imported: Vec<String> = Vec::new();
            for path in &files {
                let text = pcbcraft_io::read_text(path).map_err(Error::Invalid)?;
                let defs = pcbcraft_library::read_symbols(&text)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                if defs.is_empty() {
                    return Err(Error::Invalid(format!(
                        "{} defines no symbols",
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("a file")
                    )));
                }
                for def in defs {
                    def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
                    imported.push(def.name.clone());
                    import_symbol(p, def);
                }
            }
            p.library.validate()?;
            Ok(json!({"imported": imported.len(), "names": imported}))
        }
        "library.footprint.pad_set" => {
            let a: PadSetParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "footprint name must be 1 to 128 bytes".into(),
                ));
            }
            if a.pad.number.is_empty() || a.pad.number.len() > 32 {
                return Err(Error::Invalid("pad number must be 1 to 32 bytes".into()));
            }
            let number = a.pad.number.clone();
            let pads = {
                let footprint = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == a.name)
                    .ok_or_else(missing)?;
                let replaced = footprint.pads.iter().any(|q| q.number == number);
                if let Some(existing) = footprint.pads.iter_mut().find(|q| q.number == number) {
                    *existing = a.pad;
                } else {
                    footprint.pads.push(a.pad);
                }
                footprint
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                (footprint.pads.len(), !replaced)
            };
            p.library.validate()?;
            Ok(json!({"name": a.name, "pad": number, "added": pads.1, "pads": pads.0}))
        }
        "library.footprint.pad_remove" => {
            let a: PadRemoveParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "footprint name must be 1 to 128 bytes".into(),
                ));
            }
            if a.pad.is_empty() || a.pad.len() > 32 {
                return Err(Error::Invalid("pad number must be 1 to 32 bytes".into()));
            }
            let name = a.name.clone();
            let pads = {
                let footprint = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let before = footprint.pads.len();
                footprint.pads.retain(|q| q.number != a.pad);
                if footprint.pads.len() == before {
                    return Err(Error::Invalid(format!(
                        "footprint {name} has no pad {}",
                        a.pad
                    )));
                }
                if footprint.pads.is_empty() {
                    return Err(Error::Invalid(
                        "a footprint keeps at least one pad; remove the footprint itself instead"
                            .into(),
                    ));
                }
                footprint
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                footprint.pads.len()
            };
            p.library.validate()?;
            Ok(json!({"name": name, "pad": a.pad, "pads": pads}))
        }
        "library.footprint.pad_move" => {
            let a: PadMoveParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "footprint name must be 1 to 128 bytes".into(),
                ));
            }
            if a.pad.is_empty() || a.pad.len() > 32 {
                return Err(Error::Invalid("pad number must be 1 to 32 bytes".into()));
            }
            if !a.delta.x.is_finite()
                || !a.delta.y.is_finite()
                || a.delta.x.abs() > 1000.0
                || a.delta.y.abs() > 1000.0
            {
                return Err(Error::Invalid(
                    "offset must be finite and within 1000 mm".into(),
                ));
            }
            let name = a.name.clone();
            let footprint = p
                .library
                .footprints
                .iter_mut()
                .find(|f| f.name == name)
                .ok_or_else(missing)?;
            let pad = footprint
                .pads
                .iter_mut()
                .find(|q| q.number == a.pad)
                .ok_or_else(|| Error::Invalid(format!("footprint {name} has no pad {}", a.pad)))?;
            let moved = Point::new(pad.position.x + a.delta.x, pad.position.y + a.delta.y);
            if !moved.valid() {
                return Err(Error::Invalid(
                    "the shifted pad left the board coordinate range".into(),
                ));
            }
            pad.position = moved;
            if let Some(drill) = &mut pad.drill {
                let shifted = Point::new(drill.offset.x + a.delta.x, drill.offset.y + a.delta.y);
                if !shifted.valid() {
                    return Err(Error::Invalid(
                        "the shifted drill left the board coordinate range".into(),
                    ));
                }
                drill.offset = shifted;
            }
            footprint
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            p.library.validate()?;
            Ok(json!({"name": name, "pad": a.pad}))
        }
        "library.footprint.pad_rotate" | "library.footprint.pad_mirror" => {
            let a: PadTurnParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "footprint name must be 1 to 128 bytes".into(),
                ));
            }
            if a.pad.is_empty() || a.pad.len() > 32 {
                return Err(Error::Invalid("pad number must be 1 to 32 bytes".into()));
            }
            let name = a.name.clone();
            let footprint = p
                .library
                .footprints
                .iter_mut()
                .find(|f| f.name == name)
                .ok_or_else(missing)?;
            let pad = footprint
                .pads
                .iter_mut()
                .find(|q| q.number == a.pad)
                .ok_or_else(|| Error::Invalid(format!("footprint {name} has no pad {}", a.pad)))?;
            if command == "library.footprint.pad_rotate" {
                if !a.degrees.is_some_and(f64::is_finite) || a.degrees.unwrap_or(0.0).abs() > 1e5 {
                    return Err(Error::Invalid(
                        "rotation must be finite and within 100000 degrees".into(),
                    ));
                }
                if a.axis.is_some() {
                    return Err(Error::Invalid("rotation needs degrees, not an axis".into()));
                }
                let rad = a.degrees.unwrap_or(0.0).to_radians();
                pad.position = rot(pad.position, rad.cos(), rad.sin())?;
                pad.rotation = turn(pad.rotation, rad.sin())?;
                if let Some(drill) = &mut pad.drill {
                    drill.offset = rot(drill.offset, rad.cos(), rad.sin())?;
                }
            } else {
                if a.axis.as_deref() != Some("y") {
                    return Err(Error::Invalid("mirror needs axis \"y\"".into()));
                }
                if a.degrees.is_some() {
                    return Err(Error::Invalid("mirror needs an axis, not degrees".into()));
                }
                pad.position = flip_x(pad.position);
                pad.rotation = flip_angle(pad.rotation)?;
                if let Some(drill) = &mut pad.drill {
                    drill.offset = flip_x(drill.offset);
                }
            }
            footprint
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            p.library.validate()?;
            Ok(json!({"name": name, "pad": a.pad}))
        }
        "library.footprint.duplicate" | "library.symbol.duplicate" => {
            let a: DuplicateParams = parse(params)?;
            if a.name.is_empty()
                || a.new_name.is_empty()
                || a.name.len() > 128
                || a.new_name.len() > 128
            {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            if a.name == a.new_name {
                return Err(Error::Invalid("the copy needs a name of its own".into()));
            }
            let foot = command == "library.footprint.duplicate";
            if foot {
                let source = p
                    .library
                    .footprints
                    .iter()
                    .find(|f| f.name == a.name)
                    .ok_or_else(missing)?
                    .clone();
                if p.library.footprints.iter().any(|f| f.name == a.new_name) {
                    return Err(Error::Invalid(format!(
                        "a footprint named {} already exists",
                        a.new_name
                    )));
                }
                let mut copy = source;
                copy.name = a.new_name.clone();
                copy.validate().map_err(|e| Error::Invalid(e.to_string()))?;
                p.library.footprints.push(copy);
                p.library.footprints.sort_by(|x, y| x.name.cmp(&y.name));
            } else {
                let source = p
                    .library
                    .symbols
                    .iter()
                    .find(|f| f.name == a.name)
                    .ok_or_else(missing)?
                    .clone();
                if p.library.symbols.iter().any(|f| f.name == a.new_name) {
                    return Err(Error::Invalid(format!(
                        "a symbol named {} already exists",
                        a.new_name
                    )));
                }
                let mut copy = source;
                copy.name = a.new_name.clone();
                copy.validate().map_err(|e| Error::Invalid(e.to_string()))?;
                p.library.symbols.push(copy);
                p.library.symbols.sort_by(|x, y| x.name.cmp(&y.name));
            }
            p.library.validate()?;
            Ok(json!({"name": a.name, "new_name": a.new_name}))
        }
        "library.footprint.adopt" | "library.symbol.adopt" => {
            let a: AdoptParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            let foot = command == "library.footprint.adopt";
            let component = p
                .components
                .iter()
                .find(|c| c.id == a.id)
                .ok_or_else(missing)?;
            if foot {
                let source = component
                    .library_footprint
                    .as_ref()
                    .map(|f| f.definition.clone())
                    .ok_or_else(|| {
                        Error::Invalid("that component carries no embedded footprint".into())
                    })?;
                let mut def = source;
                def.name = a.name.clone();
                def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
                import_footprint(p, def);
            } else {
                let source = component
                    .library_symbol
                    .as_ref()
                    .map(|f| f.definition.clone())
                    .ok_or_else(|| {
                        Error::Invalid("that component carries no embedded symbol".into())
                    })?;
                let mut def = source;
                def.name = a.name.clone();
                def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
                import_symbol(p, def);
            }
            p.library.validate()?;
            Ok(json!({"id": a.id, "name": a.name}))
        }
        "library.symbol.pin_set" => {
            let a: PinSetParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid("symbol name must be 1 to 128 bytes".into()));
            }
            if a.pin.number.is_empty() || a.pin.number.len() > 32 {
                return Err(Error::Invalid("pin number must be 1 to 32 bytes".into()));
            }
            let number = a.pin.number.clone();
            let pins = {
                let symbol = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|f| f.name == a.name)
                    .ok_or_else(missing)?;
                let replaced = symbol.pins.iter().any(|q| q.number == number);
                if let Some(existing) = symbol.pins.iter_mut().find(|q| q.number == number) {
                    *existing = a.pin;
                } else {
                    symbol.pins.push(a.pin);
                }
                symbol
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                (symbol.pins.len(), !replaced)
            };
            p.library.validate()?;
            Ok(json!({"name": a.name, "pin": number, "added": pins.1, "pins": pins.0}))
        }
        "library.symbol.pin_remove" => {
            let a: PinRemoveParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid("symbol name must be 1 to 128 bytes".into()));
            }
            if a.pin.is_empty() || a.pin.len() > 32 {
                return Err(Error::Invalid("pin number must be 1 to 32 bytes".into()));
            }
            let name = a.name.clone();
            let pins = {
                let symbol = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let before = symbol.pins.len();
                symbol.pins.retain(|q| q.number != a.pin);
                if symbol.pins.len() == before {
                    return Err(Error::Invalid(format!(
                        "symbol {name} has no pin {}",
                        a.pin
                    )));
                }
                if symbol.pins.is_empty() {
                    return Err(Error::Invalid(format!(
                        "symbol {name} would lose its last pin"
                    )));
                }
                symbol
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                symbol.pins.len()
            };
            p.library.validate()?;
            Ok(json!({"name": name, "pin": a.pin, "pins": pins}))
        }
        "library.symbol.pin_move" => {
            let a: PinMoveParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid("symbol name must be 1 to 128 bytes".into()));
            }
            if a.pin.is_empty() || a.pin.len() > 32 {
                return Err(Error::Invalid("pin number must be 1 to 32 bytes".into()));
            }
            if !a.delta.x.is_finite()
                || !a.delta.y.is_finite()
                || a.delta.x.abs() > 1000.0
                || a.delta.y.abs() > 1000.0
            {
                return Err(Error::Invalid(
                    "offset must be finite and within 1000 mm".into(),
                ));
            }
            let name = a.name.clone();
            let symbol = p
                .library
                .symbols
                .iter_mut()
                .find(|f| f.name == name)
                .ok_or_else(missing)?;
            let pin = symbol
                .pins
                .iter_mut()
                .find(|q| q.number == a.pin)
                .ok_or_else(|| Error::Invalid(format!("symbol {name} has no pin {}", a.pin)))?;
            let moved = Point::new(pin.position.x + a.delta.x, pin.position.y + a.delta.y);
            if !moved.valid() {
                return Err(Error::Invalid(
                    "the shifted pin left the board coordinate range".into(),
                ));
            }
            pin.position = moved;
            symbol
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            p.library.validate()?;
            Ok(json!({"name": name, "pin": a.pin}))
        }
        "library.symbol.pin_rotate" | "library.symbol.pin_mirror" => {
            let a: PinTurnParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid("symbol name must be 1 to 128 bytes".into()));
            }
            if a.pin.is_empty() || a.pin.len() > 32 {
                return Err(Error::Invalid("pin number must be 1 to 32 bytes".into()));
            }
            let name = a.name.clone();
            let symbol = p
                .library
                .symbols
                .iter_mut()
                .find(|f| f.name == name)
                .ok_or_else(missing)?;
            let pin = symbol
                .pins
                .iter_mut()
                .find(|q| q.number == a.pin)
                .ok_or_else(|| Error::Invalid(format!("symbol {name} has no pin {}", a.pin)))?;
            if command == "library.symbol.pin_rotate" {
                if !a.degrees.is_some_and(f64::is_finite) || a.degrees.unwrap_or(0.0).abs() > 1e5 {
                    return Err(Error::Invalid(
                        "rotation must be finite and within 100000 degrees".into(),
                    ));
                }
                if a.axis.is_some() {
                    return Err(Error::Invalid("rotation needs degrees, not an axis".into()));
                }
                let rad = a.degrees.unwrap_or(0.0).to_radians();
                pin.position = rot(pin.position, rad.cos(), rad.sin())?;
                pin.angle = turn(pin.angle, rad.sin())?;
            } else {
                if a.axis.as_deref() != Some("y") {
                    return Err(Error::Invalid("mirror needs axis \"y\"".into()));
                }
                if a.degrees.is_some() {
                    return Err(Error::Invalid("mirror needs an axis, not degrees".into()));
                }
                pin.position = flip_x(pin.position);
                pin.angle = flip_angle(pin.angle)?;
            }
            symbol
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            p.library.validate()?;
            Ok(json!({"name": name, "pin": a.pin}))
        }
        "library.symbol.renumber_pins" => {
            let a: RenumberParams = parse(params)?;
            if a.renames.is_empty() || a.renames.len() > 512 {
                return Err(Error::Invalid(
                    "pin renumbering needs 1 to 512 old-new pairs".into(),
                ));
            }
            for r in &a.renames {
                if r.old.is_empty() || r.new.is_empty() || r.old.len() > 32 || r.new.len() > 32 {
                    return Err(Error::Invalid("pin numbers must be 1 to 32 bytes".into()));
                }
            }
            let mut sources: Vec<&str> = a.renames.iter().map(|r| r.old.as_str()).collect();
            sources.sort_unstable();
            sources.dedup();
            if sources.len() != a.renames.len() {
                return Err(Error::Invalid(
                    "each old pin number may appear once in the map".into(),
                ));
            }
            let targets: Vec<&str> = a.renames.iter().map(|r| r.new.as_str()).collect();
            let mut dedup: Vec<&str> = targets.clone();
            dedup.sort_unstable();
            dedup.dedup();
            if dedup.len() != targets.len() {
                return Err(Error::Invalid(
                    "two pins cannot be renumbered to the same number".into(),
                ));
            }
            let def = p
                .library
                .symbols
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            for r in &a.renames {
                if !def.pins.iter().any(|pin| pin.number == r.old) {
                    return Err(Error::Invalid(format!("this symbol has no pin {}", r.old)));
                }
            }
            for target in &targets {
                if !a.renames.iter().any(|r| r.old == **target)
                    && def.pins.iter().any(|pin| pin.number == **target)
                {
                    return Err(Error::Invalid(format!(
                        "pin number {target} is already used by a pin this map leaves alone"
                    )));
                }
            }
            let touched: Vec<(usize, String)> = def
                .pins
                .iter()
                .enumerate()
                .filter(|(_, pin)| a.renames.iter().any(|r| r.old == pin.number))
                .map(|(i, pin)| (i, pin.number.clone()))
                .collect();
            for &(i, _) in &touched {
                def.pins[i].number = String::new();
            }
            for (i, old) in &touched {
                if let Some(r) = a.renames.iter().find(|r| r.old == *old) {
                    def.pins[*i].number = r.new.clone();
                }
            }
            if def.pins.iter().any(|pin| pin.number.is_empty()) {
                return Err(Error::Invalid(
                    "every renamed pin needs a new number".into(),
                ));
            }
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            let affected: Vec<Id> = p
                .components
                .iter()
                .filter(|c| {
                    c.library_symbol
                        .as_ref()
                        .is_some_and(|sym| sym.definition.name == a.name)
                })
                .map(|c| c.id)
                .collect();
            for c in &mut p.components {
                if !affected.contains(&c.id) {
                    continue;
                }
                if let Some(embedded) = &mut c.library_symbol {
                    for pin in &mut embedded.definition.pins {
                        if let Some(r) = a.renames.iter().find(|r| r.old == pin.number) {
                            pin.number = r.new.clone();
                        }
                    }
                }
                for pin in &mut c.pins {
                    if let Some(r) = a.renames.iter().find(|r| r.old == pin.number) {
                        pin.number = r.new.clone();
                    }
                }
            }
            for flag in &mut p.no_connects {
                if !affected.contains(&flag.pin.component) {
                    continue;
                }
                if let Some(r) = a.renames.iter().find(|r| r.old == flag.pin.pin) {
                    flag.pin.pin = r.new.clone();
                }
            }
            Ok(json!({
                "name": a.name,
                "renamed": touched.len(),
                "components": affected.len()
            }))
        }
        "library.footprint.graphic_add" => {
            let a: GraphicAddParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "footprint name must be 1 to 128 bytes".into(),
                ));
            }
            let layer = graphic_layer(&a.graphic)?;
            let graphics = {
                let footprint = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == a.name)
                    .ok_or_else(missing)?;
                footprint.graphics.push(a.graphic);
                footprint
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                footprint.graphics.len()
            };
            p.library.validate()?;
            Ok(json!({"name": a.name, "layer": layer, "graphics": graphics}))
        }
        "library.footprint.graphic_clear" => {
            let a: GraphicClearParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "footprint name must be 1 to 128 bytes".into(),
                ));
            }
            if a.layer.as_deref().is_some_and(str::is_empty) {
                return Err(Error::Invalid("layer must not be empty".into()));
            }
            let name = a.name.clone();
            let graphics = {
                let footprint = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let before = footprint.graphics.len();
                for graphic in &footprint.graphics {
                    graphic_layer(graphic)?;
                }
                let kept: Vec<Graphic> = match &a.layer {
                    Some(layer) => footprint
                        .graphics
                        .iter()
                        .filter(|g| graphic_layer(g).is_ok_and(|l| l != *layer))
                        .cloned()
                        .collect(),
                    None => Vec::new(),
                };
                footprint.graphics = kept;
                if footprint.graphics.len() == before {
                    return Err(Error::Invalid(format!(
                        "footprint {name} has no graphics to remove"
                    )));
                }
                footprint
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                (before, footprint.graphics.len())
            };
            p.library.validate()?;
            Ok(json!({
                "name": name,
                "removed": graphics.0 - graphics.1,
                "graphics": graphics.1
            }))
        }
        "library.symbol.graphic_add" => {
            let a: SymbolGraphicAddParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid("symbol name must be 1 to 128 bytes".into()));
            }
            let unit = a.unit.unwrap_or(1);
            let body_style = a.body_style.unwrap_or(1);
            let layer = graphic_layer(&a.graphic)?;
            let graphics = {
                let symbol = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|f| f.name == a.name)
                    .ok_or_else(missing)?;
                symbol.graphics.push(UnitGraphic {
                    unit,
                    body_style,
                    primitive: a.graphic,
                });
                symbol
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                symbol.graphics.len()
            };
            p.library.validate()?;
            Ok(json!({"name": a.name, "unit": unit, "layer": layer, "graphics": graphics}))
        }
        "library.symbol.graphic_clear" => {
            let a: SymbolGraphicClearParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid("symbol name must be 1 to 128 bytes".into()));
            }
            let unit = a.unit.unwrap_or(1);
            let name = a.name.clone();
            let graphics = {
                let symbol = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let before = symbol.graphics.len();
                symbol.graphics.retain(|g| g.unit != unit);
                if symbol.graphics.len() == before {
                    return Err(Error::Invalid(format!(
                        "symbol {name} has no graphics on unit {unit}"
                    )));
                }
                symbol
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                (before, symbol.graphics.len())
            };
            p.library.validate()?;
            Ok(json!({
                "name": name,
                "unit": unit,
                "removed": graphics.0 - graphics.1,
                "graphics": graphics.1
            }))
        }
        "library.footprint.graphic_set" | "library.symbol.graphic_set" => {
            let a: GraphicSetParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            let layer = graphic_layer(&a.graphic)?;
            let name = a.name.clone();
            let graphics = if command == "library.symbol.graphic_set" {
                let symbol = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let Some(art) = symbol.graphics.get_mut(a.index) else {
                    return Err(Error::Invalid(format!(
                        "artwork position {} is outside the 0 to {} this symbol has",
                        a.index,
                        symbol.graphics.len().saturating_sub(1)
                    )));
                };
                art.primitive = a.graphic;
                symbol
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                symbol.graphics.len()
            } else {
                let footprint = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let Some(art) = footprint.graphics.get_mut(a.index) else {
                    return Err(Error::Invalid(format!(
                        "artwork position {} is outside the 0 to {} this package has",
                        a.index,
                        footprint.graphics.len().saturating_sub(1)
                    )));
                };
                *art = a.graphic;
                footprint
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                footprint.graphics.len()
            };
            p.library.validate()?;
            Ok(json!({"name": name, "index": a.index, "layer": layer, "graphics": graphics}))
        }
        "library.footprint.graphic_remove" | "library.symbol.graphic_remove" => {
            let a: GraphicRemoveParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            let name = a.name.clone();
            let graphics = if command == "library.symbol.graphic_remove" {
                let symbol = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                if a.index >= symbol.graphics.len() {
                    return Err(Error::Invalid(format!(
                        "artwork position {} is outside the 0 to {} this symbol has",
                        a.index,
                        symbol.graphics.len().saturating_sub(1)
                    )));
                }
                symbol.graphics.remove(a.index);
                symbol
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                symbol.graphics.len()
            } else {
                let footprint = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                if a.index >= footprint.graphics.len() {
                    return Err(Error::Invalid(format!(
                        "artwork position {} is outside the 0 to {} this package has",
                        a.index,
                        footprint.graphics.len().saturating_sub(1)
                    )));
                }
                footprint.graphics.remove(a.index);
                footprint
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                footprint.graphics.len()
            };
            p.library.validate()?;
            Ok(json!({"name": name, "index": a.index, "graphics": graphics}))
        }
        "library.footprint.graphic_move" | "library.symbol.graphic_move" => {
            let a: GraphicMoveParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            if !a.delta.x.is_finite()
                || !a.delta.y.is_finite()
                || a.delta.x.abs() > 1000.0
                || a.delta.y.abs() > 1000.0
            {
                return Err(Error::Invalid(
                    "offset must be finite and within 1000 mm".into(),
                ));
            }
            let shift = |q: Point| -> Result<Point, Error> {
                let moved = Point::new(q.x + a.delta.x, q.y + a.delta.y);
                if moved.valid() {
                    Ok(moved)
                } else {
                    Err(Error::Invalid(
                        "the shifted graphic left the board coordinate range".into(),
                    ))
                }
            };
            let name = a.name.clone();
            if command == "library.symbol.graphic_move" {
                let symbol = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let Some(art) = symbol.graphics.get_mut(a.index) else {
                    return Err(Error::Invalid(format!(
                        "artwork position {} is outside the 0 to {} this symbol has",
                        a.index,
                        symbol.graphics.len().saturating_sub(1)
                    )));
                };
                shift_graphic(&mut art.primitive, &shift)?;
                symbol
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
            } else {
                let footprint = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let Some(art) = footprint.graphics.get_mut(a.index) else {
                    return Err(Error::Invalid(format!(
                        "artwork position {} is outside the 0 to {} this package has",
                        a.index,
                        footprint.graphics.len().saturating_sub(1)
                    )));
                };
                shift_graphic(art, &shift)?;
                footprint
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
            }
            p.library.validate()?;
            Ok(json!({"name": name, "index": a.index}))
        }
        "library.footprint.graphic_rotate"
        | "library.symbol.graphic_rotate"
        | "library.footprint.graphic_mirror"
        | "library.symbol.graphic_mirror" => {
            let a: GraphicTurnParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            let symbol = command.starts_with("library.symbol.");
            let name = a.name.clone();
            let graphic = if symbol {
                let item = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let Some(art) = item.graphics.get_mut(a.index) else {
                    return Err(Error::Invalid(format!(
                        "artwork position {} is outside the 0 to {} this symbol has",
                        a.index,
                        item.graphics.len().saturating_sub(1)
                    )));
                };
                &mut art.primitive
            } else {
                let item = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let Some(art) = item.graphics.get_mut(a.index) else {
                    return Err(Error::Invalid(format!(
                        "artwork position {} is outside the 0 to {} this package has",
                        a.index,
                        item.graphics.len().saturating_sub(1)
                    )));
                };
                art
            };
            if command.ends_with("graphic_rotate") {
                if !a.degrees.is_some_and(f64::is_finite) || a.degrees.unwrap_or(0.0).abs() > 1e5 {
                    return Err(Error::Invalid(
                        "rotation must be finite and within 100000 degrees".into(),
                    ));
                }
                if a.axis.is_some() {
                    return Err(Error::Invalid("rotation needs degrees, not an axis".into()));
                }
                let rad = a.degrees.unwrap_or(0.0).to_radians();
                rotate_graphic(graphic, rad.cos(), rad.sin())?;
            } else {
                if a.axis.as_deref() != Some("y") {
                    return Err(Error::Invalid("mirror needs axis \"y\"".into()));
                }
                if a.degrees.is_some() {
                    return Err(Error::Invalid("mirror needs an axis, not degrees".into()));
                }
                mirror_graphic(graphic)?;
            }
            if symbol {
                let item = p
                    .library
                    .symbols
                    .iter()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                item.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            } else {
                let item = p
                    .library
                    .footprints
                    .iter()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                item.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            }
            p.library.validate()?;
            Ok(json!({"name": name, "index": a.index}))
        }
        "library.footprint.graphic_scale" | "library.symbol.graphic_scale" => {
            let a: GraphicScaleParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            if !a.factor.is_finite() || a.factor <= 0.0 || a.factor > 1000.0 {
                return Err(Error::Invalid(
                    "factor must be finite and within 0 to 1000".into(),
                ));
            }
            let k = a.factor;
            let scale = |q: Point| -> Result<Point, Error> {
                let grown = Point::new(q.x * k, q.y * k);
                if grown.valid() {
                    Ok(grown)
                } else {
                    Err(Error::Invalid(
                        "the scaled graphic left the board coordinate range".into(),
                    ))
                }
            };
            let scale_dim = |v: f64| -> Result<f64, Error> {
                let grown = v * k;
                if grown.is_finite() {
                    Ok(grown)
                } else {
                    Err(Error::Invalid("scaled size left range".into()))
                }
            };
            let name = a.name.clone();
            if command == "library.symbol.graphic_scale" {
                let symbol = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let Some(art) = symbol.graphics.get_mut(a.index) else {
                    return Err(Error::Invalid(format!(
                        "artwork position {} is outside the 0 to {} this symbol has",
                        a.index,
                        symbol.graphics.len().saturating_sub(1)
                    )));
                };
                scale_graphic(&mut art.primitive, &scale, scale_dim)?;
                symbol
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
            } else {
                let footprint = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let Some(art) = footprint.graphics.get_mut(a.index) else {
                    return Err(Error::Invalid(format!(
                        "artwork position {} is outside the 0 to {} this package has",
                        a.index,
                        footprint.graphics.len().saturating_sub(1)
                    )));
                };
                scale_graphic(art, &scale, scale_dim)?;
                footprint
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
            }
            p.library.validate()?;
            Ok(json!({"name": name, "index": a.index}))
        }
        "library.symbol.set_properties" | "library.footprint.set_properties" => {
            let a: SetPropertyParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            let symbol = command == "library.symbol.set_properties";
            let touched = a.reference.is_some()
                || a.value.is_some()
                || a.description.is_some()
                || a.keywords.is_some()
                || a.footprint.is_some()
                || a.set.is_some();
            if !touched {
                return Err(Error::Invalid(
                    "give at least one property to change".into(),
                ));
            }
            for text in [
                a.reference.as_deref(),
                a.value.as_deref(),
                a.description.as_deref(),
                a.keywords.as_deref(),
                a.footprint.as_deref(),
                a.set.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if text.len() > 128 * 1024 {
                    return Err(Error::Invalid(
                        "library text fields are capped at 128 KiB".into(),
                    ));
                }
            }
            let name = a.name.clone();
            let item = if symbol {
                let symbol = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                if let Some(v) = a.reference {
                    symbol.reference = v;
                }
                if let Some(v) = a.value {
                    symbol.value = v;
                }
                if let Some(v) = a.description {
                    symbol.description = v;
                }
                if let Some(v) = a.keywords {
                    symbol.keywords = v;
                }
                if let Some(v) = a.footprint {
                    symbol.footprint = v;
                }
                if let Some(v) = a.set {
                    symbol.set = v;
                }
                symbol
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                symbol.name.clone()
            } else {
                let footprint = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                if a.reference.is_some()
                    || a.value.is_some()
                    || a.footprint.is_some()
                    || a.set.is_some()
                {
                    return Err(Error::Invalid(
                        "footprints carry description and keywords only".into(),
                    ));
                }
                if let Some(v) = a.description {
                    footprint.description = v;
                }
                if let Some(v) = a.keywords {
                    footprint.keywords = v;
                }
                footprint
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                footprint.name.clone()
            };
            p.library.validate()?;
            Ok(json!({"name": item}))
        }
        "library.part.set_substitute" => {
            let a: SetPartSubstituteParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            if !p.library.symbols.iter().any(|s| s.name == a.name) {
                return Err(Error::Invalid(format!(
                    "no project-library symbol named {}",
                    a.name
                )));
            }
            let symbol_exists =
                |sym: &str| p.library.symbols.iter().any(|s| s.name == sym);
            let footprint_exists =
                |foot: &str| p.library.footprints.iter().any(|f| f.name == foot);
            let record = p
                .library
                .part_substitutes
                .entry(a.name.clone())
                .or_insert_with(PartSubstitutes::default);
            match a.kind.as_str() {
                "second_source" => {
                    let manufacturer = a.manufacturer.filter(|s| !s.is_empty()).ok_or_else(|| {
                        Error::Invalid("second_source needs a manufacturer".into())
                    })?;
                    let mpn = match a.mpn {
                        None => {
                            return Err(Error::Invalid(
                                "second_source needs mpn or null to remove".into(),
                            ));
                        }
                        Some(Value::Null) => None,
                        Some(Value::String(text)) => Some(text.as_str()),
                        Some(_) => {
                            return Err(Error::Invalid("mpn must be a string or null".into()));
                        }
                    };
                    record
                        .set_second_source(&manufacturer, mpn, a.note.as_deref())
                        .map_err(Error::Invalid)?;
                }
                "alternate" => {
                    let symbol = a.symbol.filter(|s| !s.is_empty()).ok_or_else(|| {
                        Error::Invalid("alternate needs the substitute symbol name".into())
                    })?;
                    if a.clear {
                        record.remove_alternate(&symbol).map_err(Error::Invalid)?;
                    } else {
                        if symbol == a.name {
                            return Err(Error::Invalid(
                                "a part cannot list itself as an alternate".into(),
                            ));
                        }
                        record
                            .set_alternate(
                                &symbol,
                                a.footprint.as_deref(),
                                a.note.as_deref(),
                                symbol_exists,
                                footprint_exists,
                            )
                            .map_err(Error::Invalid)?;
                    }
                }
                _ => {
                    return Err(Error::Invalid(
                        "kind must be second_source or alternate".into(),
                    ));
                }
            }
            if record.is_empty() {
                p.library.part_substitutes.remove(&a.name);
            }
            p.library.validate()?;
            Ok(json!({"name": a.name, "kind": a.kind}))
        }
        "library.symbol.rename" | "library.footprint.rename" => {
            let a: RenameParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            if a.new_name.is_empty() || a.new_name.len() > 128 {
                return Err(Error::Invalid("new name must be 1 to 128 bytes".into()));
            }
            if command == "library.symbol.rename" {
                let Some(found) = p.library.symbols.iter().position(|i| i.name == a.name) else {
                    return Err(missing());
                };
                if p.library.symbols.iter().any(|i| i.name == a.new_name) {
                    return Err(Error::Invalid(format!(
                        "a library item is already named {}",
                        a.new_name
                    )));
                }
                p.library.symbols[found].name = a.new_name.clone();
                if let Some(subs) = p.library.part_substitutes.remove(&a.name) {
                    p.library
                        .part_substitutes
                        .insert(a.new_name.clone(), subs);
                }
                for subs in p.library.part_substitutes.values_mut() {
                    for alt in &mut subs.alternates {
                        if alt.symbol == a.name {
                            alt.symbol = a.new_name.clone();
                        }
                    }
                }
            } else {
                let Some(found) = p.library.footprints.iter().position(|i| i.name == a.name) else {
                    return Err(missing());
                };
                if p.library.footprints.iter().any(|i| i.name == a.new_name) {
                    return Err(Error::Invalid(format!(
                        "a library item is already named {}",
                        a.new_name
                    )));
                }
                p.library.footprints[found].name = a.new_name.clone();
                for subs in p.library.part_substitutes.values_mut() {
                    for alt in &mut subs.alternates {
                        if alt.footprint == a.name {
                            alt.footprint = a.new_name.clone();
                        }
                    }
                }
            }
            p.library.validate()?;
            p.validate()?;
            Ok(json!({"name": a.new_name}))
        }
        "library.symbol.set_extends" | "library.symbol.clear_extends" => {
            let a: SymbolExtendsParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid("symbol names must be 1 to 128 bytes".into()));
            }
            let parent = if command == "library.symbol.set_extends" {
                let parent = a.extends.as_deref().unwrap_or_default();
                if parent.is_empty() || parent.len() > 128 {
                    return Err(Error::Invalid(
                        "parent symbol name must be 1 to 128 bytes".into(),
                    ));
                }
                Some(parent.to_owned())
            } else {
                None
            };
            let name = a.name.clone();
            if let Some(parent) = &parent {
                if *parent == name {
                    return Err(Error::Invalid("a symbol cannot inherit from itself".into()));
                }
                if !p.library.symbols.iter().any(|s| s.name == *parent) {
                    return Err(Error::Invalid(format!(
                        "no symbol named {parent} to inherit from"
                    )));
                }
            }
            let symbol = p
                .library
                .symbols
                .iter_mut()
                .find(|s| s.name == name)
                .ok_or_else(missing)?;
            symbol.extends = parent;
            let linked = symbol.extends.clone();
            symbol
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            p.library.validate()?;
            p.validate()?;
            Ok(json!({"name": name, "extends": linked}))
        }
        "library.symbol.rotate" | "library.symbol.mirror" => {
            let a: GeometryParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid("symbol names must be 1 to 128 bytes".into()));
            }
            let name = a.name.clone();
            let symbol = p
                .library
                .symbols
                .iter_mut()
                .find(|s| s.name == name)
                .ok_or_else(missing)?;
            let counts = if command == "library.symbol.rotate" {
                if !a.degrees.is_some_and(f64::is_finite) || a.degrees.unwrap_or(0.0).abs() > 1e5 {
                    return Err(Error::Invalid(
                        "rotation must be finite and within 100000 degrees".into(),
                    ));
                }
                if a.axis.is_some() {
                    return Err(Error::Invalid("rotation needs degrees, not an axis".into()));
                }
                rotate_symbol(symbol, a.degrees.unwrap_or(0.0))?
            } else {
                if a.axis.as_deref() != Some("y") {
                    return Err(Error::Invalid("mirror needs axis \"y\"".into()));
                }
                if a.degrees.is_some() {
                    return Err(Error::Invalid("mirror needs an axis, not degrees".into()));
                }
                mirror_symbol(symbol)?
            };
            symbol
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            p.library.validate()?;
            p.validate()?;
            Ok(json!({"name": name, "pins": counts.0, "graphics": counts.1}))
        }
        "library.footprint.rotate" | "library.footprint.mirror" => {
            let a: GeometryParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "footprint names must be 1 to 128 bytes".into(),
                ));
            }
            let name = a.name.clone();
            let footprint = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == name)
                .ok_or_else(missing)?;
            let counts = if command == "library.footprint.rotate" {
                if !a.degrees.is_some_and(f64::is_finite) || a.degrees.unwrap_or(0.0).abs() > 1e5 {
                    return Err(Error::Invalid(
                        "rotation must be finite and within 100000 degrees".into(),
                    ));
                }
                if a.axis.is_some() {
                    return Err(Error::Invalid("rotation needs degrees, not an axis".into()));
                }
                rotate_footprint(footprint, a.degrees.unwrap_or(0.0))?
            } else {
                if a.axis.as_deref() != Some("y") {
                    return Err(Error::Invalid("mirror needs axis \"y\"".into()));
                }
                if a.degrees.is_some() {
                    return Err(Error::Invalid("mirror needs an axis, not degrees".into()));
                }
                mirror_footprint(footprint)?
            };
            footprint
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            p.library.validate()?;
            p.validate()?;
            Ok(json!({"name": name, "pads": counts.0, "graphics": counts.1}))
        }
        "library.footprint.recenter" | "library.symbol.recenter" => {
            let a: RecenterParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            if !a.delta.x.is_finite()
                || !a.delta.y.is_finite()
                || a.delta.x.abs() > 1000.0
                || a.delta.y.abs() > 1000.0
            {
                return Err(Error::Invalid(
                    "offset must be finite and within 1000 mm".into(),
                ));
            }
            let shift = |p: Point| -> Result<Point, Error> {
                let moved = Point::new(p.x + a.delta.x, p.y + a.delta.y);
                if moved.valid() {
                    Ok(moved)
                } else {
                    Err(Error::Invalid(
                        "the shifted shape left the board coordinate range".into(),
                    ))
                }
            };
            let name = a.name.clone();
            if command == "library.symbol.recenter" {
                let symbol = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|s| s.name == name)
                    .ok_or_else(missing)?;
                for pin in symbol.pins.iter_mut() {
                    pin.position = shift(pin.position)?;
                }
                let mut count = 0;
                for art in symbol.graphics.iter_mut() {
                    shift_graphic(&mut art.primitive, &shift)?;
                    count += 1;
                }
                let counts = (symbol.pins.len(), count);
                symbol
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                p.library.validate()?;
                p.validate()?;
                return Ok(json!({"name": name, "pins": counts.0, "graphics": counts.1}));
            }
            let footprint = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == name)
                .ok_or_else(missing)?;
            for pad in footprint.pads.iter_mut() {
                pad.position = shift(pad.position)?;
                if let Some(drill) = &mut pad.drill {
                    drill.offset = shift(drill.offset)?;
                }
            }
            let mut count = 0;
            for art in footprint.graphics.iter_mut() {
                shift_graphic(art, &shift)?;
                count += 1;
            }
            if let Some(model) = &mut footprint.model {
                model.offset = [
                    model.offset[0] - a.delta.x,
                    model.offset[1] - a.delta.y,
                    model.offset[2],
                ];
            }
            let counts = (footprint.pads.len(), count);
            footprint
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            p.library.validate()?;
            p.validate()?;
            Ok(json!({"name": name, "pads": counts.0, "graphics": counts.1}))
        }
        "library.footprint.scale" | "library.symbol.scale" => {
            let a: ScaleParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "library names must be 1 to 128 bytes".into(),
                ));
            }
            if !a.factor.is_finite() || a.factor <= 0.0 || a.factor > 1000.0 {
                return Err(Error::Invalid(
                    "factor must be finite and within 0 to 1000".into(),
                ));
            }
            let k = a.factor;
            let scale = |p: Point| -> Result<Point, Error> {
                let grown = Point::new(p.x * k, p.y * k);
                if grown.valid() {
                    Ok(grown)
                } else {
                    Err(Error::Invalid(
                        "the scaled shape left the board coordinate range".into(),
                    ))
                }
            };
            let scale_dim = |v: f64| -> Result<f64, Error> {
                let grown = v * k;
                if grown.is_finite() {
                    Ok(grown)
                } else {
                    Err(Error::Invalid("scaled size left range".into()))
                }
            };
            let name = a.name.clone();
            if command == "library.symbol.scale" {
                let symbol = p
                    .library
                    .symbols
                    .iter_mut()
                    .find(|s| s.name == name)
                    .ok_or_else(missing)?;
                for pin in symbol.pins.iter_mut() {
                    pin.position = scale(pin.position)?;
                    pin.length = scale_dim(pin.length)?;
                }
                let mut count = 0;
                for art in symbol.graphics.iter_mut() {
                    scale_graphic(&mut art.primitive, &scale, scale_dim)?;
                    count += 1;
                }
                let counts = (symbol.pins.len(), count);
                symbol
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                p.library.validate()?;
                p.validate()?;
                return Ok(json!({"name": name, "pins": counts.0, "graphics": counts.1}));
            }
            let footprint = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == name)
                .ok_or_else(missing)?;
            for pad in footprint.pads.iter_mut() {
                pad.position = scale(pad.position)?;
                pad.size = scale(pad.size)?;
                if let Some(drill) = &mut pad.drill {
                    drill.size = scale(drill.size)?;
                    drill.offset = scale(drill.offset)?;
                }
            }
            let mut count = 0;
            for art in footprint.graphics.iter_mut() {
                scale_graphic(art, &scale, scale_dim)?;
                count += 1;
            }
            if let Some(model) = &mut footprint.model {
                model.offset = [
                    model.offset[0] * k,
                    model.offset[1] * k,
                    model.offset[2] * k,
                ];
                for axis in model.scale.iter_mut() {
                    *axis = scale_dim(*axis)?;
                }
            }
            let counts = (footprint.pads.len(), count);
            footprint
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            p.library.validate()?;
            p.validate()?;
            Ok(json!({"name": name, "pads": counts.0, "graphics": counts.1}))
        }
        "library.import_part" => {
            let a: ImportPartParams = parse(params)?;
            let (symbol, footprint) = import_part(p, a.symbol, a.footprint, a.notice)?;
            Ok(json!({"symbol":symbol,"footprint":footprint}))
        }
        "part.place" => {
            let a: PlacePartParams = parse(params)?;
            if a.target != "schematic" && a.target != "board" {
                return Err(Error::Invalid("target must be schematic or board".into()));
            }
            let at = if a.target == "board" {
                parked_position(p)
            } else {
                a.position
            };
            let id = if let Some(kind) = a.kind.as_deref().filter(|_| a.symbol.is_none()) {
                let template = TEMPLATES
                    .iter()
                    .find(|t| t.kind == kind)
                    .ok_or_else(|| Error::Invalid("unknown symbol kind".into()))?;
                let reference = a.reference.clone().unwrap_or_else(|| {
                    let mut n = 1;
                    while p
                        .components
                        .iter()
                        .any(|c| c.reference == format!("{}{n}", template.prefix))
                    {
                        n += 1;
                    }
                    format!("{}{n}", template.prefix)
                });
                let id = allocate(next)?;
                let mut c = component_template(kind, id, reference, at).ok_or_else(missing)?;
                if let Some(value) = a.value.clone() {
                    c.value = value;
                }
                p.components.push(c);
                id
            } else {
                let name = match a.symbol {
                    Some(symbol) => import_part(p, symbol, a.footprint, a.notice)?.0,
                    None => a.name.ok_or_else(|| {
                        Error::Invalid(
                            "give a symbol definition, a built-in kind, or the name of a symbol in the project library"
                                .into(),
                        )
                    })?,
                };
                let def = p
                    .library
                    .symbols
                    .iter()
                    .find(|s| s.name == name)
                    .cloned()
                    .ok_or_else(missing)?;
                let footprint = p
                    .library
                    .footprints
                    .iter()
                    .find(|f| f.name == def.footprint)
                    .cloned();
                let placed =
                    place_definition(p, next, "project".into(), def, at, a.reference, a.value)?;
                let id = placed["id"].as_u64().ok_or_else(missing)?;
                if let Some(footprint) = footprint
                    && let Some(c) = p.components.iter_mut().find(|c| c.id == id)
                {
                    // A part whose pins and pads disagree stays without a library footprint.
                    let _ = assign_to(c, "project".into(), &footprint);
                }
                id
            };
            let mut out = json!({"id":id});
            if a.target == "board" {
                let pads = p
                    .components
                    .iter()
                    .find(|c| c.id == id)
                    .map(component_pads)
                    .ok_or_else(missing)?;
                let footprint_id = allocate(next)?;
                p.board.footprints.push(Footprint {
                    preserved: pcbcraft_doc::PreservedSource::default(),
                    id: footprint_id,
                    component: id,
                    position: a.position,
                    rotation: 0.0,
                    locked: None,
                    side: None,
                    pads,
                });
                out["footprint"] = json!(footprint_id);
            }
            Ok(out)
        }
        "component.place_custom" => {
            let a: Place = parse(params)?;
            let def = p
                .library
                .symbols
                .iter()
                .find(|s| s.name == a.name)
                .cloned()
                .ok_or_else(missing)?;
            place_definition(
                p,
                next,
                "project".into(),
                def,
                a.position,
                a.reference,
                a.value,
            )
            .and_then(|placed| turn_placed(&placed, p, a.rotation))
        }
        "footprint.assign_custom" => {
            let a: Assign = parse(params)?;
            let def = p
                .library
                .footprints
                .iter()
                .find(|s| s.name == a.name)
                .cloned()
                .ok_or_else(missing)?;
            assign_definition(p, a.id, "project".into(), def)
        }
        _ => Err(Error::Invalid("unknown library authoring command".into())),
    })())
}

/// Put a part's symbol, and its footprint when it has one, into the project
/// library; the symbol names the footprint it was imported with. Returns the
/// symbol name and the footprint name when there is one.
fn import_part(
    p: &mut Project,
    mut symbol: SymbolDefinition,
    footprint: Option<FootprintDefinition>,
    notice: Option<String>,
) -> Result<(String, Option<String>), Error> {
    symbol
        .validate()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let footprint_name = match footprint {
        Some(footprint) => {
            footprint
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let name = footprint.name.clone();
            symbol.footprint.clone_from(&name);
            import_footprint(p, footprint);
            Some(name)
        }
        None => None,
    };
    if let Some(notice) = notice
        && !notice.is_empty()
        && !p.library.notices.contains(&notice)
    {
        p.library.notices.push(notice);
    }
    let name = symbol.name.clone();
    import_symbol(p, symbol);
    p.library.validate()?;
    Ok((name, footprint_name))
}

/// Where a part added from the board waits on the schematic: left edge,
/// below everything already drawn, stepping down so parked parts do not stack.
fn parked_position(p: &Project) -> Point {
    let bottom = p
        .components
        .iter()
        .map(|c| c.position.y)
        .fold(0.0_f64, f64::max);
    Point::new(0.0, bottom + 20.0)
}

/// Upserts one footprint into `Project::library` by name and keeps the footprint list sorted.
///
/// # Dependencies
/// - crates/library/src/lib.rs
fn import_footprint(p: &mut Project, def: pcbcraft_library::FootprintDefinition) {
    if let Some(existing) = p.library.footprints.iter_mut().find(|f| f.name == def.name) {
        *existing = def;
    } else {
        p.library.footprints.push(def);
    }
    p.library.footprints.sort_by(|a, b| a.name.cmp(&b.name));
}

/// Upserts one symbol into `Project::library` by name and keeps the symbol list sorted.
///
/// # Dependencies
/// - crates/library/src/lib.rs
fn import_symbol(p: &mut Project, def: pcbcraft_library::SymbolDefinition) {
    if let Some(existing) = p.library.symbols.iter_mut().find(|s| s.name == def.name) {
        *existing = def;
    } else {
        p.library.symbols.push(def);
    }
    p.library.symbols.sort_by(|a, b| a.name.cmp(&b.name));
}

/// Returns the layer name on a library graphic after checking it is 1 to 64 bytes.
///
/// # Rules
/// - Empty or overlong layer strings return `Error::Invalid`.
///
/// # Dependencies
/// - crates/library/src/lib.rs
fn graphic_layer(graphic: &Graphic) -> Result<String, Error> {
    let layer = match graphic {
        Graphic::Line { layer, .. }
        | Graphic::Rectangle { layer, .. }
        | Graphic::Circle { layer, .. }
        | Graphic::Arc { layer, .. }
        | Graphic::Polyline { layer, .. }
        | Graphic::Text { layer, .. } => layer,
    };
    if layer.is_empty() || layer.len() > 64 {
        return Err(Error::Invalid("layer must be 1 to 64 bytes".into()));
    }
    Ok(layer.clone())
}

/// Applies a point transform to every vertex of a graphic primitive.
///
/// # Dependencies
/// - crates/library/src/lib.rs
/// - crates/geom/src/lib.rs
fn shift_graphic(
    g: &mut Graphic,
    shift: &dyn Fn(Point) -> Result<Point, Error>,
) -> Result<(), Error> {
    match g {
        Graphic::Line { start, end, .. } => {
            *start = shift(*start)?;
            *end = shift(*end)?;
        }
        Graphic::Rectangle { start, end, .. } => {
            *start = shift(*start)?;
            *end = shift(*end)?;
        }
        Graphic::Circle { center, .. } => *center = shift(*center)?,
        Graphic::Arc {
            start, mid, end, ..
        } => {
            *start = shift(*start)?;
            *mid = shift(*mid)?;
            *end = shift(*end)?;
        }
        Graphic::Polyline { points, .. } => {
            for p in points.iter_mut() {
                *p = shift(*p)?;
            }
        }
        Graphic::Text { position, .. } => *position = shift(*position)?,
    }
    Ok(())
}

/// Scales graphic vertices and stroke or text dimensions using caller-supplied closures.
///
/// # Dependencies
/// - crates/library/src/lib.rs
/// - crates/geom/src/lib.rs
fn scale_graphic(
    g: &mut Graphic,
    scale: &dyn Fn(Point) -> Result<Point, Error>,
    scale_dim: impl Fn(f64) -> Result<f64, Error>,
) -> Result<(), Error> {
    match g {
        Graphic::Line {
            start, end, width, ..
        } => {
            *start = scale(*start)?;
            *end = scale(*end)?;
            *width = scale_dim(*width)?;
        }
        Graphic::Rectangle {
            start, end, width, ..
        } => {
            *start = scale(*start)?;
            *end = scale(*end)?;
            *width = scale_dim(*width)?;
        }
        Graphic::Circle {
            center,
            radius,
            width,
            ..
        } => {
            *center = scale(*center)?;
            *radius = scale_dim(*radius)?;
            *width = scale_dim(*width)?;
        }
        Graphic::Arc {
            start,
            mid,
            end,
            width,
            ..
        } => {
            *start = scale(*start)?;
            *mid = scale(*mid)?;
            *end = scale(*end)?;
            *width = scale_dim(*width)?;
        }
        Graphic::Polyline { points, width, .. } => {
            for p in points.iter_mut() {
                *p = scale(*p)?;
            }
            *width = scale_dim(*width)?;
        }
        Graphic::Text { position, size, .. } => {
            *position = scale(*position)?;
            *size = scale(*size)?;
        }
    }
    Ok(())
}

/// Rotates a point about the origin by the given cosine and sine; refuses a result outside the board range.
///
/// # Rules
/// - Returns `Error::Invalid` when the turned point fails `Point::valid`.
///
/// # Dependencies
/// - crates/geom/src/lib.rs
fn rot(p: Point, cos: f64, sin: f64) -> Result<Point, Error> {
    let out = Point::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos);
    if out.valid() {
        Ok(out)
    } else {
        Err(Error::Invalid(
            "the turned shape left the board coordinate range".into(),
        ))
    }
}

/// Adds the arcsine of the clamped second argument, in degrees, to a facing angle.
///
/// # Rules
/// - Returns `Error::Invalid` when the sum is not finite.
fn turn(angle: f64, sin: f64) -> Result<f64, Error> {
    let turned = angle + sin.clamp(-1.0, 1.0).asin().to_degrees();
    if turned.is_finite() {
        Ok(turned)
    } else {
        Err(Error::Invalid("turned angle left range".into()))
    }
}

/// Mirrors a point across the vertical axis by negating x.
fn flip_x(p: Point) -> Point {
    Point::new(-p.x, p.y)
}

/// Mirrors a facing angle for vertical flip and normalizes into (-180, 180] degrees.
///
/// # Rules
/// - Returns `Error::Invalid` when the normalized angle is not finite.
fn flip_angle(angle: f64) -> Result<f64, Error> {
    let mut turned = 180.0 - angle;
    while turned > 180.0 {
        turned -= 360.0;
    }
    while turned <= -180.0 {
        turned += 360.0;
    }
    if turned.is_finite() {
        Ok(turned)
    } else {
        Err(Error::Invalid("flipped angle left range".into()))
    }
}

/// Rotates every vertex of a graphic about the origin; updates text angle via `turn`.
///
/// # Dependencies
/// - crates/library/src/lib.rs
fn rotate_graphic(g: &mut Graphic, cos: f64, sin: f64) -> Result<(), Error> {
    match g {
        Graphic::Line { start, end, .. } => {
            *start = rot(*start, cos, sin)?;
            *end = rot(*end, cos, sin)?;
        }
        Graphic::Rectangle { start, end, .. } => {
            *start = rot(*start, cos, sin)?;
            *end = rot(*end, cos, sin)?;
        }
        Graphic::Circle { center, .. } => *center = rot(*center, cos, sin)?,
        Graphic::Arc {
            start, mid, end, ..
        } => {
            *start = rot(*start, cos, sin)?;
            *mid = rot(*mid, cos, sin)?;
            *end = rot(*end, cos, sin)?;
        }
        Graphic::Polyline { points, .. } => {
            for p in points.iter_mut() {
                *p = rot(*p, cos, sin)?;
            }
        }
        Graphic::Text {
            position, angle, ..
        } => {
            *position = rot(*position, cos, sin)?;
            *angle = turn(*angle, sin)?;
        }
    }
    Ok(())
}

/// Mirrors a graphic about the vertical axis; reverses polyline order and updates text angle.
///
/// # Dependencies
/// - crates/library/src/lib.rs
fn mirror_graphic(g: &mut Graphic) -> Result<(), Error> {
    match g {
        Graphic::Line { start, end, .. } => {
            let flipped = flip_x(*end);
            *end = flip_x(*start);
            *start = flipped;
        }
        Graphic::Rectangle { start, end, .. } => {
            *start = flip_x(*start);
            *end = flip_x(*end);
        }
        Graphic::Circle { center, .. } => *center = flip_x(*center),
        Graphic::Arc {
            start, mid, end, ..
        } => {
            let flipped = flip_x(*end);
            *end = flip_x(*start);
            *start = flipped;
            *mid = flip_x(*mid);
        }
        Graphic::Polyline { points, .. } => {
            for p in points.iter_mut() {
                *p = flip_x(*p);
            }
            points.reverse();
        }
        Graphic::Text {
            position, angle, ..
        } => {
            *position = flip_x(*position);
            *angle = flip_angle(*angle)?;
        }
    }
    Ok(())
}

/// Rotates every pin and unit graphic on a symbol by degrees about the origin.
///
/// # Dependencies
/// - crates/library/src/lib.rs
fn rotate_symbol(symbol: &mut SymbolDefinition, degrees: f64) -> Result<(usize, usize), Error> {
    let rad = degrees.to_radians();
    let (cos, sin) = (rad.cos(), rad.sin());
    for pin in symbol.pins.iter_mut() {
        pin.position = rot(pin.position, cos, sin)?;
        pin.angle = turn(pin.angle, sin)?;
    }
    let mut count = 0;
    for art in symbol.graphics.iter_mut() {
        rotate_graphic(&mut art.primitive, cos, sin)?;
        count += 1;
    }
    Ok((symbol.pins.len(), count))
}

/// Mirrors every pin and unit graphic on a symbol about the vertical axis.
///
/// # Dependencies
/// - crates/library/src/lib.rs
fn mirror_symbol(symbol: &mut SymbolDefinition) -> Result<(usize, usize), Error> {
    for pin in symbol.pins.iter_mut() {
        pin.position = flip_x(pin.position);
        pin.angle = -pin.angle;
    }
    let mut count = 0;
    for art in symbol.graphics.iter_mut() {
        mirror_graphic(&mut art.primitive)?;
        count += 1;
    }
    Ok((symbol.pins.len(), count))
}

/// Rotates every pad, drill offset, and graphic on a footprint by degrees about the origin.
///
/// # Dependencies
/// - crates/library/src/lib.rs
fn rotate_footprint(
    footprint: &mut FootprintDefinition,
    degrees: f64,
) -> Result<(usize, usize), Error> {
    let rad = degrees.to_radians();
    let (cos, sin) = (rad.cos(), rad.sin());
    for pad in footprint.pads.iter_mut() {
        pad.position = rot(pad.position, cos, sin)?;
        pad.rotation = turn(pad.rotation, sin)?;
        if let Some(drill) = &mut pad.drill {
            drill.offset = rot(drill.offset, cos, sin)?;
        }
    }
    let mut count = 0;
    for art in footprint.graphics.iter_mut() {
        rotate_graphic(art, cos, sin)?;
        count += 1;
    }
    Ok((footprint.pads.len(), count))
}

/// Mirrors every pad, drill offset, and graphic on a footprint about the vertical axis.
///
/// # Dependencies
/// - crates/library/src/lib.rs
fn mirror_footprint(footprint: &mut FootprintDefinition) -> Result<(usize, usize), Error> {
    for pad in footprint.pads.iter_mut() {
        pad.position = flip_x(pad.position);
        pad.rotation = flip_angle(pad.rotation)?;
        if let Some(drill) = &mut pad.drill {
            drill.offset = flip_x(drill.offset);
        }
    }
    let mut count = 0;
    for art in footprint.graphics.iter_mut() {
        mirror_graphic(art)?;
        count += 1;
    }
    Ok((footprint.pads.len(), count))
}
