//! Runs the component gallery in a window and optionally captures it to a PNG.
//!
//! Usage: `ui_kit_gallery [--light] [--page NAME] [--capture /absolute/path.png]`.
//! Local rules: no app state; the page code lives in `src/ui_kit/gallery.rs`.
use eframe::egui;
use pcbcraft_ui_egui::theme;
use pcbcraft_ui_egui::ui_kit::gallery::{self, GalleryState, Page};

struct Gallery {
    state: GalleryState,
    capture: Option<String>,
    frames: u32,
}

impl eframe::App for Gallery {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        theme::apply(&ctx, &self.state.tokens);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(self.state.tokens.ground)
                    .inner_margin(24),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| gallery::show(ui, &mut self.state));
            });
        self.frames += 1;
        if let Some(path) = self.capture.clone() {
            if self.frames == 8 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            let shots: Vec<_> = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                    .collect()
            });
            for image in shots {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                if let Err(e) = image::save_buffer(
                    &path,
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    eprintln!("capture: {e}");
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let option = |key: &str| {
        args.iter()
            .position(|a| a == key)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let dark = !args.iter().any(|a| a == "--light");
    let capture = option("--capture");
    let page = option("--page").and_then(|name| Page::from_name(&name));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1560.0, 960.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PCBCraft component gallery",
        options,
        Box::new(move |cc| {
            pcbcraft_ui_egui::fonts::install(&cc.egui_ctx);
            let mut state = GalleryState::new(dark);
            if let Some(page) = page {
                state.page = page;
            }
            Ok(Box::new(Gallery {
                state,
                capture,
                frames: 0,
            }))
        }),
    )
}
