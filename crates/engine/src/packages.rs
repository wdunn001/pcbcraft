//! Export package commands: `package.list`, `package.save`, `package.remove`,
//! `package.export`.
//!
//! Context: issue #224 (child of #214). A package is a `PackageDef` record; bundled ones
//! ship in `crates/io/data/packages.json` and a project saves its own in `Project::packages`.
//! `package.export` builds one through `pcbcraft_io::build_package` (existing writers and
//! the deterministic ZIP writer), writes it under a directory, and records the revision and
//! content hash in `Project::package_runs` like `artifact.export` does.
//!
//! Local rules:
//! - A package that is not available (a writer is still a stub) refuses with its reasons
//!   and writes nothing.
//! - `previous` names an earlier package ZIP on disk. It is read before anything is built,
//!   at most `MAX_PREVIOUS_PACKAGE_BYTES`; one that is missing, too large or not a package
//!   ZIP refuses the command and writes nothing. A program path never opens a dialog.
//! - A project package may not reuse a bundled id; duplicate a bundled one under a new id.
//!
//! # Dependencies
//! - crates/io/src/export_package.rs (templates, writer table, builder)
//! - crates/doc/src/package_def.rs (records)
//! - crates/engine/src/artifacts.rs (`command_is_read_only` lists `package.list`)
//!
//! # Do not
//! - Render files here; call `build_package`.
use super::*;
use pcbcraft_io::{
    GeneratedOutput, MAX_PREVIOUS_PACKAGE_BYTES, PackageOutput, availability,
    build_package_with_previous, bundled_packages, find_package, output_hash,
};
use std::path::Path;

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "package.list",
            description: "List the export packages: the bundled ones and the ones this project saved, each with who it is for, its contents, and whether it can run (a package whose writer is still a stub says why and which issue tracks it)",
            input_schema: schema(json!({}), &[]),
        },
        CommandSpec {
            id: "package.save",
            description: "Save an export package record in the project (a new one or a replacement of the project's own by id). The record names its contents by writer format, options and file name template; unknown formats and options, absolute or folder names and more than 64 contents are refused. A bundled id cannot be reused: duplicate it under a new id",
            input_schema: schema(json!({"package":{"type":"object"}}), &["package"]),
        },
        CommandSpec {
            id: "package.remove",
            description: "Remove a package the project saved; bundled packages cannot be removed",
            input_schema: schema(
                json!({"id":{"type":"string","minLength":1,"maxLength":32}}),
                &["id"],
            ),
        },
        CommandSpec {
            id: "package.duplicate",
            description: "Duplicate an existing package (bundled or saved in the project) under a new id, so it can be edited without changing the original. The new id must not already be in use or be a bundled id. If a name is not given, the duplicate is called '<old name> copy'",
            input_schema: schema(
                json!({"id":{"type":"string","minLength":1,"maxLength":32},"new_id":{"type":"string","minLength":1,"maxLength":32},"name":{"type":"string","minLength":1,"maxLength":64}}),
                &["id", "new_id"],
            ),
        },
        CommandSpec {
            id: "package.export",
            description: "Write one export package under a directory: a ZIP, a folder with manifest.json, or a single file, named from the package's templates. Give the package by id (bundled or saved in the project) or inline as a record. Runs each content through the existing writers; the archive is deterministic (manifest first, fixed stamps). Records the revision and content hash for a package given by id. Refuses a package whose writer is not written yet. An optional previous path names an earlier package ZIP; the revision notes then list the parts added and removed since it (at most 64 MiB, must be a package ZIP, else the command is refused and writes nothing)",
            input_schema: schema(
                json!({"id":{"type":"string","minLength":1,"maxLength":32},"package":{"type":"object"},"directory":{"type":"string","minLength":1,"maxLength":4096},"variant":{"type":"string","minLength":1,"maxLength":64},"previous":{"type":"string","minLength":1,"maxLength":4096}}),
                &["directory"],
            ),
        },
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveParams {
    package: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdParams {
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportParams {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    package: Option<Value>,
    directory: String,
    #[serde(default)]
    variant: Option<String>,
    #[serde(default)]
    previous: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DuplicateParams {
    id: String,
    new_id: String,
    #[serde(default)]
    name: Option<String>,
}

fn def_from(v: &Value) -> Result<PackageDef, Error> {
    serde_json::from_value(v.clone()).map_err(|e| Error::Invalid(e.to_string()))
}

fn listing(def: &PackageDef, source: &str) -> Result<Value, Error> {
    let a = availability(def).map_err(Error::Invalid)?;
    Ok(json!({
        "id": def.id,
        "name": def.name,
        "audience": def.audience,
        "container": def.container,
        "experimental": def.experimental,
        "source": source,
        "available": a.available,
        "reasons": a.reasons,
        "issues": a.issues,
        "contents": def.files.iter().map(|c| json!({
            "format": c.format, "kind": c.kind, "name": c.name, "options": c.options,
        })).collect::<Vec<_>>(),
    }))
}

/// Runs a package command, or returns `None` for any other command.
///
/// Why: keeps the package commands out of the 36k-line match in `lib.rs`.
///
/// # Rules
/// - `package.export` refuses to run read-only (it writes files).
///
/// # Dependencies
/// - crates/io/src/export_package.rs for every decision about names and writers.
///
/// # Do not
/// - Write a file before the whole package has been built.
pub(super) fn run(
    p: &mut Project,
    command: &str,
    params: &Value,
    record_revision: u64,
    mutating: bool,
) -> Option<Result<Value, Error>> {
    Some(match command {
        "package.list" => (|| {
            let mut out = Vec::new();
            for d in bundled_packages() {
                out.push(listing(d, "bundled")?);
            }
            for d in &p.packages {
                out.push(listing(d, "project")?);
            }
            Ok(json!({"packages": out}))
        })(),
        "package.save" => (|| {
            let a: SaveParams = parse(params)?;
            let def = def_from(&a.package)?;
            availability(&def).map_err(Error::Invalid)?;
            if bundled_packages().iter().any(|b| b.id == def.id) {
                return Err(Error::Invalid(format!(
                    "{} is a bundled package id; duplicate it under a new id",
                    def.id
                )));
            }
            let mut list = p.packages.clone();
            let replaced = match list.iter().position(|d| d.id == def.id) {
                Some(i) => {
                    list[i] = def.clone();
                    true
                }
                None => {
                    list.push(def.clone());
                    false
                }
            };
            validate_packages(&list).map_err(Error::Invalid)?;
            p.packages = list;
            Ok(json!({"id": def.id, "replaced": replaced}))
        })(),
        "package.remove" => (|| {
            let a: IdParams = parse(params)?;
            let before = p.packages.len();
            p.packages.retain(|d| d.id != a.id);
            if p.packages.len() == before {
                return Err(Error::Invalid(format!(
                    "the project saved no package {}",
                    a.id
                )));
            }
            p.package_runs.retain(|r| r.package != a.id);
            Ok(json!({"id": a.id}))
        })(),
        "package.duplicate" => (|| {
            let a: DuplicateParams = parse(params)?;
            let source = find_package(p, &a.id)
                .ok_or_else(|| Error::Invalid(format!("no package has id {}", a.id)))?;
            if bundled_packages().iter().any(|b| b.id == a.new_id) {
                return Err(Error::Invalid(format!(
                    "{} is a bundled package id; duplicate it under a new id",
                    a.new_id
                )));
            }
            let mut dup = source.clone();
            dup.id = a.new_id.clone();
            dup.name = a.name.unwrap_or_else(|| format!("{} copy", source.name));
            let mut list = p.packages.clone();
            list.push(dup);
            validate_packages(&list).map_err(Error::Invalid)?;
            p.packages = list;
            Ok(json!({"id": a.new_id, "source": a.id}))
        })(),
        "package.export" => (|| {
            if !mutating {
                return Err(Error::Invalid(
                    "package.export cannot run as a read-only command".into(),
                ));
            }
            let a: ExportParams = parse(params)?;
            if a.directory.is_empty() || a.directory.len() > 4096 {
                return Err(Error::Invalid("output directory is out of range".into()));
            }
            let def = match (&a.id, &a.package) {
                (Some(id), None) => find_package(p, id)
                    .ok_or_else(|| Error::Invalid(format!("no package has id {id}")))?,
                (None, Some(v)) => def_from(v)?,
                _ => {
                    return Err(Error::Invalid(
                        "give either id or package, not both and not neither".into(),
                    ));
                }
            };
            let previous = match &a.previous {
                Some(path) => Some(read_previous_package(path)?),
                None => None,
            };
            let out =
                build_package_with_previous(p, &def, a.variant.as_deref(), previous.as_deref())
                    .map_err(Error::Invalid)?;
            for f in &out.files {
                pcbcraft_io::validate_fabrication_package_file("", &f.0, &f.1)
                    .map_err(Error::Invalid)?;
            }
            let hash = write_output(&out, Path::new(&a.directory))?;

            if a.id.is_some() {
                p.package_runs.retain(|r| r.package != def.id);
                p.package_runs.push(PackageRun {
                    package: def.id.clone(),
                    revision: record_revision,
                    hash: hash.clone(),
                });
                p.package_runs.sort_by(|x, y| x.package.cmp(&y.package));
            }
            Ok(json!({
                "id": def.id,
                "directory": a.directory,
                "written": out.files.iter().map(|(n, _)| n).collect::<Vec<_>>(),
                "files": out.entries,
                "hash": hash,
                "revision": record_revision,
            }))
        })(),
        _ => return None,
    })
}

/// Reads an earlier package ZIP for the revision notes.
///
/// # Rules
/// - Refuses an empty or over-long path, a path that is not a file, and a file over
///   `MAX_PREVIOUS_PACKAGE_BYTES`, before reading it.
///
/// # Do not
/// - Open a dialog; the caller names the file.
fn read_previous_package(path: &str) -> Result<Vec<u8>, Error> {
    if path.is_empty() || path.len() > 4096 {
        return Err(Error::Invalid(
            "previous package path is out of range".into(),
        ));
    }
    let meta = std::fs::metadata(path)
        .map_err(|e| Error::Invalid(format!("previous package {path}: {e}")))?;
    if !meta.is_file() {
        return Err(Error::Invalid(format!(
            "previous package {path} is not a file"
        )));
    }
    if meta.len() > MAX_PREVIOUS_PACKAGE_BYTES as u64 {
        return Err(Error::Invalid(format!(
            "previous package {path} is over {MAX_PREVIOUS_PACKAGE_BYTES} bytes"
        )));
    }
    std::fs::read(path).map_err(|e| Error::Invalid(format!("previous package {path}: {e}")))
}

/// Writes the built files under `directory` and returns the content hash.
fn write_output(out: &PackageOutput, directory: &Path) -> Result<String, Error> {
    std::fs::create_dir_all(directory).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut all = Vec::new();
    for (name, bytes) in &out.files {
        all.extend_from_slice(name.as_bytes());
        all.push(0);
        all.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        all.extend_from_slice(bytes);
    }
    for (name, bytes) in &out.files {
        pcbcraft_io::atomic_write(&directory.join(name), bytes).map_err(Error::Invalid)?;
    }
    Ok(output_hash(&GeneratedOutput::Bytes(all)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fab_prototype_export_produces_files_and_hash() -> Result<(), Error> {
        let mut s = Session::sample()?;
        let directory =
            std::env::temp_dir().join(format!("pcbcraft-pkg-fabproto-{}", std::process::id()));
        std::fs::create_dir_all(&directory).map_err(|e| Error::Invalid(e.to_string()))?;
        let result = s.execute(
            "package.export",
            json!({
                "id": "fab_prototype",
                "directory": directory.to_string_lossy()
            }),
            None,
        )?;
        let written: Vec<String> = result["result"]["written"]
            .as_array()
            .ok_or_else(|| Error::Invalid("no written list".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        assert!(!written.is_empty(), "fab_prototype export wrote no files");
        assert!(
            written.iter().any(|n| n.ends_with(".zip")),
            "expected a ZIP archive in the written list"
        );
        let hash = result["result"]["hash"]
            .as_str()
            .ok_or_else(|| Error::Invalid("missing hash in package.export result".into()))?;
        assert_eq!(hash.len(), 64, "hash must be a 64-character hex string");
        Ok(())
    }

    fn zip_in(directory: &Path) -> Result<std::path::PathBuf, Error> {
        std::fs::read_dir(directory)
            .map_err(|e| Error::Invalid(e.to_string()))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "zip"))
            .ok_or_else(|| Error::Invalid("no zip was written".into()))
    }

    #[test]
    fn export_with_a_previous_package_writes_notes_that_compare_with_it() -> Result<(), Error> {
        let mut s = Session::sample()?;
        let base = std::env::temp_dir().join(format!("pcbcraft-pkg-prev-{}", std::process::id()));
        let (first, second) = (base.join("first"), base.join("second"));
        s.execute(
            "package.export",
            json!({"id": "fab_complete", "directory": first.to_string_lossy()}),
            None,
        )?;
        let previous = zip_in(&first)?;
        s.execute(
            "package.export",
            json!({
                "id": "fab_complete",
                "directory": second.to_string_lossy(),
                "previous": previous.to_string_lossy()
            }),
            None,
        )?;
        let entries = pcbcraft_io::package_stored_zip_entries(
            &std::fs::read(zip_in(&second)?).map_err(|e| Error::Invalid(e.to_string()))?,
        )
        .map_err(Error::Invalid)?;
        let notes = entries
            .iter()
            .find(|(name, _)| name.contains("-revision-notes-"))
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
            .ok_or_else(|| Error::Invalid("the package holds revision notes".into()))?;
        assert!(
            !notes.contains("no previous export was supplied"),
            "the notes compare with the previous package: {notes}"
        );
        let _ = std::fs::remove_dir_all(&base);
        Ok(())
    }

    #[test]
    fn export_refuses_a_previous_file_that_is_not_a_package_and_changes_nothing()
    -> Result<(), Error> {
        let mut s = Session::sample()?;
        let base = std::env::temp_dir().join(format!("pcbcraft-pkg-bad-{}", std::process::id()));
        std::fs::create_dir_all(&base).map_err(|e| Error::Invalid(e.to_string()))?;
        let bad = base.join("notes.zip");
        std::fs::write(&bad, b"not a package").map_err(|e| Error::Invalid(e.to_string()))?;
        let out = base.join("out");
        let (runs, revision) = (s.project.package_runs.clone(), s.revision);
        let refused = s.execute(
            "package.export",
            json!({
                "id": "fab_complete",
                "directory": out.to_string_lossy(),
                "previous": bad.to_string_lossy()
            }),
            None,
        );
        assert!(refused.is_err(), "a file that is not a package is refused");
        assert!(!out.exists(), "a refused export writes nothing");
        assert_eq!(s.project.package_runs, runs, "no run is recorded");
        assert_eq!(s.revision, revision, "the revision does not move");
        let missing = s.execute(
            "package.export",
            json!({
                "id": "fab_complete",
                "directory": out.to_string_lossy(),
                "previous": base.join("gone.zip").to_string_lossy()
            }),
            None,
        );
        assert!(missing.is_err(), "a missing previous package is refused");
        let _ = std::fs::remove_dir_all(&base);
        Ok(())
    }
}
