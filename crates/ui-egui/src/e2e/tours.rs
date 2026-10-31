//! Tour anchors (#772), Help > Tours listing (#769).

use super::super::authoring;
use super::support::*;

#[test]
fn a_newcomer_opens_Help_Tours_and_sees_every_tour_listed() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Help")?;
    authoring::tests::tap(&ctx, &mut app, "Tours")?;
    for tour in &app.tours.tour {
        let title = pcbcraft_text::lookup(&tour.list_key(), &[]);
        assert!(
            paints_text(&ctx, &mut app, &title)?,
            "Help > Tours lists {title}"
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
                id => app.region().left_tab = id.to_owned(),
            }
        }
    }
    if target.ends_with(".library.search") || target.contains(".library.") {
        app.region().left_tab = "library".into();
    }
}
