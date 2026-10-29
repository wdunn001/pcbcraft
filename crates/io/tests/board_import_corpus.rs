//! Imports the open-hardware corpus named by `PCBCRAFT_OPENDRONE_DIR`.
//!
//! Context: the designs are CERN-OHL-S-2.0 and must never be copied into this
//! repository or embedded in a binary, so this test reads them from disk at
//! run time and skips when the variable is unset.
//!
//! # Do not
//! - Use `include_str!` or `include_bytes!` on any corpus file.
use std::path::Path;

fn board_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let dir = entry.path().join("hardware");
        if let Ok(files) = std::fs::read_dir(&dir) {
            for f in files.flatten() {
                let p = f.path();
                if p.extension().is_some_and(|e| e == "kicad_pcb") {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

#[test]
fn corpus_boards_import_without_error() -> Result<(), String> {
    let Ok(root) = std::env::var("PCBCRAFT_OPENDRONE_DIR") else {
        eprintln!("PCBCRAFT_OPENDRONE_DIR unset; skipping corpus import");
        return Ok(());
    };
    let mut failures = Vec::new();
    for path in board_files(Path::new(&root)) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        match pcbcraft_io::read_board_interchange_report(&text) {
            Ok((project, r)) => {
                // Optional: keep the imported project for screenshots (outside the repository).
                if let Ok(dir) = std::env::var("PCBCRAFT_OPENDRONE_SAVE_DIR") {
                    let parent = path.parent().and_then(|p| p.parent());
                    let name = parent
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "design".into());
                    let stem = path
                        .file_stem()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let out = Path::new(&dir).join(format!("{name}-{stem}.pcbcraft"));
                    match pcbcraft_io::save(&project, &out).and_then(|()| pcbcraft_io::load(&out)) {
                        Ok(loaded) if loaded.board.tracks.len() == project.board.tracks.len() => {}
                        Ok(_) => {
                            failures.push(format!("{}: reload changed the tracks", out.display()))
                        }
                        Err(e) => {
                            println!("ODSAVE {} ERR {e}", out.display());
                            failures.push(format!("{}: save or reload failed: {e}", out.display()));
                        }
                    }
                }
                match pcbcraft_io::kicad_board(&project)
                    .and_then(|t| pcbcraft_io::read_board_interchange_report(&t))
                {
                    Ok((again, _)) => {
                        let same = again.board.tracks.len() == project.board.tracks.len()
                            && again.board.vias.len() == project.board.vias.len()
                            && again.board.zones.len() == project.board.zones.len()
                            && again.board.footprints.len() == project.board.footprints.len()
                            && again.nets.len() == project.nets.len()
                            && again.copper_layer_count() == project.copper_layer_count();
                        println!(
                            "ODROUNDTRIP {} {}",
                            path.display(),
                            if same { "same" } else { "DIFFERENT" }
                        );
                        if !same {
                            failures.push(format!(
                                "{}: export and re-import changed counts",
                                path.display()
                            ));
                        }
                    }
                    Err(e) => {
                        println!("ODROUNDTRIP {} ERR {e}", path.display());
                        failures.push(format!("{}: re-import failed: {e}", path.display()));
                    }
                }
                println!(
                    "ODRESULT {} OK src={:?} imp={:?}",
                    path.display(),
                    r.source,
                    r.imported
                );
                for n in &r.notices {
                    println!("ODNOTICE {} | {n}", path.display());
                }
            }
            Err(e) => {
                println!("ODRESULT {} ERR {e}", path.display());
                failures.push(format!("{}: {e}", path.display()));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Reads the root schematic of every design. Failures are printed, not
/// asserted: the schematic reader has its own supported subset, and this
/// test only requires that no file makes it panic.
#[test]
fn corpus_schematics_import_without_panic() {
    let Ok(root) = std::env::var("PCBCRAFT_OPENDRONE_DIR") else {
        eprintln!("PCBCRAFT_OPENDRONE_DIR unset; skipping corpus schematics");
        return;
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    let mut dirs: Vec<_> = entries
        .flatten()
        .map(|e| e.path().join("hardware"))
        .collect();
    dirs.sort();
    for dir in dirs {
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().is_some_and(|e| e == "kicad_pro") {
                let sch = p.with_extension("kicad_sch");
                let Ok(text) = std::fs::read_to_string(&sch) else {
                    continue;
                };
                match pcbcraft_io::read_schematic_interchange_path(&sch, &text) {
                    Ok(project) => println!(
                        "ODSCH {} OK components={} nets={} sheets={}",
                        sch.display(),
                        project.components.len(),
                        project.nets.len(),
                        project.sheets.len()
                    ),
                    Err(e) => println!("ODSCH {} ERR {e}", sch.display()),
                }
            }
        }
    }
}
