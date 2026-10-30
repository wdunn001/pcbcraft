//! Centroid profiles and Gerber component layers for assembly exports.

use pcbcraft_doc::{Component, Footprint, Layer, Pin, Project};
use pcbcraft_geom::Point;
use pcbcraft_io::{
    AssemblyPlacementProfile, BottomRotationPolicy, PlacementColumn, PlacementField,
    PlacementUnits, export_rotation, gerber_component_layers, placement_text,
    placement_text_with_profile,
};
use pcbcraft_library::FootprintPad;

fn pad(kind: &str, layers: &[&str]) -> FootprintPad {
    FootprintPad {
        number: "1".into(),
        pad_type: kind.into(),
        shape: "rect".into(),
        position: Point::default(),
        rotation: 0.0,
        size: Point::new(1.0, 1.0),
        layers: layers.iter().map(|s| (*s).into()).collect(),
        drill: if kind == "thru_hole" {
            Some(pcbcraft_library::Drill {
                size: Point::new(0.6, 0.6),
                offset: Point::default(),
            })
        } else {
            None
        },
        roundrect_ratio: None,
        raw_source: String::new(),
        chamfer: None,
        polygon: Vec::new(),
    }
}

fn add_part(
    p: &mut Project,
    id: u64,
    reference: &str,
    footprint: &str,
    at: (f64, f64, f64),
    pad_kind: (&str, &[&str]),
    side: Option<Layer>,
) {
    p.components.push(Component {
        sheet: 0,
        id,
        reference: reference.into(),
        value: "1k".into(),
        kind: "resistor".into(),
        position: Point::default(),
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
        library_footprint: Some(pcbcraft_doc::LibraryFootprint {
            library: "TestLib".into(),
            definition: pcbcraft_library::FootprintDefinition {
                name: footprint.into(),
                description: String::new(),
                keywords: String::new(),
                pads: vec![pad(pad_kind.0, pad_kind.1)],
                graphics: vec![],
                model: None,
                rules: None,
                raw_source: String::new(),
                unsupported: vec![],
            },
        }),
        fields: vec![
            pcbcraft_doc::ComponentField {
                key: "MPN".into(),
                value: format!("MPN-{reference}"),
            },
            pcbcraft_doc::ComponentField {
                key: "manufacturer".into(),
                value: "Acme".into(),
            },
        ],
    });
    p.board.footprints.push(Footprint {
        id: id + 100,
        component: id,
        position: Point::new(at.0, at.1),
        rotation: at.2,
        locked: None,
        side,
        pads: vec![pcbcraft_doc::Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 1.0,
            net: None,
            castellated: None,
            geometry: Some(pad(pad_kind.0, pad_kind.1)),
        }],
    });
}

fn two_side_board() -> Result<Project, String> {
    let mut p = Project::default();
    p.board.width = 50.0;
    p.board.height = 40.0;
    add_part(
        &mut p,
        1,
        "R1",
        "0603",
        (10.0, 5.0, 30.0),
        ("smd", &["F.Cu"]),
        None,
    );
    add_part(
        &mut p,
        2,
        "R2",
        "0603",
        (20.0, 8.0, 45.0),
        ("smd", &["B.Cu"]),
        Some(Layer::Back),
    );
    p.validate()?;
    Ok(p)
}

#[test]
fn the_default_profile_writes_the_same_file_as_before() -> Result<(), String> {
    let project = two_side_board()?;
    let legacy = placement_text(&project)?;
    let profiled = placement_text_with_profile(&project, None)?;
    assert_eq!(legacy, profiled);
    assert!(legacy.contains("SMT,THT,X,Y,Rotation,Side,Package\n"));
    Ok(())
}

#[test]
fn a_profile_renames_and_reorders_columns() -> Result<(), String> {
    let project = two_side_board()?;
    let profile = AssemblyPlacementProfile {
        columns: vec![
            PlacementColumn {
                field: PlacementField::Reference,
                header: "Designator".into(),
            },
            PlacementColumn {
                field: PlacementField::MountType,
                header: "Kind".into(),
            },
            PlacementColumn {
                field: PlacementField::X,
                header: "MidX".into(),
            },
            PlacementColumn {
                field: PlacementField::Y,
                header: "MidY".into(),
            },
            PlacementColumn {
                field: PlacementField::Rotation,
                header: "Angle".into(),
            },
            PlacementColumn {
                field: PlacementField::Side,
                header: "Layer".into(),
            },
            PlacementColumn {
                field: PlacementField::Package,
                header: "Footprint".into(),
            },
        ],
        units: PlacementUnits::Inches,
        ..AssemblyPlacementProfile::legacy_default()
    };
    let text = placement_text_with_profile(&project, Some(&profile))?;
    assert!(text.contains("Designator,Kind,MidX,MidY,Angle,Layer,Footprint\n"));
    assert!(text.contains("\"R1\",SMT,0.3937,0.1969,30.0,T,\"0603\""));
    Ok(())
}

#[test]
fn a_bottom_rotation_policy_changes_only_bottom_parts() -> Result<(), String> {
    let project = two_side_board()?;
    let base = placement_text_with_profile(&project, None)?;
    let profile = AssemblyPlacementProfile {
        bottom_rotation: BottomRotationPolicy::NegateOnBottom,
        ..AssemblyPlacementProfile::legacy_default()
    };
    let adjusted = placement_text_with_profile(&project, Some(&profile))?;
    assert_ne!(base, adjusted);
    assert!(base.contains("SMT,\"R1\",10.000,5.000,30.0,T,\"0603\""));
    assert!(adjusted.contains("SMT,\"R2\",20.000,8.000,315.0,B,\"0603\""));
    assert!(base.contains("SMT,\"R2\",20.000,8.000,45.0,B,\"0603\""));
    assert_eq!(export_rotation(45.0, true, &profile), 315.0);
    assert_eq!(export_rotation(45.0, false, &profile), 45.0);
    Ok(())
}

#[test]
fn the_component_layer_round_trips_through_the_gerber_reader() -> Result<(), String> {
    let project = two_side_board()?;
    let profile = AssemblyPlacementProfile {
        bottom_rotation: BottomRotationPolicy::NegateOnBottom,
        ..AssemblyPlacementProfile::legacy_default()
    };
    let files = gerber_component_layers(&project, Some(&profile))?;
    let top = files
        .get("pcb-F.Cmp.gbr")
        .ok_or("top component layer missing")?;
    let bottom = files
        .get("pcb-B.Cmp.gbr")
        .ok_or("bottom component layer missing")?;
    let top_parts = gerber_component_reader::component_flashes(top)?;
    let bottom_parts = gerber_component_reader::component_flashes(bottom)?;
    assert_eq!(top_parts.len(), 1);
    assert_eq!(bottom_parts.len(), 1);
    assert_eq!(top_parts[0].reference, "R1");
    assert_eq!(top_parts[0].rotation, 30.0);
    assert_eq!(top_parts[0].mpn, "MPN-R1");
    assert_eq!(top_parts[0].manufacturer, "Acme");
    assert_eq!(top_parts[0].mount, "SMD");
    assert_eq!(bottom_parts[0].reference, "R2");
    assert_eq!(bottom_parts[0].rotation, 315.0);
    Ok(())
}

#[path = "gerber_component_reader.rs"]
mod gerber_component_reader;
