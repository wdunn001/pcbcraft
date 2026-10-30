//! Gerber X2 aperture and file function fidelity (issue #655).
use pcbcraft_doc::{
    CopperZone, Footprint, Layer, LayerRecord, Net, Pad, Project, Via, ViaCovering, ZoneConnect,
    ZonePour,
};
use pcbcraft_geom::Point;
use pcbcraft_library::{Drill, FootprintPad};

fn smdef_pad() -> FootprintPad {
    FootprintPad {
        number: "1".into(),
        pad_type: "smd".into(),
        shape: "rect".into(),
        position: Point::default(),
        rotation: 0.0,
        size: Point::new(1.0, 0.6),
        layers: vec!["F.Mask".into(), "F.Paste".into()],
        drill: None,
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    }
}

#[test]
fn a_mask_defined_pad_uses_smdef_for_copper_and_mask() -> Result<(), String> {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "N1".into(),
        ..Default::default()
    });
    p.board.footprints.push(Footprint {
        id: 1,
        component: 0,
        position: Point::new(10.0, 10.0),
        rotation: 0.0,
        locked: None,
        side: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 1.0,
            net: Some(1),
            castellated: None,
            geometry: Some(smdef_pad()),
        }],
    });
    p.validate()?;
    let files = pcbcraft_io::gerber_files(&p)?;
    let cu = files
        .get("pcb-F-Cu.gbr")
        .ok_or("missing front copper")?;
    let mask = files
        .get("pcb-F.Mask.gbr")
        .ok_or("missing front mask")?;
    assert!(
        cu.contains("%TA.AperFunction,SMDPad,SMDef*%"),
        "copper names mask-defined pad"
    );
    assert!(
        mask.contains("%TA.AperFunction,SMDPad,SMDef*%"),
        "mask names mask-defined pad"
    );
    Ok(())
}

fn six_layer_plane_board() -> Result<Project, String> {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "GND".into(),
        ..Default::default()
    });
    p.nets.push(Net {
        id: 2,
        name: "SIG".into(),
        ..Default::default()
    });
    p.board.layer_records = vec![
        LayerRecord {
            name: "F.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: None,
            ..Default::default()
        },
        LayerRecord {
            name: "L2".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: Some("plane".into()),
            ..Default::default()
        },
        LayerRecord {
            name: "L3".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: None,
            ..Default::default()
        },
        LayerRecord {
            name: "B.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: None,
            ..Default::default()
        },
    ];
    p.board.zones.push(CopperZone {
        id: 1,
        net: 1,
        layer: Layer::from_index(1)?,
        outline: vec![
            Point::new(0.0, 0.0),
            Point::new(40.0, 0.0),
            Point::new(40.0, 30.0),
            Point::new(0.0, 30.0),
            Point::new(0.0, 0.0),
        ],
        cutouts: vec![],
        filled: true,
        kind: None,
        pour: Some(ZonePour {
            via_connect: ZoneConnect::Relief,
            ..Default::default()
        }),
    });
    p.board.vias.push(Via {
        id: 2,
        net: 2,
        position: Point::new(20.0, 15.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
    });
    p.validate()?;
    Ok(p)
}

#[test]
fn plane_layers_write_antipad_and_thermal_relief_pad() -> Result<(), String> {
    let p = six_layer_plane_board()?;
    let files = pcbcraft_io::gerber_files(&p)?;
    let l2 = files.get("pcb-L2.gbr").ok_or("missing L2 copper")?;
    assert!(
        l2.contains("%TA.AperFunction,AntiPad*%"),
        "foreign via clears antipad on plane layer"
    );
    assert!(
        l2.contains("%TA.AperFunction,ThermalReliefPad*%"),
        "same-net via uses thermal relief on plane layer"
    );
    Ok(())
}

