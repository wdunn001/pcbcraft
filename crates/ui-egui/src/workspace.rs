//! Workspaces: one contract per work area (Files, Schematic, Layout,
//! Manufacture) that says what each region of the window holds.
//!
//! Context: issue #795, epic #758. The shell (menu bar, tab row, status bar)
//! is fixed; everything between is a region the workspace fills: toolbar,
//! left and bottom region tabs (the right aside is still drawn by
//! `App::inspector`; moving it here is a follow-up), and the center. What is right for the
//! Schematic is not right for the Layout, so each workspace declares its own
//! region tabs and its default [`RegionLayout`].
//!
//! Local rules: other branches plug in by adding a body arm for their region
//! tab id (`layers_tree.rs` under `"layers"`, `library_dock_body` under
//! `"library"`); they never edit the shell. A user's layout is stored per
//! workspace in Preferences and View > Reset layout restores the defaults.

use super::*;
use serde::{Deserialize, Serialize};

/// One tab of a region: a stable id and its words.
#[derive(Clone, Copy)]
pub(crate) struct RegionTab {
    pub(crate) id: &'static str,
    pub(crate) label: fn() -> String,
    pub(crate) tip: fn() -> String,
}

/// What the user chose for the regions of one workspace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RegionLayout {
    /// Id of the showing left tab.
    pub(crate) left_tab: String,
    /// Id of the showing bottom tab.
    pub(crate) bottom_tab: String,
    /// True when the bottom sheet body is open, false for its tab strip only.
    pub(crate) bottom_open: bool,
    /// Layout workspace only: the center shows the 3D view instead of the 2D board.
    #[serde(default)]
    pub(crate) view_3d: bool,
    /// Layout workspace only: look at the bottom side of the board in 2D.
    #[serde(default)]
    pub(crate) bottom_side: bool,
}

impl RegionLayout {
    fn new(left: &str, bottom: &str) -> Self {
        Self {
            left_tab: left.to_owned(),
            bottom_tab: bottom.to_owned(),
            bottom_open: false,
            view_3d: false,
            bottom_side: false,
        }
    }
}

/// The contract of a work area.
///
/// # Rules
/// - `left_tabs` and `bottom_tabs` list the region tabs in order; the first is the default.
/// - `left_body` draws the tab with that id and ignores ids it does not list.
/// - `center` draws everything between the toolbar and the bottom sheet.
///
/// # Implementations
/// - `crates/ui-egui/src/workspace.rs` holds `FilesWorkspace`, `SchematicWorkspace`,
///   `LayoutWorkspace`, and `ManufactureWorkspace`.
///
/// # Do not
/// Put document logic here; call `App` methods.
pub(crate) trait Workspace: Sync {
    /// Stable key for Preferences.
    fn id(&self) -> &'static str;
    fn toolbar(&self, app: &mut App, ui: &mut egui::Ui);
    fn left_tabs(&self) -> &'static [RegionTab];
    fn bottom_tabs(&self) -> &'static [RegionTab] {
        &BOTTOM_TABS
    }
    fn left_body(&self, app: &mut App, tab: &str, ui: &mut egui::Ui);
    fn center(&self, app: &mut App, ui: &mut egui::Ui);
    fn default_layout(&self) -> RegionLayout;
}

const BOTTOM_TABS: [RegionTab; 3] = [
    RegionTab {
        id: "findings",
        label: || t!("sheet.tab_findings"),
        tip: || t!("sheet.tab_findings.tip"),
    },
    RegionTab {
        id: "output",
        label: || t!("sheet.tab_output"),
        tip: || t!("sheet.tab_output.tip"),
    },
    RegionTab {
        id: "console",
        label: || t!("sheet.tab_console"),
        tip: || t!("sheet.tab_console.tip"),
    },
];
const LAYERS_TAB: RegionTab = RegionTab {
    id: "layers",
    label: || t!("library.browser.tab_layers"),
    tip: || t!("library.browser.tab_layers.tip"),
};
const LIBRARY_TAB: RegionTab = RegionTab {
    id: "library",
    label: || t!("library.browser.tab_library"),
    tip: || t!("library.browser.tab_library.tip"),
};
const DRAWING_TABS: [RegionTab; 2] = [LAYERS_TAB, LIBRARY_TAB];
const LIBRARY_ONLY: [RegionTab; 1] = [LIBRARY_TAB];
const MANUFACTURE_TABS: [RegionTab; 2] = [
    RegionTab {
        id: "outputs",
        label: || t!("workspace.tab_outputs"),
        tip: || t!("workspace.tab_outputs.tip"),
    },
    RegionTab {
        id: "machines",
        label: || t!("workspace.tab_machines"),
        tip: || t!("workspace.tab_machines.tip"),
    },
];

