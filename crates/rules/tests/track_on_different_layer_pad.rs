//! A track that ends on a pad its layer does not reach makes a connection that does not exist.
//!
//! Story: a designer routes a board with surface-mount parts on the front. A bottom track that
//! ends on a front-only pad reads as routed but is dead copper, and the checks must say so. They
//! must stay quiet when the pad does have copper on the track's layer: a through-hole pad has it
//! on every layer, and a via joins the layers.
//!
//! # Dependencies
//! - crates/rules/src/lib.rs (`drc`, the `track_on_different_layer_pad` finding)
//! - crates/doc (the project model) and crates/library (footprint pad geometry)
//!
//! # Do not
//! - Weaken an assertion to make a board pass; fix the check instead.

use pcbcraft_doc::{
    Footprint, Layer, LayerRecord, LibraryFootprint, Net, Pad, Project, Track, Via,
    component_template,
};
use pcbcraft_geom::Point;
use pcbcraft_library::{Drill, FootprintDefinition, FootprintPad};
use pcbcraft_rules::{Finding, drc};

type T = Result<(), String>;

const CODE: &str = "track_on_different_layer_pad";
const PAD_AT: Point = Point { x: 20.0, y: 20.0 };

/// Describes the one pad on the test board.
struct PadSpec {
    pad_type: &'static str,
    layers: &'static [&'static str],
    net: u64,
}

const FRONT_SMD: PadSpec = PadSpec {
    pad_type: "smd",
    layers: &["F.Cu", "F.Paste", "F.Mask"],
    net: 1,
};

const THROUGH_HOLE: PadSpec = PadSpec {
    pad_type: "thru_hole",
    layers: &["*.Cu", "*.Mask"],
    net: 1,
};

fn copper_layer(name: &str, side: &str) -> LayerRecord {
    LayerRecord {
        name: name.into(),
        kind: "copper".into(),
        thickness: 0.035,
        side: Some(side.into()),
        ..LayerRecord::default()
    }
}

/// A board with nets 1 and 2, one component U1 with one pad at `PAD_AT`, and no tracks.
fn board(spec: &PadSpec, four_layers: bool) -> Result<Project, String> {
    let mut p = Project::default();
    if four_layers {
        p.board.layer_records = vec![
            copper_layer("F.Cu", "top"),
            copper_layer("In1.Cu", "internal"),
            copper_layer("In2.Cu", "internal"),
            copper_layer("B.Cu", "bottom"),
        ];
    }
    for (id, name) in [(1, "SIG"), (2, "OTHER")] {
        p.nets.push(Net {
            id,
            name: name.into(),
            ..Default::default()
        });
    }
    let mut component =
        component_template("resistor", 3, "U1".into(), PAD_AT).ok_or("component template")?;
    component.pins.truncate(1);
    component.pins[0].net = Some(spec.net);
    let geometry = FootprintPad {
        number: "1".into(),
        shape: "circle".into(),
        pad_type: spec.pad_type.into(),
        position: Point::default(),
        rotation: 0.0,
        size: Point::new(1.2, 1.2),
        layers: spec.layers.iter().map(|l| (*l).to_string()).collect(),
        drill: (spec.pad_type == "thru_hole").then(|| Drill {
            size: Point::new(0.6, 0.6),
            offset: Point::default(),
        }),
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    };
    component.library_footprint = Some(LibraryFootprint {
        library: "test".into(),
        definition: FootprintDefinition {
            name: "pad".into(),
            pads: vec![geometry.clone()],
            ..Default::default()
        },
    });
    p.components.push(component);
    p.board.footprints.push(Footprint {
        id: 10,
        component: 3,
        position: PAD_AT,
        rotation: 0.0,
        locked: None,
        side: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            net: Some(spec.net),
            diameter: 1.2,
            castellated: None,
            geometry: Some(geometry),
        }],
        preserved: Default::default(),
    });
    Ok(p)
}

/// Adds a track on `layer` for net 1 that runs from 5 mm to the left of the pad to `end`.
fn add_track(p: &mut Project, id: u64, layer: Layer, end: Point) {
    p.board.tracks.push(Track {
        id,
        net: 1,
        start: Point::new(PAD_AT.x - 5.0, PAD_AT.y),
        end,
        width: 0.2,
        layer,
        arc: None,
        preserved: Default::default(),
    });
}

