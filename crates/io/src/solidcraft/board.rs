//! Writer for SolidCraft design files: the board as a project of named, placed bodies.
//!
//! Context: SolidCraft is the owner's CAD app. A design there is a JSON document (or a ZIP
//! container around the same document) with components, occurrences (one rigid placement
//! each), embedded mesh bodies, appearances and a free metadata map. Model space is
//! millimetres, Z up, right-handed, exactly the frame of [`crate::BoardMesh`], so no axis
//! conversion happens here. The research note `research-solidcraft-interop.md` has the full
//! mapping; this file is slice 1 (plain JSON, mesh bodies).
//!
//! # Rules
//! - Written file format number is [`SOLIDCRAFT_FORMAT`], the oldest number that expresses
//!   what this writer uses. Every newer SolidCraft upgrades it on open.
//! - The board slab and any copper, mask and silk bodies live in the root component with
//!   an identity placement. Each placed footprint is its own component named by its
//!   reference designator, with one occurrence whose transform is the footprint frame
//!   (position, rotation, bottom flip). Its mesh is stored in that frame, so moving the
//!   occurrence in SolidCraft moves the part about its own origin.
//! - Occurrence transforms are rigid. A loaded model's own offset, scale and turn are
//!   baked into the stored mesh, never into the occurrence.
//! - Everything PCBCraft knows about a body is stored under the metadata key
//!   [`SOLIDCRAFT_METADATA_KEY`] and keyed by body name, the identity SolidCraft itself uses
//!   for appearances and materials. The reference designator is the match key on re-import.
//!
//! # Dependencies
//! - crates/io/src/board_mesh.rs for [`BoardMesh`], [`MeshPart`] and [`crate::model_matrix`].
//! - crates/doc/src/lib.rs for `Project` and `Footprint`.
//!
//! # Do not
//! - Do not read model files here; the caller applies [`crate::with_models`] to the mesh first.
//! - Do not write nets, tracks or pad numbers; copper leaves as mesh only.
//! - Do not link SolidCraft's crates; the format is a documented subset checked by fixtures.
use crate::board_mesh::{BoardMesh, MeshPart, PartKind, model_matrix};
use pcbcraft_doc::{Layer, Project};
use pcbcraft_library::Model3D;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// The SolidCraft design format number written into the file.
pub const SOLIDCRAFT_FORMAT: u32 = 22;
/// The reverse-DNS metadata key that holds everything PCBCraft adds to the file.
pub const SOLIDCRAFT_METADATA_KEY: &str = "org.pcbcraft";
/// Schema stamp of the object under [`SOLIDCRAFT_METADATA_KEY`].
pub const SOLIDCRAFT_SCHEMA: &str = "pcbcraft-interop/1";
const MAX_COORD: f64 = 1e9;

/// Which optional thin layers leave the board as bodies of their own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SolidcraftLayers {
    pub copper: bool,
    pub mask: bool,
    pub silk: bool,
}

