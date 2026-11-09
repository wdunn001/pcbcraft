//! Headless tour replay in CI (#774) and Help > Tours listing (#769).

use super::super::Tab;
use super::super::authoring;
use super::super::tour::data::{self, Step, Tour};
use super::super::tour::{anchor, layout, playback::TourState, replay};
use super::super::*;
use super::support::*;
use egui::{Rect, Vec2};
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
    use super::super::tour::playback::{TourProgress, TourRecord, TourState};
    let mut prefs = Preferences::default();
    prefs.tour_replay.record(TourRecord {
        id: "window".into(),
        version: 1,
        state: TourState::Skipped,
        step: 1,
    });
    let value = serde_json::to_value(&prefs).map_err(|e| e.to_string())?;
    let progress = value
        .get("tour_replay")
        .ok_or("preferences JSON must carry tour_replay for web storage")?;
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

/// Plays one tour through the real controller and overlay the way a person
/// does: the card must paint its title and step count, the cutout must cover
/// the control it names, and Next (or the step's own action) moves on until
/// the tour is recorded as done.
fn replay_one_tour(tour: &Tour, profile: Profile) -> Result<(), String> {
    REPLAY_SCREEN.with(|cell| cell.set(Some((profile.width, profile.height))));
    let outcome = play_tour(tour, profile);
    REPLAY_SCREEN.with(|cell| cell.set(None));
    outcome
}

fn play_tour(tour: &Tour, profile: Profile) -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.preferences.ui_scale = profile.ui_scale;
    app.preferences.reduced_motion = profile.reduced_motion;
    let ctx = egui::Context::default();
    open_workspace(&mut app, &tour.workspace)?;
    app.tour.start_from_menu(
        &mut app.preferences.tour_progress,
        &tour.id,
        super::super::tour::state::MenuAction::Start,
    );
    let count = tour.steps.len();
    let mut guard = 0;
    while let Some((_, index)) = app.tour.active().map(|(m, s)| (m.id.clone(), s as usize)) {
        guard += 1;
        if guard > count + 2 {
            return Err(format!(
                "tour `{}` did not finish after {guard} steps",
                tour.id
            ));
        }
        let step = tour
            .steps
            .get(index)
            .ok_or_else(|| format!("tour `{}`: step index {index} out of range", tour.id))?;
        paint_step(&ctx, &mut app, tour, step, index, count, profile)?;
        if let Some(wait) = &step.wait_for {
            perform_wait(&ctx, &mut app, wait, tour, step)?;
        }
        authoring::tests::tap(&ctx, &mut app, &lookup("tour.runner.next", &[]))?;
    }
    let record = app
        .preferences
        .tour_progress
        .get(&tour.id)
        .ok_or_else(|| format!("tour `{}` left no progress record", tour.id))?;
    assert_eq!(
        record.state,
        super::super::tour::state::PersistedState::Done,
        "tour `{}` ends as done",
        tour.id
    );
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
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let anchor_rect = anchor::lookup(ctx, &step.target);
    let layout = layout::step_layout(screen(profile), &step.target, anchor_rect, &step.placement);
    assert!(
        screen(profile).contains_rect(layout.card),
        "{}",
        label("card must stay inside the window")
    );
    if step.target != "none" {
        let anchor_rect = anchor_rect
            .ok_or_else(|| label(&format!("anchor `{}` was not registered", step.target)))?;
        let hole = layout
            .hole
            .ok_or_else(|| label("expected a spotlight hole"))?;
        assert!(
            hole.contains(anchor_rect.center()),
            "{}: hole {hole:?} should cover anchor {anchor_rect:?}",
            label("hole mismatch")
        );
    }
    let title = lookup(&step.title, &[]);
    assert!(
        paints_text(ctx, app, &title)?,
        "{}",
        label("card title must paint")
    );
    let counter = pcbcraft_text::lookup(
        "tour.runner.step",
        &[
            ("step", &(index + 1).to_string()),
            ("total", &count.to_string()),
        ],
    );
    assert!(
        paints_text(ctx, app, &counter)?,
        "{}",
        label("step counter must paint")
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
        }
        "selection:part" => {
            let id = app
                .project
                .components
                .first()
                .map(|c| c.id)
                .ok_or_else(|| ctx_label("no part to select"))?;
            app.select(Some(id));
        }
        "selection:any" => {
            app.select(app.project.components.first().map(|c| c.id));
        }
        _ if wait.starts_with("click:") => {
            let anchor = wait.strip_prefix("click:").unwrap_or("");
            let by_anchor = anchor::lookup(ctx, anchor).map(|rect| tap_at(ctx, app, rect.center()));
            match by_anchor {
                Some(done) => done,
                None => tap_id(ctx, app, anchor),
            }
            .or_else(|_| {
                if let Some((menu, item)) = anchor.split_once('.') {
                    let menu = menu.replace('_', " ");
                    let item = item.replace('_', " ");
                    tap_left(ctx, app, &menu)?;
                    tap_left(ctx, app, &item)
                } else {
                    Err(format!("no control named `{anchor}`"))
                }
            })?;
        }
        _ if wait.starts_with("tool:") => {
            let name = wait.strip_prefix("tool:").unwrap_or("");
            tap_left(ctx, app, name)?;
        }
        _ if wait.starts_with("tab:") => {
            let ws = wait.strip_prefix("tab:").unwrap_or("");
            open_workspace(app, ws)?;
            frame(ctx, app, vec![], egui::Modifiers::NONE);
        }
        _ if wait.starts_with("panel_open:") => {
            let panel = wait.strip_prefix("panel_open:").unwrap_or("");
            app.tour_open_panel(panel)?;
        }
        _ if wait.starts_with("command:") => {
            let id = wait.strip_prefix("command:").unwrap_or("");
            app.tool_call(id, serde_json::json!({}));
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
    let _profile = Profile::ALL[0];
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    open_workspace(&mut app, &tour.workspace)?;
    app.tour_start(tour.id.clone(), tour.version);
    app.tour_next();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.tour_escape();
    let saved = app
        .preferences
        .tour_replay
        .get(&tour.id)
        .ok_or("expected saved progress after Esc")?;
    assert_eq!(saved.state, TourState::InProgress);
    let step = saved.step;
    app.tour_start(tour.id.clone(), tour.version);
    assert_eq!(app.tour_playback().map(|p| p.step), Some(step));
    Ok(())
}
