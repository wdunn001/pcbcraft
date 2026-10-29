//! Canvas gestures routed through the tool modifier resolver.
//!
//! The canvas hands every press, drag, click and hover to this module. It finds
//! what is under the pointer ([`Hit`]), asks `tool_modifiers::resolve` what the
//! gesture does for the active tool and held keys, then runs the matching typed
//! engine command. It also owns the hover affordance, the cursor and the status
//! hint, because all three come from the same resolved action.
//!
//! # Rules
//! - Never read egui modifiers here; use the [`Held`] the canvas passes in.
//! - Every change goes through `App::run` (typed engine commands).
//! - Hover paints and cursors only change while the pointer is over the canvas.
//!
//! # Dependencies
//! - `crates/ui-egui/src/tool_modifiers.rs` the resolver and its table.
//! - `crates/ui-egui/src/hover_affordance.rs` the animated outline.
//! - `crates/ui-egui/src/modifier_ui.rs` words for the status hint.
//! - `crates/ui-egui/src/canvas.rs` the pickers (`pick_pin`, `pick_pad`, ...).
//!
//! # Do not
//! - Add a gesture here without a row in `tool_modifiers::ROWS`.

use super::*;
use crate::hover_affordance::{self, Affordance, Outline};
use crate::tool_modifiers::{
    self as tm, ClickAction, Gesture, Held, ModAction, Phase, Target, ToolKind, Workspace,
};

/// What the pointer is over, with the ids needed to act on it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Hit {
    Empty,
    Part(Id),
    Pin(PinRef),
    Pad(Id, String),
    /// A drawn wire and the index of the segment's first vertex.
    WireSegment(Id, usize),
    TrackSegment(Id),
    /// A track and whether the end (true) or the start (false) is under the pointer.
    TrackEnd(Id, bool),
    Via(Id),
    /// A drawn wire and the index of the vertex.
    Node(Id, usize),
    Junction(Id),
    NetLabel(Id),
    Other(Id),
}

impl Hit {
    /// The resolver's name for what this is.
    pub(crate) fn target(&self) -> Target {
        match self {
            Self::Empty => Target::Empty,
            Self::Part(_) => Target::Part,
            Self::Pin(_) => Target::Pin,
            Self::Pad(..) => Target::Pad,
            Self::WireSegment(..) => Target::WireSegment,
            Self::TrackSegment(_) => Target::TrackSegment,
            Self::TrackEnd(..) => Target::TrackEnd,
            Self::Via(_) => Target::Via,
            Self::Node(..) => Target::Node,
            Self::Junction(_) => Target::Junction,
            Self::NetLabel(_) => Target::NetLabel,
            Self::Other(_) => Target::Other,
        }
    }
}

/// A press that is turning into a drag.
#[derive(Clone, Debug)]
pub(crate) struct Press {
    pub action: ClickAction,
    pub hit: Hit,
    /// Where the pointer went down, in world millimetres.
    pub from: Point,
    /// Where the pointer is now.
    pub to: Point,
    /// Position of the dragged part before the drag, for axis constraint.
    pub origin: Point,
}

/// What the pointer is hovering and what a click there will do.
#[derive(Clone, Debug)]
pub(crate) struct Hover {
    pub hit: Hit,
    pub action: ClickAction,
}

/// Rename-a-net prompt opened by a double-click on a net label.
#[derive(Clone, Debug)]
pub(crate) struct Rename {
    pub net: Id,
    pub text: String,
}

impl Tool {
    /// The resolver's name for this tool.
    pub(crate) fn kind(&self) -> ToolKind {
        match self {
            Self::Select => ToolKind::Select,
            Self::Place(_) | Self::LibraryPlace(..) | Self::CustomPlace(..) => ToolKind::Place,
            Self::Wire(_) => ToolKind::Wire,
            Self::Route(_) => ToolKind::Route,
            Self::Via => ToolKind::Via,
            Self::Keepout => ToolKind::Select,
            Self::Move => ToolKind::Move,
            Self::Cutout => ToolKind::Cutout,
            Self::Hole => ToolKind::Hole,
            Self::NoConnect => ToolKind::NoConnect,
            Self::Fiducial => ToolKind::Fiducial,
            Self::DrawLayer => ToolKind::DrawLayer,
            Self::Measure => ToolKind::Measure,
        }
    }
}

