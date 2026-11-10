//! Stories for the Parts tab: every part in one grid.
//!
//! Context: issue #169 and, for the Layout columns, issue #704. A newcomer opens the Parts tab under the canvas, sorts and
//! filters the rows, types a part number and a maker into a cell, and undoes once.
use super::super::*;
use super::support::*;

/// How the gate names a reference: every digit reads as `#`.
fn folded(reference: &str) -> String {
    reference
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect()
}

/// The sample board with the Parts tab open.
fn parts_tab() -> Result<(egui::Context, App), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    tap_id(&ctx, &mut app, "tab:Parts")?;
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    Ok((ctx, app))
}

#[test]
fn a_newcomer_fills_in_part_numbers_in_one_grid() -> Result<(), String> {
    let (ctx, mut app) = parts_tab()?;
    let first = app
        .project
        .components
        .first()
        .ok_or("the sample board has no part")?;
    let (id, reference) = (first.id, first.reference.clone());
    type_into(
        &ctx,
        &mut app,
        &format!("textinput:Part number of {}", folded(&reference)),
        "RC0603FR-0710KL",
    )?;
    type_into(
        &ctx,
        &mut app,
        &format!("textinput:Manufacturer of {}", folded(&reference)),
        "Yageo",
    )?;
    app.refresh();
    let sourcing = app
        .project
        .component(id)
        .map(|c| c.sourcing.clone())
        .ok_or("part missing")?;
    assert_eq!(sourcing.mpn, "RC0603FR-0710KL", "{}", app.message);
    assert_eq!(sourcing.manufacturer, "Yageo", "{}", app.message);
    // One Undo takes the maker back off; the part number stays.
    app.tool_call("undo", json!({}));
    app.refresh();
    let sourcing = app
        .project
        .component(id)
        .map(|c| c.sourcing.clone())
        .ok_or("part missing")?;
    assert_eq!(
        sourcing.manufacturer, "",
        "one Undo removes the maker typed last"
    );
    assert_eq!(
        sourcing.mpn, "RC0603FR-0710KL",
        "and keeps the part number typed before it"
    );
    Ok(())
}

#[test]
fn a_newcomer_renames_a_value_in_the_grid_and_it_is_one_undo_step() -> Result<(), String> {
    let (ctx, mut app) = parts_tab()?;
    let first = app
        .project
        .components
        .first()
        .ok_or("the sample board has no part")?;
    let (id, reference, before) = (first.id, first.reference.clone(), first.value.clone());
    type_into(
        &ctx,
        &mut app,
        &format!("textinput:Value of {}", folded(&reference)),
        "33k",
    )?;
    app.refresh();
    assert_eq!(
        app.project.component(id).map(|c| c.value.clone()),
        Some("33k".to_owned()),
        "typing in the Value cell changes the part: {}",
        app.message
    );
    app.tool_call("undo", json!({}));
    app.refresh();
    assert_eq!(
        app.project.component(id).map(|c| c.value.clone()),
        Some(before),
        "one Undo puts the old value back"
    );
    Ok(())
}

#[test]
fn a_newcomer_sorts_and_filters_the_parts_grid() -> Result<(), String> {
    let (ctx, mut app) = parts_tab()?;
    let refs: Vec<String> = app
        .project
        .components
        .iter()
        .map(|c| c.reference.clone())
        .collect();
    let keep = refs.first().ok_or("the sample board has no part")?.clone();
    let hidden = refs
        .iter()
        .find(|r| !r.to_lowercase().contains(&keep.to_lowercase()))
        .ok_or("the sample board needs two differently named parts")?
        .clone();
    assert!(
        on_screen(
            &ctx,
            &mut app,
            &format!("textinput:Value of {}", folded(&hidden))
        ),
        "before filtering every part has a row"
    );
    tap_id(&ctx, &mut app, "button:Sort by Value")?;
    tap_id(&ctx, &mut app, "button:Sort by Value")?;
    assert!(
        app.parts_table.is_sorted_descending_by_value(),
        "pressing the Value header twice sorts it the other way round"
    );
    type_into(&ctx, &mut app, "textinput:Filter Reference", &keep)?;
    assert!(
        on_screen(
            &ctx,
            &mut app,
            &format!("textinput:Value of {}", folded(&keep))
        ),
        "the part that matches the filter stays"
    );
    assert!(
        !on_screen(
            &ctx,
            &mut app,
            &format!("textinput:Value of {}", folded(&hidden))
        ),
        "a part that does not match the filter is hidden"
    );
    Ok(())
}

