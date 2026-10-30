//! Headless tour replay in CI (#774) and Help > Tours listing (#769).

use super::super::tour::data::{self, Step, Tour};
use super::super::tour::{anchor, layout, replay, state::TourState};
use super::super::Tab;
use super::support::*;
use super::super::authoring;
use egui::{Color32, Rect, Vec2};
use pcbcraft_text::lookup;

#[derive(Clone, Copy)]
struct Profile {
    width: f32,
    height: f32,
    ui_scale: f32,
    reduced_motion: bool,
}

impl Profile {
    const ALL: [Self; 4] = [
        Self {
            width: 1024.0,
            height: 720.0,
            ui_scale: 1.0,
            reduced_motion: false,
        },
        Self {
            width: 1024.0,
            height: 720.0,
            ui_scale: 1.0,
            reduced_motion: true,
        },
        Self {
            width: 1440.0,
            height: 940.0,
            ui_scale: 1.75,
            reduced_motion: false,
        },
        Self {
            width: 1440.0,
            height: 940.0,
            ui_scale: 1.75,
            reduced_motion: true,
        },
    ];
}

#[test]
fn a_newcomer_opens_Help_Tours_and_sees_every_tour_listed() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Help")?;
    authoring::tests::tap(&ctx, &mut app, "Tours")?;
    for tour in &app.tours.tour {
        let title = lookup(&tour.list_key(), &[]);
        assert!(
            paints_text(&ctx, &mut app, &title)?,
            "Help > Tours lists {title}"
        );
    }
    Ok(())
}

#[test]
fn every_shipped_tour_step_label_exists_in_the_catalog() -> Result<(), String> {
    let doc = data::embedded()?;
    replay::gate_catalog_labels(&doc.tour)
}

#[test]
fn tour_progress_survives_a_preferences_round_trip() -> Result<(), String> {
    replay::tour_progress_round_trip()
}

#[test]
fn tour_progress_is_stored_in_preferences_json_for_the_browser_build() -> Result<(), String> {
    use super::super::tour::state::{TourProgress, TourRecord, TourState};
    let mut prefs = Preferences::default();
    prefs.tour_progress.record(TourRecord {
        id: "window".into(),
        version: 1,
        state: TourState::Skipped,
        step: 1,
    });
    let value = serde_json::to_value(&prefs).map_err(|e| e.to_string())?;
    let progress = value
        .get("tour_progress")
        .ok_or("preferences JSON must carry tour_progress for web storage")?;
    let back: TourProgress = serde_json::from_value(progress.clone()).map_err(|e| e.to_string())?;
    assert_eq!(back.tours.len(), 1);
    Ok(())
}

#[test]
fn a_newcomer_finishes_every_tour() -> Result<(), String> {
    let doc = data::embedded()?;
    for tour in &doc.tour {
        for profile in Profile::ALL {
            replay_one_tour(tour, profile)?;
        }
    }
    Ok(())
}

// When #775 adds a tour, add a dedicated test that calls `replay_one_tour` for
// that id so CI names the tour on failure.

fn replay_one_tour(tour: &Tour, profile: Profile) -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.preferences.ui_scale = profile.ui_scale;
    app.preferences.reduced_motion = profile.reduced_motion;
    let ctx = egui::Context::default();
    open_workspace(&mut app, &tour.workspace)?;
    app.tour_start(tour.id.clone(), tour.version);
    let count = tour.steps.len();
    while app.tour_playback().is_some() {
        let index = app.tour_playback().unwrap().step;
        let step = tour.steps.get(index).ok_or_else(|| {
            format!(
                "tour `{}`: step index {index} out of range",
                tour.id
            )
        })?;
        paint_step(&ctx, &mut app, tour, step, index, count, profile)?;
        if index + 1 >= count {
            app.tour_complete();
            break;
        }
        if let Some(wait) = &step.wait_for {
            perform_wait(&ctx, &mut app, wait, tour, step)?;
        } else {
            app.tour_next();
        }
    }
    Ok(())
}

fn screen(profile: Profile) -> Rect {
    Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(profile.width, profile.height))
}

