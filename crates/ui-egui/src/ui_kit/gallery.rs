//! The component gallery: every kit component, every state, drawn on one page.
//!
//! Context: the `ui_kit_gallery` example runs this under eframe and captures a
//! PNG per theme; the unit tests run the same page headless.
//! Local rules: the gallery only calls public kit helpers and `theme`; it holds
//! no app state beyond `GalleryState`.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/examples/ui_kit_gallery.rs
//!
//! # Do not
//! - Add a component here that the kit does not export.
use super::button::{self, Kind, Look, Size, State};
use super::card;
use super::chip::{self, BadgeKind};
use super::dialog::{self, Dialog, Outcome};
use super::empty_state;
use super::form_row::{self, FormWidth};
use super::icons;
use super::number_field::{self, NumberSpec};
use super::panel_header;
use super::rows::{self, Finding, Severity, TreeRow};
use super::segmented::{self, Segment};
use super::select;
use super::status_item;
use super::tab_bar::{self, TabItem, TabSize};
use crate::fonts;
use crate::theme::{CanvasColors, Tokens};
use egui::{Color32, Sense, Stroke, Vec2};

/// Which gallery page to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    /// Colour roles and canvas colours.
    Swatches,
    /// The five-size type scale in both families.
    Type,
    /// Buttons in every kind, size and state, plus chips and badges.
    Buttons,
    /// Segmented control, tab bars, panel header, card, empty state, status items.
    Tabs,
    /// List, tree and finding rows, form rows, number field, select, checkbox.
    Rows,
    /// A modal dialog with tabs, form rows and a footer, over a dimmed page.
    Dialog,
}

impl Page {
    /// The page named on the command line, or `None`.
    pub fn from_name(name: &str) -> Option<Page> {
        match name {
            "swatches" => Some(Page::Swatches),
            "type" => Some(Page::Type),
            "buttons" => Some(Page::Buttons),
            "tabs" => Some(Page::Tabs),
            "rows" => Some(Page::Rows),
            "dialog" => Some(Page::Dialog),
            _ => None,
        }
    }
}

/// State the gallery keeps between frames.
#[derive(Clone, Debug)]
pub struct GalleryState {
    /// Tokens of the theme on show.
    pub tokens: Tokens,
    /// The page on show.
    pub page: Page,
    /// Selected index of each demo tab bar (nav, aside, sheet).
    pub tab: [usize; 3],
    /// Selected index of each demo segmented control.
    pub seg: [usize; 3],
    /// Frames shown, used to put keyboard focus on one demo control once.
    pub frames: u32,
    /// Demo values of the Properties form: x, y, rotation.
    pub xyz: [f64; 3],
    /// Demo footprint choice.
    pub footprint: usize,
    /// Demo checkboxes: parts list, placement file, lock.
    pub flags: [bool; 3],
    /// Demo selected list row.
    pub row: usize,
    /// Demo track width in mm.
    pub track: f64,
    /// The demo dialog is open.
    pub dialog_open: bool,
    /// Selected tab of the demo dialog.
    pub dialog_tab: usize,
}

impl GalleryState {
    /// A gallery for one theme.
    pub fn new(dark: bool) -> Self {
        Self {
            tokens: Tokens::for_dark(dark),
            page: Page::Swatches,
            tab: [1, 0, 0],
            seg: [0, 0, 1],
            frames: 0,
            xyz: [44.0, 20.0, 0.0],
            footprint: 1,
            flags: [true, true, false],
            row: 1,
            track: 0.25,
            dialog_open: true,
            dialog_tab: 0,
        }
    }
}

fn swatch(ui: &mut egui::Ui, t: &Tokens, name: &str, color: Color32) {
    ui.vertical(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(96.0, 40.0), Sense::hover());
        ui.painter().rect_filled(rect, 4.0, color);
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, t.line),
            egui::StrokeKind::Inside,
        );
        ui.label(fonts::caption(name).color(t.muted));
    });
}

