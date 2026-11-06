//! Tour spotlight overlay and navigation card.
//!
//! Renders a dimmed overlay with a spotlight "hole" cut out around the target
//! widget, plus a floating card with title, body, and navigation buttons.
//!
//! Architecture: the overlay is drawn every frame while a tour is active.
//! It queries the anchor registry for the target rect, computes card placement
//! via the layout module, and renders the overlay + card.
//!
//! Key decisions:
//! - Four rectangles around the target, not a hole mask (wgpu/browser compat)
//! - Card placement follows the chosen side, with fallback to the opposite
//!   side if it would overflow the screen.
//! - Recomputed every frame to handle scroll, resize, scale.
//!
//! # Dependencies
//! - crates/ui-egui/src/tour/anchor.rs
//! - crates/ui-egui/src/tour/layout.rs
//! - crates/ui-egui/src/tour/state.rs

use eframe::egui;
use std::collections::HashMap;

use super::anchor::{AnchorId, anchor_registry};
use super::layout::{self, Layout};
use super::state::{Controller, TourState};

// Tour overlay styling. These are hardcoded to avoid needing a theme reference;
// the tour overlay should look consistent across themes.
const OVERLAY_COLOR: [f32; 4] = [0.1, 0.1, 0.15, 0.6];
const CARD_BG: [f32; 4] = [0.95, 0.95, 0.97, 1.0];
const CARD_BORDER: [f32; 4] = [0.7, 0.7, 0.75, 1.0];
const CARD_TEXT: [f32; 4] = [0.2, 0.2, 0.25, 1.0];
const CARD_SUBTEXT: [f32; 4] = [0.5, 0.5, 0.55, 1.0];
const BUTTON_BG: [f32; 4] = [0.85, 0.85, 0.88, 1.0];
const BUTTON_BG_HOVER: [f32; 4] = [0.75, 0.75, 0.8, 1.0];
const BUTTON_TEXT: [f32; 4] = [0.2, 0.2, 0.25, 1.0];
const PROGRESS_TEXT: [f32; 4] = [0.4, 0.4, 0.45, 1.0];

/// Render the tour overlay for the current step.
///
/// Returns `true` if the tour should continue (no navigation action taken),
/// `false` if the user clicked Next, Back, or Skip.
///
/// # Panics
/// - If `step_index` is out of range for `controller.steps()`
pub fn paint(
    ctx: &egui::Context,
    controller: &Controller,
    step_index: usize,
    anchor_id: AnchorId,
) -> bool {
    let steps = controller.steps();
    if step_index >= steps.len() {
        panic!("tour overlay: step_index out of range");
    }

    let step = &steps[step_index];

    // Query the anchor registry for the target rect.
    let target = match anchor_registry().get(&anchor_id) {
        Some(rect) => rect,
        None => {
            // Target not registered or not visible; draw card centered.
            // This can happen if the widget hasn't been drawn yet this frame.
            let display = ctx.screen_rect();
            let layout = layout::Layout {
                card: egui::Rect::from_center_size(
                    display.center(),
                    egui::vec2(layout::CARD_MAX.x, 120.0),
                ),
                hole: step.target,
                side: layout::Side::Bottom,
                padding: layout::HOLE_PAD,
            };
            paint_overlay_and_card(ctx, controller, step_index, &layout, step, display);
            return true;
        }
    };

    let display = ctx.screen_rect();
    let layout = match layout::step_layout(target, display, step.placement) {
        Ok(layout) => layout,
        Err(msg) => {
            // Layout computation failed; fall back to centered card.
            let layout = layout::Layout {
                card: egui::Rect::from_center_size(
                    display.center(),
                    egui::vec2(layout::CARD_MAX.x, 120.0),
                ),
                hole: target,
                side: layout::Side::Bottom,
                padding: layout::HOLE_PAD,
            };
            paint_overlay_and_card(ctx, controller, step_index, &layout, step, display);
            return true;
        }
    };

    let handled = paint_overlay_and_card(ctx, controller, step_index, &layout, step, display);
    handled
}

fn paint_overlay_and_card(
    ctx: &egui::Context,
    controller: &Controller,
    step_index: usize,
    layout: &Layout,
    step: &layout::Step,
    display: egui::Rect,
) -> bool {
    // Paint dim overlay (four rectangles around the hole).
    paint_overlay(ctx, &layout.hole, display);

    // Paint the card.
    let handled = paint_card(ctx, controller, step_index, layout, step);
    handled
}

/// Paint the dimmed overlay as four rectangles around the target.
fn paint_overlay(ctx: &egui::Context, target: &egui::Rect, display: egui::Rect) {
    // Top
    ctx.fill(
        egui::Rect::from_min_max(display.min, egui::pos2(display.max.x, target.min.y)),
        egui::Color32::from_array(OVERLAY_COLOR),
    );

    // Bottom
    ctx.fill(
        egui::Rect::from_min_max(egui::pos2(display.min.x, target.max.y), display.max),
        egui::Color32::from_array(OVERLAY_COLOR),
    );

    // Left
    ctx.fill(
        egui::Rect::from_min_max(
            egui::pos2(display.min.x, target.min.y),
            egui::pos2(target.min.x, target.max.y),
        ),
        egui::Color32::from_array(OVERLAY_COLOR),
    );

    // Right
    ctx.fill(
        egui::Rect::from_min_max(
            egui::pos2(target.max.x, target.min.y),
            egui::pos2(display.max.x, target.max.y),
        ),
        egui::Color32::from_array(OVERLAY_COLOR),
    );

    // Rounded outline around the hole
    let outline_rect = target.expand(layout::HOLE_PAD * 0.5);
    ctx.stroke(
        &outline_rect,
        2.0,
        egui::Color32::from_array([0.9, 0.9, 1.0, 0.8]),
        egui::CornerRadius::uniform(8.0),
    );
}

