//! Tour spotlight overlay and step card (#770).
//!
//! While a tour runs, the window is dimmed everywhere except a cutout around
//! the control the step names, and a card with the step title, body and the
//! Back, Skip and Next buttons sits beside the cutout. The dim layer paints
//! only; it never takes a click, so the highlighted control and the rest of
//! the window keep working. Geometry comes from [`super::layout::step_layout`]
//! and the target rectangle from the anchor registry, recomputed every frame
//! so scroll, resize and scale changes follow.
//!
//! # Rules
//! - The card words are catalog keys read from the tour data, never literals.
//! - A tour that is not in the data (a test fixture) gets no overlay and the
//!   caller falls back to the small step panel.
//!
//! # Dependencies
//! - crates/ui-egui/src/tour/anchor.rs
//! - crates/ui-egui/src/tour/data.rs
//! - crates/ui-egui/src/tour/layout.rs
//! - crates/ui-egui/src/tour/state.rs
//! - crates/text/strings/en.txt
//!
//! # Do not
//! - Make the dim layer sense clicks; a person must be able to use the
//!   highlighted control.

use super::data::{Doc, Step, Tour};
use super::layout::{StepLayout, step_layout};
use super::state::{Controller, TourRecord};
use egui::{
    Align2, Color32, Context, EventFilter, Id, LayerId, Order, Rect, RichText, Stroke, StrokeKind,
};
use pcbcraft_text::{lookup, t};
use std::collections::BTreeMap;

const DIM_ALPHA: u8 = 150;

/// Paint the overlay and card for the running tour step.
///
/// Returns true when a card was painted. False means no tour is running or the
/// running tour has no step data, so the caller may show the plain panel.
pub(crate) fn paint_step_overlay(
    ctx: &Context,
    ctrl: &mut Controller,
    progress: &mut BTreeMap<String, TourRecord>,
    tours: &Doc,
    muted: Color32,
    accent: Color32,
) -> bool {
    let Some((meta, index)) = ctrl.active().map(|(m, s)| (m.clone(), s as usize)) else {
        return false;
    };
    let Some(tour) = tours.tour.iter().find(|t| t.id == meta.id) else {
        return false;
    };
    let Some(step) = tour.steps.get(index) else {
        return false;
    };
    let screen = ctx.content_rect();
    let target = super::anchor::lookup(ctx, &step.target);
    let layout = step_layout(screen, &step.target, target, &step.placement);
    paint_dim(ctx, screen, &layout, accent);
    let shown = Shown { tour, step, index };
    paint_card(ctx, ctrl, progress, &shown, &layout, muted, accent);
    handle_keyboard(ctx, ctrl, progress);
    true
}

/// Dim the window around the cutout, or everywhere when the step has no target.
fn paint_dim(ctx: &Context, screen: Rect, layout: &StepLayout, accent: Color32) {
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("tour-dim")));
    let dim = Color32::from_black_alpha(DIM_ALPHA);
    let Some(hole) = layout.hole else {
        painter.rect_filled(screen, 0.0, dim);
        return;
    };
    let strips = [
        Rect::from_min_max(screen.min, egui::pos2(screen.max.x, hole.min.y)),
        Rect::from_min_max(egui::pos2(screen.min.x, hole.max.y), screen.max),
        Rect::from_min_max(
            egui::pos2(screen.min.x, hole.min.y),
            egui::pos2(hole.min.x, hole.max.y),
        ),
        Rect::from_min_max(
            egui::pos2(hole.max.x, hole.min.y),
            egui::pos2(screen.max.x, hole.max.y),
        ),
    ];
    for strip in strips {
        if strip.width() > 0.0 && strip.height() > 0.0 {
            painter.rect_filled(strip, 0.0, dim);
        }
    }
    painter.rect_stroke(
        hole,
        crate::theme::RADIUS_CARD,
        Stroke::new(2.0, accent),
        StrokeKind::Outside,
    );
}

/// The step being shown and where it sits in its tour.
#[derive(Clone, Copy)]
struct Shown<'a> {
    tour: &'a Tour,
    step: &'a Step,
    index: usize,
}

fn paint_card(
    ctx: &Context,
    ctrl: &mut Controller,
    progress: &mut BTreeMap<String, TourRecord>,
    shown: &Shown<'_>,
    layout: &StepLayout,
    muted: Color32,
    accent: Color32,
) {
    let Shown { tour, step, index } = *shown;
    egui::Area::new(Id::new("tour-card"))
        .order(Order::Tooltip)
        .fixed_pos(layout.card.min)
        .pivot(Align2::LEFT_TOP)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(ui.style().visuals.window_fill)
                .show(ui, |ui| {
                    ui.set_width(layout.card.width());
                    ui.label(
                        RichText::new(lookup(&step.title, &[]))
                            .strong()
                            .color(accent),
                    );
                    ui.label(lookup(&step.body, &[]));
                    ui.add_space(6.0);
                    let step_text = t!(
                        "tour.runner.step",
                        step = index + 1,
                        total = tour.steps.len()
                    );
                    let counter = ui
                        .label(crate::fonts::caption(&step_text).color(muted))
                        .on_hover_text(&step_text);
                    ui.ctx().accesskit_node_builder(counter.id, |node| {
                        node.set_role(egui::accesskit::Role::Status);
                        node.set_live(egui::accesskit::Live::Polite);
                        node.set_label(step_text.as_str());
                    });
                    ui.horizontal(|ui| {
                        if index > 0
                            && ui
                                .button(t!("tour.runner.back"))
                                .on_hover_text(t!("tour.runner.back.tip"))
                                .clicked()
                        {
                            ctrl.back(progress);
                        }
                        if ui
                            .button(t!("tour.runner.skip"))
                            .on_hover_text(t!("tour.runner.skip.tip"))
                            .clicked()
                        {
                            ctrl.skip(progress);
                        }
                        if ui
                            .button(t!("tour.runner.next"))
                            .on_hover_text(t!("tour.runner.next.tip"))
                            .clicked()
                        {
                            ctrl.advance(progress);
                        }
                    });
                });
        });
}

/// Handle keyboard navigation for the tour card.
fn handle_keyboard(
    ctx: &Context,
    ctrl: &mut Controller,
    progress: &mut BTreeMap<String, TourRecord>,
) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ctrl.skip(progress);
    }

    // Focus lock: keep Tab within the tour card buttons
    let card_id = Id::new("tour-card");
    if ctx.memory(|m| m.focused()) == Some(card_id) {
        ctx.memory_mut(|m| {
            m.set_focus_lock_filter(
                card_id,
                EventFilter {
                    horizontal_arrows: true,
                    ..Default::default()
                },
            );
        });
    }

    // Left/Right arrow keys step backward/forward when no text field has focus
    if ctrl.active().is_some() {
        // A text field that has focus wants the keys itself, so the arrows stay with it.
        let has_text_focus = ctx.egui_wants_keyboard_input();
        if !has_text_focus {
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                ctrl.advance(progress);
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                ctrl.back(progress);
            }
        }
    }
}
