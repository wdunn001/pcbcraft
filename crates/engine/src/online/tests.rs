//! Tests for `library.import_online` and `library.place_online`.
use super::*;
use pcbcraft_library::{read_footprint, read_symbols};

const DEVICE: &str = include_str!("../../../library/tests/fixtures/device.kicad_sym");
const LM358: &str = include_str!("../../../library/tests/fixtures/lm358.kicad_sym");
const R0805: &str = include_str!("../../../library/tests/fixtures/R_0805_2012Metric.kicad_mod");
const NOTICE: &str = "Imported symbol from Open library: Device.kicad_symdir/R.kicad_sym. Licence CC-BY-SA 4.0 (https://creativecommons.org/licenses/by-sa/4.0/).";

fn symbol(text: &str, name: &str) -> Result<SymbolDefinition, Error> {
    read_symbols(text)
        .map_err(|e| Error::Invalid(e.to_string()))?
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(missing)
}

fn footprint() -> Result<FootprintDefinition, Error> {
    read_footprint(R0805).map_err(|e| Error::Invalid(e.to_string()))
}

fn import_params() -> Result<Value, Error> {
    Ok(json!({
        "symbol": symbol(DEVICE, "R")?,
        "footprint": footprint()?,
        "notices": [NOTICE],
    }))
}

fn place_params(target: &str, x: f64, y: f64) -> Result<Value, Error> {
    let mut v = import_params()?;
    v["target"] = json!(target);
    v["position"] = json!({"x": x, "y": y});
    Ok(v)
}

#[test]
fn both_commands_are_listed_with_their_schemas() {
    let all = commands();
    for id in ["library.import_online", "library.place_online"] {
        assert!(all.iter().any(|c| c.id == id), "{id} is registered");
    }
}

#[test]
fn import_online_adds_symbol_footprint_and_notice() -> Result<(), Error> {
    let mut s = Session::default();
    let mut params = import_params()?;
    params["model"] = json!({"path": "/cache/R_0805.step"});
    let out = s.execute("library.import_online", params, None)?;
    assert_eq!(out["result"]["symbol"], "R");
    assert_eq!(out["result"]["footprint"], "R_0805_2012Metric");
    assert_eq!(out["result"]["unchanged"], false);
    assert!(s.project.library.symbols.iter().any(|x| x.name == "R"));
    let fp = s
        .project
        .library
        .footprints
        .iter()
        .find(|f| f.name == "R_0805_2012Metric")
        .ok_or_else(missing)?;
    assert_eq!(
        fp.model.as_ref().map(|m| m.path.as_str()),
        Some("/cache/R_0805.step"),
        "the 3D model reference is attached"
    );
    assert_eq!(s.project.library.notices, vec![NOTICE.to_owned()]);
    Ok(())
}

#[test]
fn import_online_is_one_undo_step() -> Result<(), Error> {
    let mut s = Session::default();
    let before = s.project.library.clone();
    s.execute("library.import_online", import_params()?, None)?;
    assert_ne!(s.project.library, before);
    s.undo()?;
    assert_eq!(
        s.project.library, before,
        "one undo removes the symbol, footprint and notice"
    );
    Ok(())
}

#[test]
fn import_online_twice_is_a_no_op() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("library.import_online", import_params()?, None)?;
    let once = s.project.library.clone();
    let again = s.execute("library.import_online", import_params()?, None)?;
    assert_eq!(again["result"]["unchanged"], true);
    assert_eq!(s.project.library, once);
    assert_eq!(
        s.project.library.notices.len(),
        1,
        "the notice is kept once"
    );
    Ok(())
}

#[test]
fn import_online_refuses_a_different_part_with_the_same_name() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("library.import_online", import_params()?, None)?;
    let before = s.project.library.clone();
    let mut other = symbol(DEVICE, "R")?;
    other.description = "A different resistor".into();
    let mut params = import_params()?;
    params["symbol"] = serde_json::to_value(&other).map_err(|e| Error::Invalid(e.to_string()))?;
    let err = s
        .execute("library.import_online", params, None)
        .err()
        .ok_or_else(missing)?;
    let text = err.to_string();
    assert!(
        text.contains("replace") && text.contains("keep_both"),
        "{text}"
    );
    assert_eq!(s.project.library, before);
    Ok(())
}

