//! Design tokens of the approved app design: UI colour roles, canvas colours,
//! spacing, radius and heights, and the egui `Style` and `Visuals` built from them.
//!
//! Context: the approved design system (colour roles, spacing, strokes) is the
//! source of every number here. The widgets in `./ui_kit/` read `Tokens` and the
//! size constants; pages adopt them through `apply`.
//! Local rules: UI chrome uses `Tokens`; the painter uses `CanvasColors`; the two
//! sets never mix. No egui context or app state is stored here.
//!
//! # Dependencies
//! - crates/ui-egui/src/contrast.rs checks the role pairs below.
//! - crates/ui-egui/src/fonts.rs sets the text styles `apply` installs.
//!
//! # Do not
//! - Add a literal colour, size or radius to a widget; add a field or constant here.
use egui::{Color32, CornerRadius, Margin, Shadow, Stroke, Vec2};

/// The 4 px spacing scale: 4, 8, 12, 16, 24, 32.
pub const SPACE: [f32; 6] = [4.0, 8.0, 12.0, 16.0, 24.0, 32.0];
/// Corner radius of controls, chips and menus.
pub const RADIUS_CONTROL: u8 = 4;
/// Corner radius of cards, windows and floating canvas controls.
pub const RADIUS_CARD: u8 = 6;
/// Corner radius of the items inside a segmented control.
pub const RADIUS_SEGMENT: u8 = 2;
/// Height of a normal control (small 24, large 32).
pub const CONTROL_HEIGHT: f32 = 28.0;
/// Height of a small control.
pub const CONTROL_HEIGHT_SMALL: f32 = 24.0;
/// Height of a large control.
pub const CONTROL_HEIGHT_LARGE: f32 = 32.0;
/// Height of the tab nav, aside tabs and the tool bar.
pub const BAR_HEIGHT: f32 = 40.0;
/// Height of a bottom-sheet tab row and of a panel header.
pub const SHEET_TAB_HEIGHT: f32 = 36.0;
/// Height of the status bar.
pub const STATUS_HEIGHT: f32 = 24.0;
/// Width of a 1 px border.
pub const BORDER: f32 = 1.0;
/// Width of a selection or focus stroke.
pub const FOCUS_STROKE: f32 = 2.0;

/// Which of the two themes a set of tokens belongs to.
///
/// # Rules
/// - `Preferences.dark` in `crates/ui-egui/src/lib.rs` picks the theme; use `Tokens::for_dark`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    /// The dark theme.
    Dark,
    /// The light theme.
    Light,
}

