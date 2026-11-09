//! Press-and-drag routing and track end handles on the board canvas.
//!
//! Holds the drag state, the one rule that turns the preferences into a route
//! mode, the ghost line and the end handles. `canvas.rs` and `lib.rs` only call
//! the small hooks below; every mutation goes through the typed commands
//! `track.route`, `track.route_pair` and `track.drag`.
use super::*;
use crate::canvas::snap;
use egui::Sense;

/// Which end of a track a handle drags; matches the `end` parameter of `track.drag`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum End {
    Start,
    Finish,
}
impl End {
    fn name(self) -> &'static str {
        match self {
            End::Start => "start",
            End::Finish => "end",
        }
    }
}

/// One end of a route the pointer has hold of.
///
/// A pad is named by its footprint and pad number. A via has no such name,
/// so it travels as the point it sits at and the net it carries, which is
/// the other form `track.route` accepts.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RouteEnd {
    Pad(Id, String),
    Via(Point, Id),
}

impl RouteEnd {
    /// This end as `track.route` wants it.
    fn as_param(&self) -> Value {
        match self {
            RouteEnd::Pad(id, pad) => json!({"footprint": id, "pad": pad}),
            RouteEnd::Via(at, _) => json!({"x": at.x, "y": at.y}),
        }
    }

    /// The net a point-form end carries, which the command needs told.
    fn net(&self) -> Option<Id> {
        match self {
            RouteEnd::Pad(..) => None,
            RouteEnd::Via(_, net) => Some(*net),
        }
    }

    /// The pad this end names, when it names one.
    fn pad(&self) -> Option<(Id, &str)> {
        match self {
            RouteEnd::Pad(id, pad) => Some((*id, pad.as_str())),
            RouteEnd::Via(..) => None,
        }
    }

    /// The net this end sits on, for finding the pads worth landing on.
    ///
    /// A via carries its net. A pad has to be looked up, since the net lives on
    /// the pad rather than on the end that names it.
    pub(crate) fn net_on(&self, project: &Project) -> Option<Id> {
        match self {
            RouteEnd::Pad(id, pad) => project
                .footprint(*id)
                .and_then(|f| f.pads.iter().find(|p| &p.number == pad))
                .and_then(|p| p.net),
            RouteEnd::Via(_, net) => Some(*net),
        }
    }

    /// Where this end sits on the board, for drawing the line to the pointer.
    ///
    /// A pad has to be looked up, since the footprint may have moved since the
    /// end was picked. A via already travels as its point.
    pub(crate) fn position(&self, project: &Project) -> Option<Point> {
        match self {
            RouteEnd::Pad(id, pad) => project.footprint(*id).and_then(|f| f.pad_position(pad)),
            RouteEnd::Via(at, _) => Some(*at),
        }
    }
}

/// What the pointer is dragging right now, if anything.
#[derive(Default)]
pub(crate) struct State {
    /// The pad or via a route drag started on.
    route: Option<RouteEnd>,
    /// The track end being dragged.
    end: Option<(Id, End)>,
}
impl State {
    /// True while a route or a track end is being dragged.
    pub(crate) fn active(&self) -> bool {
        self.route.is_some() || self.end.is_some()
    }
}

/// The route mode the two routing preferences ask for.
///
/// Why: the click path and the drag path must never disagree.
///
/// # Rules
/// - Chamfer off means right angles, whatever the shove preference says.
///
/// # Dependencies
/// - `crates/ui-egui/src/lib.rs` for `Preferences`.
///
/// # Do not
/// - Read the preferences anywhere else to pick a mode.
pub(crate) fn route_mode(prefs: &Preferences) -> &'static str {
    match (prefs.chamfer_corners, prefs.route_shove) {
        (false, _) => "orthogonal",
        (true, false) => "octilinear",
        (true, true) => "octilinear_shove",
    }
}

/// The status line text naming the current route mode.
fn mode_label(mode: &str) -> String {
    match mode {
        "orthogonal" => t!("board.canvas.route_mode_orthogonal"),
        "octilinear_shove" => t!("board.canvas.route_mode_shove"),
        _ => t!("board.canvas.route_mode_octilinear"),
    }
}

/// Pixels around a handle centre that still count as grabbing it.
const GRAB: f64 = 8.0;

