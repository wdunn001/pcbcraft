//! Cards in the conversation: step runs, the review card, the undo card and next steps.
//!
//! Context: drawn by `pane.rs`; each function draws one transcript item and returns what the
//! user pressed so the pane (which owns the app) acts on it. Layout follows the owner-approved
//! mockups (assistant-2-run, -3-approve, -3b-undo, -6-next in the app-design set).
//! Rules: every control has a tooltip; state is shape and word as well as colour; commands show
//! only behind a collapsed "Show commands"; Enter applies and Escape discards a waiting card.
//! Dependencies: `crates/ui-egui/src/ui_kit/{button,chip,card,icons}.rs`, `theme.rs`, `fonts.rs`.
//! Do not: change the design from here; return a `Pressed` and let the pane act.

use super::{steps, text};
use crate::fonts;
use crate::theme::{self, RADIUS_CARD, Tokens};
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::chip::{BadgeKind, badge};
use crate::ui_kit::icons;
use pcbcraft_assistant::{Item, StepState, Suggestion, ToolStatus};

/// What the user pressed on a card.
pub(super) enum Pressed {
    Allow(u64),
    AlwaysAllow(u64),
    Deny(u64),
    /// Apply a batch one command at a time.
    ApplyEach(u64),
    Undo,
    /// Open or close the run that starts at this step id.
    ToggleRun(u64),
    /// Show or hide the name of every changed object.
    ToggleList,
    /// A next-step card: ask for this.
    Pick(String),
}

/// What a card needs to know besides its item.
#[derive(Clone, Copy)]
pub(super) struct View {
    pub(super) muted: egui::Color32,
    pub(super) undo_live: bool,
    pub(super) list_all: bool,
}

/// Draw one transcript item.
pub(super) fn item(ui: &mut egui::Ui, item: &Item, view: &View) -> Option<Pressed> {
    let (muted, undo_live) = (view.muted, view.undo_live);
    match item {
        Item::User { text } => {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("You").small().color(muted));
                ui.add(egui::Label::new(text.as_str()).wrap());
            });
            None
        }
        Item::Assistant { text } => {
            ui.add(egui::Label::new(text.as_str()).wrap());
            None
        }
        Item::Thought { text } => {
            ui.add(
                egui::Label::new(egui::RichText::new(text.as_str()).italics().color(muted)).wrap(),
            );
            None
        }
        Item::ToolCall {
            title,
            status,
            content,
            ..
        } => {
            let word = match status {
                Some(ToolStatus::Completed) => "done",
                Some(ToolStatus::Failed) => "failed",
                Some(ToolStatus::InProgress) => "running",
                _ => "waiting",
            };
            let response = ui.label(
                egui::RichText::new(format!("{title}  ·  {word}"))
                    .small()
                    .color(muted),
            );
            if let Some(c) = content {
                response.on_hover_text(c.chars().take(400).collect::<String>());
            }
            None
        }
        Item::Plan(entries) => {
            for (step, state) in entries {
                ui.label(
                    egui::RichText::new(format!("{state}: {step}"))
                        .small()
                        .color(muted),
                );
            }
            None
        }
        Item::Permission { title, answer, .. } => {
            ui.label(format!(
                "{title}: {}",
                answer.as_deref().unwrap_or("waiting")
            ));
            None
        }
        Item::Approval {
            id,
            title,
            detail,
            summary,
            answer,
        } => approval(ui, *id, title, detail, summary.as_ref(), *answer, view),
        // Steps are drawn together by `run`, suggestions by `suggestions`.
        Item::Step { .. } | Item::Suggestions(_) => None,
        Item::Queued { text } => {
            ui.label(egui::RichText::new(format!("Queued: {text}")).color(muted))
                .on_hover_text(text::queued_tip());
            None
        }
        Item::Note { text, error } => {
            let color = if *error {
                egui::Color32::LIGHT_RED
            } else {
                muted
            };
            ui.add(
                egui::Label::new(egui::RichText::new(text.as_str()).small().color(color)).wrap(),
            );
            None
        }
        Item::UndoStep { changes, undone } => undo_card(ui, *changes, *undone, undo_live),
    }
}

/// A bordered panel: the frame of every card in the pane.
fn frame(t: &Tokens) -> egui::Frame {
    egui::Frame::new()
        .fill(t.panel)
        .stroke(egui::Stroke::new(theme::BORDER, t.line))
        .corner_radius(egui::CornerRadius::same(RADIUS_CARD))
        .inner_margin(egui::Margin::same(12))
}

