//! Minimal Gerber reader for fab-bundle smoke tests: file attributes and flash counts.
use pcbcraft_doc::Project;
use pcbcraft_geom::Point;
use std::collections::BTreeMap;

fn flashes_per_aperture(text: &str) -> BTreeMap<u32, usize> {
    let mut counts = BTreeMap::new();
    let mut current = 0u32;
    for line in text.lines() {
        // A bare aperture select such as `D12*` chooses the aperture for the
        // flashes that follow it.
        if let Some(rest) = line.strip_prefix('D')
            && let Some(code) = rest.strip_suffix('*')
            && let Ok(code) = code.parse::<u32>()
        {
            current = code;
            continue;
        }
        if line.ends_with("D03*") {
            // An older writer put the select in front of the coordinates.
            let inline = line
                .strip_prefix('D')
                .and_then(|rest| rest.split('X').next())
                .and_then(|code| code.parse::<u32>().ok());
            let code = inline.unwrap_or(current);
            if code != 0 {
                *counts.entry(code).or_default() += 1;
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

        preserved: pcbcraft_doc::PreservedSource::default(),
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

        preserved: pcbcraft_doc::PreservedSource::default(),
        backdrill: Default::default(),
        covering_bottom: Default::default(),
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
