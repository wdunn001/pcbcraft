//! Buttons of the design: primary, secondary, ghost and icon, with every state.
//!
//! Context: design system 5.1 to 5.3. Each helper takes a label and a tooltip,
//! sets `widget_info` so the AccessKit tree has a named button, and draws its own
//! frame because egui's shared button fill cannot tell primary from secondary.
//! Local rules: colours come from `crate::theme::Tokens`, sizes from `Size`;
//! `paint_button` is the single painter, used by live buttons and the gallery.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/fonts.rs
//! - crates/ui-egui/src/ui_kit/focus.rs
//! - crates/ui-egui/src/ui_kit/icons.rs
//!
//! # Do not
//! - Add a button without a tooltip; disabled buttons say how to enable them.
//! - Show more than one primary button per region.
use super::focus;
use super::icons::{ICON_SIZE, Icon, icon_rect};
use crate::fonts::Role;
use crate::theme::{self, CONTROL_HEIGHT, CONTROL_HEIGHT_LARGE, CONTROL_HEIGHT_SMALL, Tokens};
use egui::{
    Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo,
    WidgetType, pos2,
};

/// Which of the four button looks to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Copper fill, dark text: the one main action of a region.
    Primary,
    /// Raised fill with an edge border.
    Secondary,
    /// No fill and no border, for tertiary actions in rows.
    Ghost,
    /// A 28 px square with only an icon.
    Icon,
}

/// Button height class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// 24 high.
    Small,
    /// 28 high.
    Normal,
    /// 32 high.
    Large,
}

impl Size {
    /// Height in points.
    pub fn height(self) -> f32 {
        match self {
            Size::Small => CONTROL_HEIGHT_SMALL,
            Size::Normal => CONTROL_HEIGHT,
            Size::Large => CONTROL_HEIGHT_LARGE,
        }
    }

    /// Side padding in points.
    pub fn padding(self) -> f32 {
        match self {
            Size::Small => 8.0,
            Size::Normal => 12.0,
            Size::Large => 16.0,
        }
    }
}

/// The visual state a button is drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Idle.
    Rest,
    /// Pointer over it.
    Hover,
    /// Pressed.
    Active,
    /// Cannot be used.
    Disabled,
    /// Keyboard focus: rest look plus the focus ring.
    Focus,
}

/// Everything needed to draw one button.
#[derive(Clone, Copy, Debug)]
pub struct Look<'a> {
    /// Which look.
    pub kind: Kind,
    /// Height class.
    pub size: Size,
    /// Optional leading icon (the only content of an icon button).
    pub icon: Option<Icon>,
    /// Visible label; empty for an icon button.
    pub label: &'a str,
}

pub(crate) fn brighten(c: Color32) -> Color32 {
    c.lerp_to_gamma(Color32::WHITE, 0.10)
}

pub(crate) fn darken(c: Color32) -> Color32 {
    c.lerp_to_gamma(Color32::BLACK, 0.08)
}

/// The size a button of this look needs.
///
/// Why: layout code and tests need the width without painting.
pub fn measure(ui: &Ui, look: &Look<'_>) -> Vec2 {
    let h = look.size.height();
    if look.kind == Kind::Icon {
        return Vec2::splat(h);
    }
    let font = label_font(look.kind);
    let text_w = if look.label.is_empty() {
        0.0
    } else {
        ui.painter()
            .layout_no_wrap(look.label.to_owned(), font, Color32::PLACEHOLDER)
            .size()
            .x
    };
    let icon_w = if look.icon.is_some() { ICON_SIZE } else { 0.0 };
    let gap = if icon_w > 0.0 && text_w > 0.0 {
        6.0
    } else {
        0.0
    };
    Vec2::new(text_w + icon_w + gap + 2.0 * look.size.padding(), h)
}

fn label_font(kind: Kind) -> egui::FontId {
    match kind {
        Kind::Primary => Role::Strong.font(),
        _ => Role::Button.font(),
    }
}