#[test]
fn import_online_replace_and_keep_both() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("library.import_online", import_params()?, None)?;
    let mut other = symbol(DEVICE, "R")?;
    other.description = "A different resistor".into();
    let changed = serde_json::to_value(&other).map_err(|e| Error::Invalid(e.to_string()))?;

    let mut keep = import_params()?;
    keep["symbol"] = changed.clone();
    keep["on_conflict"] = json!("keep_both");
    let out = s.execute("library.import_online", keep, None)?;
    assert_eq!(out["result"]["symbol"], "R_2");
    assert_eq!(
        s.project
            .library
            .symbols
            .iter()
            .filter(|x| x.name.starts_with('R'))
            .count(),
        2
    );

    let mut replace = import_params()?;
    replace["symbol"] = changed;
    replace["on_conflict"] = json!("replace");
    s.execute("library.import_online", replace, None)?;
    let r = s
        .project
        .library
        .symbols
        .iter()
        .find(|x| x.name == "R")
        .ok_or_else(missing)?;
    assert_eq!(r.description, "A different resistor");
    Ok(())
}

#[test]
fn import_online_with_a_bad_definition_changes_nothing() -> Result<(), Error> {
    let mut s = Session::default();
    let revision = s.revision;
    let mut bad = import_params()?;
    bad["symbol"]["name"] = json!("");
    assert!(s.execute("library.import_online", bad, None).is_err());

    let mut too_many = import_params()?;
    too_many["notices"] = json!(["a", "b", "c", "d", "e"]);
    assert!(s.execute("library.import_online", too_many, None).is_err());

    let mut stray = import_params()?;
    stray["unexpected"] = json!(1);
    assert!(s.execute("library.import_online", stray, None).is_err());

    assert_eq!(s.revision, revision);
    assert!(s.project.library.symbols.is_empty());
    assert!(s.project.library.notices.is_empty());
    Ok(())
}

#[test]
fn place_online_keeps_the_cached_model_on_the_placed_footprint() -> Result<(), Error> {
    let mut s = Session::default();
    let mut params = place_params("layout", 10.0, 10.0)?;
    params["model"] = json!({"path": "/data/cache/part.step"});
    s.execute("library.place_online", params, None)?;
    let c = s
        .project
        .components
        .last()
        .ok_or_else(missing)?;
    let path = c
        .library_footprint
        .as_ref()
        .and_then(|lf| lf.definition.model.as_ref())
        .map(|m| m.path.as_str());
    assert_eq!(path, Some("/data/cache/part.step"));
    Ok(())
}

#[test]
fn place_online_on_the_schematic_places_one_component() -> Result<(), Error> {
    let mut s = Session::default();
    let components = s.project.components.len();
    let boards = s.project.board.footprints.len();
    let out = s.execute(
        "library.place_online",
        place_params("schematic", 40.0, 30.0)?,
        None,
    )?;
    assert_eq!(s.project.components.len(), components + 1);
    assert_eq!(
        s.project.board.footprints.len(),
        boards,
        "no board footprint from a schematic drop"
    );
    let id = out["result"]["component"].as_u64().ok_or_else(missing)?;
    let c = s.project.component(id).ok_or_else(missing)?;
    assert_eq!((c.position.x, c.position.y), (40.0, 30.0));
    assert!(c.library_footprint.is_some(), "the footprint is assigned");
    assert!(c.library_symbol.is_some());
    assert_eq!(out["result"]["footprint"], Value::Null);
    Ok(())
}

#[test]
fn place_online_on_the_layout_adds_the_part_to_the_schematic_too() -> Result<(), Error> {
    let mut s = Session::default();
    let components = s.project.components.len();
    let boards = s.project.board.footprints.len();
    let out = s.execute(
        "library.place_online",
        place_params("layout", 12.0, 14.0)?,
        None,
    )?;
    assert_eq!(
        s.project.components.len(),
        components + 1,
        "also on the schematic"
    );
    assert_eq!(s.project.board.footprints.len(), boards + 1);
    let id = out["result"]["component"].as_u64().ok_or_else(missing)?;
    let fid = out["result"]["footprint"].as_u64().ok_or_else(missing)?;
    let f = s
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.id == fid)
        .ok_or_else(missing)?;
    assert_eq!(f.component, id);
    assert_eq!((f.position.x, f.position.y), (12.0, 14.0));
    assert_eq!(f.pads.len(), 2);
    let c = s.project.component(id).ok_or_else(missing)?;
    assert_eq!(c.sheet, 0);
    assert_ne!(
        (c.position.x, c.position.y),
        (12.0, 14.0),
        "the symbol sits on its own free spot"
    );

    // A second part lands on a different free spot, not on top of the first.
    let second = s.execute(
        "library.place_online",
        {
            let mut v = place_params("layout", 30.0, 14.0)?;
            v["on_conflict"] = json!("replace");
            v
        },
        None,
    )?;
    let id2 = second["result"]["component"].as_u64().ok_or_else(missing)?;
    let c2 = s.project.component(id2).ok_or_else(missing)?;
    let c1 = s.project.component(id).ok_or_else(missing)?;
    assert!(
        (c1.position.x - c2.position.x).abs() >= 30.0
            || (c1.position.y - c2.position.y).abs() >= 22.0,
        "free spots do not overlap"
    );
    Ok(())
}

