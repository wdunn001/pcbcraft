//! Tests for the net members query on a hand-built six layer design.
use pcbcraft_doc::Project;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

type T = Result<(), String>;

fn make<X: DeserializeOwned>(v: Value) -> Result<X, String> {
    serde_json::from_value(v).map_err(|e| e.to_string())
}

fn six_layer() -> Result<Project, String> {
    let mut p = Project::default();
    for name in ["F.Cu", "In1.Cu", "In2.Cu", "In3.Cu", "In4.Cu", "B.Cu"] {
        p.board.layer_records.push(make(json!({
            "name": name, "kind": "copper", "thickness": 0.035
        }))?);
    }
    p.nets = vec![
        make(json!({"id":1,"name":"SIG"}))?,
        make(json!({"id":2,"name":"OTHER"}))?,
        make(json!({"id":3,"name":"LOOSE"}))?,
    ];
    p.components = vec![make(
        json!({"id":10,"reference":"R1","value":"1k","kind":"resistor",
            "position":{"x":0.0,"y":0.0},"rotation":0.0,
            "pins":[{"number":"1","name":"1","offset":{"x":0.0,"y":0.0},"net":1},
                    {"number":"2","name":"2","offset":{"x":1.0,"y":0.0},"net":3}]}),
    )?];
    p.board.footprints = vec![make(json!({"id":20,"component":10,
        "position":{"x":5.0,"y":5.0},"rotation":0.0,
        "pads":[{"number":"1","offset":{"x":0.0,"y":0.0},"diameter":1.0,"net":1},
                {"number":"2","offset":{"x":2.0,"y":0.0},"diameter":1.0,"net":3}]}))?];
    let track = |id: u64, net: u64, layer: &str| {
        json!({"id":id,"net":net,"start":{"x":0.0,"y":0.0},"end":{"x":5.0,"y":0.0},
               "width":0.2,"layer":layer})
    };
    p.board.tracks = vec![
        make(track(30, 1, "L1"))?,
        make(track(31, 1, "L3"))?,
        make(track(32, 1, "L5"))?,
        make(track(33, 2, "L2"))?,
    ];
    let via = |id: u64, net: u64, a: &str, b: &str| {
        json!({"id":id,"net":net,"position":{"x":1.0,"y":1.0},"diameter":0.6,
               "drill":0.3,"from_layer":a,"to_layer":b})
    };
    p.board.vias = vec![make(via(40, 1, "L1", "L3"))?, make(via(41, 1, "L3", "L5"))?];
    p.board.zones = vec![make(json!({"id":50,"net":2,"layer":"L6",
        "outline":[{"x":0.0,"y":0.0},{"x":9.0,"y":0.0},{"x":9.0,"y":9.0}]}))?];
    p.wires = vec![
        make(
            json!({"id":60,"net":1,"from":{"component":10,"pin":"1"},"to":{"component":10,"pin":"2"}}),
        )?,
        make(
            json!({"id":61,"net":3,"from":{"component":10,"pin":"2"},"to":{"component":10,"pin":"1"}}),
        )?,
    ];
    p.labels = vec![make(
        json!({"id":70,"kind":"label","net":1,"position":{"x":0.0,"y":0.0},"rotation":0.0,"text":"SIG"}),
    )?];
    Ok(p)
}

#[test]
fn net_members_equals_a_direct_scan_on_the_sample_design() -> T {
    let p = six_layer()?;
    let m = p.net_members(1).ok_or("net 1 has members")?;
    let scan = |ids: Vec<u64>| ids;
    let tracks: Vec<u64> = m.by_layer.iter().flat_map(|l| l.tracks.clone()).collect();
    assert_eq!(
        scan(tracks),
        p.board
            .tracks
            .iter()
            .filter(|t| t.net == 1)
            .map(|t| t.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        m.vias.iter().map(|v| v.id).collect::<Vec<_>>(),
        vec![40, 41]
    );
    assert_eq!(m.pads.len(), 1);
    assert_eq!(
        (m.pads[0].reference.as_str(), m.pads[0].pad.as_str()),
        ("R1", "1")
    );
    assert_eq!(m.pins.len(), 1);
    assert_eq!(m.wires, vec![60]);
    assert_eq!(m.labels, vec![70]);
    let other = p.net_members(2).ok_or("net 2")?;
    assert_eq!(
        other.by_layer.iter().map(|l| l.zones.len()).sum::<usize>(),
        1
    );
    assert!(p.net_members(99).is_none());
    Ok(())
}

#[test]
fn a_net_through_two_vias_reports_every_layer_it_runs_on() -> T {
    let p = six_layer()?;
    let m = p.net_members(1).ok_or("net 1")?;
    // Tracks on layers 1, 3 and 5; the vias add layers 2 and 4 inside their spans.
    assert_eq!(m.layers, vec![0, 1, 2, 3, 4]);
    let track_layers: Vec<u8> = m
        .by_layer
        .iter()
        .filter(|l| !l.tracks.is_empty())
        .map(|l| l.layer)
        .collect();
    assert_eq!(track_layers, vec![0, 2, 4]);
    assert_eq!(m.vias[0].layers, vec![0, 1, 2]);
    assert_eq!(m.vias[1].layers, vec![2, 3, 4]);
    let layer_three = m.by_layer.iter().find(|l| l.layer == 2).ok_or("layer 3")?;
    assert_eq!(layer_three.vias, vec![40, 41]);
    Ok(())
}

#[test]
fn an_unrouted_net_has_pads_and_wires_but_no_copper() -> T {
    let p = six_layer()?;
    let m = p.net_members(3).ok_or("net 3")?;
    assert_eq!(m.pads.len(), 1);
    assert_eq!(m.wires, vec![61]);
    assert!(m.by_layer.is_empty() && m.vias.is_empty() && m.layers.is_empty());
    Ok(())
}

#[test]
fn net_members_adds_nothing_to_the_saved_file() -> T {
    let p = six_layer()?;
    let before = serde_json::to_string(&p).map_err(|e| e.to_string())?;
    let _ = p.net_members(1);
    let after = serde_json::to_string(&p).map_err(|e| e.to_string())?;
    assert_eq!(before, after);
    assert!(!after.contains("by_layer"));
    let back: Project = serde_json::from_str(&after).map_err(|e| e.to_string())?;
    assert_eq!(
        serde_json::to_string(&back).map_err(|e| e.to_string())?,
        before
    );
    Ok(())
}
