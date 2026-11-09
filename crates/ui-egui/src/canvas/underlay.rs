//! Reference pictures painted under the schematic and the board. They are
//! editor-only: nothing here touches an export.
use super::*;
use egui::{Color32, TextureHandle, TextureOptions};
use pcbcraft_doc::EmbeddedFile;

/// Cache key for a decoded attachment. It samples the base64 text instead of
/// hashing all of it, so a repaint stays cheap for large pictures.
fn cache_key(file: &EmbeddedFile) -> egui::Id {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let bytes = file.data.as_bytes();
    let stride = (bytes.len() / 512).max(1);
    for byte in bytes.iter().step_by(stride) {
        byte.hash(&mut hasher);
    }
    egui::Id::new(("reference-image", &file.name, bytes.len(), hasher.finish()))
}

/// The texture for an attachment, decoded once and then kept in the context.
/// A picture that fails to decode is remembered as missing so it is not
/// retried on every frame.
fn texture(ctx: &egui::Context, file: &EmbeddedFile) -> Option<TextureHandle> {
    let key = cache_key(file);
    if let Some(cached) = ctx.data(|d| d.get_temp::<Option<TextureHandle>>(key)) {
        return cached;
    }
    let loaded = pcbcraft_io::decode_embedded_png(file).ok().map(|img| {
        let size = [img.width() as usize, img.height() as usize];
        let pixels = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        ctx.load_texture(
            format!("reference-{}", file.name),
            pixels,
            TextureOptions::LINEAR,
        )
    });
    ctx.data_mut(|d| d.insert_temp(key, loaded.clone()));
    loaded
}

/// Paint every visible reference picture for `view` (`schematic` or `pcb`),
/// in id order, with its own opacity. Call before the artwork.
pub(super) fn paint(
    painter: &egui::Painter,
    rect: Rect,
    camera: Camera,
    project: &Project,
    view: &str,
) {
    for reference in project
        .reference_images
        .iter()
        .filter(|r| r.view == view && r.visible && r.opacity > 0.0)
    {
        let Some(file) = project.files.iter().find(|f| f.name == reference.file) else {
            continue;
        };
        let Some(tex) = texture(painter.ctx(), file) else {
            continue;
        };
        let [w, h] = tex.size();
        let corner = Point::new(
            reference.x + w as f64 * reference.scale,
            reference.y + h as f64 * reference.scale,
        );
        let target = Rect::from_two_pos(
            camera.screen(Point::new(reference.x, reference.y), rect),
            camera.screen(corner, rect),
        );
        if !target.intersects(rect) {
            continue;
        }
        painter.image(
            tex.id(),
            target,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE.gamma_multiply(reference.opacity as f32),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use serde_json::json;

    const LOGO: &[u8] = include_bytes!("../../../io/tests/fixtures/logo-64x64-two-color.png");

    fn project_with_logo(view: &str) -> Result<Project, String> {
        let mut session = pcbcraft_engine::Session::default();
        let data = base64::engine::general_purpose::STANDARD.encode(LOGO);
        session
            .execute(
                "file.attach_binary",
                json!({"name":"logo.png","data":data}),
                None,
            )
            .map_err(|e| e.to_string())?;
        session
            .execute(
                "reference_image.add",
                json!({"file":"logo.png","view":view,"x":5,"y":5,"scale":0.2,"opacity":0.5}),
                None,
            )
            .map_err(|e| e.to_string())?;
        Ok(session.project.clone())
    }

    /// Number of textured quads the underlay put on the painter.
    fn textured_quads(project: &Project, view: &str) -> usize {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |root_ui| {
            egui::CentralPanel::default().show(root_ui, |ui| {
                paint(
                    ui.painter(),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 500.0)),
                    Camera {
                        mirror: false,
                        turns: 0,
                        up: false,
                        center: Point::new(10.0, 10.0),
                        zoom: 20.0,
                    },
                    project,
                    view,
                );
            });
        });
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .filter(|s| matches!(&s.shape, egui::Shape::Mesh(m) if m.texture_id != egui::TextureId::default()))
            .count()
    }

    #[test]
    fn reference_underlay_paints_only_its_own_visible_view() -> Result<(), String> {
        let mut project = project_with_logo("pcb")?;
        assert_eq!(textured_quads(&project, "pcb"), 1, "the board shows it");
        assert_eq!(
            textured_quads(&project, "schematic"),
            0,
            "the sheet does not"
        );
        project.reference_images[0].visible = false;
        assert_eq!(
            textured_quads(&project, "pcb"),
            0,
            "hidden means not painted"
        );
        project.reference_images[0].visible = true;
        project.reference_images[0].opacity = 0.0;
        assert_eq!(
            textured_quads(&project, "pcb"),
            0,
            "fully faded is not painted"
        );
        Ok(())
    }

    #[test]
    fn reference_underlay_does_not_reach_the_preview_export() -> Result<(), String> {
        let with = project_with_logo("pcb")?;
        let mut without = with.clone();
        without.reference_images.clear();
        assert_eq!(
            pcbcraft_io::preview_png(&with, "pcb")?,
            pcbcraft_io::preview_png(&without, "pcb")?,
            "the picture is editor-only"
        );
        Ok(())
    }
}
