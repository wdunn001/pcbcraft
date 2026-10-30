//! Anchor registry: where a tour step points on screen this frame.
//!
//! Controls call [`anchor`] once per frame when they are painted. The map holds
//! one entry per id until the next frame; when no tour is running the cost is
//! only those inserts.

use egui::{Context, Rect, Response, Ui};

#[derive(Clone, Copy, Debug, PartialEq)]
struct Record {
    rect: Rect,
    clip: Rect,
}

fn key(id: &str) -> egui::Id {
    egui::Id::new(("tour-anchor", id))
}

/// Remember the clickable rectangle for a control id until the next frame.
pub(crate) fn anchor(ui: &Ui, id: &str, response: &Response) {
    if id.is_empty() || id == "none" {
        return;
    }
    let rect = response.rect;
    let clip = rect.intersect(response.clip_rect);
    if clip.width() <= 0.0 || clip.height() <= 0.0 {
        return;
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(key(id), Record { rect, clip }));
}

/// Remember a rectangle when the caller has no [`Response`].
pub(crate) fn anchor_rect(ui: &Ui, id: &str, rect: Rect) {
    if id.is_empty() || id == "none" || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let clip = rect.intersect(ui.clip_rect());
    if clip.width() <= 0.0 || clip.height() <= 0.0 {
        return;
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(key(id), Record { rect, clip }));
}

/// The visible rectangle published for this anchor, if any.
pub(crate) fn lookup(ctx: &Context, id: &str) -> Option<Rect> {
    if id.is_empty() || id == "none" {
        return None;
    }
    ctx.data(|d| {
        d.get_temp::<Record>(key(id))
            .map(|r| r.clip)
            .filter(|r| r.width() > 0.0 && r.height() > 0.0)
    })
}

/// Tool bar anchor id: `{workspace}.tool.{name}`.
pub(crate) fn tool_id(workspace: &str, item_id: &str) -> String {
    let name = item_id
        .strip_prefix("tool-")
        .unwrap_or(item_id)
        .replace('-', "_");
    format!("{workspace}.tool.{name}")
}

/// Region tab anchor id: `region.{workspace}.{tab}`.
pub(crate) fn region_tab_id(workspace: &str, tab: &str) -> String {
    format!("region.{workspace}.{tab}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame};

    #[test]
    fn anchor_lookup_returns_the_clipped_rect() {
        let ctx = context(true);
        frame(&ctx, vec![], |ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(40.0, 20.0), egui::Sense::hover());
            assert_eq!(rect, response.rect);
            anchor(ui, "shell.tab.files", &response);
        });
        let got = lookup(&ctx, "shell.tab.files").expect("anchor was stored");
        assert!(got.width() > 0.0 && got.height() > 0.0);
        assert!(lookup(&ctx, "shell.tab.missing").is_none());
    }
}