fn swatch_rows(ui: &mut egui::Ui, t: &Tokens, items: &[(&str, Color32)]) {
    for row in items.chunks(9) {
        ui.horizontal(|ui| {
            for (name, color) in row {
                swatch(ui, t, name, *color);
            }
        });
        ui.add_space(8.0);
    }
}

/// Draw the colour swatch page for the tokens in `state`.
///
/// Why: the first visual proof that both themes carry the right values.
pub fn swatches(ui: &mut egui::Ui, state: &GalleryState) {
    let t = &state.tokens;
    ui.label(fonts::title(format!("Colour roles, {:?}", t.theme)));
    ui.add_space(8.0);
    let roles: [(&str, Color32); 18] = [
        ("ground", t.ground),
        ("panel", t.panel),
        ("raised", t.raised),
        ("hover", t.hover),
        ("line", t.line),
        ("edge", t.edge),
        ("text", t.text),
        ("muted", t.muted),
        ("copper", t.copper),
        ("mask", t.mask),
        ("gold", t.gold),
        ("cyan", t.cyan),
        ("red", t.red),
        ("onacc", t.onacc),
        ("copper tint", t.copper_tint),
        ("gold tint", t.gold_tint),
        ("red tint", t.red_tint),
        ("mask tint", t.mask_tint),
    ];
    swatch_rows(ui, t, &roles);
    ui.add_space(16.0);
    ui.label(fonts::title("Canvas colours"));
    ui.add_space(8.0);
    let c = CanvasColors::for_dark(t.is_dark());
    let canvas: [(&str, Color32); 15] = [
        ("background", c.background),
        ("grid minor", c.grid_minor),
        ("grid major", c.grid_major),
        ("board fill", c.board_fill),
        ("board edge", c.board_edge),
        ("top copper", c.top_copper),
        ("bottom copper", c.bottom_copper),
        ("silkscreen", c.silkscreen),
        ("courtyard", c.courtyard),
        ("ratsnest", c.ratsnest),
        ("selection", c.selection),
        ("wire", c.schematic_wire),
        ("symbol body", c.symbol_body),
        ("pin number", c.pin_number),
        ("sheet fill", c.sheet_fill),
    ];
    swatch_rows(ui, t, &canvas);
}

/// Draw the type scale page: every role, with its size and weight named.
///
/// Why: proof that the bundled fonts load and that only the five sizes exist.
pub fn type_scale(ui: &mut egui::Ui, state: &GalleryState) {
    let t = &state.tokens;
    ui.label(fonts::title(format!("Type scale, {:?}", t.theme)));
    ui.add_space(8.0);
    let rows: [(&str, fonts::Role, &str); 7] = [
        (
            "Display 22 / 28, 600",
            fonts::Role::Display,
            "Layout and Manufacture",
        ),
        (
            "Title 16 / 24, 600",
            fonts::Role::Title,
            "Board outline and stackup",
        ),
        (
            "Strong 13 / 20, 600",
            fonts::Role::Strong,
            "VCC is not fully routed",
        ),
        (
            "Button 13 / 20, 500",
            fonts::Role::Button,
            "Run checks   Update layout",
        ),
        (
            "Body 13 / 20, 400",
            fonts::Role::Body,
            "Copper does not join J1 pin 1 and R1 pin 1.",
        ),
        (
            "Mono 12 / 16, 400",
            fonts::Role::Mono,
            "44.00 mm   R1 330 \u{3a9}   GND   gerber-top.gbr",
        ),
        (
            "Caption 11 / 16, 400",
            fonts::Role::Caption,
            "Include in parts list, placement file",
        ),
    ];
    for (name, role, sample) in rows {
        ui.horizontal(|ui| {
            cell(ui, 200.0, 28.0, |ui| {
                ui.label(fonts::caption(name).color(t.muted));
            });
            ui.label(fonts::text(role, sample));
        });
        ui.add_space(4.0);
    }
    ui.add_space(8.0);
    ui.label(fonts::section_caption("General").color(t.muted));
    ui.label(fonts::caption("Section caption: 11, 600, uppercase").color(t.muted));
}

