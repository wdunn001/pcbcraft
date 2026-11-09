//! Tour anchors (#772), Help > Tours listing (#769).

use super::super::authoring;
use super::super::bottom_sheet::SheetTab;
use super::super::tour::{self, anchor, data};
use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_opens_help_tours_and_sees_every_tour_listed() -> Result<(), String> {
    let doc = data::embedded()?;
    assert!(
        doc.tour.len() >= 2,
        "the shipped catalog must hold real tours, found {}",
        doc.tour.len()
    );
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Help")?;
    authoring::tests::tap(&ctx, &mut app, "Tours")?;
    let start = pcbcraft_text::lookup("tour.menu.start", &[]);
    for tour in &doc.tour {
        let title = pcbcraft_text::lookup(&tour.list_key(), &[]);
        let row = format!("{title} ({start})");
        assert!(
            paints_text(&ctx, &mut app, &row)?,
            "the open Help > Tours menu lists the row `{row}`"
        );
    }
    Ok(())
}

#[test]
fn every_shipped_tour_step_target_is_registered() -> Result<(), String> {
    let doc = data::embedded()?;
    for tour in &doc.tour {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        open_tour_workspace(&mut app, &tour.workspace)?;
        let ctx = egui::Context::default();
        for step in &tour.steps {
            if step.target == "none" {
                continue;
            }
            prepare_for_target(&mut app, &step.target);
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            let rect = anchor::lookup(&ctx, &step.target).ok_or_else(|| {
                format!(
                    "tour `{}` step `{}`: anchor `{}` was not registered",
                    tour.id, step.id, step.target
                )
            })?;
            assert!(
                rect.width() > 0.0 && rect.height() > 0.0,
                "tour `{}` step `{}`: anchor `{}` has an empty rect",
                tour.id,
                step.id,
                step.target
            );
        }
    }
    Ok(())
}

#[test]
fn a_newcomer_sees_the_tour_card_beside_the_control_it_names() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.change_tab(Tab::Schematic);
    app.region().left_tab = "library".into();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let target = "shell.tab.schematic";
    let anchor_rect = anchor::lookup(&ctx, target)
        .ok_or_else(|| format!("expected anchor `{target}` on the schematic tab"))?;
    let screen = ctx.content_rect();
    let layout = tour::step_layout(screen, target, Some(anchor_rect), "bottom");
    assert!(
        screen.contains_rect(layout.card),
        "tour card should stay inside the window"
    );
    assert!(
        !layout.card.intersects(anchor_rect),
        "tour card should sit beside the control, not on top of it"
    );
    let gap = layout.card.min.y - anchor_rect.max.y;
    assert!(
        gap >= 8.0,
        "card should sit below the tab with a gap, got {gap}"
    );
    Ok(())
}

fn open_tour_workspace(app: &mut App, workspace: &str) -> Result<(), String> {
    let tab = match workspace {
        "files" => Tab::Files,
        "schematic" => Tab::Schematic,
        "layout" => Tab::Pcb,
        "manufacture" => Tab::PlotFiles,
        other => return Err(format!("unknown tour workspace `{other}`")),
    };
    app.change_tab(tab);
    Ok(())
}

fn prepare_for_target(app: &mut App, target: &str) {
    if let Some(rest) = target.strip_prefix("region.") {
        let mut parts = rest.split('.');
        if let Some(ws) = parts.next() {
            let _ = open_tour_workspace(app, ws);
        }
        if let Some(tab) = parts.next() {
            match tab {
                "findings" => app.open_sheet(SheetTab::Findings),
                "output" => app.open_sheet(SheetTab::Output),
                "console" => app.open_sheet(SheetTab::Console),
                "parts" => app.open_sheet(SheetTab::Parts),
                id => app.region().left_tab = id.to_owned(),
            }
        }
    }
    if target.ends_with(".library.search") || target.contains(".library.") {
        app.region().left_tab = "library".into();
    }
}

fn probe_step(id: &str, target: &str, title: &str, body: &str) -> data::Step {
    data::Step {
        id: id.into(),
        target: target.into(),
        title: title.into(),
        body: body.into(),
        placement: "bottom".into(),
        condition: None,
        wait_for: None,
    }
}

