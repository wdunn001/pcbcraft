//! Prints the checks findings of imported boards, grouped by code.
//!
//! Context: reads project files saved by the importer corpus test from the
//! directory in `PCBCRAFT_OPENDRONE_SAVE_DIR`; skips when it is unset. The
//! designs are CERN-OHL-S-2.0 and are never copied into the repository.
use std::collections::BTreeMap;

#[test]
fn imported_board_findings_by_code() {
    let Ok(dir) = std::env::var("PCBCRAFT_OPENDRONE_SAVE_DIR") else {
        eprintln!("PCBCRAFT_OPENDRONE_SAVE_DIR unset; skipping");
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let Ok(project) = pcbcraft_io::load(&path) else {
            continue;
        };
        let mut session = pcbcraft_engine::Session::default();
        if session.replace(project).is_err() {
            continue;
        }
        let checks = session.checks();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for list in ["erc", "drc"] {
            for f in checks[list].as_array().into_iter().flatten() {
                let key = format!(
                    "{} {} {}",
                    list,
                    f["severity"].as_str().unwrap_or("?"),
                    f["code"].as_str().unwrap_or("?")
                );
                *counts.entry(key).or_insert(0) += 1;
            }
        }
        for (key, n) in counts {
            println!(
                "ODFIND {} {n} {key}",
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            );
        }
    }
}