impl App {
    /// Pick whichever end of a route the press at `pos` lands on.
    ///
    /// # Rules
    /// - A pad wins over a via when both are under the pointer, because a pad
    ///   is the more specific thing to have aimed at.
    /// - A via carries its net, since `track.route` needs telling for a
    ///   point-form end.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/canvas.rs` (`pick_pad`, `pick_via`)
    pub(super) fn pick_route_end(&self, pos: Point, zoom: f32) -> Option<RouteEnd> {
        if let Some((id, pad)) = self.pick_pad(pos, zoom) {
            return Some(RouteEnd::Pad(id, pad));
        }
        let id = self.pick_via(pos, zoom)?;
        let via = self.project.board.vias.iter().find(|v| v.id == id)?;
        Some(RouteEnd::Via(via.position, via.net))
    }

    /// Route two ends: a differential pair when both are pads with mates, else one net.
    ///
    /// Why: the click path and the drag path share this body so a route is the
    /// same copper either way. Ends the route tool.
    ///
    /// # Rules
    /// - Two point-form ends carry the net with them, since neither names one.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/canvas.rs` `pair_mates`; engine commands `track.route`,
    ///   `track.route_pair` in `crates/engine/src/lib.rs`.
    ///
    /// # Do not
    /// - Pick the mode here from anything but `route_mode`.
    pub(super) fn finish_route(&mut self, from: &RouteEnd, to: &RouteEnd) {
        let mode = self.route_mode();
        // A differential pair is named by its two pads, so pair routing stays
        // a pad to pad affair. A via end falls through to the single route.
        if let (Some((from_id, from_pad)), Some((to_id, to_pad))) = (from.pad(), to.pad())
            && let Some((mate_from, mate_to)) = self.pair_mates(from_id, from_pad, to_id, to_pad)
        {
            self.run(
                "track.route_pair",
                json!({
                    "from_p":{"footprint":from_id,"pad":from_pad},
                    "from_n":{"footprint":from_id,"pad":mate_from},
                    "to_p":{"footprint":to_id,"pad":to_pad},
                    "to_n":{"footprint":to_id,"pad":mate_to},
                    "layer":self.route_layer_param(),
                    "mode":mode,
                }),
            );
            self.tool = Tool::Select;
            return;
        }
        let mut args = json!({
            "from": from.as_param(),
            "to": to.as_param(),
            "layer": self.route_layer_param(),
            "mode": mode,
        });
        // Two points need the net spelled out. One of them being a pad is
        // enough to name it, so only say it when neither does.
        if let Some(net) = from.net().or_else(|| to.net())
            && from.pad().is_none()
            && to.pad().is_none()
        {
            args["net"] = json!(net);
        }
        self.run("track.route", args);
        self.tool = Tool::Select;
    }

    /// Start whichever drag the press at `pos` begins. Returns true when it
    /// took the press, so the caller skips the move-a-part drag.
    ///
    /// # Rules
    /// - Route drag: PCB tab, route tool, press on a pad or a via.
    /// - End drag: select tool, press on a handle of the selected track.
    ///
    /// # Do not
    /// - Start a drag from a press that missed every pad and handle.
    pub(super) fn begin_route_drag(&mut self, pos: Point, zoom: f32) -> bool {
        if self.tab != Tab::Pcb {
            return false;
        }
        if matches!(self.tool, Tool::Route(_))
            && let Some(end) = self.pick_route_end(pos, zoom)
        {
            self.route_drag.route = Some(end);
            self.message = mode_label(route_mode(&self.preferences));
            return true;
        }
        if matches!(self.tool, Tool::Select)
            && let Some((id, end)) = self.handle_at(pos, zoom)
        {
            self.route_drag.end = Some((id, end));
            return true;
        }
        false
    }

    /// Finish the drag that is under way, if any, at `pos`.
    ///
    /// # Rules
    /// - A route drag needs a second pad under the pointer; anywhere else it
    ///   cancels and changes nothing.
    /// - A handle drag is one `track.drag` command, so one undo step.
    ///
    /// # Do not
    /// - Move the track end with `track.set_end`; it skips clearance.
    pub(super) fn end_route_drag(&mut self, pos: Point, zoom: f32) {
        if let Some(from) = self.route_drag.route.take() {
            match self.pick_route_end(pos, zoom) {
                Some(to) if to != from => self.finish_route(&from, &to),
                _ => self.message = t!("board.canvas.route_drag_cancelled"),
            }
        }
        if let Some((id, end)) = self.route_drag.end.take() {
            let mode = if self.preferences.route_shove {
                "octilinear_shove"
            } else {
                "octilinear"
            };
            self.run(
                "track.drag",
                json!({"id":id,"end":end.name(),"position":snap(pos),"mode":mode}),
            );
            if !self
                .project
                .board
                .tracks
                .iter()
                .any(|t| Some(t.id) == self.selected)
            {
                self.select(None);
            }
        }
    }

