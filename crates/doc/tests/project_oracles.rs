//! Project query, clearance, and schematic helper oracle tests.
use pcbcraft_doc::{
    component_template, fill::KeepoutKind, symbol_units, ArtKind, ArtLayer, ArtShape, BoardText,
    Component, Footprint, Layer, Net, NetClass, NetPairRule, Pad, Project,
    RuleArea, RuleRegion, SymbolUnitPlacement, Track, ViaCovering, ViaKind,
};
use pcbcraft_geom::Point;
use std::collections::BTreeSet;

type T = Result<(), String>;

#[test]
fn component_unit_placement_and_schematic_views_split_units() -> T {
    let mut c = component_template("resistor", 1, "U1".into(), Point::new(1.0, 2.0))
        .ok_or("template")?;
    c.units.push(SymbolUnitPlacement {
        unit: 2,
        position: Point::new(40.0, 0.0),
        rotation: 90.0,
    });
    let (pos, rot) = c.unit_placement(2).ok_or("unit 2")?;
    assert!((pos.x - 40.0).abs() < 1e-12 && (rot - 90.0).abs() < 1e-12);
    let views = c.schematic_views();
    assert_eq!(views.len(), 2);
    assert!(views.iter().any(|v| v.reference == "UB"));
    let points = c.drawing_points();
    assert!(!points.is_empty());
    Ok(())
}

#[test]
fn footprint_drawing_points_include_pad_outlines() -> T {
    let mut p = Project::default();
    p.components.push(
        component_template("resistor", 1, "R1".into(), Point::default()).ok_or("template")?,
    );
    let fp = Footprint {
        id: 2,
        component: 1,
        position: Point::default(),
        rotation: 0.0,
        locked: None,
        side: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 1.0,
            net: None,
            castellated: None,
            geometry: None,
        }],
    };
    let points = fp.drawing_points(&p);
    assert!(!points.is_empty());
    Ok(())
}

#[test]
fn footprint_pad_world_position_rotates_offsets() {
    let fp = Footprint {
        id: 1,
        component: 1,
        position: Point::new(10.0, 0.0),
        rotation: 90.0,
        locked: None,
        side: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::new(1.0, 0.0),
            diameter: 1.0,
            net: None,
            castellated: None,
            geometry: None,
        }],
    };
    let world = fp.pad_world_position(&fp.pads[0]);
    assert!((world.x - 10.0).abs() < 1e-9 && (world.y - 1.0).abs() < 1e-9);
    assert_eq!(fp.pad_position("1"), Some(world));
}

#[test]
fn track_curve_and_layer_labels_follow_geometry() {
    let track = Track {
        id: 1,
        net: 1,
        start: Point::new(0.0, 0.0),
        end: Point::new(2.0, 0.0),
        width: 0.2,
        layer: Layer::Front,
        arc: Some(Point::new(1.0, 1.0)),
    };
    let arc = track.curve().expect("midpoint defines an arc");
    assert!(arc.length() > track.start.distance(track.end));
    assert_eq!(Layer::Front.index(), 0);
    assert_eq!(Layer::Front.label(), "Top");
    assert_eq!(ViaKind::Buried.label(), "Buried");
    assert_eq!(ViaCovering::Filled.label(), "filled");
    assert_eq!(ArtKind::Mask.label(), "solder mask");
    assert_eq!(KeepoutKind::Pads.label(), "pad");
}

#[test]
fn art_shape_anchor_is_the_navigation_point() {
    let rect = ArtShape::Rect {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 4.0,
        rotate: 0.0,
    };
    assert_eq!(rect.anchor(), Point::new(5.0, 2.0));
}

#[test]
fn project_net_and_copper_layer_queries() -> T {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 3,
        name: "CLK".into(),
        ..Default::default()
    });
    assert_eq!(p.net_name(3), "CLK");
    assert_eq!(p.net_name(99), "unassigned");
    p.board.layer_records.push(pcbcraft_doc::LayerRecord {
        name: "L1.Cu".into(),
        kind: "copper".into(),
        thickness: 0.035,
        ..Default::default()
    });
    p.board.layer_records.push(pcbcraft_doc::LayerRecord {
        name: "Core".into(),
        kind: "core".into(),
        thickness: 1.0,
        ..Default::default()
    });
    p.board.layer_records.push(pcbcraft_doc::LayerRecord {
        name: "L2.Cu".into(),
        kind: "copper".into(),
        thickness: 0.035,
        ..Default::default()
    });
    let copper: Vec<_> = p.copper_layer_records().collect();
    assert_eq!(copper.len(), 2);
    assert_eq!(p.copper_layers().len(), 2);
    assert_eq!(p.layer_name(Layer::Front), "L1.Cu");
    p.board.tracks.push(Track {
        id: 5,
        net: 1,
        start: Point::default(),
        end: Point::new(1.0, 0.0),
        width: 0.2,
        layer: Layer::Back,
        arc: None,
    });
    assert_eq!(p.max_copper_layer_index_used(), Some(1));
    let reason = p.stack_shrink_blocked_reason(1).expect("blocked");
    assert!(reason.contains("track"));
    Ok(())
}

