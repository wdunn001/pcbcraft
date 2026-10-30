//! Segmented control: two to four mutually exclusive options that change a
//! setting or a view (Top and Bottom, mm and mil, 45, 90 and Free).
//!
//! Context: design system 5.4. A raised track with a 1 px edge border holds the
//! items; the selected item is copper with dark text. It is not navigation; page
//! sections use `./tab_bar.rs`.
//! Local rules: arrow keys move the selection and the focus together, like a radio
//! group. Each item is its own focus stop and names itself to assistive tech.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/focus.rs
//!
//! # Do not
//! - Use for more than four options or for moving between pages.
use super::focus;
use crate::fonts::Role;
use crate::theme::{self, Tokens};
use egui::{
    Color32, CornerRadius, EventFilter, Id, Key, Modifiers, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2, WidgetInfo, WidgetType, pos2,
};
use std::hash::Hash;

/// Height of the track; items are 4 points shorter.
pub const TRACK_HEIGHT: f32 = 28.0;
const TRACK_PADDING: f32 = 2.0;
const ITEM_PADDING: f32 = 12.0;

/// One option of a segmented control.
#[derive(Clone, Copy, Debug)]
pub struct Segment<'a> {
    /// Visible label, one or two words.
    pub label: &'a str,
    /// Tooltip: what choosing it does.
    pub tip: &'a str,
}

/// The id of item `index` of the control built with `salt` inside `ui`.
///
/// Why: tests and callers that move focus need the same ids the widget uses.
pub fn item_id(_ui: &Ui, salt: impl Hash + std::fmt::Debug, index: usize) -> Id {
    Id::new(salt).with(index)
}

fn item_font(selected: bool) -> egui::FontId {
    if selected {
        Role::Strong.font()
    } else {
        Role::Button.font()
    }
}

/// The widths of the items, left to right, for the labels in `items`.
fn widths(ui: &Ui, items: &[Segment<'_>], selected: usize) -> Vec<f32> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let w = ui
                .painter()
                .layout_no_wrap(
                    item.label.to_owned(),
                    item_font(i == selected),
                    Color32::PLACEHOLDER,
                )
                .size()
                .x;
            w + 2.0 * ITEM_PADDING
        })
        .collect()
}

/// A segmented control over `items`; `selected` is the index in use.
///
/// Why: a compact choice that shows every option at once and never opens a menu.
///
/// # Rules
/// - Returns the track response; `changed()` is true on the frame the selection moves.
/// - Left and right arrows, Home and End move the selection while an item has focus.
/// - `selected` is clamped into range before drawing.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/focus.rs
///
/// # Do not
/// - Pass more than four items or an empty list.
pub fn segmented(
    ui: &mut Ui,
    salt: impl Hash + std::fmt::Debug,
    selected: &mut usize,
    items: &[Segment<'_>],
) -> Response {
    let t = theme::tokens(ui.ctx());
    let count = items.len();
    if count == 0 {
        return ui.allocate_response(Vec2::ZERO, Sense::hover());
    }
    *selected = (*selected).min(count - 1);
    // Reserve the widest the selected label can be so the track never jiggles.
    let w = widths(ui, items, usize::MAX);
    let total: f32 = w.iter().sum::<f32>() + 2.0 * TRACK_PADDING;
    let (track, mut outer) = ui.allocate_exact_size(Vec2::new(total, TRACK_HEIGHT), Sense::hover());
    let mut rects = Vec::with_capacity(count);
    let mut x = track.min.x + TRACK_PADDING;
    for width in &w {
        rects.push(Rect::from_min_size(
            pos2(x, track.min.y + TRACK_PADDING),
            Vec2::new(*width, TRACK_HEIGHT - 2.0 * TRACK_PADDING),
        ));
        x += width;
    }
    let mut responses = Vec::with_capacity(count);
    let mut moved_to = None;
    for (i, rect) in rects.iter().enumerate() {
        let id = item_id(ui, &salt, i);
        let r = ui.interact(*rect, id, Sense::click());
        r.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::RadioButton,
                true,
                i == *selected,
                items[i].label,
            )
        });
        if r.clicked() {
            moved_to = Some(i);
        }
        if r.has_focus() {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    EventFilter {
                        horizontal_arrows: true,
                        ..Default::default()
                    },
                );
            });
            let step = ui.input_mut(|input| {
                if input.consume_key(Modifiers::NONE, Key::ArrowLeft) {
                    Some(i.saturating_sub(1))
                } else if input.consume_key(Modifiers::NONE, Key::ArrowRight) {
                    Some((i + 1).min(count - 1))
                } else if input.consume_key(Modifiers::NONE, Key::Home) {
                    Some(0)
                } else if input.consume_key(Modifiers::NONE, Key::End) {
                    Some(count - 1)
                } else {
                    None
                }
            });
            if let Some(target) = step {
                moved_to = Some(target);
                ui.memory_mut(|m| m.request_focus(item_id(ui, &salt, target)));
            }
        }
        responses.push(r);
    }
    if let Some(i) = moved_to
        && i != *selected
    {
        *selected = i;
        outer.mark_changed();
    }
    paint(ui, track, &rects, &responses, items, *selected, &t);
    for (r, item) in responses.into_iter().zip(items) {
        r.on_hover_text(item.tip);
    }
    outer
}

