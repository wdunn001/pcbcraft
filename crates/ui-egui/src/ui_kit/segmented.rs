//! Segmented control: two to four mutually exclusive options that change a
//! setting or a view (Top and Bottom, mm and mil, 45, 90 and Free).
//!
//! Context: design system 5.4. A raised track with a 1 px edge border holds the
//! items; the selected item is copper with dark text. It is not navigation; page
//! sections use `./tab_bar.rs`.
//! Local rules: one tab stop for the whole group; arrow keys move the selection
//! while the track has focus, like a radio group.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/focus.rs
//! - crates/ui-egui/src/icons.rs
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
const ICON_ONLY_BREAK: f32 = 320.0;
const COUNT_NARROW_BREAK: f32 = 380.0;
const ICON_ONLY_WIDTH: f32 = 36.0;
const ICON_SIZE: f32 = 16.0;

/// One option of a segmented control.
#[derive(Clone, Copy, Debug)]
pub struct Segment<'a> {
    /// Visible label, one or two words.
    pub label: &'a str,
    /// Tooltip: what choosing it does.
    pub tip: &'a str,
    /// Optional icon from `assets/icons/ui/`; shown alone below [`ICON_ONLY_BREAK`].
    pub icon: Option<&'a str>,
    /// Live result count; may be hidden on narrow panels.
    pub count: Option<usize>,
    /// When set, the segment cannot be chosen; the string is the tooltip.
    pub disabled: Option<&'a str>,
}

impl<'a> Segment<'a> {
    /// Label and tip only; no icon, count, or disabled state.
    pub const fn text(label: &'a str, tip: &'a str) -> Self {
        Self {
            label,
            tip,
            icon: None,
            count: None,
            disabled: None,
        }
    }
}

/// Layout and accessibility options for [`segmented`].
#[derive(Clone, Copy, Debug, Default)]
pub struct SegmentedStyle<'a> {
    /// Stretch the track to the full width of the parent.
    pub full_width: bool,
    /// Screen-reader name for the radio group, for example "Show".
    pub group_name: Option<&'a str>,
}

/// The id of the whole control built with `salt` inside `ui`.
///
/// Why: tests and callers that move focus need the same id the widget uses.
pub fn group_id(_ui: &Ui, salt: impl Hash + std::fmt::Debug) -> Id {
    Id::new(salt)
}

/// The id of item `index` of the control built with `salt` inside `ui`.
///
/// Why: legacy gallery focus demo; prefer [`group_id`] for keyboard tests.
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

fn show_count(item: &Segment<'_>, selected: bool, panel_width: f32) -> Option<usize> {
    let count = item.count?;
    if panel_width <= COUNT_NARROW_BREAK {
        selected.then_some(count)
    } else {
        Some(count)
    }
}

fn display_label(item: &Segment<'_>, selected: bool, icons_only: bool, panel_width: f32) -> String {
    if icons_only {
        return String::new();
    }
    let mut text = item.label.to_owned();
    if let Some(n) = show_count(item, selected, panel_width) {
        text.push(' ');
        text.push('(');
        text.push_str(&n.to_string());
        text.push(')');
    }
    text
}

fn segment_tip(item: &Segment<'_>) -> &str {
    item.disabled.unwrap_or(item.tip)
}

