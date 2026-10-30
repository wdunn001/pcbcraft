//! Assembly house profiles for centroid files and Gerber component layers.
//!
//! A profile chooses centroid column order and labels, length units, optional
//! header lines, and how bottom-side rotation is exported. With no profile the
//! legacy centroid layout is unchanged.

use pcbcraft_doc::{Component, Footprint, Layer, Project};
use std::collections::BTreeMap;

const MAX_PLACED: usize = 5_000;
const MAX_HEADER_LINES: usize = 8;
const MAX_HEADER_LEN: usize = 256;
const MAX_COLUMNS: usize = 16;
const MAX_COLUMN_HEADER_LEN: usize = 64;

/// Length unit for centroid coordinates in the placement file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementUnits {
    Millimetres,
    Inches,
}

/// How bottom-side rotation is written into centroid and component layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BottomRotationPolicy {
    /// Board rotation normalized to 0..360, unchanged for bottom parts.
    AsAuthored,
    /// Negate rotation on the bottom side before normalization.
    NegateOnBottom,
    /// Add 180 degrees on the bottom side before normalization.
    Offset180OnBottom,
}

/// One column in a centroid file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacementColumn {
    pub field: PlacementField,
    pub header: String,
}

/// Fields that can appear in a centroid row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlacementField {
    MountType,
    Reference,
    X,
    Y,
    Rotation,
    Side,
    Package,
}

/// Layout and rotation choices for assembly exports.
#[derive(Clone, Debug, PartialEq)]
pub struct AssemblyPlacementProfile {
    /// Lines printed after the format line and before the column header.
    pub header_lines: Vec<String>,
    pub columns: Vec<PlacementColumn>,
    pub units: PlacementUnits,
    pub bottom_rotation: BottomRotationPolicy,
    /// Added to every part rotation before the bottom policy runs.
    pub reference_angle_deg: f64,
}

impl Default for AssemblyPlacementProfile {
    fn default() -> Self {
        Self::legacy_default()
    }
}

impl AssemblyPlacementProfile {
    /// The legacy centroid layout: classic header, column names and millimetres.
    pub fn legacy_default() -> Self {
        let mut profile = Self::default();
        profile.columns = vec![
            PlacementColumn {
                field: PlacementField::MountType,
                header: "SMT,THT".into(),
            },
            PlacementColumn {
                field: PlacementField::Reference,
                header: String::new(),
            },
            PlacementColumn {
                field: PlacementField::X,
                header: "X".into(),
            },
            PlacementColumn {
                field: PlacementField::Y,
                header: "Y".into(),
            },
            PlacementColumn {
                field: PlacementField::Rotation,
                header: "Rotation".into(),
            },
            PlacementColumn {
                field: PlacementField::Side,
                header: "Side".into(),
            },
            PlacementColumn {
                field: PlacementField::Package,
                header: "Package".into(),
            },
        ];
        profile
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.header_lines.len() > MAX_HEADER_LINES {
            return Err("a placement profile lists at most 8 header lines".into());
        }
        for line in &self.header_lines {
            if line.is_empty() || line.len() > MAX_HEADER_LEN {
                return Err("header lines must be 1 to 256 bytes".into());
            }
            if line.contains('\n') || line.contains('\r') {
                return Err("header lines must not contain line breaks".into());
            }
        }
        if self.columns.is_empty() || self.columns.len() > MAX_COLUMNS {
            return Err("a placement profile needs 1 to 16 columns".into());
        }
        for col in &self.columns {
            if col.header.len() > MAX_COLUMN_HEADER_LEN {
                return Err("column headers must be at most 64 bytes".into());
            }
        }
        if !self.reference_angle_deg.is_finite() {
            return Err("reference angle must be a finite number".into());
        }
        if self.reference_angle_deg.abs() > 720.0 {
            return Err("reference angle must stay within two full turns".into());
        }
        Ok(())
    }
}

/// One row of a centroid file before formatting.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementRow {
    pub mount: String,
    pub reference: String,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub side: String,
    pub package: String,
    pub bottom: bool,
}