fn paint(
    ui: &Ui,
    track: Rect,
    rects: &[Rect],
    responses: &[Response],
    items: &[Segment<'_>],
    selected: usize,
    t: &Tokens,
) {
    let painter = ui.painter();
    painter.rect_filled(track, CornerRadius::same(theme::RADIUS_CONTROL), t.raised);
    painter.rect_stroke(
        track,
        CornerRadius::same(theme::RADIUS_CONTROL),
        Stroke::new(theme::BORDER, t.edge),
        StrokeKind::Inside,
    );
    for (i, ((rect, r), item)) in rects.iter().zip(responses).zip(items).enumerate() {
        let is_selected = i == selected;
        let (fill, ink) = if is_selected {
            (t.copper, t.onacc)
        } else if r.hovered() {
            (t.hover, t.text)
        } else {
            (Color32::TRANSPARENT, t.muted)
        };
        painter.rect_filled(*rect, CornerRadius::same(theme::RADIUS_SEGMENT), fill);
        let galley = painter.layout_no_wrap(item.label.to_owned(), item_font(is_selected), ink);
        let pos = pos2(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        );
        painter.galley(pos, galley, ink);
        if r.has_focus() {
            focus::paint_ring(painter, *rect, theme::RADIUS_SEGMENT, t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame, key};

    const ITEMS: [Segment<'static>; 3] = [
        Segment {
            label: "45",
            tip: "Cut corners at 45 degrees",
        },
        Segment {
            label: "90",
            tip: "Square corners",
        },
        Segment {
            label: "Free",
            tip: "Any angle",
        },
    ];

    /// Arrow keys move the selection and the focus; Home and End jump.
    #[test]
    fn arrow_keys_move_the_selection() -> Result<(), String> {
        let ctx = context(true);
        let mut selected = 0usize;
        let first = std::cell::Cell::new(None);
        let draw = |ui: &mut Ui, selected: &mut usize| {
            first.set(Some(item_id(ui, "corners", 0)));
            segmented(ui, "corners", selected, &ITEMS);
        };
        frame(&ctx, vec![], |ui| draw(ui, &mut selected));
        let id = first.get().ok_or("the frame did not draw the control")?;
        ctx.memory_mut(|m| m.request_focus(id));
        // One frame to take focus and one for the arrow filter to apply.
        frame(&ctx, vec![], |ui| draw(ui, &mut selected));
        frame(&ctx, vec![], |ui| draw(ui, &mut selected));
        frame(&ctx, key(Key::ArrowRight), |ui| draw(ui, &mut selected));
        assert_eq!(selected, 1);
        frame(&ctx, vec![], |ui| draw(ui, &mut selected));
        frame(&ctx, key(Key::ArrowRight), |ui| draw(ui, &mut selected));
        assert_eq!(selected, 2);
        frame(&ctx, vec![], |ui| draw(ui, &mut selected));
        frame(&ctx, key(Key::ArrowRight), |ui| draw(ui, &mut selected));
        assert_eq!(selected, 2, "stops at the last item");
        frame(&ctx, vec![], |ui| draw(ui, &mut selected));
        frame(&ctx, key(Key::Home), |ui| draw(ui, &mut selected));
        assert_eq!(selected, 0);
        Ok(())
    }

    /// A click on an item selects it and reports a change.
    #[test]
    fn clicking_an_item_selects_it() -> Result<(), String> {
        let ctx = context(true);
        let mut selected = 0usize;
        let mut changed = false;
        let draw = |ui: &mut Ui, selected: &mut usize, changed: &mut bool| {
            let r = segmented(ui, "corners", selected, &ITEMS);
            *changed |= r.changed();
            r.rect
        };
        let mut track = egui::Rect::NOTHING;
        frame(&ctx, vec![], |ui| {
            track = draw(ui, &mut selected, &mut changed)
        });
        let target = pos2(track.max.x - 20.0, track.center().y);
        frame(&ctx, crate::ui_kit::testing::click(target), |ui| {
            draw(ui, &mut selected, &mut changed);
        });
        assert_eq!(selected, 2);
        assert!(changed);
        Ok(())
    }
}
