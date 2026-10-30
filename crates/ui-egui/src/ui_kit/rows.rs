//! List row, tree row and finding row: the repeating rows of lists, trees and checks.
//!
//! Context: design system 5.7 and 5.8. Rows paint their own background (hover on
//! `raised`, selected copper tint with a 3 px copper bar), a 55 percent `line`
//! divider, and lay content out through small closures or fixed cells.
//! Local rules: state is shown by shape and text as well as colour (eye icon plus a
//! dimmed row, warning triangle plus words); row heights are 36, 28 and 48.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/button.rs
//! - crates/ui-egui/src/ui_kit/chip.rs
//! - crates/ui-egui/src/ui_kit/icons.rs
//!
//! # Do not
//! - Draw a row's selected state anywhere but here.
use super::button::{self, Size};
use super::chip;
use super::icons::{self, ICON_SIZE, Icon};
use crate::fonts::{self, Role};
use crate::theme::{self, Tokens};
use egui::{
    Color32, CornerRadius, Id, Layout, Rect, Response, Sense, Stroke, Ui, Vec2, WidgetInfo,
    WidgetType, pos2,
};

/// Height of a list row.
pub const LIST_ROW_HEIGHT: f32 = 36.0;
/// Height of a tree row.
pub const TREE_ROW_HEIGHT: f32 = 28.0;
/// Height of a finding row.
pub const FINDING_ROW_HEIGHT: f32 = 48.0;
/// Height of a compact finding row.
pub const FINDING_COMPACT_HEIGHT: f32 = 36.0;
/// Indent per tree level.
pub const TREE_INDENT: f32 = 16.0;
/// Width of the lock cell and the eye cell.
pub const CELL: f32 = 24.0;

fn paint_row_background(ui: &Ui, rect: Rect, selected: bool, hovered: bool, t: &Tokens) {
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 0.0, t.copper_tint);
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
            0.0,
            t.copper,
        );
    } else if hovered {
        painter.rect_filled(rect, 0.0, t.raised);
    }
}

fn divider(ui: &Ui, rect: Rect, t: &Tokens) {
    ui.painter().hline(
        rect.x_range(),
        rect.max.y - 0.5,
        Stroke::new(theme::BORDER, t.line.gamma_multiply(0.55)),
    );
}

/// A 36 high list row with free content.
///
/// Why: lists of parts, files and outputs share hover, selection and divider.
///
/// # Rules
/// - `name` is the accessible name; `selected` draws the copper tint and bar.
/// - Content runs left to right, vertically centred, inset 12 on both sides.
///
/// # Do not
/// - Put a primary button in a row.
pub fn list_row(
    ui: &mut Ui,
    selected: bool,
    name: &str,
    tip: &str,
    content: impl FnOnce(&mut Ui),
) -> Response {
    let t = theme::tokens(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), LIST_ROW_HEIGHT),
        Sense::click(),
    );
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, name));
    paint_row_background(ui, rect, selected, response.hovered(), &t);
    divider(ui, rect, &t);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(12.0, 0.0)))
            .layout(Layout::left_to_right(egui::Align::Center)),
    );
    content(&mut child);
    super::focus::focus_ring(ui, &response, 0);
    response.on_hover_text(tip)
}

/// What a tree row shows.
#[derive(Clone, Debug)]
pub struct TreeRow<'a> {
    /// Indent level, 0 for a top row.
    pub depth: usize,
    /// `Some(open)` for a row with children, `None` for a leaf.
    pub expanded: Option<bool>,
    /// Colour swatch before the name (layer and net colours).
    pub swatch: Option<Color32>,
    /// Kind icon before the name, used when there is no swatch.
    pub icon: Option<Icon>,
    /// The name.
    pub name: &'a str,
    /// Count at the right, drawn in Mono.
    pub count: Option<String>,
    /// `Some(locked)` shows the lock cell.
    pub locked: Option<bool>,
    /// `Some(visible)` shows the eye cell.
    pub visible: Option<bool>,
    /// The row is the current selection.
    pub selected: bool,
    /// Draw the name and swatch at 45 percent (hidden or empty layer).
    pub dimmed: bool,
}

