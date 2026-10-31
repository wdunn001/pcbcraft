//! The PCBCraft mark inside the application: the window icon, the app bar
//! brand, the start page and the About window.
//!
//! Context: the artwork is generated from the masters in `assets/brand/` by
//! `assets/brand/build.sh`. This file embeds the generated PNGs and never draws
//! the mark itself, so there is one source of truth for the shape.
//! Local rules: every size shown comes from the nearest embedded size, with the
//! hand-tuned 16 and 32 px art for small drawing; all text comes from the catalog.

use egui::{ColorImage, Context, Id, TextureHandle, TextureOptions, Vec2};

const ICON_16: &[u8] = include_bytes!(
    "../../../assets/brand/generated/linux/hicolor/16x16/apps/io.github.tangency-labs.pcbcraft.png"
);
const ICON_32: &[u8] = include_bytes!(
    "../../../assets/brand/generated/linux/hicolor/32x32/apps/io.github.tangency-labs.pcbcraft.png"
);
const ICON_128: &[u8] = include_bytes!(
    "../../../assets/brand/generated/linux/hicolor/128x128/apps/io.github.tangency-labs.pcbcraft.png"
);
const ICON_256: &[u8] = include_bytes!(
    "../../../assets/brand/generated/linux/hicolor/256x256/apps/io.github.tangency-labs.pcbcraft.png"
);
const ICON_512: &[u8] = include_bytes!(
    "../../../assets/brand/generated/linux/hicolor/512x512/apps/io.github.tangency-labs.pcbcraft.png"
);

/// The embedded size to use for a mark drawn `pixels` device pixels wide.
///
/// Why: 16 and 32 px are drawn by hand on a pixel grid and read better than a
/// scaled-down large icon.
///
/// # Rules
/// - Returns one of 16, 32, 128 or 512.
///
/// # Dependencies
/// - `assets/brand/generated/linux/hicolor` holds the PNGs.
///
/// # Do not
/// - Scale a larger icon down when a smaller embedded one is close enough.
pub(crate) fn variant_for(pixels: f32) -> u32 {
    if pixels <= 20.0 {
        16
    } else if pixels <= 40.0 {
        32
    } else if pixels <= 160.0 {
        128
    } else {
        512
    }
}

fn bytes_for(size: u32) -> &'static [u8] {
    match size {
        16 => ICON_16,
        32 => ICON_32,
        128 => ICON_128,
        256 => ICON_256,
        _ => ICON_512,
    }
}

fn decode(bytes: &[u8]) -> Option<image::RgbaImage> {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()
        .map(|decoded| decoded.to_rgba8())
}

