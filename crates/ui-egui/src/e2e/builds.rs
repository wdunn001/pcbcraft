use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_describes_a_build_that_leaves_a_part_off() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Files")?;
    authoring::tests::tap(&ctx, &mut app, "Project settings…")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Builds")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "No extra builds")?,
        "a plain design says so: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "New build")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.variants.len(),
        1,
        "one click adds one build: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "choose a part to leave off")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "R1")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Leave off")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let r1 = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R1")
        .map(|c| c.id)
        .ok_or("the sample should have an R1")?;
    assert!(
        app.project.variants[0].dnp.contains(&r1),
        "the build leaves R1 off: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "1 left off")?,
        "the row counts what it skips"
    );
    authoring::tests::tap(&ctx, &mut app, "Note")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.build_note = Some((app.project.variants[0].name.clone(), "no blue LED".into()));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Save note")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.variants[0].note.as_deref(),
        Some("no blue LED"),
        "the reminder is kept: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "no blue LED")?,
        "the reminder shows on the row"
    );
    authoring::tests::tap(&ctx, &mut app, "\u{d7}")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.variants[0].dnp.is_empty(),
        "the part is back on the build: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Remove")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.variants.is_empty(),
        "the build is gone: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_fits_a_part_in_another_package_for_one_build() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Files")?;
    authoring::tests::tap(&ctx, &mut app, "Project settings…")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Builds")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "New build")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "choose a part to swap")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "R1")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.swap_name = "0603".into();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Swap package")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let r1 = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R1")
        .map(|c| c.id)
        .ok_or("the sample should have an R1")?;
    assert_eq!(
        app.project.variants[0]
            .footprints
            .iter()
            .find(|o| o.component == r1)
            .map(|o| o.footprint.as_str()),
        Some("0603"),
        "the build swaps R1 into 0603: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "1 swapped")?,
        "the row counts the swap"
    );
    assert!(
        paints_text(&ctx, &mut app, "R1 fits 0603")?,
        "the swap is spelled out in plain words"
    );
    authoring::tests::tap(&ctx, &mut app, "\u{d7}")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.variants[0].footprints.is_empty(),
        "the part goes back to its own package: {}",
        app.message
    );
    Ok(())
}
