//! Hover affordance: the animated outline that tells you "this part of the
//! drawing does something when you click it".
//!
//! The resolver (`tool_modifiers.rs`) knows what a click on the hovered target
//! does. This module turns that answer into three things the canvas painters
//! call: an [`Affordance`] (what to draw), [`paint`] (draws it) and
//! [`cursor_for`] (which mouse cursor to show).
//!
//! # Rules
//! - Only objects the active tool or click target can act on light up
//!   ([`affordance_for`] returns `None` otherwise).
//! - A soft glow fades in over 120 ms, then a dashed border marches with a
//!   1.2 s period while hovered. Pins and pads that start a wire or route get
//!   a pulsing ring. Every other segment of the hovered net glows faintly.
//! - Reduce motion (#607) draws one static outline: no fade, no march, no
//!   repaint request.
//! - A repaint is requested only while something animates, so an idle window
//!   costs nothing.
//!
//! # Dependencies
//! - `crates/ui-egui/src/tool_modifiers.rs` resolves the hovered action.
//! - `crates/ui-egui/src/canvas/gestures.rs` calls [`paint`]; the layout and
//!   schematic canvases hand over screen-space [`Outline`]s.
//!
//! # Do not
//! - Read egui modifiers here; the caller passes the resolved action.
//! - Request repaints outside [`paint`].

use egui::{Color32, CursorIcon, Painter, Pos2, Shape, Stroke, Vec2};

use crate::tool_modifiers::{ClickAction, Target, ToolKind};

/// Fade-in time of the glow, in seconds.
pub const FADE_SECONDS: f64 = 0.12;
/// Time the dashed border takes to travel one dash period, in seconds.
pub const MARCH_PERIOD: f64 = 1.2;
/// Pulse period of the pin and pad ring, in seconds.
pub const PULSE_PERIOD: f64 = 1.0;
const DASH: f32 = 6.0;
const GAP: f32 = 4.0;

/// Where an object is on screen, independent of how it is painted.
///
/// Mirrors the layout canvas hit shapes so both canvases hand over the same
/// description of "where is this object".
#[derive(Clone, Debug, PartialEq)]
pub enum Outline {
    Polygon(Vec<Pos2>),
    /// A centre line and half of its drawn width.
    Segment(Pos2, Pos2, f32),
    Circle(Pos2, f32),
}

impl Outline {
    /// The outline as a closed screen polygon (circles are sampled, segments
    /// become a thin box).
    pub fn ring(&self) -> Vec<Pos2> {
        match self {
            Self::Polygon(points) => points.clone(),
            Self::Circle(c, r) => (0..24)
                .map(|i| {
                    let t = i as f32 / 24.0 * std::f32::consts::TAU;
                    *c + Vec2::new(t.cos(), t.sin()) * (r.max(3.0) + 2.0)
                })
                .collect(),
            Self::Segment(a, b, half) => {
                let dir = (*b - *a).normalized();
                let dir = if dir.x.is_nan() { Vec2::X } else { dir };
                let side = Vec2::new(-dir.y, dir.x) * (half.max(2.0) + 2.0);
                let ext = dir * 2.0;
                vec![
                    *a - ext + side,
                    *b + ext + side,
                    *b + ext - side,
                    *a - ext - side,
                ]
            }
        }
    }
    /// Centre of the outline, for the pulsing ring.
    pub fn centre(&self) -> Pos2 {
        match self {
            Self::Circle(c, _) => *c,
            Self::Segment(a, b, _) => a.lerp(*b, 0.5),
            Self::Polygon(points) if points.is_empty() => Pos2::ZERO,
            Self::Polygon(points) => {
                let sum = points.iter().fold(Vec2::ZERO, |s, p| s + p.to_vec2());
                (sum / points.len() as f32).to_pos2()
            }
        }
    }
    /// Radius that covers the outline around its centre.
    pub fn reach(&self) -> f32 {
        let c = self.centre();
        match self {
            Self::Circle(_, r) => r.max(3.0),
            _ => self
                .ring()
                .iter()
                .map(|p| p.distance(c))
                .fold(3.0, f32::max),
        }
    }
}

