//! The one place the application draws icons.
//!
//! Icons are the embedded SVG files in `assets/icons/ui/`, held in one
//! `craft_ui::icons::SvgIconSet`. `egui_extras`' SVG loader rasterises each
//! icon once per pixel size, so it stays sharp at every scale and DPI; the
//! colour is white in the file and tinted here from the theme.
//!
//! # Rules
//! - Draw icons only through the helpers below; `egui::Image` and
//!   `ImageButton` appear nowhere else in `crates/ui-egui/src`.
//! - `icon_button` needs a tip and gives the control that tip as its
//!   screen-reader name, so an icon-only control without a tooltip cannot
//!   be written.
//! - Names are the file stems in `assets/icons/ui/`; the test below checks
//!   that every file is in the set.
//!
//! # Dependencies
//! - `assets/icons/ui/` (the SVG files) and `assets/icons/ui/LICENSE-lucide.txt`
//! - `crates/ui-egui/src/a11y.rs` for `name_control`
//!
//! # Do not
//! - Hard-code an icon colour; pass the theme colour.

use std::sync::OnceLock;

use egui::{Color32, Context, Image, Response, Ui, Vec2};

use craft_ui::icons::{SvgIconSet, SvgStyle};

macro_rules! icon {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!("../../../assets/icons/ui/", $name, ".svg")) as &[u8],
        )
    };
}

static ICONS: &[(&str, &[u8])] = &[
    icon!("add"),
    icon!("omega"),
    icon!("plug"),
    icon!("cpu"),
    icon!("library"),
    icon!("folder"),
    icon!("globe"),
    icon!("star-filled"),
    icon!("star"),
    icon!("assistant"),
    icon!("board-outline"),
    icon!("bom"),
    icon!("box"),
    icon!("bus"),
    icon!("check"),
    icon!("chevron-down"),
    icon!("chevron-right"),
    icon!("circle"),
    icon!("close"),
    icon!("copy"),
    icon!("courtyard"),
    icon!("cutout"),
    icon!("diff-pair"),
    icon!("drill"),
    icon!("drill-file"),
    icon!("error"),
    icon!("eye"),
    icon!("eye-off"),
    icon!("fit"),
    icon!("focus"),
    icon!("footprint"),
    icon!("grid"),
    icon!("help"),
    icon!("info"),
    icon!("junction"),
    icon!("keepout"),
    icon!("label"),
    icon!("layer-copper"),
    icon!("layer-core"),
    icon!("layer-mask"),
    icon!("layer-paste"),
    icon!("layer-silk"),
    icon!("layers"),
    icon!("line"),
    icon!("lock"),
    icon!("measure"),
    icon!("menu"),
    icon!("mounting-hole"),
    icon!("move"),
    icon!("net"),
    icon!("no-connect"),
    icon!("open"),
    icon!("pad-smd"),
    icon!("pad-th"),
    icon!("part"),
    icon!("plot"),
    icon!("port"),
    icon!("power"),
    icon!("redo"),
    icon!("rotate"),
    icon!("rotate-ccw"),
    icon!("route"),
    icon!("save"),
    icon!("search"),
    icon!("select"),
    icon!("settings"),
    icon!("shapes"),
    icon!("sheet"),
    icon!("stackup"),
    icon!("text"),
    icon!("track"),
    icon!("trash"),
    icon!("undo"),
    icon!("unlock"),
    icon!("via"),
    icon!("view-3d"),
    icon!("warning"),
    icon!("wire"),
    icon!("zone"),
    icon!("zoom-in"),
    icon!("zoom-out"),
];

/// Default icon edge in points; 16 reads in a toolbar, 24 is the pointer target.
pub(crate) const ICON_SIZE: f32 = 18.0;

fn set() -> &'static SvgIconSet {
    static SET: OnceLock<SvgIconSet> = OnceLock::new();
    SET.get_or_init(|| {
        SvgIconSet::new(
            ICONS,
            SvgStyle {
                current_color: "#ffffff",
                stroke_width: None,
            },
            "bytes://pcbcraft/icons/ui/",
            "help",
        )
    })
}

