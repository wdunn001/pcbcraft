//! A small set of 3D models shipped beside the app for common footprints.
//! Each entry records licence text and provenance; parametric bodies stay the
//! default when no starter model matches.

/// One redistributable model file embedded in the library crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StarterModel {
    /// Stable id used by engine commands and cache file names.
    pub id: &'static str,
    /// File name written under the user cache directory.
    pub file_name: &'static str,
    /// Model bytes (STEP today).
    pub bytes: &'static [u8],
    /// Footprint names that should prefer this starter model.
    pub footprint_names: &'static [&'static str],
    /// Basenames of model paths (from imported footprints) that map here.
    pub model_basenames: &'static [&'static str],
    /// Short licence title for listings.
    pub license_title: &'static str,
    /// One-line provenance and licence notice stored on the project library.
    pub notice: &'static str,
    /// Where the file came from, for attribution tables.
    pub source: &'static str,
}

const R_0603: StarterModel = StarterModel {
    id: "chip_resistor_0603",
    file_name: "R_0603_1608Metric.step",
    bytes: include_bytes!("../assets/starter_models/R_0603_1608Metric.step"),
    footprint_names: &["R_0603_1608Metric", "Chip_0603"],
    model_basenames: &["R_0603_1608Metric.step", "R_0603_1608Metric.wrl"],
    license_title: "CC-BY-SA 4.0 with design-output exception",
    notice: "Starter 3D model chip_resistor_0603 (R_0603_1608Metric.step): CC-BY-SA 4.0 with design-output exception; bundled under crates/library/assets/starter_models/LICENSE.md",
    source: "Open library models collection, revision 89757151",
};

/// Every starter model the app ships today.
pub fn all() -> &'static [StarterModel] {
    &[R_0603]
}

/// Lookup by stable id.
pub fn by_id(id: &str) -> Option<&'static StarterModel> {
    all().iter().copied().find(|m| m.id == id)
}

fn name_matches(list: &[&str], name: &str) -> bool {
    list.iter().any(|n| n.eq_ignore_ascii_case(name))
}

/// Pick a starter model for a project-library footprint name.
pub fn for_footprint(name: &str) -> Option<&'static StarterModel> {
    all()
        .iter()
        .copied()
        .find(|m| name_matches(m.footprint_names, name))
}

/// Pick a starter model from a footprint model reference path.
pub fn for_model_path(path: &str) -> Option<&'static StarterModel> {
    let base = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();
    all().iter().copied().find(|m| {
        m.model_basenames
            .iter()
            .any(|b| base == b.to_ascii_lowercase())
    })
}
