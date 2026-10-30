//! Reads STEP solids into triangles with the pure Rust truck crates. STEP is
//! in millimetres already, so nothing is rescaled. Faces are tessellated at
//! [`STEP_TOLERANCE_MM`]. Used only by `model_load.rs`, which owns limits.
//! A file the reader cannot use gives a plain reason, never a panic.

use crate::model_load::{LoadedModel, MAX_MODEL_FACES, MAX_MODEL_VERTICES, ModelError};
use truck_meshalgo::prelude::*;
use truck_stepio::r#in::Table;

/// Tessellation tolerance in millimetres.
pub const STEP_TOLERANCE_MM: f64 = 0.05;

fn fail(why: &str) -> ModelError {
    ModelError(why.to_string())
}

fn convert(text: &str) -> Result<LoadedModel, ModelError> {
    let table = Table::from_step(text)
        .ok_or_else(|| fail("The STEP file could not be read; it may be damaged or not STEP."))?;
    let mut out = LoadedModel {
        positions: vec![],
        indices: vec![],
        color: None,
        triangle_materials: vec![],
        authored_colors: vec![],
    };
    for holder in table.shell.values() {
        let Ok(shell) = table.to_compressed_shell(holder) else {
            continue;
        };
        let poly = shell.triangulation(STEP_TOLERANCE_MM).to_polygon();
        let base = out.positions.len() as u32;
        out.positions
            .extend(poly.positions().iter().map(|p| [p.x, p.y, p.z]));
        for tri in poly.faces().triangle_iter() {
            out.indices.extend(tri.iter().map(|v| base + v.pos as u32));
        }
        if out.positions.len() > MAX_MODEL_VERTICES {
            return Err(fail(
                "The STEP model has too many vertices, over the limit.",
            ));
        }
        if out.indices.len() / 3 > MAX_MODEL_FACES {
            return Err(fail("The STEP model has too many faces, over the limit."));
        }
    }
    if out.indices.is_empty() {
        return Err(fail("The STEP file holds no solid the reader can use."));
    }
    out.finish_materials()?;
    Ok(out)
}

/// Read STEP text into triangles in millimetres.
///
/// # Rules
/// Shells the reader cannot build are skipped; no usable shell is an error.
/// A panic inside the third-party reader becomes a plain error.
/// # Dependencies
/// `./model_load.rs` for [`LoadedModel`] and the limits.
/// # Do not
/// Do not rescale; STEP is millimetres.
pub fn read_step(bytes: &[u8]) -> Result<LoadedModel, ModelError> {
    let text = String::from_utf8_lossy(bytes).into_owned();
    std::panic::catch_unwind(|| convert(&text))
        .unwrap_or_else(|_| Err(fail("The STEP file could not be turned into a solid.")))
}
