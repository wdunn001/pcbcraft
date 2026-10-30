//! Modal dialog frame: dimmed scrim, header, optional tab row, scrolling body, footer.
//!
//! Context: design system section 7 (Project settings and Preferences). Built on
//! `egui::Modal`, which blocks input and focus for everything beneath it. The
//! sibling craft-ui crate pinned in `Cargo.toml` has no `Window` or `Confirm`
//! container at that revision, so this is the one implementation; when craft-ui
//! ships them, this file should become a thin wrapper.
//! Local rules: header 40, footer 48, body padding 16. Escape cancels, Enter runs the
//! primary action unless a text box is being typed in, and focus stays inside.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/button.rs
//! - crates/ui-egui/src/ui_kit/tab_bar.rs
//!
//! # Do not
//! - Close on a click outside; unsaved settings would be lost by accident.
//! - Put a side list of pages in a dialog; pages are tabs at the top.
use super::button::{self, Size};
use super::icons;
use super::tab_bar::{self, TabItem, TabSize};
use crate::fonts;
use crate::theme::{self, RADIUS_CARD};
use egui::{
    Context, CornerRadius, Frame, Id, Key, Layout, Margin, Modal, Modifiers, Popup, ScrollArea,
    Sense, Stroke, Ui, Vec2,
};

/// Header height in points.
pub const HEADER_HEIGHT: f32 = 40.0;
/// Footer height in points.
pub const FOOTER_HEIGHT: f32 = 48.0;
/// Body padding in points.
pub const BODY_PADDING: i8 = 16;

/// What a dialog shows.
#[derive(Clone, Debug)]
pub struct Dialog<'a> {
    /// Stable id of the dialog.
    pub id: &'a str,
    /// Title, in the header.
    pub title: &'a str,
    /// Width in points.
    pub width: f32,
    /// Tabs at the top of the dialog; empty for none.
    pub tabs: &'a [TabItem<'a>],
    /// Muted note at the left of the footer.
    pub note: Option<&'a str>,
    /// The secondary button as (label, tooltip); `None` for a single-button dialog.
    pub cancel: Option<(&'a str, &'a str)>,
    /// The primary button as (label, tooltip).
    pub primary: (&'a str, &'a str),
}

/// What the person did this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Still open.
    Open,
    /// The primary action ran (button or Enter).
    Primary,
    /// Cancelled (Cancel, the close button or Escape).
    Cancel,
}

