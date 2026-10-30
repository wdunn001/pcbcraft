//! IDF 3.0 board and library writers for mechanical exchange.

use crate::part_bodies;
use pcbcraft_doc::{Footprint, Layer, Project};
use pcbcraft_geom::Point;
use std::collections::BTreeMap;

const MAX_PLACED: usize = 5_000;
const SOURCE: &str = "pcbcraft";
const EXPORT_STAMP: &str = "1970/01/01.00:00:00";

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct LibraryKey {
    geometry: String,
    part_number: String,
    mechanical: bool,
}

#[derive(Clone, Debug)]
struct LibraryEntry {
    height_mm: f64,
    outline: (Point, Point),
}

fn mm3(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 {
        "0.000".into()
    } else {
        format!("{r:.3}")
    }
}

fn idf_string(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "UNKNOWN".into();
    }
    let safe = trimmed.replace('"', "'");
    if safe
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        safe
    } else {
        format!("\"{safe}\"")
    }
}

fn library_key(project: &Project, fp: &Footprint) -> Result<LibraryKey, String> {
    let component = project
        .component(fp.component)
        .ok_or("footprint references an unknown component")?;
    let mechanical = component.kind == "mechanical";
    let (geometry, part_number) = match component.library_footprint.as_ref() {
        Some(lib_fp) => {
            let name = idf_string(&lib_fp.definition.name);
            let part = if lib_fp.library.trim().is_empty() {
                name.clone()
            } else {
                idf_string(&format!("{}/{}", lib_fp.library, lib_fp.definition.name))
            };
            (name, part)
        }
        None => {
            let fallback = idf_string(&component.reference);
            (fallback.clone(), fallback)
        }
    };
    Ok(LibraryKey {
        geometry,
        part_number,
        mechanical,
    })
}

