//! Tab bar in three sizes: tab nav, aside and page tabs, and sheet tabs.
//!
//! Context: design system 5.5. The tab itself is `craft_ui::tabs::Tab` (the shared
//! component that gives each tab the Tab role and a selected state); this file
//! adds the design's sizes, colours, copper bar, optional icon and count badge, and
//! arrow-key movement between tabs.
//! Local rules: irrelevant tabs are not passed in, never shown disabled. Active
//! text is 600 and carries a 2 px copper bar inset from the bottom corners.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/chip.rs
//! - crates/ui-egui/src/ui_kit/icons.rs
//!
//! # Do not
//! - Use `SelectableLabel` for tabs, or a tab bar for a setting (use `segmented`).
use super::chip::{self, BadgeKind};
use super::icons::{ICON_SIZE, Icon};
use crate::fonts::Role;
use crate::theme::{self, BAR_HEIGHT, SHEET_TAB_HEIGHT};
use craft_ui::tabs::Tab;
use egui::{
    Color32, CornerRadius, EventFilter, Id, Key, Modifiers, Rect, Response, Sense, Stroke, Ui,
    Vec2, pos2,
};
use std::hash::Hash;

/// Which of the three tab levels a bar is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabSize {
    /// The work-area tab nav: 40 high, 16 side padding.
    Nav,
    /// Aside tabs and page tabs: 40 high, 14 side padding.
    Aside,
    /// Bottom-sheet tabs: 36 high, 12 side padding.
    Sheet,
}

impl TabSize {
    /// Bar height in points.
    pub fn height(self) -> f32 {
        match self {
            TabSize::Nav | TabSize::Aside => BAR_HEIGHT,
            TabSize::Sheet => SHEET_TAB_HEIGHT,
        }
    }

    /// Side padding of one tab in points.
    pub fn padding(self) -> f32 {
        match self {
            TabSize::Nav => 16.0,
            TabSize::Aside => 14.0,
            TabSize::Sheet => 12.0,
        }
    }
}

/// One tab of a bar.
#[derive(Clone, Debug)]
pub struct TabItem<'a> {
    /// Visible label.
    pub label: &'a str,
    /// Tooltip: what the tab shows.
    pub tip: &'a str,
    /// Optional leading icon.
    pub icon: Option<Icon>,
    /// Optional count or status badge after the label.
    pub badge: Option<(String, BadgeKind)>,
}

impl<'a> TabItem<'a> {
    /// A plain tab with a label and a tooltip.
    pub fn new(label: &'a str, tip: &'a str) -> Self {
        Self {
            label,
            tip,
            icon: None,
            badge: None,
        }
    }
}

/// The id of tab `index` of the bar built with `salt` inside `ui`.
pub fn tab_id(_ui: &Ui, salt: impl Hash + std::fmt::Debug, index: usize) -> Id {
    Id::new(salt).with(index)
}

fn label_font(active: bool) -> egui::FontId {
    if active {
        Role::Strong.font()
    } else {
        Role::Button.font()
    }
}

fn tab_width(ui: &Ui, item: &TabItem<'_>, size: TabSize) -> f32 {
    // Measure at the active weight so selecting a tab never shifts its neighbours.
    let text = ui
        .painter()
        .layout_no_wrap(
            item.label.to_owned(),
            label_font(true),
            Color32::PLACEHOLDER,
        )
        .size()
        .x;
    let icon = if item.icon.is_some() {
        ICON_SIZE + 6.0
    } else {
        0.0
    };
    let badge = item
        .badge
        .as_ref()
        .map_or(0.0, |(text, _)| 6.0 + chip::badge_size(ui, text).x);
    text + icon + badge + 2.0 * size.padding()
}

