//! One design origin for every layer file, and a job file that lists exactly
//! the files that ship (issue 176). Layers are read back with `gerber_parser`.

mod common;
use common::*;
use gerber_parser::gerber_types::{Command, DCode, FunctionCode, Operation};
use pcbcraft_geom::Point;
use pcbcraft_io::{gerber_files_at, parse_gerber_layer};
use std::collections::BTreeMap;

const CREATED: &str = "2026-10-29T00:00:00Z";

fn bundle(origin: Point) -> Result<BTreeMap<String, String>, String> {
    let mut p = board(2);
    p.board.vias.push(via(10, 20.0, 0.3, None, None, None)?);
    p.board.holes.push(hole(11, 30.0, 12.0, 2.0));
    p.board.drill_origin = origin;
    gerber_files_at(&p, CREATED)
}

/// Every flash position in a Gerber file, as read by the independent parser.
fn flashes(text: &str) -> Result<Vec<(f64, f64)>, String> {
    let doc = gerber_parser::parse(std::io::BufReader::new(text.as_bytes()))
        .map_err(|(_, e)| e.to_string())?;
    let (mut x, mut y) = (0.0f64, 0.0f64);
    let mut out = Vec::new();
    for command in doc.commands() {
        let Command::FunctionCode(FunctionCode::DCode(DCode::Operation(op))) = command else {
            continue;
        };
        let (Operation::Interpolate(c, _) | Operation::Move(c) | Operation::Flash(c)) = op;
        if let Some(c) = c {
            if let Some(v) = c.x {
                x = f64::from(v);
            }
            if let Some(v) = c.y {
                y = f64::from(v);
            }
        }
        if matches!(op, Operation::Flash(_)) {
            out.push((x, y));
        }
    }
    Ok(out)
}

/// The first hit of an XNC drill file, in millimetres.
fn first_hit(drl: &str) -> Result<(f64, f64), String> {
    let line = drl
        .lines()
        .find(|l| l.starts_with('X') && l.contains('Y'))
        .ok_or("no drill hit")?;
    let (x, y) = line[1..].split_once('Y').ok_or("bad hit")?;
    let num = |s: &str| s.parse::<f64>().map_err(|e| e.to_string());
    Ok((num(x)?, num(y)?))
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}

#[test]
fn copper_outline_and_drill_share_the_design_origin() -> T {
    let moved = bundle(Point::new(5.0, 2.0))?;
    let plain = bundle(Point::new(0.0, 0.0))?;
    let via_at = |files: &BTreeMap<String, String>| -> Result<(f64, f64), String> {
        let copper = files.get("pcb-F.Cu.gbr").ok_or("no front copper")?;
        flashes(copper)?
            .into_iter()
            .find(|f| (f.0 - 20.0).abs() < 20.0 && (f.1 - 10.0).abs() < 20.0)
            .ok_or_else(|| "no via flash".to_string())
    };
    assert!(near(via_at(&plain)?, (20.0, 10.0)));
    assert!(
        near(via_at(&moved)?, (15.0, 8.0)),
        "copper moved by the origin"
    );
    let plated = moved.get("pcb-Plated.drl").ok_or("no plated drill")?;
    assert!(
        near(first_hit(plated)?, (15.0, 8.0)),
        "the drill hit lands on the copper"
    );
    let npth = moved.get("pcb-NPTH.drl").ok_or("no unplated drill")?;
    assert!(
        near(first_hit(npth)?, (25.0, 10.0)),
        "unplated hole shifted too"
    );
    // The outline starts at the board corner minus the origin.
    let edge = parse_gerber_layer(moved.get("pcb-Edge.gbr").ok_or("no edge")?, "edge")?;
    let min_x = edge
        .strokes
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p.0))
        .fold(f64::MAX, f64::min);
    let min_y = edge
        .strokes
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p.1))
        .fold(f64::MAX, f64::min);
    assert!(
        near((min_x, min_y), (-5.0, -2.0)),
        "outline moved: {min_x} {min_y}"
    );
    // Every moved file still carries a valid MD5.
    for (name, text) in &moved {
        if name.ends_with(".gbr") && text.contains("TF.MD5,") {
            pcbcraft_io::assert_gerber_md5_valid(text).map_err(|e| format!("{name}: {e}"))?;
        }
    }
    Ok(())
}

#[test]
fn every_layer_file_moves_with_the_origin() -> T {
    let moved = bundle(Point::new(5.0, 2.0))?;
    let plain = bundle(Point::new(0.0, 0.0))?;
    for (name, text) in &plain {
        if !name.ends_with(".gbr") || name == "pcb-DrillMap.gbr" {
            continue;
        }
        let other = moved.get(name).ok_or_else(|| format!("{name} missing"))?;
        let has_coords = text.lines().any(|l| l.starts_with('X'));
        if has_coords {
            assert_ne!(text, other, "{name} must follow the origin");
        }
    }
    Ok(())
}

#[test]
fn the_job_file_lists_exactly_the_split_files_that_ship() -> T {
    let files = bundle(Point::new(0.0, 0.0))?;
    assert!(
        !files.contains_key("pcb-Drill.drl"),
        "no combined drill ships"
    );
    let job: serde_json::Value =
        serde_json::from_str(files.get("pcb-job.gbrjob").ok_or("no job file")?)
            .map_err(|e| e.to_string())?;
    let listed = job["FilesAttributes"].as_array().ok_or("no file list")?;
    let mut drills = BTreeMap::new();
    for entry in listed {
        let path = entry["Path"].as_str().ok_or("no path")?;
        assert!(
            files.contains_key(path),
            "{path} is listed but does not ship"
        );
        assert_ne!(
            path, "pcb-Drill.drl",
            "the job never lists a combined drill"
        );
        if path.ends_with(".drl") {
            drills.insert(
                path.to_string(),
                entry["FileFunction"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            );
        }
    }
    let shipped: Vec<&String> = files.keys().filter(|n| n.ends_with(".drl")).collect();
    assert_eq!(
        shipped,
        drills.keys().collect::<Vec<_>>(),
        "the job lists every drill file and only those"
    );
    assert!(drills["pcb-Plated.drl"].starts_with("Plated,"));
    assert!(drills["pcb-NPTH.drl"].starts_with("NonPlated,"));
    Ok(())
}