/// Write the board as a SolidCraft design (plain JSON text).
///
/// The board slab and every placed part are always written; `layers` adds copper, mask
/// and silk as bodies. `mesh` should come from [`crate::board_mesh`], with
/// [`crate::with_models`] applied when model files are available, so loaded models leave
/// as their triangles and the rest as generated package bodies.
///
/// # Rules
/// - Fails with a plain sentence on non-finite or out-of-range coordinates and on indices
///   past the vertex list; nothing partial is returned.
/// - Body names are unique: a part named like a board body gets the suffix ` (part)`.
/// # Dependencies
/// - crates/io/src/board_mesh.rs
/// # Do not
/// - Do not change the key layout under [`SOLIDCRAFT_METADATA_KEY`] without bumping
///   [`SOLIDCRAFT_SCHEMA`].
pub fn solidcraft_board(
    project: &Project,
    mesh: &BoardMesh,
    layers: SolidcraftLayers,
) -> Result<String, String> {
    let thickness = project.board.thickness;
    let mut ids = Ids::default();
    let mut doc = Doc::default();
    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut bodies_meta = Map::new();

    for (kind, display, material, look) in [
        (PartKind::Board, "Board", Some("FR-4"), Look::board()),
        (
            PartKind::CopperTop,
            "Copper top",
            Some("Copper"),
            Look::copper(),
        ),
        (
            PartKind::CopperBottom,
            "Copper bottom",
            Some("Copper"),
            Look::copper(),
        ),
        (PartKind::MaskTop, "Mask top", None, Look::mask()),
        (PartKind::MaskBottom, "Mask bottom", None, Look::mask()),
        (PartKind::SilkTop, "Silk top", None, Look::silk()),
        (PartKind::SilkBottom, "Silk bottom", None, Look::silk()),
    ] {
        let wanted = match kind {
            PartKind::Board => true,
            PartKind::CopperTop | PartKind::CopperBottom => layers.copper,
            PartKind::MaskTop | PartKind::MaskBottom => layers.mask,
            _ => layers.silk,
        };
        if !wanted {
            continue;
        }
        let merged = merge(mesh.parts.iter().filter(|p| p.kind == kind));
        if merged.indices.is_empty() {
            continue;
        }
        check_mesh(display, &merged)?;
        names.insert(display.to_string());
        doc.mesh_feature(&mut ids, 0, display, &merged, None);
        doc.look(display, look);
        if let Some(m) = material {
            doc.materials.insert(display.to_string(), json!(m));
        }
        bodies_meta.insert(
            display.to_string(),
            json!({"kind": kind.name(), "source": "generated"}),
        );
    }

    let mut part_names: BTreeSet<String> = BTreeSet::new();
    for part in mesh.parts.iter().filter(|p| p.kind == PartKind::Body) {
        let Some(component_id) = part.component else {
            continue;
        };
        if part.indices.is_empty() {
            continue;
        }
        let component = project.component(component_id);
        let footprint = project
            .board
            .footprints
            .iter()
            .find(|f| f.component == component_id);
        let reference = component
            .map(|c| c.reference.clone())
            .filter(|r| !r.trim().is_empty())
            .unwrap_or_else(|| part.name.clone());
        let body = unique_name(&reference, &names, &mut part_names);
        names.insert(body.clone());
        // The footprint frame: where the part sits on the board. Its mesh is stored in it.
        let frame = match footprint {
            Some(fp) => model_matrix(
                &Model3D {
                    path: String::new(),
                    offset: [0.0; 3],
                    scale: [1.0; 3],
                    rotate: [0.0; 3],
                    raw_source: String::new(),
                },
                fp.position,
                fp.rotation,
                fp.board_side() == Layer::Back,
                thickness,
            ),
            None => IDENTITY16,
        };
        let (local, placement) = match rigid_inverse(&frame) {
            Some(inv) => (to_frame(part, &inv), frame),
            None => (part.clone(), IDENTITY16),
        };
        check_mesh(&body, &local)?;
        let color = part.color.map(|c| {
            [
                (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                (c[2].clamp(0.0, 1.0) * 255.0).round() as u8,
            ]
        });
        let component_index = ids.next();
        let occurrence_id = ids.next();
        doc.components
            .push(json!({"id": component_index, "name": reference, "parent": 0}));
        doc.occurrences.push(json!({
            "id": occurrence_id,
            "component": component_index,
            "parent": 0,
            "name": reference,
            "transform": column_major(&placement),
            "grounded": false,
        }));
        doc.mesh_feature(&mut ids, component_index, &body, &local, color);
        doc.look(&body, Look::part(color, part.family.as_deref()));
        let loaded = part.model.as_ref().is_some_and(|m| m.loaded);
        let mut meta = Map::new();
        meta.insert("kind".into(), json!("part"));
        meta.insert("component".into(), json!(reference));
        meta.insert("component_id".into(), json!(component_id));
        meta.insert(
            "source".into(),
            json!(if loaded { "model" } else { "generated" }),
        );
        if let Some(c) = component {
            meta.insert("value".into(), json!(c.value));
            if let Some(lib) = &c.library_footprint {
                meta.insert("footprint".into(), json!(lib.definition.name));
            }
        }
        if let Some(family) = &part.family {
            meta.insert("family".into(), json!(family));
        }
        if let Some(model) = &part.model {
            meta.insert("model".into(), json!(model.path));
        }
        bodies_meta.insert(body, Value::Object(meta));
    }

    let name = if project.name.trim().is_empty() {
        "board".to_string()
    } else {
        project.name.clone()
    };
    let mut metadata = Map::new();
    metadata.insert("project".into(), json!(name));
    metadata.insert(
        "description".into(),
        json!("Board exported from PCBCraft: one body per placed part."),
    );
    metadata.insert(
        SOLIDCRAFT_METADATA_KEY.into(),
        json!({
            "schema": SOLIDCRAFT_SCHEMA,
            "exporter": format!("pcbcraft-io {}", env!("CARGO_PKG_VERSION")),
            "project": name,
            "frame": {"units": "mm", "up": "z", "origin": "board file origin", "bottom_face_z": 0},
            "board": {"thickness": thickness},
            "bodies": Value::Object(bodies_meta),
        }),
    );
    let design = json!({
        "format": format!("solidcraft/{SOLIDCRAFT_FORMAT}"),
        "name": name,
        "units": "mm",
        "metadata": Value::Object(metadata),
        "params": [],
        "features": doc.features,
        "next_id": ids.0,
        "components": doc.components,
        "occurrences": doc.occurrences,
        "user_materials": [
            {"name": "FR-4", "density": 1.85},
            {"name": "Copper", "density": 8.96},
        ],
        "materials": Value::Object(doc.materials),
        "appearances": {"bodies": Value::Object(doc.looks)},
    });
    serde_json::to_string_pretty(&design).map_err(|e| e.to_string())
}

const IDENTITY16: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

#[derive(Default)]
struct Ids(u64);
impl Ids {
    fn next(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }
}

#[derive(Default)]
struct Doc {
    features: Vec<Value>,
    components: Vec<Value>,
    occurrences: Vec<Value>,
    materials: Map<String, Value>,
    looks: Map<String, Value>,
}
impl Doc {
    fn mesh_feature(
        &mut self,
        ids: &mut Ids,
        component: u64,
        body: &str,
        part: &MeshPart,
        color: Option<[u8; 3]>,
    ) {
        let positions: Vec<f64> = part.positions.iter().flatten().copied().collect();
        let mut mesh = json!({
            "name": body,
            "positions": positions,
            "triangles": part.indices,
        });
        if let (Some(c), Some(object)) = (color, mesh.as_object_mut()) {
            object.insert(
                "color".into(),
                json!([
                    f32::from(c[0]) / 255.0,
                    f32::from(c[1]) / 255.0,
                    f32::from(c[2]) / 255.0
                ]),
            );
        }
        let mut feature = json!({
            "id": ids.next(),
            "name": body,
            "type": "mesh_import",
            "file": "pcbcraft",
            "meshes": [mesh],
        });
        if component != 0
            && let Some(object) = feature.as_object_mut()
        {
            object.insert("component".into(), json!(component));
        }
        self.features.push(feature);
    }
    fn look(&mut self, body: &str, look: Look) {
        self.looks.insert(
            body.to_string(),
            json!({"name": look.name, "color": look.color, "opacity": look.opacity}),
        );
    }
}

struct Look {
    name: &'static str,
    color: [u8; 3],
    opacity: f64,
}
impl Look {
    fn board() -> Self {
        Self {
            name: "Solder mask green",
            color: [22, 90, 52],
            opacity: 1.0,
        }
    }
    fn copper() -> Self {
        Self {
            name: "Copper",
            color: [184, 115, 72],
            opacity: 1.0,
        }
    }
    fn mask() -> Self {
        Self {
            name: "Solder mask",
            color: [22, 90, 52],
            opacity: 0.6,
        }
    }
    fn silk() -> Self {
        Self {
            name: "Silkscreen",
            color: [240, 240, 235],
            opacity: 1.0,
        }
    }
    fn part(model_color: Option<[u8; 3]>, family: Option<&str>) -> Self {
        let color = model_color.unwrap_or(match family {
            Some("soic" | "qfp" | "sot" | "dip") => [30, 30, 34],
            Some("chip") => [150, 140, 120],
            _ => [70, 70, 76],
        });
        Self {
            name: "Part",
            color,
            opacity: 1.0,
        }
    }
}

/// Concatenate parts of one kind into one mesh (top and bottom stay separate bodies).
fn merge<'a>(parts: impl Iterator<Item = &'a MeshPart>) -> MeshPart {
    let mut out = MeshPart {
        kind: PartKind::Board,
        name: String::new(),
        component: None,
        family: None,
        height_source: None,
        model: None,
        color: None,
        positions: vec![],
        indices: vec![],
    };
    for p in parts {
        let base = out.positions.len() as u32;
        out.kind = p.kind;
        out.positions.extend_from_slice(&p.positions);
        out.indices.extend(p.indices.iter().map(|i| i + base));
    }
    out
}

