//! Pours in the Gerber export (issue #673): the showcase exports every copper
//! layer, a small pour with an island and a thermal matches a golden file, the
//! written regions read back to the pour area, and a large six-layer pour stays
//! under the layer cap in bounded time.

use pcbcraft_doc::{
    CopperZone, Layer, LayerRecord, Net, Project, Track, Via, ZoneConnect, ZoneIslands, ZonePour,
    zone_fill,
};
use pcbcraft_geom::Point;
use pcbcraft_io::{gerber_files_report, gerber_files_with_budget, parse_gerber_layer};
use std::path::Path;
use std::time::{Duration, Instant};

const CREATED: &str = "2026-10-29T00:00:00Z";

fn copper_records(names: &[&str]) -> Vec<LayerRecord> {
    names
        .iter()
        .map(|name| LayerRecord {
            name: (*name).into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: None,
            ..LayerRecord::default()
        })
        .collect()
}

fn square_zone(
    id: u64,
    net: u64,
    layer: u8,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) -> Result<CopperZone, String> {
    Ok(CopperZone {
        id,
        net,
        layer: Layer::from_index(layer)?,
        outline: vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
            Point::new(x0, y0),
        ],
        cutouts: vec![],
        filled: true,
        kind: None,
        pour: None,
    })
}

fn net(id: u64, name: &str) -> Net {
    Net {
        id,
        name: name.into(),
        ..Default::default()
    }
}

/// A two-layer board whose top pour carries a hole (foreign via), an island
/// (a foreign track walls off the right edge) and a thermal (a same-net via
/// joined through spokes).
fn island_and_thermal() -> Result<Project, String> {
    let mut p = Project::default();
    p.board.width = 40.0;
    p.board.height = 30.0;
    p.nets.push(net(1, "GND"));
    p.nets.push(net(2, "SIG"));
    let mut zone = square_zone(10, 1, 0, 1.0, 1.0, 39.0, 29.0)?;
    zone.pour = Some(ZonePour {
        via_connect: ZoneConnect::Relief,
        islands: ZoneIslands::Keep,
        ..Default::default()
    });
    p.board.zones.push(zone);
    p.board.vias.push(Via {
        id: 20,
        net: 1,
        position: Point::new(10.0, 15.0),
        diameter: 1.0,
        drill: 0.5,
        from_layer: None,
        to_layer: None,
        covering: None,
    });
    p.board.vias.push(Via {
        id: 21,
        net: 2,
        position: Point::new(20.0, 15.0),
        diameter: 1.0,
        drill: 0.5,
        from_layer: None,
        to_layer: None,
        covering: None,
    });
    p.board.tracks.push(Track {
        id: 30,
        net: 2,
        start: Point::new(33.0, 0.0),
        end: Point::new(33.0, 30.0),
        width: 1.0,
        layer: Layer::Front,
        arc: None,
    });
    p.validate()?;
    Ok(p)
}

#[test]
fn showcase_exports_every_copper_layer() -> Result<(), String> {
    let p = pcbcraft_io::load(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/showcase.pcbcraft"
    )))?;
    let (files, warnings) = gerber_files_report(&p, CREATED)?;
    assert!(warnings.is_empty(), "no pour is left out: {warnings:?}");
    for name in [
        "pcb-F.Cu.gbr",
        "pcb-In1.Cu.gbr",
        "pcb-In2.Cu.gbr",
        "pcb-B.Cu.gbr",
    ] {
        let text = files.get(name).ok_or(format!("{name} is missing"))?;
        assert!(text.ends_with("M02*\n"), "{name} is complete");
        assert!(
            parse_gerber_layer(text, name)?.region_area_mm2() > 100.0,
            "{name} carries its pour"
        );
    }
    // Every pour lands in the file of its own layer.
    for z in &p.board.zones {
        let name = match z.layer.index() {
            0 => "pcb-F.Cu.gbr".to_owned(),
            3 => "pcb-B.Cu.gbr".to_owned(),
            n => format!("pcb-In{n}.Cu.gbr"),
        };
        assert!(files[&name].contains("G36*"), "zone {} in {name}", z.id);
    }
    Ok(())
}

