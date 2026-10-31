//! Minimal Gerber reader for fab-bundle smoke tests: file attributes and flash counts.
use pcbcraft_doc::Project;
use pcbcraft_geom::Point;
use std::collections::BTreeMap;

fn flashes_per_aperture(text: &str) -> BTreeMap<u32, usize> {
    let mut counts = BTreeMap::new();
    let mut current = 0u32;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('D') {
            if rest.ends_with('*')
                && !rest.contains('X')
                && let Ok(code) = rest.trim_end_matches('*').parse::<u32>()
            {
                current = code;
            }
            if rest.contains("D03*") {
                let code_text = rest.split('X').next().unwrap_or("");
                if let Ok(n) = code_text.parse::<u32>() {
                    *counts.entry(n).or_default() += 1;
                } else if current != 0 {
                    *counts.entry(current).or_default() += 1;
                }
            }
        }
    }
    counts
}

#[test]
fn gerber_minimal_reader_counts_flashes_per_aperture_on_sample_board() -> Result<(), String> {
    let mut project = Project::default();
    project.nets.push(pcbcraft_doc::Net {
        id: 1,
        name: "NET1".into(),
        ..Default::default()
    });
    project.board.tracks.push(pcbcraft_doc::Track {
        id: 2,
        net: 1,
        start: Point::new(10.0, 10.0),
        end: Point::new(20.0, 10.0),
        width: 0.25,
        layer: pcbcraft_doc::Layer::Front,
        arc: None,
    });
    project.board.vias.push(pcbcraft_doc::Via {
        id: 3,
        net: 1,
        position: Point::new(20.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
        covering_bottom: None,
    });
    project.validate()?;
    let files = pcbcraft_io::gerber_files(&project)?;
    let front = &files["pcb-F.Cu.gbr"];
    assert!(front.contains("%TF.FileFunction,Copper,L1,Top*%"));
    let counts = flashes_per_aperture(front);
    assert!(
        counts.values().sum::<usize>() >= 1,
        "at least one pad or via flash on front copper"
    );
    pcbcraft_io::assert_gerber_md5_valid(front)?;
    Ok(())
}

#[test]
fn gerber_reader_accepts_md5_and_aperture_functions() -> Result<(), String> {
    let mut project = Project::default();
    project.nets.push(pcbcraft_doc::Net {
        id: 1,
        name: "GND".into(),
        ..Default::default()
    });
    project.board.layer_records = vec![
        pcbcraft_doc::LayerRecord {
            name: "F.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: Some("plane".into()),
            ..Default::default()
        },
        pcbcraft_doc::LayerRecord {
            name: "B.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            ..Default::default()
        },
    ];
    project.board.zones.push(pcbcraft_doc::CopperZone {
        id: 2,
        net: 1,
        layer: pcbcraft_doc::Layer::Front,
        outline: vec![
            Point::new(0.0, 0.0),
            Point::new(30.0, 0.0),
            Point::new(30.0, 20.0),
            Point::new(0.0, 20.0),
            Point::new(0.0, 0.0),
        ],
        cutouts: vec![],
        filled: true,
        kind: None,
        pour: None,
    });
    project.board.vias.push(pcbcraft_doc::Via {
        id: 3,
        net: 9,
        position: Point::new(10.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
    });
    project.validate()?;
    let files = pcbcraft_io::gerber_files(&project)?;
    let front = &files["pcb-F.Cu.gbr"];
    pcbcraft_io::assert_gerber_md5_valid(front)?;
    assert!(front.contains("AntiPad"));
    Ok(())
}