fn enabled_indices(items: &[Segment<'_>]) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.disabled.is_none().then_some(i))
        .collect()
}

fn next_enabled(current: usize, items: &[Segment<'_>], step: i32) -> usize {
    let enabled = enabled_indices(items);
    if enabled.is_empty() {
        return current.min(items.len().saturating_sub(1));
    }
    let pos = enabled
        .iter()
        .position(|&i| i == current)
        .unwrap_or(0);
    let next = if step >= 0 {
        enabled[(pos + 1).min(enabled.len() - 1)]
    } else {
        enabled[pos.saturating_sub(1)]
    };
    next
}

/// Item widths for the labels (or icon-only slots) in `items`.
fn widths(
    ui: &Ui,
    items: &[Segment<'_>],
    selected: usize,
    icons_only: bool,
    panel_width: f32,
    total: f32,
) -> Vec<f32> {
    if icons_only {
        return vec![ICON_ONLY_WIDTH; items.len()];
    }
    let mut w: Vec<f32> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let label = display_label(item, i == selected, false, panel_width);
            let text_w = ui
                .painter()
                .layout_no_wrap(label, item_font(i == selected), Color32::PLACEHOLDER)
                .size()
                .x;
            let icon_w = item.icon.map(|_| ICON_SIZE + 4.0).unwrap_or(0.0);
            text_w + icon_w + 2.0 * ITEM_PADDING
        })
        .collect();
    if total > w.iter().sum::<f32>() + 2.0 * TRACK_PADDING {
        let extra = total - w.iter().sum::<f32>() - 2.0 * TRACK_PADDING;
        let share = extra / items.len() as f32;
        for width in &mut w {
            *width += share;
        }
    }
    w
}

/// A segmented control over `items`; `selected` is the index in use.
///
/// Why: a compact choice that shows every option at once and never opens a menu.
///
/// # Rules
/// - Returns the track response; `changed()` is true on the frame the selection moves.
/// - Left and right arrows, Home and End move the selection while the track has focus.
/// - Space or Enter selects the focused choice when it differs from `selected`.
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
    segmented_styled(ui, salt, selected, items, SegmentedStyle::default())
}

