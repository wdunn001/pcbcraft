//! Turns an open-library card sku into the parameters of `library.place_online`:
//! the symbol, the footprint its own link names, an optional cached 3D model,
//! and the source and licence notices.
//!
//! # Rules
//! - When `fetch_3d` is false, symbol and footprint files are still fetched and
//!   noticed; the 3D model file is skipped and any footprint model reference is dropped.
//! - When `fetch_3d` is true, a model fetch is tried after symbol and footprint;
//!   a missing model does not stop the part (same fallbacks as before, without the model).
//! - A footprint sku (`footprint:Library/Name`) uses a generic symbol from its pads.
//!
//! # Dependencies
//! - crates/library/src/online_import.rs
//! - crates/library/src/open_library.rs
//! - crates/engine/src/online.rs (the commands that take the result)
//!
//! # Do not
//! - Write to a project; the caller runs `library.place_online` with the result.
use crate::cad::{CadKind, CadRef, CadSource};
use crate::online_import::{footprint_ref_from_field, import_footprint_only, import_part};
use crate::open_library::{OpenLibrarySource, FOOTPRINT_SKU_PREFIX};
use crate::read_symbols;
use crate::sources::SourceError;
use serde_json::Value;

fn invalid(text: &str) -> SourceError {
    SourceError::Invalid(text.to_owned())
}

/// Fetch one open-library part named by its `Library/Name` sku and shape it
/// for the engine.
///
/// # Rules
/// - When `fetch_3d` is true, tries symbol + footprint + model, then without the
///   model, then the symbol alone; when false, the model is never fetched.
///
/// # Dependencies
/// - [`import_part`] and [`import_footprint_only`]
///
/// # Do not
/// - Write to the project; the caller runs `library.place_online` with the result.
pub fn import_open(
    source: &OpenLibrarySource,
    sku: &str,
    fetch_3d: bool,
) -> Result<Value, SourceError> {
    if let Some(rest) = sku.strip_prefix(FOOTPRINT_SKU_PREFIX) {
        return import_footprint_sku(source, rest, fetch_3d);
    }
    let (library, name) = sku
        .split_once('/')
        .ok_or_else(|| invalid("this part has no library path to open"))?;
    let symbol = CadRef::new(CadKind::Symbol, library, name)?;
    let file = source.fetch_symbol(&symbol)?;
    let text = file.text().map_err(|e| invalid(&e.to_string()))?;
    let footprint = read_symbols(text)
        .map_err(|e| invalid(&format!("the symbol file could not be read: {e}")))?
        .into_iter()
        .find(|d| d.name == name)
        .and_then(|d| footprint_ref_from_field(&d.footprint));
    let model = footprint
        .as_ref()
        .and_then(|f| CadRef::new(CadKind::Model, &f.library, &f.name).ok());
    let candidates = [
        (footprint.as_ref(), model.as_ref()),
        (footprint.as_ref(), None),
        (None, None),
    ];
    let start = usize::from(!fetch_3d);
    let mut last = invalid("the part could not be imported");
    for (fp, m) in &candidates[start..] {
        match import_part(source, &symbol, fp.copied(), m.copied()) {
            Ok(part) => return Ok(part.to_params()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn footprint_cad_ref(rest: &str) -> Result<CadRef, SourceError> {
    let (library, name) = rest
        .split_once('/')
        .ok_or_else(|| invalid("this footprint has no library path to open"))?;
    CadRef::new(CadKind::Footprint, library, name)
}

/// A footprint found on its own, with a generic symbol and its 3D model when
/// `fetch_3d` is true and one can be had.
fn import_footprint_sku(
    source: &OpenLibrarySource,
    rest: &str,
    fetch_3d: bool,
) -> Result<Value, SourceError> {
    let footprint = footprint_cad_ref(rest)?;
    let model = CadRef::new(CadKind::Model, &footprint.library, &footprint.name).ok();
    let candidates = [model.as_ref(), None];
    let start = usize::from(!fetch_3d);
    let mut last = invalid("the footprint could not be imported");
    for m in &candidates[start..] {
        match import_footprint_only(source, &footprint, *m) {
            Ok(part) => return Ok(part.to_params()),
            Err(e) => last = e,
        }
    }
    Err(last)
}