/// Left bodies shared by the drawing workspaces.
fn drawing_left(app: &mut App, tab: &str, ui: &mut egui::Ui) {
    match tab {
        "layers" => app.left_layers_tab(ui),
        "library" => app.left_library_tab(ui),
        _ => {}
    }
}

/// Files: project overview.
pub(crate) struct FilesWorkspace;
/// Schematic: the sheet.
pub(crate) struct SchematicWorkspace;
/// Layout: the board, in 2D or 3D.
pub(crate) struct LayoutWorkspace;
/// Manufacture: outputs and machines.
pub(crate) struct ManufactureWorkspace;

impl Workspace for FilesWorkspace {
    // Contract: this file, `Workspace`.
    fn id(&self) -> &'static str {
        "files"
    }
    fn toolbar(&self, _app: &mut App, _ui: &mut egui::Ui) {}
    fn left_tabs(&self) -> &'static [RegionTab] {
        &LIBRARY_ONLY
    }
    fn left_body(&self, app: &mut App, tab: &str, ui: &mut egui::Ui) {
        drawing_left(app, tab, ui);
    }
    fn center(&self, app: &mut App, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| App::padded(ui, |ui| app.files_tab(ui)));
    }
    fn default_layout(&self) -> RegionLayout {
        RegionLayout::new("library", "findings")
    }
}

impl Workspace for SchematicWorkspace {
    // Contract: this file, `Workspace`.
    fn id(&self) -> &'static str {
        "schematic"
    }
    fn toolbar(&self, app: &mut App, ui: &mut egui::Ui) {
        app.tool_strip(ui);
    }
    fn left_tabs(&self) -> &'static [RegionTab] {
        &DRAWING_TABS
    }
    fn left_body(&self, app: &mut App, tab: &str, ui: &mut egui::Ui) {
        drawing_left(app, tab, ui);
    }
    fn center(&self, app: &mut App, ui: &mut egui::Ui) {
        app.canvas(ui);
    }
    fn default_layout(&self) -> RegionLayout {
        RegionLayout::new("layers", "findings")
    }
}

impl Workspace for LayoutWorkspace {
    // Contract: this file, `Workspace`. Both the 2D board and the 3D view live
    // here; `App::tab` is `Tab::Pcb` for 2D and `Tab::Board3d` for 3D.
    fn id(&self) -> &'static str {
        "layout"
    }
    fn toolbar(&self, app: &mut App, ui: &mut egui::Ui) {
        if app.tab == Tab::Board3d {
            app.view3d_controls(ui);
        } else {
            app.tool_strip(ui);
        }
    }
    fn left_tabs(&self) -> &'static [RegionTab] {
        &DRAWING_TABS
    }
    fn left_body(&self, app: &mut App, tab: &str, ui: &mut egui::Ui) {
        drawing_left(app, tab, ui);
    }
    fn center(&self, app: &mut App, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        if app.tab == Tab::Board3d {
            app.board3d_tab(ui);
        } else {
            app.canvas(ui);
        }
        app.layout_view_toggles(ui.ctx(), rect);
    }
    fn default_layout(&self) -> RegionLayout {
        RegionLayout::new("layers", "findings")
    }
}

