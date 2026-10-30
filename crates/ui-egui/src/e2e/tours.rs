use super::super::*;
use super::support::*;

fn fresh() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app
}

fn prefs_round_trip(app: &App) -> Result<Preferences, String> {
    let text = serde_json::to_string(&app.preferences).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[test]
fn a_newcomer_skips_a_tour_and_resumes_it_from_help() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Help")?;
    authoring::tests::tap(&ctx, &mut app, "Tours")?;
    let start = t!("tour.menu.start");
    authoring::tests::tap(&ctx, &mut app, &format!("Layout walkthrough (test) ({start})"))?;
    assert!(
        paints_text(&ctx, &mut app, "Step 1 of 4")?,
        "the tour runner should open on step 1"
    );
    authoring::tests::tap(&ctx, &mut app, "Next")?;
    authoring::tests::tap(&ctx, &mut app, "Next")?;
    assert!(
        paints_text(&ctx, &mut app, "Step 3 of 4")?,
        "two Next clicks should reach step 3"
    );
    authoring::tests::tap(&ctx, &mut app, "Skip")?;
    let saved = prefs_round_trip(&app)?;
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, saved, None, None);
    app.refresh();
    authoring::tests::tap(&ctx, &mut app, "Help")?;
    authoring::tests::tap(&ctx, &mut app, "Tours")?;
    let resume = pcbcraft_text::lookup("tour.menu.resume_at", &[("step", "3")]);
    assert!(
        paints_text(&ctx, &mut app, &resume)?,
        "Help > Tours should offer resume at step 3"
    );
    authoring::tests::tap(
        &ctx,
        &mut app,
        &format!("Layout walkthrough (test) ({resume})"),
    )?;
    assert!(
        paints_text(&ctx, &mut app, "Step 3 of 4")?,
        "resume should continue at step 3"
    );
    Ok(())
}

#[test]
fn a_newcomer_turns_off_tour_offers() -> Result<(), String> {
    let mut catalog = tour::state::e2e_fixture_catalog();
    catalog[0].trigger = tour::Trigger::FirstVisit;
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.tour = tour::Controller::new(catalog);
    app.change_tab(Tab::Pcb);
    app.tour.test_quiet_tab_changes();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let chip = pcbcraft_text::lookup(
        "tour.e2e_resume.chip",
        &[("steps", "4")],
    );
    assert!(
        paints_text(&ctx, &mut app, &chip)?,
        "with offers on, the tab chip should appear"
    );
    app.change_tab(Tab::Settings);
    assert!(
        scrolled_to_at(&ctx, &mut app, &t!("settings.settings_tab.offer_tours"), 700.0)?,
        "Settings should list Offer tours"
    );
    authoring::tests::tap(&ctx, &mut app, &t!("settings.settings_tab.offer_tours"))?;
    app.change_tab(Tab::Pcb);
    app.tour.test_quiet_tab_changes();
    for _ in 0..5 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        !paints_text(&ctx, &mut app, &chip)?,
        "with offers off, no chip should appear unasked"
    );
    Ok(())
}
