//! Companion CAD project files as a 3D model source and sink until the shared
//! container format lands everywhere. Geometry rides on STEP sidecars; the
//! `.solidcraft` stub carries stable body ids and the pad-plane frame in
//! [`COMPANION_METADATA_KEY`].
//!
//! # Rules
//! - Units are millimetres, Z up, the same frame as [`crate::board_mesh`].
//! - A model load resolves a STEP byte stream, then uses [`crate::step_read`].
//! - Cache keys use the SHA-256 of the project file bytes, not its mtime.
//!
//! # Dependencies
//! - `./step_read.rs`, `./model_load.rs`
//!
//! # Do not
//! - Do not link the CAD app's crates; exchange is documented data only.

use pcbcraft_library::{FootprintDefinition, Model3D};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Metadata key shared with the interop note.
pub const COMPANION_METADATA_KEY: &str = "org.pcbcraft";
/// Schema stamp under [`COMPANION_METADATA_KEY`].
pub const COMPANION_SCHEMA: &str = "pcbcraft-interop/1";
/// Oldest SolidCraft design format number used in stubs.
pub const COMPANION_FORMAT: u32 = 22;

/// True when `path` names a companion CAD project file.
pub fn is_companion_project_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("solidcraft"))
}

/// Hex SHA-256 of `bytes`.
pub fn content_sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push(nibble(byte >> 4));
        hex.push(nibble(byte & 0x0f));
    }
    hex
}

fn nibble(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}

/// Parsed stub metadata from a companion project file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompanionStub {
    pub step_sidecar: Option<String>,
    pub body_id: Option<String>,
    pub cad_part: Option<String>,
    pub content_sha256: Option<String>,
}

/// Read the sidecar hints from a companion project JSON stub.
pub fn parse_stub(bytes: &[u8]) -> Result<CompanionStub, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| "The companion project file is not valid JSON.".to_string())?;
    let meta = value
        .get("metadata")
        .and_then(|m| m.get(COMPANION_METADATA_KEY));
    let Some(meta) = meta.and_then(Value::as_object) else {
        return Ok(CompanionStub {
            step_sidecar: None,
            body_id: None,
            cad_part: None,
            content_sha256: None,
        });
    };
    let text = |key: &str| meta.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(CompanionStub {
        step_sidecar: text("step_sidecar"),
        body_id: text("body_id"),
        cad_part: text("cad_part"),
        content_sha256: text("content_sha256"),
    })
}

fn sidecar_candidates(project_path: &Path, stub: &CompanionStub) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(name) = &stub.step_sidecar {
        if Path::new(name).is_absolute() {
            out.push(PathBuf::from(name));
        } else if let Some(dir) = project_path.parent() {
            out.push(dir.join(name));
        }
    }
    if let Some(stem) = project_path.file_stem().and_then(|s| s.to_str()) {
        if let Some(dir) = project_path.parent() {
            out.push(dir.join(format!("{stem}.step")));
            out.push(dir.join(format!("{stem}.solidcraft.step")));
        }
    }
    out
}

fn cache_step_path(cache_root: Option<&Path>, hash: &str) -> Option<PathBuf> {
    cache_root.map(|root| root.join("cad-cache").join(format!("{hash}.step")))
}

