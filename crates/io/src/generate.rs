//! Artifact generation dispatch and content hashing.
//!
//! Turns an `Artifact` record into bytes by calling the existing writers in
//! this crate, and hashes the result so the engine can tell a stale output
//! from a current one. Pure functions except `write_generated`, which is the
//! only place here that touches disk.
//!
//! # Rules
//! - Kinds without a writer return a plain refusal naming the tracking issue.
//! - Artifact paths and generated file names never escape the output folder.
//!
//! # Dependencies
//! - `crates/doc/src/lib.rs` for `Artifact`, `Project` and the kind list.
//! - `crates/io/src/lib.rs` for the writers and `atomic_write`.
//!
//! # Do not
//! - Add a per-kind branch in the engine; new kinds are added to `generate`.
use pcbcraft_doc::{Artifact, Project};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Component, Path};

use crate::{
    bom_csv, cmp_text, drill_chart_csv, dxf_board, gencad_text, gerber_files, hpgl_plot,
    hyperlynx_csv, idf_board, ipc356_netlist, ipc2581_text, netlist_text, odb_package,
    fab_drawing_pdf, pdf_document, placement_text, plot_csv, postscript, report_text,
    specctra_dsn, stackup_text,
    stats_text, step_board, svg, vrml_board,
};

/// What one artifact produces: one text file, one binary file, or a named set
/// of files (a package). A single type lets the engine hash, encode and write
/// every kind the same way.
///
/// # Rules
/// - `Files` keys are relative names; the map order is the hash order.
///
/// # Dependencies
/// - `crates/engine/src/artifacts.rs` encodes it for `artifact.generate`.
///
/// # Do not
/// - Put absolute paths or `..` segments in `Files` keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeneratedOutput {
    /// One text file.
    Text(String),
    /// One binary file.
    Bytes(Vec<u8>),
    /// Several text files keyed by relative file name.
    Files(BTreeMap<String, String>),
}

/// Gives a project saved before artifacts existed one artifact per legacy
/// output job entry, so old files keep exporting the same outputs.
///
/// # Rules
/// - Runs only when `artifacts` is empty and `output_jobs` is not.
/// - Ids start above the highest id in use.
///
/// # Dependencies
/// - `crates/doc/src/lib.rs` `Project::migrate_output_jobs_to_artifacts`.
///
/// # Do not
/// - Call it after validation; `load` runs it before `validate`.
pub fn migrate_loaded_project(project: &mut Project) -> Result<(), String> {
    if project.artifacts.is_empty() && !project.output_jobs.is_empty() {
        let mut next = project.ids().max().unwrap_or(0).saturating_add(1);
        project.migrate_output_jobs_to_artifacts(&mut next)?;
    }
    Ok(())
}

/// SHA-256 of the canonical bytes of an output, as 64 lowercase hex digits.
/// The engine compares it with the stored hash to decide staleness.
///
/// # Rules
/// - Identical bytes always give the same hash; `Files` hash in name order.
///
/// # Dependencies
/// - `crates/engine/src/artifacts.rs` `artifact_is_stale`.
///
/// # Do not
/// - Hash anything that varies between runs, such as timestamps.
pub fn output_hash(output: &GeneratedOutput) -> String {
    let mut hasher = Sha256::new();
    match output {
        GeneratedOutput::Text(text) => hasher.update(text.as_bytes()),
        GeneratedOutput::Bytes(bytes) => hasher.update(bytes.as_slice()),
        GeneratedOutput::Files(map) => {
            for (name, body) in map {
                hasher.update(name.as_bytes());
                hasher.update(body.as_bytes());
            }
        }
    }
    let digest = hasher.finalize();
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

fn settings_view(settings: &serde_json::Value) -> &str {
    settings
        .get("view")
        .and_then(|v| v.as_str())
        .unwrap_or("pcb")
}

fn copper_gerber_name(layer: &str) -> Option<&'static str> {
    match layer {
        "F.Cu" => Some("pcb-F.Cu.gbr"),
        "B.Cu" => Some("pcb-B.Cu.gbr"),
        _ => None,
    }
}

