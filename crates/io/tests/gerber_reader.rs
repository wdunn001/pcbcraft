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
        id: 801,
        name: "NET1".into(),
        ..Default::default()
    });
    project.board.tracks.push(pcbcraft_doc::Track {
        id: 802,
        net: 801,
        start: Point::new(10.0, 10.0),
        end: Point::new(20.0, 10.0),
        width: 0.25,
        layer: pcbcraft_doc::Layer::Front,
        arc: None,
    });
    project.board.vias.push(pcbcraft_doc::Via {
        id: 803,
        net: 801,
        position: Point::new(20.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
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
    Ok(())
}