#[test]
fn net_class_and_rule_overrides() -> T {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "A".into(),
        ..Default::default()
    });
    p.nets.push(Net {
        id: 2,
        name: "B".into(),
        ..Default::default()
    });
    p.net_classes.push(NetClass {
        id: 10,
        name: "fast".into(),
        clearance: 0.5,
        width: 0.2,
        min_width: None,
        max_width: None,
        target_length: None,
        length_tolerance: None,
        max_stub: None,
        via_diameter: None,
        via_drill: None,
        pair_width: None,
        pair_gap: None,
        nets: vec![1],
        profile: None,
    });
    assert_eq!(p.net_class_id(1), Some(10));
    assert!(p.net_class(2).is_none());
    p.rule_severity.insert("short".into(), "warning".into());
    assert_eq!(p.rule_severity("short", "error"), "warning");
    assert_eq!(p.rule_severity("open", "error"), "error");
    p.rule_severity.insert("quiet".into(), "off".into());
    assert!(p.rule_disabled("quiet"));
    p.rule_exclusions.insert("short".into(), vec![7, 8]);
    assert!(p.rule_excluded("short", &[7, 8]));
    assert!(!p.rule_excluded("short", &[7]));
    p.rule_regions.insert(
        "short".into(),
        vec![RuleRegion {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        }],
    );
    p.board.tracks.push(Track {
        id: 7,
        net: 1,
        start: Point::new(1.0, 1.0),
        end: Point::new(2.0, 1.0),
        width: 0.2,
        layer: Layer::Front,
        arc: None,
    });
    assert!(p.rule_region_excluded("short", &[7]));
    Ok(())
}

#[test]
fn required_clearance_takes_the_strictest_rule() -> T {
    let mut p = Project::default();
    p.board.clearance = 0.15;
    p.nets.extend([
        Net {
            id: 1,
            name: "A".into(),
            ..Default::default()
        },
        Net {
            id: 2,
            name: "B".into(),
            ..Default::default()
        },
    ]);
    p.net_classes.push(NetClass {
        id: 10,
        name: "wide".into(),
        clearance: 0.4,
        width: 0.2,
        min_width: None,
        max_width: None,
        target_length: None,
        length_tolerance: None,
        max_stub: None,
        via_diameter: None,
        via_drill: None,
        pair_width: None,
        pair_gap: None,
        nets: vec![1],
        profile: None,
    });
    assert!((p.required_clearance(1, 2) - 0.4).abs() < 1e-12);
    p.board.pair_rules.push(NetPairRule {
        a: 1,
        b: 2,
        clearance: 0.6,
    });
    assert!((p.required_clearance(1, 2) - 0.6).abs() < 1e-12);
    p.board.rule_areas.push(RuleArea {
        id: 1,
        name: None,
        net: None,
        layer: None,
        clearance: 0.9,
        keepout: None,
        max_height: None,
        region: RuleRegion {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        profile: None,
    });
    let at = Point::new(5.0, 5.0);
    assert!((p.required_clearance_at(1, 2, at, at, Layer::Front, Layer::Front) - 0.9).abs() < 1e-12);
    Ok(())
}

#[test]
fn object_position_user_layer_and_ids() -> T {
    let mut p = Project::default();
    p.components.push(Component {
        id: 4,
        reference: "R1".into(),
        value: "1k".into(),
        kind: "resistor".into(),
        position: Point::new(3.0, 4.0),
        rotation: 0.0,
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sheet: 0,
        pins: vec![],
        units: vec![],
    });
    assert_eq!(p.object_position(4), Some(Point::new(3.0, 4.0)));
    p.board.user_layers.push(ArtLayer {
        id: 20,
        name: "Outline".into(),
        kind: ArtKind::Cut,
        dotted: false,
        dot_pitch: 1.0,
        dot_size: 0.5,
        visible: true,
        shapes: vec![ArtShape::Line {
            start: Point::new(1.0, 2.0),
            end: Point::new(3.0, 4.0),
        }],
    });
    assert!(p.user_layer(20).is_some());
    p.board.texts.push(BoardText {
        id: 30,
        text: "rev A".into(),
        position: Point::new(1.0, 1.0),
        size: 1.0,
        layer: Layer::Front,
        rotation: 0.0,
        component: None,
    });
    let ids: BTreeSet<_> = p.ids().collect();
    assert!(ids.contains(&4) && ids.contains(&20) && ids.contains(&30));
    Ok(())
}

#[test]
fn symbol_units_collects_pin_and_graphic_units() -> T {
    let symbol = pcbcraft_library::read_symbols(include_str!(
        "../../library/tests/fixtures/device.kicad_sym"
    ))
    .map_err(|e| e.to_string())?
    .into_iter()
    .find(|s| s.name == "R")
    .ok_or("missing R")?;
    let units = symbol_units(&symbol)?;
    assert!(units.contains(&1));
    Ok(())
}
