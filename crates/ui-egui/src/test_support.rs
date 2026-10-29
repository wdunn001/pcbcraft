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

    pub(crate) fn scrolled_to(
        ctx: &egui::Context,
        app: &mut App,
        needle: &str,
    ) -> Result<bool, String> {
        scrolled_to_at(ctx, app, needle, 150.0)
    }

    pub(crate) fn scrolled_to_at(
        ctx: &egui::Context,
        app: &mut App,
        needle: &str,
        x: f32,
    ) -> Result<bool, String> {
        // One fixed pointer can land on a text field, which swallows the
        // wheel; step down the same column so the scroll always finds a gap.
        let lanes = [(x, 500.0), (x, 300.0), (x, 700.0), (x, 880.0)];
        for step in 0..80 {
            let (lane_x, lane_y) = lanes[step % lanes.len()];
            // Small point-sized steps: a line-sized jump can sail a short
            // button clean past the viewport between two polls.
            let wheel = egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -60.0),
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

    /// Find the button whose label reads exactly `label`, scrolling the page
    /// down and then up if needed, and click it like a person would.
    pub(crate) fn tap(ctx: &egui::Context, app: &mut App, label: &str) -> Result<(), String> {
        // Two quiet passes first: hit-testing always trails the painter by a
        // frame, and a scroll area only grows its scrollbar on the second
        // pass, which shifts every row a hair.
        frame(ctx, app, vec![], egui::Modifiers::NONE);
        frame(ctx, app, vec![], egui::Modifiers::NONE);
        // Where to park the pointer when a label sits off screen: the middle
        // page low and high, the settings column, and the left dock. A spot
        // that nothing moves under (a text field swallowing the wheel, or
        // the canvas zooming instead) is given up on after two still frames.
        let lanes = [
            egui::pos2(720.0, 750.0),
            egui::pos2(720.0, 400.0),
            egui::pos2(300.0, 500.0),
            egui::pos2(150.0, 500.0),
        ];
        // Later shapes sit on top of earlier ones, so the last visible
        // copy of a label is the one a real click would land on.
        let find = |output: &egui::FullOutput| -> Option<egui::Pos2> {
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
            found
        };
        // A fingerprint of where the words sit: if it repeats, the wheel
        // is being swallowed here and there is no point turning it more.
        let witness = |output: &egui::FullOutput| -> Vec<(i32, i32)> {
            let mut marks: Vec<(i32, i32)> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => {
                        Some((text.pos.x.round() as i32, text.pos.y.round() as i32))
                    }
                    _ => None,
                })
                .collect();
            marks.sort_unstable();
            marks.truncate(40);
            marks
        };
        for lane in lanes {
            for direction in [-60.0, 90.0] {
                let mut stills = 0;
                while stills < 2 {
                    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
                    if let Some(pos) = find(&output) {
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
                    let before = witness(&output);
                    let wheel = egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, direction),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    };
                    let after = frame(
                        ctx,
                        app,
                        vec![egui::Event::PointerMoved(lane), wheel],
                        egui::Modifiers::NONE,
                    );
                    if witness(&after) == before {
                        stills += 1;
                    } else {
                        stills = 0;
                    }
                    // A scrolled frame needs one more pass before its
                    // hit-testing matches what the painter just drew.
                    frame(ctx, app, vec![], egui::Modifiers::NONE);
                }
            }
        }
        Err(format!("{label} never appeared on screen"))
    }
}

#[cfg(test)]
pub(crate) use test_support_inner::{
    frame, paints_text, scrolled_to, scrolled_to_at, self_names, tap,
};
