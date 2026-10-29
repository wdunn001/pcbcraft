//! Board side in the Layout view: flip a part, look from below, adopt the layer.
//!
//! Context: `lib.rs` keeps the `Camera` mirror flag and the `Preferences`
//! field `view_from_below`; `canvas.rs` paints with them. This file owns the
//! rest: the Flip button, the From below toggle, the console entry and the
//! rule that parts created while the bottom layer is active land on the bottom.
//! Local rules: the side itself lives in the document (`Footprint::side`) and
//! changes only through the engine command `footprint.flip`; the view flag is
//! a display preference and never touches the document.
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
/// Why: seen from below, the bottom copper is the near side and must cover the top.
///
/// # Rules
/// - Order is unchanged when `from_below` is false, and stable inside each side.
///
/// # Do not
/// - Use it for hit testing; picking has its own order.
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
    /// Whether the Layout canvas is drawn from the bottom side right now.
    ///
    /// # Rules
    /// - Only the Layout tab mirrors; the schematic never does.
    pub(crate) fn seen_from_below(&self) -> bool {
        self.tab == Tab::Pcb && self.preferences.view_from_below
    }

    /// Switch between the top view and the view from below, keeping the same
    /// board point in the middle of the canvas.
    pub(crate) fn toggle_view_from_below(&mut self) {
        self.preferences.view_from_below = !self.preferences.view_from_below;
        self.message = if self.preferences.view_from_below {
            t!("board.canvas.from_below.on")
        } else {
            t!("board.canvas.from_below.off")
        };
    }

    /// The floating From below toggle, shown beside Fit on the Layout canvas.
    ///
    /// # Rules
    /// - Shows as selected while the view is mirrored.
    /// - Hidden on the schematic, where there is no other side.
    pub(crate) fn view_from_below_button(&mut self, ui: &mut egui::Ui) {
        if self.tab != Tab::Pcb {
            return;
        }
        let on = self.preferences.view_from_below;
        if ui
            .add(
                egui::Button::new(t!("board.canvas.from_below"))
                    .selected(on)
                    .min_size(Vec2::splat(24.0)),
            )
            .on_hover_text(t!("board.canvas.from_below.tip"))
            .clicked()
        {
            self.toggle_view_from_below();
        }
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
