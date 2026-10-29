//! Every colour the shell and the canvas use for meaning, in one pure place.
//!
//! Context: `App::style`, `App::accent`, `App::surface`, `App::muted`,
//! `App::ink` and the canvas painters in `canvas.rs` read from here, and
//! `contrast.rs` reads the same values to check them, so the check can never
//! drift from what is drawn.
//! Local rules: no egui context or app state in this file; a colour change
//! here must keep `contrast::tests::every_theme_meets_contrast_minimums` green.
use crate::contrast::contrast_ratio;
use egui::Color32;

/// Colours painted on the board and in the schematic that carry meaning.
///
/// Why: front and back copper, pads, markers and the net highlight must stay
/// visible against the board fill in both themes (WCAG 1.4.11, 3:1).
///
/// # Rules
/// - Dark values are the original look; light values are darker so they hold 3:1
///   on the pale board fill.
///
/// # Dependencies
/// - `crates/ui-egui/src/contrast.rs` checks every field against `board` or `surface`.
///
/// # Do not
/// - Add a literal colour for something meaningful in `canvas.rs`; add a field.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CanvasColours {
    pub board: Color32,
    pub front: Color32,
    pub back: Color32,
    pub pad: Color32,
    pub pad_label_hole: Color32,
    pub pad_label_smd: Color32,
    pub net_tie: Color32,
    pub mask: Color32,
    pub wire: Color32,
}

/// Every colour of one theme: shell text, backgrounds, accent and canvas.
///
/// # Rules
/// - `accent` is already adjusted to read on `panel`, `surface` and `hover`.
///
/// # Dependencies
/// - `crates/ui-egui/src/contrast.rs` reads all fields; `crates/ui-egui/src/lib.rs` `App::style` applies them.
///
/// # Do not
/// - Build one by hand outside `palette` and tests.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ThemeColours {
    pub dark: bool,
    pub accent: Color32,
    pub panel: Color32,
    pub surface: Color32,
    pub hover: Color32,
    pub icon_fill: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub warn: Color32,
    pub error: Color32,
    pub link: Color32,
    pub canvas: CanvasColours,
}

/// The colours for a theme and a chosen accent.
///
/// Why: the accent is a user choice, so it is nudged toward white (dark) or
/// black (light) in small steps until it reads at 4.5:1 on every background it
/// is drawn on. A picker extreme can then never produce unreadable text.
///
/// # Rules
/// - Total: any `accent` gives a valid theme. The last result is memoised per
///   thread because painters ask for it per object.
///
/// # Dependencies
/// - `crates/ui-egui/src/contrast.rs` `contrast_ratio`.
///
/// # Do not
/// - Cache the result across a theme or accent change.
pub(crate) fn palette(dark: bool, accent: [u8; 3]) -> ThemeColours {
    thread_local! {
        static LAST: std::cell::Cell<Option<(bool, [u8; 3], ThemeColours)>> =
            const { std::cell::Cell::new(None) };
    }
    if let Some((d, a, colours)) = LAST.with(std::cell::Cell::get)
        && d == dark
        && a == accent
    {
        return colours;
    }
    let colours = build(dark, accent);
    LAST.with(|last| last.set(Some((dark, accent, colours))));
    colours
}