impl App {
    fn modifier_workspace(&self) -> Workspace {
        if self.tab == Tab::Pcb {
            Workspace::Board
        } else {
            Workspace::Schematic
        }
    }

    /// Resolve a gesture on a hit for the active tool.
    fn resolved(&self, hit: &Hit, held: Held, gesture: Gesture) -> ClickAction {
        tm::resolve(
            &self.preferences.modifiers,
            self.tool.kind(),
            self.modifier_workspace(),
            held,
            hit.target(),
            gesture,
        )
    }

    /// What is under `pos`: pins and pads first, then vias, junctions, labels,
    /// vertices, track ends, segments and finally whole parts.
    ///
    /// # Rules
    /// - Reach is in screen points, so it feels the same at every zoom.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/canvas.rs` pickers.
    pub(crate) fn hit_at(&self, pos: Point, zoom: f32) -> Hit {
        let near = |d: f64, px: f64| d * zoom as f64 <= px;
        if self.tab == Tab::Schematic {
            if let Some(pin) = self.pick_pin(pos, zoom) {
                return Hit::Pin(pin);
            }
            if let Some(l) = self
                .project
                .labels
                .iter()
                .filter(|l| near(l.position.distance(pos), 7.0))
                .min_by(|a, b| {
                    a.position
                        .distance(pos)
                        .total_cmp(&b.position.distance(pos))
                })
            {
                return if l.kind == "junction" {
                    Hit::Junction(l.id)
                } else {
                    Hit::NetLabel(l.id)
                };
            }
            for w in &self.project.drawn_wires {
                if let Some(i) = w.points.iter().position(|p| near(p.distance(pos), 6.0)) {
                    return Hit::Node(w.id, i);
                }
            }
            for w in &self.project.drawn_wires {
                for (i, pair) in w.points.windows(2).enumerate() {
                    if near(segment_distance(pos, pair[0], pair[1]), 5.0) {
                        return Hit::WireSegment(w.id, i);
                    }
                }
            }
        } else {
            if let Some((id, pad)) = self.pick_pad(pos, zoom) {
                return Hit::Pad(id, pad);
            }
            if let Some(id) = self.pick_via(pos, zoom) {
                return Hit::Via(id);
            }
            for t in &self.project.board.tracks {
                if near(t.start.distance(pos), 6.0) {
                    return Hit::TrackEnd(t.id, false);
                }
                if near(t.end.distance(pos), 6.0) {
                    return Hit::TrackEnd(t.id, true);
                }
            }
            if let Some(id) = self.pick_track(pos, zoom) {
                return Hit::TrackSegment(id);
            }
        }
        match self.pick_filtered(pos, zoom) {
            Some(id)
                if self.project.component(id).is_some() || self.project.footprint(id).is_some() =>
            {
                Hit::Part(id)
            }
            Some(id) => Hit::Other(id),
            None => Hit::Empty,
        }
    }

