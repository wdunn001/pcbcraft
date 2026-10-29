//! Helpers shared by every headless UI story and by the unit tests in `lib.rs`.

use super::super::*;

/// True when the named words appear anywhere on screen this frame or the
/// next two, so tests can assert what a person actually sees.
pub(crate) fn paints_text(
    ctx: &egui::Context,
    app: &mut App,
    needle: &str,
) -> Result<bool, String> {
    for _ in 0..3 {
        let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
        if output.shapes.iter().any(|shape| {
            if let egui::Shape::Text(text) = &shape.shape {
                return text.galley.text().contains(needle);
            }
            false
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Scroll the right-aside layers tree until `needle` appears, then nudge
/// the wheel so rows just below it are visible too.
pub(crate) fn scrolled_to(
    ctx: &egui::Context,
    app: &mut App,
    needle: &str,
) -> Result<bool, String> {
    scrolled_to_at(ctx, app, needle, 1280.0)
}
/// Same, but over a panel whose horizontal centre is at `x`: 150 is the
/// left dock, 300 the middle page (Rules, Settings, Checks).
pub(crate) fn scrolled_to_at(
    ctx: &egui::Context,
    app: &mut App,
    needle: &str,
    x: f32,
) -> Result<bool, String> {
    // One fixed pointer can land on a text field, which swallows the
    // wheel; step down the same column so the scroll always finds a gap.
    let lanes = [(x, 500.0), (x, 300.0), (x, 700.0), (x, 880.0)];
    // Down first, then back up, in case the label sits above where an earlier
    // search left the scroll.
    for step in 0..160 {
        let (lane_x, lane_y) = lanes[step % lanes.len()];
        let direction = if step < 80 { -60.0 } else { 60.0 };
        // Small point-sized steps: a line-sized jump can sail a short
        // button clean past the viewport between two polls.
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, direction),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        };
        let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
        let clickable = output.shapes.iter().any(|shape| {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text().contains(needle)
            {
                let center = text.pos + text.galley.size() / 2.0;
                return shape.clip_rect.contains(center);
            }
            false
        });
        if clickable {
            return Ok(true);
        }
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(egui::pos2(lane_x, lane_y)), wheel],
            egui::Modifiers::NONE,
        );
    }
    Ok(false)
}
pub(crate) fn self_names(app: &App) -> Vec<String> {
    app.project
        .board
        .layer_records
        .iter()
        .map(|r| r.name.clone())
        .collect()
}
pub(crate) fn frame(
    ctx: &egui::Context,
    app: &mut App,
    mut events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> egui::FullOutput {
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1440.0, 940.0),
            )),
            events,
            ..Default::default()
        },
        |ui| app.show(ui),
    );
    // This input test has no GPU renderer; intentionally discard texture uploads.
    output.textures_delta.clear();
    output
}