/// The small caption at the top of a card, with an optional badge on its right.
fn caption(ui: &mut egui::Ui, t: &Tokens, word: &str, badge_word: Option<(&str, BadgeKind)>) {
    ui.horizontal(|ui| {
        ui.label(fonts::section_caption(word).color(t.muted));
        if let Some((w, kind)) = badge_word {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                badge(ui, w, kind, word);
            });
        }
    });
}

/// "under 1 s" or "7 s".
fn took(ms: u64) -> String {
    if ms < 1000 {
        text::step_quick()
    } else {
        text::step_secs(ms / 1000)
    }
}

/// The state mark of a step: a shape, never colour alone.
fn state_mark(ui: &mut egui::Ui, t: &Tokens, state: StepState) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    let word = match state {
        StepState::Waiting => text::step_state_waiting(),
        StepState::Running => text::step_state_running(),
        StepState::Done => text::step_state_done(),
        StepState::Failed => text::step_state_failed(),
    };
    response.on_hover_text(word);
    let p = ui.painter();
    let c = rect.center();
    match state {
        StepState::Done => {
            p.circle_stroke(c, 7.0, egui::Stroke::new(1.4, t.mask));
            (icons::CHECK.0)(p, rect.shrink(3.0), t.mask);
        }
        StepState::Failed => {
            p.circle_stroke(c, 7.0, egui::Stroke::new(1.4, t.red));
            (icons::CLOSE.0)(p, rect.shrink(3.0), t.red);
        }
        StepState::Running => {
            p.circle_stroke(c, 7.0, egui::Stroke::new(1.4, t.copper));
            p.circle_filled(c, 2.5, t.copper);
        }
        StepState::Waiting => {
            p.circle_stroke(c, 7.0, egui::Stroke::new(1.0, t.muted));
        }
    }
}

/// One run of steps. A finished run is one summary line until opened.
pub(super) fn run(
    ui: &mut egui::Ui,
    steps: &[&Item],
    working: bool,
    open: bool,
) -> Option<Pressed> {
    let t = theme::tokens(ui.ctx());
    let first = steps
        .iter()
        .find_map(|i| match i {
            Item::Step { id, .. } => Some(*id),
            _ => None,
        })
        .unwrap_or(0);
    let total_ms: u64 = steps
        .iter()
        .map(|i| {
            if let Item::Step { ms, .. } = i {
                *ms
            } else {
                0
            }
        })
        .sum();
    let secs = took(total_ms);
    let (title, tip) = if working {
        (text::run_working(), text::run_working_tip())
    } else {
        (text::run_summary(steps.len(), &secs), text::run_tip())
    };
    let show = open || working;
    let mut pressed = None;
    frame(&t)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let header = egui::CollapsingHeader::new(fonts::strong(&title))
                .id_salt(("assistant-run", first))
                .open(Some(show))
                .show(ui, |ui| {
                    for step in steps {
                        ui.add_space(4.0);
                        step_row(ui, &t, step);
                    }
                });
            header.header_response.clone().on_hover_text(tip);
            if header.header_response.clicked() && !working {
                pressed = Some(Pressed::ToggleRun(first));
            }
        });
    pressed
}

/// One step: mark, plain name, duration, the one-line result and the hidden commands.
fn step_row(ui: &mut egui::Ui, t: &Tokens, step: &Item) {
    let Item::Step {
        id,
        title,
        state,
        ms,
        result,
        commands,
    } = step
    else {
        return;
    };
    ui.horizontal(|ui| {
        state_mark(ui, t, *state);
        let name = ui.label(fonts::strong(title));
        name.on_hover_text(text::step_tip());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if matches!(state, StepState::Done | StepState::Failed) {
                ui.label(fonts::mono(took(*ms)).color(t.muted));
            }
        });
    });
    if !result.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.add(egui::Label::new(fonts::caption(result).color(t.muted)).wrap());
        });
    }
    if !commands.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            egui::CollapsingHeader::new(fonts::caption(text::step_commands()).color(t.muted))
                .id_salt(("assistant-commands", *id))
                .show(ui, |ui| {
                    for c in commands {
                        ui.label(fonts::mono(c));
                    }
                })
                .header_response
                .on_hover_text(text::step_commands_tip());
        });
    }
}

