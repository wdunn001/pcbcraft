//! Cards in the conversation: tool rows, approval questions and the undo step.
//!
//! Context: drawn by `pane.rs`; each function draws one transcript item and returns what the
//! user pressed so the pane (which owns the app) acts on it.
//! Rules: every button has a tooltip; a card is reachable with Tab, Enter allows, Escape denies.

use super::text;
use pcbcraft_assistant::{Item, ToolStatus};

/// What the user pressed on a card.
pub(super) enum Pressed {
    Allow(u64),
    AlwaysAllow(u64),
    Deny(u64),
    Undo,
}

/// Draw one transcript item.
pub(super) fn item(
    ui: &mut egui::Ui,
    item: &Item,
    muted: egui::Color32,
    undo_live: bool,
) -> Option<Pressed> {
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
            answer,
        } => approval(ui, *id, title, detail, *answer),
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
        Item::UndoStep { changes, undone } => {
            let mut pressed = None;
            ui.horizontal(|ui| {
                ui.label(format!("{changes} changes made"));
                if ui
                    .add_enabled(undo_live && !undone, egui::Button::new(text::undo()))
                    .on_hover_text(text::undo_tip())
                    .clicked()
                {
                    pressed = Some(Pressed::Undo);
                }
            });
            pressed
        }
        Item::Sources(links) => {
            for (i, link) in links.iter().enumerate() {
                ui.label(format!("Source {}: {}", i + 1, link.title));
            }
            None
        }
    }
}

fn approval(
    ui: &mut egui::Ui,
    id: u64,
    title: &str,
    detail: &str,
    answer: Option<bool>,
) -> Option<Pressed> {
    if let Some(allowed) = answer {
        // An answered question shrinks to one line.
        let word = if allowed { "Allowed" } else { "Denied" };
        ui.add(egui::Label::new(format!("{word}: {title}")).wrap());
        return None;
    }
    let mut pressed = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add(egui::Label::new(egui::RichText::new(title).strong()).wrap());
        if !detail.is_empty() {
            ui.add(egui::Label::new(detail).wrap());
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(text::allow())
                .on_hover_text(text::allow_tip())
                .clicked()
            {
                pressed = Some(Pressed::Allow(id));
            }
            if ui
                .button(text::always_allow())
                .on_hover_text(text::always_allow_tip())
                .clicked()
            {
                pressed = Some(Pressed::AlwaysAllow(id));
            }
            if ui
                .button(text::deny())
                .on_hover_text(text::deny_tip())
                .clicked()
            {
                pressed = Some(Pressed::Deny(id));
            }
        });
    });
    pressed
}
