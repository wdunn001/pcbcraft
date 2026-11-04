//! The vertex table in Board settings: one row per outline vertex with editable
//! X and Y, showing arc radii for curved segments.
//!
//! Context: the board outline is a list of vertices in millimetres; segments can
//! be straight or arcs. This table shows every vertex so a designer can type exact
//! positions without dragging on the canvas. Picking a preset shape from the Shape
//! section first makes the outline editable; a plain rectangle shows its four
//! corners read-only.
//! Local rules: draft values live in egui memory; a typed coordinate is committed
//! to the board when the field loses focus or Enter is pressed, via
//! `board.outline_set_vertex`. A refused coordinate (for example a vertex pushed
//! past the origin) shows the engine's reason in the message line and the field
//! reverts next frame. The table is capped at 64 rows with a plain note.
//!
//! # Dependencies
//! - crates/engine/src/outline.rs (`board.outline_set_vertex`)
//! - crates/ui-egui/src/ui_kit/number_field.rs
//! - crates/doc/src/outline.rs (`segment_arc`)
//!
//! # Do not
//! - Store draft vertices in the document; they belong in egui memory only.
//! - Compute the flattened outline or the board area here; the Shape section owns
//!   that.
//! - Add or delete vertices here; that is a canvas operation.
use super::*;

/// Most vertices the table will show; the board supports far more but a table of
/// 64 rows is already too tall to use.
const MAX_ROWS: usize = 64;

/// The outline points to display. If the board has an explicit outline, use it;
/// otherwise the plain rectangle's four corners.
fn outline_points(board: &pcbcraft_doc::Board) -> Vec<Point> {
    if let Some(points) = &board.outline {
        return points.clone();
    }
    let (w, h) = (board.width, board.height);
    vec![
        Point::new(0.0, 0.0),
        Point::new(w, 0.0),
        Point::new(w, h),
        Point::new(0.0, h),
    ]
}

/// Is the board a plain rectangle with no explicit outline?
fn is_plain(board: &pcbcraft_doc::Board) -> bool {
    board.outline.is_none()
}

/// The radius of the arc on segment `(index - 1)` that touches vertex
/// `index` (the segment that ends at this vertex), in millimetres.
fn touching_arc_radius(board: &pcbcraft_doc::Board, index: usize, n: usize) -> Option<f64> {
    let seg = (index + n - 1) % n;
    pcbcraft_doc::segment_arc(&outline_points(board), &board.outline_arcs, seg)
        .map(|arc| arc.radius)
}

impl App {
    /// The vertex table: one row per outline vertex with editable X and Y.
    ///
    /// # Rules
    /// - When the board is a plain rectangle, show the four corners read-only and
    ///   a sentence that picking a shape makes the points editable.
    /// - A typed coordinate is committed when the field loses focus or Enter is
    ///   pressed. A refused coordinate shows the engine's message and reverts
    ///   next frame.
    ///
    /// # Dependencies
    /// - crates/engine/src/outline.rs (`board.outline_set_vertex`)
    ///
    /// # Do not
    /// - Add or delete vertices here.
    pub(crate) fn outline_vertex_table(&mut self, ui: &mut egui::Ui) {
        let board = &self.project.board;
        let points = outline_points(board);
        let n = points.len();
        let plain = is_plain(board);
        let rows = n.min(MAX_ROWS);

        if plain {
            let msg = t!("shape.plain_rectangle");
            let tip = t!("shape.plain_rectangle.tip");
            ui.label(fonts::caption(&msg)).on_hover_text(&tip);
        }

        let mut changed = Vec::new();
        for (i, pt) in points.iter().enumerate().take(rows) {
            let (mut x, mut y) = (pt.x, pt.y);
            let row = t!("shape.vertex_row", index = i + 1);
            let commit_x = form_row(ui, ROW, &row, None, None, |ui| {
                let spec = NumberSpec {
                    min: -2000.0,
                    max: 2000.0,
                    step: 0.5,
                    places: 2,
                    width: 100.0,
                    ..NumberSpec::default()
                };
                let tip = t!("shape.vertex_x_tip", index = i + 1);
                let name = t!("shape.vertex_x_name", index = i + 1);
                if plain {
                    value_text(ui, &format!("{:.2}", x));
                    false
                } else {
                    number_field(ui, format!("outline-x-{}", i), &mut x, &spec, &name, &tip)
                        .changed()
                }
            });
            if commit_x {
                changed.push((i, x, points[i].y));
            }
            let commit_y = form_row(ui, ROW, "", None, None, |ui| {
                let spec = NumberSpec {
                    min: -2000.0,
                    max: 2000.0,
                    step: 0.5,
                    places: 2,
                    width: 100.0,
                    ..NumberSpec::default()
                };
                let tip = t!("shape.vertex_y_tip", index = i + 1);
                let name = t!("shape.vertex_y_name", index = i + 1);
                if plain {
                    value_text(ui, &format!("{:.2}", y));
                    false
                } else {
                    number_field(ui, format!("outline-y-{}", i), &mut y, &spec, &name, &tip)
                        .changed()
                }
            });
            if commit_y {
                changed.push((i, points[i].x, y));
            }
            // Show the touching arc radius, if any.
            if let Some(r) = touching_arc_radius(board, i, n) {
                let arc_msg = t!("shape.vertex_arc_radius", radius = r);
                ui.label(fonts::caption(&arc_msg))
                    .on_hover_text(t!("shape.vertex_arc_radius.tip"));
            }
        }
        for (i, x, y) in changed {
            self.run(
                "board.outline_set_vertex",
                json!({"index": i, "at": {"x": x, "y": y}}),
            );
        }
        if n > MAX_ROWS {
            let note = t!("shape.vertex_table_note", count = MAX_ROWS);
            ui.label(fonts::caption(&note))
                .on_hover_text(t!("shape.vertex_table_note.tip"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_points_defaults_to_the_rectangle_corners() {
        let board = pcbcraft_doc::Board::default();
        let pts = outline_points(&board);
        assert_eq!(pts.len(), 4);
        assert_eq!(pts[0], Point::new(0.0, 0.0));
        assert_eq!(pts[1], Point::new(board.width, 0.0));
    }

    #[test]
    fn is_plain_returns_true_for_a_board_without_an_outline() {
        let board = pcbcraft_doc::Board::default();
        assert!(is_plain(&board));
    }
}