fn board_outline_ring(p: &Project) -> Vec<Point> {
    match &p.board.outline {
        Some(points) if points.len() >= 3 => {
            let mut ring = points.clone();
            if ring.first() != ring.last() {
                ring.push(ring[0]);
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

fn outline_lines(lo: Point, hi: Point) -> String {
    let corners = [
        Point::new(lo.x, lo.y),
        Point::new(hi.x, lo.y),
        Point::new(hi.x, hi.y),
        Point::new(lo.x, hi.y),
        Point::new(lo.x, lo.y),
    ];
    corners
        .iter()
        .map(|p| format!("0 {} {} 0.0", mm3(p.x), mm3(p.y)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_library(project: &Project) -> Result<BTreeMap<LibraryKey, LibraryEntry>, String> {
    let mut entries: BTreeMap<LibraryKey, LibraryEntry> = BTreeMap::new();
    for fp in &project.board.footprints {
        let key = library_key(project, fp)?;
        let height = part_bodies::placed_body_height_mm(project, fp);
        let component = project
            .component(fp.component)
            .ok_or("footprint references an unknown component")?;
        let outline = component
            .library_footprint
            .as_ref()
            .and_then(part_bodies::library_footprint_extent)
            .or_else(|| part_bodies::footprint_local_extent(project, fp))
            .ok_or_else(|| {
                format!(
                    "footprint {} has no courtyard or pad extent for IDF export",
                    key.geometry
                )
            })?;
        entries
            .entry(key)
            .and_modify(|entry| {
                if height > entry.height_mm {
                    entry.height_mm = height;
                }
            })
            .or_insert(LibraryEntry {
                height_mm: height,
                outline,
            });
    }
    Ok(entries)
}

fn validate_export(project: &Project) -> Result<(), String> {
    project.validate()?;
    if project.board.footprints.is_empty() {
        return Err("board has no placed footprints to export".into());
    }
    if project.board.footprints.len() > MAX_PLACED {
        return Err("mechanical drawing exceeds the placed-object limit".into());
    }
    Ok(())
}

/// Library half of an IDF 3.0 exchange: one outline and height per distinct footprint.
pub fn idf_library(p: &Project) -> Result<String, String> {
    validate_export(p)?;
    let entries = collect_library(p)?;
    let mut out = String::new();
    out.push_str(".HEADER\n");
    out.push_str(&format!("LIBRARY_FILE 3.0 \"{SOURCE}\" {EXPORT_STAMP} 1\n"));
    out.push_str(".END_HEADER\n");
    for (key, entry) in entries {
        let section = if key.mechanical {
            ".MECHANICAL"
        } else {
            ".ELECTRICAL"
        };
        out.push_str(section);
        out.push('\n');
        out.push_str(&format!(
            "{} {} MM {}\n",
            key.geometry,
            key.part_number,
            mm3(entry.height_mm)
        ));
        out.push_str(&outline_lines(entry.outline.0, entry.outline.1));
        out.push('\n');
        out.push_str(if key.mechanical {
            ".END_MECHANICAL\n"
        } else {
            ".END_ELECTRICAL\n"
        });
    }
    Ok(out)
}

/// Board half of an IDF 3.0 exchange: outline and placements referencing the library file.
pub fn idf_board(p: &Project) -> Result<String, String> {
    validate_export(p)?;
    let _ = collect_library(p)?;
    let board_name = idf_string(&p.name.replace('"', "'"));
    let mut out = String::new();
    out.push_str(".HEADER\n");
    out.push_str(&format!("BOARD_FILE 3.0 \"{SOURCE}\" {EXPORT_STAMP} 1\n"));
    out.push_str(&format!("{board_name} MM\n"));
    out.push_str(".END_HEADER\n");
    out.push_str(".BOARD_OUTLINE ECAD\n");
    out.push_str(&format!("{}\n", mm3(p.board.thickness)));
    let ring = board_outline_ring(p);
    for point in ring {
        out.push_str(&format!("0 {} {} 0.0\n", mm3(point.x), mm3(point.y)));
    }
    out.push_str(".END_BOARD_OUTLINE\n");
    out.push_str(".PLACEMENT\n");
    for fp in &p.board.footprints {
        let component = project_component(p, fp)?;
        let key = library_key(p, fp)?;
        let back = fp.board_side() == Layer::Back;
        out.push_str(&format!(
            "{} {} {}\n",
            key.geometry,
            key.part_number,
            idf_string(&component.reference)
        ));
        out.push_str(&format!(
            "{} {} 0.000 {} {} PLACED\n",
            mm3(fp.position.x),
            mm3(fp.position.y),
            mm3(fp.rotation),
            if back { "BOTTOM" } else { "TOP" }
        ));
    }
    out.push_str(".END_PLACEMENT\n");
    Ok(out)
}

fn project_component<'a>(
    p: &'a Project,
    fp: &Footprint,
) -> Result<&'a pcbcraft_doc::Component, String> {
    p.component(fp.component)
        .ok_or_else(|| "footprint references an unknown component".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{
        Component, ComponentField, Footprint, LibraryFootprint, Pad, Pin, Project,
    };
    use pcbcraft_library::{FootprintDefinition, FootprintPad};

    fn pad(number: &str, x: f64) -> FootprintPad {
        FootprintPad {
            number: number.into(),
            pad_type: "smd".into(),
            shape: "rect".into(),
            position: Point::new(x, 0.0),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: vec!["F.Cu".into()],
            drill: None,
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        }
    }

    fn sample_project() -> Result<Project, String> {
        let mut project = Project::default();
        project.board.width = 60.0;
        project.board.height = 40.0;
        project.components.push(Component {
            sheet: 0,
            id: 1,
            reference: "R5".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            pins: vec![Pin {
                number: "1".into(),
                name: String::new(),
                offset: Point::new(-1.0, 0.0),
                net: None,
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(LibraryFootprint {
                library: "Test".into(),
                definition: FootprintDefinition {
                    name: "TEST_ONE".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![pad("1", -1.0)],
                    graphics: vec![],
                    model: None,
                    rules: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                },
            }),
            fields: vec![ComponentField {
                key: "height".into(),
                value: "2.5mm".into(),
            }],
        });
        project.components.push(Component {
            sheet: 0,
            id: 2,
            reference: "U1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::new(20.0, 10.0),
            rotation: 0.0,
            pins: vec![Pin {
                number: "1".into(),
                name: String::new(),
                offset: Point::new(-1.0, 0.0),
                net: None,
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(LibraryFootprint {
                library: "Test".into(),
                definition: FootprintDefinition {
                    name: "TEST_ONE".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![pad("1", 0.0)],
                    graphics: vec![],
                    model: None,
                    rules: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                },
            }),
            fields: vec![],
        });
        project.board.footprints.push(Footprint {
            id: 10,
            component: 1,
            position: Point::new(12.5, 8.25),
            rotation: 90.0,
            locked: None,
            side: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::new(-1.0, 0.0),
                diameter: 1.0,
                net: None,
                castellated: None,
                geometry: Some(pad("1", -1.0)),
            }],
        });
        project.board.footprints.push(Footprint {
            id: 11,
            component: 2,
            position: Point::new(30.0, 20.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::new(0.0, 0.0),
                diameter: 1.0,
                net: None,
                castellated: None,
                geometry: Some(pad("1", 0.0)),
            }],
        });
        project.validate()?;
        Ok(project)
    }

    #[test]
    fn idf_pair_lists_each_footprint_once_and_board_references_names() -> Result<(), String> {
        let project = sample_project()?;
        let board = idf_board(&project)?;
        let library = idf_library(&project)?;
        assert!(
            board.starts_with(".HEADER\nBOARD_FILE 3.0"),
            "board declares IDF 3.0"
        );
        assert!(
            library.starts_with(".HEADER\nLIBRARY_FILE 3.0"),
            "library declares IDF 3.0"
        );
        assert_eq!(
            library.matches(".ELECTRICAL").count(),
            1,
            "one footprint entry"
        );
        assert!(
            library.contains("TEST_ONE Test/TEST_ONE MM 2.500"),
            "height comes from the component body field"
        );
        assert!(
            board.contains("TEST_ONE Test/TEST_ONE R5\n")
                && board.contains("TEST_ONE Test/TEST_ONE U1\n"),
            "board references library geometry names, not embedded pin heights"
        );
        assert!(
            !board.contains(".PIN_FILE"),
            "board file does not embed a legacy pin file"
        );
        Ok(())
    }
}
