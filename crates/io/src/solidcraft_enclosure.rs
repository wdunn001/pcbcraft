//! SolidCraft enclosure and assembly geometry for the 3D board view.
//!
//! Context: issue #735. Reads a design through [`crate::solidcraft_format`] and
//! returns every body with its occurrence transform and appearance. The layer is
//! editor-only and never enters fabrication outputs.
//!
//! # Rules
//! - When [`solidcraft_has_board_export_frame`] is true, placement stays identity.
//! - Otherwise the project stores offset, scale and rotation in millimetres and degrees.
//! - Meshes are merged into [`BoardMesh`] as `PartKind::Body` with family `enclosure`.
//!
//! # Dependencies
//! - `crate::solidcraft_format`, `crate::board_mesh`, `pcbcraft_doc`
//!
//! # Do not
//! - Do not link SolidCraft crates.

use crate::board_mesh::{apply_matrix, mul, MeshPart, PartKind, rot_x, rot_y, rot_z, translate, diag};
use crate::solidcraft_format::{read_solidcraft, SolidcraftDesign};
use crate::solidcraft_write::{SOLIDCRAFT_METADATA_KEY, SOLIDCRAFT_SCHEMA};
use pcbcraft_doc::{BoardEnclosure, EmbeddedFile, Project, ENCLOSURE_ATTACHMENT_NAME};
use serde_json::Value;
use std::collections::BTreeMap;

/// Default paint opacity for the enclosure layer in the 3D view.
pub const DEFAULT_ENCLOSURE_OPACITY: f64 = 0.45;

/// One body in board space after occurrence and optional user placement.
#[derive(Clone, Debug, PartialEq)]
pub struct EnclosureBody {
    pub name: String,
    pub positions: Vec<[f64; 3]>,
    pub indices: Vec<u32>,
    pub color: Option<[f32; 3]>,
    pub opacity: f64,
}

/// True when metadata marks a PCBCraft board export (board file origin, Z up).
pub fn solidcraft_has_board_export_frame(design: &SolidcraftDesign) -> bool {
    design
        .metadata
        .get(SOLIDCRAFT_METADATA_KEY)
        .and_then(Value::as_object)
        .is_some_and(|m| {
            m.get("schema")
                .and_then(Value::as_str)
                == Some(SOLIDCRAFT_SCHEMA)
                && m.get("frame").is_some()
        })
}

/// Read every body from a SolidCraft file in scene mode.
pub fn solidcraft_scene(bytes: &[u8]) -> Result<Vec<EnclosureBody>, String> {
    let design = read_solidcraft(bytes)?;
    scene_from_design(&design, &identity_placement())
}

/// Scene bodies with an extra user placement matrix (row-major).
pub fn solidcraft_scene_with_placement(
    bytes: &[u8],
    placement: &[f64; 16],
) -> Result<Vec<EnclosureBody>, String> {
    let design = read_solidcraft(bytes)?;
    scene_from_design(&design, placement)
}

fn scene_from_design(
    design: &SolidcraftDesign,
    placement: &[f64; 16],
) -> Result<Vec<EnclosureBody>, String> {
    let worlds = occurrence_worlds(design)?;
    let mut out = vec![];
    for occ in &design.occurrences {
        let Some(world) = worlds.get(&occ.id) else {
            continue;
        };
        let combined = mul(placement, world);
        for feature in &design.features {
            if !feature_matches_component(feature, occ.component) {
                continue;
            }
            push_feature(feature, &combined, &design.appearances, &mut out)?;
        }
    }
    if out.is_empty() {
        return Err("The SolidCraft file has no mesh bodies to show.".into());
    }
    Ok(out)
}

