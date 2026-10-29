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
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Left open")?,
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
