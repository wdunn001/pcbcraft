//! The tool bar row under the tab nav: tool buttons, the Layer and Corners options and
//! the schematic update button, drawn with the kit and never clipped.
//!
//! Context: design system 5.14 and the Layout mockup. One 40 high row; tools that do not
//! fit the width move into a "More" menu, so the row never scrolls or cuts a label.
//! Local rules: the items are data (`Item`), the same list feeds the row and the menu;
//! every control has a tooltip from the catalog; the active tool wears the copper tint.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/segmented.rs, button.rs, checkbox.rs
//! - crates/ui-egui/src/icons.rs (tool icons)
//!
//! # Do not
//! - Add a tool button anywhere else; add an `Item` here.
use super::*;
use crate::fonts::{self, Role};
use crate::theme::{self, Tokens};
use crate::ui_kit::button::{self, Kind, Size};
use crate::ui_kit::icons as kit_icons;
use crate::ui_kit::segmented::{Segment, segmented};
use egui::{CornerRadius, Rect, Sense, Stroke, StrokeKind, Vec2, pos2};

/// What clicking a tool bar item does.
#[derive(Clone, Copy, PartialEq)]
enum Act {
    Select,
    Move,
    PlacePart,
    Wire,
    NoConnect,
    Route,
    Via,
    Keepout,
    Draw(DrawShape),
    DrawShape,
    Cutout,
    Hole,
    Fiducial,
    Measure,
}

/// One entry of the bar.
struct Item {
    id: &'static str,
    icon: &'static str,
    label: String,
    tip: String,
    armed: bool,
    act: Act,
    /// A divider is drawn before this item.
    divider: bool,
}

const PAD: f32 = 10.0;
const ICON_GAP: f32 = 6.0;
const GROUP_GAP: f32 = 8.0;

fn label_width(ui: &egui::Ui, text: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(
            text.to_owned(),
            Role::Strong.font(),
            egui::Color32::PLACEHOLDER,
        )
        .size()
        .x
}

impl Item {
    fn width(&self, ui: &egui::Ui) -> f32 {
        let icon = 16.0 + ICON_GAP;
        let gap = if self.divider {
            2.0 * GROUP_GAP + 1.0
        } else {
            0.0
        };
        2.0 * PAD + icon + label_width(ui, &self.label) + 4.0 + gap
    }
}

impl App {
    /// The toolbar button look: copper tint when active, raised on hover.
    fn toolbar_button(&self, ui: &mut egui::Ui, item: &Item) -> egui::Response {
        let t = theme::tokens(ui.ctx());
        let size = Vec2::new(
            item.width(ui)
                - if item.divider {
                    2.0 * GROUP_GAP + 1.0
                } else {
                    0.0
                },
            theme::CONTROL_HEIGHT,
        );
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let radius = CornerRadius::same(theme::RADIUS_CONTROL);
        let painter = ui.painter();
        let ink = if item.armed {
            painter.rect_filled(rect, radius, t.copper_tint);
            painter.rect_stroke(
                rect,
                radius,
                Stroke::new(theme::BORDER, t.copper),
                StrokeKind::Inside,
            );
            t.copper
        } else {
            if response.hovered() {
                painter.rect_filled(rect, radius, t.raised);
            }
            t.text
        };
        let mut x = rect.min.x + PAD;
        let r = Rect::from_min_size(pos2(x, rect.center().y - 8.0), Vec2::splat(16.0));
        crate::icons::icon_at(ui, r, item.icon, ink);
        x += 16.0 + ICON_GAP;
        let font = if item.armed {
            Role::Strong.font()
        } else {
            Role::Button.font()
        };
        let galley = ui.painter().layout_no_wrap(item.label.clone(), font, ink);
        ui.painter().galley(
            pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            ink,
        );
        crate::a11y::name_control(&response, &item.label, Some(item.armed));
        if response.has_focus() {
            crate::ui_kit::focus::paint_ring(ui.painter(), rect, theme::RADIUS_CONTROL, &t);
        }
        response.on_hover_text(&item.tip)
    }

