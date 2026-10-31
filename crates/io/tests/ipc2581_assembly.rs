//! IPC-2581C assembly export tests (issue #661).

use pcbcraft_doc::{
    Component, ComponentField, Footprint, Layer, LibraryFootprint, Pad, Pin, Project,
};
use pcbcraft_geom::Point;
use pcbcraft_io::ipc2581_text;
use pcbcraft_library::FootprintDefinition;

fn smd_pad(number: &str, x: f64) -> pcbcraft_library::FootprintPad {
    pcbcraft_library::FootprintPad {
        number: number.into(),
        pad_type: "smd".into(),
        shape: "rect".into(),
        position: Point::new(x, 0.0),
        rotation: 0.0,
        size: Point::new(1.0, 1.0),
        layers: vec!["F.Cu".into(), "F.Mask".into()],
        drill: None,
        roundrect_ratio: None,
        raw_source: String::new(),
        chamfer: None,
        polygon: Vec::new(),
    }
}

fn th_pad(number: &str, x: f64) -> pcbcraft_library::FootprintPad {
    pcbcraft_library::FootprintPad {
        number: number.into(),
        pad_type: "thru_hole".into(),
        shape: "circle".into(),
        position: Point::new(x, 0.0),
        rotation: 0.0,
        size: Point::new(1.2, 1.2),
        layers: vec!["F.Cu".into(), "B.Cu".into()],
        drill: Some(pcbcraft_library::Drill {
            size: Point::new(0.8, 0.8),
            offset: Point::default(),
        }),
        roundrect_ratio: None,
        raw_source: String::new(),
        chamfer: None,
        polygon: Vec::new(),
    }
}

fn placed_board() -> Result<Project, String> {
    let mut p = Project::default();
    p.name = "demo".into();
    let fp_def = |name: &str, smd: bool| FootprintDefinition {
        name: name.into(),
        description: String::new(),
        keywords: String::new(),
        pads: if smd {
            vec![smd_pad("1", 0.0)]
        } else {
            vec![th_pad("1", 0.0)]
        },
        graphics: vec![],
        model: None,
        rules: None,
        raw_source: String::new(),
        unsupported: vec![],
    };
    p.components.push(Component {
        sheet: 0,
        id: 1,
        reference: "R1".into(),
        value: "10k".into(),
        kind: "resistor".into(),
        position: Point::new(5.0, 5.0),
        rotation: 0.0,
        pins: vec![Pin {
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
        library_footprint: Some(LibraryFootprint {
            library: "Test".into(),
            definition: fp_def("R0603", true),
        }),
        fields: vec![
            ComponentField {
                key: "Manufacturer".into(),
                value: "Acme".into(),
            },
            ComponentField {
                key: "MPN".into(),
                value: "R0603-10K".into(),
            },
            ComponentField {
                key: "Moisture level".into(),
                value: "3".into(),
            },
            ComponentField {
                key: "Shop PN".into(),
                value: "123-ABC".into(),
            },
        ],
    });
    p.components.push(Component {
        sheet: 0,
        id: 2,
        reference: "J1".into(),
        value: "HDR".into(),
        kind: "connector".into(),
        position: Point::new(15.0, 5.0),
        rotation: 90.0,
        pins: vec![Pin {
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
        library_footprint: Some(LibraryFootprint {
            library: "Test".into(),
            definition: fp_def("HDR1", false),
        }),
        fields: vec![],
    });
    p.board.footprints.push(Footprint {
        id: 10,
        component: 1,
        position: Point::new(10.0, 10.0),
        rotation: 45.0,
        locked: None,
        side: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 1.0,
            net: None,
            castellated: None,
            geometry: Some(smd_pad("1", 0.0)),
        }],
    });
    p.board.footprints.push(Footprint {
        id: 11,
        component: 2,
        position: Point::new(20.0, 12.0),
        rotation: 90.0,
        locked: None,
        side: Some(Layer::Back),
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 1.2,
            net: None,
            castellated: None,
            geometry: Some(th_pad("1", 0.0)),
        }],
    });
    p.validate()?;
    Ok(p)
}

#[test]
fn the_output_with_assembly_data_validates_against_the_schema() -> Result<(), String> {
    let p = placed_board()?;
    let text = ipc2581_text(&p)?;
    ipc2581::validate(&text).map_err(|e| e.to_string())?;
    assert!(text.contains("<Bom name=\"BOM\">"));
    assert!(text.contains("<Avl name=\"Primary_Vendor_List\">"));
    Ok(())
}

#[test]
fn each_component_appears_once_with_mount_type_and_layer() -> Result<(), String> {
    let p = placed_board()?;
    let text = ipc2581_text(&p)?;
    ipc2581::validate(&text).map_err(|e| e.to_string())?;
    assert_eq!(text.matches("refDes=\"R1\"").count(), 1);
    assert_eq!(text.matches("refDes=\"J1\"").count(), 1);
    assert!(
        text.contains("<Component refDes=\"R1\"")
            && text.contains("mountType=\"SMT\"")
            && text.contains("layerRef=\"F.Cu\"")
    );
    assert!(
        text.contains("<Component refDes=\"J1\"")
            && text.contains("mountType=\"THMT\"")
            && text.contains("layerRef=\"B.Cu\"")
    );
    assert!(text.contains("<Xform rotation=\"45.000000\"/>"));
    Ok(())
}

#[test]
fn a_board_without_sourcing_data_writes_no_vendor_list() -> Result<(), String> {
    let mut p = placed_board()?;
    for c in &mut p.components {
        c.fields.retain(|f| {
            !matches!(
                f.key.as_str(),
                "Manufacturer" | "MPN" | "Moisture level" | "Shop PN"
            )
        });
    }
    p.validate()?;
    let text = ipc2581_text(&p)?;
    ipc2581::validate(&text).map_err(|e| e.to_string())?;
    assert!(text.contains("<Bom name=\"BOM\">"));
    assert!(!text.contains("<Avl "));
    assert!(!text.contains("<AvlRef"));
    Ok(())
}
