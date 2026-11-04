//! One text style for everything written on the sheet and the board.
//!
//! Part references, values, net names, port names, pin names and numbers and pad labels belong to the
//! drawing. They are sized in millimetres of the drawing, so they grow and shrink with the zoom the way
//! silkscreen text does. Every painter asks this module for its font and none picks a point size itself.
//!
//! # Rules
//! - A role names what the text is. `TextRole::mm` is the one table of sizes.
//! - The point size is the size in millimetres times the zoom, kept between `MIN_POINTS` and `MAX_POINTS`.
//!   Far out, text stays readable. Far in, it stays cheap to draw.
//! - Text the designer sized (silkscreen text, drawn words) goes through `font_mm` with its own size.
//!
//! # Dependencies
//! - crates/ui-egui/src/canvas.rs (`Camera::zoom` is pixels per millimetre)
//!
//! # Do not
//! - Write `FontId::monospace(10.0)` or any fixed point size for text that sits on the drawing.
//!   Fixed sizes are for the interface around the drawing: rulers, readouts, hints.
use egui::FontId;

/// The smallest point size drawing text is painted at.
pub(crate) const MIN_POINTS: f32 = 6.0;
/// The largest point size drawing text is painted at.
pub(crate) const MAX_POINTS: f32 = 48.0;

/// What a piece of drawing text is. The role decides its size and its face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextRole {
    /// A part's reference on the sheet, such as R1.
    Reference,
    /// A part's value on the sheet, such as 10k.
    Value,
    /// A net name beside a wire or on a label.
    NetName,
    /// A hierarchy port's name.
    PortName,
    /// A pin's name inside or beside a symbol.
    PinName,
    /// A pin's number at the end of its stub.
    PinNumber,
    /// The text on a pad on the board: number or net.
    PadLabel,
    /// A part's reference on the board.
    BoardReference,
    /// A part's value on the board.
    BoardValue,
}

impl TextRole {
    /// Height of this text in millimetres of the drawing.
    pub(crate) fn mm(self) -> f32 {
        match self {
            Self::Reference => 2.5,
            Self::Value | Self::NetName | Self::PortName => 2.0,
            Self::PinName => 1.7,
            Self::PinNumber => 1.8,
            Self::PadLabel => 1.25,
            Self::BoardReference => 1.6,
            Self::BoardValue => 1.3,
        }
    }

    /// True when this text is set in the fixed-width face, as sheet text is.
    pub(crate) fn mono(self) -> bool {
        !matches!(self, Self::BoardReference | Self::BoardValue)
    }
}

/// Point size of text `mm` tall at `zoom` pixels per millimetre.
pub(crate) fn points_mm(mm: f32, zoom: f32) -> f32 {
    (mm * zoom).clamp(MIN_POINTS, MAX_POINTS)
}

/// The font for text `mm` tall at `zoom`, fixed-width when `mono` is set.
pub(crate) fn font_mm(mm: f32, zoom: f32, mono: bool) -> FontId {
    crate::fonts::scaled(mono, points_mm(mm, zoom))
}

/// The font for a role at `zoom` pixels per millimetre. This is what painters call.
pub(crate) fn role_font(role: TextRole, zoom: f32) -> FontId {
    font_mm(role.mm(), zoom, role.mono())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROLES: [TextRole; 9] = [
        TextRole::Reference,
        TextRole::Value,
        TextRole::NetName,
        TextRole::PortName,
        TextRole::PinName,
        TextRole::PinNumber,
        TextRole::PadLabel,
        TextRole::BoardReference,
        TextRole::BoardValue,
    ];

    #[test]
    fn text_grows_with_the_zoom_between_the_limits() {
        for role in ROLES {
            let near = role_font(role, 12.0).size;
            let far = role_font(role, 6.0).size;
            assert!(
                near > far || (near - MAX_POINTS).abs() < f32::EPSILON,
                "{role:?} is larger at 12 px/mm than at 6 px/mm: {near} and {far}"
            );
        }
    }

    #[test]
    fn text_stays_readable_far_out_and_bounded_far_in() {
        for role in ROLES {
            assert!(
                (role_font(role, 0.01).size - MIN_POINTS).abs() < f32::EPSILON,
                "{role:?} keeps the smallest size far out"
            );
            assert!(
                (role_font(role, 1000.0).size - MAX_POINTS).abs() < f32::EPSILON,
                "{role:?} keeps the largest size far in"
            );
        }
    }

    #[test]
    fn every_role_has_a_size_and_sheet_text_is_fixed_width() {
        for role in ROLES {
            assert!(role.mm() > 0.0, "{role:?} has a size");
        }
        assert!(TextRole::Reference.mono());
        assert!(!TextRole::BoardReference.mono());
    }

    #[test]
    fn a_designer_sized_text_follows_the_zoom_too() {
        assert!(points_mm(1.5, 20.0) > points_mm(1.5, 10.0));
    }
}
