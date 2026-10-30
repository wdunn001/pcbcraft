//! Turn a footprint's model reference into a path the loader can open on this
//! machine. Library-table nicknames, KiCad-style `${…_3DMODEL_DIR}/…` prefixes,
//! and paths relative to the saved design file are handled here.
use pcbcraft_doc::Project;
use std::path::{Path, PathBuf};

/// Rewrite a library path whose first segment names a project library table
/// entry onto the folder that entry points at.
pub fn resolve_library_path(project: &Project, path: &str) -> String {
    if let Some((head, rest)) = path.split_once(['/', '\\'])
        && let Some(link) = project.library_table.iter().find(|l| l.name == head)
    {
        let sep = if link.path.ends_with('/') || link.path.ends_with('\\') {
            ""
        } else {
            "/"
        };
        return format!("{}{sep}{rest}", link.path);
    }
    path.to_string()
}

fn path_is_absolute(path: &str) -> bool {
    let p = Path::new(path);
    p.is_absolute()
        || (cfg!(windows)
            && path.as_bytes().get(1) == Some(&b':')
            && path.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic()))
}

/// Drop a KiCad `${KICAD…_3DMODEL_DIR}/` prefix so the tail can be joined
/// onto a library folder.
fn strip_kicad_3dmodel_prefix(path: &str) -> &str {
    if path.starts_with("${KICAD") {
        if let Some(rest) = path.strip_prefix("${KICAD") {
            if let Some(tail) = rest.split_once("}/") {
                return tail.1;
            }
        }
    }
    path
}

fn existing_file(path: PathBuf) -> Option<String> {
    if path.is_file() {
        return path.to_str().map(str::to_owned);
    }
    None
}

fn candidates_for_library_root(root: &Path, relative: &str) -> Vec<PathBuf> {
    let mut out = vec![root.join(relative)];
    if let Some(parent) = root.parent() {
        out.push(parent.join(relative));
    }
    if root.extension().is_some_and(|e| e == "pretty") {
        if let Some(parent) = root.parent() {
            out.push(parent.join(relative));
        }
    }
    out
}

/// Resolve `model_path` from a footprint onto a concrete file path when
/// possible. `footprint_library` is the `library` field on the embedded
/// footprint copy, when present. `design_dir` is the folder that holds the
/// saved design file.
pub fn resolve_model_path(
    project: &Project,
    model_path: &str,
    footprint_library: Option<&str>,
    design_dir: Option<&Path>,
) -> String {
    let table_resolved = resolve_library_path(project, model_path);
    if path_is_absolute(&table_resolved) {
        return table_resolved;
    }
    let relative = strip_kicad_3dmodel_prefix(&table_resolved);
    if let Some(dir) = design_dir {
        for candidate in [dir.join(relative), dir.join(&table_resolved)] {
            if let Some(hit) = existing_file(candidate) {
                return hit;
            }
        }
    }
    let mut links: Vec<&pcbcraft_doc::LibraryLink> = project.library_table.iter().collect();
    if let Some(hint) = footprint_library {
        links.sort_by_key(|l| if l.name == hint { 0 } else { 1 });
    }
    for link in links {
        let root = Path::new(&link.path);
        for candidate in candidates_for_library_root(root, relative) {
            if let Some(hit) = existing_file(candidate) {
                return hit;
            }
        }
        if relative != table_resolved {
            for candidate in candidates_for_library_root(root, &table_resolved) {
                if let Some(hit) = existing_file(candidate) {
                    return hit;
                }
            }
        }
    }
    table_resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::LibraryLink;

    fn project_with_table(path: &str) -> Project {
        let mut p = Project::default();
        p.library_table.push(LibraryLink {
            name: "libs".into(),
            path: path.into(),
        });
        p
    }

    #[test]
    fn library_table_nickname_is_rewritten() {
        let p = project_with_table("/tmp/libroot");
        assert_eq!(
            resolve_library_path(&p, "libs/foo.glb"),
            "/tmp/libroot/foo.glb"
        );
    }

    type Check = Result<(), String>;

    #[test]
    fn relative_model_is_found_beside_the_library_folder() -> Check {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let model = dir.path().join("part.wrl");
        std::fs::write(&model, b"#VRML V2.0 utf8\n").map_err(|e| e.to_string())?;
        let mut p = Project::default();
        p.library_table.push(LibraryLink {
            name: "fp".into(),
            path: dir.path().to_string_lossy().into(),
        });
        let hit = resolve_model_path(&p, "part.wrl", Some("fp"), None);
        assert_eq!(hit, model.to_string_lossy());
        Ok(())
    }
}
