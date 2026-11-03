//! Stories for the Parts tab: every part in one grid.
//!
//! Context: issue #169. A newcomer opens the Parts tab under the canvas, sorts and
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