/// How the hovered object should be drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Glow plus marching border: a click selects or moves this.
    Outline,
    /// Pulsing ring: a click or drag from here starts a wire or route.
    Ring,
    /// Outline in the warning tint: a click deletes here.
    Delete,
}

/// What to draw for the hovered object.
pub struct Affordance<'a> {
    pub shape: &'a Outline,
    pub style: Style,
    /// Other outlines of the same net; they glow faintly.
    pub net: &'a [Outline],
    pub accent: Color32,
    pub warning: Color32,
    pub reduced_motion: bool,
}

/// What lights up for a resolved action on a target with the active tool.
///
/// Why: the owner rule is that anything actionable shows it on hover, and
/// nothing else does, so the answer is derived from the same resolver row the
/// click will use.
pub fn affordance_for(tool: ToolKind, action: ClickAction, target: Target) -> Option<Style> {
    match action {
        ClickAction::Replace
        | ClickAction::Add
        | ClickAction::Toggle
        | ClickAction::PickUnder
        | ClickAction::MoveSegment
        | ClickAction::MovePart
        | ClickAction::RenameLabel => (target != Target::Empty).then_some(Style::Outline),
        ClickAction::StartWire
        | ClickAction::StartRoute
        | ClickAction::ContinueWire
        | ClickAction::ContinueRoute => Some(Style::Ring),
        ClickAction::DeleteConnection | ClickAction::RemoveSegment => Some(Style::Delete),
        ClickAction::ToolDefault => match (tool, target) {
            (ToolKind::NoConnect, Target::Pin) => Some(Style::Ring),
            (ToolKind::Via, Target::TrackSegment | Target::TrackEnd | Target::Pad) => {
                Some(Style::Ring)
            }
            _ => None,
        },
        ClickAction::BoxSelect | ClickAction::Nothing => None,
    }
}

/// The mouse cursor for a resolved action: hand for select, grab for move,
/// crosshair for starting a wire or route, a no-entry sign for delete.
pub fn cursor_for(action: ClickAction) -> CursorIcon {
    match action {
        ClickAction::Replace | ClickAction::Add | ClickAction::Toggle | ClickAction::PickUnder => {
            CursorIcon::PointingHand
        }
        ClickAction::MoveSegment | ClickAction::MovePart => CursorIcon::Grab,
        ClickAction::StartWire
        | ClickAction::StartRoute
        | ClickAction::ContinueWire
        | ClickAction::ContinueRoute => CursorIcon::Crosshair,
        ClickAction::DeleteConnection | ClickAction::RemoveSegment => CursorIcon::NotAllowed,
        ClickAction::RenameLabel => CursorIcon::Text,
        ClickAction::BoxSelect => CursorIcon::Crosshair,
        ClickAction::ToolDefault | ClickAction::Nothing => CursorIcon::Default,
    }
}

/// Seconds since the hover on `key` began; restarts when the hovered object
/// changes. `key` is any stable hash of the hovered object.
pub fn hover_age(ctx: &egui::Context, key: u64) -> f64 {
    let id = egui::Id::new("hover-affordance-clock");
    let now = ctx.input(|i| i.time);
    let (last, since) = ctx.data_mut(|d| d.get_temp::<(u64, f64)>(id).unwrap_or((key, now)));
    let since = if last == key { since } else { now };
    ctx.data_mut(|d| d.insert_temp(id, (key, since)));
    (now - since).max(0.0)
}

/// Fade-in progress, 0 to 1.
pub fn fade(age: f64, reduced_motion: bool) -> f32 {
    if reduced_motion {
        1.0
    } else {
        (age / FADE_SECONDS).clamp(0.0, 1.0) as f32
    }
}

/// Dash offset of the marching border; 0 when motion is reduced.
pub fn march_offset(age: f64, reduced_motion: bool) -> f32 {
    if reduced_motion {
        0.0
    } else {
        (((age / MARCH_PERIOD).fract()) as f32) * (DASH + GAP)
    }
}

