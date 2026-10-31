//! Turns an online card into the parameters of `library.place_online`: the
//! symbol, the footprint its own link names, the 3D model when one is kept on
//! this computer, and the source and licence notices.
//!
//! # Rules
//! - Runs on a worker; a frame never waits for it.
//! - A footprint or model that cannot be had does not stop the part: the
//!   symbol alone is still placed, and the notice list says what was read.
//! - Only parts of the open library carry the `Library/Name` sku this reads; a footprint
//!   found on its own has `footprint:Library/Name` and is placed with a generic symbol
//!   made from its pads (crates/library/src/online_import.rs).
//!
//! # Dependencies
//! - crates/library/src/online_import.rs
//! - crates/library/src/open_library.rs
//! - crates/engine/src/online.rs (the command that takes the result)

use pcbcraft_library::cad::{CadKind, CadRef, CadSource};
use pcbcraft_library::online_import::{
    footprint_ref_from_field, generic_symbol_for, import_footprint_only, import_part,
};
use pcbcraft_library::open_library::FOOTPRINT_SKU_PREFIX;
use pcbcraft_library::open_library::OpenLibrarySource;
use pcbcraft_library::read_footprint;
use pcbcraft_library::read_symbols;
use serde_json::Value;
use serde_json::json;

/// Fetch one open-library part named by its `Library/Name` sku and shape it
/// for the engine.
///
/// # Rules
/// - When `fetch_3d` is true, tries symbol + footprint + model, then without the
///   model, then the symbol alone; when false, the model is never fetched.
///
/// # Do not
/// - Write to the project; the caller runs `library.place_online` with the result.
pub(crate) fn import_open(
    source: &OpenLibrarySource,
    sku: &str,
    fetch_3d: bool,
) -> Result<Value, String> {
    if let Some(rest) = sku.strip_prefix(FOOTPRINT_SKU_PREFIX) {
        return import_footprint(source, rest, fetch_3d);
    }
    let (library, name) = sku
        .split_once('/')
        .ok_or_else(|| "this part has no library path to open".to_owned())?;
    let symbol = CadRef::new(CadKind::Symbol, library, name).map_err(|e| e.to_string())?;
    let file = source.fetch_symbol(&symbol).map_err(|e| e.to_string())?;
    let text = file.text().map_err(|e| e.to_string())?;
    let footprint = read_symbols(text)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|d| d.name == name)
        .and_then(|d| footprint_ref_from_field(&d.footprint));
    let model = footprint
        .as_ref()
        .and_then(|f| CadRef::new(CadKind::Model, &f.library, &f.name).ok());
    let with_model = (footprint.as_ref(), model.as_ref());
    let symbol_footprint = (footprint.as_ref(), None);
    let symbol_only = (None, None);
    let attempts: &[(_, _)] = if fetch_3d {
        &[with_model, symbol_footprint, symbol_only]
    } else {
        &[symbol_footprint, symbol_only]
    };
    let mut last = String::new();
    for (fp, m) in attempts {
        match import_part(source, &symbol, fp, m) {
            Ok(part) => return Ok(part.to_params()),
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

/// Read just the symbol of an open-library part, and the footprint it names
/// when one can be had, for the card thumbnail and description.
///
/// # Rules
/// - Goes through the source's cache; a missing footprint is not an error.
///
/// # Do not
/// - Fetch a 3D model; that happens only when the part is placed.
pub(crate) fn peek_open(source: &OpenLibrarySource, sku: &str) -> Result<Value, String> {
    if let Some(rest) = sku.strip_prefix(FOOTPRINT_SKU_PREFIX) {
        let footprint = footprint_cad_ref(rest)?;
        let file = source
            .fetch_footprint(&footprint)
            .map_err(|e| e.to_string())?;
        let def =
            read_footprint(file.text().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let symbol = generic_symbol_for(&def).map_err(|e| e.to_string())?;
        return Ok(json!({"symbol": symbol, "footprint": def}));
    }
    let (library, name) = sku
        .split_once('/')
        .ok_or_else(|| "this part has no library path to open".to_owned())?;
    let symbol = CadRef::new(CadKind::Symbol, library, name).map_err(|e| e.to_string())?;
    let file = source.fetch_symbol(&symbol).map_err(|e| e.to_string())?;
    let text = file.text().map_err(|e| e.to_string())?;
    let def = read_symbols(text)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|d| d.name == name)
        .ok_or_else(|| "the symbol file does not hold this part".to_owned())?;
    let footprint = footprint_ref_from_field(&def.footprint).and_then(|f| {
        let file = source.fetch_footprint(&f).ok()?;
        read_footprint(file.text().ok()?).ok()
    });
    Ok(json!({"symbol": def, "footprint": footprint}))
}

fn footprint_cad_ref(rest: &str) -> Result<CadRef, String> {
    let (library, name) = rest
        .split_once('/')
        .ok_or_else(|| "this footprint has no library path to open".to_owned())?;
    CadRef::new(CadKind::Footprint, library, name).map_err(|e| e.to_string())
}

/// A footprint found on its own, with a generic symbol and its 3D model when one can be had.
///
/// # Rules
/// - Tries with the model, then without it; the footprint file itself must be readable.
///
/// # Do not
/// - Write to the project; the caller runs `library.place_online` with the result.
fn import_footprint(
    source: &OpenLibrarySource,
    rest: &str,
    fetch_3d: bool,
) -> Result<Value, String> {
    let footprint = footprint_cad_ref(rest)?;
    let model = CadRef::new(CadKind::Model, &footprint.library, &footprint.name).ok();
    let mut last = String::new();
    let tries: &[Option<&CadRef>] = if fetch_3d {
        &[model.as_ref(), None]
    } else {
        &[None]
    };
    for m in tries {
        match import_footprint_only(source, &footprint, *m) {
            Ok(part) => return Ok(part.to_params()),
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}