/// The review card for a waiting change, or its collapsed line once answered.
fn approval(
    ui: &mut egui::Ui,
    id: u64,
    title: &str,
    detail: &str,
    summary: Option<&pcbcraft_engine::ChangeSummary>,
    answer: Option<bool>,
    view: &View,
) -> Option<Pressed> {
    let t = theme::tokens(ui.ctx());
    if let Some(allowed) = answer {
        // An answered question shrinks to a caption, a badge and one line.
        let (word, kind) = if allowed {
            (text::review_applied(), BadgeKind::Success)
        } else {
            (text::review_discarded(), BadgeKind::Neutral)
        };
        frame(&t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, &t, &text::review_caption(), Some((&word, kind)));
            ui.add(egui::Label::new(fonts::caption(title).color(t.muted)).wrap());
            if let Some(s) = summary.filter(|_| allowed) {
                ui.add(egui::Label::new(fonts::caption(review_line(s)).color(t.muted)).wrap());
            }
        });
        return None;
    }
    let mut pressed = None;
    frame(&t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        caption(ui, &t, &text::review_caption(), None);
        ui.add_space(4.0);
        ui.add(egui::Label::new(fonts::title(text::review_title())).wrap());
        ui.add(egui::Label::new(fonts::strong(title)).wrap());
        if summary.is_none() && !detail.is_empty() {
            // A call with no dry run (not a batch) has only its description.
            ui.add(egui::Label::new(fonts::caption(detail).color(t.muted)).wrap());
        }
        if let Some(s) = summary {
            ui.add_space(8.0);
            rows(ui, &t, s);
            ui.add_space(4.0);
            let mut on = view.list_all;
            if ui
                .checkbox(&mut on, text::review_list())
                .on_hover_text(text::review_list_tip())
                .clicked()
            {
                pressed = Some(Pressed::ToggleList);
            }
            if view.list_all {
                names(ui, &t, s);
            }
        }
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if button::primary_button(
                ui,
                Some(icons::OK),
                &text::allow(),
                &text::allow_tip(),
                Size::Normal,
                true,
            )
            .clicked()
            {
                pressed = Some(Pressed::Allow(id));
            }
            if summary.is_some()
                && button::secondary_button(
                    ui,
                    None,
                    &text::apply_each(),
                    &text::apply_each_tip(),
                    Size::Normal,
                    true,
                )
                .clicked()
            {
                pressed = Some(Pressed::ApplyEach(id));
            }
            if button::ghost_button(
                ui,
                None,
                &text::deny(),
                &text::deny_tip(),
                Size::Normal,
                true,
            )
            .clicked()
            {
                pressed = Some(Pressed::Deny(id));
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(fonts::caption(text::review_step_hint()).color(t.muted));
            if button::ghost_button(
                ui,
                None,
                &text::always_allow(),
                &text::always_allow_tip(),
                Size::Small,
                true,
            )
            .clicked()
            {
                pressed = Some(Pressed::AlwaysAllow(id));
            }
        });
    });
    pressed
}

/// "Parts +3, nets +3, tracks +8, objects untouched 5 of 5."
fn review_line(s: &pcbcraft_engine::ChangeSummary) -> String {
    let count = |k: &str| s.added_counts.get(k).copied().unwrap_or(0);
    text::review_line(
        count("component"),
        s.nets_added.len(),
        count("track"),
        s.untouched,
        s.total,
    )
}

/// The summary rows: a label on the left, the figure on the right, a thin rule between.
fn rows(ui: &mut egui::Ui, t: &Tokens, s: &pcbcraft_engine::ChangeSummary) {
    let count = |k: &str| s.added_counts.get(k).copied().unwrap_or(0);
    let first_names = |list: Vec<&str>| -> String {
        let shown: Vec<&str> = list.iter().take(3).copied().collect();
        let more = list.len().saturating_sub(shown.len());
        if more > 0 {
            format!("{}, {}", shown.join(", "), text::review_more(more))
        } else {
            shown.join(", ")
        }
    };
    let parts: Vec<&str> = s
        .added
        .iter()
        .filter(|(k, _)| k == "component")
        .map(|(_, n)| n.as_str())
        .collect();
    let nets: Vec<&str> = s.nets_added.iter().map(String::as_str).collect();
    let row = |ui: &mut egui::Ui, label: String, tip: String, value: String| {
        ui.horizontal(|ui| {
            ui.label(fonts::text(fonts::Role::Body, &label))
                .on_hover_text(tip);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(fonts::mono(value)).wrap());
            });
        });
        let y = ui.cursor().top();
        let w = ui.available_rect_before_wrap();
        ui.painter()
            .hline(w.x_range(), y, egui::Stroke::new(1.0, t.line));
    };
    row(
        ui,
        text::review_row_parts(),
        text::review_row_parts_tip(),
        format!("+{} {}", count("component"), first_names(parts)),
    );
    row(
        ui,
        text::review_row_nets(),
        text::review_row_nets_tip(),
        format!("+{} {}", s.nets_added.len(), first_names(nets)),
    );
    row(
        ui,
        text::review_row_tracks(),
        text::review_row_tracks_tip(),
        format!("+{} {:.1} mm", count("track"), s.track_mm_added),
    );
    row(
        ui,
        text::review_row_vias(),
        text::review_row_vias_tip(),
        count("via").to_string(),
    );
    row(
        ui,
        text::review_row_untouched(),
        text::review_row_untouched_tip(),
        text::review_of(s.untouched, s.total),
    );
    let delta = s.bytes_after.abs_diff(s.bytes_before);
    let base = s.bytes_after.max(s.bytes_before).max(1);
    row(
        ui,
        text::review_row_size(),
        text::review_row_size_tip(),
        text::review_kb(&steps::kb(delta), &steps::kb(base)),
    );
    let share = (delta * 100 / base).min(100);
    meter(ui, t, share, &text::review_share(share));
}