/// A fixed-size, vertically centred cell, so columns line up whatever is inside.
fn cell(ui: &mut egui::Ui, width: f32, height: f32, f: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(Vec2::new(width, height));
            f(ui);
        },
    );
}

const STATES: [(&str, State); 5] = [
    ("Rest", State::Rest),
    ("Hover", State::Hover),
    ("Active", State::Active),
    ("Disabled", State::Disabled),
    ("Focus", State::Focus),
];

fn state_header(ui: &mut egui::Ui, t: &Tokens) {
    ui.horizontal(|ui| {
        cell(ui, 96.0, 20.0, |_| {});
        for (name, _) in STATES {
            cell(ui, 144.0, 20.0, |ui| {
                ui.label(fonts::section_caption(name).color(t.muted));
            });
        }
    });
}

fn state_row(ui: &mut egui::Ui, t: &Tokens, name: &str, look: Look<'_>) {
    ui.horizontal(|ui| {
        cell(ui, 96.0, 36.0, |ui| {
            ui.label(fonts::caption(name).color(t.muted));
        });
        for (_, state) in STATES {
            cell(ui, 144.0, 36.0, |ui| {
                ui.add_space(4.0);
                button::static_button(ui, look, state);
            });
        }
    });
}

/// Draw the buttons page: every kind in every state, three sizes, a live row,
/// chips and badges.
///
/// Why: the visual proof for ticket #761.
pub fn buttons(ui: &mut egui::Ui, state: &GalleryState) {
    let t = &state.tokens;
    ui.label(fonts::title(format!("Buttons, {:?}", t.theme)));
    ui.add_space(8.0);
    state_header(ui, t);
    let plus = Some(icons::PLUS);
    let rows: [(&str, Look<'_>); 7] = [
        (
            "Primary",
            Look {
                kind: Kind::Primary,
                size: Size::Normal,
                icon: None,
                label: "Save",
            },
        ),
        (
            "Primary, icon",
            Look {
                kind: Kind::Primary,
                size: Size::Normal,
                icon: plus,
                label: "Add part",
            },
        ),
        (
            "Secondary",
            Look {
                kind: Kind::Secondary,
                size: Size::Normal,
                icon: None,
                label: "Cancel",
            },
        ),
        (
            "Secondary, icon",
            Look {
                kind: Kind::Secondary,
                size: Size::Normal,
                icon: Some(icons::FIT),
                label: "Fit",
            },
        ),
        (
            "Ghost",
            Look {
                kind: Kind::Ghost,
                size: Size::Normal,
                icon: None,
                label: "Hide",
            },
        ),
        (
            "Icon",
            Look {
                kind: Kind::Icon,
                size: Size::Normal,
                icon: Some(icons::MORE),
                label: "",
            },
        ),
        (
            "Icon, zoom",
            Look {
                kind: Kind::Icon,
                size: Size::Normal,
                icon: Some(icons::ZOOM_IN),
                label: "",
            },
        ),
    ];
    for (name, look) in rows {
        state_row(ui, t, name, look);
    }
    ui.add_space(16.0);
    ui.label(fonts::title("Sizes"));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        for (name, size) in [
            ("Small 24", Size::Small),
            ("Normal 28", Size::Normal),
            ("Large 32", Size::Large),
        ] {
            ui.vertical(|ui| {
                ui.label(fonts::caption(name).color(t.muted));
                button::primary_button(
                    ui,
                    None,
                    "Update layout",
                    "Bring the board in line with the schematic",
                    size,
                    true,
                );
                button::secondary_button(
                    ui,
                    None,
                    "Run checks",
                    "Check the board against its rules",
                    size,
                    true,
                );
                button::ghost_button(
                    ui,
                    None,
                    "Mute",
                    "Hide this finding until the next run",
                    size,
                    true,
                );
                button::icon_button(ui, icons::PLUS, "Add", "Add a layer", size, true);
            });
            ui.add_space(24.0);
        }
    });
    ui.add_space(16.0);
    ui.label(fonts::title("Chips and badges"));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        chip::chip(ui, "VCC", "Net VCC, press to highlight it");
        chip::chip(ui, "R1.2", "Pin 2 of R1, press to select it");
        chip::chip(ui, "gerber-top.gbr", "Top copper file");
        ui.add_space(16.0);
        chip::badge(ui, "3", BadgeKind::Warning, "Three warnings");
        chip::badge(ui, "0", BadgeKind::Neutral, "No muted findings");
        chip::badge(ui, "Current", BadgeKind::Success, "Output is up to date");
        chip::badge(
            ui,
            "Stale",
            BadgeKind::Warning,
            "Output is older than the board",
        );
        chip::badge(ui, "2 errors", BadgeKind::Error, "Two errors");
        chip::badge(ui, "Active", BadgeKind::Accent, "Active layer");
    });
}