/// Paint one button in one state into `rect`.
///
/// Why: the live helpers and the gallery draw through this, so the gallery shows
/// exactly what a page gets in every state.
///
/// # Rules
/// - Rest, hover, active, disabled and focus follow design system 5.1 to 5.3.
/// - Disabled is 45 percent opacity with no hover change.
/// - Focus draws the rest look plus the 2 px ring.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/focus.rs
///
/// # Do not
/// - Allocate space here; the caller owns the rect.
pub fn paint_button(ui: &Ui, rect: Rect, look: &Look<'_>, state: State, t: &Tokens) {
    let painter = ui.painter();
    let radius = CornerRadius::same(theme::RADIUS_CONTROL);
    let (mut fill, mut border, mut ink) = match look.kind {
        Kind::Primary => {
            let fill = match state {
                State::Hover => brighten(t.copper),
                State::Active => darken(t.copper),
                _ => t.copper,
            };
            (fill, Color32::TRANSPARENT, t.onacc)
        }
        Kind::Secondary => match state {
            State::Hover => (t.hover, t.edge, t.text),
            State::Active => (t.raised, t.copper, t.text),
            _ => (t.raised, t.edge, t.text),
        },
        Kind::Ghost => match state {
            State::Hover => (t.hover, Color32::TRANSPARENT, t.text),
            State::Active => (t.raised, t.copper, t.text),
            _ => (Color32::TRANSPARENT, Color32::TRANSPARENT, t.text),
        },
        Kind::Icon => match state {
            State::Hover => (t.raised, Color32::TRANSPARENT, t.text),
            State::Active => (Color32::TRANSPARENT, Color32::TRANSPARENT, t.copper),
            _ => (Color32::TRANSPARENT, Color32::TRANSPARENT, t.muted),
        },
    };
    if state == State::Disabled {
        fill = Tokens::faded(fill, 0.45);
        border = Tokens::faded(border, 0.45);
        ink = Tokens::faded(ink, 0.45);
    }
    painter.rect_filled(rect, radius, fill);
    if border != Color32::TRANSPARENT {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(theme::BORDER, border),
            StrokeKind::Inside,
        );
    }
    let mut x = rect.min.x + look.size.padding();
    let cy = rect.center().y;
    if look.kind == Kind::Icon {
        if let Some(icon) = look.icon {
            (icon.0)(painter, icon_rect(rect), ink);
        }
    } else {
        if let Some(icon) = look.icon {
            let r = Rect::from_min_size(pos2(x, cy - ICON_SIZE / 2.0), Vec2::splat(ICON_SIZE));
            (icon.0)(painter, r, ink);
            x += ICON_SIZE + 6.0;
        }
        if !look.label.is_empty() {
            let galley = painter.layout_no_wrap(look.label.to_owned(), label_font(look.kind), ink);
            let pos = pos2(x, cy - galley.size().y / 2.0);
            painter.galley(pos, galley, ink);
        }
    }
    if state == State::Focus {
        focus::paint_ring(painter, rect, theme::RADIUS_CONTROL, t);
    }
}

/// Allocate, paint and report one button. Every public helper calls this.
fn show(ui: &mut Ui, look: Look<'_>, name: &str, tip: &str, enabled: bool) -> Response {
    let t = theme::tokens(ui.ctx());
    let size = measure(ui, &look);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, name));
    if ui.is_rect_visible(rect) {
        let state = if !enabled {
            State::Disabled
        } else if response.is_pointer_button_down_on() {
            State::Active
        } else if response.hovered() {
            State::Hover
        } else if response.has_focus() {
            State::Focus
        } else {
            State::Rest
        };
        paint_button(ui, rect, &look, state, &t);
        if response.has_focus() && state != State::Focus {
            focus::paint_ring(ui.painter(), rect, theme::RADIUS_CONTROL, &t);
        }
    }
    if enabled {
        response.on_hover_text(tip)
    } else {
        response.on_disabled_hover_text(tip)
    }
}

/// The main action of a region: copper fill, dark text, 600, 28 high.
///
/// Why: one primary per page header, tool bar or aside footer tells a first-time
/// user what to press.
///
/// # Rules
/// - `label` is one to three words; the effect goes in `tip`.
/// - Pass `enabled = false` with a tip that says how to enable it.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/button.rs (`paint_button`)
///
/// # Do not
/// - Use two primaries in one region.
pub fn primary_button(
    ui: &mut Ui,
    icon: Option<Icon>,
    label: &str,
    tip: &str,
    size: Size,
    enabled: bool,
) -> Response {
    let look = Look {
        kind: Kind::Primary,
        size,
        icon,
        label,
    };
    show(ui, look, label, tip, enabled)
}