#[test]
fn place_online_is_one_undo_step() -> Result<(), Error> {
    let mut s = Session::default();
    let components = s.project.components.clone();
    let footprints = s.project.board.footprints.clone();
    let library = s.project.library.clone();
    s.execute(
        "library.place_online",
        place_params("layout", 12.0, 14.0)?,
        None,
    )?;
    s.undo()?;
    assert_eq!(s.project.components, components);
    assert_eq!(s.project.board.footprints, footprints);
    assert_eq!(s.project.library, library, "the import and notice go too");
    Ok(())
}

#[test]
fn place_online_with_mismatched_pads_changes_nothing() -> Result<(), Error> {
    let mut s = Session::default();
    let revision = s.revision;
    let mut params = place_params("layout", 12.0, 14.0)?;
    params["symbol"] =
        serde_json::to_value(symbol(LM358, "LM358")?).map_err(|e| Error::Invalid(e.to_string()))?;
    let err = s
        .execute("library.place_online", params, None)
        .err()
        .ok_or_else(missing)?;
    assert!(err.to_string().contains("pad numbers must match"), "{err}");
    assert_eq!(s.revision, revision);
    assert!(
        s.project.library.symbols.is_empty(),
        "the import was rolled back"
    );
    assert!(s.project.components.is_empty());
    Ok(())
}

#[test]
fn place_online_without_a_footprint_places_the_symbol_only() -> Result<(), Error> {
    let mut s = Session::default();
    let mut params = place_params("schematic", 40.0, 30.0)?;
    if let Some(map) = params.as_object_mut() {
        map.remove("footprint");
    }
    let out = s.execute("library.place_online", params, None)?;
    assert_eq!(out["result"]["footprint_missing"], true);
    let id = out["result"]["component"].as_u64().ok_or_else(missing)?;
    assert!(
        s.project
            .component(id)
            .ok_or_else(missing)?
            .library_footprint
            .is_none()
    );

    let mut layout = place_params("layout", 5.0, 5.0)?;
    if let Some(map) = layout.as_object_mut() {
        map.remove("footprint");
    }
    assert!(
        s.execute("library.place_online", layout, None).is_err(),
        "the layout needs a footprint"
    );
    Ok(())
}

#[test]
fn place_online_refuses_a_reference_in_use() -> Result<(), Error> {
    let mut s = Session::default();
    let mut first = place_params("schematic", 40.0, 30.0)?;
    first["reference"] = json!("R7");
    s.execute("library.place_online", first.clone(), None)?;
    first["position"] = json!({"x": 80.0, "y": 30.0});
    let before = s.project.components.len();
    assert!(s.execute("library.place_online", first, None).is_err());
    assert_eq!(s.project.components.len(), before);
    Ok(())
}

#[test]
fn place_online_applies_fields_and_rotation() -> Result<(), Error> {
    let mut s = Session::default();
    let mut params = place_params("schematic", 40.0, 30.0)?;
    params["rotation"] = json!(90.0);
    params["value"] = json!("10k");
    params["fields"] = json!([{"key": "Datasheet", "value": "https://example.com/r.pdf"}]);
    let out = s.execute("library.place_online", params, None)?;
    let id = out["result"]["component"].as_u64().ok_or_else(missing)?;
    let c = s.project.component(id).ok_or_else(missing)?;
    assert_eq!(c.rotation, 90.0);
    assert_eq!(c.value, "10k");
    assert!(
        c.fields
            .iter()
            .any(|f| f.key == "Datasheet" && f.value == "https://example.com/r.pdf")
    );
    Ok(())
}

#[test]
fn place_online_refuses_positions_out_of_range() -> Result<(), Error> {
    let mut s = Session::default();
    let params = place_params("schematic", 1.0e9, 0.0)?;
    assert!(s.execute("library.place_online", params, None).is_err());
    Ok(())
}