/// A bar of tabs; `current` is the index of the active one.
///
/// Why: every region with sub-views (work areas, asides, the bottom sheet) uses
/// the same bar, so tabs look and behave the same everywhere.
///
/// # Rules
/// - Returns the bar response; `changed()` is true on the frame the active tab moves.
/// - Left and right arrows, Home and End move the active tab while a tab has focus.
/// - A `current` past the last tab means none is active (the page is not one of the tabs), so
///   a press on any tab, the last included, still counts as a change.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/chip.rs (badge painting)
///
/// # Do not
/// - Pass a tab that does not apply to the active page; hide it instead.
pub fn tab_bar(
    ui: &mut Ui,
    salt: impl Hash + std::fmt::Debug,
    current: &mut usize,
    tabs: &[TabItem<'_>],
    size: TabSize,
    tour_anchors: Option<&[&str]>,
) -> Response {
    let t = theme::tokens(ui.ctx());
    let count = tabs.len();
    let (bar, mut outer) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), size.height()),
        Sense::hover(),
    );
    if count == 0 {
        return outer;
    }
    let mut x = bar.min.x;
    let mut moved_to = None;
    let mut rects = Vec::with_capacity(count);
    for item in tabs {
        let w = tab_width(ui, item, size);
        rects.push(Rect::from_min_size(
            pos2(x, bar.min.y),
            Vec2::new(w, size.height()),
        ));
        x += w;
    }
    for (i, (item, rect)) in tabs.iter().zip(&rects).enumerate() {
        let id = tab_id(ui, &salt, i);
        let active = i == *current;
        let response = Tab::new(id, item.label, active)
            .focus_stroke(Stroke::new(theme::FOCUS_STROKE, t.text))
            .show_at(ui, *rect, |ui, r| {
                let hovered = r.hovered();
                if hovered && !active {
                    ui.painter().rect_filled(
                        *rect,
                        CornerRadius::same(theme::RADIUS_CONTROL),
                        t.raised,
                    );
                }
                let ink = if active || hovered { t.text } else { t.muted };
                let font = label_font(active);
                let galley = ui
                    .painter()
                    .layout_no_wrap(item.label.to_owned(), font, ink);
                let mut cx = rect.min.x + size.padding();
                let cy = rect.center().y;
                if let Some(icon) = item.icon {
                    let r =
                        Rect::from_min_size(pos2(cx, cy - ICON_SIZE / 2.0), Vec2::splat(ICON_SIZE));
                    (icon.0)(ui.painter(), r, if active { t.copper } else { ink });
                    cx += ICON_SIZE + 6.0;
                }
                ui.painter()
                    .galley(pos2(cx, cy - galley.size().y / 2.0), galley.clone(), ink);
                cx += galley.size().x;
                if let Some((text, kind)) = &item.badge {
                    let bs = chip::badge_size(ui, text);
                    let br = Rect::from_min_size(pos2(cx + 6.0, cy - bs.y / 2.0), bs);
                    chip::paint_badge(ui, br, text, *kind, &t);
                }
                if active {
                    let bar_rect = Rect::from_min_max(
                        pos2(rect.min.x + 8.0, rect.max.y - theme::FOCUS_STROKE),
                        pos2(rect.max.x - 8.0, rect.max.y),
                    );
                    ui.painter()
                        .rect_filled(bar_rect, CornerRadius::same(1), t.copper);
                }
            });
        if response.clicked() {
            moved_to = Some(i);
        }
        if response.has_focus() {
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
                ui.memory_mut(|m| m.request_focus(tab_id(ui, &salt, target)));
            }
        }
        response.on_hover_text(item.tip);
        if let Some(ids) = tour_anchors
            && let Some(id) = ids.get(i)
        {
            crate::tour::anchor::anchor(ui, id, &response);
        }
    }
    if let Some(i) = moved_to
        && i != *current
    {
        *current = i;
        outer.mark_changed();
    }
    outer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame, key};

    fn items() -> Vec<TabItem<'static>> {
        vec![
            TabItem::new("Layers", "Layers of the board"),
            TabItem::new("Library", "Parts library"),
            TabItem::new("Nets", "Nets of the board"),
        ]
    }

    /// Heights are 40, 40 and 36 for nav, aside and sheet.
    #[test]
    fn heights_follow_the_three_sizes() {
        let ctx = context(true);
        for (size, h) in [
            (TabSize::Nav, 40.0),
            (TabSize::Aside, 40.0),
            (TabSize::Sheet, 36.0),
        ] {
            let mut cur = 0usize;
            let mut got = 0.0;
            frame(&ctx, vec![], |ui| {
                got = tab_bar(ui, "t", &mut cur, &items(), size, None).rect.height();
            });
            assert_eq!(got, h, "{size:?}");
        }
    }

    /// Arrow keys move the active tab; Home and End jump; the ends stop.
    #[test]
    fn arrow_keys_move_between_tabs() -> Result<(), String> {
        let ctx = context(true);
        let mut cur = 0usize;
        let first = std::cell::Cell::new(None);
        let draw = |ui: &mut Ui, cur: &mut usize| {
            first.set(Some(tab_id(ui, "t", 0)));
            tab_bar(ui, "t", cur, &items(), TabSize::Aside, None);
        };
        frame(&ctx, vec![], |ui| draw(ui, &mut cur));
        let id = first.get().ok_or("the frame did not draw the bar")?;
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&ctx, vec![], |ui| draw(ui, &mut cur));
        frame(&ctx, vec![], |ui| draw(ui, &mut cur));
        frame(&ctx, key(Key::ArrowRight), |ui| draw(ui, &mut cur));
        assert_eq!(cur, 1);
        frame(&ctx, vec![], |ui| draw(ui, &mut cur));
        frame(&ctx, key(Key::End), |ui| draw(ui, &mut cur));
        assert_eq!(cur, 2);
        frame(&ctx, vec![], |ui| draw(ui, &mut cur));
        frame(&ctx, key(Key::ArrowRight), |ui| draw(ui, &mut cur));
        assert_eq!(cur, 2);
        frame(&ctx, vec![], |ui| draw(ui, &mut cur));
        frame(&ctx, key(Key::ArrowLeft), |ui| draw(ui, &mut cur));
        assert_eq!(cur, 1);
        Ok(())
    }

    /// Each tab announces the Tab role with its label, and only the active one is selected.
    #[test]
    fn tabs_have_the_tab_role_and_a_name() {
        let ctx = context(true);
        let mut cur = 1usize;
        let out = frame(&ctx, vec![], |ui| {
            tab_bar(ui, "t", &mut cur, &items(), TabSize::Aside, None);
        });
        let Some(update) = out.platform_output.accesskit_update else {
            assert!(cur == 1, "accesskit was off");
            return;
        };
        let tabs: Vec<_> = update
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == egui::accesskit::Role::Tab)
            .map(|(_, n)| (n.label().unwrap_or_default().to_owned(), n.is_selected()))
            .collect();
        assert_eq!(tabs.len(), 3);
        assert!(tabs.iter().all(|(name, _)| !name.is_empty()));
        assert_eq!(tabs.iter().filter(|(_, sel)| *sel == Some(true)).count(), 1);
    }
}
