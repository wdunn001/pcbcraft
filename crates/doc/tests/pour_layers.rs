//! Six-layer fixture for the pour and keepout obstacle lists (issue #800).
//!
//! Each test builds a small six-layer board and asks which objects the pour
//! sees on each copper layer. A via is copper only on the layers of its span, a
//! surface pad only on the face its part is soldered on, a through-hole pad on
//! every layer.

use pcbcraft_doc::{
    CopperZone, Footprint, Id, Keepout, Layer, LayerRecord, Net, Pad, Project, RuleArea,
    RuleRegion, Via, keepout_hits, layer_obstacles, zone_fill,
};
use pcbcraft_geom::Point;

const BLIND: Id = 10;
const BURIED: Id = 11;
const THROUGH: Id = 12;
const TOP_PART: Id = 20;
const THRU_PART: Id = 21;
const BOTTOM_PART: Id = 22;

fn layer(index: u8) -> Result<Layer, String> {
    Layer::from_index(index)
}

fn copper(name: &str) -> LayerRecord {
    LayerRecord {
        name: name.into(),
        kind: "copper".into(),
        thickness: 0.035,
        copper_role: None,
        ..LayerRecord::default()
    }
}

fn via(id: Id, net: Id, x: f64, from: Option<u8>, to: Option<u8>) -> Result<Via, String> {
    Ok(Via {
        id,
        net,
        position: Point::new(x, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: from.map(layer).transpose()?,
        to_layer: to.map(layer).transpose()?,
        covering: None,
        backdrill: None,
    })
}

fn part(id: Id, net: Id, x: f64, thru: bool, side: Option<Layer>) -> Footprint {
    let geometry = pcbcraft_library::FootprintPad {
        number: "1".into(),
        pad_type: if thru { "thru_hole" } else { "smd" }.into(),
        shape: "circle".into(),
        position: Point::default(),
        rotation: 0.0,
        size: Point::new(1.6, 1.6),
        layers: if thru {
            vec!["*.Cu".into()]
        } else {
            vec!["F.Cu".into()]
        },
        drill: thru.then(|| pcbcraft_library::Drill {
            size: Point::new(0.8, 0.8),
            offset: Point::default(),
        }),
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    };
    Footprint {
        id,
        component: 0,
        position: Point::new(x, 30.0),
        rotation: 0.0,
        locked: None,
        side,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 1.6,
            net: Some(net),
            castellated: None,
            geometry: Some(geometry),
        }],
    }
}

fn square(id: Id, net: Id, index: u8) -> Result<CopperZone, String> {
    Ok(CopperZone {
        id,
        net,
        layer: layer(index)?,
        outline: vec![
            Point::new(1.0, 1.0),
            Point::new(59.0, 1.0),
            Point::new(59.0, 39.0),
            Point::new(1.0, 39.0),
            Point::new(1.0, 1.0),
        ],
        cutouts: vec![],
        filled: true,
        kind: None,
        pour: None,
    })
}

/// Six copper layers, three vias, one surface pad on each face and one
/// through-hole pad. Zone ids 31 to 34 sit on layers 2 to 5.
fn board() -> Result<Project, String> {
    let mut p = Project::default();
    for (id, name) in [
        (1, "BLIND"),
        (2, "BURIED"),
        (3, "THROUGH"),
        (4, "TOP"),
        (5, "THRU"),
        (6, "BOT"),
    ] {
        p.nets.push(Net {
            id,
            name: name.into(),
            ..Default::default()
        });
    }
    p.board.layer_records = ["F.Cu", "In1.Cu", "In2.Cu", "In3.Cu", "In4.Cu", "B.Cu"]
        .iter()
        .map(|name| copper(name))
        .collect();
    p.board.vias.push(via(BLIND, 1, 10.0, Some(0), Some(2))?);
    p.board.vias.push(via(BURIED, 2, 20.0, Some(2), Some(4))?);
    p.board.vias.push(via(THROUGH, 3, 30.0, None, None)?);
    p.board
        .footprints
        .push(part(TOP_PART, 4, 10.0, false, None));
    p.board
        .footprints
        .push(part(THRU_PART, 5, 30.0, true, None));
    p.board
        .footprints
        .push(part(BOTTOM_PART, 6, 45.0, false, Some(Layer::Back)));
    for (id, index) in [(31, 1), (32, 2), (33, 3), (34, 4)] {
        p.board.zones.push(square(id, 9, index)?);
    }
    Ok(p)
}

fn sorted(mut ids: Vec<Id>) -> Vec<Id> {
    ids.sort_unstable();
    ids
}

fn on(project: &Project, index: u8) -> Result<Vec<Id>, String> {
    let ids = layer_obstacles(project, layer(index)?)?;
    // The zones themselves are listed on their own layer; the test is about
    // the vias and pads, so drop the zone ids.
    Ok(sorted(
        ids.into_iter()
            .filter(|id| !(31..=34).contains(id))
            .collect(),
    ))
}

