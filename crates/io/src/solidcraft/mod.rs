//! SolidCraft board export and interchange test helpers.
//!
//! Context: SolidCraft is the owner's mechanical CAD app. PCBCraft keeps a small reader and
//! writer for the subset needed to move board and component 3D between the two tools. Model
//! space is millimetres, Z up, right-handed, the same frame as [`crate::BoardMesh`].
//!
//! # What survives exchange
//! - Reference designators, values, footprint names and model paths (under [`SOLIDCRAFT_METADATA_KEY`])
//! - Triangle meshes, per-body colour and material names
//! - Component hierarchy, occurrence names and rigid placement transforms
//! - Unknown top-level and metadata keys (kept in the JSON so a re-export does not drop them)
//!
//! # What does not survive
//! - Nets, tracks, pad numbers and copper connectivity (copper may leave as mesh only)
//! - Schematic text, parametric feature trees and SolidCraft-native CAD history
//! - SolidCraft UI state outside the documented JSON fields
//!
//! # Dependencies
//! - [`super::board_mesh`] for board export meshes
//! - [`super::solidcraft_format`] for JSON and container I/O
//!
//! # Do not
//! - Do not link SolidCraft crates from this crate; fixtures and optional SolidCraft-side scripts
//!   check compatibility instead.
mod board;
mod exchange;

pub use board::{
    SOLIDCRAFT_FORMAT, SOLIDCRAFT_METADATA_KEY, SOLIDCRAFT_SCHEMA, SolidcraftLayers,
    solidcraft_board,
};
pub use exchange::{
    Bounds, PartSnapshot, bounds_close, design_as_value, designs_equal, part_snapshots,
    solidcraft_counts, transforms_close,
};