    /// The selected track end under `pos`, if a track is selected.
    fn handle_at(&self, pos: Point, zoom: f32) -> Option<(Id, End)> {
        let track = self.selected_track()?;
        [(End::Start, track.start), (End::Finish, track.end)]
            .into_iter()
            .find(|(_, at)| at.distance(pos) * (zoom as f64) < GRAB)
            .map(|(end, _)| (track.id, end))
    }

    fn selected_track(&self) -> Option<&pcbcraft_doc::Track> {
        if !matches!(self.tool, Tool::Select) || self.tab != Tab::Pcb {
            return None;
        }
        let id = self.selected?;
        self.project.board.tracks.iter().find(|t| t.id == id)
    }

    /// Paint the ghost line of a route drag, the ring on a matching pad, and
    /// the end handles of the selected track.
    ///
    /// # Rules
    /// - Each handle is a hover target with a tooltip and an accessible name.
    ///
    /// # Do not
    /// - Change any state; this only draws and registers hover targets.
    pub(super) fn paint_route_drag(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        cursor: Option<Point>,
    ) {
        let accent = self.accent();
        if let Some(from) = &self.route_drag.route
            && let Some(start) = from.position(&self.project)
            && let Some(to) = cursor
        {
            let line = [camera.screen(start, rect), camera.screen(to, rect)];
            painter.extend(egui::Shape::dashed_line(
                &line,
                Stroke::new(2.0, accent),
                6.0,
                4.0,
            ));
            let net = from.net_on(&self.project);
            if let Some((other, number)) = self.pick_pad(to, camera.zoom)
                && let Some(of) = self.project.footprint(other)
                && let Some(op) = of.pads.iter().find(|p| p.number == number)
                && net.is_some()
                && op.net == net
                && *from != RouteEnd::Pad(other, number.clone())
                && let Some(centre) = of.pad_position(&number)
            {
                painter.circle_stroke(
                    camera.screen(centre, rect),
                    (op.diameter as f32 * camera.zoom / 2.0) + 4.0,
                    Stroke::new(2.0, accent),
                );
            }
        }
        let Some(track) = self.selected_track() else {
            return;
        };
        let (id, ends) = (
            track.id,
            [(End::Start, track.start), (End::Finish, track.end)],
        );
        for (end, at) in ends {
            // While a handle is dragged, it follows the pointer.
            let shown = match (self.route_drag.end, cursor) {
                (Some((d, e)), Some(c)) if d == id && e == end => snap(c),
                _ => at,
            };
            let centre = camera.screen(shown, rect);
            painter.circle_filled(centre, 5.0, accent);
            painter.circle_stroke(centre, 5.0, Stroke::new(1.0, self.colours().ink));
            let hit = Rect::from_center_size(centre, egui::vec2(16.0, 16.0));
            let r = ui
                .interact(
                    hit,
                    egui::Id::new(("track-end-handle", id, end.name())),
                    Sense::hover(),
                )
                .on_hover_text(t!("board.canvas.track_end_handle.tip"));
            a11y::name_control(&r, &t!("board.canvas.track_end_handle"), None);
        }
    }
}

/// The "Push other tracks aside" checkbox of the Settings tab.
///
/// Why: the router can push other nets' tracks, but only with 45 degree routes,
/// so the box is disabled, and says why, while 90 degree corners are selected.
///
/// # Dependencies
/// - `crates/ui-egui/src/lib.rs` for `Preferences::route_shove` and `chamfer_corners`.
///
/// # Do not
/// - Add a second place that edits `route_shove`.
pub(super) fn shove_checkbox(app: &mut App, ui: &mut egui::Ui) {
    let enabled = app.preferences.chamfer_corners;
    let tip = if enabled {
        t!("settings.settings_tab.route_shove.tip")
    } else {
        t!("settings.settings_tab.route_shove.tip_off")
    };
    let label = t!("settings.settings_tab.route_shove");
    let r = ui
        .add_enabled(
            enabled,
            egui::Checkbox::new(&mut app.preferences.route_shove, label.clone()),
        )
        .on_disabled_hover_text(tip.clone())
        .on_hover_text(tip);
    a11y::name_control(&r, &label, Some(app.preferences.route_shove));
}
