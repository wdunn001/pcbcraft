//! Shared helpers for headless ui-egui story tests.

#[cfg(test)]
pub(crate) mod test_support_inner {
    use super::super::*;
    use egui::{Rect, Vec2};

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

pub(crate) fn paints_text(ctx: &egui::Context, app: &mut App, needle: &str) -> Result<bool, String> {
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

pub(crate) fn scrolled_to(ctx: &egui::Context, app: &mut App, needle: &str) -> Result<bool, String> {
    for _ in 0..14 {
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -4.0),
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
            vec![egui::Event::PointerMoved(egui::pos2(150.0, 500.0)), wheel],
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

/// Find the button whose label reads exactly `label`, scrolling the page
/// down and then up if needed, and click it like a person would.
pub(crate) fn tap(ctx: &egui::Context, app: &mut App, label: &str) -> Result<(), String> {
    // Two quiet passes first: hit-testing always trails the painter by a
    // frame, and a scroll area only grows its scrollbar on the second
    // pass, which shifts every row a hair.
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    for attempt in 0..16 {
        let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
        // Later shapes sit on top of earlier ones, so the last visible
        // copy of a label is the one a real click would land on.
        let mut found = None;
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text() == label
            {
                let center = text.pos + text.galley.size() / 2.0;
                if shape.clip_rect.contains(center) {
                    found = Some(center);
                }
            }
        }
        if let Some(pos) = found {
            for pressed in [true, false] {
                frame(
                    ctx,
                    app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    egui::Modifiers::NONE,
                );
            }
            return Ok(());
        }
        let direction = if attempt < 10 { -120.0 } else { 120.0 };
        frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(egui::pos2(720.0, 750.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, direction),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
        // A scrolled frame needs one more pass before its hit-testing
        // matches what the painter just drew, so land the settle frame.
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    Err(format!("{label} never appeared on screen"))
}
}

#[cfg(test)]
pub(crate) use test_support_inner::{frame, paints_text, scrolled_to, self_names, tap};