/// The sample board's parts in the order the grid shows them with no sort chosen.
fn references_in_grid_order(app: &App) -> Vec<(Id, String)> {
    let mut parts: Vec<(Id, String)> = app
        .project
        .components
        .iter()
        .map(|c| (c.id, c.reference.clone()))
        .collect();
    parts.sort_by(|a, b| {
        let split = |s: &str| {
            let at = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
            (
                s[..at].to_owned(),
                s[at..].parse::<u64>().unwrap_or(0),
                s.to_owned(),
            )
        };
        split(&a.1).cmp(&split(&b.1))
    });
    parts
}

#[test]
fn a_newcomer_exports_the_parts_table_as_a_spreadsheet() -> Result<(), String> {
    let (ctx, mut app) = parts_tab()?;
    let file = std::env::temp_dir().join(format!("pcbcraft-parts-{}.csv", std::process::id()));
    let path = file.to_string_lossy().to_string();
    type_into(&ctx, &mut app, "textinput:Save path", &path)?;
    tap_id(&ctx, &mut app, "button:Export")?;
    let all = std::fs::read_to_string(&file).map_err(|e| format!("{e}: {}", app.message))?;
    let lines: Vec<&str> = all.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("Reference,Value,Footprint,Manufacturer,Part number"),
        "the header names the visible columns"
    );
    let parts = references_in_grid_order(&app);
    assert_eq!(lines.len(), parts.len() + 1, "one line per part");
    let (id, reference) = parts.first().cloned().ok_or("the board has no part")?;
    let part = app.project.component(id).ok_or("part missing")?;
    assert!(
        lines
            .get(1)
            .is_some_and(|l| l.starts_with(&format!("{reference},{},", part.value))),
        "the first line is the first part: {:?}",
        lines.get(1)
    );
    // A filter narrows the file to what the grid shows.
    type_into(&ctx, &mut app, "textinput:Filter Reference", &reference)?;
    tap_id(&ctx, &mut app, "button:Export")?;
    let narrow = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    let kept = parts
        .iter()
        .filter(|(_, r)| r.to_lowercase().contains(&reference.to_lowercase()))
        .count();
    assert_eq!(narrow.lines().count(), kept + 1, "{narrow}");
    assert!(
        narrow.lines().count() < lines.len(),
        "the filter made the file shorter"
    );
    let _ = std::fs::remove_file(&file);
    Ok(())
}

