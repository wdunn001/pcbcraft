//! Artifact commands, stale detection and export helpers.
//!
//! Shared logic behind `artifact.generate`, `artifact.export`, `artifact.stale`
//! and `output_job.run`, so a job and a single artifact export the same way.
//!
//! # Rules
//! - Read-only commands (`command_is_read_only`) never change revision or history.
//! - Staleness is decided by output hash, so a no-op edit is not stale.
//!
//! # Dependencies
//! - `crates/io/src/generate.rs` for `generate`, `output_hash`, `write_generated`.
//! - `crates/doc/src/lib.rs` for `Artifact` and `Project`.
//!
//! # Do not
//! - Touch the UI or add per-kind writers here; those belong in the io crate.
use crate::Error;
use base64::Engine as _;
use pcbcraft_doc::{Artifact, Id, Project};
use pcbcraft_io::{GeneratedOutput, generate, output_hash, write_generated};
use serde_json::{Value, json};
use std::path::Path;

/// True for the artifact commands that only read, so the session runs them
/// without a commit.
///
/// # Rules
/// - Keep in step with `commands()` in `./lib.rs`.
///
/// # Dependencies
/// - `crates/engine/src/lib.rs` `Session::execute`.
///
/// # Do not
/// - List a command that mutates the project.
pub fn command_is_read_only(id: &str) -> bool {
    matches!(
        id,
        "artifact.generate" | "artifact.stale" | "library.export"
    )
}

/// An artifact is stale when it was never generated or its live output hash
/// differs from the stored one.
///
/// # Rules
/// - Regenerating at any revision clears it while the bytes are unchanged.
///
/// # Dependencies
/// - `crates/io/src/generate.rs` `generate`, `output_hash`.
///
/// # Do not
/// - Compare revisions alone; that flags edits that change nothing.
pub fn artifact_is_stale(p: &Project, artifact: &Artifact) -> Result<bool, Error> {
    if artifact.generated_at_revision.is_none() {
        return Ok(true);
    }
    let Some(stored) = artifact.generated_hash.as_ref() else {
        return Ok(true);
    };
    let live = output_hash(&generate(p, artifact).map_err(Error::Invalid)?);
    Ok(stored != &live)
}

/// Ids of stale artifacts in ascending order, optionally limited to `filter`.
///
/// # Rules
/// - A generate failure surfaces as an error rather than a silent skip.
///
/// # Dependencies
/// - `crates/engine/src/artifacts.rs` for `artifact_is_stale` in this file.
///
/// # Do not
/// - Mutate the project.
pub fn stale_artifact_ids(p: &Project, filter: Option<&[Id]>) -> Result<Vec<Id>, Error> {
    let mut stale = Vec::new();
    for artifact in &p.artifacts {
        if filter.is_some_and(|ids| !ids.contains(&artifact.id)) {
            continue;
        }
        if artifact_is_stale(p, artifact)? {
            stale.push(artifact.id);
        }
    }
    stale.sort_unstable();
    Ok(stale)
}

/// JSON body for `artifact.generate`: text as text, bytes as base64, and a
/// package as a list of named files.
///
/// # Rules
/// - Binary output is always base64 so the result stays valid JSON.
///
/// # Dependencies
/// - `crates/io/src/generate.rs` `GeneratedOutput`.
///
/// # Do not
/// - Write files from here.
pub fn format_generate_payload(output: &GeneratedOutput) -> Value {
    match output {
        GeneratedOutput::Text(text) => json!({
            "encoding": "text",
            "payload": text,
        }),
        GeneratedOutput::Bytes(bytes) => json!({
            "encoding": "base64",
            "payload": base64::engine::general_purpose::STANDARD.encode(bytes),
        }),
        GeneratedOutput::Files(map) => {
            let files: Vec<Value> = map
                .iter()
                .map(|(name, body)| {
                    json!({
                        "name": name,
                        "encoding": "text",
                        "payload": body,
                    })
                })
                .collect();
            json!({ "files": files })
        }
    }
}

/// Generates one artifact, writes it under `directory`, and records the
/// revision and hash on the artifact. Returns the names written and the hash.
///
/// # Rules
/// - Nothing is recorded when generation or writing fails.
/// - The caller commits, so the record change is one undo step.
///
/// # Dependencies
/// - `crates/io/src/generate.rs` `write_generated`.
///
/// # Do not
/// - Pass a `directory` the user did not choose.
pub fn export_artifact_record(
    p: &mut Project,
    id: Id,
    directory: &Path,
    record_revision: u64,
) -> Result<(Vec<String>, String), Error> {
    let index = p
        .artifacts
        .iter()
        .position(|a| a.id == id)
        .ok_or_else(|| Error::Invalid("object not found".into()))?;
    let artifact = p.artifacts[index].clone();
    let output = generate(p, &artifact).map_err(Error::Invalid)?;
    let hash = output_hash(&output);
    let written = write_generated(directory, &artifact, &output).map_err(Error::Invalid)?;
    let artifact = p
        .artifacts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| Error::Invalid("object not found".into()))?;
    artifact.generated_at_revision = Some(record_revision);
    artifact.generated_hash = Some(hash.clone());
    Ok((written, hash))
}
