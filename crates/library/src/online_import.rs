//! Turns the files an online CAD source returns into validated definitions
//! plus the notices that record where they came from. The result is shaped
//! for the engine commands `library.import_online` and `library.place_online`
//! (crates/engine/src/online.rs); the engine itself never fetches anything.
//!
//! # Rules
//! - The symbol is picked by name from the fetched file; nothing is guessed.
//! - A footprint keeps its 3D model reference only when a model file was fetched:
//!   the path becomes the cached file and the transform stays. Otherwise it is dropped.
//! - Callers that import from the open library gate model fetch with
//!   [`crate::open_import::import_open`] and its `fetch_3d` flag (default off at the UI layer).
//! - Every definition is validated here, so a bad file fails before the engine sees it.
//! - A footprint found on its own is placed with a generic symbol made from its pads
//!   ([`generic_symbol_for`]): the engine places parts as symbol plus footprint, so this
//!   keeps one placement path and the pad and pin numbers match by construction.
//!
//! # Dependencies
//! - crates/library/src/cad.rs
//! - crates/library/src/reader.rs
//! - crates/engine/src/online.rs
use crate::cad::{CadKind, CadRef, CadSource};
use crate::sources::SourceError;
use crate::{
    FootprintDefinition, Graphic, Model3D, SymbolDefinition, SymbolPin, UnitGraphic,
    read_footprint, read_symbols,
};
use pcbcraft_geom::Point;
use serde_json::{Value, json};
use std::path::PathBuf;

/// One part ready for the engine.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedPart {
    /// The symbol, validated.
    pub symbol: SymbolDefinition,
    /// The footprint, validated, with its 3D model reference pointing at the cached file.
    pub footprint: Option<FootprintDefinition>,
    /// The cached 3D model file on this computer, when one was fetched and
    /// the source keeps files on disk.
    pub model_path: Option<PathBuf>,
    /// One notice per fetched file, in symbol, footprint, model order.
    pub notices: Vec<String>,
}

fn invalid(text: String) -> SourceError {
    SourceError::Invalid(text)
}

/// Fetch and read one part: a symbol, and optionally its footprint and 3D model.
///
/// # Rules
/// - Each reference must have the matching kind; [`CadSource::fetch_symbol`] and its siblings check it.
///
/// # Dependencies
/// - crates/library/src/cad.rs
///
/// # Do not
/// - Write to a project; hand the result to the engine commands instead.
pub fn import_part(
    source: &dyn CadSource,
    symbol: &CadRef,
    footprint: Option<&CadRef>,
    model: Option<&CadRef>,
) -> Result<ImportedPart, SourceError> {
    let file = source.fetch_symbol(symbol)?;
    let defs = read_symbols(file.text()?)
        .map_err(|e| invalid(format!("the symbol file could not be read: {e}")))?;
    let symbol_def = defs
        .into_iter()
        .find(|d| d.name == symbol.name)
        .ok_or_else(|| SourceError::NotFound(format!("{} {}", symbol.library, symbol.name)))?;
    symbol_def
        .validate()
        .map_err(|e| invalid(format!("the symbol is not usable: {e}")))?;
    let mut notices = vec![file.provenance.notice()];

    let mut footprint_def = None;
    if let Some(fref) = footprint {
        let ffile = source.fetch_footprint(fref)?;
        let def = read_footprint(ffile.text()?)
            .map_err(|e| invalid(format!("the footprint file could not be read: {e}")))?;
        def.validate()
            .map_err(|e| invalid(format!("the footprint is not usable: {e}")))?;
        notices.push(ffile.provenance.notice());
        footprint_def = Some(def);
    }

    let mut model_path = None;
    if let Some(mref) = model {
        if footprint_def.is_none() {
            return Err(invalid("a 3D model needs a footprint to attach to".into()));
        }
        let mfile = source.fetch_model(mref)?;
        notices.push(mfile.provenance.notice());
        model_path = source.cached_path(mref);
    }
    if let Some(def) = footprint_def.as_mut() {
        def.model = match (def.model.take(), &model_path) {
            (Some(m), Some(path)) => Some(Model3D {
                path: path.to_string_lossy().into_owned(),
                ..m
            }),
            (None, Some(path)) => Some(Model3D {
                path: path.to_string_lossy().into_owned(),
                offset: [0.0; 3],
                scale: [1.0; 3],
                rotate: [0.0; 3],
                cad_part: None,
                raw_source: String::new(),
            }),
            (_, None) => None,
        };
    }
    notices.dedup();
    Ok(ImportedPart {
        symbol: symbol_def,
        footprint: footprint_def,
        model_path,
        notices,
    })
}

/// Most pins a generated symbol gets; larger footprints are refused.
pub const MAX_GENERIC_PINS: usize = 1024;

