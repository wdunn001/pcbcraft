//! Number field with a unit: Mono value on the left, muted unit on the right.
//!
//! Context: design system 5.10. A raised box with a 1 px edge border, 28 high; focus
//! turns the border to `text` and adds the ring; an invalid entry turns it `red` and
//! shows the message under the box. Parsing goes through `crate::number_entry`
//! so the decimal marker follows the language.
//! Local rules: the unit is always visible in the box, never in the label. Arrow up
//! and down step by `step` (Shift by ten times), Enter or leaving the box commits,
//! Escape reverts. A typed unit that is a length or angle unit of the same kind is
//! converted (`10 mil` in a millimetre box stores 0.254).
//!
//! # Dependencies
//! - crates/ui-egui/src/number_entry.rs
//! - crates/text/src/numbers.rs
//! - crates/ui-egui/src/ui_kit/focus.rs
//!
//! # Do not
//! - Store the unit text; the value is a plain number in the field's unit.
//! - Accept NaN, infinity or a value outside `min..=max`.
use super::focus;
use super::icons::{self, ICON_SIZE};
use crate::fonts::{self, Role};
use crate::number_entry::parse_box;
use crate::theme::{self, Tokens};
use egui::{
    Color32, CornerRadius, Id, Key, Modifiers, Response, Sense, Stroke, StrokeKind, TextEdit, Ui,
    Vec2, WidgetInfo, WidgetType, pos2,
};
use pcbcraft_text::numbers::fmt_ui;
use std::hash::Hash;

/// What a number field accepts and how it looks.
#[derive(Clone, Copy, Debug)]
pub struct NumberSpec<'a> {
    /// Unit shown at the right of the box (`mm`, `deg`, `V`); may be empty.
    pub unit: &'a str,
    /// Arrow-key step in the field's unit.
    pub step: f64,
    /// Smallest accepted value.
    pub min: f64,
    /// Largest accepted value.
    pub max: f64,
    /// Decimals shown.
    pub places: usize,
    /// Width of the box in points (96 to 140 in the design).
    pub width: f32,
}

impl Default for NumberSpec<'_> {
    fn default() -> Self {
        Self {
            unit: "mm",
            step: 0.1,
            min: -1.0e6,
            max: 1.0e6,
            places: 2,
            width: 120.0,
        }
    }
}

/// What a unit word means: a kind of quantity and its size in the kind's base unit.
fn unit_factor(unit: &str) -> Option<(&'static str, f64)> {
    match unit.trim().to_lowercase().as_str() {
        "mm" => Some(("length", 1.0)),
        "um" | "\u{b5}m" => Some(("length", 0.001)),
        "cm" => Some(("length", 10.0)),
        "mil" | "thou" => Some(("length", 0.0254)),
        "in" | "inch" | "\"" => Some(("length", 25.4)),
        "deg" | "\u{b0}" => Some(("angle", 1.0)),
        "rad" => Some(("angle", 180.0 / std::f64::consts::PI)),
        _ => None,
    }
}

/// Read what a person typed into a field whose unit is `unit`.
///
/// Why: people type `0.25 mm` or `10 mil` into a box that already shows `mm`; the
/// number should arrive in the box's unit.
///
/// # Rules
/// - No unit typed: the number is in the field's unit.
/// - Same kind of unit (length with length, angle with angle): converted.
/// - A unit of another kind, or an unknown word, is refused with a plain message.
///
/// # Dependencies
/// - crates/ui-egui/src/number_entry.rs (`parse_box`)
///
/// # Do not
/// - Convert between units of different kinds.
pub fn parse_with_unit(text: &str, unit: &str) -> Result<f64, String> {
    let text = text.trim();
    let split = text
        .char_indices()
        .find(|(_, c)| c.is_alphabetic() || *c == '\u{b0}' || *c == '"')
        .map_or(text.len(), |(i, _)| i);
    let (number, typed) = text.split_at(split);
    let value = parse_box(number)?;
    let typed = typed.trim();
    if typed.is_empty() || typed.eq_ignore_ascii_case(unit) {
        return Ok(value);
    }
    match (unit_factor(typed), unit_factor(unit)) {
        (Some((from_kind, from)), Some((to_kind, to))) if from_kind == to_kind => {
            Ok(value * from / to)
        }
        _ => Err(if unit.is_empty() {
            "This box has no unit".to_owned()
        } else {
            format!("Use {unit} here")
        }),
    }
}

