//! Board side in the Layout view: flip a part, look from below, adopt the layer.
//!
//! Context: `lib.rs` keeps the `Camera` (mirror flag and quarter turns) in
//! `App::pcb`; `canvas.rs` paints with it. This file owns the rest: the
//! Top | Bottom view side, the rotate and reset buttons' logic, pinned and
//! hovered layer focus, Page Up and Page Down stepping, the console entry and
//! the rule that parts created while the bottom layer is active land on the bottom.
//! Local rules: the side of a part lives in the document (`Footprint::side`) and
//! changes only through the engine command `footprint.flip`; the view state is a
//! display preference and never touches the document.
use super::*;
use pcbcraft_doc::{Layer, Project};
use pcbcraft_library::Graphic;

/// Console id of the view toggle; handled by `App::run`, never sent to the engine.
pub(crate) const VIEW_FROM_BELOW: &str = "view.from_below";

/// The view toggle as a console entry, so Ctrl+K reaches it like any command.
///
/// Why: the console lists engine commands; the toggle is display-only.
///
/// # Rules
/// - Takes an empty object; `App::run` flips the preference.
///
/// # Dependencies
/// - `crates/ui-egui/src/lib.rs` `App::run` intercepts the id; the console list chains this entry.
///
/// # Do not
/// - Send this id to `Session::execute`; the engine does not know it.
pub(crate) fn view_command() -> pcbcraft_engine::CommandSpec {
    pcbcraft_engine::CommandSpec {
        id: VIEW_FROM_BELOW,
        description: "Look at the board from below, mirrored left to right, or return to the top view",
        input_schema: json!({"type":"object","properties":{},"required":[],"additionalProperties":false}),
    }
}

/// Ids of every placed footprint, taken before a command runs.
///
/// Why: `App::run` compares it with the list afterwards to find new parts.
pub(crate) fn footprint_ids(project: &Project) -> std::collections::BTreeSet<Id> {
    project.board.footprints.iter().map(|f| f.id).collect()
}

/// Put the items that sit on the bottom last, so they paint on top.
///
/// Why: a two-sided order kept for its story; the canvas now orders by the
/// whole stack with `stack_view::StackView::far_to_near`.
#[cfg(test)]
pub(crate) fn bottom_last<'a, T>(
    items: impl Iterator<Item = &'a T>,
    from_below: bool,
    is_bottom: impl Fn(&T) -> bool,
) -> Vec<&'a T> {
    let mut list: Vec<&T> = items.collect();
    if from_below {
        list.sort_by_key(|item| is_bottom(item));
    }
    list
}

/// A package drawing turned over about its own vertical axis.
///
/// Why: a part on the bottom shows its artwork mirrored when the board is seen
/// from the top; the library keeps one top-side drawing per package.
///
/// # Rules
/// - Only x changes (sign) and text angles negate; widths, layers and fills stay.
///
/// # Do not
/// - Write the result back to the library; the package stays authored on top.
pub(crate) fn mirrored_graphic(graphic: &Graphic) -> Graphic {
    let flip = |p: &Point| Point::new(-p.x, p.y);
    match graphic {
        Graphic::Line {
            start,
            end,
            width,
            layer,
        } => Graphic::Line {
            start: flip(start),
            end: flip(end),
            width: *width,
            layer: layer.clone(),
        },
        Graphic::Rectangle {
            start,
            end,
            width,
            fill,
            layer,
        } => Graphic::Rectangle {
            start: flip(start),
            end: flip(end),
            width: *width,
            fill: fill.clone(),
            layer: layer.clone(),
        },
        Graphic::Circle {
            center,
            radius,
            width,
            fill,
            layer,
        } => Graphic::Circle {
            center: flip(center),
            radius: *radius,
            width: *width,
            fill: fill.clone(),
            layer: layer.clone(),
        },
        Graphic::Arc {
            start,
            mid,
            end,
            width,
            layer,
        } => Graphic::Arc {
            start: flip(start),
            mid: flip(mid),
            end: flip(end),
            width: *width,
            layer: layer.clone(),
        },
        Graphic::Polyline {
            points,
            width,
            fill,
            layer,
        } => Graphic::Polyline {
            points: points.iter().map(flip).collect(),
            width: *width,
            fill: fill.clone(),
            layer: layer.clone(),
        },
        Graphic::Text {
            position,
            angle,
            text,
            size,
            layer,
        } => Graphic::Text {
            position: flip(position),
            angle: -*angle,
            text: text.clone(),
            size: *size,
            layer: layer.clone(),
        },
    }
}