/// What happened in a tree row this frame.
#[derive(Clone, Debug)]
pub struct TreeRowResponse {
    /// The row itself; `clicked()` selects it.
    pub row: Response,
    /// The chevron was pressed.
    pub toggled: bool,
    /// The lock cell was pressed.
    pub lock_pressed: bool,
    /// The eye cell was pressed.
    pub eye_pressed: bool,
}

/// A 28 high tree row: indent, chevron, swatch or icon, name, count, lock cell, eye cell.
///
/// Why: the Layers tree and the nets list are the centre of the Layout page.
///
/// # Rules
/// - Indent is 16 per level; the chevron cell is 16 wide.
/// - Lock and eye cells are 24 wide and each has its own accessible name and tooltip.
/// - Hidden rows show the crossed eye; dimmed rows draw the name at 45 percent.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/icons.rs
///
/// # Do not
/// - Mutate state; the caller acts on the returned flags.
pub fn tree_row(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    row: &TreeRow<'_>,
) -> TreeRowResponse {
    let t = theme::tokens(ui.ctx());
    let base = Id::new(id_salt);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), TREE_ROW_HEIGHT),
        Sense::click(),
    );
    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::SelectableLabel, true, row.selected, row.name)
    });
    paint_row_background(ui, rect, row.selected, response.hovered(), &t);
    let fade = if row.dimmed { 0.45 } else { 1.0 };
    let ink = Tokens::faded(t.text, fade);
    let cy = rect.center().y;
    let mut x = rect.min.x + 8.0 + TREE_INDENT * row.depth as f32;
    let mut toggled = false;
    if let Some(open) = row.expanded {
        let cell = Rect::from_min_size(pos2(x, cy - 8.0), Vec2::splat(ICON_SIZE));
        let r = ui.interact(cell, base.with("toggle"), Sense::click());
        r.widget_info(|| {
            WidgetInfo::labeled(
                WidgetType::Button,
                true,
                format!("{} {}", if open { "Collapse" } else { "Expand" }, row.name),
            )
        });
        let icon = if open {
            icons::CHEVRON_DOWN
        } else {
            icons::CHEVRON_RIGHT
        };
        (icon.0)(
            ui.painter(),
            cell,
            if r.hovered() { t.text } else { t.muted },
        );
        toggled = r.clicked();
        r.on_hover_text(if open { "Collapse" } else { "Expand" });
    }
    x += ICON_SIZE + 4.0;
    if let Some(swatch) = row.swatch {
        let s = Rect::from_min_size(pos2(x, cy - 6.0), Vec2::splat(12.0));
        ui.painter()
            .rect_filled(s, CornerRadius::same(3), Tokens::faded(swatch, fade));
        x += 12.0 + 8.0;
    } else if let Some(icon) = row.icon {
        let r = Rect::from_min_size(pos2(x, cy - 8.0), Vec2::splat(ICON_SIZE));
        (icon.0)(ui.painter(), r, Tokens::faded(t.muted, fade));
        x += ICON_SIZE + 8.0;
    }
    let mut right = rect.max.x - 4.0;
    let mut eye_pressed = false;
    if let Some(visible) = row.visible {
        let cell = Rect::from_min_size(
            pos2(right - CELL, rect.min.y),
            Vec2::new(CELL, TREE_ROW_HEIGHT),
        );
        let r = ui.interact(cell, base.with("eye"), Sense::click());
        let label = format!("{} {}", if visible { "Hide" } else { "Show" }, row.name);
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label.clone()));
        let icon = if visible { icons::EYE } else { icons::EYE_OFF };
        let c = if r.hovered() { t.text } else { t.muted };
        (icon.0)(
            ui.painter(),
            Rect::from_center_size(cell.center(), Vec2::splat(ICON_SIZE)),
            c,
        );
        eye_pressed = r.clicked();
        r.on_hover_text(if visible {
            "Hide this item"
        } else {
            "Show this item"
        });
        right -= CELL;
    }
    let mut lock_pressed = false;
    if let Some(locked) = row.locked {
        let cell = Rect::from_min_size(
            pos2(right - CELL, rect.min.y),
            Vec2::new(CELL, TREE_ROW_HEIGHT),
        );
        let r = ui.interact(cell, base.with("lock"), Sense::click());
        let label = format!("{} {}", if locked { "Unlock" } else { "Lock" }, row.name);
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label.clone()));
        let icon = if locked {
            icons::LOCK
        } else {
            icons::LOCK_OPEN
        };
        let base_colour = if locked { t.text } else { t.muted };
        let c = if r.hovered() {
            t.text
        } else {
            Tokens::faded(base_colour, if locked { 1.0 } else { 0.7 })
        };
        (icon.0)(
            ui.painter(),
            Rect::from_center_size(cell.center(), Vec2::splat(ICON_SIZE)),
            c,
        );
        lock_pressed = r.clicked();
        r.on_hover_text(if locked {
            "Unlock this item"
        } else {
            "Lock this item"
        });
        right -= CELL;
    }
    if let Some(count) = &row.count {
        let galley = ui
            .painter()
            .layout_no_wrap(count.clone(), Role::Mono.font(), t.muted);
        let w = galley.size().x;
        ui.painter().galley(
            pos2(right - 8.0 - w, cy - galley.size().y / 2.0),
            galley,
            t.muted,
        );
        right -= w + 12.0;
    }
    let galley = ui.painter().layout_no_wrap(
        row.name.to_owned(),
        if row.selected {
            Role::Strong.font()
        } else {
            Role::Body.font()
        },
        ink,
    );
    // Clip the name so a long one never runs under the count or cells.
    let clip = Rect::from_min_max(pos2(x, rect.min.y), pos2(right, rect.max.y));
    ui.painter()
        .with_clip_rect(clip.intersect(ui.clip_rect()))
        .galley(pos2(x, cy - galley.size().y / 2.0), galley, ink);
    super::focus::focus_ring(ui, &response, 0);
    TreeRowResponse {
        row: response,
        toggled,
        lock_pressed,
        eye_pressed,
    }
}