/// Install the SVG loader on the context. Safe to call every frame.
pub(crate) fn install(ctx: &Context) {
    egui_extras::install_image_loaders(ctx);
}

/// Whether the set holds an icon called `name`.
#[cfg(test)]
pub(crate) fn exists(name: &str) -> bool {
    set().contains(name)
}

/// Colour for an icon on this control: the selected colour while armed.
fn tint(ui: &Ui, selected: bool) -> Color32 {
    if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    }
}

fn image(ui: &Ui, name: &str, size: f32, selected: bool) -> Image<'static> {
    Image::new(set().source(name))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(tint(ui, selected))
}

/// A small icon in a line of text, not clickable.
///
/// # Rules
/// - Decorative: the text beside it carries the meaning.
#[expect(dead_code, reason = "wired in the tree and rail steps that follow")]
pub(crate) fn icon_inline(ui: &mut Ui, name: &str) -> Response {
    ui.add(image(ui, name, ICON_SIZE, false))
}

/// An icon laid at a fixed spot, not clickable: a mark inside a field or a
/// placeholder picture on a card. `color` comes from the theme.
///
/// # Rules
/// - Decorative: whatever it stands for is named by its neighbour.
pub(crate) fn icon_at(ui: &mut Ui, rect: egui::Rect, name: &str, color: Color32) {
    let image = Image::new(set().source(name))
        .fit_to_exact_size(rect.size())
        .tint(color);
    let mut at = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    at.add(image);
}

/// An icon-only button. The tip is required: it is the hover text and the
/// screen-reader name.
pub(crate) fn icon_button(ui: &mut Ui, name: &str, tip: &str, selected: bool) -> Response {
    icon_button_sized(ui, name, tip, selected, 26.0)
}

/// An icon-only button of a given side, for dense rows of toggles. The side
/// must stay at least `targets::MIN_TARGET`.
pub(crate) fn icon_button_sized(
    ui: &mut Ui,
    name: &str,
    tip: &str,
    selected: bool,
    side: f32,
) -> Response {
    let button = egui::Button::image(image(ui, name, ICON_SIZE, selected))
        .selected(selected)
        .min_size(Vec2::splat(side.max(crate::targets::MIN_TARGET)));
    let response = ui.add(button).on_hover_text(tip);
    crate::a11y::name_control(&response, tip, Some(selected));
    response
}

/// A button with an icon and its label. `tip` is the longer hover text.
pub(crate) fn icon_label_button(
    ui: &mut Ui,
    name: &str,
    label: &str,
    tip: &str,
    selected: bool,
) -> Response {
    let button = egui::Button::image_and_text(image(ui, name, ICON_SIZE, selected), label)
        .selected(selected);
    let response = ui.add(button).on_hover_text(tip);
    crate::a11y::name_control(&response, label, Some(selected));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svg_stems(dir: &std::path::Path) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|e| e == "svg")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                out.push(stem.to_owned());
            }
        }
        Ok(out)
    }

    #[test]
    fn every_icon_file_is_in_the_set() -> Result<(), String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons/ui");
        let missing: Vec<String> = svg_stems(&dir)?
            .into_iter()
            .filter(|stem| !exists(stem))
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!("icons not in the set: {missing:?}"))
        }
    }

    #[test]
    fn icons_are_drawn_only_in_the_icon_module() -> Result<(), String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![dir];
        let mut bad = Vec::new();
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(d).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs")
                    // The brand mark is a texture of the app logo, not a glyph of the icon set.
                    && path
                        .file_name()
                        .is_some_and(|n| n != "icons.rs" && n != "brand.rs")
                {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    for needle in ["egui::Image::", "ImageButton", "include_image!"] {
                        if text.contains(needle) {
                            bad.push(format!("{} uses {needle}", path.display()));
                        }
                    }
                }
            }
        }
        if bad.is_empty() {
            Ok(())
        } else {
            Err(format!("{bad:?}"))
        }
    }
}