#[test]
fn a_newcomer_fills_a_column_for_several_parts() -> Result<(), String> {
    let (ctx, mut app) = parts_tab()?;
    let parts = references_in_grid_order(&app);
    assert!(parts.len() >= 3, "the sample board needs three parts");
    let (first, third) = (parts[0].clone(), parts[2].clone());
    let before: Vec<(Id, String)> = app
        .project
        .components
        .iter()
        .map(|c| (c.id, c.sourcing.mpn.clone()))
        .collect();
    tap_named_holding(
        &ctx,
        &mut app,
        &format!("CheckBox Pick {}", first.1),
        egui::Modifiers::NONE,
    )?;
    tap_named_holding(
        &ctx,
        &mut app,
        &format!("CheckBox Pick {}", third.1),
        egui::Modifiers::SHIFT,
    )?;
    assert_eq!(
        app.parts_table.picked_count(),
        3,
        "a click and a Shift click pick the range between them"
    );
    tap_id(&ctx, &mut app, "button:Fill into Part number")?;
    type_into(&ctx, &mut app, "textinput:Fill value", "RC0603FR-0710KL")?;
    tap_id(&ctx, &mut app, "button:Fill")?;
    app.refresh();
    let filled: Vec<&str> = parts
        .iter()
        .take(3)
        .filter_map(|(id, _)| app.project.component(*id))
        .map(|c| c.sourcing.mpn.as_str())
        .collect();
    assert_eq!(
        filled, ["RC0603FR-0710KL"; 3],
        "all three picked parts got the part number: {}",
        app.message
    );
    if let Some((id, _)) = parts.get(3) {
        let was = before.iter().find(|(b, _)| b == id).map(|(_, m)| m.clone());
        assert_eq!(
            app.project.component(*id).map(|c| c.sourcing.mpn.clone()),
            was,
            "a part that was not picked is left alone"
        );
    }
    app.tool_call("undo", json!({}));
    app.refresh();
    for (id, mpn) in &before {
        assert_eq!(
            app.project.component(*id).map(|c| c.sourcing.mpn.clone()),
            Some(mpn.clone()),
            "one Undo restores every filled part"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_groups_identical_parts() -> Result<(), String> {
    let (ctx, mut app) = parts_tab()?;
    for (reference, kind, value) in [
        ("R901", "resistor", "47k"),
        ("R902", "resistor", "47k"),
        ("R903", "resistor", "47k"),
        ("R904", "resistor", "68k"),
    ] {
        app.run(
            "component.add",
            json!({"kind": kind, "reference": reference, "value": value,
                   "position": {"x": 10.0, "y": 10.0}}),
        );
    }
    app.refresh();
    let total = app.project.components.len();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_id(&ctx, &mut app, "checkbox:Group identical")?;
    assert!(
        paints_text(&ctx, &mut app, "Quantity")?,
        "grouping adds a Quantity column"
    );
    let file = std::env::temp_dir().join(format!("pcbcraft-grouped-{}.csv", std::process::id()));
    type_into(
        &ctx,
        &mut app,
        "textinput:Save path",
        &file.to_string_lossy(),
    )?;
    tap_id(&ctx, &mut app, "button:Export")?;
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{e}: {}", app.message))?;
    let _ = std::fs::remove_file(&file);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("Quantity,Reference,Value,Footprint,Manufacturer,Part number")
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("3,\"R901, R902, R903\",47k,")),
        "the three 47k resistors are one line with quantity 3: {text}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("1,R904,68k,")),
        "a different value stays on its own line: {text}"
    );
    assert!(lines.len() < total, "fewer lines than parts: {text}");
    Ok(())
}

#[test]
fn a_newcomer_turns_on_layout_columns_in_the_parts_grid_and_finds_side_rotation_locked_x_and_y()
-> Result<(), String> {
    let (ctx, mut app) = parts_tab()?;
    assert!(
        !app.parts_table.layout_columns_on(),
        "the Layout columns are off until the person asks for them"
    );
    let (id, reference) = references_in_grid_order(&app)
        .first()
        .cloned()
        .ok_or("the sample board has no part")?;
    let footprint = app
        .project
        .footprints
        .iter()
        .find(|f| f.component == id)
        .ok_or_else(|| format!("{reference} has no placed footprint on the sample board"))?;
    let (x, y) = (
        format!("{:.2}", footprint.position.x),
        format!("{:.2}", footprint.position.y),
    );
    tap_id(&ctx, &mut app, "checkbox:Layout columns")?;
    assert!(
        app.parts_table.layout_columns_on(),
        "ticking Layout columns turns the columns on"
    );
    for heading in ["Side", "Rotation", "Locked", "X", "Y"] {
        assert!(
            paints_text(&ctx, &mut app, heading)?,
            "the grid shows a {heading} column once Layout columns is on"
        );
    }
    assert!(
        paints_text(&ctx, &mut app, &x)? && paints_text(&ctx, &mut app, &y)?,
        "the first part's position {x}, {y} is shown in the X and Y columns"
    );
    tap_id(&ctx, &mut app, "checkbox:Layout columns")?;
    assert!(
        !app.parts_table.layout_columns_on(),
        "ticking it again turns the columns back off"
    );
    Ok(())
}
