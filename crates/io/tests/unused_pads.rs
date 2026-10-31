//! Unused inner pad removal in Gerber copper plots.
use pcbcraft_doc::fill::pad_exports_on_layer;
use pcbcraft_doc::{Component, FabRules, Footprint, Layer, Net, Pad, Pin, Project, Track, Via};
use pcbcraft_geom::Point;
use pcbcraft_library::{Drill, FootprintPad};
use std::collections::BTreeMap;

fn flashes(text: &str) -> usize {
    text.lines()
        .filter(|line| line.contains("D03*"))
        .count()
}

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

fn stack_layers(count: u8) -> Project {
    let mut p = Project::default();
    for i in 1..=count {
        p.board.layer_records.push(pcbcraft_doc::LayerRecord {
            name: format!("L{i}"),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: None,
            ..Default::default()
        });
        if i < count {
            p.board.layer_records.push(pcbcraft_doc::LayerRecord {
                name: format!("P{i}"),
                kind: "prepreg".into(),
                thickness: 0.2,
                copper_role: None,
                ..Default::default()
            });
        }
    }
    p
}

fn tht_pad(number: &str, net: u64, x: f64, y: f64) -> Pad {
    Pad {
        number: number.into(),
        net: Some(net),
        offset: Point::new(x, y),
        diameter: 1.6,
        geometry: Some(FootprintPad {
            pad_type: "thru_hole".into(),
            shape: "circle".into(),
            size: Point::new(1.6, 1.6),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            drill: Some(Drill {
                size: Point::new(0.8, 0.8),
                offset: Point::default(),
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn place_tht(p: &mut Project, id: u64, net: u64, x: f64, y: f64) {
    if !p.nets.iter().any(|n| n.id == net) {
        p.nets.push(Net {
            id: net,
            name: format!("N{net}"),
            ..Default::default()
        });
    }
    p.components.push(Component {
        id,
        reference: format!("U{id}"),
        value: "PART".into(),
        pins: vec![Pin {
            number: "1".into(),
            net: Some(net),
            ..Default::default()
        }],
        ..Default::default()
    });
    p.board.footprints.push(Footprint {
        id,
        component: id,
        position: Point::new(x, y),
        pads: vec![tht_pad("1", net, 0.0, 0.0)],
        ..Default::default()
    });
}

#[test]
fn an_unconnected_inner_pad_is_left_off_a_six_layer_board() -> Result<(), String> {
    let mut project = stack_layers(6);
    place_tht(&mut project, 10, 1, 12.0, 10.0);
    project.validate()?;
    let inner = Layer::from_index(2)?;
    let text = pcbcraft_io::gerber_copper(&project, inner)?;
    assert_eq!(
        flashes(&text),
        0,
        "an unconnected through-hole pad should not flash on an inner layer"
    );
    let top = pcbcraft_io::gerber_copper(&project, Layer::Front)?;
    assert!(
        flashes(&top) >= 1,
        "the end layer should still carry the pad"
    );
    Ok(())
}

#[test]
fn the_setting_off_keeps_the_pad() -> Result<(), String> {
    let mut project = stack_layers(6);
    place_tht(&mut project, 10, 1, 12.0, 10.0);
    project.board.rules = Some(FabRules {
        remove_unused_inner_pads: Some(false),
        ..FabRules::default()
    });
    project.validate()?;
    let inner = Layer::from_index(2)?;
    let text = pcbcraft_io::gerber_copper(&project, inner)?;
    assert_eq!(flashes(&text), 1, "with removal off the inner pad stays");
    Ok(())
}

#[test]
fn the_end_layers_always_keep_their_pads() -> Result<(), String> {
    let mut project = stack_layers(6);
    place_tht(&mut project, 10, 1, 12.0, 10.0);
    project.validate()?;
    let bottom = Layer::from_index(5)?;
    let text = pcbcraft_io::gerber_copper(&project, bottom)?;
    assert_eq!(flashes(&text), 1, "the bottom end layer keeps the pad");
    Ok(())
}

#[test]
fn a_connected_inner_pad_is_never_removed() -> Result<(), String> {
    let mut project = stack_layers(6);
    place_tht(&mut project, 10, 1, 12.0, 10.0);
    project.board.tracks.push(Track {
        id: 20,
        net: 1,
        start: Point::new(11.0, 10.0),
        end: Point::new(13.0, 10.0),
        width: 0.25,
        layer: Layer::from_index(2)?,
        arc: None,
    });
    project.validate()?;
    let inner = Layer::from_index(2)?;
    let text = pcbcraft_io::gerber_copper(&project, inner)?;
    assert_eq!(
        flashes(&text),
        1,
        "a track on the inner layer keeps the pad in the plot"
    );
    Ok(())
}

#[test]
fn a_profile_without_the_field_uses_the_layer_count_default() -> Result<(), String> {
    let json = r#"{"min_track":0.15}"#;
    let rules: FabRules = serde_json::from_str(json).map_err(|e| e.to_string())?;
    assert!(rules.remove_unused_inner_pads.is_none());
    let mut six = stack_layers(6);
    six.board.rules = Some(rules);
    assert!(six.remove_unused_inner_pads_enabled());
    let mut four = stack_layers(4);
    four.board.rules = Some(FabRules::default());
    assert!(!four.remove_unused_inner_pads_enabled());
    Ok(())
}

#[test]
fn four_layer_board_honours_an_explicit_on_setting() -> Result<(), String> {
    let mut project = stack_layers(4);
    place_tht(&mut project, 10, 1, 12.0, 10.0);
    project.board.rules = Some(FabRules {
        remove_unused_inner_pads: Some(true),
        ..FabRules::default()
    });
    project.validate()?;
    let inner = Layer::from_index(1)?;
    let text = pcbcraft_io::gerber_copper(&project, inner)?;
    assert_eq!(
        flashes(&text),
        0,
        "explicit on removes an unconnected pad on a four-layer inner layer"
    );
    Ok(())
}

#[test]
fn an_unconnected_inner_via_pad_is_omitted_from_the_plot() -> Result<(), String> {
    let mut project = stack_layers(6);
    project.nets.push(Net {
        id: 1,
        name: "N1".into(),
        ..Default::default()
    });
    project.board.vias.push(Via {
        id: 11,
        net: 1,
        position: Point::new(15.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
    });
    project.validate()?;
    let inner = Layer::from_index(2)?;
    let text = pcbcraft_io::gerber_copper(&project, inner)?;
    assert_eq!(flashes(&text), 0, "unconnected via inner pad is omitted");
    let top = pcbcraft_io::gerber_copper(&project, Layer::Front)?;
    assert!(flashes(&top) >= 1, "via still flashes on the top end layer");
    let _ = flashes_per_aperture(&top);
    Ok(())
}

#[test]
fn pad_exports_on_layer_matches_gerber_for_a_footprint() -> Result<(), String> {
    let mut project = stack_layers(6);
    place_tht(&mut project, 10, 1, 12.0, 10.0);
    project.validate()?;
    let f = &project.board.footprints[0];
    let pad = &f.pads[0];
    let inner = Layer::from_index(2)?;
    assert!(
        !pad_exports_on_layer(&project, f, pad, inner),
        "export helper agrees the inner pad is skipped"
    );
    Ok(())
}