fn layer_findings(p: &Project) -> Vec<Finding> {
    drc(p).into_iter().filter(|f| f.code == CODE).collect()
}

#[test]
fn track_ending_on_a_pad_its_layer_does_not_reach_is_flagged() -> T {
    let mut p = board(&FRONT_SMD, false)?;
    add_track(&mut p, 12, Layer::Back, PAD_AT);
    p.validate()?;
    let found = layer_findings(&p);
    assert_eq!(found.len(), 1, "one finding: {found:?}");
    let finding = &found[0];
    assert_eq!(finding.severity, "error", "the board cannot be built");
    for word in ["SIG", "B.Cu", "U1.1", "F.Cu"] {
        assert!(
            finding.message.contains(word),
            "the message names {word:?}: {}",
            finding.message
        );
    }
    assert!(finding.objects.contains(&12), "names the track");
    assert!(finding.objects.contains(&10), "names the pad's footprint");
    Ok(())
}

#[test]
fn track_on_the_pads_own_layer_is_clean() -> T {
    let mut p = board(&FRONT_SMD, false)?;
    add_track(&mut p, 12, Layer::Front, PAD_AT);
    p.validate()?;
    assert!(layer_findings(&p).is_empty(), "{:?}", layer_findings(&p));
    Ok(())
}

#[test]
fn through_hole_pad_accepts_tracks_on_both_outer_layers() -> T {
    let mut p = board(&THROUGH_HOLE, false)?;
    add_track(&mut p, 12, Layer::Front, PAD_AT);
    add_track(&mut p, 13, Layer::Back, PAD_AT);
    p.validate()?;
    assert!(layer_findings(&p).is_empty(), "{:?}", layer_findings(&p));
    Ok(())
}

#[test]
fn through_hole_pad_accepts_a_track_on_an_inner_layer() -> T {
    let mut p = board(&THROUGH_HOLE, true)?;
    let inner = Layer::from_index(1).map_err(|e| e.to_string())?;
    add_track(&mut p, 12, inner, PAD_AT);
    p.validate()?;
    assert!(layer_findings(&p).is_empty(), "{:?}", layer_findings(&p));
    Ok(())
}

#[test]
fn track_ending_on_a_via_over_the_pad_is_clean() -> T {
    let mut p = board(&FRONT_SMD, false)?;
    p.board.vias.push(Via {
        id: 11,
        net: 1,
        position: PAD_AT,
        diameter: 0.6,
        drill: 0.1,
        preserved: Default::default(),
        from_layer: None,
        to_layer: None,
        covering: None,
        covering_bottom: None,
        backdrill: None,
    });
    add_track(&mut p, 12, Layer::Back, PAD_AT);
    p.validate()?;
    assert!(layer_findings(&p).is_empty(), "{:?}", layer_findings(&p));
    Ok(())
}

#[test]
fn pad_of_another_net_is_not_this_tracks_business() -> T {
    let mut p = board(
        &PadSpec {
            net: 2,
            ..FRONT_SMD
        },
        false,
    )?;
    add_track(&mut p, 12, Layer::Back, PAD_AT);
    p.validate()?;
    assert!(layer_findings(&p).is_empty(), "{:?}", layer_findings(&p));
    Ok(())
}

#[test]
fn track_ending_well_away_from_the_pad_is_clean() -> T {
    let mut p = board(&FRONT_SMD, false)?;
    add_track(
        &mut p,
        12,
        Layer::Back,
        Point::new(PAD_AT.x - 2.0, PAD_AT.y),
    );
    p.validate()?;
    assert!(layer_findings(&p).is_empty(), "{:?}", layer_findings(&p));
    Ok(())
}

#[test]
fn a_track_with_both_ends_on_front_only_pads_is_flagged_at_each_end() -> T {
    let mut p = board(&FRONT_SMD, false)?;
    // Both ends lie on the 1.2 mm pad, so each end is its own finding.
    p.board.tracks.push(Track {
        id: 12,
        net: 1,
        start: PAD_AT,
        end: Point::new(PAD_AT.x + 0.3, PAD_AT.y),
        width: 0.2,
        layer: Layer::Back,
        arc: None,
        preserved: Default::default(),
    });
    p.validate()?;
    assert_eq!(layer_findings(&p).len(), 2, "{:?}", layer_findings(&p));
    Ok(())
}