/// Every UI colour role of one theme, as named in the design system.
///
/// Why: chrome (menus, panels, buttons, tabs) takes all its colour from here so
/// both themes pass the contrast gate and no widget carries its own colour.
///
/// # Rules
/// - Tint fields are translucent; composite them over `panel` before measuring contrast.
///
/// # Dependencies
/// - crates/ui-egui/src/contrast.rs measures the pairs.
///
/// # Do not
/// - Use for canvas drawing; use `CanvasColors`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tokens {
    /// Which theme this is.
    pub theme: Theme,
    /// Window, menu bar, tab nav, work area behind cards.
    pub ground: Color32,
    /// Asides, cards, tool bar, bottom sheet, floating canvas controls.
    pub panel: Color32,
    /// Inputs, segmented track, chips, hovered rows.
    pub raised: Color32,
    /// Hover on raised surfaces.
    pub hover: Color32,
    /// Decorative dividers; never the only border of a control.
    pub line: Color32,
    /// Borders of inputs and buttons.
    pub edge: Color32,
    /// Body text and the focus ring.
    pub text: Color32,
    /// Secondary text and idle icons.
    pub muted: Color32,
    /// Primary action, active tab bar, selection.
    pub copper: Color32,
    /// Success, no problems.
    pub mask: Color32,
    /// Warning.
    pub gold: Color32,
    /// Links, information.
    pub cyan: Color32,
    /// Error.
    pub red: Color32,
    /// Text on a copper fill.
    pub onacc: Color32,
    /// Copper tint for selected rows and active tools.
    pub copper_tint: Color32,
    /// Gold tint for warning badges.
    pub gold_tint: Color32,
    /// Red tint for error badges.
    pub red_tint: Color32,
    /// Mask tint for success badges.
    pub mask_tint: Color32,
    /// Dimming layer behind a modal dialog.
    pub scrim: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const fn tint(hex: u32, percent: u8) -> Color32 {
    // percent of 255, rounded.
    let alpha = ((percent as u32 * 255 + 50) / 100) as u8;
    Color32::from_rgba_unmultiplied_const((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, alpha)
}

impl Tokens {
    /// The dark theme values.
    pub const DARK: Tokens = Tokens {
        theme: Theme::Dark,
        ground: rgb(0x0A1411),
        panel: rgb(0x111F1A),
        raised: rgb(0x182A23),
        hover: rgb(0x1F352C),
        line: rgb(0x2C4A3E),
        edge: rgb(0x5E8576),
        text: rgb(0xE8F0EB),
        muted: rgb(0x9DB3A8),
        copper: rgb(0xE39A5B),
        mask: rgb(0x4FCB8C),
        gold: rgb(0xE6BE5A),
        cyan: rgb(0x5CC8E8),
        red: rgb(0xFF8A80),
        onacc: rgb(0x0A1411),
        copper_tint: tint(0xE39A5B, 16),
        gold_tint: tint(0xE6BE5A, 16),
        red_tint: tint(0xFF8A80, 16),
        mask_tint: tint(0x4FCB8C, 16),
        scrim: tint(0x0A1411, 72),
    };
    /// The light theme values.
    pub const LIGHT: Tokens = Tokens {
        theme: Theme::Light,
        ground: rgb(0xF4F3EC),
        panel: rgb(0xFFFFFF),
        raised: rgb(0xE9E8DF),
        hover: rgb(0xDEDDD2),
        line: rgb(0xB9C4BC),
        edge: rgb(0x667B70),
        text: rgb(0x101C17),
        muted: rgb(0x44574E),
        copper: rgb(0x8F4A12),
        mask: rgb(0x14663F),
        gold: rgb(0x7A5A00),
        cyan: rgb(0x0B6785),
        red: rgb(0xB3261E),
        onacc: rgb(0xFFFFFF),
        copper_tint: tint(0x8F4A12, 12),
        gold_tint: tint(0x7A5A00, 12),
        red_tint: tint(0xB3261E, 12),
        mask_tint: tint(0x14663F, 12),
        scrim: tint(0x101C17, 40),
    };

    /// The tokens for `Preferences.dark`.
    pub fn for_dark(dark: bool) -> Tokens {
        if dark { Self::DARK } else { Self::LIGHT }
    }

    /// True for the dark theme.
    pub fn is_dark(&self) -> bool {
        self.theme == Theme::Dark
    }

    /// A colour at the given opacity, used for disabled (45 percent) and dimmed rows.
    pub fn faded(color: Color32, opacity: f32) -> Color32 {
        color.gamma_multiply(opacity.clamp(0.0, 1.0))
    }
}

/// Colours of the board and schematic painters, separate from chrome.
///
/// Why: swatches in the Layers tree and the canvas share one source so they
/// can never drift apart.
///
/// # Rules
/// - Translucent values (ratsnest) are composited by the painter.
///
/// # Do not
/// - Use for chrome; use `Tokens`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanvasColors {
    /// Canvas background.
    pub background: Color32,
    /// Minor grid lines.
    pub grid_minor: Color32,
    /// Major grid lines.
    pub grid_major: Color32,
    /// Board substrate fill.
    pub board_fill: Color32,
    /// Board edge.
    pub board_edge: Color32,
    /// Top copper.
    pub top_copper: Color32,
    /// Bottom copper.
    pub bottom_copper: Color32,
    /// Silkscreen.
    pub silkscreen: Color32,
    /// Courtyard.
    pub courtyard: Color32,
    /// Ratsnest (70 percent in the dark theme).
    pub ratsnest: Color32,
    /// Selection.
    pub selection: Color32,
    /// Schematic wire.
    pub schematic_wire: Color32,
    /// Schematic symbol body.
    pub symbol_body: Color32,
    /// Schematic pin number.
    pub pin_number: Color32,
    /// Schematic sheet fill.
    pub sheet_fill: Color32,
}