fn check_mesh(body: &str, part: &MeshPart) -> Result<(), String> {
    if !part.indices.len().is_multiple_of(3) {
        return Err(format!("body {body} has a broken triangle list"));
    }
    if part
        .positions
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > MAX_COORD)
    {
        return Err(format!(
            "body {body} has a coordinate that is not a usable number"
        ));
    }
    if part
        .indices
        .iter()
        .any(|&i| i as usize >= part.positions.len())
    {
        return Err(format!("body {body} points past its vertex list"));
    }
    Ok(())
}

/// A body name nobody else in the file has: part names that clash with a board body or an
/// earlier part get ` (part)` or a number.
fn unique_name(reference: &str, taken: &BTreeSet<String>, parts: &mut BTreeSet<String>) -> String {
    let mut candidate = reference.to_string();
    let mut n = 1;
    while taken.contains(&candidate) || parts.contains(&candidate) {
        candidate = if n == 1 {
            format!("{reference} (part)")
        } else {
            format!("{reference} (part {n})")
        };
        n += 1;
    }
    parts.insert(candidate.clone());
    candidate
}

/// Row-major 4x4 to SolidCraft's column-major `[[f64; 4]; 4]` (`m[col][row]`).
fn column_major(m: &[f64; 16]) -> [[f64; 4]; 4] {
    let mut out = [[0.0; 4]; 4];
    for (c, column) in out.iter_mut().enumerate() {
        for (r, cell) in column.iter_mut().enumerate() {
            *cell = m[r * 4 + c];
        }
    }
    out
}