/// A secondary action: raised fill, 1 px edge border, 500 weight.
///
/// # Rules
/// - Same label and tip rules as `primary_button`.
pub fn secondary_button(
    ui: &mut Ui,
    icon: Option<Icon>,
    label: &str,
    tip: &str,
    size: Size,
    enabled: bool,
) -> Response {
    let look = Look {
        kind: Kind::Secondary,
        size,
        icon,
        label,
    };
    show(ui, look, label, tip, enabled)
}

/// A tertiary action with no fill and no border (Hide, Mute, Reset view).
///
/// # Rules
/// - Same label and tip rules as `primary_button`.
pub fn ghost_button(
    ui: &mut Ui,
    icon: Option<Icon>,
    label: &str,
    tip: &str,
    size: Size,
    enabled: bool,
) -> Response {
    let look = Look {
        kind: Kind::Ghost,
        size,
        icon,
        label,
    };
    show(ui, look, label, tip, enabled)
}

/// A square icon-only button, 28 by 28 with a 16 px icon.
///
/// Why: add, more, zoom and collapse controls need a plain target; the tooltip and
/// accessible name are required because the icon carries no text.
///
/// # Rules
/// - `name` becomes the accessible name; `tip` is the tooltip and may add detail.
///
/// # Do not
/// - Pass an empty `name` or `tip`.
pub fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    name: &str,
    tip: &str,
    size: Size,
    enabled: bool,
) -> Response {
    let look = Look {
        kind: Kind::Icon,
        size,
        icon: Some(icon),
        label: "",
    };
    show(ui, look, name, tip, enabled)
}

/// Draw a button in a fixed state, without interaction. For the gallery and tests.
///
/// Why: states such as hover and focus cannot be reached by a screenshot run.
pub fn static_button(ui: &mut Ui, look: Look<'_>, state: State) -> Response {
    let t = theme::tokens(ui.ctx());
    let size = measure(ui, &look);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    paint_button(ui, rect, &look, state, &t);
    response
}

/// Text style of a button label, for callers that lay out their own content.
pub fn label_role(kind: Kind) -> Role {
    if kind == Kind::Primary {
        Role::Strong
    } else {
        Role::Button
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Tokens;

    /// Run one frame that shows a button and report its response.
    fn run(f: impl FnOnce(&mut Ui) -> Response + 'static) -> Result<Response, String> {
        let ctx = egui::Context::default();
        theme::apply(&ctx, &Tokens::DARK);
        crate::fonts::install(&ctx);
        let mut out = None;
        let mut f = Some(f);
        let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
            if let Some(f) = f.take() {
                out = Some(f(ui));
            }
        });
        full.textures_delta.clear();
        out.ok_or_else(|| "the frame did not run".to_owned())
    }

    /// Heights follow the three size classes.
    #[test]
    fn heights_are_24_28_and_32() -> Result<(), String> {
        for (size, h) in [
            (Size::Small, 24.0),
            (Size::Normal, 28.0),
            (Size::Large, 32.0),
        ] {
            let r =
                run(move |ui| primary_button(ui, None, "Save", "Save the project", size, true))?;
            assert_eq!(r.rect.height(), h, "{size:?}");
        }
        Ok(())
    }

    /// An icon button is a square of its height class.
    #[test]
    fn icon_button_is_square() -> Result<(), String> {
        let r = run(|ui| {
            icon_button(
                ui,
                crate::ui_kit::icons::PLUS,
                "Add",
                "Add a layer",
                Size::Normal,
                true,
            )
        })?;
        assert_eq!(r.rect.size(), Vec2::splat(28.0));
        Ok(())
    }

    /// Disabled buttons do not report clicks.
    #[test]
    fn disabled_button_has_no_click_sense() -> Result<(), String> {
        let r = run(|ui| secondary_button(ui, None, "Run", "Run the checks", Size::Normal, false))?;
        assert!(!r.sense.senses_click());
        Ok(())
    }

    /// The disabled look is 45 percent opacity.
    #[test]
    fn disabled_fades_to_45_percent() {
        let faded = Tokens::faded(Tokens::DARK.copper, 0.45);
        assert!(faded.a() >= 112 && faded.a() <= 118, "alpha {}", faded.a());
    }
}
