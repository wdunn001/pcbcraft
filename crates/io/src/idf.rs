//! IDF version 3 board and library files for mechanical exchange.

use crate::part_bodies::{
    component_body_height, footprint_idf_names, footprint_local_outline, is_bottom_side,
};
use pcbcraft_doc::Project;
use pcbcraft_geom::Point;
use std::collections::BTreeMap;

const IDF_SOURCE: &str = "pcbcraft";
const IDF_DATE: &str = "1970/01/01.00:00:00";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FootprintKey {
    geometry: String,
    part: String,
}

#[derive(Clone, Debug, PartialEq)]
struct FootprintOutline {
    height_mm: f64,
    ring: Vec<Point>,
}

fn idf_float(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 {
        "0.0".into()
    } else {
        format!("{r:.3}")
    }
}

fn idf_token(text: &str) -> String {
    if text.is_empty() {
        return "\"\"".into();
    }
    let needs_quotes = text.contains(' ') || text.contains('"');
    if needs_quotes {
        format!("\"{}\"", text.replace('"', "'"))
    } else {
        text.to_string()
    }
}

fn board_outline_ring(p: &Project) -> Vec<Point> {
    match &p.board.outline {
        Some(outline) if outline.len() >= 3 => {
            let mut ring = outline.clone();
            if ring.first() != ring.last() {
                if let Some(first) = ring.first().copied() {
                    ring.push(first);
                }
            }
            ring
        }
        _ => vec![
            Point::new(0.0, 0.0),
            Point::new(p.board.width, 0.0),
            Point::new(p.board.width, p.board.height),
            Point::new(0.0, p.board.height),
            Point::new(0.0, 0.0),
        ],
    }
}

fn outline_lines(ring: &[Point]) -> String {
    let mut out = String::new();
    for point in ring {
        out.push_str(&format!(
            "0 {} {} 0.0\n",
            idf_float(point.x),
            idf_float(point.y)
        ));
    }
    out
}

fn catalog(p: &Project) -> Result<BTreeMap<FootprintKey, FootprintOutline>, String> {
    let mut entries: BTreeMap<FootprintKey, FootprintOutline> = BTreeMap::new();
    for fp in &p.board.footprints {
        let (geometry, part) = footprint_idf_names(p, fp)?;
        let key = FootprintKey { geometry, part };
        let component = p
            .component(fp.component)
            .ok_or("footprint references an unknown component")?;
        let height = component_body_height(Some(component));
        let Some(ring) = footprint_local_outline(p, fp) else {
            continue;
        };
        entries
            .entry(key)
            .and_modify(|existing| {
                if height > existing.height_mm {
                    existing.height_mm = height;
                }
            })
            .or_insert(FootprintOutline {
                height_mm: height,
                ring,
            });
    }
    if entries.is_empty() {
        return Err("board has no placed footprints to export".into());
    }
    Ok(entries)
}

/// Library half of an IDF version 3 pair: one electrical outline per distinct
/// footprint, with height taken from the same body rules as STEP and VRML.
pub fn idf_library(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_DISTINCT: usize = 5_000;
    let entries = catalog(p)?;
    if entries.len() > MAX_DISTINCT {
        return Err("mechanical library exceeds the footprint limit".into());
    }
    let mut out = format!(".HEADER\nLIBRARY_FILE 3.0 \"{IDF_SOURCE}\" {IDF_DATE} 1\n.END_HEADER\n");
    for (key, outline) in entries {
        out.push_str(".ELECTRICAL\n");
        out.push_str(&format!(
            "{} {} MM {}\n",
            idf_token(&key.geometry),
            idf_token(&key.part),
            idf_float(outline.height_mm)
        ));
        out.push_str(&outline_lines(&outline.ring));
        out.push_str(".END_ELECTRICAL\n");
    }
    Ok(out)
}

/// Board half of an IDF version 3 pair: outline, thickness and placements that
/// reference geometry names from the library file.
pub fn idf_board(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PLACED: usize = 5_000;
    if p.board.footprints.len() > MAX_PLACED {
        return Err("mechanical drawing exceeds the placed-object limit".into());
    }
    let entries = catalog(p)?;
    let board_name = p.name.replace('"', "'");
    let mut out = format!(
        ".HEADER\nBOARD_FILE 3.0 \"{IDF_SOURCE}\" {IDF_DATE} 1\n{board_name} MM\n.END_HEADER\n"
    );
    out.push_str(".BOARD_OUTLINE ECAD\n");
    out.push_str(&format!("{}\n", idf_float(p.board.thickness)));
    out.push_str(&outline_lines(&board_outline_ring(p)));
    out.push_str(".END_BOARD_OUTLINE\n");
    out.push_str(".PLACEMENT\n");
    for fp in &p.board.footprints {
        let component = p
            .component(fp.component)
            .ok_or("footprint references an unknown component")?;
        let (geometry, part) = footprint_idf_names(p, fp)?;
        let key = FootprintKey { geometry, part };
        if !entries.contains_key(&key) {
            return Err("footprint outline is missing from the mechanical library".into());
        }
        out.push_str(&format!(
            "{} {} {}\n",
            idf_token(&key.geometry),
            idf_token(&key.part),
            idf_token(&component.reference)
        ));
        let side = if is_bottom_side(fp) { "BOTTOM" } else { "TOP" };
        out.push_str(&format!(
            "{} {} 0.0 {} {} PLACED\n",
            idf_float(fp.position.x),
            idf_float(fp.position.y),
            idf_float(fp.rotation),
            side
        ));
    }
    out.push_str(".END_PLACEMENT\n");
    Ok(out)
}