#[test]
fn six_layer_fixture_lists_the_right_obstacles_on_every_layer() -> Result<(), String> {
    let p = board()?;
    assert_eq!(on(&p, 0)?, vec![BLIND, THROUGH, TOP_PART, THRU_PART], "L1");
    assert_eq!(on(&p, 1)?, vec![BLIND, THROUGH, THRU_PART], "L2");
    assert_eq!(on(&p, 2)?, vec![BLIND, BURIED, THROUGH, THRU_PART], "L3");
    assert_eq!(on(&p, 3)?, vec![BURIED, THROUGH, THRU_PART], "L4");
    assert_eq!(on(&p, 4)?, vec![BURIED, THROUGH, THRU_PART], "L5");
    assert_eq!(on(&p, 5)?, vec![THROUGH, THRU_PART, BOTTOM_PART], "L6");
    Ok(())
}

#[test]
fn a_blind_via_does_not_touch_a_pour_below_its_span() -> Result<(), String> {
    let mut p = board()?;
    // The pour on L4 carries the blind via's foreign net; the via ends at L3.
    let zone = p.board.zones[2].clone();
    assert_eq!(zone.layer, layer(3)?);
    let fill = zone_fill(&p, &zone)?;
    assert!(
        fill.contains(Point::new(10.0, 10.0)),
        "L4 pour is whole above the blind via"
    );
    // The same via, stretched to L5, now cuts an antipad into the L4 pour.
    p.board.vias[0].to_layer = Some(layer(4)?);
    let fill = zone_fill(&p, &zone)?;
    assert!(
        !fill.contains(Point::new(10.0, 10.0)),
        "a via through L4 clears an antipad"
    );
    Ok(())
}

#[test]
fn a_surface_pad_is_not_an_obstacle_on_the_other_face() -> Result<(), String> {
    let p = board()?;
    for index in 1..=5 {
        assert!(
            !on(&p, index)?.contains(&TOP_PART),
            "top pad on L{}",
            index + 1
        );
    }
    for index in 0..=4 {
        assert!(
            !on(&p, index)?.contains(&BOTTOM_PART),
            "bottom pad on L{}",
            index + 1
        );
    }
    for index in 0..=5 {
        assert!(
            on(&p, index)?.contains(&THRU_PART),
            "through-hole pad on L{}",
            index + 1
        );
    }
    Ok(())
}

#[test]
fn the_bottom_of_a_six_layer_board_is_the_last_layer_not_index_one() -> Result<(), String> {
    let p = board()?;
    // Index one is the second copper layer: a bottom-face pad must not be
    // listed there, and an unplated surface pad must not be listed on L6.
    assert!(!on(&p, 1)?.contains(&BOTTOM_PART));
    assert!(on(&p, 5)?.contains(&BOTTOM_PART));
    assert!(!on(&p, 5)?.contains(&TOP_PART));
    Ok(())
}

#[test]
fn a_foreign_via_gets_an_antipad_and_a_same_net_via_a_thermal_connection() -> Result<(), String> {
    let mut p = board()?;
    // L3 pour on the buried via's own net: the via joins it.
    assert_eq!(p.board.zones[1].layer, layer(2)?);
    p.board.zones[1].net = 2;
    let zone = p.board.zones[1].clone();
    let own = zone_fill(&p, &zone)?;
    assert!(
        own.contains(Point::new(20.0, 10.0)),
        "same-net via joins the pour"
    );
    // The same pour on another net clears an antipad around it.
    p.board.zones[1].net = 9;
    let zone = p.board.zones[1].clone();
    let foreign = zone_fill(&p, &zone)?;
    assert!(
        !foreign.contains(Point::new(20.0, 10.0)),
        "foreign via gets an antipad"
    );
    // Thermal connection: spokes instead of solid.
    p.board.zones[1].net = 2;
    p.board.zones[1].pour = Some(pcbcraft_doc::ZonePour {
        via_connect: pcbcraft_doc::ZoneConnect::Relief,
        ..Default::default()
    });
    let zone = p.board.zones[1].clone();
    let thermal = zone_fill(&p, &zone)?;
    assert!(thermal.spokes > 0, "a relief connection reports spokes");
    Ok(())
}

#[test]
fn keepout_hits_cover_inner_layers() -> Result<(), String> {
    let mut p = board()?;
    // A keepout that only bites on L4 hits the buried via (it conducts there)
    // and the through via, never the blind via that ends at L3.
    p.board.rule_areas.push(RuleArea {
        id: 40,
        name: None,
        net: None,
        layer: Some(layer(3)?),
        clearance: 0.0,
        keepout: Some(Keepout {
            tracks: true,
            vias: true,
            pads: true,
            zones: false,
        }),
        max_height: None,
        region: RuleRegion {
            x: 0.0,
            y: 5.0,
            width: 60.0,
            height: 10.0,
        },
        profile: None,
    });
    let hits = keepout_hits(&p)?;
    let hit: Vec<Id> = sorted(hits.iter().map(|hit| hit.object).collect());
    assert_eq!(hit, vec![BURIED, THROUGH]);
    Ok(())
}
