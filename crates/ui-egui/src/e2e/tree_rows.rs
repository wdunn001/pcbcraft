//! Stories for pressing the rows, eyes, locks and chevrons of the Layers tree
//! by name, one list per page, the way a person works down the tree.
use super::super::*;
use super::support::*;

fn practice(tab: Tab) -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(tab);
    app.region().left_tab = "layers".to_owned();
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}

/// Press every eye and lock in `pairs`; each one reads the other way after.
fn flip_each(ctx: &egui::Context, app: &mut App, pairs: &[(&str, &str)]) -> Result<(), String> {
    let failed: Vec<String> = pairs
        .iter()
        .filter_map(|(from, to)| flips_to(ctx, app, from, to).err())
        .collect();
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("; "))
    }
}

#[test]
fn a_newcomer_picks_each_row_of_the_schematic_tree() -> Result<(), String> {
    let (ctx, mut app) = practice(Tab::Schematic);
    // The sample has no power ports or drawing, which the tree leaves out until asked.
    tap_id(&ctx, &mut app, "button:Show empty")?;
    let missing = press_each(
        &ctx,
        &mut app,
        &[
            "button:Symbols, shown, unlocked",
            "button:Power ports, shown, unlocked",
            "button:Drawing, shown, unlocked",
            "button:Title block, shown, unlocked",
            "button:Nets",
            "button:GND",
            "button:LED",
            "button:D#, Green LED",
            "button:J#, #V input",
            "button:R#, # \u{3a9}",
        ],
    );
    assert!(missing.is_empty(), "rows not in the tree: {missing:?}");
    assert!(
        app.selected.is_some(),
        "the last part row selects its part on the drawing: {}",
        app.message
    );
    let missing = press_each(&ctx, &mut app, &["button:Sheet #"]);
    assert!(missing.is_empty(), "rows not in the tree: {missing:?}");
    assert_eq!(
        app.tree.selected_key.as_deref(),
        Some("sec:sheet:0"),
        "the sheet row selects its page so Properties shows it: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_hides_and_locks_each_schematic_layer_from_its_row() -> Result<(), String> {
    let (ctx, mut app) = practice(Tab::Schematic);
    tap_id(&ctx, &mut app, "button:Show empty")?;
    flip_each(
        &ctx,
        &mut app,
        &[
            ("button:Hide Symbols", "button:Show Symbols"),
            ("button:Hide Power ports", "button:Show Power ports"),
            ("button:Hide Title block", "button:Show Title block"),
            ("button:Hide Drawing", "button:Show Drawing"),
            ("button:Hide Nets", "button:Show Nets"),
            ("button:Lock Symbols", "button:Unlock Symbols"),
            ("button:Lock Power ports", "button:Unlock Power ports"),
            ("button:Lock Drawing", "button:Unlock Drawing"),
            ("button:Lock Title block", "button:Unlock Title block"),
        ],
    )?;
    assert!(
        app.tree.locked.contains("page:0:symbols"),
        "a locked row is remembered by the tree: {:?}",
        app.tree.locked
    );
    tap_id(&ctx, &mut app, "button:Show all")?;
    assert!(
        app.preferences.hidden_layers.is_empty(),
        "Show all brings every layer back: {:?}",
        app.preferences.hidden_layers
    );
    tap_id(&ctx, &mut app, "button:Hide others")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok(())
}

#[test]
fn a_newcomer_opens_and_closes_the_branches_of_the_schematic_tree() -> Result<(), String> {
    let (ctx, mut app) = practice(Tab::Schematic);
    for branch in ["Symbols", "D#", "J#", "R#", "GND", "LED", "VCC"] {
        flips_to(
            &ctx,
            &mut app,
            &format!("button:Expand {branch}"),
            &format!("button:Collapse {branch}"),
        )?;
    }
    for branch in ["Nets", "Parts", "Sheet #"] {
        flips_to(
            &ctx,
            &mut app,
            &format!("button:Collapse {branch}"),
            &format!("button:Expand {branch}"),
        )?;
    }
    Ok(())
}

#[test]
fn a_newcomer_picks_the_parts_row_of_the_schematic_tree() -> Result<(), String> {
    let (ctx, mut app) = practice(Tab::Schematic);
    tap_id(&ctx, &mut app, "button:Parts")?;
    assert!(
        app.tree.cursor.is_some() || app.tree.selected_key.is_some(),
        "pressing the Parts row puts the tree cursor on it"
    );
    assert!(
        paints_text(&ctx, &mut app, "Parts")?,
        "the Parts row stays in the tree after it is picked"
    );
    Ok(())
}

#[test]
fn a_newcomer_filters_the_tree_by_typing() -> Result<(), String> {
    let (ctx, mut app) = practice(Tab::Schematic);
    type_into(&ctx, &mut app, "textinput:Filter layers and objects", "LED")?;
    assert_eq!(app.tree.query, "LED", "the filter box keeps what was typed");
    assert!(
        paints_text(&ctx, &mut app, "Green LED")?,
        "the matching part stays in the tree"
    );
    Ok(())
}

#[test]
fn a_newcomer_picks_each_row_of_the_layout_tree() -> Result<(), String> {
    let (ctx, mut app) = practice(Tab::Pcb);
    let missing = press_each(
        &ctx,
        &mut app,
        &[
            "button:Layer # \u{b7} Bottom",
            "button:Solder mask, shown, unlocked",
            "button:Paste, shown, unlocked",
            "button:Edge, # x # mm",
            "button:Cut-out, #, #",
            "button:User layers",
            "button:GND, L#",
            "button:LED, L#",
            "button:D#, Green LED top",
            "button:J#, #V input top",
            "button:R#, # \u{3a9} top",
        ],
    );
    assert!(missing.is_empty(), "rows not in the tree: {missing:?}");
    Ok(())
}

#[test]
fn a_newcomer_hides_and_locks_each_layout_layer_from_its_row() -> Result<(), String> {
    let (ctx, mut app) = practice(Tab::Pcb);
    flip_each(
        &ctx,
        &mut app,
        &[
            ("button:Hide Copper", "button:Show Copper"),
            ("button:Hide Silkscreen", "button:Show Silkscreen"),
            // The drawing paints no solder mask or paste yet: the eye is inert.
            ("button:Hide Solder mask", "button:Hide Solder mask"),
            ("button:Hide Paste", "button:Hide Paste"),
            ("button:Hide Components", "button:Show Components"),
            ("button:Hide Drills and vias", "button:Show Drills and vias"),
            ("button:Hide Board edge", "button:Show Board edge"),
            ("button:Lock Copper", "button:Unlock Copper"),
            ("button:Lock Silkscreen", "button:Unlock Silkscreen"),
            ("button:Lock Solder mask", "button:Unlock Solder mask"),
            ("button:Lock Paste", "button:Unlock Paste"),
            ("button:Lock Components", "button:Unlock Components"),
            ("button:Lock Board edge", "button:Unlock Board edge"),
        ],
    )?;
    Ok(())
}

#[test]
fn a_newcomer_opens_and_closes_the_branches_of_the_layout_tree() -> Result<(), String> {
    let (ctx, mut app) = practice(Tab::Pcb);
    for branch in ["Copper", "Silkscreen", "Components"] {
        flips_to(
            &ctx,
            &mut app,
            &format!("button:Expand {branch}"),
            &format!("button:Collapse {branch}"),
        )?;
    }
    for branch in [
        "Copper",
        "Board edge",
        "Layer # \u{b7} Top",
        "Layer # \u{b7} Bottom",
    ] {
        flips_to(
            &ctx,
            &mut app,
            &format!("button:Collapse {branch}"),
            &format!("button:Expand {branch}"),
        )?;
    }
    Ok(())
}