impl Workspace for ManufactureWorkspace {
    // Contract: this file, `Workspace`.
    fn id(&self) -> &'static str {
        "manufacture"
    }
    fn toolbar(&self, _app: &mut App, _ui: &mut egui::Ui) {}
    fn left_tabs(&self) -> &'static [RegionTab] {
        &MANUFACTURE_TABS
    }
    fn left_body(&self, app: &mut App, tab: &str, ui: &mut egui::Ui) {
        let words = match tab {
            "outputs" => t!("workspace.outputs_hint"),
            "machines" => t!("workspace.machines_hint"),
            _ => return,
        };
        ui.label(egui::RichText::new(words).color(app.muted()));
    }
    fn center(&self, app: &mut App, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            App::margined(ui, |ui| {
                app.drill_table_section(ui);
                ui.add_space(12.0);
                let theme = plot_viewer::Theme {
                    surface: app.surface(),
                    muted: app.muted(),
                    accent: app.accent(),
                };
                plot_viewer::ui(
                    &mut app.plot_viewer,
                    ui,
                    theme,
                    &mut app.message,
                    app.make_dir.trim(),
                );
            });
        });
    }
    fn default_layout(&self) -> RegionLayout {
        RegionLayout::new("outputs", "output")
    }
}

static FILES: FilesWorkspace = FilesWorkspace;
static SCHEMATIC: SchematicWorkspace = SchematicWorkspace;
static LAYOUT: LayoutWorkspace = LayoutWorkspace;
static MANUFACTURE: ManufactureWorkspace = ManufactureWorkspace;

/// The workspace that owns a tab, or `None` for the pages (Rules, Settings,
/// Libraries, Checks) that are not work areas.
pub(crate) fn workspace_for(tab: Tab) -> Option<&'static dyn Workspace> {
    match tab {
        Tab::Files => Some(&FILES),
        Tab::Schematic => Some(&SCHEMATIC),
        Tab::Pcb | Tab::Board3d => Some(&LAYOUT),
        Tab::PlotFiles => Some(&MANUFACTURE),
        _ => None,
    }
}