/// The 256 px icon for the operating system window, or `None` if it cannot be decoded.
///
/// # Rules
/// - The result is a 256 by 256 RGBA image.
///
/// # Dependencies
/// - `apps/pcbcraft/src/main.rs` `egui_viewport` is the caller.
///
/// # Do not
/// - Fall back to a blank icon with a panic; callers keep the default icon on `None`.
pub fn window_icon() -> Option<egui::IconData> {
    let rgba = decode(ICON_256)?;
    Some(egui::IconData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

/// The mark as an image `size` logical pixels square, with its accessible name.
///
/// # Rules
/// - Textures are decoded once per embedded size and kept in the context.
/// - The alt text is the catalog name `brand.mark`.
///
/// # Dependencies
/// - `crates/text/strings/en.txt` keys `brand.mark` and `brand.mark.tip`.
///
/// # Do not
/// - Make the image clickable here; the app bar wraps it in its own button.
pub(crate) fn image(ctx: &Context, size: f32) -> Option<egui::Image<'static>> {
    let variant = variant_for(size * ctx.pixels_per_point());
    let id = Id::new(("brand-mark", variant));
    let cached = ctx.data_mut(|d| d.get_temp::<TextureHandle>(id));
    let texture = match cached {
        Some(texture) => texture,
        None => {
            let rgba = decode(bytes_for(variant))?;
            let size = [rgba.width() as usize, rgba.height() as usize];
            let texture = ctx.load_texture(
                format!("brand-mark-{variant}"),
                ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
                TextureOptions::LINEAR,
            );
            ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
            texture
        }
    };
    Some(
        egui::Image::new((texture.id(), Vec2::splat(size)))
            .fit_to_exact_size(Vec2::splat(size))
            .alt_text(pcbcraft_text::t!("brand.mark")),
    )
}

/// Draw the mark with its tooltip, for places where it is not a button.
///
/// # Do not
/// - Use it inside a button; use [`image`] so the button keeps one name.
pub(crate) fn mark(ui: &mut egui::Ui, size: f32) {
    if let Some(img) = image(ui.ctx(), size) {
        ui.add(img.sense(egui::Sense::hover()))
            .on_hover_text(pcbcraft_text::t!("brand.mark.tip"));
    }
}

/// The About window: mark, name, version, licence and the font credit.
///
/// # Rules
/// - Closes from its title button or the Close button; both clear `open`.
///
/// # Dependencies
/// - `crates/text/strings/en.txt` keys under `about.` and `app.`.
///
/// # Do not
/// - Add controls without a catalog tooltip.
pub(crate) fn about_window(ui: &mut egui::Ui, open: &mut bool, muted: egui::Color32) {
    if !*open {
        return;
    }
    let mut close = false;
    egui::Window::new(pcbcraft_text::t!("about.title"))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                mark(ui, 128.0);
                ui.add_space(12.0);
                ui.label(crate::fonts::text(
                    crate::fonts::Role::Display,
                    pcbcraft_text::t!("app.name"),
                ));
                ui.label(
                    egui::RichText::new(pcbcraft_text::t!(
                        "about.version",
                        version = env!("CARGO_PKG_VERSION")
                    ))
                    .color(muted),
                );
                ui.add_space(12.0);
                ui.label(pcbcraft_text::t!("about.tagline"));
                ui.add_space(8.0);
                ui.label(crate::fonts::caption(pcbcraft_text::t!("about.licence")).color(muted));
                ui.label(crate::fonts::caption(pcbcraft_text::t!("about.font")).color(muted));
                ui.add_space(16.0);
                if ui
                    .button(pcbcraft_text::t!("about.close"))
                    .on_hover_text(pcbcraft_text::t!("about.close.tip"))
                    .clicked()
                {
                    close = true;
                }
                ui.add_space(8.0);
            });
        });
    if close {
        *open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASTERS: [(&str, &str); 11] = [
        (
            "icon",
            include_str!("../../../assets/brand/pcbcraft-icon.svg"),
        ),
        (
            "icon-32",
            include_str!("../../../assets/brand/pcbcraft-icon-32.svg"),
        ),
        (
            "icon-16",
            include_str!("../../../assets/brand/pcbcraft-icon-16.svg"),
        ),
        (
            "mark-dark",
            include_str!("../../../assets/brand/pcbcraft-mark-dark.svg"),
        ),
        (
            "mark-light",
            include_str!("../../../assets/brand/pcbcraft-mark-light.svg"),
        ),
        (
            "mark-mono",
            include_str!("../../../assets/brand/pcbcraft-mark-mono.svg"),
        ),
        (
            "logo-dark",
            include_str!("../../../assets/brand/pcbcraft-logo-dark.svg"),
        ),
        (
            "logo-light",
            include_str!("../../../assets/brand/pcbcraft-logo-light.svg"),
        ),
        (
            "logo-mono",
            include_str!("../../../assets/brand/pcbcraft-logo-mono.svg"),
        ),
        (
            "wordmark-dark",
            include_str!("../../../assets/brand/pcbcraft-wordmark-dark.svg"),
        ),
        (
            "wordmark-light",
            include_str!("../../../assets/brand/pcbcraft-wordmark-light.svg"),
        ),
    ];

    fn root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/brand")
    }

    #[test]
    fn brand_masters_are_outlined_and_licensed() {
        for (name, svg) in MASTERS {
            assert!(svg.starts_with("<?xml"), "{name} is an XML document");
            assert!(svg.contains("<title>"), "{name} has a title");
            assert!(!svg.contains("<text"), "{name} has live text, outline it");
            assert!(!svg.contains("font-family"), "{name} names a font");
        }
        let licence = std::fs::read_to_string(root().join("fonts/OFL.txt")).unwrap_or_default();
        assert!(licence.contains("SIL OPEN FONT LICENSE Version 1.1"));
        let attribution =
            std::fs::read_to_string(root().join("../../ATTRIBUTION.md")).unwrap_or_default();
        assert!(
            attribution.contains("IBM Plex Sans"),
            "ATTRIBUTION.md credits the font"
        );
    }

    #[test]
    fn brand_generated_icons_cover_every_platform() {
        let generated = root().join("generated");
        for size in [16, 24, 32, 48, 64, 128, 256, 512] {
            let p = generated.join(format!(
                "linux/hicolor/{size}x{size}/apps/io.github.tangency-labs.pcbcraft.png"
            ));
            let img = image::open(&p).map(|i| (i.width(), i.height()));
            assert_eq!(img.ok(), Some((size, size)), "{}", p.display());
        }
        assert!(
            generated
                .join("linux/hicolor/scalable/apps/io.github.tangency-labs.pcbcraft.svg")
                .exists()
        );
        let ico = std::fs::read(generated.join("windows/pcbcraft.ico")).unwrap_or_default();
        assert_eq!(ico.get(..4), Some(&[0u8, 0, 1, 0][..]), "ico header");
        assert!(ico.len() > 1000);
        let icns = std::fs::read(generated.join("macos/pcbcraft.icns")).unwrap_or_default();
        assert_eq!(icns.get(..4), Some(&b"icns"[..]), "icns header");
        for file in [
            "favicon.ico",
            "favicon.svg",
            "apple-touch-icon.png",
            "icon-192.png",
            "icon-512.png",
            "icon-maskable-512.png",
            "icon-monochrome-512.png",
            "manifest.webmanifest",
        ] {
            assert!(generated.join("web").join(file).exists(), "web/{file}");
        }
    }

    #[test]
    fn brand_has_no_old_icon_source() {
        assert!(!root().join("../icons/app/linux/source.svg").exists());
    }

    #[test]
    fn window_icon_is_a_256_square() {
        let icon = window_icon();
        assert_eq!(icon.as_ref().map(|i| (i.width, i.height)), Some((256, 256)));
        assert_eq!(icon.map(|i| i.rgba.len()), Some(256 * 256 * 4));
    }

    #[test]
    fn small_sizes_use_the_hand_tuned_art() {
        assert_eq!(variant_for(16.0), 16);
        assert_eq!(variant_for(24.0), 32);
        assert_eq!(variant_for(64.0), 128);
        assert_eq!(variant_for(400.0), 512);
        for size in [16, 32, 128, 256, 512] {
            assert!(decode(bytes_for(size)).is_some(), "{size} px decodes");
        }
    }

    fn luminance(hex: &str) -> f64 {
        let channel = |i: usize| {
            let v = u8::from_str_radix(&hex[1 + i * 2..3 + i * 2], 16).unwrap_or(0) as f64 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2)
    }

    fn ratio(a: &str, b: &str) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    fn fills(svg: &str) -> Vec<String> {
        let mut out = Vec::new();
        for part in svg.split("fill=\"#").skip(1) {
            let hex: String = part.chars().take_while(|c| *c != '"').collect();
            if hex.len() == 6 && !out.contains(&format!("#{hex}")) {
                out.push(format!("#{hex}"));
            }
        }
        out
    }

    #[test]
    fn brand_variants_meet_non_text_contrast() {
        for (name, svg, background) in [
            ("mark-dark", MASTERS[3].1, "#0b1a16"),
            ("logo-dark", MASTERS[6].1, "#0b1a16"),
            ("mark-light", MASTERS[4].1, "#f4f2ec"),
            ("logo-light", MASTERS[7].1, "#f4f2ec"),
        ] {
            let colours = fills(svg);
            assert!(colours.len() >= 3, "{name} has its plate colours");
            for colour in colours {
                let r = ratio(&colour, background);
                assert!(
                    r >= 3.0,
                    "{name}: {colour} on {background} is {r:.2}:1, needs 3:1"
                );
            }
        }
    }
}