/// Run `f` in a top-down column of fixed width, whatever layout `ui` has.
fn column(ui: &mut egui::Ui, width: f32, f: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(width);
            f(ui);
        },
    );
}

/// Draw the tabs page: segmented controls, the three tab bars, a panel header,
/// a card with form content, an empty state and status items.
///
/// Why: the visual proof for ticket #762; keyboard focus is shown on the first
/// segmented item and on the aside tab so the ring is visible in a screenshot.
pub fn tabs(ui: &mut egui::Ui, state: &mut GalleryState) {
    let t = state.tokens;
    state.frames += 1;
    ui.label(fonts::title(format!(
        "Tabs, segmented, cards, {:?}",
        t.theme
    )));
    ui.add_space(8.0);
    ui.label(fonts::caption("Segmented control").color(t.muted));
    ui.horizontal(|ui| {
        segmented::segmented(
            ui,
            "seg-side",
            &mut state.seg[0],
            &[
                Segment::text("Top", "Show the top side"),
                Segment::text("Bottom", "Show the bottom side"),
            ],
        );
        ui.add_space(16.0);
        segmented::segmented(
            ui,
            "seg-units",
            &mut state.seg[1],
            &[
                Segment::text("mm", "Show millimetres"),
                Segment::text("mil", "Show thousandths of an inch"),
            ],
        );
        ui.add_space(16.0);
        segmented::segmented(
            ui,
            "seg-corners",
            &mut state.seg[2],
            &[
                Segment::text("45", "Cut corners at 45 degrees"),
                Segment::text("90", "Square corners"),
                Segment::text("Free", "Any angle"),
            ],
        );
    });
    if state.frames == 2 {
        let id = segmented::group_id(ui, "seg-side");
        ui.memory_mut(|m| m.request_focus(id));
    }
    ui.add_space(16.0);
    ui.label(fonts::caption("Tab nav (40), aside tabs (40), sheet tabs (36)").color(t.muted));
    let nav = [
        TabItem::new("Files", "Project files"),
        TabItem::new("Schematic", "Draw the circuit"),
        TabItem::new("Layout", "Place and route the board"),
        TabItem::new("3D", "See the board in 3D"),
        TabItem::new("Manufacture", "Make fabrication files"),
    ];
    ui.allocate_ui(Vec2::new(520.0, 40.0), |ui| {
        tab_bar::tab_bar(ui, "nav", &mut state.tab[0], &nav, TabSize::Nav);
    });
    let aside = [
        TabItem {
            icon: Some(icons::EYE),
            ..TabItem::new("Layers", "Layers of the board")
        },
        TabItem {
            icon: Some(icons::SEARCH),
            ..TabItem::new("Library", "Parts library")
        },
    ];
    ui.allocate_ui(Vec2::new(320.0, 40.0), |ui| {
        tab_bar::tab_bar(ui, "aside", &mut state.tab[1], &aside, TabSize::Aside);
    });
    let sheet = [
        TabItem {
            badge: Some(("3".to_owned(), BadgeKind::Warning)),
            ..TabItem::new("Findings", "Problems found by the checks")
        },
        TabItem::new("Output", "Messages from long jobs"),
        TabItem {
            badge: Some(("12".to_owned(), BadgeKind::Neutral)),
            ..TabItem::new("Console", "Commands and their results")
        },
    ];
    ui.allocate_ui(Vec2::new(420.0, 36.0), |ui| {
        tab_bar::tab_bar(ui, "sheet", &mut state.tab[2], &sheet, TabSize::Sheet);
    });
    if state.frames == 2 {
        let id = tab_bar::tab_id(ui, "aside", 0);
        ui.memory_mut(|m| m.request_focus(id));
    }
    ui.add_space(16.0);
    ui.horizontal_top(|ui| {
        column(ui, 360.0, |ui| {
            ui.label(fonts::caption("Panel header (36)").color(t.muted));
            panel_header::panel_header(ui, "Layers", |ui| {
                button::icon_button(
                    ui,
                    icons::PLUS,
                    "Add layer",
                    "Add a layer",
                    Size::Small,
                    true,
                );
            });
            ui.add_space(24.0);
            ui.label(fonts::caption("Status items (24)").color(t.muted));
            ui.horizontal(|ui| {
                status_item::status_item(
                    ui,
                    Some(icons::OK),
                    "Ready",
                    "The project is saved",
                    false,
                );
                status_item::status_item(
                    ui,
                    Some(icons::ERROR),
                    "0 errors",
                    "Open the findings",
                    false,
                );
                status_item::status_item(
                    ui,
                    Some(icons::WARNING),
                    "3 warnings",
                    "Open the findings",
                    false,
                );
                status_item::status_item(
                    ui,
                    None,
                    "X 44.00 Y 20.00 mm",
                    "Pointer position on the board",
                    true,
                );
            });
        });
        ui.add_space(24.0);
        column(ui, 520.0, |ui| {
            card::card(
                ui,
                "Board outline",
                Some("60 by 40 mm"),
                |ui| {
                    button::ghost_button(
                        ui,
                        None,
                        "Reset",
                        "Restore the default outline",
                        Size::Small,
                        true,
                    );
                },
                |ui| {
                    ui.label(
                        fonts::text(fonts::Role::Body, "Corners are cut at 45 degrees.")
                            .color(t.text),
                    );
                    ui.label(
                        fonts::caption("Change the corner style in the Layout tool bar.")
                            .color(t.muted),
                    );
                },
            );
            ui.add_space(16.0);
            card::card(
                ui,
                "Fabrication files",
                None,
                |_| {},
                |ui| {
                    empty_state::empty_state(
                        ui,
                        icons::FIT,
                        "No fabrication files yet",
                        "Plot files appear here once you run the output set.",
                        "Go to Plot files",
                        "Open the Plot files output",
                    );
                },
            );
        });
    });
}