/// Rotation exported for a part, shared by centroid and component layers.
pub fn export_rotation(
    board_deg: f64,
    bottom: bool,
    profile: &AssemblyPlacementProfile,
) -> f64 {
    let mut angle = board_deg + profile.reference_angle_deg;
    if bottom {
        angle = match profile.bottom_rotation {
            BottomRotationPolicy::AsAuthored => angle,
            BottomRotationPolicy::NegateOnBottom => -angle,
            BottomRotationPolicy::Offset180OnBottom => angle + 180.0,
        };
    }
    angle.rem_euclid(360.0)
}

/// Collect centroid rows for every placed footprint and assembly fiducial.
pub fn placement_rows(p: &Project) -> Result<Vec<PlacementRow>, String> {
    p.validate()?;
    if p.board.footprints.len() > MAX_PLACED {
        return Err("placement file exceeds the placed-object limit".into());
    }
    let refs: BTreeMap<u64, &Component> = p.components.iter().map(|c| (c.id, c)).collect();
    let mut rows = Vec::new();
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        let bottom = f.board_side() == Layer::Back;
        rows.push(PlacementRow {
            mount: mount_type_label(f),
            reference: c.reference.clone(),
            x: f.position.x,
            y: f.position.y,
            rotation: f.rotation,
            side: if bottom { "B" } else { "T" }.into(),
            package: package_name(c),
            bottom,
        });
    }
    for (i, fid) in p.board.fiducials.iter().enumerate() {
        let bottom = fid.layer == Layer::Back;
        rows.push(PlacementRow {
            mount: "SMT".into(),
            reference: format!("FID{:02}", i + 1),
            x: fid.position.x,
            y: fid.position.y,
            rotation: 0.0,
            side: if bottom { "B" } else { "T" }.into(),
            package: "FIDUCIAL".into(),
            bottom,
        });
    }
    rows.sort_by(|a, b| a.reference.cmp(&b.reference));
    Ok(rows)
}

fn mount_type_label(f: &Footprint) -> String {
    if f.pads.iter().any(|pad| {
        pad.geometry
            .as_ref()
            .is_some_and(|g| g.pad_type.contains("press"))
    }) {
        return "PressFit".into();
    }
    if f.pads.iter().any(|pad| {
        pad.geometry
            .as_ref()
            .is_some_and(|g| g.pad_type == "thru_hole")
    }) {
        return "THT".into();
    }
    "SMT".into()
}

fn package_name(c: &Component) -> String {
    match &c.library_footprint {
        Some(footprint) => footprint.definition.name.clone(),
        None => c.kind.clone(),
    }
}

fn scale_coords(profile: &AssemblyPlacementProfile) -> f64 {
    match profile.units {
        PlacementUnits::Millimetres => 1.0,
        PlacementUnits::Inches => 1.0 / 25.4,
    }
}

fn coord_decimals(profile: &AssemblyPlacementProfile) -> usize {
    match profile.units {
        PlacementUnits::Millimetres => 3,
        PlacementUnits::Inches => 4,
    }
}

fn rotation_decimals(_profile: &AssemblyPlacementProfile) -> usize {
    1
}

fn field_cell(row: &PlacementRow, field: PlacementField, profile: &AssemblyPlacementProfile) -> String {
    let scale = scale_coords(profile);
    let cd = coord_decimals(profile);
    let rd = rotation_decimals(profile);
    match field {
        PlacementField::MountType => row.mount.clone(),
        PlacementField::Reference => format!("\"{}\"", csv_safe(&row.reference)),
        PlacementField::X => format!("{:.prec$}", row.x * scale, prec = cd),
        PlacementField::Y => format!("{:.prec$}", row.y * scale, prec = cd),
        PlacementField::Rotation => format!(
            "{:.prec$}",
            export_rotation(row.rotation, row.bottom, profile),
            prec = rd
        ),
        PlacementField::Side => row.side.clone(),
        PlacementField::Package => format!("\"{}\"", csv_safe(&row.package)),
    }
}

fn csv_safe(text: &str) -> String {
    text.replace('"', " ")
}

