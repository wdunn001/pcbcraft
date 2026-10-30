//! Export package commands: `package.list`, `package.save`, `package.duplicate`,
//! `package.remove`, `package.export`.
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
    GeneratedOutput, PackageOutput, availability, build_package, bundled_packages, find_package,
    output_hash,
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
            id: "package.duplicate",
            description: "Copy a bundled or project export package into the project under a new id so it can be edited. The bundled original stays read-only; give a new id and optionally a new name",
            input_schema: schema(
                json!({
                    "from":{"type":"string","minLength":1,"maxLength":32},
                    "id":{"type":"string","minLength":1,"maxLength":32},
                    "name":{"type":"string","minLength":1,"maxLength":64}
                }),
                &["from", "id"],
            ),
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
            id: "package.export",
            description: "Write one export package under a directory: a ZIP, a folder with manifest.json, or a single file, named from the package's templates. Give the package by id (bundled or saved in the project) or inline as a record. Runs each content through the existing writers; the archive is deterministic (manifest first, fixed stamps). Records the revision and content hash for a package given by id. Refuses a package whose writer is not written yet",
            input_schema: schema(
                json!({"id":{"type":"string","minLength":1,"maxLength":32},"package":{"type":"object"},"directory":{"type":"string","minLength":1,"maxLength":4096},"variant":{"type":"string","minLength":1,"maxLength":64}}),
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
struct DuplicateParams {
    from: String,
    id: String,
    #[serde(default)]
    name: Option<String>,
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
        "package.duplicate" => (|| {
            let a: DuplicateParams = parse(params)?;
            let mut def = find_package(p, &a.from)
                .ok_or_else(|| Error::Invalid(format!("no package has id {}", a.from)))?;
            if bundled_packages().iter().any(|b| b.id == a.id) {
                return Err(Error::Invalid(format!(
                    "{} is a bundled package id; pick a new id for the copy",
                    a.id
                )));
            }
            if p.packages.iter().any(|d| d.id == a.id) {
                return Err(Error::Invalid(format!(
                    "the project already saved a package {}",
                    a.id
                )));
            }
            def.id = a.id.clone();
            if let Some(name) = a.name {
                def.name = name;
            }
            availability(&def).map_err(Error::Invalid)?;
            let mut list = p.packages.clone();
            list.push(def.clone());
            validate_packages(&list).map_err(Error::Invalid)?;
            p.packages = list;
            Ok(json!({"from": a.from, "id": def.id, "name": def.name}))
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
            let out = build_package(p, &def, a.variant.as_deref()).map_err(Error::Invalid)?;
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