/// A plain rectangular symbol with one passive pin per distinct pad number of
/// `footprint`, so the part can be placed on the schematic and the board.
///
/// # Rules
/// - Pins keep the pad order of the footprint; a number used by several pads (a thermal pad cut in pieces) gets one pin.
/// - Pads with no number are not wired parts of the package and get no pin.
/// - Pins sit on a 2.54 mm grid, half on each side; the symbol links to the footprint by name.
///
/// # Dependencies
/// - crates/library/src/lib.rs (the symbol and footprint types)
///
/// # Do not
/// - Name pins after signals; the footprint file does not know them.
pub fn generic_symbol_for(
    footprint: &FootprintDefinition,
) -> Result<SymbolDefinition, SourceError> {
    let mut numbers: Vec<&str> = Vec::new();
    for pad in &footprint.pads {
        let n = pad.number.trim();
        if !n.is_empty() && !numbers.contains(&n) {
            numbers.push(n);
        }
    }
    if numbers.is_empty() {
        return Err(invalid(
            "this footprint has no numbered pads to wire".into(),
        ));
    }
    if numbers.len() > MAX_GENERIC_PINS {
        return Err(invalid(
            "this footprint has too many pads for a generic symbol".into(),
        ));
    }
    const GRID: f64 = 2.54;
    const HALF_WIDTH: f64 = 6.35;
    let left = numbers.len().div_ceil(2);
    let right = numbers.len() - left;
    let rows = left.max(1);
    let pin = |number: &str, x: f64, y: f64, angle: f64| SymbolPin {
        number: number.to_owned(),
        name: number.to_owned(),
        position: Point::new(x, y),
        angle,
        length: GRID,
        electrical_type: "passive".into(),
        unit: 1,
        body_style: 1,
        graphical_style: "line".into(),
        visible: true,
    };
    let mut pins = Vec::with_capacity(numbers.len());
    for (i, n) in numbers.iter().take(left).enumerate() {
        pins.push(pin(n, -(HALF_WIDTH + GRID), i as f64 * GRID, 0.0));
    }
    for (i, n) in numbers.iter().skip(left).take(right).enumerate() {
        pins.push(pin(n, HALF_WIDTH + GRID, i as f64 * GRID, 180.0));
    }
    let graphics = vec![UnitGraphic {
        unit: 1,
        body_style: 1,
        primitive: Graphic::Rectangle {
            start: Point::new(-HALF_WIDTH, -GRID),
            end: Point::new(HALF_WIDTH, rows as f64 * GRID),
            width: 0.254,
            fill: "background".into(),
            layer: String::new(),
        },
    }];
    let symbol = SymbolDefinition {
        name: footprint.name.clone(),
        reference: "U".into(),
        value: footprint.name.clone(),
        description: footprint.description.clone(),
        keywords: footprint.keywords.clone(),
        footprint: footprint.name.clone(),
        set: String::new(),
        pins,
        graphics,
        extends: None,
        raw_source: String::new(),
        unsupported: Vec::new(),
        retained: Vec::new(),
    };
    symbol
        .validate()
        .map_err(|e| invalid(format!("the generated symbol is not usable: {e}")))?;
    Ok(symbol)
}

/// Fetch one footprint and shape it for the engine with a generic symbol.
///
/// # Rules
/// - Same validation, notices and model handling as [`import_part`].
///
/// # Dependencies
/// - crates/library/src/cad.rs
///
/// # Do not
/// - Write to a project; hand the result to the engine commands instead.
pub fn import_footprint_only(
    source: &dyn CadSource,
    footprint: &CadRef,
    model: Option<&CadRef>,
) -> Result<ImportedPart, SourceError> {
    let ffile = source.fetch_footprint(footprint)?;
    let mut def = read_footprint(ffile.text()?)
        .map_err(|e| invalid(format!("the footprint file could not be read: {e}")))?;
    def.validate()
        .map_err(|e| invalid(format!("the footprint is not usable: {e}")))?;
    let symbol = generic_symbol_for(&def)?;
    let mut notices = vec![ffile.provenance.notice()];
    let mut model_path = None;
    if let Some(mref) = model {
        let mfile = source.fetch_model(mref)?;
        notices.push(mfile.provenance.notice());
        model_path = source.cached_path(mref);
    }
    def.model = match (def.model.take(), &model_path) {
        (Some(m), Some(path)) => Some(Model3D {
            path: path.to_string_lossy().into_owned(),
            ..m
        }),
        _ => None,
    };
    notices.dedup();
    Ok(ImportedPart {
        symbol,
        footprint: Some(def),
        model_path,
        notices,
    })
}

/// The reference a symbol's own linked-footprint field names, such as
/// `Resistor_SMD:R_0603_1608Metric`. `None` when the field is empty or not in
/// `Library:Name` form.
pub fn footprint_ref_from_field(field: &str) -> Option<CadRef> {
    let (library, name) = field.trim().split_once(':')?;
    CadRef::new(CadKind::Footprint, library, name).ok()
}

impl ImportedPart {
    /// Parameters for the engine command `library.import_online`; add
    /// `target`, `position` and the rest to place it with `library.place_online`.
    ///
    /// # Dependencies
    /// - crates/engine/src/online.rs
    pub fn to_params(&self) -> Value {
        let mut v = json!({
            "symbol": self.symbol,
            "notices": self.notices,
        });
        if let Some(map) = v.as_object_mut()
            && let Some(fp) = &self.footprint
        {
            let mut fp_value = serde_json::to_value(fp).unwrap_or(Value::Null);
            if let Some(model) = fp.model.as_ref() {
                map.insert(
                    "model".into(),
                    json!({
                        "path": model.path,
                        "offset": model.offset,
                        "scale": model.scale,
                        "rotate": model.rotate,
                    }),
                );
                if let Some(obj) = fp_value.as_object_mut() {
                    obj.remove("model");
                }
            }
            map.insert("footprint".into(), fp_value);
        }
        v
    }
}