/// The error text a refused entry shows, if the buffer cannot be read in range.
fn check(text: &str, spec: &NumberSpec<'_>) -> Result<f64, String> {
    let value = parse_with_unit(text, spec.unit)?;
    if !value.is_finite() || value < spec.min || value > spec.max {
        let lo = fmt_ui(&pcbcraft_text::active_number_tag(), spec.min, spec.places);
        let hi = fmt_ui(&pcbcraft_text::active_number_tag(), spec.max, spec.places);
        return Err(format!("Enter a value from {lo} to {hi}"));
    }
    Ok(value)
}

fn show_value(value: f64, spec: &NumberSpec<'_>) -> String {
    fmt_ui(&pcbcraft_text::active_number_tag(), value, spec.places)
}

/// Per-field edit state kept between frames.
#[derive(Clone, Debug, Default)]
struct Edit {
    text: String,
    error: Option<String>,
    editing: bool,
}

/// A number field. `value` is in the field's unit; the response reports `changed()`
/// on the frame the value moves.
///
/// Why: every numeric property (position, width, clearance) reads the same and
/// follows the same keys.
///
/// # Rules
/// - `name` is the accessible name, `tip` the tooltip.
/// - Arrow up and down step by `spec.step`; Shift steps ten times as far; both clamp.
/// - Enter or leaving the box commits a valid entry; Escape reverts; a refused
///   entry keeps the box open with a red border and a message below it.
///
/// # Dependencies
/// - crates/ui-egui/src/number_entry.rs
///
/// # Do not
/// - Use for integer counters; use a plain spin box.
pub fn number_field(
    ui: &mut Ui,
    salt: impl Hash + std::fmt::Debug,
    value: &mut f64,
    spec: &NumberSpec<'_>,
    name: &str,
    tip: &str,
) -> Response {
    let t = theme::tokens(ui.ctx());
    let id = Id::new(salt);
    let mut edit: Edit = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let unit_w = if spec.unit.is_empty() {
        0.0
    } else {
        ui.painter()
            .layout_no_wrap(
                spec.unit.to_owned(),
                Role::Mono.font(),
                Color32::PLACEHOLDER,
            )
            .size()
            .x
            + 8.0
    };
    let (rect, mut outer) =
        ui.allocate_exact_size(Vec2::new(spec.width, theme::CONTROL_HEIGHT), Sense::hover());
    let text_rect = egui::Rect::from_min_max(
        pos2(rect.min.x + 8.0, rect.min.y + 1.0),
        pos2(rect.max.x - 8.0 - unit_w, rect.max.y - 1.0),
    );
    let focused = ui.memory(|m| m.has_focus(id));
    // Keys are read before the text box sees them so arrows step instead of moving the cursor.
    let mut stepped = false;
    if focused {
        let (up, down, shift) = ui.input_mut(|i| {
            // Shift is matched first: a plain match would also swallow Shift+arrow.
            if i.consume_key(Modifiers::SHIFT, Key::ArrowUp) {
                (true, false, true)
            } else if i.consume_key(Modifiers::SHIFT, Key::ArrowDown) {
                (false, true, true)
            } else {
                (
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    false,
                )
            }
        });
        if up || down {
            let base = check(&edit.text, spec).unwrap_or(*value);
            let step = spec.step * if shift { 10.0 } else { 1.0 };
            let next = (if up { base + step } else { base - step }).clamp(spec.min, spec.max);
            let rounded = round_to(next, spec.places);
            if rounded != *value {
                *value = rounded;
                stepped = true;
            }
            edit.text = show_value(*value, spec);
            edit.error = None;
            edit.editing = true;
        }
    }
    if !focused && !edit.editing {
        edit.text = show_value(*value, spec);
    }
    // Frame first, so the text edit draws on top of it.
    paint(ui, rect, spec.unit, focused, edit.error.is_some(), &t);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let response = child.add(
        TextEdit::singleline(&mut edit.text)
            .id(id)
            .font(Role::Mono.font())
            .frame(egui::Frame::NONE)
            .desired_width(text_rect.width())
            // The whole field is the target, not just the line of digits.
            .min_size(Vec2::new(text_rect.width(), text_rect.height()))
            .vertical_align(egui::Align::Center)
            .text_color(t.text)
            .margin(egui::Margin::ZERO),
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::DragValue, true, name));
    ui.ctx().accesskit_node_builder(id, |node| {
        node.set_role(egui::accesskit::Role::SpinButton);
        node.set_label(name);
    });
    if response.gained_focus() {
        edit.editing = true;
    }
    if response.changed() {
        edit.error = None;
    }
    let escape = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape));
    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    let committed = if escape {
        edit.text = show_value(*value, spec);
        edit.error = None;
        edit.editing = false;
        false
    } else if enter || (response.lost_focus() && edit.editing) {
        match check(&edit.text, spec) {
            Ok(v) => {
                let changed = (v - *value).abs() > f64::EPSILON;
                *value = v;
                edit.text = show_value(v, spec);
                edit.error = None;
                edit.editing = false;
                changed
            }
            Err(message) if enter => {
                edit.error = Some(message);
                ui.memory_mut(|m| m.request_focus(id));
                false
            }
            Err(_) => {
                edit.text = show_value(*value, spec);
                edit.editing = false;
                false
            }
        }
    } else {
        false
    };
    if committed || stepped {
        outer.mark_changed();
    }
    if response.has_focus() && !focused {
        focus::paint_ring(ui.painter(), rect, theme::RADIUS_CONTROL, &t);
    }
    if let Some(message) = edit.error.clone() {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(ICON_SIZE), Sense::hover());
            (icons::ERROR.0)(ui.painter(), r, t.red);
            ui.label(fonts::caption(message).color(t.red));
        });
    }
    ui.data_mut(|d| d.insert_temp(id, edit));
    outer = outer.union(response.clone());
    outer.on_hover_text(tip)
}

