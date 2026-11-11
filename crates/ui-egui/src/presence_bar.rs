//! Presence bar: shows who is currently in the collaborative room.

use egui::{RichText, Ui};

#[derive(Clone, Debug)]
pub struct PresenceInfo {
    pub name: String,
    pub is_you: bool,
}

/// Render the presence bar showing all participants in the room.
pub fn presence_bar(ui: &mut Ui, participants: &[PresenceInfo]) -> bool {
    let mut hovered = false;

    ui.horizontal(|ui| {
        ui.label(RichText::new("Room:").strong());
        for (i, p) in participants.iter().enumerate() {
            if i > 0 {
                ui.label("·");
            }
            let name = if p.is_you {
                format!("{} (you)", p.name)
            } else {
                p.name.clone()
            };
            hovered = ui.label(name).hovered() || hovered;
        }
    });

    hovered
}
