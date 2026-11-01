//! Back-drill in the fabrication files (issue #801): its own Excellon file
//! per pass, rows in the drill table, and no change for a project without one.

mod common;
use common::*;
use pcbcraft_io::{DrillFunction, drill_table, drill_table_lines};

#[test]
fn a_backdrill_pass_gets_its_own_drill_file() -> T {
    let files = pcbcraft_io::gerber_files(&backdrill_board()?)?;
    let bottom = files
        .get("pcb-BackDrill-L6-L5.drl")
        .ok_or("no file for the pass from the bottom")?;
    let top = files
        .get("pcb-BackDrill-L1-L2.drl")
        .ok_or("no file for the pass from the top")?;
    golden("backdrill_from_bottom.drl", bottom)?;
    golden("backdrill_from_top.drl", top)?;
    assert_eq!(
        files.keys().filter(|k| k.contains("BackDrill")).count(),
        2,
        "one file per pass"
    );
    let plated = files.get("pcb-Plated.drl").ok_or("no plated file")?;
    assert!(
        !plated.contains("C0.500") && !plated.contains("C0.600"),
        "the through file keeps only the via drills: {plated}"
    );
    Ok(())
}

#[test]
fn the_drill_table_lists_the_backdrill_rows() -> T {
    let p = backdrill_board()?;
    let rows = drill_table(&p)?;
    let back: Vec<_> = rows
        .iter()
        .filter(|r| r.function == DrillFunction::BackDrill)
        .map(|r| (r.from_layer, r.to_layer, r.plated, r.diameter_mm, r.count))
        .collect();
    assert_eq!(back, vec![(1, 2, false, 0.6, 1), (5, 6, false, 0.5, 2)]);
    golden(
        "backdrill_table_lines.csv",
        &(drill_table_lines(&rows).join("\n") + "\n"),
    )?;
    let files = pcbcraft_io::gerber_files(&p)?;
    let readme = files.get("README-fabrication.txt").ok_or("no readme")?;
    let map = files.get("pcb-DrillMap.gbr").ok_or("no map")?;
    for line in drill_table_lines(&rows) {
        assert!(readme.contains(&line), "readme lacks {line}");
        assert!(map.contains(&line), "map lacks {line}");
    }
    assert!(readme.contains("pcb-BackDrill-L6-L5.drl"));
    Ok(())
}

#[test]
fn a_project_without_backdrills_exports_the_same_files_as_before() -> T {
    let with = backdrill_board()?;
    let mut without = with.clone();
    for v in &mut without.board.vias {
        v.backdrill = None;
    }
    let a = pcbcraft_io::gerber_files(&with)?;
    let b = pcbcraft_io::gerber_files(&without)?;
    for (name, text) in &b {
        if [
            "README-fabrication.txt",
            "pcb-DrillMap.gbr",
            "pcb-job.gbrjob",
        ]
        .contains(&name.as_str())
        {
            continue;
        }
        assert_eq!(a.get(name), Some(text), "{name} must not change");
    }
    assert!(!b.keys().any(|k| k.contains("BackDrill")));
    assert!(!b["README-fabrication.txt"].contains("Back-drill"));
    assert!(
        drill_table(&without)?
            .iter()
            .all(|r| r.function != DrillFunction::BackDrill)
    );
    let old = pcbcraft_io::gerber_files(&golden_board()?)?;
    assert!(!old.keys().any(|k| k.contains("BackDrill")));
    Ok(())
}