/// Append enclosure triangles to a board mesh when the project carries one.
pub fn append_enclosure_parts(
    parts: &mut Vec<MeshPart>,
    project: &Project,
    enc: &BoardEnclosure,
) -> Result<(), String> {
    if !enc.visible {
        return Ok(());
    }
    let bytes = enclosure_bytes(project, enc)?;
    let placement = if enc.auto_aligned {
        identity_placement()
    } else {
        user_placement(enc)
    };
    let bodies = solidcraft_scene_with_placement(&bytes, &placement)?;
    for body in bodies {
        if body.indices.is_empty() {
            continue;
        }
        let mut part = MeshPart::new(PartKind::Body, format!("enclosure:{}", body.name));
        part.family = Some("enclosure".into());
        part.component = None;
        part.color = body.color;
        part.positions = body.positions;
        part.indices = body.indices;
        parts.push(part);
    }
    Ok(())
}

pub fn enclosure_bytes(project: &Project, enc: &BoardEnclosure) -> Result<Vec<u8>, String> {
    let file = project
        .files
        .iter()
        .find(|f| f.name == enc.file)
        .ok_or_else(|| "The project has no enclosure attachment.".to_string())?;
    crate::embedded_bytes(file)
}

/// Replace or add the base64 attachment and return the attachment name.
pub fn attach_enclosure_file(project: &mut Project, bytes: &[u8]) -> Result<(), String> {
    use base64::Engine as _;
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    if data.len() > pcbcraft_doc::MAX_BINARY_ATTACHMENT_BASE64 {
        return Err("The enclosure file is over the size limit.".into());
    }
    let file = EmbeddedFile {
        name: ENCLOSURE_ATTACHMENT_NAME.into(),
        data,
        encoding: "base64".into(),
    };
    if !file.valid() {
        return Err("The enclosure attachment is too large.".into());
    }
    if let Some(slot) = project.files.iter_mut().find(|f| f.name == ENCLOSURE_ATTACHMENT_NAME) {
        *slot = file;
    } else {
        if project
            .files
            .iter()
            .filter(|f| f.is_binary())
            .count()
            >= pcbcraft_doc::MAX_BINARY_ATTACHMENTS
        {
            return Err("The project already holds the maximum number of binary attachments.".into());
        }
        project.files.push(file);
    }
    Ok(())
}

fn identity_placement() -> [f64; 16] {
    diag(1.0, 1.0, 1.0)
}

fn user_placement(enc: &BoardEnclosure) -> [f64; 16] {
    let scale = diag(enc.scale[0], enc.scale[1], enc.scale[2]);
    let turn = mul(
        &rot_z(-enc.rotate[2]),
        &mul(&rot_y(-enc.rotate[1]), &rot_x(-enc.rotate[0])),
    );
    mul(
        &translate(enc.offset[0], enc.offset[1], enc.offset[2]),
        &mul(&turn, &scale),
    )
}

fn occurrence_worlds(design: &SolidcraftDesign) -> Result<BTreeMap<u64, [f64; 16]>, String> {
    let mut by_id = BTreeMap::new();
    for occ in &design.occurrences {
        by_id.insert(occ.id, occ);
    }
    let mut worlds = BTreeMap::new();
    for occ in &design.occurrences {
        worlds.insert(occ.id, world_for(occ.id, &by_id)?);
    }
    Ok(worlds)
}

fn world_for(
    id: u64,
    by_id: &BTreeMap<u64, &crate::solidcraft_format::SolidcraftOccurrence>,
) -> Result<[f64; 16], String> {
    let occ = by_id
        .get(&id)
        .ok_or_else(|| "The design references a missing occurrence.".to_string())?;
    let local = occ
        .transform
        .as_ref()
        .map(parse_solidcraft_transform)
        .transpose()?
        .unwrap_or_else(identity_placement);
    if occ.parent == 0 || occ.parent == id {
        return Ok(local);
    }
    let parent = world_for(occ.parent, by_id)?;
    Ok(mul(&parent, &local))
}

