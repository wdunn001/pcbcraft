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
    // The sample has no power symbols or drawing, which the tree leaves out until asked.
    tap_id(&ctx, &mut app, "button:Show empty")?;
    let missing = press_each(
        &ctx,
        &mut app,
        &[
            "button:Symbols, shown, unlocked",
            "button:Power symbols, shown, unlocked",
            "button:Drawing, shown, unlocked",
            "button:Net labels, shown, unlocked",
            "button:Buses, shown, unlocked",
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
    let missing = press_each(&ctx, &mut app, &["button:Main"]);
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
            ("button:Hide Power symbols", "button:Show Power symbols"),
            ("button:Hide Net labels", "button:Show Net labels"),
            ("button:Hide Drawing", "button:Show Drawing"),
            ("button:Lock Symbols", "button:Unlock Symbols"),
            ("button:Lock Power symbols", "button:Unlock Power symbols"),
            ("button:Lock Drawing", "button:Unlock Drawing"),
            ("button:Lock Net labels", "button:Unlock Net labels"),
        ],
    )?;
    assert!(
        app.tree.locked.contains("page:0:symbols"),
        "a locked row is remembered by the tree: {:?}",
        app.tree.locked
    );
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
    for branch in ["Nets", "Parts", "Main"] {
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
            "button:Layer # \u{b7} Bottom, shown, unlocked",
            "button:Solder mask, shown, unlocked",
            "button:Paste, shown, unlocked",
            "button:Rectangle, # corners",
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
            ("button:Hide Tracks", "button:Show Tracks"),
            ("button:Hide Package artwork", "button:Show Package artwork"),
            // The drawing paints no solder mask or paste yet: the eye is inert.
            ("button:Hide Solder mask", "button:Hide Solder mask"),
            ("button:Hide Paste", "button:Hide Paste"),
            ("button:Hide Pads", "button:Show Pads"),
            ("button:Hide Drills and vias", "button:Show Drills and vias"),
            ("button:Hide Board edge", "button:Show Board edge"),
            ("button:Lock Tracks", "button:Unlock Tracks"),
            ("button:Lock Silkscreen", "button:Unlock Silkscreen"),
            ("button:Lock Solder mask", "button:Unlock Solder mask"),
            ("button:Lock Paste", "button:Unlock Paste"),
            ("button:Lock Pads", "button:Unlock Pads"),
            ("button:Lock Board edge", "button:Unlock Board edge"),
        ],
    )?;
    Ok(())
}

#[test]
fn a_newcomer_opens_and_closes_the_branches_of_the_layout_tree() -> Result<(), String> {
    let (ctx, mut app) = practice(Tab::Pcb);
    // The top layer opens with its Tracks branch showing; the others start closed.
    for branch in ["Silkscreen", "Pads"] {
        flips_to(
            &ctx,
            &mut app,
            &format!("button:Expand {branch}"),
            &format!("button:Collapse {branch}"),
        )?;
    }
    for branch in [
        "Tracks",
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

#[test]
fn a_newcomer_hides_and_locks_each_copper_layer_and_reopens_the_tracks_branch() -> Result<(), String>
{
    let (ctx, mut app) = practice(Tab::Pcb);
    flip_each(
        &ctx,
        &mut app,
        &[
            (
                "button:Hide Layer # \u{b7} Top",
                "button:Show Layer # \u{b7} Top",
            ),
            (
                "button:Hide Layer # \u{b7} Bottom",
                "button:Show Layer # \u{b7} Bottom",
            ),
            (
                "button:Lock Layer # \u{b7} Top",
                "button:Unlock Layer # \u{b7} Top",
            ),
            (
                "button:Lock Layer # \u{b7} Bottom",
                "button:Unlock Layer # \u{b7} Bottom",
            ),
            // Closed, then opened again: both faces of the chevron are pressed.
            ("button:Collapse Tracks", "button:Expand Tracks"),
            ("button:Expand Tracks", "button:Collapse Tracks"),
        ],
    )?;
    Ok(())
}

#[test]
fn a_newcomer_sees_no_layer_buttons_in_the_schematic_aside_but_still_has_them_in_layout()
-> Result<(), String> {
    let buttons = [
        "button:Show all",
        "button:Hide others",
        "button:New layer",
        "button:Add layer",
    ];
    let (ctx, mut app) = practice(Tab::Schematic);
    for id in buttons {
        assert!(
            !on_screen(&ctx, &mut app, id),
            "the schematic aside has no {id}"
        );
    }
    let names: Vec<String> = interactive_rects(&ctx, &mut app)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert!(
        names
            .iter()
            .any(|n| n.ends_with("Filter layers and objects")),
        "the filter box stays: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.ends_with("Add layer")),
        "a schematic has pages, not layers, so no plus beside the filter"
    );
    assert!(
        names.iter().any(|n| n.ends_with("Add page")),
        "pages are added from the page strip above the canvas"
    );
    assert!(
        on_screen(&ctx, &mut app, "button:Show empty"),
        "Show empty stays: it lists page rows that hold nothing yet"
    );
    let (ctx, mut app) = practice(Tab::Pcb);
    for id in &buttons[..3] {
        assert!(on_screen(&ctx, &mut app, id), "Layout keeps {id}");
    }
    // Parts and Nets are lists of objects, not drawing layers: no eye on either tab.
    for tab in [Tab::Schematic, Tab::Pcb] {
        let (ctx, mut app) = practice(tab);
        for row in ["Parts", "Nets"] {
            assert!(
                on_screen(&ctx, &mut app, &format!("button:{row}")),
                "the {row} row is listed"
            );
            for verb in ["Hide", "Show", "Lock", "Unlock"] {
                assert!(
                    !on_screen(&ctx, &mut app, &format!("button:{verb} {row}")),
                    "{row} has no {verb} control"
                );
            }
        }
    }
    Ok(())
}
