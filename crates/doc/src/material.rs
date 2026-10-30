//! Built-in table of common laminates for the physical layer record.
//!
//! Values are typical figures from each maker's published data, kept as
//! starting points. A fab's own datasheet value belongs on the layer row,
//! where it overrides the table.
//!
//! # Rules
//! - Every entry carries the frequency its figures were stated at and a source note.
//!
//! # Dependencies
//! - `crates/doc/src/lib.rs` uses it from `LayerRecord::dk` and `LayerRecord::df`.
//!
//! # Do not
//! - Add an entry without a source note.

/// One laminate in the built-in table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Name a layer row refers to.
    pub name: &'static str,
    /// Relative permittivity at `frequency_mhz`.
    pub dk: f64,
    /// Loss tangent at `frequency_mhz`.
    pub df: f64,
    /// Frequency the figures were stated at, in megahertz.
    pub frequency_mhz: f64,
    /// Where the figures come from.
    pub source: &'static str,
}

/// The built-in laminates.
pub const MATERIALS: [Material; 5] = [
    Material {
        name: "FR-4 standard",
        dk: 4.5,
        df: 0.020,
        frequency_mhz: 1000.0,
        source: "Typical epoxy glass laminate figures from common maker data sheets (IPC-4101 /21 class). Confirm with your fab's stackup.",
    },
    Material {
        name: "FR-4 mid Tg",
        dk: 4.4,
        df: 0.019,
        frequency_mhz: 1000.0,
        source: "Typical mid-Tg epoxy glass laminate figures from common maker data sheets (IPC-4101 /24 class). Confirm with your fab's stackup.",
    },
    Material {
        name: "FR-4 high Tg",
        dk: 4.2,
        df: 0.018,
        frequency_mhz: 1000.0,
        source: "Typical high-Tg epoxy glass laminate figures from common maker data sheets (IPC-4101 /126 class). Confirm with your fab's stackup.",
    },
    Material {
        name: "Polyimide flex",
        dk: 3.4,
        df: 0.002,
        frequency_mhz: 10000.0,
        source: "Typical adhesiveless polyimide flex laminate figures from the maker's published data sheet. Confirm against the current data sheet.",
    },
    Material {
        name: "High-speed laminate",
        dk: 3.48,
        df: 0.0037,
        frequency_mhz: 10000.0,
        source: "Hydrocarbon ceramic laminate, RO4350B class, figures from the maker's published data sheet (10 GHz process value).",
    },
];

/// Look up a built-in material by name, ignoring case.
pub fn material(name: &str) -> Option<&'static Material> {
    MATERIALS
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(name.trim()))
}
