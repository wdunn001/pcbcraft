//! `part.place` puts a part on the schematic page it is given, as the library card does.

use pcbcraft_engine::Session;
use serde_json::json;

#[test]
fn a_part_placed_with_a_page_lands_on_that_page() -> Result<(), String> {
    let mut s = Session::default();
    s.execute("schematic.new_sheet", json!({"name": "Page 1"}), None)
        .map_err(|e| e.to_string())?;
    let page = s.project.sheets.first().ok_or("no page")?.id;
    for (params, expected) in [
        (
            json!({"target":"schematic","kind":"resistor","position":{"x":20,"y":20},"sheet":page}),
            page,
        ),
        (
            json!({"target":"schematic","kind":"resistor","position":{"x":40,"y":20}}),
            0,
        ),
    ] {
        let out = s
            .execute("part.place", params, None)
            .map_err(|e| e.to_string())?;
        let id = out["result"]["id"].as_u64().ok_or("no id")?;
        let part = s.project.component(id).ok_or("part missing")?;
        assert_eq!(part.sheet, expected);
    }
    let refused = s.execute(
        "part.place",
        json!({"target":"schematic","kind":"resistor","position":{"x":0,"y":0},"sheet":9999}),
        None,
    );
    assert!(refused.is_err(), "a page that does not exist is refused");
    Ok(())
}