/// A thin bar with the share beside it.
fn meter(ui: &mut egui::Ui, t: &Tokens, share: usize, label: &str) {
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 80.0).max(40.0);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 6.0), egui::Sense::hover());
        response.on_hover_text(text::review_row_size_tip());
        ui.painter().rect_filled(rect, 3.0, t.raised);
        let mut fill = rect;
        fill.set_width(rect.width() * share as f32 / 100.0);
        ui.painter().rect_filled(fill, 3.0, t.cyan);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(fonts::mono(label).color(t.muted));
        });
    });
}

/// Every object the change touches, by kind and name.
fn names(ui: &mut egui::Ui, t: &Tokens, s: &pcbcraft_engine::ChangeSummary) {
    for (word, list) in [
        ("+", &s.added),
        ("~", &s.changed),
        (">", &s.moved),
        ("-", &s.removed),
    ] {
        for (kind, name) in list {
            ui.label(fonts::mono(format!("{word} {kind} {name}")).color(t.muted));
        }
    }
    if s.more > 0 {
        ui.label(fonts::caption(text::review_more(s.more)).color(t.muted));
    }
}

/// The card after a change was made: what happened and the one button that takes it back.
fn undo_card(ui: &mut egui::Ui, changes: u32, undone: bool, live: bool) -> Option<Pressed> {
    let t = theme::tokens(ui.ctx());
    let mut pressed = None;
    frame(&t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        caption(ui, &t, &text::undo_caption(), None);
        ui.add_space(4.0);
        let title = if changes == 1 {
            text::undo_title_one()
        } else {
            text::undo_title(changes)
        };
        ui.add(egui::Label::new(fonts::title(title)).wrap());
        let body = if undone {
            text::undo_done()
        } else {
            text::undo_body()
        };
        ui.add(egui::Label::new(fonts::caption(body).color(t.muted)).wrap());
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if button::secondary_button(
                ui,
                None,
                &text::undo(),
                &text::undo_tip(),
                Size::Normal,
                live && !undone,
            )
            .clicked()
            {
                pressed = Some(Pressed::Undo);
            }
            ui.label(fonts::caption(text::undo_key()).color(t.muted));
        });
    });
    pressed
}

/// Next-step cards, one may be marked Recommended.
pub(super) fn suggestions(
    ui: &mut egui::Ui,
    cards: &[Suggestion],
    enabled: bool,
) -> Option<Pressed> {
    let t = theme::tokens(ui.ctx());
    ui.label(fonts::section_caption(text::next_caption()).color(t.muted));
    ui.add_space(4.0);
    let mut pressed = None;
    for card in cards {
        let fill = if card.recommended {
            t.copper_tint
        } else {
            t.panel
        };
        let stroke = if card.recommended { t.copper } else { t.line };
        let out = egui::Frame::new()
            .fill(fill)
            .stroke(egui::Stroke::new(theme::BORDER, stroke))
            .corner_radius(egui::CornerRadius::same(RADIUS_CARD))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(fonts::strong(&card.title));
                    if card.recommended {
                        badge(
                            ui,
                            &text::next_recommended(),
                            BadgeKind::Accent,
                            &text::next_recommended_tip(),
                        );
                    }
                });
                if !card.detail.is_empty() {
                    ui.add(egui::Label::new(fonts::caption(&card.detail).color(t.muted)).wrap());
                }
            });
        let response = ui
            .interact(
                out.response.rect,
                ui.id().with(("assistant-next", &card.title)),
                egui::Sense::click(),
            )
            .on_hover_text(text::next_tip());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &card.title)
        });
        if enabled && response.clicked() {
            pressed = Some(Pressed::Pick(card.title.clone()));
        }
        ui.add_space(8.0);
    }
    pressed
}