/// Resolve STEP bytes for a companion project path.
///
/// `project_bytes` are the on-disk `.solidcraft` contents when already read.
/// `cache_root` is usually the project's `.pcbcraft` folder.
pub fn resolve_step_bytes(
    project_path: &str,
    project_bytes: &[u8],
    cad_part: Option<&str>,
    cache_root: Option<&Path>,
) -> Result<Vec<u8>, String> {
    if project_bytes.is_empty() {
        return Err("The companion project file is empty.".into());
    }
    if project_bytes.len() > crate::model_load::MAX_MODEL_BYTES {
        return Err(format!(
            "The companion project file is over the {} MB limit.",
            crate::model_load::MAX_MODEL_BYTES / (1024 * 1024)
        ));
    }
    let stub = parse_stub(project_bytes)?;
    let hash = content_sha256_hex(project_bytes);
    if stub
        .content_sha256
        .as_ref()
        .is_some_and(|want| want != &hash)
    {
        return Err(
            "The companion project file changed since its STEP sidecar was written.".into(),
        );
    }
    let _part = cad_part.or(stub.cad_part.as_deref());
    let path = Path::new(project_path);
    for candidate in sidecar_candidates(path, &stub) {
        if let Ok(bytes) = std::fs::read(&candidate) {
            if !bytes.is_empty() {
                return Ok(bytes);
            }
        }
    }
    if let Some(cached) = cache_step_path(cache_root, &hash)
        && let Ok(bytes) = std::fs::read(&cached)
        && !bytes.is_empty()
    {
        return Ok(bytes);
    }
    Err(
        "This companion project has no STEP sidecar yet. Export or reimport from the Library tab."
            .into(),
    )
}

/// Cache key fragment for a companion project file from its content hash.
pub fn companion_cache_token(content_hash: &str, cad_part: Option<&str>) -> String {
    match cad_part.filter(|p| !p.is_empty()) {
        Some(part) => format!("{content_hash}::{part}"),
        None => content_hash.to_string(),
    }
}

/// Write a footprint package body as STEP plus a companion project stub.
///
/// `body_id` stays stable across reimports so alignment transforms survive edits
/// in the CAD app.
pub fn write_footprint_body(
    def: &FootprintDefinition,
    directory: &Path,
    base_name: &str,
    body_id: &str,
    height_mm: f64,
) -> Result<(PathBuf, PathBuf), String> {
    if base_name.is_empty() || base_name.len() > 128 {
        return Err("The export name must be 1 to 128 characters.".into());
    }
    if body_id.is_empty() || body_id.len() > 128 {
        return Err("The body id must be 1 to 128 characters.".into());
    }
    std::fs::create_dir_all(directory)
        .map_err(|_| "The export folder could not be created.".to_string())?;
    let step_name = format!("{base_name}.step");
    let project_name = format!("{base_name}.solidcraft");
    let step_path = directory.join(&step_name);
    let project_path = directory.join(&project_name);
    let step_text = crate::step_footprint_package(def, height_mm)?;
    std::fs::write(&step_path, step_text.as_bytes())
        .map_err(|_| "The STEP sidecar could not be written.".to_string())?;
    let meta = json!({
        "schema": COMPANION_SCHEMA,
        "kind": "footprint_body",
        "frame": "footprint_pad_plane",
        "body_id": body_id,
        "step_sidecar": step_name,
        "cad_part": base_name,
    });
    let hash = content_sha256_hex(
        serde_json::to_vec(&json!({
            "format": COMPANION_FORMAT,
            "units": "mm",
            "metadata": { COMPANION_METADATA_KEY: meta.clone() },
        }))
        .map_err(|_| "The companion project stub could not be encoded.".to_string())?
        .as_slice(),
    );
    let doc = json!({
        "format": COMPANION_FORMAT,
        "units": "mm",
        "metadata": {
            COMPANION_METADATA_KEY: {
                "schema": COMPANION_SCHEMA,
                "kind": "footprint_body",
                "frame": "footprint_pad_plane",
                "body_id": body_id,
                "step_sidecar": step_name,
                "cad_part": base_name,
                "content_sha256": hash,
            },
        },
    });
    let project_bytes = serde_json::to_vec_pretty(&doc)
        .map_err(|_| "The companion project stub could not be encoded.".to_string())?;
    std::fs::write(&project_path, &project_bytes)
        .map_err(|_| "The companion project file could not be written.".to_string())?;
    Ok((project_path, step_path))
}

