//! Fillet geometry for two straight track legs meeting at one corner.
use crate::{Arc, Point, arc_through};

/// Tangent points and an arc midpoint for one rounded corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackFillet {
    pub tangent_a: Point,
    pub tangent_b: Point,
    pub arc_mid: Point,
    pub radius: f64,
}

fn unit(from: Point, to: Point) -> Option<Point> {
    let d = from.distance(to);
    if d < 1e-12 {
        return None;
    }
    Some(Point::new((to.x - from.x) / d, (to.y - from.y) / d))
}

/// Replace a sharp corner with a circular arc of the requested radius.
///
/// `corner` is where the legs meet. `away_a` and `away_b` lie further along
/// each leg. `leg_a` and `leg_b` are the leg lengths before any edit. The
/// tangent length is capped at half the shorter leg; the radius shrinks to
/// match when needed.
pub fn track_fillet(
    corner: Point,
    away_a: Point,
    away_b: Point,
    leg_a: f64,
    leg_b: f64,
    radius: f64,
) -> Option<TrackFillet> {
    if !(radius.is_finite() && radius > 0.0 && leg_a.is_finite() && leg_b.is_finite()) {
        return None;
    }
    let v1 = unit(corner, away_a)?;
    let v2 = unit(corner, away_b)?;
    let dot = (v1.x * v2.x + v1.y * v2.y).clamp(-1.0, 1.0);
    let angle = dot.acos();
    if angle < 1e-6 || angle > std::f64::consts::PI - 1e-6 {
        return None;
    }
    let half = angle / 2.0;
    let tan_half = half.tan();
    let sin_half = half.sin();
    if tan_half.abs() < 1e-12 || sin_half.abs() < 1e-12 {
        return None;
    }
    let cap = leg_a.min(leg_b) * 0.5;
    if cap < 1e-9 {
        return None;
    }
    let mut t = radius / tan_half;
    let mut r = radius;
    if t > cap {
        t = cap;
        r = t * tan_half;
    }
    if t < 1e-9 || r < 1e-9 {
        return None;
    }
    let p1 = Point::new(corner.x + v1.x * t, corner.y + v1.y * t);
    let p2 = Point::new(corner.x + v2.x * t, corner.y + v2.y * t);
    if !p1.valid() || !p2.valid() || p1.distance(p2) < 1e-6 {
        return None;
    }
    let bis_len = (v1.x + v2.x).hypot(v1.y + v2.y);
    if bis_len < 1e-12 {
        return None;
    }
    let bis = Point::new((v1.x + v2.x) / bis_len, (v1.y + v2.y) / bis_len);
    let center = Point::new(
        corner.x + bis.x * (r / sin_half),
        corner.y + bis.y * (r / sin_half),
    );
    let a0 = (p1.y - center.y).atan2(p1.x - center.x);
    let a1 = (p2.y - center.y).atan2(p2.x - center.x);
    let sweep = {
        let mut ccw = a1 - a0;
        while ccw <= 0.0 {
            ccw += 2.0 * std::f64::consts::PI;
        }
        while ccw > 2.0 * std::f64::consts::PI {
            ccw -= 2.0 * std::f64::consts::PI;
        }
        if ccw > std::f64::consts::PI {
            ccw -= 2.0 * std::f64::consts::PI;
        }
        ccw
    };
    let mid_angle = a0 + sweep / 2.0;
    let arc_mid = Point::new(
        center.x + r * mid_angle.cos(),
        center.y + r * mid_angle.sin(),
    );
    arc_through(p1, arc_mid, p2)?;
    Some(TrackFillet {
        tangent_a: p1,
        tangent_b: p2,
        arc_mid,
        radius: r,
    })
}

/// Sample points along an arc for chord-style output, including both ends.
pub fn arc_chord_points(arc: &Arc, max_error: f64, max_chords: usize) -> Vec<Point> {
    let e = max_error.max(1e-6);
    let n = if e >= arc.radius {
        1
    } else {
        let step = 2.0 * (1.0 - e / arc.radius).acos();
        if !step.is_finite() || step <= 0.0 {
            max_chords
        } else {
            ((arc.sweep.abs() / step).ceil() as usize).clamp(1, max_chords)
        }
    };
    (0..=n)
        .map(|k| arc.point_at(k as f64 / n as f64))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    #[test]
    fn a_right_angle_corner_is_a_quarter_circle_of_radius_r() -> Result<(), &'static str> {
        let corner = Point::new(10.0, 10.0);
        let away_a = Point::new(0.0, 10.0);
        let away_b = Point::new(10.0, 20.0);
        let Some(fillet) = track_fillet(corner, away_a, away_b, 10.0, 10.0, 2.0) else {
            return Err("fillet");
        };
        assert!((fillet.tangent_a.x - 8.0).abs() < 1e-9);
        assert!((fillet.tangent_a.y - 10.0).abs() < 1e-9);
        assert!((fillet.tangent_b.x - 10.0).abs() < 1e-9);
        assert!((fillet.tangent_b.y - 12.0).abs() < 1e-9);
        let Some(arc) = arc_through(fillet.tangent_a, fillet.arc_mid, fillet.tangent_b) else {
            return Err("arc");
        };
        assert!((arc.radius - 2.0).abs() < 1e-9);
        assert!((arc.sweep.abs() - FRAC_PI_2).abs() < 1e-9);
        assert!((arc.length() - PI / 2.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn short_legs_cap_the_radius() -> Result<(), &'static str> {
        let corner = Point::new(10.0, 10.0);
        let away_a = Point::new(9.0, 10.0);
        let away_b = Point::new(10.0, 11.0);
        let Some(fillet) = track_fillet(corner, away_a, away_b, 1.0, 1.0, 2.0) else {
            return Err("fillet");
        };
        assert!((fillet.radius - 0.5).abs() < 1e-9, "radius {}", fillet.radius);
        assert!((fillet.tangent_a.x - 9.5).abs() < 1e-9);
        assert!((fillet.tangent_b.y - 10.5).abs() < 1e-9);
        Ok(())
    }
}