/// Paint the tour card with title, body, and navigation buttons.
fn paint_card(
    ctx: &egui::Context,
    controller: &Controller,
    step_index: usize,
    layout: &Layout,
    step: &layout::Step,
) -> bool {
    let card_rect = layout.card;

    // Card background
    ctx.fill(
        card_rect,
        egui::Color32::from_array(CARD_BG),
        egui::CornerRadius::uniform(6.0),
    );

    // Card border
    ctx.stroke(
        &card_rect,
        1.0,
        egui::Color32::from_array(CARD_BORDER),
        egui::CornerRadius::uniform(6.0),
    );

    // Title
    let title_color = egui::Color32::from_array(CARD_TEXT);
    let mut title = ctx.fonts().layout_text(
        &step.title,
        egui::TextOrientation::LeftToRight,
        card_rect.width() - 32.0,
        egui::FontSelection::new(egui::TextStyle::Heading),
    );
    let title_size = title.size();
    let title_pos = egui::pos2(card_rect.min.x + 16.0, card_rect.min.y + 16.0);
    ctx.text(title_pos, title_color, step.title);

    // Body
    let body_color = egui::Color32::from_array(CARD_SUBTEXT);
    let body_pos = egui::pos2(
        card_rect.min.x + 16.0,
        card_rect.min.y + 32.0 + title_size.y,
    );
    let body_width = card_rect.width() - 32.0;
    let body = ctx.fonts().layout_text(
        &step.body,
        egui::TextOrientation::LeftToRight,
        body_width,
        egui::FontSelection::new(egui::TextStyle::Body),
    );
    ctx.text(body_pos, body_color, step.body);

    // Step counter
    let total = controller.steps().len();
    let counter_text = format!("Step {} of {}", step_index + 1, total);
    let counter_color = egui::Color32::from_array(PROGRESS_TEXT);
    let counter = ctx.fonts().layout_text(
        &counter_text,
        egui::TextOrientation::LeftToRight,
        0.0,
        egui::FontSelection::new(egui::TextStyle::Small),
    );
    let counter_pos = egui::pos2(
        card_rect.max.x - 16.0 - counter.size().x,
        card_rect.max.y - 40.0,
    );
    ctx.text(counter_pos, counter_color, counter_text);

    // Buttons: Back, Next/Skip
    let btn_height = 28.0;
    let btn_y = card_rect.max.y - 36.0;

    // Back button
    let back_enabled = step_index > 0;
    let back_width = 60.0;
    let back_rect = egui::Rect::from_min_max(
        egui::pos2(card_rect.min.x + 16.0, btn_y),
        egui::pos2(card_rect.min.x + 16.0 + back_width, btn_y + btn_height),
    );
    paint_button(ctx, back_rect, "Back", back_enabled);
    if back_enabled
        && ctx
            .input()
            .pointer
            .clicked(egui::PointerButton::Primary, back_rect)
    {
        controller.back();
        return false;
    }

    // Next or Skip button
    let last = step_index == total - 1;
    let next_text = if last { "Skip tour" } else { "Try it" };
    let next_width = 90.0;
    let next_rect = egui::Rect::from_min_max(
        egui::pos2(card_rect.max.x - 16.0 - next_width, btn_y),
        egui::pos2(card_rect.max.x - 16.0, btn_y + btn_height),
    );
    paint_button(ctx, next_rect, next_text, true);
    if ctx
        .input()
        .pointer
        .clicked(egui::PointerButton::Primary, next_rect)
    {
        if last {
            controller.skip();
        } else {
            controller.advance();
        }
        return false;
    }

    // Escape key
    if ctx.input().key_pressed(egui::Key::Escape) {
        controller.skip();
        return false;
    }

    // Left/Right arrow keys
    if step_index < total && !step.body.is_empty() {
        // Only step on arrows if no text field is focused (heuristic: body text)
        if ctx.input().key_pressed(egui::Key::ArrowLeft) && back_enabled {
            controller.back();
            return false;
        }
        if ctx.input().key_pressed(egui::Key::ArrowRight) {
            if last {
                controller.skip();
            } else {
                controller.advance();
            }
            return false;
        }
    }

    true
}

fn paint_button(ctx: &egui::Context, rect: egui::Rect, text: &str, enabled: bool) {
    let bg = if enabled {
        BUTTON_BG
    } else {
        [0.9, 0.9, 0.9, 1.0]
    };
    ctx.fill(
        rect,
        egui::Color32::from_array(bg),
        egui::CornerRadius::uniform(4.0),
    );
    ctx.stroke(
        &rect,
        1.0,
        egui::Color32::from_array(CARD_BORDER),
        egui::CornerRadius::uniform(4.0),
    );

    let text_color = if enabled {
        BUTTON_TEXT
    } else {
        [0.6, 0.6, 0.6, 1.0]
    };
    let text_rect = rect.expand(4.0);
    let text_size = ctx.fonts().layout_text(
        text,
        egui::TextOrientation::LeftToRight,
        0.0,
        egui::FontSelection::new(egui::TextStyle::Small),
    );
    let text_pos = egui::pos2(
        rect.center().x - text_size.size().x / 2.0,
        rect.center().y - text_size.size().y / 2.0,
    );
    ctx.text(text_pos, egui::Color32::from_array(text_color), text);
}