fn package_from_path(p: &Project, artifact: &Artifact) -> Result<GeneratedOutput, String> {
    let path = artifact.path.as_str();
    if path == "gerber/" || path.starts_with("gerber/") {
        let files = gerber_files(p)?;
        if path == "gerber/" {
            return Ok(GeneratedOutput::Files(files));
        }
        let name = path
            .strip_prefix("gerber/")
            .filter(|n| !n.is_empty() && !n.contains('/'))
            .ok_or_else(|| "gerber package path must be gerber/ or gerber/<file>".to_string())?;
        let text = files
            .get(name)
            .ok_or_else(|| format!("gerber export has no layer named {name}"))?
            .clone();
        return Ok(GeneratedOutput::Text(text));
    }
    if path.ends_with(".odb.zip") {
        return Ok(GeneratedOutput::Bytes(odb_package(p)?));
    }
    if path.ends_with(".xml") {
        return Ok(GeneratedOutput::Text(ipc2581_text(p)?));
    }
    let text_output = |text: String| Ok(GeneratedOutput::Text(text));
    match path {
        "pcb-netlist.net" => text_output(netlist_text(p)?),
        "bom.csv" => text_output(bom_csv(p, &[], None)?),
        "placement.txt" => text_output(placement_text(p)?),
        "board-drill-tools.csv" => text_output(drill_chart_csv(p)?),
        "board.pdf" => Ok(GeneratedOutput::Bytes(pdf_document(p, "pcb")?)),
        "board.svg" => text_output(svg(p, "pcb")?),
        "pcb.dxf" => text_output(dxf_board(p)?),
        "pcb.cmp" => text_output(cmp_text(p)?),
        "board.idf" => text_output(idf_board(p)?),
        "board.wrl" => text_output(vrml_board(p)?),
        "board.step" => text_output(step_board(p)?),
        "pcb.dsn" => text_output(specctra_dsn(p)?),
        "test.brd" => text_output(ipc356_netlist(p)?),
        "board.gencad" => text_output(gencad_text(p)?),
        "netlist.csv" => text_output(hyperlynx_csv(p)?),
        "plot-report.csv" => text_output(plot_csv(p)?),
        "board-report.txt" => text_output(report_text(p)?),
        "pcb.ps" => text_output(postscript(p, "pcb")?),
        "board.hpgl" => text_output(hpgl_plot(p, "pcb")?),
        "board-stackup.txt" => text_output(stackup_text(p)?),
        "board-stats.csv" => text_output(stats_text(p)?),
        other => Err(format!(
            "package output for {other} is not available yet; see issue #322"
        )),
    }
}

/// Produces the output for one artifact by dispatching on its `kind` to the
/// existing writers. Settings pick the view where a writer needs one.
///
/// # Rules
/// - Refuses an invalid artifact before any writer runs.
/// - Kinds without a writer yet refuse with the issue that tracks them.
///
/// # Dependencies
/// - `crates/doc/src/lib.rs` `Artifact::valid`.
///
/// # Do not
/// - Write to disk here; use `write_generated`.
pub fn generate(p: &Project, artifact: &Artifact) -> Result<GeneratedOutput, String> {
    if !artifact.valid() {
        return Err("artifact record is invalid".into());
    }
    match artifact.kind.as_str() {
        "bom" => Ok(GeneratedOutput::Text(bom_csv(p, &[], None)?)),
        "placement" => Ok(GeneratedOutput::Text(placement_text(p)?)),
        "drill_map" => Ok(GeneratedOutput::Text(drill_chart_csv(p)?)),
        "fab_drawing" => Ok(GeneratedOutput::Bytes(fab_drawing_pdf(p)?)),
        "assembly_drawing" => Ok(GeneratedOutput::Bytes(pdf_document(p, "schematic")?)),
        "raster_image" => Ok(GeneratedOutput::Text(svg(
            p,
            settings_view(&artifact.settings),
        )?)),
        "package" => package_from_path(p, artifact),
        "copper_plot" => {
            let layer = artifact
                .layers
                .first()
                .map(|s| s.as_str())
                .unwrap_or("F.Cu");
            let file = copper_gerber_name(layer).ok_or_else(|| {
                format!("copper plot for layer {layer} is not available yet; see issue #325")
            })?;
            let files = gerber_files(p)?;
            let text = files
                .get(file)
                .ok_or_else(|| format!("gerber export has no layer named {file}"))?
                .clone();
            Ok(GeneratedOutput::Text(text))
        }
        "mask_plot" | "silk_plot" | "paste_plot" | "profile_plot" => {
            Err("layer plot artifacts are not available yet; see issue #325".into())
        }
        "drill_plated" | "drill_unplated" => {
            Err("split drill artifacts are not available yet; see issue #276".into())
        }
        "isolation_gcode" => Err("isolation gcode is not available yet; see issue #329".into()),
        "drill_gcode" => Err("drill gcode is not available yet; see issue #330".into()),
        "outline_gcode" => Err("outline gcode is not available yet; see issue #331".into()),
        "stencil" => Err("stencil output is not available yet; see issue #325".into()),
        "laser_path" => Err("laser path output is not available yet; see issue #358".into()),
        other => Err(format!("unknown artifact kind {other}")),
    }
}