/// Draw the rows page: a Properties panel built from form rows, the Layers tree
/// rows, finding rows and list rows.
///
/// Why: the visual proof for ticket #763, laid out like the mockup's right aside
/// and left aside so the parts can be compared with the approved design.
pub fn rows_page(ui: &mut egui::Ui, state: &mut GalleryState) {
    let t = state.tokens;
    state.frames += 1;
    ui.label(fonts::title(format!("Rows and fields, {:?}", t.theme)));
    ui.add_space(8.0);
    ui.horizontal_top(|ui| {
        column(ui, 340.0, |ui| {
            ui.label(
                fonts::caption("Properties panel: form rows (aside, 104 label)").color(t.muted),
            );
            egui::Frame::new()
                .fill(t.panel)
                .inner_margin(12)
                .show(ui, |ui| {
                    let xy = NumberSpec {
                        unit: "mm",
                        step: 0.5,
                        width: 112.0,
                        ..NumberSpec::default()
                    };
                    let deg = NumberSpec {
                        unit: "deg",
                        step: 15.0,
                        places: 0,
                        min: -360.0,
                        max: 360.0,
                        width: 112.0,
                    };
                    form_row::form_row(ui, FormWidth::Aside, "X", None, None, |ui| {
                        number_field::number_field(
                            ui,
                            "g-x",
                            &mut state.xyz[0],
                            &xy,
                            "X position",
                            "Move the part along X",
                        );
                    });
                    form_row::form_row(ui, FormWidth::Aside, "Y", None, None, |ui| {
                        number_field::number_field(
                            ui,
                            "g-y",
                            &mut state.xyz[1],
                            &xy,
                            "Y position",
                            "Move the part along Y",
                        );
                    });
                    form_row::form_row(ui, FormWidth::Aside, "Rotation", None, None, |ui| {
                        number_field::number_field(
                            ui,
                            "g-r",
                            &mut state.xyz[2],
                            &deg,
                            "Rotation",
                            "Turn the part",
                        );
                    });
                    form_row::form_row(ui, FormWidth::Aside, "Footprint", None, None, |ui| {
                        select::select(
                            ui,
                            "g-fp",
                            &mut state.footprint,
                            &["Resistor 0805", "Resistor 1206", "Resistor 0603"],
                            "Footprint",
                            "Package of the part",
                            168.0,
                        );
                    });
                    form_row::toggle_row(
                        ui,
                        FormWidth::Aside,
                        "Include in parts list",
                        "List this part in the bill of materials",
                        &mut state.flags[0],
                    );
                    form_row::toggle_row(
                        ui,
                        FormWidth::Aside,
                        "Include in placement file",
                        "List this part for the pick and place machine",
                        &mut state.flags[1],
                    );
                    form_row::toggle_row(
                        ui,
                        FormWidth::Aside,
                        "Lock position",
                        "Stop the part from being moved",
                        &mut state.flags[2],
                    );
                });
            ui.add_space(16.0);
            ui.label(
                fonts::caption("Page form row (200 label), with hint and error").color(t.muted),
            );
            egui::Frame::new()
                .fill(t.panel)
                .inner_margin(12)
                .show(ui, |ui| {
                    let w = NumberSpec {
                        unit: "mm",
                        min: 0.05,
                        max: 5.0,
                        step: 0.05,
                        width: 112.0,
                        ..NumberSpec::default()
                    };
                    let mut bad = 9.0;
                    form_row::form_row(
                        ui,
                        FormWidth::Page,
                        "Track width",
                        Some("Default for new tracks"),
                        None,
                        |ui| {
                            number_field::number_field(
                                ui,
                                "g-w",
                                &mut state.track,
                                &w,
                                "Track width",
                                "Default track width",
                            );
                        },
                    );
                    form_row::form_row(
                        ui,
                        FormWidth::Page,
                        "Clearance",
                        None,
                        Some("Enter a value from 0.05 to 5.00"),
                        |ui| {
                            number_field::number_field(
                                ui,
                                "g-bad",
                                &mut bad,
                                &NumberSpec { max: 20.0, ..w },
                                "Clearance",
                                "Smallest gap between copper",
                            );
                        },
                    );
                });
        });
        ui.add_space(24.0);
        column(ui, 340.0, |ui| {
            ui.label(fonts::caption("Layers tree rows (28)").color(t.muted));
            egui::Frame::new().fill(t.panel).show(ui, |ui| {
                let c = CanvasColors::for_dark(t.is_dark());
                let sample = [
                    TreeRow {
                        depth: 0,
                        expanded: Some(true),
                        swatch: Some(c.top_copper),
                        icon: None,
                        name: "Top copper",
                        count: Some("6".into()),
                        locked: Some(false),
                        visible: Some(true),
                        selected: false,
                        dimmed: false,
                    },
                    TreeRow {
                        depth: 1,
                        expanded: Some(false),
                        swatch: None,
                        icon: Some(icons::PLUS),
                        name: "Pads",
                        count: Some("6".into()),
                        locked: None,
                        visible: Some(true),
                        selected: true,
                        dimmed: false,
                    },
                    TreeRow {
                        depth: 1,
                        expanded: Some(false),
                        swatch: None,
                        icon: Some(icons::MINUS),
                        name: "Tracks",
                        count: Some("0".into()),
                        locked: None,
                        visible: Some(true),
                        selected: false,
                        dimmed: true,
                    },
                    TreeRow {
                        depth: 0,
                        expanded: Some(false),
                        swatch: Some(c.bottom_copper),
                        icon: None,
                        name: "Bottom copper",
                        count: Some("0".into()),
                        locked: Some(false),
                        visible: Some(true),
                        selected: false,
                        dimmed: false,
                    },
                    TreeRow {
                        depth: 0,
                        expanded: Some(false),
                        swatch: Some(c.silkscreen),
                        icon: None,
                        name: "Bottom silkscreen",
                        count: Some("0".into()),
                        locked: Some(true),
                        visible: Some(false),
                        selected: false,
                        dimmed: true,
                    },
                ];
                for (i, row) in sample.iter().enumerate() {
                    rows::tree_row(ui, ("g-tree", i), row);
                }
            });
            ui.add_space(16.0);
            ui.label(fonts::caption("List rows (36)").color(t.muted));
            egui::Frame::new().fill(t.panel).show(ui, |ui| {
                for (i, (name, meta)) in [
                    ("J1", "5V input"),
                    ("R1", "330 \u{3a9}"),
                    ("D1", "Green LED"),
                ]
                .into_iter()
                .enumerate()
                {
                    let r = rows::list_row(ui, state.row == i, name, "Select this part", |ui| {
                        ui.label(fonts::strong(name).color(t.text));
                        ui.label(fonts::mono(meta).color(t.muted));
                    });
                    if r.clicked() {
                        state.row = i;
                    }
                }
            });
        });
        ui.add_space(24.0);
        column(ui, 560.0, |ui| {
            ui.label(fonts::caption("Finding rows (48, selected, compact 36)").color(t.muted));
            egui::Frame::new().fill(t.panel).show(ui, |ui| {
                let base = Finding {
                    severity: Severity::Warning,
                    title: "VCC is not fully routed",
                    detail: "Copper does not join J1 pin 1 and R1 pin 1. Top copper.",
                    chip: Some("VCC"),
                    selected: false,
                    compact: false,
                };
                rows::finding_row(ui, &base);
                rows::finding_row(
                    ui,
                    &Finding {
                        title: "LED is not fully routed",
                        chip: Some("LED"),
                        selected: true,
                        ..base
                    },
                );
                rows::finding_row(
                    ui,
                    &Finding {
                        severity: Severity::Error,
                        title: "Track too close",
                        detail: "0.12 mm gap, rule needs 0.15 mm.",
                        chip: Some("GND"),
                        ..base
                    },
                );
                rows::finding_row(
                    ui,
                    &Finding {
                        severity: Severity::Ok,
                        title: "No problems",
                        detail: "All checks passed.",
                        chip: None,
                        ..base
                    },
                );
                rows::finding_row(
                    ui,
                    &Finding {
                        compact: true,
                        ..base
                    },
                );
            });
        });
    });
}