impl CanvasColors {
    /// The dark canvas values.
    pub const DARK: CanvasColors = CanvasColors {
        background: rgb(0x07100D),
        grid_minor: rgb(0x12231C),
        grid_major: rgb(0x1B3329),
        board_fill: rgb(0x0E3B2D),
        board_edge: rgb(0xE6BE5A),
        top_copper: rgb(0xE8834A),
        bottom_copper: rgb(0x4FA3E0),
        silkscreen: rgb(0xEDEBDD),
        courtyard: rgb(0xB58AE0),
        ratsnest: tint(0xE8F0EB, 70),
        selection: rgb(0x5CC8E8),
        schematic_wire: rgb(0x4FCB8C),
        symbol_body: rgb(0xE6BE5A),
        pin_number: rgb(0x5CC8E8),
        sheet_fill: rgb(0x0B1713),
    };
    /// The light canvas values.
    pub const LIGHT: CanvasColors = CanvasColors {
        background: rgb(0xF1F0E6),
        grid_minor: rgb(0xE1E0D3),
        grid_major: rgb(0xCFCEBF),
        board_fill: rgb(0xCFE3D6),
        board_edge: rgb(0x7A5A00),
        top_copper: rgb(0xC2571A),
        bottom_copper: rgb(0x1F6FB5),
        silkscreen: rgb(0x2A2A24),
        courtyard: rgb(0x8A52B8),
        ratsnest: rgb(0x3A4A42),
        selection: rgb(0x0B6785),
        schematic_wire: rgb(0x14663F),
        symbol_body: rgb(0x7A5A00),
        pin_number: rgb(0x0B6785),
        sheet_fill: rgb(0xFBFAF3),
    };

    /// The canvas colours for `Preferences.dark`.
    pub fn for_dark(dark: bool) -> CanvasColors {
        if dark { Self::DARK } else { Self::LIGHT }
    }
}

/// Composite a translucent colour over an opaque background.
///
/// Why: contrast is measured on what the eye sees, so tints are flattened first.
pub fn composite(src: Color32, bg: Color32) -> Color32 {
    let keep = 1.0 - f32::from(src.a()) / 255.0;
    let mix = |s: u8, d: u8| (f32::from(s) + f32::from(d) * keep).round().min(255.0) as u8;
    Color32::from_rgb(
        mix(src.r(), bg.r()),
        mix(src.g(), bg.g()),
        mix(src.b(), bg.b()),
    )
}

/// Build the egui `Visuals` of one theme from its tokens.
///
/// # Rules
/// - `expansion` is 0 on every widget state so hover never grows a widget.
/// - No shadows, no striping; borders are 1 px.
///
/// # Dependencies
/// - crates/ui-egui/src/theme.rs (`Tokens`).
///
/// # Do not
/// - Read app state; keep this pure so tests can build it.
pub fn visuals(t: &Tokens) -> egui::Visuals {
    let mut v = if t.is_dark() {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.dark_mode = t.is_dark();
    v.override_text_color = Some(t.text);
    v.panel_fill = t.panel;
    v.window_fill = t.panel;
    v.extreme_bg_color = t.raised;
    v.faint_bg_color = Tokens::faded(t.raised, 0.5);
    v.code_bg_color = t.raised;
    v.hyperlink_color = t.cyan;
    v.warn_fg_color = t.gold;
    v.error_fg_color = t.red;
    v.selection.bg_fill = t.copper_tint;
    v.selection.stroke = Stroke::new(BORDER, t.copper);
    v.window_corner_radius = CornerRadius::same(RADIUS_CARD);
    v.menu_corner_radius = CornerRadius::same(RADIUS_CONTROL);
    v.window_stroke = Stroke::new(BORDER, t.edge);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.striped = false;

    let control = CornerRadius::same(RADIUS_CONTROL);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.panel;
    w.noninteractive.weak_bg_fill = t.panel;
    w.noninteractive.bg_stroke = Stroke::new(BORDER, t.line);
    w.noninteractive.fg_stroke = Stroke::new(BORDER, t.text);
    w.noninteractive.corner_radius = control;

    w.inactive.bg_fill = t.raised;
    w.inactive.weak_bg_fill = t.raised;
    w.inactive.bg_stroke = Stroke::new(BORDER, t.edge);
    w.inactive.fg_stroke = Stroke::new(BORDER, t.text);
    w.inactive.corner_radius = control;

    w.hovered.bg_fill = t.hover;
    w.hovered.weak_bg_fill = t.hover;
    w.hovered.bg_stroke = Stroke::new(BORDER, t.edge);
    w.hovered.fg_stroke = Stroke::new(BORDER, t.text);
    w.hovered.corner_radius = control;

    w.active.bg_fill = t.raised;
    w.active.weak_bg_fill = t.raised;
    w.active.bg_stroke = Stroke::new(BORDER, t.copper);
    w.active.fg_stroke = Stroke::new(BORDER, t.text);
    w.active.corner_radius = control;

    w.open = w.active;
    for state in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        state.expansion = 0.0;
    }
    v
}

