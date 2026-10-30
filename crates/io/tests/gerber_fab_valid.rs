//! Fabrication Gerber and drill acceptance for issue 176: independent parser
//! checks and golden fixtures per layer type.
use pcbcraft_doc::Project;
use pcbcraft_geom::Point;
use pcbcraft_io::{gerber_files_at, PACKAGE_CREATION_DATE};
use std::collections::BTreeMap;
use std::io::BufReader;
use std::path::Path;

const GOLDEN_DIR: &str = "tests/fixtures/fab_bundle";

fn parse_with_gerber_parser(text: &str, name: &str) -> Result<(), String> {
    let reader = BufReader::new(text.as_bytes());
    match gerber_parser::parse(reader) {
        Ok(_doc) => Ok(()),
        Err((_partial, err)) => Err(format!("{name} failed gerber_parser: {err:?}")),
    }
}

fn normalize_fab_gerber(text: &str) -> String {
    text.lines()
        .filter(|line| {
            !line.contains("CreationDate")
                && !line.contains("MD5")
                && !line.starts_with("%TF.GenerationSoftware")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn check_golden(name: &str, text: &str) -> Result<(), String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(GOLDEN_DIR)
        .join(name);
    if std::env::var_os("PCBCRAFT_BLESS").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
    }
    let golden =
        std::fs::read_to_string(&path).map_err(|e| format!("golden fixture {name} missing: {e}"))?;
    let actual = normalize_fab_gerber(text);
    let expected = normalize_fab_gerber(&golden);
    assert_eq!(
        actual, expected,
        "Gerber output drifted from the reviewed fixture {name}"
    );
    Ok(())
}

/// Rich board used for fab-bundle parser and golden coverage.
fn fab_bundle_project() -> Result<Project, String> {
    let mut project = Project::default();
    project.nets.push(pcbcraft_doc::Net {
        id: 8,
        name: "GND".into(),
        ..Default::default()
    });
    project.board.tracks.push(pcbcraft_doc::Track {
        id: 3,
        net: 8,
        start: Point::new(10.0, 10.0),
        end: Point::new(30.0, 10.0),
        width: 0.3,
        layer: pcbcraft_doc::Layer::Back,
        arc: None,
    });
    project.board.vias.push(pcbcraft_doc::Via {
        id: 4,
        net: 8,
        position: Point::new(30.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
    });
    project.board.zones.push(pcbcraft_doc::CopperZone {
        id: 5,
        net: 8,
        layer: pcbcraft_doc::Layer::Front,
        outline: vec![
            Point::new(5.0, 5.0),
            Point::new(45.0, 5.0),
            Point::new(45.0, 25.0),
            Point::new(5.0, 25.0),
            Point::new(5.0, 5.0),
        ],
        cutouts: vec![],
        filled: true,
        kind: None,
        pour: None,
    });
    project.board.cutouts.push(pcbcraft_doc::Cutout {
        id: 6,
        center: Point::new(46.0, 20.0),
        radius: 2.0,
    });
    let through_pad = pcbcraft_doc::Pad {
        number: "1".into(),
        offset: Point::new(0.0, 0.0),
        diameter: 1.6,
        net: None,
        castellated: None,
        geometry: Some(pcbcraft_library::FootprintPad {
            number: "1".into(),
            pad_type: "thru_hole".into(),
            shape: "circle".into(),
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            size: Point::new(1.6, 1.6),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(0.8, 0.8),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        }),
    };
    let embedded = through_pad.geometry.as_ref().ok_or("pad geometry")?.clone();
    let mut pad2 = through_pad.clone();
    pad2.number = "2".into();
    pad2.offset = Point::new(2.0, 0.0);
    if let Some(g) = pad2.geometry.as_mut() {
        g.number = "2".into();
        g.position = Point::new(2.0, 0.0);
    }
    let mut embedded2 = embedded.clone();
    embedded2.number = "2".into();
    embedded2.position = Point::new(2.0, 0.0);
    project.board.footprints.push(pcbcraft_doc::Footprint {
        id: 1,
        component: 1,
        position: Point::new(0.0, 0.0),
        rotation: 0.0,
        locked: None,
        side: None,
        pads: vec![through_pad.clone(), pad2],
    });
    project.components.push(pcbcraft_doc::Component {
        id: 1,
        reference: "J1".into(),
        value: "CONN".into(),
        kind: "connector".into(),
        position: Point::new(0.0, 0.0),
        rotation: 0.0,
        pins: vec![pcbcraft_doc::Pin {
            number: "1".into(),
            name: String::new(),
            offset: Point::default(),
            net: None,
            electrical_type: "passive".into(),
            direction: None,
            unit: 1,
        }],
        units: vec![],
        library_symbol: None,
        library_footprint: Some(pcbcraft_library::LibraryFootprint {
            library: "ThroughHole".into(),
            definition: pcbcraft_library::FootprintDefinition {
                name: "PAD-1.6".into(),
                description: String::new(),
                keywords: String::new(),
                pads: vec![embedded, embedded2],
                graphics: vec![],
                model: None,
                rules: None,
                raw_source: String::new(),
                unsupported: vec![],
            },
        }),
        sheet: 0,
        fields: vec![],
    });
    project.validate()?;
    Ok(project)
}

fn fab_files() -> Result<BTreeMap<String, String>, String> {
    gerber_files_at(&fab_bundle_project()?, PACKAGE_CREATION_DATE)
}

#[test]
fn every_fab_gerber_layer_parses_with_gerber_parser() -> Result<(), String> {
    let files = fab_files()?;
    for (name, text) in &files {
        if name.ends_with(".gbr") {
            parse_with_gerber_parser(text, name)?;
        }
    }
    Ok(())
}

#[test]
fn plated_and_unplated_drill_files_are_split() -> Result<(), String> {
    let files = fab_files()?;
    let plated = files
        .get("pcb-Plated.drl")
        .ok_or("plated drill file missing")?;
    let npth = files.get("pcb-NPTH.drl").ok_or("npth drill file missing")?;
    assert!(
        plated.starts_with("M48\nPlated,1,2,PTH\n"),
        "plated header: {plated}"
    );
    assert!(
        npth.starts_with("M48\nNonPlated,1,2,NPTH\n"),
        "npth header: {npth}"
    );
    assert!(plated.contains("T1\nX30.000Y10.000\n"), "via on plated file");
    assert!(plated.contains("T2\nX0.000Y0.000\n"), "pad on plated file");
    assert!(
        !npth.contains("X30.000Y10.000"),
        "plated hits stay out of the npth file"
    );
    Ok(())
}

#[test]
fn fab_bundle_golden_fixtures_cover_each_gerber_layer_type() -> Result<(), String> {
    let files = fab_files()?;
    let layers = [
        ("pcb-F.Cu.gbr", "copper_top.gbr"),
        ("pcb-B.Cu.gbr", "copper_bottom.gbr"),
        ("pcb-Edge.gbr", "profile.gbr"),
        ("pcb-F.Mask.gbr", "mask_top.gbr"),
        ("pcb-B.Mask.gbr", "mask_bottom.gbr"),
        ("pcb-F.Paste.gbr", "paste_top.gbr"),
        ("pcb-B.Paste.gbr", "paste_bottom.gbr"),
        ("pcb-F.SilkS.gbr", "silk_top.gbr"),
        ("pcb-B.SilkS.gbr", "silk_bottom.gbr"),
        ("pcb-DrillMap.gbr", "drill_map.gbr"),
    ];
    for (key, fixture) in layers {
        let text = files.get(key).ok_or_else(|| format!("export missing {key}"))?;
        check_golden(fixture, text)?;
    }
    Ok(())
}