fn parse_solidcraft_transform(value: &Value) -> Result<[f64; 16], String> {
    let cols = value
        .as_array()
        .ok_or_else(|| "An occurrence transform is not a matrix.".to_string())?;
    if cols.len() != 4 {
        return Err("An occurrence transform is not 4 by 4.".into());
    }
    let mut out = [0.0; 16];
    for (c, col) in cols.iter().enumerate().take(4) {
        let col = col
            .as_array()
            .ok_or_else(|| "An occurrence transform column is not an array.".to_string())?;
        if col.len() != 4 {
            return Err("An occurrence transform is not 4 by 4.".into());
        }
        for (r, cell) in col.iter().enumerate().take(4) {
            let v = cell
                .as_f64()
                .ok_or_else(|| "An occurrence transform cell is not a number.".to_string())?;
            if !v.is_finite() {
                return Err("An occurrence transform cell is not finite.".into());
            }
            out[r * 4 + c] = v;
        }
    }
    Ok(out)
}

fn feature_matches_component(feature: &Value, component_id: u64) -> bool {
    match feature.get("component").and_then(Value::as_u64) {
        Some(id) => id == component_id,
        None => component_id == 0,
    }
}

fn push_feature(
    feature: &Value,
    world: &[f64; 16],
    appearances: &crate::solidcraft_format::SolidcraftAppearances,
    out: &mut Vec<EnclosureBody>,
) -> Result<(), String> {
    let kind = feature.get("type").and_then(Value::as_str).unwrap_or("");
    if kind != "mesh_import" {
        return Ok(());
    }
    let Some(meshes) = feature.get("meshes").and_then(Value::as_array) else {
        return Ok(());
    };
    for mesh in meshes {
        let name = mesh
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Body")
            .to_string();
        let (color, opacity) = appearance_for(&name, mesh, appearances);
        let (positions, indices) = read_mesh(mesh)?;
        if positions.is_empty() || indices.is_empty() {
            continue;
        }
        let transformed: Vec<[f64; 3]> = positions
            .iter()
            .map(|p| apply_matrix(world, *p))
            .collect();
        out.push(EnclosureBody {
            name,
            positions: transformed,
            indices,
            color,
            opacity,
        });
    }
    Ok(())
}

fn appearance_for(
    name: &str,
    mesh: &Value,
    appearances: &crate::solidcraft_format::SolidcraftAppearances,
) -> (Option<[f32; 3]>, f64) {
    let look = appearances
        .bodies
        .get(name)
        .or_else(|| mesh.get("name").and_then(|n| n.as_str()).and_then(|n| appearances.bodies.get(n)));
    if let Some(look) = look {
        let color = look.get("color").and_then(parse_color);
        let opacity = look
            .get("opacity")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        return (color, opacity);
    }
    (mesh.get("color").and_then(parse_color), 1.0)
}

fn parse_color(value: &Value) -> Option<[f32; 3]> {
    let arr = value.as_array()?;
    if arr.len() != 3 {
        return None;
    }
    let to_f = |v: &Value| {
        v.as_f64()
            .map(|n| {
                if n > 1.0 {
                    (n / 255.0) as f32
                } else {
                    n as f32
                }
            })
            .filter(|f| f.is_finite())
    };
    Some([to_f(&arr[0])?, to_f(&arr[1])?, to_f(&arr[2])?])
}