/// Writes a generated output under `directory`, at the artifact's relative
/// path (or inside it for a package), and returns the relative names written.
///
/// # Rules
/// - Rejects an empty directory and any file name that escapes it.
/// - Writes each file atomically and creates parent folders.
///
/// # Dependencies
/// - `crates/io/src/lib.rs` `atomic_write`, which refuses on WASM.
///
/// # Do not
/// - Record staleness here; the engine does that in the same undo step.
pub fn write_generated(
    directory: &Path,
    artifact: &Artifact,
    output: &GeneratedOutput,
) -> Result<Vec<String>, String> {
    if directory.as_os_str().is_empty() {
        return Err("output directory is empty".into());
    }
    match output {
        GeneratedOutput::Text(text) => {
            let path = directory.join(&artifact.path);
            write_parent(&path)?;
            crate::atomic_write(&path, text.as_bytes())?;
            Ok(vec![artifact.path.clone()])
        }
        GeneratedOutput::Bytes(bytes) => {
            let path = directory.join(&artifact.path);
            write_parent(&path)?;
            crate::atomic_write(&path, bytes)?;
            Ok(vec![artifact.path.clone()])
        }
        GeneratedOutput::Files(map) => {
            let base = if artifact.path.ends_with('/') {
                directory.join(&artifact.path)
            } else if artifact.path.is_empty() {
                directory.to_path_buf()
            } else {
                directory.join(&artifact.path)
            };
            let mut written = Vec::new();
            for (name, body) in map {
                if name.contains("..")
                    || Path::new(name)
                        .components()
                        .any(|c| c == Component::ParentDir)
                {
                    return Err("generated file name escapes the output folder".into());
                }
                let path = base.join(name);
                write_parent(&path)?;
                crate::atomic_write(&path, body.as_bytes())?;
                let rel = if artifact.path.ends_with('/') {
                    format!("{}{name}", artifact.path)
                } else if artifact.path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", artifact.path, name)
                };
                written.push(rel);
            }
            written.sort();
            Ok(written)
        }
    }
}

fn write_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod generate_tests {
    use super::*;
    use pcbcraft_doc::Project;

    #[test]
    fn output_hash_is_stable_for_identical_bytes() -> Result<(), String> {
        let a = GeneratedOutput::Text("same".into());
        let b = GeneratedOutput::Text("same".into());
        assert_eq!(output_hash(&a), output_hash(&b));
        let c = GeneratedOutput::Text("different".into());
        assert_ne!(output_hash(&a), output_hash(&c));
        let mut map = BTreeMap::new();
        map.insert("a.gbr".into(), "1".into());
        map.insert("b.gbr".into(), "2".into());
        assert_eq!(
            output_hash(&GeneratedOutput::Files(map.clone())),
            output_hash(&GeneratedOutput::Files(map))
        );
        Ok(())
    }

    #[test]
    fn generate_package_gerbers_matches_gerber_files() -> Result<(), String> {
        let project = Project::default();
        let artifact = Artifact {
            id: 1,
            name: "gerbers".into(),
            kind: "package".into(),
            layers: vec![],
            settings: serde_json::json!({}),
            machine: None,
            path: "gerber/".into(),
            generated_at_revision: None,
            generated_hash: None,
            job: None,
        };
        let out = generate(&project, &artifact)?;
        let GeneratedOutput::Files(map) = out else {
            return Err("expected a file map".into());
        };
        assert_eq!(map, gerber_files(&project)?);
        Ok(())
    }
}