/// Draw the dialog page: a page behind, a dimmed scrim and the Project settings
/// dialog with tabs, a scrolling body and the footer.
///
/// Why: the visual proof for ticket #774; Escape cancels and Enter saves, and a
/// button reopens the dialog so the keys can be tried by hand.
pub fn dialog_page(ui: &mut egui::Ui, state: &mut GalleryState) {
    let t = state.tokens;
    ui.label(fonts::title("Page behind the dialog"));
    ui.add_space(8.0);
    ui.label(
        fonts::text(fonts::Role::Body, "Project settings opens over this page.").color(t.text),
    );
    ui.add_space(8.0);
    if button::secondary_button(
        ui,
        None,
        "Project settings",
        "Open the settings dialog",
        Size::Normal,
        true,
    )
    .clicked()
    {
        state.dialog_open = true;
    }
    if !state.dialog_open {
        return;
    }
    let tabs = [
        TabItem::new("Board", "Board size, corners and stackup"),
        TabItem::new("Rules", "Clearances and widths"),
        TabItem::new("Machines", "Fabrication machines"),
    ];
    let spec = Dialog {
        id: "gallery-project",
        title: "Project settings",
        width: 640.0,
        tabs: &tabs,
        note: Some("Changes apply when you press Save"),
        cancel: Some(("Cancel", "Close without saving")),
        primary: ("Save", "Save the settings and close"),
    };
    let ctx = ui.ctx().clone();
    let mut tab = state.dialog_tab;
    let outcome = dialog::show(&ctx, &spec, &mut tab, |ui| {
        let w = NumberSpec {
            unit: "mm",
            min: 0.05,
            max: 5.0,
            step: 0.05,
            width: 112.0,
            ..NumberSpec::default()
        };
        let mut width = 60.0;
        let mut height = 40.0;
        let big = NumberSpec {
            min: 1.0,
            max: 500.0,
            step: 1.0,
            ..w
        };
        form_row::form_row(
            ui,
            FormWidth::Page,
            "Board width",
            Some("Outside edge"),
            None,
            |ui| {
                number_field::number_field(
                    ui,
                    "d-w",
                    &mut width,
                    &big,
                    "Board width",
                    "Width of the board",
                );
            },
        );
        form_row::form_row(ui, FormWidth::Page, "Board height", None, None, |ui| {
            number_field::number_field(
                ui,
                "d-h",
                &mut height,
                &big,
                "Board height",
                "Height of the board",
            );
        });
        form_row::form_row(ui, FormWidth::Page, "Corners", None, None, |ui| {
            let mut corner = 0;
            segmented::segmented(
                ui,
                "d-corner",
                &mut corner,
                &[
                    Segment::text("45", "Cut corners at 45 degrees"),
                    Segment::text("90", "Square corners"),
                    Segment::text("Free", "Any angle"),
                ],
            );
        });
        form_row::form_row(ui, FormWidth::Page, "Copper layers", None, None, |ui| {
            let mut n = 1;
            select::select(
                ui,
                "d-layers",
                &mut n,
                &["1", "2", "4", "6"],
                "Copper layers",
                "Number of copper layers",
                120.0,
            );
        });
        form_row::form_row(
            ui,
            FormWidth::Page,
            "Track width",
            Some("Default for new tracks"),
            None,
            |ui| {
                let mut v = 0.25;
                number_field::number_field(
                    ui,
                    "d-tw",
                    &mut v,
                    &w,
                    "Track width",
                    "Default track width",
                );
            },
        );
        let mut plated = true;
        form_row::toggle_row(
            ui,
            FormWidth::Page,
            "Plated edge",
            "Plate the board edge with copper",
            &mut plated,
        );
    });
    state.dialog_tab = tab;
    if outcome != Outcome::Open {
        state.dialog_open = false;
    }
}