#[test]
fn pour_with_island_and_thermal_matches_golden() -> Result<(), String> {
    let p = island_and_thermal()?;
    let (files, warnings) = gerber_files_report(&p, CREATED)?;
    assert!(warnings.is_empty());
    let fill = zone_fill(&p, &p.board.zones[0])?;
    assert!(fill.spokes >= 2, "the same-net via keeps its spokes");
    assert!(fill.pieces.len() >= 2, "the track cuts off an island");
    let front = files.get("pcb-F.Cu.gbr").ok_or("front copper")?;
    if std::env::var_os("BLESS_GOLDEN").is_some() {
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/pour_island_thermal_F.Cu.gbr"
            ),
            front,
        )
        .map_err(|e| e.to_string())?;
    }
    let golden = include_str!("fixtures/pour_island_thermal_F.Cu.gbr");
    assert_eq!(
        front, golden,
        "regenerate the fixture only after reading the diff"
    );
    Ok(())
}

#[test]
fn written_regions_match_pour_area() -> Result<(), String> {
    let p = island_and_thermal()?;
    let expected = zone_fill(&p, &p.board.zones[0])?.area();
    let (files, _) = gerber_files_report(&p, CREATED)?;
    let layer = parse_gerber_layer(&files["pcb-F.Cu.gbr"], "pcb-F.Cu.gbr")?;
    let read = layer.region_area_mm2();
    assert!(
        layer.region_clear.iter().any(|c| *c),
        "holes are clear regions"
    );
    assert!(
        (read - expected).abs() <= expected * 0.002,
        "read {read} mm2 against the pour's {expected} mm2"
    );
    Ok(())
}

#[test]
fn six_layer_pour_stays_under_cap_and_in_time() -> Result<(), String> {
    let mut p = Project::default();
    p.board.width = 100.0;
    p.board.height = 80.0;
    p.nets.push(net(1, "GND"));
    p.nets.push(net(2, "SIG"));
    p.board.layer_records =
        copper_records(&["F.Cu", "In1.Cu", "In2.Cu", "In3.Cu", "In4.Cu", "B.Cu"]);
    for layer in 0..6u8 {
        p.board.zones.push(square_zone(
            100 + u64::from(layer),
            1,
            layer,
            1.0,
            1.0,
            99.0,
            79.0,
        )?);
    }
    let mut id = 1000;
    for col in 0..30 {
        for row in 0..16 {
            id += 1;
            p.board.vias.push(Via {
                id,
                net: if (col + row) % 3 == 0 { 1 } else { 2 },
                position: Point::new(4.0 + f64::from(col) * 3.1, 5.0 + f64::from(row) * 4.5),
                diameter: 0.8,
                drill: 0.4,
                from_layer: None,
                to_layer: None,
                covering: None,
            });
        }
    }
    p.validate()?;
    let started = Instant::now();
    let (files, warnings) = gerber_files_report(&p, CREATED)?;
    let elapsed = started.elapsed();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(
        elapsed < Duration::from_secs(120),
        "export took {elapsed:?}"
    );
    for name in [
        "pcb-F.Cu.gbr",
        "pcb-In1.Cu.gbr",
        "pcb-In2.Cu.gbr",
        "pcb-In3.Cu.gbr",
        "pcb-In4.Cu.gbr",
        "pcb-B.Cu.gbr",
    ] {
        let text = files.get(name).ok_or(format!("{name} is missing"))?;
        let ops = text
            .lines()
            .filter(|l| l.ends_with("D01*") || l.ends_with("D02*") || l.ends_with("D03*"))
            .count();
        assert!(ops < 1_000_000, "{name} has {ops} operations");
        assert!(ops > 1_000, "{name} carries a real pour, {ops} operations");
    }
    eprintln!("six layer export took {elapsed:?}");
    Ok(())
}

#[test]
fn oversized_pour_is_skipped_with_a_named_note() -> Result<(), String> {
    let p = island_and_thermal()?;
    // A budget the top pour cannot meet even at the coarsest tolerance.
    let (files, warnings) = gerber_files_with_budget(&p, CREATED, 40)?;
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let note = &warnings[0];
    assert!(note.contains("zone 10") && note.contains("F.Cu"), "{note}");
    assert!(note.contains("Split the zone"), "{note}");
    assert!(files["pcb-F.Cu.gbr"].contains("G04 Pour (zone 10) on F.Cu was left out"));
    assert!(files["README-fabrication.txt"].contains("Export warnings"));
    // Everything else still ships.
    for name in [
        "pcb-B.Cu.gbr",
        "pcb-Edge.gbr",
        "pcb-job.gbrjob",
        "pcb-Plated.drl",
    ] {
        assert!(files.contains_key(name), "{name} is still written");
    }
    Ok(())
}
