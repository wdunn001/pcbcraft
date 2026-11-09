//! Build step: list the machine files in the repo `machine-profiles/` folder.
//!
//! The program ships every `*.toml` file in that folder as a built-in machine. This step
//! writes one generated source file that embeds each of them, so adding a machine means
//! adding a file and nothing in code.
use std::fmt::Write as _;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let folder = manifest.join("..").join("..").join("machine-profiles");
    println!("cargo:rerun-if-changed={}", folder.display());
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&folder)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            files.push(path);
        }
    }
    files.sort();
    let mut out = String::from("&[\n");
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let absolute = std::fs::canonicalize(path)?;
        writeln!(out, "    include_str!({:?}),", absolute.to_string_lossy())?;
    }
    out.push_str("]\n");
    let target = PathBuf::from(std::env::var("OUT_DIR")?).join("builtin_machine_files.rs");
    std::fs::write(target, out)?;
    Ok(())
}