impl App {
    /// The workspace whose regions are showing; pages borrow Files' regions.
    pub(crate) fn workspace(&self) -> &'static dyn Workspace {
        workspace_for(self.tab).unwrap_or(&FILES)
    }

    /// The saved region layout of the current workspace, created from the
    /// workspace default on first use.
    pub(crate) fn region(&mut self) -> &mut RegionLayout {
        let ws = self.workspace();
        self.preferences
            .layouts
            .entry(ws.id().to_owned())
            .or_insert_with(|| ws.default_layout())
    }

    /// View > Reset layout: forget every saved region layout.
    pub(crate) fn reset_layout(&mut self) {
        self.preferences.layouts.clear();
        self.sheet = bottom_sheet::SheetState::default();
        self.message = t!("workspace.layout_reset");
    }

    /// The full-width tool bar under the tab nav, for the workspaces that have one.
    ///
    /// # Rules
    /// - Files and Manufacture have no tool bar, so no panel is drawn for them.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/toolbar.rs
    pub(crate) fn toolbar_panel(&mut self, ui: &mut egui::Ui) {
        if !matches!(self.tab, Tab::Schematic | Tab::Pcb | Tab::Board3d) {
            return;
        }
        let tokens = crate::theme::tokens(ui.ctx());
        egui::Panel::top("tool-bar")
            .exact_size(crate::theme::BAR_HEIGHT)
            .frame(
                egui::Frame::NONE
                    .fill(tokens.panel)
                    .stroke(Stroke::new(crate::theme::BORDER, tokens.line)),
            )
            .show(ui, |ui| {
                self.workspace().toolbar(self, ui);
            });
    }

    /// The 2D | 3D and Top | Bottom toggles, a floating kit card just inside the
    /// canvas, below the top ruler and right of the left ruler.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/segmented.rs
    /// - crates/ui-egui/src/canvas.rs (`RULER_SIZE`)
    ///
    /// # Do not
    /// - Place it at the canvas corner; the rulers draw labels there.
    fn layout_view_toggles(&mut self, ctx: &egui::Context, rect: Rect) {
        use crate::ui_kit::segmented::{Segment, segmented};
        let view_3d = self.tab == Tab::Board3d;
        let bottom = self.region().bottom_side;
        let (mut view_idx, mut side_idx) = (usize::from(view_3d), usize::from(bottom));
        let (was_view, was_side) = (view_idx, side_idx);
        let (view_2d_label, view_3d_label) = (t!("workspace.view_2d"), t!("workspace.view_3d"));
        let (view_2d_tip, view_3d_tip) = (t!("workspace.view_2d.tip"), t!("workspace.view_3d.tip"));
        let (top_label, bottom_label) = (t!("workspace.side_top"), t!("workspace.side_bottom"));
        let (top_tip, bottom_tip) = (
            t!("workspace.side_top.tip"),
            t!("workspace.side_bottom.tip"),
        );
        let tokens = crate::theme::tokens(ctx);
        let inset = crate::canvas::RULER_SIZE + crate::theme::SPACE[2];
        egui::Area::new(egui::Id::new("layout-view-toggles"))
            .fixed_pos(rect.left_top() + egui::vec2(inset, inset))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(tokens.panel)
                    .stroke(Stroke::new(crate::theme::BORDER, tokens.edge))
                    .corner_radius(egui::CornerRadius::same(crate::theme::RADIUS_CARD))
                    .inner_margin(egui::Margin::same(4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = crate::theme::SPACE[1];
                            segmented(
                                ui,
                                "layout-view-2d3d",
                                &mut view_idx,
                                &[
                                    Segment {
                                        label: &view_2d_label,
                                        tip: &view_2d_tip,
                                    },
                                    Segment {
                                        label: &view_3d_label,
                                        tip: &view_3d_tip,
                                    },
                                ],
                            );
                            if view_idx == 0 {
                                segmented(
                                    ui,
                                    "layout-view-side",
                                    &mut side_idx,
                                    &[
                                        Segment {
                                            label: &top_label,
                                            tip: &top_tip,
                                        },
                                        Segment {
                                            label: &bottom_label,
                                            tip: &bottom_tip,
                                        },
                                    ],
                                );
                            }
                        });
                    });
            });
        if view_idx != was_view || side_idx != was_side {
            let r = self.region();
            r.view_3d = view_idx == 1;
            r.bottom_side = side_idx == 1;
            if view_idx != was_view {
                self.change_tab(if view_idx == 1 {
                    Tab::Board3d
                } else {
                    Tab::Pcb
                });
            }
        }
    }

    /// The 3D controls that replace the Layout toolbar in 3D.
    fn view3d_controls(&mut self, ui: &mut egui::Ui) {
        use std::f64::consts::FRAC_PI_2;
        // Like the tool strip, it scrolls sideways in a narrow window.
        egui::ScrollArea::horizontal()
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(Self::PAGE_GUTTER);
                    let v = &mut self.view3d;
                    use crate::ui_kit::button::{Size, secondary_button};
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if secondary_button(
                        ui,
                        None,
                        &t!("workspace.turn"),
                        &t!("workspace.turn.tip"),
                        Size::Normal,
                        true,
                    )
                    .clicked()
                    {
                        v.yaw += FRAC_PI_2;
                    }
                    for (label, tip, yaw, pitch) in [
                        (
                            t!("board.board3d.top_view"),
                            t!("board.board3d.top_view.tip"),
                            -FRAC_PI_2,
                            1.5,
                        ),
                        (
                            t!("workspace.front"),
                            t!("workspace.front.tip"),
                            -FRAC_PI_2,
                            0.0,
                        ),
                        (t!("workspace.side"), t!("workspace.side.tip"), 0.0, 0.0),
                    ] {
                        if secondary_button(ui, None, &label, &tip, Size::Normal, true).clicked() {
                            v.yaw = yaw;
                            v.pitch = pitch;
                        }
                    }
                    if secondary_button(
                        ui,
                        None,
                        &t!("board.board3d.reset_view"),
                        &t!("board.board3d.reset_view.tip"),
                        Size::Normal,
                        true,
                    )
                    .clicked()
                    {
                        v.fit();
                    }
                });
            });
    }
}
