//! Paints the copper layers' own shapes on the Layout canvas.
//!
//! Context: each copper layer may have a shape that differs from the board
//! shape and overlaps the others. The active layer's shape is drawn solid in
//! the layer colour. The other layers' own shapes are drawn as thin dashed
//! outlines in their colours so the overlap is visible.
//! Local rules: a layer whose eye is off draws nothing; `App::shape_paint_plan`
//! decides what is drawn, this file only paints that plan.
//!
//! # Dependencies
//! - crates/ui-egui/src/layer_shape_edit.rs (`App::shape_paint_plan`)
//! - crates/ui-egui/src/outline_arcs.rs (`edge_screen_points`)
//!
//! # Do not
//! - Decide visibility here; ask the plan.
use super::*;
use egui::Color32;

impl App {
    /// The colour of copper layer `key` on the canvas.
    pub(crate) fn shape_layer_colour(&self, key: &str) -> Color32 {
        let c = self.colours();
        match self.project.board.resolve_copper_index(key) {
            Ok(0) => c.canvas.front,
            Ok(i) if i + 1 >= self.project.board.copper_count() => c.canvas.back,
            _ => c.canvas.front.gamma_multiply(0.6),
        }
    }

    /// Draw every layer's own shape in the plan.
    pub(super) fn paint_layer_shapes(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        p: &Project,
    ) {
        if self.tab != Tab::Pcb {
            return;
        }
        for planned in self.shape_paint_plan() {
            let (points, arcs) = p.board.shape_of(&planned.layer);
            let n = points.len();
            if n < 3 {
                continue;
            }
            let colour = self.shape_layer_colour(&planned.layer);
            let mut ring: Vec<egui::Pos2> = Vec::new();
            for i in 0..n {
                let edge = crate::outline_arcs::edge_screen_points(points, arcs, i, camera, rect);
                ring.extend(edge.iter().take(edge.len().saturating_sub(1)));
            }
            if let Some(first) = ring.first().copied() {
                ring.push(first);
            }
            if planned.active {
                painter.add(egui::Shape::line(ring, Stroke::new(2.0, colour)));
            } else {
                painter.add(egui::Shape::dashed_line(
                    &ring,
                    Stroke::new(1.0, colour),
                    5.0,
                    4.0,
                ));
            }
        }
    }
}
