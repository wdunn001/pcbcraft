//! Schematic sheet shape tool: draw a line, rectangle or circle on the sheet.
//!
//! Context: the Add menu arms the tool. The first click stores the start
//! point, the second click completes the shape with the engine command
//! `graphic.add` on the schematic layer. The tool stays armed until Esc.
//! Local rules: the start point lives on the app until the shape is placed;
//! the document changes only on the second click.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs (`graphic.add`)
//! - crates/ui-egui/src/canvas.rs (snap, preview painting)
//! - crates/text/strings/en.txt (every label and tooltip)
//!
//! # Do not
//! - Use this on the board tab; the shape belongs to the schematic only.
//! - Change the engine shape strings; they are fixed by the command schema.

use super::*;
use crate::canvas::snap;

/// Which schematic shape the tool is placing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum SheetShapeKind {
    /// A straight line between two points.
    Line,
    /// A rectangle defined by two opposite corners.
    Rect,
    /// A circle defined by its centre and the radius (the distance to the second click).
    Circle,
}

impl SheetShapeKind {
    /// The engine command shape string for this kind.
    ///
    /// # Rules
    /// - Must match the `shape` enum values in the `graphic.add` command schema.
    ///
    /// # Do not
    /// - Change these strings without updating the engine schema.
    pub(crate) fn shape_string(&self) -> &'static str {
        match self {
            SheetShapeKind::Line => "line",
            SheetShapeKind::Rect => "rect",
            SheetShapeKind::Circle => "circle",
        }
    }

    /// The plain name for this shape, for hints and tooltips.
    pub(crate) fn name(&self) -> String {
        match self {
            SheetShapeKind::Line => t!("schematic.shape.line"),
            SheetShapeKind::Rect => t!("schematic.shape.rect"),
            SheetShapeKind::Circle => t!("schematic.shape.circle"),
        }
    }
}

impl App {
    /// Handle a click while the sheet shape tool is armed.
    ///
    /// # Rules
    /// - The first click stores the snapped start point.
    /// - The second click places the shape and clears the start.
    /// - A circle's second click defines the radius.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (`graphic.add`: line and rect take two points,
    ///   circle takes one point plus width as the radius)
    pub(crate) fn sheet_shape_click(&mut self, kind: SheetShapeKind, pos: Point) {
        let snapped = snap(pos);
        match self.sheet_shape_start {
            None => {
                self.sheet_shape_start = Some(snapped);
            }
            Some(start) => {
                let shape = kind.shape_string().to_owned();
                let params = match kind {
                    SheetShapeKind::Circle => {
                        let radius = snapped.distance(start).max(0.1);
                        json!({
                            "layer": "schematic",
                            "shape": shape,
                            "points": [start],
                            "width": radius,
                        })
                    }
                    _ => json!({
                        "layer": "schematic",
                        "shape": shape,
                        "points": [start, snapped],
                    }),
                };
                self.run("graphic.add", params);
                self.sheet_shape_start = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_kind_maps_to_engine_strings() {
        assert_eq!(SheetShapeKind::Line.shape_string(), "line");
        assert_eq!(SheetShapeKind::Rect.shape_string(), "rect");
        assert_eq!(SheetShapeKind::Circle.shape_string(), "circle");
    }

    #[test]
    fn shape_kind_names_are_not_empty() {
        assert!(!SheetShapeKind::Line.name().is_empty());
        assert!(!SheetShapeKind::Rect.name().is_empty());
        assert!(!SheetShapeKind::Circle.name().is_empty());
    }
}