#[test]
fn a_newcomer_follows_a_tour_card_with_a_spotlight_and_leaves_it_with_skip() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.change_tab(Tab::Schematic);
    app.tours = data::Doc {
        tour: vec![data::Tour {
            id: "overlay_probe".into(),
            version: 1,
            workspace: "schematic".into(),
            trigger: "manual".into(),
            steps: vec![
                probe_step(
                    "one",
                    "shell.tab.schematic",
                    "tour.card.first_launch.title",
                    "tour.card.first_launch.body",
                ),
                probe_step(
                    "two",
                    "none",
                    "tour.card.learn_more",
                    "tour.card.first_launch.footer",
                ),
                probe_step("three", "none", "tour.card.not_now", "tour.chip.hint"),
            ],
        }],
    };
    app.tour = tour::Controller::new(vec![tour::state::TourMeta {
        id: "overlay_probe".into(),
        version: 1,
        trigger: tour::state::Trigger::Manual,
        workspace: tour::WorkspaceId::Schematic,
        step_count: 3,
    }]);
    app.tour.start_from_menu(
        &mut app.preferences.tour_progress,
        "overlay_probe",
        tour::state::MenuAction::Start,
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(paints_text(&ctx, &mut app, "New here?")?, "step one title");
    assert!(
        paints_text(&ctx, &mut app, "Step 1 of 3")?,
        "step one counter"
    );
    authoring::tests::tap(&ctx, &mut app, "Next")?;
    assert!(paints_text(&ctx, &mut app, "Show me")?, "step two title");
    assert!(
        !paints_text(&ctx, &mut app, "New here?")?,
        "step one card is gone"
    );
    authoring::tests::tap(&ctx, &mut app, "Skip")?;
    assert!(
        !paints_text(&ctx, &mut app, "Show me")?,
        "the card closes on Skip"
    );
    let record = app
        .preferences
        .tour_progress
        .get("overlay_probe")
        .ok_or("Skip keeps the place")?;
    assert_eq!(record.step, 1);
    Ok(())
}

#[test]
fn a_newcomer_steps_through_a_tour_by_keyboard_only() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.change_tab(Tab::Schematic);
    app.tours = data::Doc {
        tour: vec![data::Tour {
            id: "keyboard_probe".into(),
            version: 1,
            workspace: "schematic".into(),
            trigger: "manual".into(),
            steps: vec![
                probe_step(
                    "one",
                    "shell.tab.schematic",
                    "tour.card.first_launch.title",
                    "tour.card.first_launch.body",
                ),
                probe_step(
                    "two",
                    "none",
                    "tour.card.learn_more",
                    "tour.card.first_launch.footer",
                ),
                probe_step("three", "none", "tour.card.not_now", "tour.chip.hint"),
            ],
        }],
    };
    app.tour = tour::Controller::new(vec![tour::state::TourMeta {
        id: "keyboard_probe".into(),
        version: 1,
        trigger: tour::state::Trigger::Manual,
        workspace: tour::WorkspaceId::Schematic,
        step_count: 3,
    }]);
    app.tour.start_from_menu(
        &mut app.preferences.tour_progress,
        "keyboard_probe",
        tour::state::MenuAction::Start,
    );
    let ctx = egui::Context::default();
    // Step one appears
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Step 1 of 3")?,
        "step one counter"
    );
    // Advance with Right arrow
    key(&ctx, &mut app, egui::Key::ArrowRight);
    assert!(
        paints_text(&ctx, &mut app, "Step 2 of 3")?,
        "step two counter"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Step 1 of 3")?,
        "step one is gone"
    );
    // Back with Left arrow
    key(&ctx, &mut app, egui::Key::ArrowLeft);
    assert!(
        paints_text(&ctx, &mut app, "Step 1 of 3")?,
        "step one counter again"
    );
    // Advance all the way with Right arrows
    key(&ctx, &mut app, egui::Key::ArrowRight);
    key(&ctx, &mut app, egui::Key::ArrowRight);
    key(&ctx, &mut app, egui::Key::ArrowRight);
    // Tour should be complete
    assert!(
        !paints_text(&ctx, &mut app, "Step 1 of 3")?,
        "tour card is gone after completion"
    );
    let record = app
        .preferences
        .tour_progress
        .get("keyboard_probe")
        .ok_or("tour progress record should exist")?;
    assert_eq!(record.state, tour::state::PersistedState::Done);
    Ok(())
}
