//! Board corner defaults and chamfered outline helpers.
//!
//! The board outline keeps its own corner size separate from routed track
//! corners. This module supplies the default leg for new boards and builds or
//! recognises the standard 45 degree chamfered rectangle that the UI shows by
//! default.
//!
//! # Rules
//! - The default leg is three times the track width, clamped to 0.3 to 5.0 mm.
//! - A chamfered outline has eight vertices: four corners, four chamfer ends.
//! - The chamfer leg cannot exceed half the smaller board dimension.
//!
//! # Dependencies
//! - crates/geom/src/lib.rs#Point

use pcbcraft_geom::Point;

/// Default chamfer leg for a new board, in millimetres.
///
/// Three times the track width, clamped to the practical 0.3 to 5.0 mm range.
/// An invalid or missing track width (NaN) gives 0.75 mm, a safe middle value.
///
/// # Rules
/// - The result is always finite and in the range 0.3 to 5.0.
///
/// # Do not
/// - Do not use this for routed track corners; that is a separate rule.
pub fn default_corner_leg(track_width_mm: f64) -> f64 {
    if track_width_mm.is_nan() {
        return 0.75;
    }
    (3.0 * track_width_mm).clamp(0.3, 5.0)
}

/// Eight-vertex chamfered rectangle outline.
///
/// Vertices in order: (leg,0), (w-leg,0), (w,leg), (w,h-leg), (w-leg,h),
/// (leg,h), (0,h-leg), (0,leg). The leg is clamped to 0.0 to min(w,h)/2 - 0.01.
///
/// # Rules
/// - The result always has exactly eight vertices.
/// - The outline is closed: the last vertex connects back to the first.
///
/// # Do not
/// - Do not use this for track corners; that is a separate rule.
pub fn chamfered_outline(width: f64, height: f64, leg: f64) -> Vec<Point> {
    let max_leg = (width.min(height) / 2.0) - 0.01;
    let leg = leg.clamp(0.0, max_leg);
    let w = width;
    let h = height;
    vec![
        Point::new(leg, 0.0),
        Point::new(w - leg, 0.0),
        Point::new(w, leg),
        Point::new(w, h - leg),
        Point::new(w - leg, h),
        Point::new(leg, h),
        Point::new(0.0, h - leg),
        Point::new(0.0, leg),
    ]
}

/// Leg length if `outline` is a chamfered rectangle of the given size.
///
/// Returns `Some(leg)` when the outline matches exactly what
/// `chamfered_outline` produces for that width and height, within 1e-6 mm.
/// Returns `None` for any other shape, including a plain rectangle.
///
/// # Rules
/// - The tolerance is 1e-6 mm on each coordinate.
/// - The outline must have exactly eight vertices.
pub fn chamfer_leg_of(outline: &[Point], width: f64, height: f64) -> Option<f64> {
    if outline.len() != 8 {
        return None;
    }
    // The first chamfer vertex is at (leg, 0), so leg = outline[0].x.
    let leg = outline[0].x;
    let regen = chamfered_outline(width, height, leg);
    let mut ok = true;
    for (a, b) in outline.iter().zip(regen.iter()) {
        if (a.x - b.x).abs() > 1e-6 || (a.y - b.y).abs() > 1e-6 {
            ok = false;
            break;
        }
    }
    if ok { Some(leg) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_corner_leg_is_three_track_widths() {
        assert!((default_corner_leg(0.25) - 0.75).abs() < 1e-9);
        assert!((default_corner_leg(2.0) - 5.0).abs() < 1e-9);
        assert!((default_corner_leg(0.05) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn chamfered_outline_has_eight_vertices_and_round_trips() {
        let outline = chamfered_outline(60.0, 40.0, 1.5);
        assert_eq!(outline.len(), 8);
        assert!((outline[0].x - 1.5).abs() < 1e-9);
        assert!((outline[0].y - 0.0).abs() < 1e-9);
        assert!((outline[7].x - 0.0).abs() < 1e-9);
        assert!((outline[7].y - 1.5).abs() < 1e-9);
        assert_eq!(chamfer_leg_of(&outline, 60.0, 40.0), Some(1.5));
        let plain = vec![
            Point::new(0.0, 0.0),
            Point::new(60.0, 0.0),
            Point::new(60.0, 40.0),
            Point::new(0.0, 40.0),
        ];
        assert_eq!(chamfer_leg_of(&plain, 60.0, 40.0), None);
    }
}
