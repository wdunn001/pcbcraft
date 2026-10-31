use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_tents_one_via_and_the_fabrication_notes_say_so() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let net = app
        .project
        .nets
        .first()
        .map(|n| n.id)
        .ok_or("the practice design should have a net")?;
    app.tool_call(
        "via.place",
        json!({"net": net, "position": {"x": 12.0, "y": 12.0}, "diameter": 0.7, "drill": 0.3}),
    );
    let via = app
        .project
        .board
        .vias
        .last()
        .map(|v| v.id)
        .ok_or("the via should be on the board")?;
    app.select(Some(via));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Via protection")?,
        "a chosen via offers to say how it is protected: {}",
        app.message
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Follow board")?,
        "the protection picker is reachable: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Follow board")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Tented")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project
            .board
            .vias
            .iter()
            .find(|v| v.id == via)
            .and_then(|v| v.covering),
        Some(pcbcraft_doc::ViaCovering::Tented),
        "picking Tented records it on that via: {}",
        app.message
    );
    let files = pcbcraft_io::gerber_files(&app.project)?;
    let readme = files.get("README-fabrication.txt").ok_or("no readme")?;
    assert!(
        readme.contains("Via protection override: 1 tented"),
        "the factory notes name the tented via: {readme}"
    );
    Ok(())
}

#[test]
fn a_newcomer_tents_every_via_from_the_board_notes() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Settings")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to_at(&ctx, &mut app, "Left open", 700.0)?,
        "the board notes show that vias are left open to begin with"
    );
    authoring::tests::tap(&ctx, &mut app, "Left open")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Tented")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.via_covering_default,
        pcbcraft_doc::ViaCovering::Tented,
        "the board default is recorded: {}",
        app.message
    );
    let files = pcbcraft_io::gerber_files(&app.project)?;
    let readme = files.get("README-fabrication.txt").ok_or("no readme")?;
    assert!(
        readme.contains("Via protection: tented"),
        "the factory notes name the board policy: {readme}"
    );
    Ok(())
}

/// A practice board with one via per x position, none selected yet.
fn app_with_vias(positions: &[f64]) -> Result<(App, Vec<Id>), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let net = app
        .project
        .nets
        .first()
        .map(|n| n.id)
        .ok_or("the practice design should have a net")?;
    let before = app.project.board.vias.len();
    for x in positions {
        app.tool_call(
            "via.place",
            json!({"net": net, "position": {"x": x, "y": 12.0}, "diameter": 0.7, "drill": 0.3}),
        );
    }
    let ids: Vec<Id> = app
        .project
        .board
        .vias
        .iter()
        .skip(before)
        .map(|v| v.id)
        .collect();
    if ids.len() != positions.len() {
        return Err("every via should be on the board".into());
    }
    Ok((app, ids))
}

#[test]
fn a_newcomer_tents_a_via_on_top_and_leaves_it_open_underneath_for_a_test_probe()
-> Result<(), String> {
    let (mut app, ids) = app_with_vias(&[12.0])?;
    let via = ids[0];
    app.select(Some(via));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for words in ["Via protection", "Top side", "Bottom side"] {
        assert!(
            paints_text(&ctx, &mut app, words)?,
            "the via offers {words:?}: {}",
            app.message
        );
    }
    // Tent both sides first, then reopen the bottom for the probe.
    assert!(scrolled_to(&ctx, &mut app, "Follow board")?);
    authoring::tests::tap(&ctx, &mut app, "Follow board")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Tented")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert!(
        paints_text(&ctx, &mut app, "Protection type Ib")?,
        "tented on both sides is type Ib: {}",
        app.message
    );
    // The last "Tented" on screen is the bottom side picker.
    authoring::tests::tap(&ctx, &mut app, "Tented")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Left open")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let v = app
        .project
        .board
        .vias
        .iter()
        .find(|v| v.id == via)
        .cloned()
        .ok_or("the via is still there")?;
    assert_eq!(v.covering, Some(pcbcraft_doc::ViaCovering::Tented));
    assert_eq!(v.covering_bottom, Some(pcbcraft_doc::ViaCovering::Open));
    assert!(
        paints_text(&ctx, &mut app, "Protection type Ia")?,
        "tented on one side is type Ia: {}",
        app.message
    );
    assert!(!pcbcraft_io::via_mask_opening_on(
        &v,
        &app.project.board,
        false
    ));
    assert!(pcbcraft_io::via_mask_opening_on(
        &v,
        &app.project.board,
        true
    ));
    let files = pcbcraft_io::gerber_files(&app.project)?;
    let drill = files.get("pcb-Plated.drl").ok_or("no plated drill file")?;
    assert!(
        drill.contains("ViaDrill,Ia"),
        "the drill file tells the fab the same thing: {drill}"
    );
    Ok(())
}

#[test]
fn a_newcomer_plugs_two_selected_vias_in_one_step() -> Result<(), String> {
    let (mut app, ids) = app_with_vias(&[12.0, 14.0])?;
    app.select(Some(ids[0]));
    app.edit.extra = vec![ids[1]];
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Changing 2 selected vias")?,
        "the panel says it will change both: {}",
        app.message
    );
    assert!(scrolled_to(&ctx, &mut app, "Follow board")?);
    authoring::tests::tap(&ctx, &mut app, "Follow board")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Plugged")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    for id in ids {
        let v = app
            .project
            .board
            .vias
            .iter()
            .find(|v| v.id == id)
            .ok_or("the via is still there")?;
        assert_eq!(
            v.covering,
            Some(pcbcraft_doc::ViaCovering::Plugged),
            "both vias are plugged: {}",
            app.message
        );
    }
    Ok(())
}
