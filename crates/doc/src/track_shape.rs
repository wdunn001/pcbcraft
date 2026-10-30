//! Curved-track policy shared by rounding, export, and net classes (issue #911).
//!
//! Owner defaults recorded on the epic:
//! - Corner radius defaults to three times track width unless a net class sets one.
//! - Fabrication export writes true arcs by default; callers may flatten to chords.
//! - Teardrops default on for Class 3 rule profiles only (see profile application).

use pcbcraft_geom::track_corner::{
    DEFAULT_CORNER_RADIUS_WIDTH_FACTOR, TrackCornerFillet, effective_corner_radius,
    track_corner_fillet,
};
use serde::{Deserialize, Serialize};

pub use pcbcraft_geom::track_corner::{arc_chord_count, flatten_arc};
pub use DEFAULT_CORNER_RADIUS_WIDTH_FACTOR;

/// Whether teardrop copper is applied by default when a Class 3 profile is active.
pub const TEARDROPS_DEFAULT_CLASS3: bool = true;

/// How curved tracks are written to fabrication output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum TrackArcExport {
    /// Keep circular arcs in the output stream.
    #[default]
    TrueArc,
    /// Replace each arc with short chords within a tolerance.
    FlattenChords {
        max_chord_error_mm: f64,
    },
}

impl TrackArcExport {
    /// Default export policy: true arcs.
    pub const DEFAULT: Self = Self::TrueArc;

    pub fn valid(self) -> bool {
        match self {
            Self::TrueArc => true,
            Self::FlattenChords {
                max_chord_error_mm,
            } => max_chord_error_mm.is_finite()
                && max_chord_error_mm > 0.0
                && max_chord_error_mm <= 1.0,
        }
    }
}

/// Resolve a corner radius for a track on a net class.
pub fn net_class_corner_radius(
    requested_mm: f64,
    track_width_mm: f64,
    class: Option<f64>,
) -> Option<f64> {
    effective_corner_radius(requested_mm, track_width_mm, class)
}

pub use pcbcraft_geom::track_corner::TrackCornerFillet;
pub use track_corner_fillet;

#[cfg(test)]
mod tests {
    use super::TrackArcExport;

    #[test]
    fn track_arc_export_validates_chord_tolerance() {
        assert!(TrackArcExport::TrueArc.valid());
        assert!(TrackArcExport::FlattenChords {
            max_chord_error_mm: 0.01
        }
        .valid());
        assert!(!TrackArcExport::FlattenChords {
            max_chord_error_mm: 0.0
        }
        .valid());
    }
}