/// Show a modal dialog for one frame and report what happened.
///
/// Why: Project settings, Preferences and confirmations share one frame, so keys
/// and focus behave the same in all of them.
///
/// # Rules
/// - `tab` is the selected tab index when `dialog.tabs` is not empty.
/// - The body scrolls; its height is bounded by 80 percent of the window.
/// - Escape returns `Cancel` unless a popup (a dropdown) is open; Enter returns
///   `Primary` unless a text box is being typed in.
/// - Clicks outside the dialog do nothing.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/button.rs
/// - crates/ui-egui/src/ui_kit/tab_bar.rs
///
/// # Do not
/// - Call it when the dialog is closed; the caller owns the open flag.
pub fn show(
    ctx: &Context,
    dialog: &Dialog<'_>,
    tab: &mut usize,
    body: impl FnOnce(&mut Ui),
) -> Outcome {
    let t = theme::tokens(ctx);
    let mut outcome = Outcome::Open;
    let frame = Frame::new()
        .fill(t.panel)
        .stroke(Stroke::new(theme::BORDER, t.edge))
        .corner_radius(CornerRadius::same(RADIUS_CARD))
        .inner_margin(Margin::ZERO);
    let max_body = (ctx.content_rect().height() * 0.8
        - HEADER_HEIGHT
        - FOOTER_HEIGHT
        - if dialog.tabs.is_empty() {
            0.0
        } else {
            theme::BAR_HEIGHT
        })
    .max(120.0);
    Modal::new(Id::new(("pcbcraft-dialog", dialog.id)))
        .backdrop_color(t.scrim)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(dialog.width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            // Header: title left, close right, rule under it.
            let (header, _) =
                ui.allocate_exact_size(Vec2::new(dialog.width, HEADER_HEIGHT), Sense::hover());
            let inner = header.shrink2(Vec2::new(16.0, 0.0));
            let mut left = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(Layout::left_to_right(egui::Align::Center)),
            );
            left.label(fonts::title(dialog.title).color(t.text));
            let mut right = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(Layout::right_to_left(egui::Align::Center)),
            );
            if button::icon_button(
                &mut right,
                icons::CLOSE,
                "Close",
                "Close without saving",
                Size::Normal,
                true,
            )
            .clicked()
            {
                outcome = Outcome::Cancel;
            }
            ui.painter().hline(
                header.x_range(),
                header.max.y - 0.5,
                Stroke::new(theme::BORDER, t.line),
            );
            // Tab row.
            if !dialog.tabs.is_empty() {
                let (row, _) = ui.allocate_exact_size(
                    Vec2::new(dialog.width, theme::BAR_HEIGHT),
                    Sense::hover(),
                );
                let mut tabs_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row.shrink2(Vec2::new(8.0, 0.0)))
                        .layout(Layout::top_down(egui::Align::Min)),
                );
                tab_bar::tab_bar(
                    &mut tabs_ui,
                    ("dialog-tabs", dialog.id),
                    tab,
                    dialog.tabs,
                    TabSize::Aside,
                );
                ui.painter().hline(
                    row.x_range(),
                    row.max.y - 0.5,
                    Stroke::new(theme::BORDER, t.line),
                );
            }
            // Scrolling body.
            ScrollArea::vertical()
                .id_salt(("dialog-body", dialog.id))
                .max_height(max_body)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    Frame::new()
                        .inner_margin(Margin::same(BODY_PADDING))
                        .show(ui, |ui| {
                            ui.set_width(dialog.width - 2.0 * f32::from(BODY_PADDING));
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                            body(ui);
                        });
                });
            // Footer: note left, buttons right, rule above.
            let (footer, _) =
                ui.allocate_exact_size(Vec2::new(dialog.width, FOOTER_HEIGHT), Sense::hover());
            ui.painter().hline(
                footer.x_range(),
                footer.min.y + 0.5,
                Stroke::new(theme::BORDER, t.line),
            );
            let inner = footer.shrink2(Vec2::new(16.0, 0.0));
            if let Some(note) = dialog.note {
                let mut note_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(Layout::left_to_right(egui::Align::Center)),
                );
                note_ui.label(fonts::caption(note).color(t.muted));
            }
            let mut buttons = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(Layout::right_to_left(egui::Align::Center)),
            );
            buttons.spacing_mut().item_spacing.x = 8.0;
            if button::primary_button(
                &mut buttons,
                None,
                dialog.primary.0,
                dialog.primary.1,
                Size::Normal,
                true,
            )
            .clicked()
            {
                outcome = Outcome::Primary;
            }
            if let Some((label, tip)) = dialog.cancel
                && button::secondary_button(&mut buttons, None, label, tip, Size::Normal, true)
                    .clicked()
            {
                outcome = Outcome::Cancel;
            }
        });
    if outcome == Outcome::Open && !Popup::is_any_open(ctx) {
        let typing = ctx.egui_wants_keyboard_input();
        let (esc, enter) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::Escape),
                !typing && i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        if esc {
            outcome = Outcome::Cancel;
        } else if enter {
            outcome = Outcome::Primary;
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, key};

    /// Run one frame that shows the dialog and report its outcome and the tree.
    fn run(
        ctx: &Context,
        events: Vec<egui::Event>,
        tab: &mut usize,
    ) -> (Outcome, egui::FullOutput) {
        let tabs = [
            TabItem::new("Board", "Board and stackup"),
            TabItem::new("Rules", "Design rules"),
        ];
        let spec = Dialog {
            id: "project",
            title: "Project settings",
            width: 560.0,
            tabs: &tabs,
            note: Some("Changes apply when you press Save"),
            cancel: Some(("Cancel", "Close without saving")),
            primary: ("Save", "Save the settings and close"),
        };
        let mut outcome = Outcome::Open;
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(800.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                outcome = show(ui.ctx(), &spec, tab, |ui| {
                    ui.label("Body");
                });
            },
        );
        out.textures_delta.clear();
        (outcome, out)
    }

    /// Nothing fires without a key.
    #[test]
    fn stays_open_by_default() {
        let ctx = context(true);
        let mut tab = 0;
        assert_eq!(run(&ctx, vec![], &mut tab).0, Outcome::Open);
        assert_eq!(run(&ctx, vec![], &mut tab).0, Outcome::Open);
    }

    /// Escape cancels.
    #[test]
    fn escape_cancels() {
        let ctx = context(true);
        let mut tab = 0;
        run(&ctx, vec![], &mut tab);
        assert_eq!(run(&ctx, key(Key::Escape), &mut tab).0, Outcome::Cancel);
    }

    /// Enter runs the primary action.
    #[test]
    fn enter_runs_the_primary_action() {
        let ctx = context(true);
        let mut tab = 0;
        run(&ctx, vec![], &mut tab);
        assert_eq!(run(&ctx, key(Key::Enter), &mut tab).0, Outcome::Primary);
    }

    /// Close, Cancel, Save and both tabs are named controls inside the dialog.
    #[test]
    fn controls_are_named() {
        let ctx = context(true);
        let mut tab = 0;
        run(&ctx, vec![], &mut tab);
        let (_, out) = run(&ctx, vec![], &mut tab);
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
        for expected in ["Close", "Cancel", "Save", "Board", "Rules"] {
            assert!(
                names.iter().any(|n| n == expected),
                "missing {expected}: {names:?}"
            );
        }
    }
}
