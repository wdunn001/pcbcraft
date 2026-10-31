//! Copper layer names in the project file follow the stackup (#469).
//!
//! The bottom copper layer of a 2, 4, 6 or 8 layer board is saved as `B.Cu`,
//! inner layers as `In<i>.Cu`, and a second save is byte-identical.

use pcbcraft_doc::{Layer, LayerRecord, Project, Track};
use pcbcraft_geom::Point;
use serde_json::Value;

type T = Result<(), String>;

fn board(copper: u8) -> Result<Project, String> {
    let mut p = Project::default();
    if copper > 2 {
        for i in 0..copper {
            let name = match i {
                0 => "F.Cu".to_string(),
                i if i + 1 == copper => "B.Cu".to_string(),
                i => format!("In{i}.Cu"),
            };
            p.board.layer_records.push(LayerRecord {
                name,
                kind: "copper".into(),
                thickness: 0.035,
                ..LayerRecord::default()
            });
        }
    }
    for i in 0..copper {
        p.board.tracks.push(Track {
            id: 100 + u64::from(i),
            net: 1,
            start: Point::new(1.0, f64::from(i)),
            end: Point::new(5.0, f64::from(i)),
            width: 0.25,
            layer: Layer::from_index(i)?,
            arc: None,
        });
    }
    Ok(p)
}

fn saved_layers(text: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let tracks = v["board"]["tracks"].as_array().ok_or("no tracks")?;
    tracks
        .iter()
        .map(|t| t["layer"].as_str().map(str::to_owned).ok_or("layer".into()))
        .collect()
}

#[test]
fn the_bottom_is_saved_with_the_bottom_name_on_2_4_6_and_8_layers() -> T {
    for count in [2u8, 4, 6, 8] {
        let p = board(count)?;
        let text = serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?;
        let names = saved_layers(&text)?;
        let mut want = vec!["F.Cu".to_string()];
        want.extend((1..count - 1).map(|i| format!("In{i}.Cu")));
        want.push("B.Cu".into());
        assert_eq!(names, want, "{count} layers");
        assert!(!text.contains(&format!("In{}.Cu", count - 1)), "{count}");
        let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        assert_eq!(back, p, "{count} layers read back the same");
    }
    Ok(())
}

#[test]
fn saving_twice_gives_the_same_bytes() -> T {
    for count in [2u8, 4, 6, 8] {
        let first = serde_json::to_string_pretty(&board(count)?).map_err(|e| e.to_string())?;
        let back: Project = serde_json::from_str(&first).map_err(|e| e.to_string())?;
        let second = serde_json::to_string_pretty(&back).map_err(|e| e.to_string())?;
        assert_eq!(first, second, "{count} layers");
    }
    Ok(())
}

#[test]
fn a_file_without_the_marker_keeps_the_index_reading() -> T {
    // The old rule: B.Cu is index 1 and In3.Cu is index 3, on any board.
    let p = board(4)?;
    let mut v: Value = serde_json::to_value(&p).map_err(|e| e.to_string())?;
    v.as_object_mut().ok_or("object")?.remove("layer_naming");
    let tracks = v["board"]["tracks"].as_array_mut().ok_or("tracks")?;
    let old = ["F.Cu", "B.Cu", "In2.Cu", "In3.Cu"];
    for (t, name) in tracks.iter_mut().zip(old) {
        t["layer"] = Value::String(name.into());
    }
    let back: Project = serde_json::from_value(v).map_err(|e| e.to_string())?;
    let layers: Vec<u8> = back.board.tracks.iter().map(|t| t.layer.index()).collect();
    assert_eq!(layers, vec![0, 1, 2, 3]);
    assert_eq!(back.board.tracks, p.board.tracks);
    Ok(())
}

#[test]
fn the_stackup_may_follow_the_items_in_the_file() -> T {
    let p = board(4)?;
    let v: Value = serde_json::to_value(&p).map_err(|e| e.to_string())?;
    let mut text = String::from("{");
    let obj = v.as_object().ok_or("object")?;
    // "board" last, whatever order the map keeps.
    for (k, val) in obj.iter().filter(|(k, _)| k.as_str() != "board") {
        text += &format!("{}:{},", Value::String(k.clone()), val);
    }
    text += &format!("\"board\":{}}}", obj["board"]);
    let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back, p);
    Ok(())
}