/// Draw the page named in `state`.
pub fn show(ui: &mut egui::Ui, state: &mut GalleryState) {
    match state.page {
        Page::Swatches => swatches(ui, state),
        Page::Type => type_scale(ui, state),
        Page::Buttons => buttons(ui, state),
        Page::Tabs => tabs(ui, state),
        Page::Rows => rows_page(ui, state),
        Page::Dialog => dialog_page(ui, state),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a11y::audit;
    use crate::theme;

    /// Run every gallery page once, with the accessibility tree on, and return
    /// the interactive nodes as (role, name).
    fn interactive(dark: bool) -> Vec<(egui::accesskit::Role, String)> {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = GalleryState::new(dark);
        theme::apply(&ctx, &state.tokens);
        crate::fonts::install(&ctx);
        let mut nodes = Vec::new();
        for page in [
            Page::Swatches,
            Page::Type,
            Page::Buttons,
            Page::Tabs,
            Page::Rows,
            Page::Dialog,
        ] {
            state.page = page;
            for pass in 0..2 {
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(1280.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| show(ui, &mut state),
                );
                if pass == 1 {
                    nodes.extend(audit::interactive_nodes(&out));
                }
                out.textures_delta.clear();
            }
        }
        nodes
    }

    /// Every live control in the gallery announces a name, in both themes.
    #[test]
    fn every_kit_control_has_an_accessible_name() {
        for dark in [true, false] {
            let nodes = interactive(dark);
            assert!(nodes.len() >= 15, "only {} interactive nodes", nodes.len());
            for (role, name) in nodes {
                assert!(!name.is_empty(), "{role:?} has no accessible name");
            }
        }
    }
}
