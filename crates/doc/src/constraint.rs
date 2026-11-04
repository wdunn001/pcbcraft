//! Pure placement maths for named distance constraints.
//!
//! A constraint links two objects so that moving the driver keeps a named
//! distance to the driven object. This module supplies the value types and the
//! target-position calculation only; it does not store constraints or modify
//! the project. Constraint storage and the engine commands that evaluate them
//! land in later tickets.
//!
//! # Rules
//! - DistanceX keeps the driven object at value millimetres to the right of the
//!   driver, holding the driven object's own y coordinate.
//! - DistanceY keeps the driven object at value millimetres above the driver,
//!   holding the driven object's own x coordinate.
//! - Distance keeps the full distance at value millimetres along the current
//!   driver-to-driven line; when the two objects sit on top of each other it
//!   points along +x.
//! - ConstraintValue can be a bare number or a project parameter name.
//!
//! # Dependencies
//! - crates/geom/src/lib.rs Point type for all position calculations.

use crate::Id;
use pcbcraft_geom::Point;
use serde::{Deserialize, Serialize};

/// How the driven object stays related to the driver.
///
/// # Rules
/// - DistanceX keeps the driven x at driver x plus the value.
/// - DistanceY keeps the driven y at driver y plus the value.
/// - Distance keeps the full distance along the current line.
///
/// # Do not
/// - Do not add new kinds without updating the driven_target function.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    /// Horizontal gap: driven x equals driver x plus the value.
    DistanceX,
    /// Vertical gap: driven y equals driver y plus the value.
    DistanceY,
    /// Full distance along the current line between the two objects.
    Distance,
}

/// A numeric value or a named project parameter.
///
/// # Rules
/// - Number carries a bare f64 in millimetres.
/// - Param carries the name of a project parameter to resolve.
///
/// # Dependencies
/// - crates/doc/src/lib.rs Parameter type for resolved values.
///
/// # Do not
/// - Do not add expression support; that lands in a later ticket.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConstraintValue {
    /// A bare numeric value in millimetres.
    Number(f64),
    /// A project parameter name that resolves to a value.
    Param {
        #[serde(rename = "param")]
        /// The name of the project parameter to resolve.
        name: String,
    },
}

/// One named distance between a driver and a driven object.
///
/// # Rules
/// - The driver object moves freely.
/// - The driven object follows to keep the named distance.
/// - The value can be a number or a parameter name.
///
/// # Dependencies
/// - crates/doc/src/lib.rs Id type for object references.
///
/// # Do not
/// - Do not store constraints here; this is the data type only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    /// The kind of distance relationship.
    pub kind: ConstraintKind,
    /// The object that moves freely.
    pub driver: Id,
    /// The object that follows to keep the distance.
    pub driven: Id,
    /// The distance or the parameter that names it.
    pub value: ConstraintValue,
}

/// Where the driven object should sit to satisfy the constraint.
///
/// # Rules
/// - The result is always finite.
/// - When the driver and driven coincide on a Distance constraint the driven
///   object is placed value millimetres along +x from the driver.
///
/// # Dependencies
/// - crates/geom/src/lib.rs Point type for position arithmetic.
///
/// # Do not
/// - Do not use this to move objects; it only computes the target position.
pub fn driven_target(kind: ConstraintKind, driver: Point, driven: Point, value: f64) -> Point {
    match kind {
        ConstraintKind::DistanceX => Point::new(driver.x + value, driven.y),
        ConstraintKind::DistanceY => Point::new(driven.x, driver.y + value),
        ConstraintKind::Distance => {
            let dx = driven.x - driver.x;
            let dy = driven.y - driver.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 1e-12 {
                Point::new(driver.x + value, driver.y)
            } else {
                let scale = value / dist;
                Point::new(driver.x + dx * scale, driver.y + dy * scale)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constraint_places_the_driven_point() {
        // DistanceX: driven x = driver x + value, y unchanged.
        let t = driven_target(
            ConstraintKind::DistanceX,
            Point::new(10.0, 10.0),
            Point::new(0.0, 7.0),
            3.2,
        );
        assert!((t.x - 13.2).abs() < 1e-9);
        assert!((t.y - 7.0).abs() < 1e-9);

        // DistanceY: driven y = driver y + value, x unchanged.
        let t = driven_target(
            ConstraintKind::DistanceY,
            Point::new(10.0, 10.0),
            Point::new(4.0, 0.0),
            -2.0,
        );
        assert!((t.x - 4.0).abs() < 1e-9);
        assert!((t.y - 8.0).abs() < 1e-9);

        // Distance: keep direction, set length to value.
        // Driver (0,0), driven (6,8) -> length 10, direction (0.6, 0.8).
        // New length 5 -> (3, 4).
        let t = driven_target(
            ConstraintKind::Distance,
            Point::new(0.0, 0.0),
            Point::new(6.0, 8.0),
            5.0,
        );
        assert!((t.x - 3.0).abs() < 1e-9);
        assert!((t.y - 4.0).abs() < 1e-9);

        // Distance with coincident points: fall back to +x.
        let t = driven_target(
            ConstraintKind::Distance,
            Point::new(2.0, 2.0),
            Point::new(2.0, 2.0),
            4.0,
        );
        assert!((t.x - 6.0).abs() < 1e-9);
        assert!((t.y - 2.0).abs() < 1e-9);
    }

    #[test]
    fn constraint_value_reads_a_number_or_a_param() {
        let num =
            serde_json::from_str::<ConstraintValue>("3.2").unwrap_or(ConstraintValue::Number(0.0));
        let num_is_number = matches!(num, ConstraintValue::Number(_));
        let num_value = match num {
            ConstraintValue::Number(v) => v,
            ConstraintValue::Param { .. } => 0.0,
        };
        assert!(num_is_number);
        assert!((num_value - 3.2).abs() < 1e-9);

        let param = serde_json::from_str::<ConstraintValue>(r#"{"param":"pitch"}"#)
            .unwrap_or(ConstraintValue::Number(0.0));
        let param_is_param = matches!(param, ConstraintValue::Param { .. });
        let param_name = match param {
            ConstraintValue::Param { name } => name,
            ConstraintValue::Number(_) => String::new(),
        };
        assert!(param_is_param);
        assert_eq!(param_name, "pitch");
    }
}