/// How serious a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// A warning: gold triangle.
    Warning,
    /// An error: red circle.
    Error,
    /// Nothing wrong: green tick.
    Ok,
}

/// What a finding row shows.
#[derive(Clone, Debug)]
pub struct Finding<'a> {
    /// How serious it is.
    pub severity: Severity,
    /// One strong line ("VCC is not fully routed").
    pub title: &'a str,
    /// Muted detail (where, which layer); drawn on the same line in the compact variant.
    pub detail: &'a str,
    /// Mono chip for the net or part.
    pub chip: Option<&'a str>,
    /// The row is the current selection.
    pub selected: bool,
    /// The 36 high one-line variant used in the bottom sheet.
    pub compact: bool,
}

/// What happened in a finding row this frame.
#[derive(Clone, Debug)]
pub struct FindingResponse {
    /// The row; `clicked()` selects it.
    pub row: Response,
    /// Show was pressed.
    pub show: bool,
    /// Hide was pressed.
    pub hide: bool,
    /// Mute was pressed.
    pub mute: bool,
}

/// A finding row: severity icon, strong title, muted detail, chip, Show, Hide, Mute.
///
/// Why: check results read the same in the sheet and on a page.
///
/// # Rules
/// - 48 high, or 36 compact with one line.
/// - Severity is shown by icon shape and by the title text, never by colour alone.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/button.rs
/// - crates/ui-egui/src/ui_kit/chip.rs
///
/// # Do not
/// - Hide the Show, Hide and Mute actions; every finding has all three.
pub fn finding_row(ui: &mut Ui, finding: &Finding<'_>) -> FindingResponse {
    let t = theme::tokens(ui.ctx());
    let height = if finding.compact {
        FINDING_COMPACT_HEIGHT
    } else {
        FINDING_ROW_HEIGHT
    };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    response.widget_info(|| {
        WidgetInfo::selected(
            WidgetType::SelectableLabel,
            true,
            finding.selected,
            finding.title,
        )
    });
    paint_row_background(ui, rect, finding.selected, response.hovered(), &t);
    divider(ui, rect, &t);
    let (icon, colour) = match finding.severity {
        Severity::Warning => (icons::WARNING, t.gold),
        Severity::Error => (icons::ERROR, t.red),
        Severity::Ok => (icons::OK, t.mask),
    };
    let icon_rect = Rect::from_min_size(
        pos2(rect.min.x + 12.0, rect.center().y - ICON_SIZE / 2.0),
        Vec2::splat(ICON_SIZE),
    );
    (icon.0)(ui.painter(), icon_rect, colour);
    let mut actions = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(12.0, 0.0)))
            .layout(Layout::right_to_left(egui::Align::Center)),
    );
    actions.spacing_mut().item_spacing.x = 4.0;
    let mute = button::ghost_button(
        &mut actions,
        None,
        "Mute",
        "Stop reporting this finding until the next run",
        Size::Small,
        true,
    )
    .clicked();
    let hide = button::ghost_button(
        &mut actions,
        None,
        "Hide",
        "Take this finding out of the list",
        Size::Small,
        true,
    )
    .clicked();
    let show = button::secondary_button(
        &mut actions,
        None,
        "Show",
        "Select the item on the board",
        Size::Small,
        true,
    )
    .clicked();
    if let Some(text) = finding.chip {
        actions.add_space(8.0);
        chip::chip(&mut actions, text, "The net or part this finding is about");
    }
    let used = actions.min_rect().width();
    let text_rect = Rect::from_min_max(
        pos2(rect.min.x + 12.0 + ICON_SIZE + 8.0, rect.min.y),
        pos2(rect.max.x - 12.0 - used - 8.0, rect.max.y),
    );
    let mut text_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect).layout(
        if finding.compact {
            Layout::left_to_right(egui::Align::Center)
        } else {
            Layout::top_down(egui::Align::Min)
        },
    ));
    text_ui.set_clip_rect(text_rect.intersect(ui.clip_rect()));
    if !finding.compact {
        text_ui.add_space(5.0);
    }
    text_ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
    text_ui.add(egui::Label::new(fonts::strong(finding.title).color(t.text)).truncate());
    text_ui
        .add(egui::Label::new(fonts::text(Role::Body, finding.detail).color(t.muted)).truncate());
    FindingResponse {
        row: response,
        show,
        hide,
        mute,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame};

    /// List rows are 36 high, tree rows 28, finding rows 48 and 36 compact.
    #[test]
    fn row_heights() {
        let ctx = context(true);
        let mut got = Vec::new();
        frame(&ctx, vec![], |ui| {
            got.push(
                list_row(ui, false, "R1", "Select R1", |ui| {
                    ui.label("R1");
                })
                .rect
                .height(),
            );
            let row = TreeRow {
                depth: 0,
                expanded: None,
                swatch: None,
                icon: None,
                name: "Pads",
                count: None,
                locked: None,
                visible: None,
                selected: false,
                dimmed: false,
            };
            got.push(tree_row(ui, "t", &row).row.rect.height());
            let f = Finding {
                severity: Severity::Warning,
                title: "VCC is not routed",
                detail: "Top copper",
                chip: Some("VCC"),
                selected: false,
                compact: false,
            };
            got.push(finding_row(ui, &f).row.rect.height());
            got.push(
                finding_row(ui, &Finding { compact: true, ..f })
                    .row
                    .rect
                    .height(),
            );
        });
        assert_eq!(got, vec![36.0, 28.0, 48.0, 36.0]);
    }

    /// Eye and lock cells are named for the row and reported when pressed.
    #[test]
    fn tree_cells_have_names_and_report_presses() {
        use crate::ui_kit::testing::click;
        let ctx = context(true);
        let row = TreeRow {
            depth: 1,
            expanded: Some(true),
            swatch: Some(Color32::from_rgb(232, 131, 74)),
            icon: None,
            name: "Top copper",
            count: Some("6".into()),
            locked: Some(false),
            visible: Some(true),
            selected: false,
            dimmed: false,
        };
        let mut rect = Rect::NOTHING;
        let out = frame(&ctx, vec![], |ui| {
            rect = tree_row(ui, "t", &row).row.rect;
        });
        let names: Vec<String> = out
            .platform_output
            .accesskit_update
            .map(|u| {
                u.nodes
                    .iter()
                    .filter_map(|(_, n)| n.label().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        for expected in ["Collapse Top copper", "Hide Top copper", "Lock Top copper"] {
            assert!(
                names.iter().any(|n| n == expected),
                "missing {expected}: {names:?}"
            );
        }
        let eye = pos2(rect.max.x - 4.0 - CELL / 2.0, rect.center().y);
        let mut pressed = false;
        frame(&ctx, click(eye), |ui| {
            pressed = tree_row(ui, "t", &row).eye_pressed;
        });
        assert!(pressed);
    }

    /// Indent grows 16 per level.
    #[test]
    fn indent_is_16_per_level() {
        assert_eq!(TREE_INDENT, 16.0);
    }
}