fn paint_step(
    ctx: &egui::Context,
    app: &mut App,
    tour: &Tour,
    step: &Step,
    index: usize,
    count: usize,
    profile: Profile,
) -> Result<(), String> {
    let label = |msg: &str| format!("tour `{}` step `{}`: {msg}", tour.id, step.id);
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let anchor_rect = anchor::lookup(ctx, &step.target);
    let layout = layout::step_layout(screen(profile), &step.target, anchor_rect, &step.placement);
    let title = lookup(&step.title, &[]);
    let body = lookup(&step.body, &[]);
    replay_frame(ctx, app, profile.width, profile.height, |ui| {
        let card = layout.card;
        ui.painter().rect_filled(card, 8.0, Color32::from_gray(32));
        if let Some(hole) = layout.hole {
            ui.painter()
                .rect_stroke(hole, 4.0, (2.0, Color32::from_rgb(255, 220, 80)));
        }
        let text = format!("{title}\n\n{body}\n\nStep {} of {count}", index + 1);
        ui.painter().text(
            card.left_top() + Vec2::new(12.0, 12.0),
            egui::Align2::LEFT_TOP,
            text,
            egui::FontId::proportional(14.0),
            Color32::WHITE,
        );
    });
    assert!(
        screen(profile).contains_rect(layout.card),
        "{}",
        label("card must stay inside the window")
    );
    if step.target != "none" {
        let anchor_rect = anchor_rect.ok_or_else(|| {
            label(&format!("anchor `{}` was not registered", step.target))
        })?;
        let hole = layout
            .hole
            .ok_or_else(|| label("expected a spotlight hole"))?;
        assert!(
            hole.contains(anchor_rect.center()),
            "{}: hole {hole:?} should cover anchor {anchor_rect:?}",
            label("hole mismatch")
        );
    }
    assert!(
        paints_text(ctx, app, &title)?,
        "{}",
        label("card title must paint")
    );
    Ok(())
}

fn open_workspace(app: &mut App, workspace: &str) -> Result<(), String> {
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

fn perform_wait(
    ctx: &egui::Context,
    app: &mut App,
    wait: &str,
    tour: &Tour,
    step: &Step,
) -> Result<(), String> {
    let ctx_label = |msg: &str| format!("tour `{}` step `{}`: {msg}", tour.id, step.id);
    match wait {
        "document_changed" => {
            app.tool_call("sample_design", serde_json::json!({}));
            app.tour_next();
        }
        "selection:part" => {
            let id = app
                .project
                .components
                .first()
                .map(|c| c.id)
                .ok_or_else(|| ctx_label("no part to select"))?;
            app.select(Some(id));
            app.tour_next();
        }
        "selection:any" => {
            app.select(app.project.components.first().map(|c| c.id));
            app.tour_next();
        }
        _ if wait.starts_with("click:") => {
            let anchor = wait.strip_prefix("click:").unwrap_or("");
            tap_id(ctx, app, anchor).or_else(|_| {
                if let Some((menu, item)) = anchor.split_once('.') {
                    let menu = menu.replace('_', " ");
                    let item = item.replace('_', " ");
                    tap_left(ctx, app, &menu)?;
                    tap_left(ctx, app, &item)
                } else {
                    Err(format!("no control named `{anchor}`"))
                }
            })?;
            app.tour_next();
        }
        _ if wait.starts_with("tool:") => {
            let name = wait.strip_prefix("tool:").unwrap_or("");
            tap_left(ctx, app, name)?;
            app.tour_next();
        }
        _ if wait.starts_with("tab:") => {
            let ws = wait.strip_prefix("tab:").unwrap_or("");
            open_workspace(app, ws)?;
            frame(ctx, app, vec![], egui::Modifiers::NONE);
            app.tour_next();
        }
        _ if wait.starts_with("panel_open:") => {
            let panel = wait.strip_prefix("panel_open:").unwrap_or("");
            app.tour_open_panel(panel)?;
            app.tour_next();
        }
        _ if wait.starts_with("command:") => {
            let id = wait.strip_prefix("command:").unwrap_or("");
            app.tool_call(id, serde_json::json!({}));
            app.tour_next();
        }
        _ => return Err(ctx_label(&format!("unsupported wait_for `{wait}`"))),
    }
    Ok(())
}

#[test]
fn esc_mid_tour_stores_progress_and_resume_returns_to_the_same_step() -> Result<(), String> {
    let doc = data::embedded()?;
    let Some(tour) = doc.tour.first() else {
        return Ok(());
    };
    let profile = Profile::ALL[0];
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    open_workspace(&mut app, &tour.workspace)?;
    app.tour_start(tour.id.clone(), tour.version);
    app.tour_next();
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    app.tour_escape();
    let saved = app
        .preferences
        .tour_progress
        .get(&tour.id)
        .ok_or("expected saved progress after Esc")?;
    assert_eq!(saved.state, TourState::InProgress);
    let step = saved.step;
    app.tour_start(tour.id.clone(), tour.version);
    assert_eq!(app.tour_playback().map(|p| p.step), Some(step));
    Ok(())
}