/// Inverse of a rigid row-major transform, or `None` when its 3x3 block is not a rotation.
fn rigid_inverse(m: &[f64; 16]) -> Option<[f64; 16]> {
    let r = |row: usize, col: usize| m[row * 4 + col];
    for i in 0..3 {
        for j in 0..3 {
            let dot: f64 = (0..3).map(|k| r(k, i) * r(k, j)).sum();
            let want = if i == j { 1.0 } else { 0.0 };
            if (dot - want).abs() > 1e-9 {
                return None;
            }
        }
    }
    let t = [r(0, 3), r(1, 3), r(2, 3)];
    let mut out = IDENTITY16;
    for row in 0..3 {
        for col in 0..3 {
            out[row * 4 + col] = r(col, row);
        }
        out[row * 4 + 3] = -(0..3).map(|k| r(k, row) * t[k]).sum::<f64>();
    }
    Some(out)
}

/// The part's triangles taken through `inv` (board frame into the footprint frame).
fn to_frame(part: &MeshPart, inv: &[f64; 16]) -> MeshPart {
    let mut out = part.clone();
    out.positions = part
        .positions
        .iter()
        .map(|p| crate::board_mesh::apply_matrix(inv, *p))
        .collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MeshLayers, board_mesh};
    use pcbcraft_geom::Point;

    type Check = Result<(), String>;

    fn resistors() -> Result<Project, String> {
        let definition = pcbcraft_library::read_symbols(include_str!(
            "../../../library/tests/fixtures/device.kicad_sym"
        ))
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.name == "R")
        .ok_or("fixture resistor missing")?;
        let footprint = pcbcraft_library::read_footprint(include_str!(
            "../../../library/tests/fixtures/R_0805_2012Metric.kicad_mod"
        ))
        .map_err(|e| e.to_string())?;
        let pads: Vec<pcbcraft_doc::Pad> = footprint
            .pads
            .iter()
            .map(|p| pcbcraft_doc::Pad {
                number: p.number.clone(),
                offset: p.position,
                diameter: p.size.x.min(p.size.y),
                net: None,
                castellated: None,
                geometry: Some(p.clone()),
            })
            .collect();
        let pins: Vec<pcbcraft_doc::Pin> = definition
            .pins
            .iter()
            .filter(|p| p.unit <= 1 && p.body_style <= 1)
            .map(|p| pcbcraft_doc::Pin {
                number: p.number.clone(),
                name: p.name.clone(),
                offset: p.position,
                net: None,
                electrical_type: p.electrical_type.clone(),
                direction: Some(p.angle),
                unit: p.unit,
            })
            .collect();
        let mut project = Project::default();
        for (n, (reference, x, y, rotation)) in [("R1", 10.0, 10.0, 0.0), ("R2", 30.0, 15.0, 90.0)]
            .into_iter()
            .enumerate()
        {
            let id = 1 + 2 * n as u64;
            project.components.push(pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "library".into(),
                position: Point::new(20.0, 20.0),
                rotation: 0.0,
                pins: pins.clone(),
                units: vec![],
                library_symbol: Some(pcbcraft_doc::LibrarySymbol {
                    library: "Device".into(),
                    unit: 1,
                    definition: definition.clone(),
                }),
                library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                    library: "Resistor_SMD".into(),
                    definition: footprint.clone(),
                }),
                fields: vec![],
            });
            project.board.footprints.push(pcbcraft_doc::Footprint {
                id: id + 1,
                component: id,
                position: Point::new(x, y),
                rotation,
                locked: None,
                side: None,
                pads: pads.clone(),
            });
        }
        project.name = "demo".into();
        project.validate()?;
        Ok(project)
    }

    fn export(project: &Project, layers: SolidcraftLayers) -> Result<Value, String> {
        let mesh = board_mesh(project, MeshLayers::default())?;
        let text = solidcraft_board(project, &mesh, layers)?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }

    fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value, String> {
        v.get(key).ok_or_else(|| format!("missing {key}"))
    }

    fn list<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
        field(v, key)?
            .as_array()
            .ok_or_else(|| format!("{key} is not a list"))
    }

    fn apply_column_major(m: &Value, p: [f64; 3]) -> Result<[f64; 3], String> {
        let at = |c: usize, r: usize| -> Result<f64, String> {
            m.get(c)
                .and_then(|col| col.get(r))
                .and_then(Value::as_f64)
                .ok_or("bad transform".to_string())
        };
        let mut out = [0.0; 3];
        for (r, cell) in out.iter_mut().enumerate() {
            *cell = at(0, r)? * p[0] + at(1, r)? * p[1] + at(2, r)? * p[2] + at(3, r)?;
        }
        Ok(out)
    }

    fn mesh_of<'a>(doc: &'a Value, body: &str) -> Result<&'a Value, String> {
        list(doc, "features")?
            .iter()
            .filter_map(|f| f.get("meshes").and_then(Value::as_array))
            .flatten()
            .find(|m| m.get("name").and_then(Value::as_str) == Some(body))
            .ok_or_else(|| format!("no body {body}"))
    }

    fn points(mesh: &Value) -> Result<Vec<[f64; 3]>, String> {
        let flat: Vec<f64> = list(mesh, "positions")?
            .iter()
            .map(|v| v.as_f64().ok_or("bad coordinate"))
            .collect::<Result<_, _>>()?;
        Ok(flat.chunks(3).map(|c| [c[0], c[1], c[2]]).collect())
    }

    #[test]
    fn exports_board_and_one_component_per_part() -> Check {
        let project = resistors()?;
        let doc = export(&project, SolidcraftLayers::default())?;
        assert_eq!(field(&doc, "format")?, "solidcraft/22");
        assert_eq!(field(&doc, "units")?, "mm");
        let components = list(&doc, "components")?;
        let names: Vec<&str> = components
            .iter()
            .filter_map(|c| c.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(names, ["R1", "R2"]);
        assert_eq!(list(&doc, "occurrences")?.len(), 2);
        mesh_of(&doc, "Board")?;
        mesh_of(&doc, "R1")?;
        mesh_of(&doc, "R2")?;
        assert!(mesh_of(&doc, "Copper top").is_err(), "copper is opt in");
        let ids: BTreeSet<u64> = list(&doc, "features")?
            .iter()
            .map(|f| f.get("id").and_then(Value::as_u64).unwrap_or(0))
            .chain(
                components
                    .iter()
                    .map(|c| c.get("id").and_then(Value::as_u64).unwrap_or(0)),
            )
            .collect();
        assert_eq!(ids.len(), list(&doc, "features")?.len() + components.len());
        Ok(())
    }

    #[test]
    fn occurrence_transform_reproduces_board_coordinates() -> Check {
        let project = resistors()?;
        let board = board_mesh(&project, MeshLayers::default())?;
        let doc = export(&project, SolidcraftLayers::default())?;
        for (reference, component) in [("R1", 1u64), ("R2", 3u64)] {
            let source = board
                .parts
                .iter()
                .find(|p| p.component == Some(component))
                .ok_or("part missing")?;
            let occurrence = list(&doc, "occurrences")?
                .iter()
                .find(|o| o.get("name").and_then(Value::as_str) == Some(reference))
                .ok_or("occurrence missing")?;
            let local = points(mesh_of(&doc, reference)?)?;
            assert_eq!(local.len(), source.positions.len());
            let transform = field(occurrence, "transform")?;
            for (l, want) in local.iter().zip(&source.positions) {
                let got = apply_column_major(transform, *l)?;
                for axis in 0..3 {
                    assert!(
                        (got[axis] - want[axis]).abs() < 1e-9,
                        "{reference} axis {axis}: {got:?} vs {want:?}"
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn exported_board_is_not_mirrored() -> Check {
        let project = resistors()?;
        let doc = export(&project, SolidcraftLayers::default())?;
        let mesh = mesh_of(&doc, "Board")?;
        let p = points(mesh)?;
        let tri: Vec<usize> = list(mesh, "triangles")?
            .iter()
            .map(|v| v.as_u64().unwrap_or(0) as usize)
            .collect();
        let mut volume = 0.0;
        for t in tri.chunks(3) {
            let (a, b, c) = (p[t[0]], p[t[1]], p[t[2]]);
            volume += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0;
        }
        assert!(volume > 0.0, "signed volume {volume} means a mirrored mesh");
        let hi = p.iter().fold(f64::MIN, |m, v| m.max(v[2]));
        let lo = p.iter().fold(f64::MAX, |m, v| m.min(v[2]));
        assert!((lo - 0.0).abs() < 1e-9 && (hi - project.board.thickness).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn metadata_links_each_body_to_its_reference() -> Check {
        let project = resistors()?;
        let doc = export(&project, SolidcraftLayers::default())?;
        let ours = field(field(&doc, "metadata")?, SOLIDCRAFT_METADATA_KEY)?;
        assert_eq!(field(ours, "schema")?, SOLIDCRAFT_SCHEMA);
        let bodies = field(ours, "bodies")?;
        let r2 = field(bodies, "R2")?;
        assert_eq!(field(r2, "component")?, "R2");
        assert_eq!(field(r2, "component_id")?, 3);
        assert_eq!(field(r2, "value")?, "1k");
        assert_eq!(field(r2, "source")?, "generated");
        assert_eq!(field(field(bodies, "Board")?, "kind")?, "board");
        assert_eq!(field(field(&doc, "metadata")?, "project")?, "demo");
        Ok(())
    }

    #[test]
    fn optional_layers_become_bodies_with_looks() -> Check {
        let project = resistors()?;
        let doc = export(
            &project,
            SolidcraftLayers {
                copper: true,
                mask: true,
                silk: true,
            },
        )?;
        mesh_of(&doc, "Copper top")?;
        mesh_of(&doc, "Mask top")?;
        let looks = field(field(&doc, "appearances")?, "bodies")?;
        assert_eq!(field(field(looks, "Copper top")?, "color")?[0], 184);
        assert!(field(field(looks, "Mask top")?, "opacity")?.as_f64() < Some(1.0));
        Ok(())
    }

    #[test]
    fn rigid_inverse_undoes_a_footprint_frame() -> Check {
        let model = Model3D {
            path: String::new(),
            offset: [0.0; 3],
            scale: [1.0; 3],
            rotate: [0.0; 3],
            raw_source: String::new(),
        };
        for bottom in [false, true] {
            let m = model_matrix(&model, Point::new(12.5, -3.0), 37.0, bottom, 1.6);
            let inv = rigid_inverse(&m).ok_or("not rigid")?;
            let p = [1.25, 2.5, 0.75];
            let there = crate::board_mesh::apply_matrix(&m, p);
            let back = crate::board_mesh::apply_matrix(&inv, there);
            for axis in 0..3 {
                assert!((back[axis] - p[axis]).abs() < 1e-9);
            }
        }
        let mut scaled = IDENTITY16;
        scaled[0] = 2.0;
        assert!(rigid_inverse(&scaled).is_none(), "scale is not rigid");
        Ok(())
    }

    #[test]
    fn bad_coordinates_are_refused() -> Check {
        let project = resistors()?;
        let mut mesh = board_mesh(&project, MeshLayers::default())?;
        if let Some(p) = mesh.parts.iter_mut().find(|p| p.kind == PartKind::Board) {
            p.positions[0][0] = f64::NAN;
        }
        let err = solidcraft_board(&project, &mesh, SolidcraftLayers::default())
            .err()
            .ok_or("a NaN coordinate was accepted")?;
        assert!(err.contains("Board"), "{err}");
        Ok(())
    }

    #[test]
    fn clashing_part_names_get_a_suffix() {
        let mut taken = BTreeSet::new();
        taken.insert("Board".to_string());
        let mut parts = BTreeSet::new();
        assert_eq!(unique_name("Board", &taken, &mut parts), "Board (part)");
        assert_eq!(unique_name("Board", &taken, &mut parts), "Board (part 2)");
    }
}
