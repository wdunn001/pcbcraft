//! Pure placement maths and validation for named distance constraints.
//!
//! A constraint links two objects so that moving the driver keeps a named
//! distance to the driven object. This module supplies the value types, the
//! target-position calculation and the validation that a project's constraint
//! list is consistent.
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
//! - At most 256 constraints per project.
//! - Driver and driven must be different ids.
//! - Both driver and driven must be board holes or vias.
//! - A driven id appears at most once (a forest).
//! - No cycles in the driver chain.
//!
//! # Dependencies
//! - crates/geom/src/lib.rs Point type for all position calculations.
//! - crates/doc/src/lib.rs Project type and Id type.

use crate::Id;
use pcbcraft_geom::Point;
use serde::{Deserialize, Serialize};

/// Maximum number of constraints a project can record.
pub const MAX_CONSTRAINTS: usize = 256;

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

impl crate::Project {
    /// Check the constraint list: bounded count, distinct driver and driven
    /// ids, both ids naming board holes or vias, finite values within range,
    /// parameter names that exist, each driven id appearing at most once, and
    /// no cycles in the driver chain.
    ///
    /// # Rules
    /// - At most 256 constraints.
    /// - Driver and driven must be different.
    /// - Both must be holes or vias.
    /// - Values must be finite and within plus or minus 10000.
    /// - Param names must exist in the parameter table.
    /// - Each driven id appears at most once.
    /// - No cycles.
    ///
    /// # Dependencies
    /// - crates/doc/src/lib.rs Project type.
    ///
    /// # Do not
    /// - Do not call this from the hot path; it is for validation only.
    pub fn validate_constraints(&self) -> Result<(), String> {
        if self.constraints.len() > MAX_CONSTRAINTS {
            return Err("a project records at most 256 constraints".into());
        }
        let mut driven_seen = std::collections::BTreeSet::new();
        for c in &self.constraints {
            if c.driver == c.driven {
                return Err("a constraint driver and driven must be different ids".into());
            }
            let driver_ok = self.board.holes.iter().any(|h| h.id == c.driver)
                || self.board.vias.iter().any(|v| v.id == c.driver);
            let driven_ok = self.board.holes.iter().any(|h| h.id == c.driven)
                || self.board.vias.iter().any(|v| v.id == c.driven);
            if !driver_ok {
                return Err("a constraint driver must name a board hole or via".into());
            }
            if !driven_ok {
                return Err("a constraint driven must name a board hole or via".into());
            }
            match &c.value {
                ConstraintValue::Number(v) => {
                    if !v.is_finite() || v.abs() > 10000.0 {
                        return Err(
                            "constraint values are finite numbers within plus or minus 10000 mm"
                                .into(),
                        );
                    }
                }
                ConstraintValue::Param { name } => {
                    if !self.parameters.contains_key(name) {
                        return Err(
                            "a constraint parameter name must exist in the parameter table".into(),
                        );
                    }
                }
            }
            if !driven_seen.insert(c.driven) {
                return Err(
                    "each driven id appears at mostonce once in the constraint list".into(),
                );
            }
        }
        // Cycle detection: for each constraint, walk the chain of drivers
        // from the driven id. If we reach the original driven id again,
        // there is a cycle.
        for start in &self.constraints {
            let original_driven = start.driven;
            let mut current = original_driven;
            let mut steps = 0;
            loop {
                if steps >= self.constraints.len() {
                    return Err("the constraint graph contains a cycle".into());
                }
                let next = self
                    .constraints
                    .iter()
                    .find(|c| c.driver == current)
                    .map(|c| c.driven);
                match next {
                    Some(d) if d == original_driven => {
                        return Err("the constraint graph contains a cycle".into());
                    }
                    Some(d) => {
                        current = d;
                        steps += 1;
                    }
                    None => break,
                }
            }
        }
        Ok(())
    }

