//! Free board graphics (imported drawings and traced pictures) on the board
//! canvas, coloured by the fabrication sublayer they belong to.
use super::*;
use egui::Color32;

impl App {
    pub(super) fn paint_board_graphics(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        p: &Project,
    ) {
        for g in p.graphics.iter().filter(|g| {
            g.layer == "board"
                && match g.sublayer.as_str() {
                    "F.SilkS" => self.board_sub_effective_on_layer(
                        pcbcraft_doc::Layer::Front,
                        crate::layer_visibility::SubLayer::Silk,
                    ),
                    "B.SilkS" => self.board_sub_effective_on_layer(
                        pcbcraft_doc::Layer::Back,
                        crate::layer_visibility::SubLayer::Silk,
                    ),
                    _ => true,
                }
        }) {
            let color = match g.sublayer.as_str() {
                "F.SilkS" => Color32::from_rgb(240, 236, 200),
                "B.SilkS" => Color32::from_rgb(150, 190, 240),
                "F.Mask" => Color32::from_rgb(120, 200, 140),
                "B.Mask" => Color32::from_rgb(90, 150, 120),
                _ => self.accent(),
            };
            if let Some(outline) = g.outline() {
                let points: Vec<egui::Pos2> =
                    outline.iter().map(|pt| camera.screen(*pt, rect)).collect();
                let stroke = Stroke::new((g.width as f32 * camera.zoom).clamp(1.0, 6.0), color);
                if g.shape == "line" {
                    painter.add(egui::Shape::line(points, stroke));
                } else {
                    painter.add(egui::Shape::closed_line(points, stroke));
                }
            } else if let Some(anchor) = g.points.first() {
                painter.text(
                    camera.screen(*anchor, rect),
                    Align2::LEFT_TOP,
                    &g.text,
                    FontId::proportional((g.width as f32 * camera.zoom).clamp(6.0, 48.0)),
                    color,
                );
            }
        }
    }
}