#[test]
fn inner_through_hole_pads_are_component_pads() -> Result<(), String> {
    let mut p = six_layer_plane_board()?;
    p.board.footprints.push(Footprint {
        id: 3,
        component: 0,
        position: Point::new(5.0, 5.0),
        rotation: 0.0,
        locked: None,
        side: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 1.6,
            net: Some(1),
            castellated: None,
            geometry: Some(FootprintPad {
                number: "1".into(),
                pad_type: "thru_hole".into(),
                shape: "circle".into(),
                position: Point::default(),
                rotation: 0.0,
                size: Point::new(1.6, 1.6),
                layers: vec!["*.Cu".into()],
                drill: Some(Drill {
                    size: Point::new(0.8, 0.8),
                    offset: Point::default(),
                }),
                roundrect_ratio: None,
                chamfer: None,
                polygon: vec![],
                raw_source: String::new(),
            }),
        }],
    });
    p.validate()?;
    let files = pcbcraft_io::gerber_files(&p)?;
    let inner = files.get("pcb-L2.gbr").ok_or("missing inner copper")?;
    assert!(
        inner.contains("%TA.AperFunction,ComponentPad*%"),
        "through-hole pad on inner layer"
    );
    Ok(())
}

#[test]
fn each_mask_variant_gets_its_file_function_and_outward_index() -> Result<(), String> {
    let mut p = Project::default();
    p.board.layer_records = vec![
        LayerRecord {
            name: "F.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            ..Default::default()
        },
        LayerRecord {
            name: "B.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            ..Default::default()
        },
        LayerRecord {
            name: "F.Mask".into(),
            kind: "soldermask".into(),
            thickness: 0.01,
            side: Some("top".into()),
            ..Default::default()
        },
        LayerRecord {
            name: "F.Peel".into(),
            kind: "peelable_mask".into(),
            thickness: 0.01,
            side: Some("top".into()),
            ..Default::default()
        },
        LayerRecord {
            name: "B.Mask".into(),
            kind: "soldermask".into(),
            thickness: 0.01,
            side: Some("bottom".into()),
            ..Default::default()
        },
    ];
    p.validate()?;
    let files = pcbcraft_io::gerber_files(&p)?;
    let top_soldermask = files
        .get("pcb-F-Mask.gbr")
        .ok_or("top soldermask row")?;
    assert!(
        top_soldermask.contains("%TF.FileFunction,Soldermask,Top,1*%"),
        "first top mask from the board outward"
    );
    let peel = files.get("pcb-F-Peel.gbr").ok_or("peelable mask row")?;
    assert!(
        peel.contains("%TF.FileFunction,Peelablemask,Top,2*%"),
        "second top mask variant"
    );
    let bot = files.get("pcb-B-Mask.gbr").ok_or("bottom mask row")?;
    assert!(
        bot.contains("%TF.FileFunction,Soldermask,Bot,1*%"),
        "bottom soldermask index"
    );
    Ok(())
}

#[test]
fn every_gerber_file_carries_an_md5_the_reader_accepts() -> Result<(), String> {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "N1".into(),
        ..Default::default()
    });
    p.board.tracks.push(pcbcraft_doc::Track {
        id: 2,
        net: 1,
        start: Point::new(5.0, 5.0),
        end: Point::new(15.0, 5.0),
        width: 0.25,
        layer: Layer::Front,
        arc: None,
    });
    p.validate()?;
    let files = pcbcraft_io::gerber_files(&p)?;
    for (name, text) in &files {
        if !name.ends_with(".gbr") {
            continue;
        }
        pcbcraft_io::assert_gerber_md5_valid(text)?;
    }
    Ok(())
}

#[test]
fn filled_via_writes_a_viafill_file() -> Result<(), String> {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "N1".into(),
        ..Default::default()
    });
    p.board.vias.push(Via {
        id: 2,
        net: 1,
        position: Point::new(10.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: Some(ViaCovering::Filled),
    });
    p.validate()?;
    let files = pcbcraft_io::gerber_files(&p)?;
    let fill = files.get("pcb-Viafill.gbr").ok_or("missing viafill layer")?;
    assert!(fill.contains("%TF.FileFunction,Viafill*%"));
    pcbcraft_io::assert_gerber_md5_valid(fill)?;
    Ok(())
}