/// Write a centroid file using an optional profile. With no profile the bytes
/// match the historical `placement_text` layout.
pub fn placement_text_with_profile(
    p: &Project,
    profile: Option<&AssemblyPlacementProfile>,
) -> Result<String, String> {
    let legacy = AssemblyPlacementProfile::legacy_default();
    let profile = profile.unwrap_or(&legacy);
    profile.validate()?;
    let rows = placement_rows(p)?;
    let header_join = if profile.header_lines.is_empty() {
        format!("#\n#\tPick and Place File\t{}\n#\n", p.name)
    } else {
        let mut block = String::from("#\n");
        for line in &profile.header_lines {
            block.push('#');
            if !line.starts_with('\t') {
                block.push('\t');
            }
            block.push_str(line);
            block.push('\n');
        }
        block
    };
    let column_line = if profile.header_lines.is_empty()
        && profile.units == PlacementUnits::Millimetres
        && profile.bottom_rotation == BottomRotationPolicy::AsAuthored
        && profile.reference_angle_deg.abs() < f64::EPSILON
        && profile.columns == AssemblyPlacementProfile::legacy_default().columns
    {
        "SMT,THT,X,Y,Rotation,Side,Package".into()
    } else {
        profile
            .columns
            .iter()
            .map(|c| {
                if c.header.is_empty() {
                    placement_field_default_header(c.field)
                } else {
                    c.header.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    let body: Vec<String> = rows
        .iter()
        .map(|row| {
            profile
                .columns
                .iter()
                .map(|col| field_cell(row, col.field, profile))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect();
    Ok(format!(
        "File Format,ASCII,\n#{header_join}{column_line}\n{}\n",
        body.join("\n")
    ))
}

fn placement_field_default_header(field: PlacementField) -> String {
    match field {
        PlacementField::MountType => "SMT,THT".into(),
        PlacementField::Reference => String::new(),
        PlacementField::X => "X".into(),
        PlacementField::Y => "Y".into(),
        PlacementField::Rotation => "Rotation".into(),
        PlacementField::Side => "Side".into(),
        PlacementField::Package => "Package".into(),
    }
}

fn gerber_field(text: &str) -> String {
    text.chars()
        .map(|c| {
            if matches!(c, ',' | '*' | '%') {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn component_field(c: &Component, keys: &[&str]) -> String {
    for key in keys {
        if let Some(value) = c.field(key).filter(|v| !v.is_empty()) {
            return gerber_field(value);
        }
    }
    String::new()
}

fn gerber_mount(mount: &str) -> &'static str {
    match mount {
        "THT" => "TH",
        "SMT" => "SMD",
        "PressFit" => "Other",
        _ if mount.starts_with("FID") => "Fiducial",
        _ => "Other",
    }
}

/// Gerber X3 component layers for the top and bottom sides.
pub fn gerber_component_layers(
    p: &Project,
    profile: Option<&AssemblyPlacementProfile>,
) -> Result<BTreeMap<String, String>, String> {
    p.validate()?;
    let legacy = AssemblyPlacementProfile::legacy_default();
    let profile = profile.unwrap_or(&legacy);
    profile.validate()?;
    let inch = p.unit == "inch";
    let scale = if inch { 1.0 / 25.4 } else { 1.0 };
    let aperture_mm = if inch { 0.012 } else { 0.300 };
    let refs: BTreeMap<u64, &Component> = p.components.iter().map(|c| (c.id, c)).collect();
    let last_layer = p.copper_layer_count().max(1);
    let mut files = BTreeMap::new();
    for (bottom, suffix, layer_index, side_tag) in [
        (false, "F", 1_u8, "Top"),
        (true, "B", last_layer as u8, "Bot"),
    ] {
        let mut body = String::new();
        let mut ops = 0usize;
        for f in &p.board.footprints {
            if (f.board_side() == Layer::Back) != bottom {
                continue;
            }
            let c = refs
                .get(&f.component)
                .ok_or("footprint references an unknown component")?;
            ops += 1;
            if ops > MAX_PLACED {
                return Err("component layer exceeds the placed-object limit".into());
            }
            let mount = mount_type_label(f);
            let rotation = export_rotation(f.rotation, bottom, profile);
            let mfr = component_field(c, &["manufacturer", "Manufacturer", "Mfr"]);
            let mpn = component_field(c, &["MPN", "mpn"]);
            let height = component_field(c, &["height", "Height"]);
            let footprint = package_name(c);
            let library = c
                .library_footprint
                .as_ref()
                .map(|lf| gerber_field(&lf.library))
                .unwrap_or_default();
            body.push_str("D10*\n");
            body.push_str(&format!("%TO.C,{}*%\n", gerber_field(&c.reference)));
            if !mfr.is_empty() {
                body.push_str(&format!("%TO.CMfr,{}*%\n", mfr));
            }
            if !mpn.is_empty() {
                body.push_str(&format!("%TO.CMPN,{}*%\n", mpn));
            }
            body.push_str(&format!("%TO.CVal,{}*%\n", gerber_field(&c.value)));
            body.push_str(&format!("%TO.CMnt,{}*%\n", gerber_mount(&mount)));
            body.push_str(&format!("%TO.CFtp,{}*%\n", gerber_field(&footprint)));
            body.push_str(&format!("%TO.CPgN,{}*%\n", gerber_field(&footprint)));
            if !library.is_empty() {
                body.push_str(&format!("%TO.CLbN,{}*%\n", library));
            }
            if !height.is_empty() {
                body.push_str(&format!("%TO.CHgt,{}*%\n", height));
            }
            body.push_str(&format!("%TO.CRot,{rotation:.3}*%\n"));
            body.push_str(&format!(
                "X{:.6}Y{:.6}D03*\n%TD*%\n",
                f.position.x * scale,
                f.position.y * scale
            ));
        }
        for (i, fid) in p.board.fiducials.iter().enumerate() {
            if (fid.layer == Layer::Back) != bottom {
                continue;
            }
            ops += 1;
            if ops > MAX_PLACED {
                return Err("component layer exceeds the placed-object limit".into());
            }
            let reference = format!("FID{:02}", i + 1);
            body.push_str("D10*\n");
            body.push_str(&format!("%TO.C,{}*%\n", reference));
            body.push_str("%TO.CMnt,Fiducial*%\n");
            body.push_str("%TO.CRot,0*%\n");
            body.push_str(&format!(
                "X{:.6}Y{:.6}D03*\n%TD*%\n",
                fid.position.x * scale,
                fid.position.y * scale
            ));
        }
        let file_function = format!("Component,L{layer_index},{side_tag}");
        let text = format!(
            "G04 {side_tag} component layer generated by PCBCraft *\n\
             %TF.GenerationSoftware,Tangency Labs,PCBCraft*%\n\
             %TF.FileFunction,{file_function}*%\n\
             %TF.FilePolarity,Positive*%\n\
             %TF.SameCoordinates,PCBCraftFabrication*%\n\
             %FSLAX46Y46*%\n\
             %{unit}*%\n\
             %LPD*%\n\
             G01*\n\
             %TA.AperFunction,ComponentMain*%\n\
             %ADD10C,{aperture:.6}*%\n\
             {body}M02*\n",
            side_tag = if bottom {
                "Back"
            } else {
                "Front"
            },
            unit = if inch { "MOIN" } else { "MOMM" },
            aperture = aperture_mm * if inch { 1.0 } else { 1.0 },
        );
        files.insert(format!("pcb-{suffix}.Cmp.gbr"), text);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::Pin;
    use pcbcraft_geom::Point;
    use pcbcraft_library::FootprintPad;

    fn smd_pad(layers: &[&str]) -> FootprintPad {
        FootprintPad {
            number: "1".into(),
            pad_type: "smd".into(),
            shape: "rect".into(),
            position: Point::default(),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: layers.iter().map(|s| (*s).into()).collect(),
            drill: None,
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        }
    }

    #[test]
    fn export_rotation_negates_only_bottom_when_asked() {
        let profile = AssemblyPlacementProfile {
            bottom_rotation: BottomRotationPolicy::NegateOnBottom,
            ..AssemblyPlacementProfile::legacy_default()
        };
        assert_eq!(export_rotation(30.0, false, &profile), 30.0);
        assert_eq!(export_rotation(30.0, true, &profile), 330.0);
    }

    #[test]
    fn export_rotation_adds_180_on_bottom_when_asked() {
        let profile = AssemblyPlacementProfile {
            bottom_rotation: BottomRotationPolicy::Offset180OnBottom,
            ..AssemblyPlacementProfile::legacy_default()
        };
        assert_eq!(export_rotation(10.0, true, &profile), 190.0);
    }
}