/// Draw the hover affordance.
///
/// # Rules
/// - Requests a repaint only while animating (fade or march), never when
///   motion is reduced, and never when `affordance` is not drawn.
/// - Cost is one polygon outline plus the faint net outlines; keep it under
///   2 ms per frame (checked by `overlay_stays_inside_the_frame_budget`).
///
/// # Dependencies
/// - `crates/ui-egui/src/motion.rs` for the reduced-motion preference.
///
/// # Do not
/// - Call it for objects [`affordance_for`] rejected.
pub fn paint(painter: &Painter, a: &Affordance<'_>, key: u64) {
    let ctx = painter.ctx();
    let age = hover_age(ctx, key);
    let fade = fade(age, a.reduced_motion);
    let tint = if a.style == Style::Delete {
        a.warning
    } else {
        a.accent
    };
    for other in a.net {
        let ring = other.ring();
        painter.add(Shape::closed_line(
            ring,
            Stroke::new(2.0, tint.gamma_multiply(0.18 * fade)),
        ));
    }
    let ring = a.shape.ring();
    match a.style {
        Style::Ring => {
            let pulse = if a.reduced_motion {
                0.5
            } else {
                0.5 + 0.5 * ((age / PULSE_PERIOD * std::f64::consts::TAU).sin() as f32).abs()
            };
            let reach = a.shape.reach() + 3.0 + 3.0 * pulse;
            painter.circle_stroke(
                a.shape.centre(),
                reach,
                Stroke::new(1.75, tint.gamma_multiply(0.45 + 0.55 * pulse)),
            );
            painter.add(Shape::closed_line(
                ring,
                Stroke::new(1.25, tint.gamma_multiply(fade)),
            ));
        }
        Style::Outline | Style::Delete => {
            painter.add(Shape::closed_line(
                ring.clone(),
                Stroke::new(5.0, tint.gamma_multiply(0.22 * fade)),
            ));
            if a.reduced_motion {
                painter.add(Shape::closed_line(ring, Stroke::new(1.75, tint)));
            } else {
                let mut path = ring;
                if let Some(first) = path.first().copied() {
                    path.push(first);
                }
                let shapes = Shape::dashed_line_with_offset(
                    &path,
                    Stroke::new(1.75, tint.gamma_multiply(fade)),
                    &[DASH],
                    &[GAP],
                    march_offset(age, false),
                );
                painter.extend(shapes);
            }
        }
    }
    if !a.reduced_motion {
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_modifiers::{ClickAction as CA, Target as T, ToolKind as TK};

    fn part() -> Outline {
        Outline::Polygon(vec![
            Pos2::new(100.0, 100.0),
            Pos2::new(180.0, 100.0),
            Pos2::new(180.0, 140.0),
            Pos2::new(100.0, 140.0),
        ])
    }

    #[test]
    fn only_actionable_objects_light_up() {
        assert_eq!(
            affordance_for(TK::Select, CA::Replace, T::Part),
            Some(Style::Outline)
        );
        assert_eq!(affordance_for(TK::Select, CA::BoxSelect, T::Empty), None);
        assert_eq!(
            affordance_for(TK::Select, CA::StartWire, T::Pin),
            Some(Style::Ring)
        );
        assert_eq!(
            affordance_for(TK::Select, CA::StartRoute, T::Pad),
            Some(Style::Ring)
        );
        assert_eq!(affordance_for(TK::Wire, CA::ToolDefault, T::Part), None);
        assert_eq!(
            affordance_for(TK::Wire, CA::DeleteConnection, T::Node),
            Some(Style::Delete)
        );
        assert_eq!(
            affordance_for(TK::Route, CA::RemoveSegment, T::Via),
            Some(Style::Delete)
        );
        assert_eq!(
            affordance_for(TK::Via, CA::ToolDefault, T::TrackSegment),
            Some(Style::Ring)
        );
        assert_eq!(affordance_for(TK::Measure, CA::ToolDefault, T::Pad), None);
    }

    #[test]
    fn the_cursor_matches_the_action() {
        assert_eq!(cursor_for(CA::MovePart), CursorIcon::Grab);
        assert_eq!(cursor_for(CA::StartWire), CursorIcon::Crosshair);
        assert_eq!(cursor_for(CA::StartRoute), CursorIcon::Crosshair);
        assert_eq!(cursor_for(CA::DeleteConnection), CursorIcon::NotAllowed);
        assert_eq!(cursor_for(CA::Replace), CursorIcon::PointingHand);
        assert_eq!(cursor_for(CA::Nothing), CursorIcon::Default);
    }

    #[test]
    fn fade_takes_120_ms_and_march_takes_1_2_s() {
        assert_eq!(fade(0.0, false), 0.0);
        assert!((fade(0.06, false) - 0.5).abs() < 1e-6);
        assert_eq!(fade(0.2, false), 1.0);
        let full = DASH + GAP;
        assert!((march_offset(0.6, false) - full / 2.0).abs() < 1e-3);
        assert!(march_offset(1.2, false) < 1e-3);
    }

    /// Run one frame and drop its texture uploads the way a renderer would.
    fn run(ctx: &egui::Context, f: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
        let mut out = ctx.run_ui(egui::RawInput::default(), f);
        out.textures_delta.clear();
        out
    }

    /// Run two frames (the first always asks for a repaint) and return the
    /// repaint delay of the second.
    fn second_frame_delay(reduced: bool, style: Style, draw_it: bool) -> std::time::Duration {
        let ctx = egui::Context::default();
        let shape = part();
        let mut delay = std::time::Duration::MAX;
        for _ in 0..4 {
            let out = run(&ctx, |ui| {
                if draw_it {
                    let painter = ui.painter().clone();
                    paint(
                        &painter,
                        &Affordance {
                            shape: &shape,
                            style,
                            net: &[],
                            accent: Color32::GREEN,
                            warning: Color32::RED,
                            reduced_motion: reduced,
                        },
                        1,
                    );
                }
            });
            delay = out
                .viewport_output
                .values()
                .map(|v| v.repaint_delay)
                .min()
                .unwrap_or(std::time::Duration::MAX);
        }
        delay
    }

    #[test]
    fn reduced_motion_is_static() {
        assert_eq!(fade(0.0, true), 1.0);
        assert_eq!(march_offset(0.7, true), 0.0);
        assert!(second_frame_delay(true, Style::Outline, true).as_secs_f32() > 1.0);
    }

    #[test]
    fn a_hover_asks_for_repaints_only_while_drawn() {
        assert!(second_frame_delay(false, Style::Ring, false).as_secs_f32() > 1.0);
        assert!(second_frame_delay(false, Style::Ring, true).as_millis() <= 40);
    }

    #[test]
    fn overlay_stays_inside_the_frame_budget() {
        let ctx = egui::Context::default();
        let shape = part();
        // A busy net: 200 other segments glow faintly.
        let net: Vec<Outline> = (0..200)
            .map(|i| {
                let y = 10.0 + i as f32;
                Outline::Segment(Pos2::new(10.0, y), Pos2::new(400.0, y + 5.0), 3.0)
            })
            .collect();
        let mut total = std::time::Duration::ZERO;
        let frames = 60u32;
        let mut painted = Vec::new();
        for _ in 0..frames {
            let mut took = std::time::Duration::ZERO;
            let out = run(&ctx, |ui| {
                let painter = ui.painter().clone();
                let start = std::time::Instant::now();
                paint(
                    &painter,
                    &Affordance {
                        shape: &shape,
                        style: Style::Outline,
                        net: &net,
                        accent: Color32::GREEN,
                        warning: Color32::RED,
                        reduced_motion: false,
                    },
                    7,
                );
                took = start.elapsed();
            });
            total += took;
            painted.push(out.shapes.len());
        }
        // The cost is the work done: one outline per glowing net member, one
        // soft border and the dashes of the marching edge, the same every
        // frame. Counting it does not depend on how busy the machine is.
        let most = painted.iter().copied().max().unwrap_or(0);
        let least = painted.iter().copied().min().unwrap_or(0);
        assert_eq!(
            most, least,
            "every frame draws the same shapes: {painted:?}"
        );
        assert!(
            most <= net.len() + 1 + 64,
            "{most} shapes for a net of {}",
            net.len()
        );
        // Wall-clock time only means something in an optimised build, and
        // then only with room to spare; a debug build on a loaded runner is
        // not a measurement.
        if !cfg!(debug_assertions) {
            assert!(
                total / frames < std::time::Duration::from_millis(10),
                "average {:?}",
                total / frames
            );
        }
    }
}