fn round_to(value: f64, places: usize) -> f64 {
    let p = 10f64.powi(i32::try_from(places).unwrap_or(6));
    (value * p).round() / p
}

fn paint(ui: &Ui, rect: egui::Rect, unit: &str, focused: bool, error: bool, t: &Tokens) {
    let radius = CornerRadius::same(theme::RADIUS_CONTROL);
    let painter = ui.painter();
    painter.rect_filled(rect, radius, t.raised);
    let border = if error {
        t.red
    } else if focused {
        t.text
    } else {
        t.edge
    };
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(theme::BORDER, border),
        StrokeKind::Inside,
    );
    if !unit.is_empty() {
        let galley = painter.layout_no_wrap(unit.to_owned(), Role::Mono.font(), t.muted);
        painter.galley(
            pos2(
                rect.max.x - 8.0 - galley.size().x,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            t.muted,
        );
    }
    if focused {
        focus::paint_ring(painter, rect, theme::RADIUS_CONTROL, t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame, key};

    /// The box paints its value, and the value is visible (not clipped away).
    #[test]
    fn the_box_paints_its_value() {
        let ctx = context(true);
        let mut value = 2.0;
        let spec = NumberSpec::default();
        let mut found = Vec::new();
        for _ in 0..2 {
            let out = frame(&ctx, vec![], |ui| {
                number_field(ui, "x", &mut value, &spec, "X position", "Move along X");
            });
            found.clear();
            for shape in &out.shapes {
                if let egui::Shape::Text(text) = &shape.shape {
                    let c = text.pos + text.galley.size() / 2.0;
                    found.push((
                        text.galley.text().to_owned(),
                        shape.clip_rect.contains(c),
                        text.fallback_color,
                        text.override_text_color,
                    ));
                }
            }
        }
        assert!(
            found
                .iter()
                .any(|(t, visible, _, _)| t == "2.00" && *visible),
            "{found:?}"
        );
    }

    /// The value is painted after the box fill, so the fill never hides it.
    #[test]
    fn the_value_is_painted_over_the_box_fill() -> Result<(), String> {
        let ctx = context(true);
        let mut value = 44.0;
        let spec = NumberSpec {
            width: 112.0,
            step: 0.5,
            ..NumberSpec::default()
        };
        let mut boxed = egui::Rect::NOTHING;
        let mut text_at = None;
        let mut fill_at = None;
        for _ in 0..3 {
            let out = frame(&ctx, vec![], |ui| {
                egui::Frame::new()
                    .fill(Color32::BLACK)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        crate::ui_kit::form_row::form_row(
                            ui,
                            crate::ui_kit::form_row::FormWidth::Aside,
                            "X",
                            None,
                            None,
                            |ui| {
                                boxed = number_field(ui, "x", &mut value, &spec, "X", "Move").rect;
                            },
                        );
                    });
            });
            text_at = None;
            fill_at = None;
            for (i, shape) in out.shapes.iter().enumerate() {
                match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "44.00" => text_at = Some(i),
                    egui::Shape::Rect(r) if r.rect == boxed && r.fill == Tokens::DARK.raised => {
                        fill_at = Some(i);
                    }
                    _ => {}
                }
            }
        }
        let (Some(text), Some(fill)) = (text_at, fill_at) else {
            return Err(format!(
                "missing text {text_at:?} or fill {fill_at:?} at {boxed:?}"
            ));
        };
        assert!(text > fill, "text {text} is under the fill {fill}");
        Ok(())
    }

    /// Typed units convert within a kind and refuse across kinds.
    #[test]
    fn parsing_reads_units() {
        assert_eq!(parse_with_unit("0.25", "mm"), Ok(0.25));
        assert_eq!(parse_with_unit("0.25 mm", "mm"), Ok(0.25));
        assert_eq!(parse_with_unit("0,25mm", "mm"), Ok(0.25));
        assert_eq!(parse_with_unit("1 in", "mm"), Ok(25.4));
        assert_eq!(parse_with_unit("10 mil", "mm"), Ok(0.254));
        assert_eq!(
            parse_with_unit("25.4 mm", "in").map(|v| (v * 1000.0).round()),
            Ok(1000.0)
        );
        assert_eq!(parse_with_unit("90 deg", "deg"), Ok(90.0));
        assert!(parse_with_unit("3 V", "mm").is_err());
        assert!(parse_with_unit("abc", "mm").is_err());
        assert!(parse_with_unit("5 mil", "deg").is_err());
        assert_eq!(parse_with_unit("-1.5", "deg"), Ok(-1.5));
    }

    /// Values outside the range or not finite are refused.
    #[test]
    fn range_is_enforced() {
        let spec = NumberSpec {
            min: 0.0,
            max: 10.0,
            ..NumberSpec::default()
        };
        assert_eq!(check("5", &spec), Ok(5.0));
        assert!(check("11", &spec).is_err());
        assert!(check("-1", &spec).is_err());
        assert!(check("inf", &spec).is_err());
    }

    fn focus_and_settle(ctx: &egui::Context, value: &mut f64, spec: &NumberSpec<'_>) {
        let id = Id::new("x");
        ctx.memory_mut(|m| m.request_focus(id));
        for _ in 0..3 {
            frame(ctx, vec![], |ui| {
                number_field(ui, "x", value, spec, "X position", "Move along X");
            });
        }
    }

    /// Arrow up and down step by the grid; Shift steps ten times; the range clamps.
    #[test]
    fn arrow_keys_step_by_the_grid() {
        let ctx = context(true);
        let spec = NumberSpec {
            step: 0.5,
            min: 0.0,
            max: 10.0,
            ..NumberSpec::default()
        };
        let mut value = 2.0;
        focus_and_settle(&ctx, &mut value, &spec);
        frame(&ctx, key(Key::ArrowUp), |ui| {
            number_field(ui, "x", &mut value, &spec, "X position", "Move along X");
        });
        assert_eq!(value, 2.5);
        frame(&ctx, key(Key::ArrowDown), |ui| {
            number_field(ui, "x", &mut value, &spec, "X position", "Move along X");
        });
        assert_eq!(value, 2.0);
        let mut shifted = key(Key::ArrowUp);
        for e in &mut shifted {
            if let egui::Event::Key { modifiers, .. } = e {
                *modifiers = Modifiers::SHIFT;
            }
        }
        frame(&ctx, shifted, |ui| {
            number_field(ui, "x", &mut value, &spec, "X position", "Move along X");
        });
        assert_eq!(value, 7.0);
        for _ in 0..3 {
            let mut up = key(Key::ArrowUp);
            for e in &mut up {
                if let egui::Event::Key { modifiers, .. } = e {
                    *modifiers = Modifiers::SHIFT;
                }
            }
            frame(&ctx, up, |ui| {
                number_field(ui, "x", &mut value, &spec, "X position", "Move along X");
            });
        }
        assert_eq!(value, 10.0, "clamped at the maximum");
    }

    /// Escape puts the shown text back to the stored value.
    #[test]
    fn escape_reverts_the_text() {
        let ctx = context(true);
        let spec = NumberSpec::default();
        let mut value = 1.0;
        focus_and_settle(&ctx, &mut value, &spec);
        frame(&ctx, vec![egui::Event::Text("9".into())], |ui| {
            number_field(ui, "x", &mut value, &spec, "X position", "Move along X");
        });
        frame(&ctx, key(Key::Escape), |ui| {
            number_field(ui, "x", &mut value, &spec, "X position", "Move along X");
        });
        frame(&ctx, vec![], |ui| {
            number_field(ui, "x", &mut value, &spec, "X position", "Move along X");
        });
        assert_eq!(value, 1.0);
        let edit: Edit = ctx.data(|d| d.get_temp(Id::new("x"))).unwrap_or_default();
        assert_eq!(edit.text, "1.00");
    }
}