/// Write a board-level STEP export and a companion stub that points at it.
pub fn write_board_sink(
    project: &pcbcraft_doc::Project,
    directory: &Path,
    base_name: &str,
) -> Result<(PathBuf, PathBuf), String> {
    if base_name.is_empty() || base_name.len() > 128 {
        return Err("The export name must be 1 to 128 characters.".into());
    }
    std::fs::create_dir_all(directory)
        .map_err(|_| "The export folder could not be created.".to_string())?;
    let step_name = format!("{base_name}.step");
    let project_name = format!("{base_name}.solidcraft");
    let step_path = directory.join(&step_name);
    let project_path = directory.join(&project_name);
    let step_text = crate::step_board(project)?;
    std::fs::write(&step_path, step_text.as_bytes())
        .map_err(|_| "The board STEP file could not be written.".to_string())?;
    let mut bodies: Map<String, Value> = Map::new();
    bodies.insert(
        "Board".into(),
        json!({"kind": "board", "body_id": "board"}),
    );
    for fp in &project.board.footprints {
        let reference = project
            .component(fp.component)
            .map(|c| c.reference.clone())
            .unwrap_or_else(|| format!("footprint {}", fp.id));
        bodies.insert(
            reference.clone(),
            json!({"kind": "component", "body_id": reference}),
        );
    }
    let doc = json!({
        "format": COMPANION_FORMAT,
        "units": "mm",
        "metadata": {
            COMPANION_METADATA_KEY: {
                "schema": COMPANION_SCHEMA,
                "kind": "board_assembly",
                "step_sidecar": step_name,
                "bodies": bodies,
            },
        },
    });
    let project_bytes = serde_json::to_vec_pretty(&doc)
        .map_err(|_| "The companion project stub could not be encoded.".to_string())?;
    let hash = content_sha256_hex(&project_bytes);
    let doc = json!({
        "format": COMPANION_FORMAT,
        "units": "mm",
        "metadata": {
            COMPANION_METADATA_KEY: {
                "schema": COMPANION_SCHEMA,
                "kind": "board_assembly",
                "step_sidecar": step_name,
                "content_sha256": hash,
                "bodies": bodies,
            },
        },
    });
    let project_bytes = serde_json::to_vec_pretty(&doc)
        .map_err(|_| "The companion project stub could not be encoded.".to_string())?;
    std::fs::write(&project_path, &project_bytes)
        .map_err(|_| "The companion project file could not be written.".to_string())?;
    Ok((project_path, step_path))
}

/// Build a [`Model3D`] that points at a companion project and optional part name.
pub fn model_reference(project_path: &str, body_id: &str, cad_part: Option<&str>) -> Model3D {
    let part = cad_part.unwrap_or("");
    let raw_source = if part.is_empty() {
        format!(
            "(model \"{}\" (companion_body \"{}\"))",
            project_path, body_id
        )
    } else {
        format!(
            "(model \"{}\" (companion_body \"{}\") (cad_part \"{}\"))",
            project_path, body_id, part
        )
    };
    Model3D {
        path: project_path.to_string(),
        offset: [0.0; 3],
        scale: [1.0; 3],
        rotate: [0.0; 3],
        raw_source,
        cad_part: cad_part.filter(|p| !p.is_empty()).map(str::to_owned),
        body_id: Some(body_id.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_sidecar_is_found_next_to_the_project() {
        let dir = tempfile::tempdir().expect("tempdir");
        let step_path = dir.path().join("part.step");
        std::fs::write(&step_path, include_bytes!("../tests/fixtures/box.step")).unwrap();
        let project_path = dir.path().join("part.solidcraft");
        let stub = json!({
            "format": 22,
            "metadata": {
                COMPANION_METADATA_KEY: {
                    "schema": COMPANION_SCHEMA,
                    "step_sidecar": "part.step",
                    "body_id": "body-1",
                }
            }
        });
        let bytes = serde_json::to_vec(&stub).unwrap();
        std::fs::write(&project_path, &bytes).unwrap();
        let resolved = resolve_step_bytes(
            project_path.to_str().unwrap(),
            &bytes,
            None,
            None,
        )
        .expect("step sidecar");
        assert!(resolved.starts_with(b"ISO-10303-21"));
    }
}