/// Set the spacing numbers of the design on a style.
///
/// # Rules
/// - Item spacing 8 by 4, button padding 12 by 4, interact size 28, indent 16.
pub fn apply_spacing(style: &mut egui::Style) {
    let s = &mut style.spacing;
    s.item_spacing = Vec2::new(8.0, 4.0);
    s.button_padding = Vec2::new(12.0, 4.0);
    s.interact_size = Vec2::new(CONTROL_HEIGHT, CONTROL_HEIGHT);
    s.window_margin = Margin::same(16);
    s.menu_margin = Margin::same(4);
    s.indent = 16.0;
    s.icon_width = 16.0;
    s.tooltip_width = 360.0;
}

const TOKENS_KEY: &str = "pcbcraft-ui-tokens";

/// The tokens most recently passed to `apply`, or the dark or light set that
/// matches the context's visuals when `apply` has not run.
///
/// Why: kit widgets read colours from here, so a page never threads tokens through.
pub fn tokens(ctx: &egui::Context) -> Tokens {
    ctx.data(|d| d.get_temp::<Tokens>(egui::Id::new(TOKENS_KEY)))
        .unwrap_or_else(|| Tokens::for_dark(ctx.global_style().visuals.dark_mode))
}

/// Apply the tokens to a whole egui context, for the theme of `tokens`.
///
/// Why: one call switches colours, spacing and radii together.
///
/// # Rules
/// - Sets the style for the theme named by `tokens`, then makes it the active theme.
///
/// # Do not
/// - Call from widget code; the shell calls it when `Preferences.dark` changes.
pub fn apply(ctx: &egui::Context, tokens: &Tokens) {
    let theme = if tokens.is_dark() {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    let visuals = visuals(tokens);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(TOKENS_KEY), *tokens));
    ctx.set_theme(egui::ThemePreference::from(theme));
    ctx.style_mut_of(theme, |style| {
        apply_spacing(style);
        crate::fonts::apply_text_styles(style);
        style.visuals = visuals;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contrast::contrast_ratio;

    const TEXT: f64 = 4.5;

    fn both() -> [Tokens; 2] {
        [Tokens::DARK, Tokens::LIGHT]
    }

    /// The three pairs named by the ticket reach 4.5 to 1 in both themes.
    #[test]
    fn named_text_pairs_reach_4_5_to_1_in_both_themes() {
        for t in both() {
            let name = format!("{:?}", t.theme);
            assert!(contrast_ratio(t.text, t.panel) >= TEXT, "{name} text/panel");
            assert!(
                contrast_ratio(t.muted, t.panel) >= TEXT,
                "{name} muted/panel"
            );
            assert!(
                contrast_ratio(t.onacc, t.copper) >= TEXT,
                "{name} onacc/copper"
            );
        }
    }

    /// Copper text on its own tint over the panel stays readable.
    #[test]
    fn copper_text_on_its_tint_reaches_4_5_to_1() {
        for t in both() {
            let tinted = composite(t.copper_tint, t.panel);
            assert!(contrast_ratio(t.copper, tinted) >= TEXT, "{:?}", t.theme);
        }
    }

    /// `apply` writes the design numbers into the egui style.
    #[test]
    fn apply_sets_spacing_and_visuals() {
        for t in both() {
            let ctx = egui::Context::default();
            apply(&ctx, &t);
            let style = ctx.style_of(if t.is_dark() {
                egui::Theme::Dark
            } else {
                egui::Theme::Light
            });
            assert_eq!(style.spacing.item_spacing, Vec2::new(8.0, 4.0));
            assert_eq!(style.spacing.button_padding, Vec2::new(12.0, 4.0));
            assert_eq!(style.spacing.interact_size, Vec2::new(28.0, 28.0));
            assert_eq!(style.spacing.indent, 16.0);
            assert_eq!(style.visuals.panel_fill, t.panel);
            assert_eq!(style.visuals.window_fill, t.panel);
            assert_eq!(style.visuals.extreme_bg_color, t.raised);
            assert_eq!(style.visuals.hyperlink_color, t.cyan);
            assert_eq!(style.visuals.window_corner_radius, CornerRadius::same(6));
            assert_eq!(style.visuals.window_shadow, Shadow::NONE);
            assert_eq!(style.visuals.dark_mode, t.is_dark());
            assert_eq!(style.visuals.widgets.hovered.expansion, 0.0);
            assert_eq!(style.visuals.widgets.inactive.weak_bg_fill, t.raised);
        }
    }

    /// Preferences.dark picks the theme.
    #[test]
    fn for_dark_picks_the_theme() {
        assert_eq!(Tokens::for_dark(true), Tokens::DARK);
        assert_eq!(Tokens::for_dark(false), Tokens::LIGHT);
        assert_eq!(CanvasColors::for_dark(true), CanvasColors::DARK);
    }
}
