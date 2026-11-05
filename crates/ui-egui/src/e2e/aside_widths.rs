use super::super::*;
use super::layers::pcb_app;
use super::support::*;

const WINDOWS: [f32; 5] = [800.0, 1000.0, 1280.0, 1600.0, 1920.0];

fn panel_width(ctx: &egui::Context, id: &str) -> Option<f32> {
    egui::containers::panel::PanelState::load(ctx, egui::Id::new(id)).map(|s| s.outer_rect.width())
}

fn settle(ctx: &egui::Context, app: &mut App, width: f32) {
    for _ in 0..5 {
        replay_frame(ctx, app, width, 900.0, |_| {});
    }
}

/// The widths the two asides take after the window settles.
fn measured(ctx: &egui::Context, tab: Tab) -> Result<(f32, f32), String> {
    let left_id = if tab == Tab::Library {
        "left-aside-library"
    } else {
        "left-aside"
    };
    let left = panel_width(ctx, left_id).ok_or("the left aside never showed")?;
    let right = panel_width(ctx, "inspector").ok_or("the right aside never showed")?;
    Ok((left, right))
}

#[test]
fn a_newcomer_keeps_the_drawing_on_a_regular_monitor() -> Result<(), String> {
    for tab in [Tab::Schematic, Tab::Pcb, Tab::Library] {
        for width in WINDOWS {
            let (ctx, mut app) = pcb_app();
            app.change_tab(tab);
            settle(&ctx, &mut app, width);
            let (left, right) = measured(&ctx, tab)?;
            let rule = aside::aside_widths(width, tab == Tab::Library);
            let edge = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("inspector"))
                .map_or(0.0, |s| s.outer_rect.max.x);
            assert!(
                edge <= width + 0.5,
                "{tab:?} at {width}: the right aside ends at {edge}, past the window"
            );
            let centre = width - left - right;
            assert!(
                centre >= width * 0.4 - 0.5,
                "{tab:?} at {width}: the drawing keeps {centre} of {width} (left {left}, right {right})"
            );
            assert!(
                left >= rule.left_min - 0.5 && left <= rule.left_max + 0.5,
                "{tab:?} at {width}: left aside {left} is outside {}..{}",
                rule.left_min,
                rule.left_max
            );
            assert!(
                right >= rule.right_min - 0.5 && right <= rule.right_max + 0.5,
                "{tab:?} at {width}: right aside {right} is outside {}..{}",
                rule.right_min,
                rule.right_max
            );
        }
    }
    Ok(())
}

#[test]
fn a_newcomer_keeps_a_dragged_width_when_it_fits() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    app.change_tab(Tab::Schematic);
    let saved = egui::containers::panel::PanelState {
        outer_rect: Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(260.0, 900.0)),
    };
    ctx.data_mut(|d| d.insert_persisted(egui::Id::new("left-aside"), saved));
    settle(&ctx, &mut app, 1600.0);
    let (left, _) = measured(&ctx, Tab::Schematic)?;
    assert!(
        (left - 260.0).abs() < 1.0,
        "a width the person dragged to is kept: {left}"
    );
    let huge = egui::containers::panel::PanelState {
        outer_rect: Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(900.0, 900.0)),
    };
    ctx.data_mut(|d| d.insert_persisted(egui::Id::new("left-aside"), huge));
    settle(&ctx, &mut app, 1000.0);
    let (left, right) = measured(&ctx, Tab::Schematic)?;
    assert!(
        1000.0 - left - right >= 400.0 - 0.5,
        "a saved width too wide for the window is clamped: left {left}, right {right}"
    );
    Ok(())
}