    fn toolbar_divider(ui: &mut egui::Ui, t: &Tokens) {
        ui.add_space(GROUP_GAP);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 20.0), Sense::hover());
        ui.painter().rect_filled(rect, 0.0, t.line);
        ui.add_space(GROUP_GAP);
    }

    /// The items for the active workspace tab, in bar order.
    fn toolbar_items(&self) -> Vec<Item> {
        use tool_modifiers::{Hotkey, ToolKind};
        let mut out = Vec::new();
        let mut push = |id, icon, label: String, tip: String, armed, act, divider| {
            out.push(Item {
                id,
                icon,
                label,
                tip,
                armed,
                act,
                divider,
            });
        };
        let pcb = self.tab == Tab::Pcb;
        push(
            "tool-select",
            "select",
            t!("board.tool_strip.select"),
            self.tool_tip(
                t!("board.tool_strip.select_objects_on_the_drawing"),
                ToolKind::Select,
                Some(Hotkey::Select),
            ),
            matches!(self.tool, Tool::Select),
            Act::Select,
            false,
        );
        push(
            "tool-move",
            "move",
            t!("board.tool_strip.move"),
            self.tool_tip(
                t!("board.tool_strip.move_tool_drop_the_selection"),
                ToolKind::Move,
                None,
            ),
            matches!(self.tool, Tool::Move),
            Act::Move,
            false,
        );
        if self.tab == Tab::Schematic {
            push(
                "tool-place-part",
                "part",
                t!("library.browser.place_part"),
                t!("library.browser.place_part.tip"),
                self.palette,
                Act::PlacePart,
                true,
            );
            push(
                "tool-wire",
                "wire",
                t!("board.tool_strip.wire"),
                self.tool_tip(
                    t!("board.tool_strip.wire_tool_click_two_pins"),
                    ToolKind::Wire,
                    Some(Hotkey::Wire),
                ),
                matches!(self.tool, Tool::Wire(_)),
                Act::Wire,
                false,
            );
            push(
                "tool-noconnect",
                "no-connect",
                t!("board.tool_strip.no_connect"),
                self.tool_tip(
                    t!("board.tool_strip.mark_a_pin_deliberately_unconnected"),
                    ToolKind::NoConnect,
                    None,
                ),
                matches!(self.tool, Tool::NoConnect),
                Act::NoConnect,
                false,
            );
        }
        if pcb {
            push(
                "tool-route",
                "route",
                t!("board.tool_strip.route"),
                self.tool_tip(
                    t!("board.tool_strip.route_tool_click_two_pads"),
                    ToolKind::Route,
                    Some(Hotkey::Route),
                ),
                matches!(self.tool, Tool::Route(_)),
                Act::Route,
                true,
            );
            push(
                "tool-via",
                "via",
                t!("board.tool_strip.via"),
                self.tool_tip(
                    t!("board.tool_strip.via.tip"),
                    ToolKind::Via,
                    Some(Hotkey::Via),
                ),
                matches!(self.tool, Tool::Via),
                Act::Via,
                false,
            );
            push(
                "tool-keepout",
                "keepout",
                t!("board.tool_strip.keepout"),
                t!("board.tool_strip.keepout.tip"),
                matches!(self.tool, Tool::Keepout),
                Act::Keepout,
                false,
            );
            let shapes = [
                (
                    DrawShape::Line,
                    "tool-line",
                    "line",
                    t!("board.tool_strip.line"),
                    t!("board.tool_strip.draw_a_straight_line_on"),
                ),
                (
                    DrawShape::Rect,
                    "tool-box",
                    "box",
                    t!("board.tool_strip.box"),
                    t!("board.tool_strip.draw_a_box_on_a"),
                ),
                (
                    DrawShape::Circle,
                    "tool-circle",
                    "circle",
                    t!("board.tool_strip.circle"),
                    t!("board.tool_strip.draw_a_circle_on_a"),
                ),
                (
                    DrawShape::Poly,
                    "tool-outline",
                    "board-outline",
                    t!("board.tool_strip.outline"),
                    t!("board.tool_strip.draw_an_outline_on_a"),
                ),
                (
                    DrawShape::Words,
                    "tool-words",
                    "text",
                    t!("board.tool_strip.text"),
                    t!("board.tool_strip.print_words_on_a_user"),
                ),
            ];
            for (i, (shape, id, icon, label, tip)) in shapes.into_iter().enumerate() {
                let armed = matches!(self.tool, Tool::DrawLayer) && self.art_shape == shape;
                let hotkey = (shape == DrawShape::Words).then_some(Hotkey::Text);
                push(
                    id,
                    icon,
                    label,
                    self.tool_tip(tip, ToolKind::DrawLayer, hotkey),
                    armed,
                    Act::Draw(shape),
                    i == 0,
                );
            }
            push(
                "tool-draw-shape",
                "shapes",
                t!("board.tool_strip.draw_shape"),
                t!("board.tool_strip.draw_shape.tip"),
                matches!(self.tool, Tool::DrawLayer),
                Act::DrawShape,
                false,
            );
            push(
                "tool-cutout",
                "cutout",
                t!("board.tool_strip.cutout"),
                self.tool_tip(command_tip("board.cutout.add"), ToolKind::Cutout, None),
                matches!(self.tool, Tool::Cutout),
                Act::Cutout,
                true,
            );
            push(
                "tool-hole",
                "mounting-hole",
                t!("board.tool_strip.mounting_hole"),
                self.tool_tip(
                    t!("board.tool_strip.mounting_hole.tip"),
                    ToolKind::Hole,
                    None,
                ),
                matches!(self.tool, Tool::Hole),
                Act::Hole,
                false,
            );
            push(
                "tool-fiducial",
                "focus",
                t!("board.tool_strip.assembly_mark"),
                self.tool_tip(
                    t!("board.tool_strip.assembly_mark.tip"),
                    ToolKind::Fiducial,
                    None,
                ),
                matches!(self.tool, Tool::Fiducial),
                Act::Fiducial,
                false,
            );
        }
        push(
            "tool-measure",
            "measure",
            t!("toolbar.measure"),
            t!("toolbar.measure.tip"),
            matches!(self.tool, Tool::Measure),
            Act::Measure,
            true,
        );
        out
    }

    fn toolbar_apply(&mut self, act: Act) {
        match act {
            Act::Select => self.tool = Tool::Select,
            Act::Move => self.tool = Tool::Move,
            Act::PlacePart => self.open_part_search(),
            Act::Wire => self.tool = Tool::Wire(None),
            Act::NoConnect => self.tool = Tool::NoConnect,
            Act::Route => self.tool = Tool::Route(None),
            Act::Via => self.tool = Tool::Via,
            Act::Keepout => {
                self.tool = Tool::Keepout;
                self.pours.keepout_from = None;
            }
            Act::Draw(shape) => {
                self.tool = Tool::DrawLayer;
                self.art_shape = shape;
                self.art_start = None;
                self.art_points.clear();
            }
            Act::DrawShape => {
                self.tool = Tool::DrawLayer;
                self.art_shape = DrawShape::Line;
                self.art_start = None;
                self.art_points.clear();
            }
            Act::Cutout => self.tool = Tool::Cutout,
            Act::Hole => self.tool = Tool::Hole,
            Act::Fiducial => self.tool = Tool::Fiducial,
            Act::Measure => self.arm_measure(),
        }
    }

    /// The tool bar: tools that fit, then "More" for the rest, then the options.
    ///
    /// # Rules
    /// - The row never scrolls or clips; items that do not fit go into More.
    /// - Options (Keepout toggles while that tool is active, Corners while Route is
    ///   active) sit at the right.
    ///
    /// # Do not
    /// - Draw a tool outside `toolbar_items`.
    pub(crate) fn toolbar_row(&mut self, ui: &mut egui::Ui) {
        if !matches!(self.tab, Tab::Schematic | Tab::Pcb) {
            return;
        }
        let t = theme::tokens(ui.ctx());
        let items = self.toolbar_items();
        let pcb = self.tab == Tab::Pcb;
        // Width the right-hand options take; the tools get the rest.
        let keepout_on = pcb && matches!(self.tool, Tool::Keepout);
        let route_on = pcb && matches!(self.tool, Tool::Route(_));
        let mut right_w = 0.0;
        if route_on {
            right_w += 16.0 + 56.0 + 110.0 + 24.0;
        }
        if keepout_on {
            right_w += 340.0;
        }
        let more_label = t!("toolbar.more");
        let more_w = 2.0 * PAD + label_width(ui, &more_label) + 16.0 + 8.0;
        let avail = ui.available_width() - Self::PAGE_GUTTER - right_w - 16.0;
        let mut used = 0.0;
        let mut shown = items.len();
        for (i, item) in items.iter().enumerate() {
            used += item.width(ui) + 2.0;
            let reserve = if i + 1 < items.len() { more_w } else { 0.0 };
            if used + reserve > avail {
                shown = i;
                break;
            }
        }
        let mut action = None;
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(Self::PAGE_GUTTER - 8.0);
            for (i, item) in items.iter().take(shown).enumerate() {
                if item.divider && i > 0 {
                    Self::toolbar_divider(ui, &t);
                }
                let r = self.toolbar_button(ui, item);
                self.record_icon(item.id, r.rect);
                if r.clicked() {
                    action = Some(item.act);
                }
            }
            if shown < items.len() {
                let rest = &items[shown..];
                let look = button::Look {
                    kind: Kind::Ghost,
                    size: Size::Normal,
                    icon: None,
                    label: &more_label,
                };
                let size = button::measure(ui, &look) + Vec2::new(16.0, 0.0);
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                let state = if resp.hovered() {
                    button::State::Hover
                } else {
                    button::State::Rest
                };
                button::paint_button(ui, rect, &look, state, &t);
                let chev = Rect::from_center_size(
                    pos2(rect.max.x - 14.0, rect.center().y),
                    Vec2::splat(16.0),
                );
                (kit_icons::CHEVRON_DOWN.0)(ui.painter(), chev, t.muted);
                crate::a11y::name_control(&resp, &more_label, None);
                let resp = resp.on_hover_text(t!("toolbar.more.tip"));
                egui::Popup::menu(&resp).show(|ui| {
                    ui.set_min_width(200.0);
                    for item in rest {
                        let label = item.label.clone();
                        if ui
                            .selectable_label(item.armed, fonts::text(Role::Body, label))
                            .on_hover_text(&item.tip)
                            .clicked()
                        {
                            action = Some(item.act);
                            ui.close();
                        }
                    }
                });
            }
            if pcb {
                self.toolbar_options(ui, &t);
            }
        });
        if let Some(act) = action {
            self.toolbar_apply(act);
        }
    }

    /// The right-hand options: Keepout toggles while that tool is active, Corners
    /// while Route is active.
    fn toolbar_options(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add_space(16.0);
            if matches!(self.tool, Tool::Route(_)) {
                let mut idx = usize::from(!self.preferences.chamfer_corners);
                let chamfer = t!("toolbar.corners_45");
                let square = t!("toolbar.corners_90");
                let (chamfer_tip, square_tip) =
                    (t!("toolbar.corners_45.tip"), t!("toolbar.corners_90.tip"));
                segmented(
                    ui,
                    "toolbar-corners",
                    &mut idx,
                    &[
                        Segment {
                            label: &chamfer,
                            tip: &chamfer_tip,
                        },
                        Segment {
                            label: &square,
                            tip: &square_tip,
                        },
                    ],
                );
                ui.label(fonts::caption(t!("toolbar.corners")).color(t.muted))
                    .on_hover_text(t!("toolbar.corners.tip"));
                self.preferences.chamfer_corners = idx == 0;
            }
            if matches!(self.tool, Tool::Keepout) {
                ui.add_space(8.0);
                self.keepout_options(ui, t);
            }
        });
    }
}
