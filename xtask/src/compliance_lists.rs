//! CI guard for bundled compliance lists: metadata completeness and no list
//! names copied into Rust source.
use pcbcraft_doc::ComplianceListSnapshot;
use pcbcraft_io::{
    builtin_compliance_list, builtin_compliance_list_ids, bundled_list_name_tokens,
    compliance_list_from_toml,
};
use std::path::{Path, PathBuf};

pub fn run(root: &Path) -> Result<(), String> {
    check_bundled_metadata()?;
    check_rust_leaks(root)?;
    println!("compliance lists policy passed");
    Ok(())
}

fn check_bundled_metadata() -> Result<(), String> {
    for id in builtin_compliance_list_ids() {
        let snapshot = builtin_compliance_list(id)?;
        if !snapshot.bundled_metadata_ok() {
            return Err(format!(
                "bundled compliance list {id} must set source_url, retrieved and licence"
            ));
        }
    }
    let lists_dir = root.join("compliance-lists");
    for entry in std::fs::read_dir(&lists_dir).map_err(|e| format!("{}: {e}", lists_dir.display()))?
    {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|e| e == "toml") {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let snapshot: ComplianceListSnapshot = compliance_list_from_toml(&text)?;
            if !snapshot.bundled_metadata_ok() {
                return Err(format!(
                    "bundled list {} must set source_url, retrieved and licence",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn check_rust_leaks(root: &Path) -> Result<(), String> {
    let tokens = bundled_list_name_tokens();
    if tokens.is_empty() {
        return Ok(());
    }
    let mut files = Vec::new();
    collect_rust(root.join("crates"), &mut files)?;
    collect_rust(root.join("apps"), &mut files)?;
    collect_rust(root.join("xtask"), &mut files)?;
    let mut hits = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let lower = text.to_lowercase();
        for token in &tokens {
            if lower.contains(token) {
                hits.push(format!("{} contains list name token `{token}`", path.display()));
            }
        }
    }
    if hits.is_empty() {
        Ok(())
    } else {
        Err(hits.join("\n"))
    }
}

fn collect_rust(dir: PathBuf, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target" || n == "tests") {
                continue;
            }
            collect_rust(path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}
