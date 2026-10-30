//! Geometry for filleting the corner between two straight track legs.
//!
//! Pure millimetre math shared by rounding commands and export flattening.
//! See issue #911 and the `track.round_corners` command (#912).

use crate::{Arc, Point, arc_through, positive};

/// Default corner radius as a multiple of track width when no radius is given.
pub const DEFAULT_CORNER_RADIUS_WIDTH_FACTOR: f64 = 3.0;

/// Tangent points and arc midpoint after replacing a sharp corner with a circular arc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackCornerFillet {
    /// Where the first straight leg ends before the arc.
    pub tangent_a: Point,
    /// Where the second straight leg begins after the arc.
    pub tangent_b: Point,
    /// Midpoint on the circular arc, in the same form track geometry stores.
    pub arc_mid: Point,
    /// Radius of the fillet arc, in millimetres.
    pub radius: f64,
}

/// Resolve a corner radius in millimetres from an explicit request, a net-class
/// default, or three times the track width.
///
/// A non-positive `requested_mm` means "use class default, then width factor".
pub fn effective_corner_radius(
    requested_mm: f64,
    track_width_mm: f64,
    class_default_mm: Option<f64>,
) -> Option<f64> {
    if requested_mm.is_finite() && requested_mm > 0.0 {
        return Some(requested_mm);
    }
    if let Some(r) = class_default_mm.filter(|r| r.is_finite() && *r > 0.0) {
        return Some(r);
    }
    if positive(track_width_mm) {
        Some(track_width_mm * DEFAULT_CORNER_RADIUS_WIDTH_FACTOR)
    } else {
        None
    }
}

fn unit(from: Point, to: Point) -> Option<Point> {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let len = dx.hypot(dy);
    if !len.is_finite() || len < 1e-9 {
        return None;
    }
    Some(Point::new(dx / len, dy / len))
}

/// Replace the corner at `joint` between legs `leg_a` and `leg_b` with a circular
/// arc of `radius` millimetres.
///
/// `leg_a` and `leg_b` lie on the two straight segments; `joint` is their common
/// endpoint. Tangent length is capped at half the shorter leg so neighbouring
/// corners cannot collide; the returned [`TrackCornerFillet::radius`] reflects
/// any reduction.
pub fn track_corner_fillet(
    leg_a: Point,
    joint: Point,
    leg_b: Point,
    radius: f64,
) -> Option<TrackCornerFillet> {
    if !positive(radius) {
        return None;
    }
    let dir_a = unit(joint, leg_a)?;
    let dir_b = unit(joint, leg_b)?;
    let cos_turn = (dir_a.x * dir_b.x + dir_a.y * dir_b.y).clamp(-1.0, 1.0);
    let angle = cos_turn.acos();
    if !angle.is_finite() || angle < 1e-6 || angle > std::f64::consts::PI - 1e-6 {
        return None;
    }
    let half = angle / 2.0;
    let tan_half = half.tan();
    if !tan_half.is_finite() || tan_half.abs() < 1e-12 {
        return None;
    }
    let mut t = radius / tan_half;
    let len_a = joint.distance(leg_a);
    let len_b = joint.distance(leg_b);
    let max_t = len_a.min(len_b) * 0.5;
    let effective_radius = if t > max_t {
        t = max_t;
        t * tan_half
    } else {
        radius
    };
    if !positive(effective_radius) || t < 1e-9 {
        return None;
    }
    let tangent_a = joint.offset(Point::new(dir_a.x * t, dir_a.y * t));
    let tangent_b = joint.offset(Point::new(dir_b.x * t, dir_b.y * t));
    let bis_x = dir_a.x + dir_b.x;
    let bis_y = dir_a.y + dir_b.y;
    let bis_len = bis_x.hypot(bis_y);
    if bis_len < 1e-9 {
        return None;
    }
    let sin_half = half.sin();
    if sin_half.abs() < 1e-12 {
        return None;
    }
    let center_dist = effective_radius / sin_half;
    let center = joint.offset(Point::new(
        bis_x / bis_len * center_dist,
        bis_y / bis_len * center_dist,
    ));
    let to_a_x = tangent_a.x - center.x;
    let to_a_y = tangent_a.y - center.y;
    let to_b_x = tangent_b.x - center.x;
    let to_b_y = tangent_b.y - center.y;
    let cross = to_a_x * to_b_y - to_a_y * to_b_x;
    let dot = to_a_x * to_b_x + to_a_y * to_b_y;
    let sweep = cross.atan2(dot);
    if !sweep.is_finite() || sweep.abs() < 1e-9 {
        return None;
    }
    let mid_angle = (to_a_y.atan2(to_a_x) + sweep * 0.5);
    let arc_mid = Point::new(
        center.x + effective_radius * mid_angle.cos(),
        center.y + effective_radius * mid_angle.sin(),
    );
    arc_through(tangent_a, arc_mid, tangent_b)?;
    Some(TrackCornerFillet {
        tangent_a,
        tangent_b,
        arc_mid,
        radius: effective_radius,
    })
}