/// Like [`segmented`], with layout and group naming options.
pub fn segmented_styled(
    ui: &mut Ui,
    salt: impl Hash + std::fmt::Debug,
    selected: &mut usize,
    items: &[Segment<'_>],
    style: SegmentedStyle<'_>,
) -> Response {
    let t = theme::tokens(ui.ctx());
    let count = items.len();
    if count == 0 {
        return ui.allocate_response(Vec2::ZERO, Sense::hover());
    }
    *selected = (*selected).min(count - 1);
    if items[*selected].disabled.is_some() {
        if let Some(&first) = enabled_indices(items).first() {
            *selected = first;
        }
    }
    let panel_width = ui.max_rect().width();
    let icons_only = panel_width < ICON_ONLY_BREAK
        && items.iter().all(|s| s.icon.is_some());
    let total = if style.full_width {
        ui.available_width()
    } else {
        let w = widths(ui, items, usize::MAX, icons_only, panel_width, 0.0);
        w.iter().sum::<f32>() + 2.0 * TRACK_PADDING
    };
    let id = group_id(ui, &salt);
    let (track, _track_slot) = ui.allocate_exact_size(
        Vec2::new(total.max(0.0), TRACK_HEIGHT),
        Sense::hover(),
    );
    let w = widths(ui, items, *selected, icons_only, panel_width, track.width());
    let mut rects = Vec::with_capacity(count);
    let mut x = track.min.x + TRACK_PADDING;
    for width in &w {
        rects.push(Rect::from_min_size(
            pos2(x, track.min.y + TRACK_PADDING),
            Vec2::new(*width, TRACK_HEIGHT - 2.0 * TRACK_PADDING),
        ));
        x += width;
    }
    let mut outer = ui.interact(track, id, Sense::click().union(Sense::focus()));
    let group_label = style
        .group_name
        .map(str::to_owned)
        .unwrap_or_else(|| items[*selected].label.to_owned());
    outer.widget_info(|| {
        WidgetInfo::labeled(
            WidgetType::RadioButton,
            true,
            format!("{group_label}, {} of {}", *selected + 1, count),
        )
    });
    if outer.has_focus() {
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
                Some(-1)
            } else if input.consume_key(Modifiers::NONE, Key::ArrowRight) {
                Some(1)
            } else if input.consume_key(Modifiers::NONE, Key::Home) {
                Some(-2)
            } else if input.consume_key(Modifiers::NONE, Key::End) {
                Some(2)
            } else if input.consume_key(Modifiers::NONE, Key::Space)
                || input.consume_key(Modifiers::NONE, Key::Enter)
            {
                Some(0)
            } else {
                None
            }
        });
        if let Some(step) = step {
            let target = match step {
                -2 => enabled_indices(items).first().copied(),
                2 => enabled_indices(items).last().copied(),
                -1 => Some(next_enabled(*selected, items, -1)),
                1 => Some(next_enabled(*selected, items, 1)),
                0 => Some(*selected),
                _ => None,
            };
            if let Some(i) = target
                && items[i].disabled.is_none()
                && i != *selected
            {
                *selected = i;
                outer.mark_changed();
            }
        }
    }
    if outer.clicked() {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            for (i, rect) in rects.iter().enumerate() {
                if rect.contains(pos)
                    && items[i].disabled.is_none()
                    && i != *selected
                {
                    *selected = i;
                    outer.mark_changed();
                    break;
                }
            }
        }
    }
    let pointer = ui.input(|i| i.pointer.hover_pos());
    paint(
        ui,
        track,
        &rects,
        items,
        *selected,
        pointer,
        icons_only,
        panel_width,
        &t,
    );
    if outer.has_focus() {
        focus::paint_ring(ui.painter(), track, theme::RADIUS_CONTROL, &t);
    }
    for (i, (rect, item)) in rects.iter().zip(items).enumerate() {
        let tip = segment_tip(item);
        let hover = ui.interact(*rect, id.with(i), Sense::hover());
        if hover.hovered() {
            hover.on_hover_text(tip);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    outer
}

fn paint(
    ui: &mut Ui,
    track: Rect,
    rects: &[Rect],
    items: &[Segment<'_>],
    selected: usize,
    pointer: Option<egui::Pos2>,
    icons_only: bool,
    panel_width: f32,
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
    for (i, (rect, item)) in rects.iter().zip(items).enumerate() {
        let disabled = item.disabled.is_some();
        let is_selected = i == selected;
        let hovered = !disabled && pointer.is_some_and(|p| rect.contains(p));
        let (fill, ink) = if disabled {
            (Color32::TRANSPARENT, t.muted.gamma_multiply(0.5))
        } else if is_selected {
            (t.copper, t.onacc)
        } else if hovered {
            (t.hover, t.text)
        } else {
            (Color32::TRANSPARENT, t.muted)
        };
        painter.rect_filled(*rect, CornerRadius::same(theme::RADIUS_SEGMENT), fill);
        let icon_color = if is_selected && !disabled {
            t.onacc
        } else if disabled {
            t.muted.gamma_multiply(0.5)
        } else {
            ink
        };
        let label = display_label(item, is_selected, icons_only, panel_width);
        let mut x = rect.center().x;
        let mut parts_w = 0.0f32;
        if let Some(name) = item.icon {
            parts_w += ICON_SIZE;
            if !label.is_empty() {
                parts_w += 4.0;
            }
        }
        if !label.is_empty() {
            parts_w += painter
                .layout_no_wrap(label.clone(), item_font(is_selected), ink)
                .size()
                .x;
        }
        x -= parts_w / 2.0;
        if let Some(name) = item.icon {
            let icon_rect = Rect::from_min_size(
                pos2(x, rect.center().y - ICON_SIZE / 2.0),
                Vec2::splat(ICON_SIZE),
            );
            crate::icons::icon_at(ui, icon_rect, name, icon_color);
            x += ICON_SIZE + if label.is_empty() { 0.0 } else { 4.0 };
        }
        if !label.is_empty() {
            let galley = painter.layout_no_wrap(label, item_font(is_selected), ink);
            let pos = pos2(x, rect.center().y - galley.size().y / 2.0);
            painter.galley(pos, galley, ink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame, key};

    const ITEMS: [Segment<'static>; 3] = [
        Segment::text("45", "Cut corners at 45 degrees"),
        Segment::text("90", "Square corners"),
        Segment::text("Free", "Any angle"),
    ];

    /// Arrow keys move the selection while the track has focus; Home and End jump.
    #[test]
    fn arrow_keys_move_the_selection() -> Result<(), String> {
        let ctx = context(true);
        let mut selected = 0usize;
        let first = std::cell::Cell::new(None);
        let draw = |ui: &mut Ui, selected: &mut usize| {
            first.set(Some(group_id(ui, "corners")));
            segmented(ui, "corners", selected, &ITEMS);
        };
        frame(&ctx, vec![], |ui| draw(ui, &mut selected));
        let id = first.get().ok_or("the frame did not draw the control")?;
        ctx.memory_mut(|m| m.request_focus(id));
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
        frame(
            &ctx,
            crate::ui_kit::testing::click(target),
            |ui| draw(ui, &mut selected, &mut changed),
        );
        assert_eq!(selected, 2);
        assert!(changed);
        Ok(())
    }
}