    /// Short words for the hovered thing: `R1.2`, `R1`, a net name.
    pub(crate) fn hit_words(&self, hit: &Hit) -> String {
        let reference = |component: Id| {
            self.project
                .component(component)
                .map(|c| c.reference.clone())
                .unwrap_or_default()
        };
        match hit {
            Hit::Pin(p) => format!("{}.{}", reference(p.component), p.pin),
            Hit::Pad(f, pad) => {
                let owner = self
                    .project
                    .footprint(*f)
                    .map(|fp| reference(fp.component))
                    .unwrap_or_default();
                format!("{owner}.{pad}")
            }
            Hit::Part(id) => self
                .project
                .component(*id)
                .map(|c| c.reference.clone())
                .or_else(|| self.project.footprint(*id).map(|f| reference(f.component)))
                .unwrap_or_default(),
            Hit::NetLabel(id) => self
                .project
                .labels
                .iter()
                .find(|l| l.id == *id)
                .map(|l| self.project.net_name(l.net).to_owned())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Per-frame step: refresh the live modifier set, the snap switch, the
    /// hover, the mouse cursor and the status hint.
    ///
    /// # Rules
    /// - Runs before anything snaps, so the snap flag is current for the frame.
    pub(crate) fn gesture_frame(
        &mut self,
        ui: &egui::Ui,
        held: Held,
        hovered: bool,
        cursor: Option<Point>,
        zoom: f32,
    ) {
        let kind = self.tool.kind();
        let dragging = self.drag.is_some() || self.edit.press.is_some();
        let bindings = &self.preferences.modifiers;
        self.edit.live = tm::active(bindings, kind, held, Phase::Live);
        tm::set_snap_suspended(tm::suspends_snap(bindings, kind, held, dragging));
        self.edit.hover = None;
        self.edit.hint.clear();
        if !hovered || self.edit.press.is_some() {
            return;
        }
        let Some(pos) = cursor else {
            return;
        };
        let hit = self.hit_at(pos, zoom);
        let action = self.resolved(&hit, held, Gesture::Click);
        let drag = self.resolved(&hit, held, Gesture::Drag);
        let plain = self.resolved(&hit, Held::default(), Gesture::Click);
        ui.ctx().set_cursor_icon(hover_affordance::cursor_for(
            if action == ClickAction::ToolDefault || action == ClickAction::Replace {
                if drag == ClickAction::MovePart || drag == ClickAction::MoveSegment {
                    drag
                } else {
                    action
                }
            } else {
                action
            },
        ));
        let what = self.hit_words(&hit);
        self.edit.hint = crate::modifier_ui::status_hint(
            action,
            plain,
            drag,
            &what,
            kind,
            self.modifier_workspace(),
            held,
            bindings,
            tm::Platform::of(ui.ctx()),
        );
        self.edit.hover = Some(Hover { hit, action });
    }

    /// Draw the animated outline on the hovered object, if the tool can act on it.
    pub(crate) fn paint_affordance(&self, painter: &egui::Painter, rect: Rect, camera: Camera) {
        let Some(hover) = &self.edit.hover else {
            return;
        };
        if self.edit.press.is_some() {
            return;
        }
        let Some(style) =
            hover_affordance::affordance_for(self.tool.kind(), hover.action, hover.hit.target())
        else {
            return;
        };
        let Some((shape, net)) = self.hit_outlines(&hover.hit, rect, camera) else {
            return;
        };
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            format!("{:?}", hover.hit).hash(&mut h);
            h.finish()
        };
        hover_affordance::paint(
            painter,
            &Affordance {
                shape: &shape,
                style,
                net: &net,
                accent: self.accent(),
                warning: Color32::from_rgb(240, 90, 80),
                reduced_motion: self.preferences.reduced_motion,
            },
            key,
        );
    }

    /// The hovered object's outline and, for wires and tracks, the other
    /// segments of its net.
    fn hit_outlines(
        &self,
        hit: &Hit,
        rect: Rect,
        camera: Camera,
    ) -> Option<(Outline, Vec<Outline>)> {
        let s = |p: Point| camera.screen(p, rect);
        let dot = |p: Point, px: f32| Outline::Circle(s(p), px);
        let boxed = |id: Id| {
            self.bounds_of(id).map(|(lo, hi)| {
                Outline::Polygon(vec![
                    s(lo),
                    s(Point::new(hi.x, lo.y)),
                    s(hi),
                    s(Point::new(lo.x, hi.y)),
                ])
            })
        };
        match hit {
            Hit::Empty => None,
            Hit::Part(id) | Hit::Other(id) => boxed(*id).map(|o| (o, vec![])),
            Hit::NetLabel(id) => boxed(*id).map(|o| (o, vec![])),
            Hit::Pin(p) => self
                .project
                .pin_position(p)
                .map(|at| (dot(at, 6.0), vec![])),
            Hit::Pad(f, pad) => {
                let footprint = self.project.footprint(*f)?;
                let at = footprint.pad_position(pad)?;
                let r = footprint
                    .pads
                    .iter()
                    .find(|p| &p.number == pad)
                    .map_or(0.5, |p| p.diameter / 2.0);
                Some((dot(at, (r as f32 * camera.zoom).max(5.0)), vec![]))
            }
            Hit::Junction(id) => {
                let l = self.project.labels.iter().find(|l| l.id == *id)?;
                Some((dot(l.position, 5.0), vec![]))
            }
            Hit::Node(w, i) => {
                let wire = self.project.drawn_wires.iter().find(|x| x.id == *w)?;
                Some((dot(*wire.points.get(*i)?, 5.0), vec![]))
            }
            Hit::WireSegment(w, i) => {
                let wire = self.project.drawn_wires.iter().find(|x| x.id == *w)?;
                let (a, b) = (*wire.points.get(*i)?, *wire.points.get(i + 1)?);
                let net = wire
                    .net
                    .map(|n| {
                        self.project
                            .drawn_wires
                            .iter()
                            .filter(|o| o.net == Some(n))
                            .flat_map(|o| {
                                o.points
                                    .windows(2)
                                    .filter(|p| !(o.id == *w && p[0] == a))
                                    .map(|p| Outline::Segment(s(p[0]), s(p[1]), 1.5))
                                    .collect::<Vec<_>>()
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Some((Outline::Segment(s(a), s(b), 1.5), net))
            }
            Hit::TrackSegment(id) | Hit::TrackEnd(id, _) => {
                let t = self.project.board.tracks.iter().find(|t| t.id == *id)?;
                let half = (t.width as f32 * camera.zoom / 2.0).max(2.0);
                let net: Vec<Outline> = self
                    .project
                    .board
                    .tracks
                    .iter()
                    .filter(|o| o.net == t.net && o.id != t.id)
                    .map(|o| {
                        Outline::Segment(
                            s(o.start),
                            s(o.end),
                            (o.width as f32 * camera.zoom / 2.0).max(2.0),
                        )
                    })
                    .chain(
                        self.project
                            .board
                            .vias
                            .iter()
                            .filter(|v| v.net == t.net)
                            .map(|v| {
                                dot(v.position, (v.diameter as f32 * camera.zoom / 2.0).max(3.0))
                            }),
                    )
                    .collect();
                let shape = match hit {
                    Hit::TrackEnd(_, end) => dot(if *end { t.end } else { t.start }, half.max(5.0)),
                    _ => Outline::Segment(s(t.start), s(t.end), half),
                };
                Some((shape, net))
            }
            Hit::Via(id) => {
                let v = self.project.board.vias.iter().find(|v| v.id == *id)?;
                Some((
                    dot(v.position, (v.diameter as f32 * camera.zoom / 2.0).max(4.0)),
                    vec![],
                ))
            }
        }
    }

    // ----- press, drag, release -----

    /// A press started moving on the Select tool: decide by what is under it.
    pub(crate) fn press_start(&mut self, pos: Point, zoom: f32, held: Held) {
        let hit = self.hit_at(pos, zoom);
        let action = self.resolved(&hit, held, Gesture::Drag);
        let extend = tm::active(
            &self.preferences.modifiers,
            ToolKind::Select,
            held,
            Phase::Click,
        )
        .contains(ModAction::SelectAdd);
        let mut press = Press {
            action,
            hit: hit.clone(),
            from: pos,
            to: pos,
            origin: pos,
        };
        match (action, &hit) {
            (ClickAction::StartWire, Hit::Pin(pin)) => {
                self.tool = Tool::Wire(Some(pin.clone()));
            }
            (ClickAction::StartRoute, Hit::Pad(id, pad)) => {
                self.tool = Tool::Route(Some((*id, pad.clone())));
            }
            (ClickAction::StartRoute, Hit::TrackEnd(id, end)) => {
                if let Some(t) = self.project.board.tracks.iter().find(|t| t.id == *id) {
                    press.origin = if *end { t.end } else { t.start };
                }
                self.select(Some(*id));
            }
            (ClickAction::MoveSegment, Hit::WireSegment(id, _) | Hit::TrackSegment(id)) => {
                self.select(Some(*id));
            }
            (ClickAction::MovePart, Hit::Via(id)) => {
                self.select(Some(*id));
                press.origin = self
                    .project
                    .board
                    .vias
                    .iter()
                    .find(|v| v.id == *id)
                    .map_or(pos, |v| v.position);
            }
            (ClickAction::MovePart, Hit::Part(hit_id)) => {
                if !self.promote_member(*hit_id) {
                    self.select_at(pos, zoom);
                }
                if let Some(id) = self.selected {
                    let original = self
                        .project
                        .component(id)
                        .and_then(|c| c.unit_placement(self.selected_unit).map(|p| p.0))
                        .or_else(|| self.project.footprint(id).map(|f| f.position))
                        .unwrap_or(pos);
                    self.drag_offset = Point::new(original.x - pos.x, original.y - pos.y);
                    self.drag = Some((id, original));
                    press.origin = original;
                }
            }
            (ClickAction::BoxSelect, _) => self.begin_box(pos, extend),
            _ => {
                if !matches!(hit, Hit::Empty) {
                    self.select_at(pos, zoom);
                }
                press.action = ClickAction::Nothing;
            }
        }
        self.edit.press = Some(press);
    }

    /// The pointer moved during a drag.
    pub(crate) fn press_move(&mut self, pos: Point, held: Held) {
        let Some(press) = self.edit.press.as_mut() else {
            return;
        };
        press.to = pos;
        let action = press.action;
        let origin = press.origin;
        match action {
            ClickAction::BoxSelect => self.edit.box_to = Some(pos),
            ClickAction::MovePart => {
                let live = tm::active(
                    &self.preferences.modifiers,
                    ToolKind::Select,
                    held,
                    Phase::Drag,
                );
                if let Some((id, _)) = self.drag {
                    let mut at = pos.offset(self.drag_offset);
                    at = if live.contains(ModAction::DragNoSnap) {
                        at
                    } else {
                        snap(at)
                    };
                    if live.contains(ModAction::ConstrainAxis) {
                        let (x, y) = tm::constrain_axis((origin.x, origin.y), (at.x, at.y));
                        at = Point::new(x, y);
                    }
                    self.drag = Some((id, at));
                }
            }
            _ => {}
        }
    }

    /// The drag ended: commit what it was doing.
    pub(crate) fn press_end(&mut self, pos: Option<Point>, zoom: f32, held: Held) {
        let Some(press) = self.edit.press.take() else {
            return;
        };
        let live = tm::active(
            &self.preferences.modifiers,
            ToolKind::Select,
            held,
            Phase::Drag,
        );
        let place = |p: Point| {
            if live.contains(ModAction::DragNoSnap) {
                p
            } else {
                snap(p)
            }
        };
        let delta = pos.map_or(Point::new(0.0, 0.0), |p| {
            let (a, b) = (place(press.from), place(p));
            Point::new(b.x - a.x, b.y - a.y)
        });
        match (press.action, &press.hit) {
            (ClickAction::BoxSelect, _) => self.finish_box(),
            (ClickAction::MovePart, Hit::Via(id)) => {
                let to = place(press.origin.offset(Point::new(delta.x, delta.y)));
                if to != press.origin {
                    self.run("via.move", json!({"id": id, "position": to}));
                }
            }
            (ClickAction::MovePart, _) => {
                let Some((id, at)) = self.drag.take() else {
                    return;
                };
                if live.contains(ModAction::DuplicateDrag) {
                    let offset = Point::new(at.x - press.origin.x, at.y - press.origin.y);
                    self.run("object.duplicate", json!({"ids": [id], "offset": offset}));
                } else if self.selection().len() > 1
                    && let Some(origin) = self.drag_origin(id)
                {
                    self.move_selection(at.x - origin.x, at.y - origin.y);
                } else {
                    let mut params = json!({"id":id,"position":at});
                    if self.project.component(id).is_some() {
                        params["unit"] = json!(self.selected_unit);
                    }
                    self.run(
                        if self.project.footprint(id).is_some() {
                            "footprint.move"
                        } else {
                            "component.move"
                        },
                        params,
                    );
                }
            }
            (ClickAction::MoveSegment, Hit::WireSegment(id, _)) => {
                if delta.x != 0.0 || delta.y != 0.0 {
                    self.run("wire.move", json!({"id": id, "delta": delta}));
                }
            }
            (ClickAction::MoveSegment, Hit::TrackSegment(id)) => {
                if delta.x != 0.0 || delta.y != 0.0 {
                    self.run("track.move", json!({"id": id, "delta": delta}));
                }
            }
            (ClickAction::StartRoute, Hit::TrackEnd(id, end)) => {
                if delta.x != 0.0 || delta.y != 0.0 {
                    let mode = if self.edit.live.contains(ModAction::ShoveToggle) {
                        "octilinear_shove"
                    } else {
                        "octilinear"
                    };
                    let to = place(press.origin.offset(delta));
                    self.run(
                        "track.drag",
                        json!({"id": id, "end": if *end {"end"} else {"start"}, "position": to, "mode": mode}),
                    );
                }
            }
            (ClickAction::StartWire | ClickAction::StartRoute, start) => {
                // Released on the far pin or pad: finish. Elsewhere: stay armed
                // when it was only a click, cancel when it was dragged away.
                let Some(at) = pos else {
                    self.tool = Tool::Select;
                    return;
                };
                let end = self.hit_at(at, zoom);
                let same = &end == start;
                let lands = matches!(end, Hit::Pin(_) | Hit::Pad(..));
                if lands && !same {
                    self.canvas_click(at, zoom);
                } else if !same {
                    self.tool = Tool::Select;
                }
            }
            _ => {}
        }
    }

    /// Move the preview of an in-flight segment or track-end drag onto a copy
    /// of the project so the canvas draws it where it will land.
    pub(crate) fn press_preview(&self, visible: &mut Project) {
        let Some(press) = &self.edit.press else {
            return;
        };
        let d = Point::new(press.to.x - press.from.x, press.to.y - press.from.y);
        match (press.action, &press.hit) {
            (ClickAction::MoveSegment, Hit::WireSegment(id, _)) => {
                if let Some(w) = visible.drawn_wires.iter_mut().find(|w| w.id == *id) {
                    for p in &mut w.points {
                        *p = p.offset(d);
                    }
                }
            }
            (ClickAction::MoveSegment, Hit::TrackSegment(id)) => {
                if let Some(t) = visible.board.tracks.iter_mut().find(|t| t.id == *id) {
                    t.start = t.start.offset(d);
                    t.end = t.end.offset(d);
                }
            }
            (ClickAction::StartRoute, Hit::TrackEnd(id, end)) => {
                if let Some(t) = visible.board.tracks.iter_mut().find(|t| t.id == *id) {
                    if *end {
                        t.end = press.origin.offset(d);
                    } else {
                        t.start = press.origin.offset(d);
                    }
                }
            }
            (ClickAction::MovePart, Hit::Via(id)) => {
                if let Some(v) = visible.board.vias.iter_mut().find(|v| v.id == *id) {
                    v.position = press.origin.offset(d);
                }
            }
            _ => {}
        }
    }

    // ----- clicks -----

    /// A click on the canvas: resolve it, then run the action.
    pub(crate) fn click_dispatch(&mut self, pos: Point, zoom: f32, held: Held) {
        let hit = self.hit_at(pos, zoom);
        let action = self.resolved(&hit, held, Gesture::Click);
        let pos = self.constrained(pos);
        match action {
            ClickAction::Toggle => self.shift_click(pos, zoom),
            ClickAction::Add => self.add_to_selection(pos, zoom),
            ClickAction::PickUnder => self.pick_under(pos, zoom),
            ClickAction::DeleteConnection => self.delete_connection(&hit),
            ClickAction::RemoveSegment => self.remove_segment(&hit),
            ClickAction::ToolDefault
                if matches!(self.tool, Tool::Move)
                    && self.edit.live.contains(ModAction::DuplicateDrag) =>
            {
                self.duplicate_to(pos);
            }
            _ => self.canvas_click(pos, zoom),
        }
    }

    /// A double-click: rename the net of a label.
    pub(crate) fn double_click(&mut self, pos: Point, zoom: f32, held: Held) {
        let hit = self.hit_at(pos, zoom);
        if self.resolved(&hit, held, Gesture::DoubleClick) != ClickAction::RenameLabel {
            return;
        }
        if let Hit::NetLabel(id) = hit
            && let Some(l) = self.project.labels.iter().find(|l| l.id == id)
        {
            self.edit.rename = Some(Rename {
                net: l.net,
                text: self.project.net_name(l.net).to_owned(),
            });
        }
    }

    /// The small prompt that renames a net. Shown from the canvas each frame.
    pub(crate) fn rename_prompt(&mut self, ctx: &egui::Context) {
        let Some(mut ask) = self.edit.rename.take() else {
            return;
        };
        let mut keep = true;
        let mut apply = false;
        egui::Window::new(t!("modifier.rename.title"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                let field = ui
                    .text_edit_singleline(&mut ask.text)
                    .on_hover_text(t!("modifier.rename.field.tip"));
                field.request_focus();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(t!("modifier.rename.ok"))
                        .on_hover_text(t!("modifier.rename.ok.tip"))
                        .clicked()
                    {
                        apply = true;
                    }
                    if ui
                        .button(t!("modifier.rename.cancel"))
                        .on_hover_text(t!("modifier.rename.cancel.tip"))
                        .clicked()
                    {
                        keep = false;
                    }
                });
            });
        if apply && !ask.text.trim().is_empty() {
            self.run(
                "net.rename",
                json!({"id": ask.net, "name": ask.text.trim()}),
            );
        } else if keep && !apply {
            self.edit.rename = Some(ask);
        }
    }

    /// Shift+click adds the object to the selection and never removes it.
    fn add_to_selection(&mut self, pos: Point, zoom: f32) {
        let Some(id) = self.pick_filtered(pos, zoom) else {
            return;
        };
        let mut ids = self.selection();
        if !ids.contains(&id) && ids.len() < crate::editing::MAX_SELECTED {
            ids.push(id);
            self.set_selection(ids);
        }
    }

    /// Alt+click selects the next object stacked under the pointer.
    fn pick_under(&mut self, pos: Point, zoom: f32) {
        let reach = 4.0 / zoom as f64;
        let stack = self.ids_in_rect(
            Point::new(pos.x - reach, pos.y - reach),
            Point::new(pos.x + reach, pos.y + reach),
        );
        if stack.is_empty() {
            return;
        }
        let next = self
            .selected
            .and_then(|cur| stack.iter().position(|id| *id == cur))
            .map_or(0, |i| (i + 1) % stack.len());
        let id = stack[next];
        self.select(Some(id));
        self.message = t!(
            "modifier.pick_under.message",
            number = next + 1,
            count = stack.len()
        );
    }

    /// Wire tool, Ctrl+click on a node or junction.
    fn delete_connection(&mut self, hit: &Hit) {
        match hit {
            Hit::Junction(id) => self.run("object.delete", json!({"id": id})),
            Hit::Node(wire, index) => {
                let long = self
                    .project
                    .drawn_wires
                    .iter()
                    .find(|w| w.id == *wire)
                    .is_some_and(|w| w.points.len() > 2);
                if long {
                    self.run("wire.remove_point", json!({"id": wire, "index": index}));
                } else {
                    self.run("object.delete", json!({"id": wire}));
                }
            }
            _ => {}
        }
    }

    /// Route tool, Ctrl+click on a track end or via.
    fn remove_segment(&mut self, hit: &Hit) {
        match hit {
            Hit::TrackEnd(id, _) | Hit::Via(id) => {
                self.run("object.delete", json!({"id": id}));
            }
            _ => {}
        }
    }

    /// Move tool with Ctrl held: place a copy instead of moving.
    fn duplicate_to(&mut self, pos: Point) {
        let Some(id) = self.selected else {
            return;
        };
        let from = self
            .project
            .component(id)
            .map(|c| c.position)
            .or_else(|| self.project.footprint(id).map(|f| f.position));
        if let Some(from) = from {
            let at = snap(pos);
            self.run(
                "object.duplicate",
                json!({"ids": [id], "offset": Point::new(at.x - from.x, at.y - from.y)}),
            );
            self.tool = Tool::Select;
        }
    }

    /// Apply the live angle or axis constraint to a point, from the tool's anchor.
    ///
    /// # Rules
    /// - Only the Draw layer, Measure and Move tools constrain what they place.
    pub(crate) fn constrained(&self, pos: Point) -> Point {
        let live = self.edit.live;
        let anchor = match self.tool {
            Tool::DrawLayer => self.art_points.last().copied().or(self.art_start),
            Tool::Measure => match self.edit.measure {
                Some((first, None)) => Some(first),
                _ => None,
            },
            Tool::Move => self.selected.and_then(|id| {
                self.project
                    .component(id)
                    .map(|c| c.position)
                    .or_else(|| self.project.footprint(id).map(|f| f.position))
            }),
            _ => None,
        };
        let Some(a) = anchor else {
            return pos;
        };
        let (x, y) = if matches!(self.tool, Tool::Move) {
            if live.contains(ModAction::ConstrainAxis) {
                tm::constrain_axis((a.x, a.y), (pos.x, pos.y))
            } else {
                return pos;
            }
        } else if live.contains(ModAction::ConstrainAngle) {
            tm::constrain_angle((a.x, a.y), (pos.x, pos.y))
        } else {
            return pos;
        };
        Point::new(x, y)
    }

    /// Route mode for a commit: the corner posture preference, flipped by the
    /// corner action, and walk-around or shove by the "Push other tracks aside"
    /// preference, flipped by the shove action.
    pub(crate) fn route_mode(&self) -> &'static str {
        let flip = self.edit.live.contains(ModAction::RouteCorner);
        let chamfer = self.preferences.chamfer_corners != flip;
        let shove = self.preferences.route_shove != self.edit.live.contains(ModAction::ShoveToggle);
        match (chamfer, shove) {
            (true, true) => "octilinear_shove",
            (true, false) => "octilinear",
            (false, _) => "orthogonal",
        }
    }

    /// Points of the wire preview from a pin to the pointer. Shift keeps it to
    /// 45 and 90 degrees; Alt flips which way the corner turns.
    pub(crate) fn wire_preview_points(&self, a: Point, b: Point) -> Vec<Point> {
        if self.edit.live.contains(ModAction::ConstrainAngle) {
            let (x, y) = tm::constrain_angle((a.x, a.y), (b.x, b.y));
            return vec![a, Point::new(x, y)];
        }
        if self.edit.live.contains(ModAction::WireCorner) {
            return vec![a, Point::new(a.x, b.y), b];
        }
        let mid = (a.x + b.x) / 2.0;
        vec![a, Point::new(mid, a.y), Point::new(mid, b.y), b]
    }

    /// The wheel: pan sideways with Shift, zoom in fine steps with Ctrl.
    ///
    /// Returns the zoom factor exponent source and the horizontal pan in
    /// screen points for this frame.
    pub(crate) fn wheel_plan(&self, ui: &egui::Ui, held: Held) -> (f32, f32, f32) {
        let plan = tm::wheel_plan(&self.preferences.modifiers, self.tool.kind(), held);
        let scroll = motion::wheel_zoom_delta(ui, self.preferences.reduced_motion);
        if plan.pan_horizontal {
            let sideways = ui.input(|i| i.smooth_scroll_delta.x + i.smooth_scroll_delta.y);
            return (0.0, sideways, 1.0);
        }
        let ctrl_zoom = ui.input(|i| i.zoom_delta().ln()) * 300.0;
        let amount = if scroll.abs() > 0.01 {
            scroll
        } else {
            ctrl_zoom
        };
        (amount, 0.0, plan.zoom_scale)
    }

    /// Via tool click: put a via on the net of the track or pad under the pointer.
    pub(crate) fn place_via_click(&mut self, pos: Point, zoom: f32) {
        let net = match self.hit_at(pos, zoom) {
            Hit::TrackSegment(id) | Hit::TrackEnd(id, _) => self
                .project
                .board
                .tracks
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.net),
            Hit::Pad(f, pad) => self
                .project
                .footprint(f)
                .and_then(|fp| fp.pads.iter().find(|p| p.number == pad).and_then(|p| p.net)),
            _ => None,
        };
        match net {
            Some(net) => self.run("via.place", json!({"net": net, "position": snap(pos)})),
            None => self.message = t!("modifier.via.pick_net"),
        }
    }

    /// Arm the tool a hotkey names.
    pub(crate) fn arm_hotkey(&mut self, hotkey: tm::Hotkey) {
        match hotkey {
            tm::Hotkey::Select => self.tool = Tool::Select,
            tm::Hotkey::Wire => self.tool = Tool::Wire(None),
            tm::Hotkey::Route => self.tool = Tool::Route(None),
            tm::Hotkey::Via => self.tool = Tool::Via,
            tm::Hotkey::Text => {
                self.art_shape = DrawShape::Words;
                self.tool = Tool::DrawLayer;
            }
        }
    }
}