fn build(dark: bool, accent: [u8; 3]) -> ThemeColours {
    let (panel, surface, hover, icon_fill, ink, muted, warn, error, link, canvas) = if dark {
        (
            Color32::from_rgb(17, 24, 36),
            Color32::from_rgb(20, 29, 43),
            Color32::from_rgb(37, 53, 73),
            Color32::from_rgb(28, 40, 56),
            Color32::from_rgb(225, 234, 247),
            Color32::from_rgb(140, 157, 182),
            Color32::from_rgb(255, 190, 90),
            Color32::from_rgb(255, 130, 120),
            Color32::from_rgb(110, 190, 255),
            canvas_colours(true),
        )
    } else {
        (
            Color32::from_rgb(235, 239, 245),
            Color32::from_rgb(245, 247, 251),
            Color32::from_rgb(215, 225, 236),
            Color32::from_rgb(236, 240, 246),
            Color32::from_rgb(34, 48, 68),
            Color32::from_rgb(78, 94, 118),
            Color32::from_rgb(138, 78, 0),
            Color32::from_rgb(178, 28, 28),
            Color32::from_rgb(0, 90, 170),
            canvas_colours(false),
        )
    };
    let backgrounds = [panel, surface, hover];
    let mut chosen = Color32::from_rgb(accent[0], accent[1], accent[2]);
    let toward = if dark { 255.0 } else { 0.0 };
    for step in 0..=20 {
        if backgrounds
            .iter()
            .all(|b| contrast_ratio(chosen, *b) >= 4.5)
        {
            break;
        }
        let t = 0.05 * (step as f32 + 1.0);
        let mix = |c: u8| {
            (c as f32 + (toward - c as f32) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        chosen = Color32::from_rgb(mix(accent[0]), mix(accent[1]), mix(accent[2]));
    }
    ThemeColours {
        dark,
        accent: chosen,
        panel,
        surface,
        hover,
        icon_fill,
        ink,
        muted,
        warn,
        error,
        link,
        canvas,
    }
}

/// The canvas colours for one mode, without the accent.
///
/// Why: painters call this per object, so it must be cheap; `palette` calls it
/// too so the check and the painting share one source.
///
/// # Rules
/// - Pure and constant-time.
///
/// # Dependencies
/// - `crates/ui-egui/src/canvas.rs` painters; `crates/ui-egui/src/contrast.rs` tests.
///
/// # Do not
/// - Read preferences here.
pub(crate) fn canvas_colours(dark: bool) -> CanvasColours {
    if dark {
        CanvasColours {
            board: Color32::from_rgb(21, 54, 49),
            front: Color32::from_rgb(245, 141, 121),
            back: Color32::from_rgb(116, 167, 236),
            pad: Color32::from_rgb(230, 194, 125),
            pad_label_hole: Color32::from_rgb(230, 194, 125),
            pad_label_smd: Color32::from_rgb(48, 44, 32),
            net_tie: Color32::from_rgb(216, 180, 255),
            mask: Color32::from_rgb(96, 170, 130),
            wire: Color32::from_rgb(126, 153, 161),
        }
    } else {
        CanvasColours {
            board: Color32::from_rgb(205, 233, 222),
            front: Color32::from_rgb(176, 58, 38),
            back: Color32::from_rgb(28, 92, 176),
            pad: Color32::from_rgb(128, 86, 6),
            pad_label_hole: Color32::from_rgb(128, 86, 6),
            pad_label_smd: Color32::from_rgb(255, 244, 220),
            net_tie: Color32::from_rgb(112, 56, 176),
            mask: Color32::from_rgb(30, 110, 70),
            wire: Color32::from_rgb(70, 96, 106),
        }
    }
}

/// The egui visuals for a theme.
///
/// Why: `App::style` and the contrast test must see the same widget colours,
/// so both build them here.
///
/// # Rules
/// - Sets text, link, warning, error, selection and every widget state from
///   `colours`; nothing is left at the egui default that carries meaning.
///
/// # Dependencies
/// - `crates/ui-egui/src/lib.rs` `App::style` applies the result; craft-ui `WidgetPalette` fills the widgets.
///
/// # Do not
/// - Touch spacing here; `App::style` owns that.
pub(crate) fn visuals(colours: &ThemeColours) -> egui::Visuals {
    let mut visuals = if colours.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = colours.panel;
    visuals.window_fill = colours.surface;
    visuals.override_text_color = None;
    visuals.hyperlink_color = colours.link;
    visuals.warn_fg_color = colours.warn;
    visuals.error_fg_color = colours.error;
    visuals.selection.bg_fill = colours.accent.gamma_multiply(0.25);
    visuals.selection.stroke = egui::Stroke::new(1.0, colours.accent);
    craft_ui::theme::WidgetPalette {
        noninteractive: colours.surface,
        inactive: colours.surface,
        hovered: colours.hover,
        active: colours.accent.gamma_multiply(0.25),
        open: colours.surface,
    }
    .apply_to(
        &mut visuals.widgets,
        egui::CornerRadius::same(6),
        egui::Stroke::new(1.0, colours.ink),
    );
    visuals
}
