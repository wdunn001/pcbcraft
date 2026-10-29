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
/// left dock, 700 the middle page (Rules, Settings, Checks).
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
    for step in 0..300 {
        let (lane_x, lane_y) = lanes[step % lanes.len()];
        let direction = if step < 150 { -60.0 } else { 60.0 };
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
thread_local! {
    /// When set, `frame` paints on a screen tall enough to show a whole page,
    /// so the tooltip walk can hover controls that sit below the usual fold.
    pub(crate) static TALL_SCREEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub(crate) fn frame(
    ctx: &egui::Context,
    app: &mut App,
    mut events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> egui::FullOutput {
    ctx.enable_accesskit();
    super::audit::record_touches(ctx, app, &events);
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(
                    1440.0,
                    if TALL_SCREEN.with(std::cell::Cell::get) {
                        6000.0
                    } else {
                        940.0
                    },
                ),
            )),
            events,
            ..Default::default()
        },
        |ui| app.show(ui),
    );
    super::audit::remember_labels(&output);
    // This input test has no GPU renderer; intentionally discard texture uploads.
    output.textures_delta.clear();
    output
}

/// Every interactive control the last frame published to assistive
/// technology, as `(Role label, screen rectangle)`.
///
/// Why: the target-size walk must see all widgets, including ones no test
/// knows by name, and the accessibility tree is the one place egui lists them.
///
/// # Rules
/// - Runs two frames so the tree is complete; only roles a pointer can press.
///
/// # Dependencies
/// - `crates/ui-egui/src/target_size.rs` consumes the result.
///
/// # Do not
/// - Use it to find text; it returns controls only.
pub(crate) fn interactive_rects(ctx: &egui::Context, app: &mut App) -> Vec<(String, Rect)> {
    use egui::accesskit::Role;
    ctx.enable_accesskit();
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let mut out = Vec::new();
    if let Some(update) = output.platform_output.accesskit_update {
        for (_, node) in update.nodes {
            let pressable = matches!(
                node.role(),
                Role::Button
                    | Role::CheckBox
                    | Role::RadioButton
                    | Role::ComboBox
                    | Role::TextInput
                    | Role::MultilineTextInput
                    | Role::Slider
                    | Role::SpinButton
                    | Role::Link
                    | Role::Tab
                    | Role::MenuItem
                    | Role::Switch
                    | Role::ColorWell
            );
            if let (true, Some(b)) = (pressable, node.bounds()) {
                let name = format!("{:?} {}", node.role(), node.label().unwrap_or(""));
                out.push((
                    name.trim().to_string(),
                    Rect::from_min_max(
                        egui::pos2(b.x0 as f32, b.y0 as f32),
                        egui::pos2(b.x1 as f32, b.y1 as f32),
                    ),
                ));
            }
        }
    }
    out
}

/// Make the tall-screen mode last for one scope, even when a story bails out.
struct TallScope;
impl TallScope {
    fn enter() -> Self {
        TALL_SCREEN.with(|tall| tall.set(true));
        Self
    }
}
impl Drop for TallScope {
    fn drop(&mut self) {
        TALL_SCREEN.with(|tall| tall.set(false));
    }
}

/// Where the control the gate calls `id` (for example `spinbutton:Board
/// width`) sits right now, found on a screen tall enough for the whole page
/// so a control below the fold needs no scrolling.
fn control_center(ctx: &egui::Context, app: &mut App, id: &str) -> Result<egui::Pos2, String> {
    // A tooltip left by the last control must not cover this one.
    for _ in 0..3 {
        frame(
            ctx,
            app,
            vec![egui::Event::PointerGone],
            egui::Modifiers::NONE,
        );
    }
    let found: Vec<_> = super::audit::controls(ctx, app)
        .into_iter()
        .filter(|c| c.enabled && c.rect.height() >= 1.0 && c.id.as_deref() == Some(id))
        .collect();
    found
        .into_iter()
        .next_back()
        .map(|c| c.rect.center())
        .ok_or_else(|| format!("{id} is not on screen, or is switched off"))
}

/// Click the control the gate calls `id`, like a person pressing it.
pub(crate) fn tap_id(ctx: &egui::Context, app: &mut App, id: &str) -> Result<(), String> {
    let _tall = TallScope::enter();
    let pos = control_center(ctx, app, id)?;
    // Rest the pointer on it first, as a hand does, before pressing.
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(pos)],
        egui::Modifiers::NONE,
    );
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
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

/// Click into a field, replace what it holds with `text` and press Enter.
/// Works for number fields and text boxes alike.
pub(crate) fn type_into(
    ctx: &egui::Context,
    app: &mut App,
    id: &str,
    text: &str,
) -> Result<(), String> {
    tap_id(ctx, app, id)?;
    let _tall = TallScope::enter();
    let key = |key: egui::Key, modifiers: egui::Modifiers| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    };
    frame(
        ctx,
        app,
        vec![key(egui::Key::A, egui::Modifiers::COMMAND)],
        egui::Modifiers::COMMAND,
    );
    frame(
        ctx,
        app,
        vec![egui::Event::Text(text.to_owned())],
        egui::Modifiers::NONE,
    );
    frame(
        ctx,
        app,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
    );
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}
