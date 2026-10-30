//! Test helpers shared by the kit's unit tests: one headless frame with the
//! theme and fonts installed and the accessibility tree on.
//!
//! Context: every kit widget test runs through `frame` so they all see the same
//! screen size and settings.
//! Local rules: test-only; production code never calls into this file.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/fonts.rs
//!
//! # Do not
//! - Add assertions here; helpers only drive frames.
use crate::theme::{self, Tokens};
use egui::{Context, Event, FullOutput, Key, Modifiers, Pos2, RawInput, Rect, Ui, Vec2};

/// A context ready for kit tests: theme applied, fonts installed, accesskit on.
pub fn context(dark: bool) -> Context {
    let ctx = Context::default();
    ctx.enable_accesskit();
    theme::apply(&ctx, &Tokens::for_dark(dark));
    crate::fonts::install(&ctx);
    ctx
}

/// Run one frame with `events` and draw with `f`.
pub fn frame(ctx: &Context, events: Vec<Event>, f: impl FnMut(&mut Ui)) -> FullOutput {
    let mut out = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            events,
            ..Default::default()
        },
        f,
    );
    out.textures_delta.clear();
    out
}

/// A key press followed by its release, as two events.
pub fn key(key: Key) -> Vec<Event> {
    [true, false]
        .into_iter()
        .map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        })
        .collect()
}

/// A left click at `pos`: move, press, release.
pub fn click(pos: Pos2) -> Vec<Event> {
    let mut events = vec![Event::PointerMoved(pos)];
    for pressed in [true, false] {
        events.push(Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    events
}