const MAX_ARC_CHORDS: usize = 64;

/// Sample points along `arc` so no chord deviates more than `max_error` millimetres.
pub fn flatten_arc(arc: &Arc, max_error: f64) -> Vec<Point> {
    let e = max_error.max(1e-6);
    let n = arc_chord_count(arc, e);
    let mut points = Vec::with_capacity(n + 1);
    for k in 0..=n {
        points.push(arc.point_at(k as f64 / n as f64));
    }
    points
}

/// Chords needed so no chord on `arc` strays more than `max_error` from the true arc.
pub fn arc_chord_count(arc: &Arc, max_error: f64) -> usize {
    let e = max_error.max(1e-6);
    if e >= arc.radius {
        return 1;
    }
    let step = 2.0 * (1.0 - e / arc.radius).acos();
    if !step.is_finite() || step <= 0.0 {
        return MAX_ARC_CHORDS;
    }
    ((arc.sweep.abs() / step).ceil() as usize)
        .clamp(1, MAX_ARC_CHORDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_radius_prefers_explicit_then_class_then_width_factor() {
        assert_eq!(
            effective_corner_radius(1.2, 0.25, Some(0.9)),
            Some(1.2)
        );
        assert_eq!(
            effective_corner_radius(0.0, 0.25, Some(0.9)),
            Some(0.9)
        );
        assert_eq!(
            effective_corner_radius(-1.0, 0.25, None),
            Some(0.75)
        );
    }

    #[test]
    fn right_angle_fillet_is_a_quarter_circle() {
        let joint = Point::new(0.0, 0.0);
        let leg_a = Point::new(-4.0, 0.0);
        let leg_b = Point::new(0.0, 4.0);
        let fillet = track_corner_fillet(leg_a, joint, leg_b, 1.0).expect("fillet");
        assert!((fillet.radius - 1.0).abs() < 1e-9);
        assert!((fillet.tangent_a.x + 1.0).abs() < 1e-9);
        assert!((fillet.tangent_b.y - 1.0).abs() < 1e-9);
        let arc = arc_through(fillet.tangent_a, fillet.arc_mid, fillet.tangent_b).expect("arc");
        assert!((arc.radius - 1.0).abs() < 1e-9);
        assert!((arc.length() - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn short_legs_cap_the_fillet_radius() {
        let joint = Point::new(0.0, 0.0);
        let leg_a = Point::new(-0.4, 0.0);
        let leg_b = Point::new(0.0, 0.4);
        let fillet = track_corner_fillet(leg_a, joint, leg_b, 1.0).expect("fillet");
        assert!(fillet.radius < 1.0);
        assert!((fillet.tangent_a.x + 0.2).abs() < 1e-9);
    }
}
