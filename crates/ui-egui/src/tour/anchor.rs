//! Anchor registry: where a tour step points on screen this frame.
//!
//! Controls call [`register`] once per frame when a tour might run. The map
//! costs one insert per registered id; when no tour is active nothing registers.

use egui::{Context, Rect, Ui};

fn key(id: &str) -> egui::Id {
    egui::Id::new(("tour-anchor", id))
}

/// Remember the clickable rectangle for a control id until the next frame.
pub(crate) fn register(ui: &Ui, id: &str, rect: Rect) {
    if id.is_empty() || id == "none" {
        return;
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(key(id), rect.expand(0.0)));
}

/// The last rectangle published for this anchor, if any.
pub(crate) fn lookup(ctx: &Context, id: &str) -> Option<Rect> {
    if id.is_empty() || id == "none" {
        return None;
    }
    ctx.data(|d| d.get_temp::<Rect>(key(id)))
}