    /// Drop constraints whose driver or driven no longer exists. Called from
    /// prune_object_refs after every command.
    ///
    /// # Rules
    /// - Only removes constraints with dangling ids.
    ///
    /// # Dependencies
    /// - crates/doc/src/lib.rs Project type.
    pub(crate) fn prune_constraints(&mut self) {
        let mut i = 0;
        while i < self.constraints.len() {
            let c = &self.constraints[i];
            let driver_ok = self.board.holes.iter().any(|h| h.id == c.driver)
                || self.board.vias.iter().any(|v| v.id == c.driver);
            let driven_ok = self.board.holes.iter().any(|h| h.id == c.driven)
                || self.board.vias.iter().any(|v| v.id == c.driven);
            if !driver_ok || !driven_ok {
                self.constraints.remove(i);
            } else {
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hole;

    fn test_project() -> crate::Project {
        let mut p = crate::Project::default();
        p.board.holes = vec![
            Hole {
                id: 1,
                position: Point::new(10.0, 10.0),
                diameter: 2.0,
                end: None,
            },
            Hole {
                id: 2,
                position: Point::new(20.0, 10.0),
                diameter: 2.0,
                end: None,
            },
        ];
        p
    }

    #[test]
    fn constraint_places_the_driven_point() {
        let t = driven_target(
            ConstraintKind::DistanceX,
            Point::new(10.0, 10.0),
            Point::new(0.0, 7.0),
            3.2,
        );
        assert!((t.x - 13.2).abs() < 1e-9);
        assert!((t.y - 7.0).abs() < 1e-9);

        let t = driven_target(
            ConstraintKind::DistanceY,
            Point::new(10.0, 10.0),
            Point::new(4.0, 0.0),
            -2.0,
        );
        assert!((t.x - 4.0).abs() < 1e-9);
        assert!((t.y - 8.0).abs() < 1e-9);

        let t = driven_target(
            ConstraintKind::Distance,
            Point::new(0.0, 0.0),
            Point::new(6.0, 8.0),
            5.0,
        );
        assert!((t.x - 3.0).abs() < 1e-9);
        assert!((t.y - 4.0).abs() < 1e-9);

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

    #[test]
    fn constraints_are_validated_and_pruned() {
        let mut p = test_project();

        // Valid constraint: h1 drives h2 with DistanceX of 10mm.
        p.constraints.push(Constraint {
            kind: ConstraintKind::DistanceX,
            driver: 1,
            driven: 2,
            value: ConstraintValue::Number(10.0),
        });
        assert!(p.validate_constraints().is_ok());

        // Driver equals driven fails.
        let mut p2 = test_project();
        p2.constraints.push(Constraint {
            kind: ConstraintKind::DistanceX,
            driver: 1,
            driven: 1,
            value: ConstraintValue::Number(10.0),
        });
        assert!(p2.validate_constraints().is_err());

        // Second constraint with same driven fails (forest rule).
        let mut p3 = test_project();
        p3.constraints.push(Constraint {
            kind: ConstraintKind::DistanceX,
            driver: 1,
            driven: 2,
            value: ConstraintValue::Number(10.0),
        });
        p3.constraints.push(Constraint {
            kind: ConstraintKind::DistanceY,
            driver: 1,
            driven: 2,
            value: ConstraintValue::Number(5.0),
        });
        assert!(p3.validate_constraints().is_err());

        // Cycle: h1->h2 and h2->h1.
        let mut p4 = test_project();
        p4.constraints.push(Constraint {
            kind: ConstraintKind::DistanceX,
            driver: 1,
            driven: 2,
            value: ConstraintValue::Number(10.0),
        });
        p4.constraints.push(Constraint {
            kind: ConstraintKind::DistanceX,
            driver: 2,
            driven: 1,
            value: ConstraintValue::Number(10.0),
        });
        assert!(p4.validate_constraints().is_err());

        // Missing id fails.
        let mut p5 = test_project();
        p5.constraints.push(Constraint {
            kind: ConstraintKind::DistanceX,
            driver: 1,
            driven: 99,
            value: ConstraintValue::Number(10.0),
        });
        assert!(p5.validate_constraints().is_err());

        // Prune: delete hole 2, prune should drop the constraint.
        let mut p6 = test_project();
        p6.constraints.push(Constraint {
            kind: ConstraintKind::DistanceX,
            driver: 1,
            driven: 2,
            value: ConstraintValue::Number(10.0),
        });
        p6.board.holes.retain(|h| h.id != 2);
        p6.prune_object_refs();
        assert!(p6.constraints.is_empty());
    }
}