impl App {
    /// Switch between the top view and the view from below.
    pub(crate) fn toggle_view_from_below(&mut self) {
        let below = !self.pcb.mirror;
        self.set_view_side(below);
    }

    /// The copper layer on the bottom of this board.
    pub(crate) fn bottom_copper(&self) -> Layer {
        Layer::from_index(self.project.copper_layer_count().max(2) - 1).unwrap_or(Layer::Back)
    }

    /// Choose the side the Layout canvas looks at and draws on.
    ///
    /// Why: the Top | Bottom control and the console entry mean the same thing,
    /// so they share one setter.
    ///
    /// # Rules
    /// - Mirrors the camera and keeps the layout region flag in step.
    /// - Moves the drawing target to the same kind of layer on that side:
    ///   copper to the bottom copper, silkscreen to the bottom silkscreen, and
    ///   back again for Top. Edge cuts and user layers stay where they are.
    /// - A pinned layer view moves with the target.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/draw_target.rs` `App::set_target`.
    /// - `crates/ui-egui/src/workspace.rs` `RegionLayout::bottom_side`.
    ///
    /// # Do not
    /// - Touch the document; the side of a view is a display preference.
    pub(crate) fn set_view_side(&mut self, below: bool) {
        use crate::draw_target::{Side, Target};
        self.pcb.mirror = below;
        self.region().bottom_side = below;
        let side = if below { Side::Bottom } else { Side::Top };
        let copper = if below {
            self.bottom_copper()
        } else {
            Layer::Front
        };
        let target = match self.target {
            Target::Copper(_) => Target::Copper(copper),
            Target::Silk(_) => Target::Silk(side),
            Target::Mask(_) => Target::Mask(side),
            Target::Paste(_) => Target::Paste(side),
            other => other,
        };
        self.set_target(target);
        if self.pinned_view.is_some() {
            self.pinned_view = Some(target);
        }
        self.message = if below {
            t!("board.canvas.from_below.on")
        } else {
            t!("board.canvas.from_below.off")
        };
    }

    /// Turn the Layout view by quarter turns; positive is clockwise.
    ///
    /// # Rules
    /// - Only the camera changes; the board and its coordinates do not.
    pub(crate) fn rotate_view(&mut self, quarters: i8) {
        self.pcb.turns = (self.pcb.turns as i8 + quarters).rem_euclid(4) as u8;
    }

    /// Fit the board, turn it upright, look from the top and unpin a layer view.
    pub(crate) fn reset_view(&mut self) {
        self.pcb.turns = 0;
        self.pinned_view = None;
        if self.pcb.mirror {
            self.set_view_side(false);
        }
        self.needs_fit = true;
    }

    /// Pin the see-through view to `target`, or unpin when it is already pinned.
    ///
    /// Why: a click on a layer row should keep that layer forward after the
    /// pointer moves on.
    pub(crate) fn pin_view(&mut self, target: crate::draw_target::Target) {
        self.pinned_view = if self.pinned_view == Some(target) {
            None
        } else {
            Some(target)
        };
    }

