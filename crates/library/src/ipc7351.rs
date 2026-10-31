use crate::{generate_footprint, LandPatternDensity, LandPatternParams};
use crate::{FootprintDefinition, PackageFamily, PackageKey};

/// Build a land pattern footprint from a package key and IPC density level.
pub fn generate_footprint_from_key(
    key: &PackageKey,
    density: LandPatternDensity,
    name: &str,
) -> Result<FootprintDefinition, String> {
    let family = match key.family {
        PackageFamily::Chip => "chip_2terminal",
        PackageFamily::Sot => "sot",
        PackageFamily::GullWing => "soic",
        PackageFamily::Qfn => "qfn",
        PackageFamily::Bga => "bga",
        PackageFamily::Dip => "dip",
        PackageFamily::PinHeader => "header_single",
        PackageFamily::Radial => "radial",
        PackageFamily::Axial => "axial",
        PackageFamily::Cylinder | PackageFamily::Unknown => {
            return Err("package family has no IPC land pattern generator".into());
        }
    };
    let mut params = LandPatternParams {
        name: name.to_owned(),
        description: String::new(),
        keywords: String::new(),
        pitch: key.pitch_mm,
        pin_count: key.pin_count,
        body_length: key.body_length_mm,
        body_width: key.body_width_mm,
        row_spacing: key.lead_span_mm,
        lead_width: 0.0,
        lead_diameter: 0.0,
        grid_rows: 0,
        grid_cols: 0,
        thermal_size: key.exposed_pad_mm,
    };
    if key.family == PackageFamily::Bga {
        let side = (key.pin_count as f64).sqrt().round() as u32;
        params.grid_rows = side;
        params.grid_cols = side;
        params.pin_count = side * side;
    }
    generate_footprint(family, density, &params)
}
