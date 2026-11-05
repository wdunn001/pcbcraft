//! Board totals: how many connections are still open and how far they reach.
//!
//! The checks command (engine) produces an `airwires` array with one entry per
//! unrouted connection, each carrying `from` and `to` pad coordinates. This
//! module sums those into a count and a total straight-line length. The Layout
//! status bar shows the result so a newcomer can see, at a glance, how much
//! of the board is still unrouted.
//!
//! The length is the straight line between pads, not the length of copper.
//! No engine call is made here; the checks data is already in `App::checks`.
//! A frame costs nothing.

use crate::t;

use serde_json::Value;

/// Count unrouted connections and sum their straight-line lengths.
///
/// # Rules
/// - Wires that fail to parse are skipped (counted as neither a connection
///   nor any length), so bad JSON never breaks the totals.
/// - Distance is the straight line between pad centres, not routed copper.
///
/// # Dependencies
/// - `crates/geom/src/lib.rs` provides `Point::distance`.
///
/// # Do not
/// - Do not add rounding or unit conversion; callers format the output.
/// - Do not make an engine call here; callers supply the data.
pub(crate) fn layout_totals_from_wires(wires: &[Value]) -> (usize, f64) {
    let mut count = 0;
    let mut length = 0.0;
    for wire in wires {
        let from = match wire.get("from") {
            Some(v) => match serde_json::from_value::<pcbcraft_geom::Point>(v.clone()) {
                Ok(p) => p,
                Err(_) => continue,
            },
            None => continue,
        };
        let to = match wire.get("to") {
            Some(v) => match serde_json::from_value::<pcbcraft_geom::Point>(v.clone()) {
                Ok(p) => p,
                Err(_) => continue,
            },
            None => continue,
        };
        count += 1;
        length += from.distance(to);
    }
    (count, length)
}

impl crate::App {
    /// "N unrouted, M mm" for the Layout status bar.
    ///
    /// Reads the cached checks data from `self.checks` and formats the
    /// connection totals. No engine call is made; a frame costs nothing.
    ///
    /// # Rules
    /// - If the airwires array is missing or empty the result is "0 unrouted, 0.0 mm".
    /// - Length is shown to one decimal place, in millimetres.
    ///
    /// The helper `layout_totals_from_wires` in this file does the summing.
    ///
    /// # Do not
    /// - Do not make an engine call here; callers refresh checks themselves.
    /// - Do not round the count; it is always a whole number.
    pub(crate) fn layout_totals_text(&self) -> String {
        let (count, length) = match self.checks.get("airwires").and_then(Value::as_array) {
            Some(wires) => layout_totals_from_wires(wires),
            None => (0, 0.0),
        };
        t!(
            "board.footer.unrouted",
            count = count,
            mm = format!("{:.1}", length)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn layout_totals_from_wires_sums_count_and_length() {
        // Two wires: (0,0)-(3,0) = 3.0 mm, (0,0)-(0,4) = 4.0 mm. Total 7.0 mm.
        let wires = vec![
            json!({"from": {"x": 0.0, "y": 0.0}, "to": {"x": 3.0, "y": 0.0}}),
            json!({"from": {"x": 0.0, "y": 0.0}, "to": {"x": 0.0, "y": 4.0}}),
        ];
        let (count, length) = layout_totals_from_wires(&wires);
        assert_eq!(count, 2);
        assert!((length - 7.0).abs() < f64::EPSILON);
    }

    #[test]
    fn layout_totals_from_wires_empty() {
        let (count, length) = layout_totals_from_wires(&[]);
        assert_eq!(count, 0);
        assert_eq!(length, 0.0);
    }
}
