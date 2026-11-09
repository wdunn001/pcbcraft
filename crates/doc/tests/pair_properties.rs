//! DiffPair optional property fields load from older saved shapes.
use pcbcraft_doc::{DiffPair, Project};
use serde_json::Value;

type T = Result<(), String>;

#[test]
fn an_old_pair_loads_unchanged() -> T {
    let raw = serde_json::json!({
        "id": 1,
        "name": "USB",
        "net_p": 2,
        "net_n": 3,
        "gap": 0.2,
        "length_tolerance": 0.5
    });
    let pair: DiffPair = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
    assert_eq!(pair.target_impedance, None);
    assert_eq!(pair.skew_limit_mm, None);
    assert_eq!(pair.skew_limit_ps, None);
    assert_eq!(pair.max_uncoupled_length, None);
    let mut p = Project::default();
    p.nets.push(pcbcraft_doc::Net {
        id: 2,
        name: "P".into(),
        ..Default::default()
    });
    p.nets.push(pcbcraft_doc::Net {
        id: 3,
        name: "N".into(),
        ..Default::default()
    });
    p.board.diff_pairs.push(pair);
    p.validate().map_err(|e| e.to_string())?;
    let saved: Value = serde_json::to_value(&p.board.diff_pairs[0]).map_err(|e| e.to_string())?;
    let keys: Vec<&String> = saved.as_object().ok_or("pair")?.keys().collect();
    for field in [
        "target_impedance",
        "skew_limit_mm",
        "skew_limit_ps",
        "max_uncoupled_length",
    ] {
        assert!(
            !keys.iter().any(|k| k.as_str() == field),
            "field {field} should stay absent on save when unset"
        );
    }
    Ok(())
}