    /// The target the see-through view is built around right now.
    ///
    /// # Rules
    /// - Order: the row under the pointer, the pinned layer, the active target.
    pub(crate) fn focus_target(&mut self) -> crate::draw_target::Target {
        self.tree
            .hover_target
            .take()
            .or(self.pinned_view)
            .unwrap_or_else(|| self.current_target())
    }

    /// Step the active layer one physical level toward or away from the viewer.
    ///
    /// # Rules
    /// - Walks `stack_view::levels`; stops at the top and the bottom of the stack.
    /// - Copper steps also move the routing layer; a pinned view follows.
    pub(crate) fn step_layer(&mut self, toward_viewer: bool) {
        let from = self.current_target();
        let Some(next) = crate::stack_view::step(
            self.project.copper_layer_count(),
            from,
            self.pcb.mirror,
            toward_viewer,
        ) else {
            return;
        };
        self.set_target(next);
        if self.pinned_view.is_some() {
            self.pinned_view = Some(next);
        }
        self.message = t!("view.stepped", name = self.target_label());
    }

    /// The stack strengths for this frame, eased toward their goals.
    ///
    /// Why: layers fade rather than jump when the pointer crosses a tree row.
    ///
    /// # Rules
    /// - With Reduce motion on, the strengths change at once.
    pub(crate) fn stack_view(
        &mut self,
        ctx: &egui::Context,
        focus: crate::draw_target::Target,
    ) -> crate::stack_view::StackView {
        let copper = self.project.copper_layer_count();
        let flipped = self.pcb.mirror;
        let goals = crate::stack_view::StackView::targets(copper, flipped, Some(focus));
        let time = if self.preferences.reduced_motion {
            0.0
        } else {
            0.15
        };
        let alpha = goals
            .iter()
            .enumerate()
            .map(|(i, goal)| {
                ctx.animate_value_with_time(egui::Id::new(("stack-fade", i)), *goal, time)
            })
            .collect();
        crate::stack_view::StackView {
            copper,
            flipped,
            alpha,
        }
    }

    /// Page Up and Page Down step through the stack; Esc drops a pinned view.
    ///
    /// # Rules
    /// - Ignored while a text field has the keyboard.
    pub(crate) fn step_layers_by_key(&mut self, ui: &egui::Ui) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let (up, down, esc) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::PageUp),
                i.key_pressed(egui::Key::PageDown),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if up {
            self.step_layer(true);
        } else if down {
            self.step_layer(false);
        }
        if esc && self.pinned_view.is_some() {
            self.pinned_view = None;
        }
    }

    /// "Layer 2 of 4 · In1": where the active target sits, for the status bar.
    pub(crate) fn layer_status(&self) -> String {
        use crate::draw_target::{Side, Target};
        let total = self.project.copper_layer_count().max(2);
        let n = match self.current_target() {
            Target::Copper(l) => l.index() as usize + 1,
            Target::Silk(s) | Target::Mask(s) | Target::Paste(s) => {
                if s == Side::Top {
                    1
                } else {
                    total as usize
                }
            }
            _ => 0,
        };
        if n == 0 {
            return t!("target.status", layer = self.target_label());
        }
        t!(
            "view.layer_status",
            n = n,
            total = total,
            name = self.target_label()
        )
    }

    /// Flip parts that a command just created when the bottom layer is active.
    ///
    /// Why: dropped components adopt the current layer, with no extra click.
    ///
    /// # Rules
    /// - `known` is the footprint list before the command; only newer ids flip.
    /// - Does nothing unless the active routing layer is the bottom.
    pub(crate) fn adopt_active_layer(&mut self, known: &std::collections::BTreeSet<Id>) {
        if self.route_layer != Layer::Back {
            return;
        }
        let fresh: Vec<Id> = self
            .project
            .board
            .footprints
            .iter()
            .filter(|f| !known.contains(&f.id) && f.board_side() == Layer::Front)
            .map(|f| f.id)
            .collect();
        for id in fresh {
            self.run("footprint.flip", json!({"id": id}));
        }
    }
}