fn read_mesh(mesh: &Value) -> Result<(Vec<[f64; 3]>, Vec<u32>), String> {
    let positions = mesh
        .get("positions")
        .and_then(Value::as_array)
        .ok_or_else(|| "A mesh is missing its positions.".to_string())?;
    let triangles = mesh
        .get("triangles")
        .and_then(Value::as_array)
        .ok_or_else(|| "A mesh is missing its triangles.".to_string())?;
    let mut out_pos = Vec::with_capacity(positions.len() / 3);
    for chunk in positions.as_chunks::<3>().0 {
        let p = [
            chunk[0].as_f64().ok_or_else(|| "A mesh position is not a number.".to_string())?,
            chunk[1].as_f64().ok_or_else(|| "A mesh position is not a number.".to_string())?,
            chunk[2].as_f64().ok_or_else(|| "A mesh position is not a number.".to_string())?,
        ];
        if !p.iter().all(|v| v.is_finite()) {
            return Err("A mesh position is not finite.".into());
        }
        out_pos.push(p);
    }
    let mut out_idx = Vec::with_capacity(triangles.len());
    for v in triangles {
        let n = v
            .as_u64()
            .ok_or_else(|| "A mesh index is not an integer.".to_string())?;
        let n = u32::try_from(n).map_err(|_| "A mesh index is too large.".to_string())?;
        if n as usize >= out_pos.len() {
            return Err("A mesh index points past its vertices.".into());
        }
        out_idx.push(n);
    }
    Ok((out_pos, out_idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{board_mesh, solidcraft_board, MeshLayers, SolidcraftLayers};
    use pcbcraft_doc::Project;

    type Check = Result<(), String>;

    fn sample_project() -> Project {
        let mut p = Project::default();
        p.board.width = 30.0;
        p.board.height = 20.0;
        p.board.thickness = 1.6;
        p
    }

    #[test]
    fn enclosure_from_an_export_needs_no_placement() -> Check {
        let project = sample_project();
        let mesh = board_mesh(&project, MeshLayers::default())?;
        let json = solidcraft_board(&project, &mesh, SolidcraftLayers::default())?;
        let design = read_solidcraft(json.as_bytes())?;
        assert!(
            solidcraft_has_board_export_frame(&design),
            "export carries a board frame"
        );
        let board_only = mesh
            .parts
            .iter()
            .find(|p| p.kind == PartKind::Board)
            .ok_or("board part")?;
        let (b_lo, b_hi) = board_only.bounds().ok_or("board bounds")?;
        let scene = solidcraft_scene(json.as_bytes())?;
        let export_board = scene
            .iter()
            .find(|b| b.name == "Board")
            .ok_or("Board body in export")?;
        let xs: Vec<f64> = export_board.positions.iter().map(|p| p[0]).collect();
        let ys: Vec<f64> = export_board.positions.iter().map(|p| p[1]).collect();
        let zs: Vec<f64> = export_board.positions.iter().map(|p| p[2]).collect();
        let (e_lo, e_hi) = (
            [
                xs.iter().copied().fold(f64::INFINITY, f64::min),
                ys.iter().copied().fold(f64::INFINITY, f64::min),
                zs.iter().copied().fold(f64::INFINITY, f64::min),
            ],
            [
                xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                zs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            ],
        );
        for i in 0..3 {
            if (e_lo[i] - b_lo[i]).abs() > 0.05 || (e_hi[i] - b_hi[i]).abs() > 0.05 {
                return Err(format!(
                    "board body axis {i} mismatch: mesh {:?}-{:?} export {:?}-{:?}",
                    b_lo, b_hi, e_lo, e_hi
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn enclosure_is_not_in_fab_outputs() -> Check {
        let mut project = sample_project();
        let mesh = board_mesh(&project, MeshLayers::default())?;
        let json = solidcraft_board(&project, &mesh, SolidcraftLayers::default())?;
        attach_enclosure_file(&mut project, json.as_bytes())?;
        project.board.enclosure = Some(BoardEnclosure {
            file: ENCLOSURE_ATTACHMENT_NAME.into(),
            source_path: String::new(),
            auto_aligned: true,
            offset: [0.0; 3],
            scale: [1.0; 3],
            rotate: [0.0; 3],
            visible: true,
            opacity: DEFAULT_ENCLOSURE_OPACITY,
        });
        let step = crate::step_board(&project)?;
        assert!(
            !step.to_ascii_lowercase().contains("enclosure"),
            "step must not name enclosure bodies"
        );
        let names = crate::fab_export::gerber_files(&project)?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let readme = crate::fab_readme_text(&project, &names)?;
        assert!(
            !readme.contains("solidcraft") && !readme.contains("enclosure"),
            "fab readme must not mention the enclosure layer"
        );
        Ok(())
    }
}
