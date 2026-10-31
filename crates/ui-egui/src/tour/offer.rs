//! Dismissable tour offers and a minimal in-progress panel (#773). The dimmed
//! overlay popover is #770; this only paints chips, the first-launch card and
//! step controls wired to [`super::state::Controller`].

use super::state::{Controller, OfferKind, TourMeta};
use egui::{Align2, Color32, RichText};
use pcbcraft_text::t;

pub(crate) fn paint_offers(
    ui: &mut egui::Ui,
    ctrl: &mut Controller,
    progress: &mut std::collections::BTreeMap<String, super::state::TourRecord>,
    muted: Color32,
    accent: Color32,
) {
    paint_active_runner(ui, ctrl, progress, muted, accent);
    let Some(offer) = ctrl.pending_offer().cloned() else {
        return;
    };
    let Some(meta) = ctrl.meta(&offer.tour_id).cloned() else {
        return;
    };
    match offer.kind {
        OfferKind::FirstLaunchCard => paint_first_launch_card(ui, ctrl, progress, &meta, muted, accent),
        OfferKind::TabChip | OfferKind::VersionChip => {
            paint_tab_chip(ui, ctrl, progress, &meta, muted, accent)
        }
    }
}

fn paint_active_runner(
    ui: &mut egui::Ui,
    ctrl: &mut Controller,
    progress: &mut std::collections::BTreeMap<String, super::state::TourRecord>,
    muted: Color32,
    accent: Color32,
) {
    let Some((meta, step)) = ctrl.active().map(|(m, s)| (m.clone(), s)) else {
        return;
    };
    let ctx = ui.ctx().clone();
    egui::Area::new(egui::Id::new("tour-runner"))
        .anchor(Align2::RIGHT_BOTTOM, [-16.0, -72.0])
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(ui.style().visuals.window_fill)
                .show(ui, |ui| {
                    ui.set_max_width(280.0);
                    ui.label(
                        RichText::new(t!(
                            "tour.runner.step",
                            step = step + 1,
                            total = meta.step_count
                        ))
                        .strong()
                        .color(accent),
                    );
                    ui.horizontal(|ui| {
                        if step > 0
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
                    ui.label(RichText::new(t!("tour.runner.hint")).small().color(muted));
                });
        });
}

fn paint_first_launch_card(
    ui: &mut egui::Ui,
    ctrl: &mut Controller,
    progress: &mut std::collections::BTreeMap<String, super::state::TourRecord>,
    meta: &TourMeta,
    muted: Color32,
    accent: Color32,
) {
    let ctx = ui.ctx().clone();
    let mut open = true;
    egui::Window::new(t!("tour.card.first_launch.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(&ctx, |ui| {
            ui.label(t!("tour.card.first_launch.body"));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button(t!("tour.card.learn_more"))
                    .on_hover_text(t!("tour.card.learn_more.tip"))
                    .clicked()
                {
                    ctrl.accept_offer(progress, &meta.id);
                }
                if ui
                    .button(t!("tour.card.not_now"))
                    .on_hover_text(t!("tour.card.not_now.tip"))
                    .clicked()
                {
                    ctrl.dismiss_first_launch_card();
                }
            });
            ui.label(RichText::new(t!("tour.card.first_launch.footer")).small().color(muted));
        });
    if !open {
        ctrl.dismiss_first_launch_card();
    }
}

fn paint_tab_chip(
    ui: &mut egui::Ui,
    ctrl: &mut Controller,
    progress: &mut std::collections::BTreeMap<String, super::state::TourRecord>,
    meta: &TourMeta,
    muted: Color32,
    accent: Color32,
) {
    let ctx = ui.ctx().clone();
    egui::Area::new(egui::Id::new(("tour-chip", meta.id.as_str())))
        .anchor(Align2::LEFT_TOP, [16.0, 56.0])
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            egui::Frame::NONE
                .fill(ui.style().visuals.extreme_bg_color)
                .stroke(egui::Stroke::new(1.0, accent))
                .inner_margin(8.0)
                .corner_radius(egui::CornerRadius::same(crate::theme::RADIUS_CARD))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let label = pcbcraft_text::lookup(
                            &meta.chip_label_key(),
                            &[("steps", &meta.step_count.to_string())],
                        );
                        if ui
                            .button(label)
                            .on_hover_text(t!("tour.chip.start.tip"))
                            .clicked()
                        {
                            ctrl.accept_offer(progress, &meta.id);
                        }
                        if ui
                            .small_button("×")
                            .on_hover_text(t!("tour.chip.dismiss.tip"))
                            .clicked()
                        {
                            ctrl.dismiss_chip(&meta.id);
                        }
                    });
                    ui.label(RichText::new(t!("tour.chip.hint")).small().color(muted));
                });
        });
}
