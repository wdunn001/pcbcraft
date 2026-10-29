//! Everyday editing gestures on the canvas: picking several objects at once
//! (drag box, shift-click), the pick filters, the measure tool, the text
//! clipboard shortcuts and track cleanup. Everything here is reachable by
//! direct manipulation and calls the typed engine commands underneath.

use super::*;
use egui::Pos2;

/// The most objects one selection may hold; matches the clipboard command.
pub(super) const MAX_SELECTED: usize = 64;
/// Distance, in screen points, within which a measure click snaps to a hole,
/// pad, via or part centre.
const MEASURE_SNAP: f64 = 10.0;
/// How far each repeated paste steps away from the copied objects, in mm.
const PASTE_STEP: f64 = 5.0;

/// The four kinds of object the pick filter chips switch on and off.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    Part,
    Copper,
    Graphic,
    Text,
    Other,
}

/// Which kinds of object a click or a drag box may pick up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct SelectionFilter {
    pub parts: bool,
    pub copper: bool,
    pub graphics: bool,
    pub text: bool,
}

impl Default for SelectionFilter {
    fn default() -> Self {
        Self {
            parts: true,
            copper: true,
            graphics: true,
            text: true,
        }
    }
}

impl SelectionFilter {
    pub(super) fn allows(self, kind: Kind) -> bool {
        match kind {
            Kind::Part => self.parts,
            Kind::Copper => self.copper,
            Kind::Graphic => self.graphics,
            Kind::Text => self.text,
            Kind::Other => true,
        }
    }
}

/// Everything the editing gestures remember between frames.
#[derive(Default)]
pub(super) struct EditState {
    /// Selected objects besides the primary one (`App::selected`).
    pub extra: Vec<Id>,
    pub filter: SelectionFilter,
    pub box_from: Option<Point>,
    pub box_to: Option<Point>,
    pub box_extend: bool,
    /// The measure tool: first point, then the second once it is clicked.
    pub measure: Option<(Point, Option<Point>)>,
    /// The last text copied, kept so Paste works inside one window even when
    /// the system clipboard is out of reach.
    pub clip: Option<String>,
    /// What was selected when the clip was taken, to select the copies.
    pub clip_sources: Vec<Id>,
    pub pastes: u32,
    /// Centre of the objects when they were copied; Paste from the menu moves
    /// the copies from here to where the menu was opened.
    pub clip_origin: Option<Point>,
    /// Where the right-click menu was opened, in world millimetres.
    pub menu_at: Option<Point>,
}

/// Distance, dx, dy and angle between two points, as one plain sentence.
pub(super) fn measure_text(a: Point, b: Point) -> String {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let distance = (dx * dx + dy * dy).sqrt();
    let angle = (-dy).atan2(dx).to_degrees();
    format!("Distance {distance:.2} mm · dx {dx:.2} · dy {dy:.2} · angle {angle:.1}°")
}

fn bounds_of_points(points: &[Point]) -> Option<(Point, Point)> {
    let first = points.first()?;
    let mut lo = *first;
    let mut hi = *first;
    for p in points {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    Some((lo, hi))
}

fn around(center: Point, reach: f64) -> (Point, Point) {
    (
        Point::new(center.x - reach, center.y - reach),
        Point::new(center.x + reach, center.y + reach),
    )
}

/// The largest distance from the origin of a set of local drawing points.
fn reach_of(points: &[Point]) -> f64 {
    points
        .iter()
        .map(|p| (p.x * p.x + p.y * p.y).sqrt())
        .fold(1.0_f64, f64::max)
}

impl App {
    /// Every selected object, the primary one first, at most `MAX_SELECTED`.
    pub(super) fn selection(&self) -> Vec<Id> {
        let mut ids: Vec<Id> = vec![];
        for id in self.selected.iter().chain(self.edit.extra.iter()) {
            if !ids.contains(id) && ids.len() < MAX_SELECTED {
                ids.push(*id);
            }
        }
        ids
    }

    pub(super) fn kind_of(&self, id: Id) -> Kind {
        let board = &self.project.board;
        if self.project.component(id).is_some() || self.project.footprint(id).is_some() {
            Kind::Part
        } else if board.tracks.iter().any(|t| t.id == id)
            || board.vias.iter().any(|v| v.id == id)
            || board.zones.iter().any(|z| z.id == id)
        {
            Kind::Copper
        } else if self.project.graphics.iter().any(|g| g.id == id)
            || board.cutouts.iter().any(|c| c.id == id)
            || board.holes.iter().any(|h| h.id == id)
        {
            Kind::Graphic
        } else if board.texts.iter().any(|t| t.id == id)
            || self.project.labels.iter().any(|l| l.id == id)
        {
            Kind::Text
        } else {
            Kind::Other
        }
    }

    pub(super) fn id_allowed(&self, id: Id) -> bool {
        self.edit.filter.allows(self.kind_of(id))
    }

    /// The object under the pointer, skipping kinds the filter chips turned off.
    pub(super) fn pick_filtered(&self, pos: Point, zoom: f32) -> Option<Id> {
        self.pick_object(pos, zoom)
            .filter(|id| self.id_allowed(*id))
    }

    /// A box around one object in world millimetres, for box select and the
    /// selection outline.
    pub(super) fn bounds_of(&self, id: Id) -> Option<(Point, Point)> {
        let p = &self.project;
        let b = &p.board;
        if self.tab == Tab::Schematic {
            if let Some(c) = p.component(id) {
                let mut lo: Option<(Point, Point)> = None;
                for view in c.schematic_views() {
                    let reach = reach_of(&view.drawing_points());
                    let (a, z) = around(view.position, reach);
                    lo = Some(match lo {
                        None => (a, z),
                        Some((l, h)) => (
                            Point::new(l.x.min(a.x), l.y.min(a.y)),
                            Point::new(h.x.max(z.x), h.y.max(z.y)),
                        ),
                    });
                }
                return lo;
            }
            if let Some(g) = p.graphics.iter().find(|g| g.id == id) {
                return bounds_of_points(&g.points);
            }
            return p
                .labels
                .iter()
                .find(|l| l.id == id)
                .map(|l| around(l.position, 1.5));
        }
        if let Some(f) = p.footprint(id) {
            return Some(around(f.position, reach_of(&f.drawing_points(p))));
        }
        if let Some(t) = b.tracks.iter().find(|t| t.id == id) {
            let mut pts = vec![t.start, t.end];
            pts.extend(t.arc);
            return bounds_of_points(&pts);
        }
        if let Some(v) = b.vias.iter().find(|v| v.id == id) {
            return Some(around(v.position, v.diameter / 2.0));
        }
        if let Some(z) = b.zones.iter().find(|z| z.id == id) {
            return bounds_of_points(&z.outline);
        }
        if let Some(c) = b.cutouts.iter().find(|c| c.id == id) {
            return Some(around(c.center, c.radius));
        }
        if let Some(h) = b.holes.iter().find(|h| h.id == id) {
            return Some(around(h.position, h.diameter / 2.0));
        }
        if let Some(t) = b.texts.iter().find(|t| t.id == id) {
            return Some(around(t.position, t.size.max(1.0)));
        }
        p.graphics
            .iter()
            .find(|g| g.id == id && g.layer == "board")
            .and_then(|g| bounds_of_points(&g.points))
    }

    /// Every object on this tab whose box touches the world rectangle and
    /// that the filter chips allow.
    pub(super) fn ids_in_rect(&self, a: Point, b: Point) -> Vec<Id> {
        let (lo, hi) = (
            Point::new(a.x.min(b.x), a.y.min(b.y)),
            Point::new(a.x.max(b.x), a.y.max(b.y)),
        );
        let p = &self.project;
        let mut candidates: Vec<Id> = vec![];
        if self.tab == Tab::Schematic {
            candidates.extend(p.components.iter().map(|c| c.id));
            candidates.extend(p.graphics.iter().map(|g| g.id));
            candidates.extend(p.labels.iter().map(|l| l.id));
        } else {
            let board = &p.board;
            candidates.extend(board.footprints.iter().map(|f| f.id));
            candidates.extend(board.tracks.iter().map(|t| t.id));
            candidates.extend(board.vias.iter().map(|v| v.id));
            candidates.extend(board.zones.iter().map(|z| z.id));
            candidates.extend(board.cutouts.iter().map(|c| c.id));
            candidates.extend(board.holes.iter().map(|h| h.id));
            candidates.extend(board.texts.iter().map(|t| t.id));
            candidates.extend(
                p.graphics
                    .iter()
                    .filter(|g| g.layer == "board")
                    .map(|g| g.id),
            );
        }
        candidates
            .into_iter()
            .filter(|id| self.id_allowed(*id))
            .filter(|id| {
                self.bounds_of(*id)
                    .is_some_and(|(l, h)| l.x <= hi.x && h.x >= lo.x && l.y <= hi.y && h.y >= lo.y)
            })
            .take(MAX_SELECTED)
            .collect()
    }

    /// Make these ids the selection; the first one gets the inspector.
    pub(super) fn set_selection(&mut self, ids: Vec<Id>) {
        let mut ids = ids;
        ids.truncate(MAX_SELECTED);
        match ids.split_first() {
            None => self.select(None),
            Some((first, rest)) => {
                self.select(Some(*first));
                self.edit.extra = rest.to_vec();
            }
        }
    }

    fn selection_message(&mut self) {
        let n = self.selection().len();
        self.message = match n {
            0 => "Nothing selected".into(),
            1 => "1 object selected".into(),
            n => format!("{n} objects selected"),
        };
    }

    /// Shift-click: add the object under the pointer to the selection, or take
    /// it out when it is already in.
    pub(super) fn shift_click(&mut self, pos: Point, zoom: f32) {
        let Some(id) = self.pick_filtered(pos, zoom) else {
            return;
        };
        let mut ids = self.selection();
        if let Some(at) = ids.iter().position(|i| *i == id) {
            ids.remove(at);
        } else if ids.len() < MAX_SELECTED {
            ids.push(id);
        } else {
            self.message = t!(
                "status.shift_click.a_selection_holds_at_most",
                MAX_SELECTED = MAX_SELECTED
            );
            return;
        }
        self.set_selection(ids);
        self.selection_message();
    }

    /// A press on a member of a larger selection keeps the selection and
    /// makes that member the one the inspector shows.
    pub(super) fn promote_member(&mut self, id: Id) -> bool {
        let ids = self.selection();
        if ids.len() < 2 || !ids.contains(&id) {
            return false;
        }
        let mut reordered = vec![id];
        reordered.extend(ids.into_iter().filter(|i| *i != id));
        self.set_selection(reordered);
        true
    }

    pub(super) fn begin_box(&mut self, pos: Point, extend: bool) {
        if !extend {
            self.select(None);
        }
        self.edit.box_from = Some(pos);
        self.edit.box_to = Some(pos);
        self.edit.box_extend = extend;
    }

    pub(super) fn finish_box(&mut self) {
        let (Some(a), Some(b)) = (self.edit.box_from.take(), self.edit.box_to.take()) else {
            return;
        };
        let mut ids = if self.edit.box_extend {
            self.selection()
        } else {
            vec![]
        };
        for id in self.ids_in_rect(a, b) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        self.set_selection(ids);
        self.selection_message();
    }

    /// Where a part sits before a drag moves it.
    pub(super) fn drag_origin(&self, id: Id) -> Option<Point> {
        self.project
            .component(id)
            .and_then(|c| c.unit_placement(self.selected_unit).map(|p| p.0))
            .or_else(|| self.project.footprint(id).map(|f| f.position))
    }

    /// Slide every selected object by the same distance.
    pub(super) fn move_selection(&mut self, dx: f64, dy: f64) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let ids = self.selection();
        let mut moved = 0;
        for id in ids {
            if let Some(c) = self.project.component(id) {
                let to = Point::new(c.position.x + dx, c.position.y + dy);
                self.run("component.move", json!({"id":id,"position":to}));
            } else {
                self.run("object.nudge", json!({"id":id,"dx":dx,"dy":dy}));
            }
            moved += 1;
        }
        self.message = t!(
            "status.move_selection.moved_moved_objects_together",
            moved = moved
        );
    }

    /// Delete every selected object.
    pub(super) fn delete_selection(&mut self) {
        let ids = self.selection();
        for id in &ids {
            if self.project.component(*id).is_some() || self.project_has(*id) {
                self.run("object.delete", json!({"id":id}));
            }
        }
        self.select(None);
        if ids.len() > 1 {
            self.message = t!(
                "status.delete_selection.deleted_len_objects",
                len = ids.len()
            );
        }
    }

    fn project_has(&self, id: Id) -> bool {
        self.project.ids().any(|i| i == id)
    }

    /// Duplicate every selected part a little to the side.
    pub(super) fn duplicate_selection(&mut self) {
        let mut targets: Vec<Id> = vec![];
        for id in self.selection() {
            let target = self.project.footprint(id).map_or(id, |fp| fp.component);
            if self.project.component(target).is_some() && !targets.contains(&target) {
                targets.push(target);
            }
        }
        if targets.is_empty() {
            self.message = t!("status.duplicate_selection.select_a_part_to_duplicate");
            return;
        }
        let count = targets.len();
        self.run(
            "object.duplicate",
            json!({"ids": targets, "offset": {"x": 2.0, "y": 2.0}}),
        );
        if count > 1 {
            self.message = t!(
                "status.duplicate_selection.duplicated_count_parts",
                count = count
            );
        }
    }

    // ----- clipboard -----

    /// Copy the selection to the text clipboard.
    pub(super) fn copy_selection(&mut self, ctx: &egui::Context) {
        let mut ids: Vec<Id> = vec![];
        for id in self.selection() {
            let id = self.project.footprint(id).map_or(id, |fp| fp.component);
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if ids.is_empty() {
            self.message = t!("status.copy_selection.select_something_first_then_copy");
            return;
        }
        let context = if self.tab == Tab::Schematic {
            "schematic"
        } else {
            "board"
        };
        let result = self.run_and_keep("selection.copy_text", json!({"ids":ids,"context":context}));
        if let Some(text) = result.get("text").and_then(Value::as_str) {
            ctx.copy_text(text.to_owned());
            self.edit.clip = Some(text.to_owned());
            self.edit.clip_sources = self.selection();
            self.edit.pastes = 0;
            self.edit.clip_origin = self.selection_centre();
            self.message = t!("status.copy_selection.copied_len_objects", len = ids.len());
        }
    }

    /// The middle of everything selected, in world millimetres.
    fn selection_centre(&self) -> Option<Point> {
        let boxes: Vec<(Point, Point)> = self
            .selection()
            .into_iter()
            .filter_map(|id| self.bounds_of(id))
            .collect();
        let points: Vec<Point> = boxes.iter().flat_map(|(a, b)| [*a, *b]).collect();
        let (lo, hi) = bounds_of_points(&points)?;
        Some(Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0))
    }

    /// Paste clipboard text at a step away from the originals, then select
    /// the new copies so they can be moved straight away.
    pub(super) fn paste_text(&mut self, text: &str) -> bool {
        self.paste_text_at(text, None)
    }

    /// Paste the last copy so its middle lands on `at` (the right-click spot).
    pub(super) fn paste_last_copy_at(&mut self, at: Point) {
        if let Some(text) = self.edit.clip.clone() {
            self.paste_text_at(&text, Some(at));
        } else {
            self.message = t!("status.paste_last_copy_at.nothing_copied_yet");
        }
    }

    fn paste_text_at(&mut self, text: &str, at: Option<Point>) -> bool {
        if !text.starts_with("PCBCRAFT selection/") {
            self.message = t!("status.paste_text_at.that_text_is_not_copied");
            return false;
        }
        let (dx, dy) = match (at, self.edit.clip_origin) {
            (Some(at), Some(from)) => (at.x - from.x, at.y - from.y),
            _ => {
                self.edit.pastes += 1;
                let step = PASTE_STEP * f64::from(self.edit.pastes);
                (step, step)
            }
        };
        let result = self.run_and_keep(
            "selection.paste",
            json!({"text":text,"offset":{"x":dx,"y":dy},"renumber":true}),
        );
        self.refresh();
        let Some(map) = result.get("ids").and_then(Value::as_object) else {
            return true;
        };
        let count = result.get("count").and_then(Value::as_u64).unwrap_or(0);
        let mut fresh: Vec<Id> = self
            .edit
            .clip_sources
            .iter()
            .filter_map(|old| map.get(&old.to_string()).and_then(Value::as_u64))
            .collect();
        if fresh.is_empty() {
            fresh = map.values().filter_map(Value::as_u64).collect();
            fresh.sort_unstable();
        }
        fresh.retain(|id| self.project_has(*id));
        self.set_selection(fresh);
        self.message = t!(
            "status.paste_text_at.pasted_count_objects_drag_them",
            count = count
        );
        true
    }

    pub(super) fn paste_last_copy(&mut self) {
        if let Some(text) = self.edit.clip.clone() {
            self.paste_text(&text);
        } else {
            self.message = t!("status.paste_last_copy.nothing_copied_yet");
        }
    }

    /// Copy, paste, select all and the measure key.
    pub(super) fn edit_events(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || ctx.current_pass_index() > 0 {
            return;
        }
        let (copy, paste, select_all, measure) = ctx.input(|i| {
            let mut copy = false;
            let mut paste: Option<String> = None;
            for event in &i.events {
                match event {
                    egui::Event::Copy => copy = true,
                    egui::Event::Paste(text) => paste = Some(text.clone()),
                    _ => {}
                }
            }
            (
                copy,
                paste,
                i.modifiers.command && i.key_pressed(egui::Key::A),
                !i.modifiers.command && i.key_pressed(egui::Key::U),
            )
        });
        if copy {
            self.copy_selection(ctx);
        }
        if let Some(text) = paste
            && !self.paste_text(&text)
        {
            self.paste_last_copy();
        }
        if select_all && matches!(self.tab, Tab::Schematic | Tab::Pcb) {
            let everything = self.ids_in_rect(Point::new(-1.0e6, -1.0e6), Point::new(1.0e6, 1.0e6));
            self.set_selection(everything);
            self.selection_message();
        }
        if measure && matches!(self.tab, Tab::Schematic | Tab::Pcb) {
            self.arm_measure();
        }
    }

    // ----- measure -----

    pub(super) fn arm_measure(&mut self) {
        self.tool = Tool::Measure;
        self.edit.measure = None;
    }

    /// The nearest hole, pad, via or part centre within a fingertip, else the
    /// click itself.
    fn measure_snap(&self, pos: Point, zoom: f32) -> Point {
        let reach = MEASURE_SNAP / f64::from(zoom.max(0.001));
        let mut best: Option<(f64, Point)> = None;
        let mut consider = |at: Point| {
            let d = at.distance(pos);
            if d <= reach && best.is_none_or(|(known, _)| d < known) {
                best = Some((d, at));
            }
        };
        if self.tab == Tab::Pcb {
            for h in &self.project.board.holes {
                consider(h.position);
            }
            for v in &self.project.board.vias {
                consider(v.position);
            }
            for f in &self.project.board.footprints {
                for pad in &f.pads {
                    if let Some(at) = f.pad_position(&pad.number) {
                        consider(at);
                    }
                }
            }
        } else {
            for c in &self.project.components {
                for pin in &c.pins {
                    if let Some(at) = c.pin_position(&pin.number) {
                        consider(at);
                    }
                }
            }
        }
        best.map_or(pos, |(_, at)| at)
    }

    pub(super) fn measure_click(&mut self, pos: Point, zoom: f32) {
        let at = self.measure_snap(pos, zoom);
        match self.edit.measure {
            Some((first, None)) => {
                self.edit.measure = Some((first, Some(at)));
                self.message = measure_text(first, at);
            }
            _ => {
                self.edit.measure = Some((at, None));
                self.message = t!("status.measure_click.measure_click_the_second_point");
            }
        }
    }

    /// The finished reading, for the status strip.
    pub(super) fn measure_readout(&self) -> Option<String> {
        match self.edit.measure {
            Some((a, Some(b))) => Some(measure_text(a, b)),
            _ => None,
        }
    }

    // ----- cleanup -----

    pub(super) fn cleanup_tracks(&mut self) {
        let result = self.run_and_keep("track.cleanup", json!({}));
        if result.is_null() {
            if self.message.contains("no track cleanup") {
                self.message = t!("status.cleanup_tracks.track_cleanup_found_nothing_to");
            }
            return;
        }
        let removed = result.get("removed").and_then(Value::as_u64).unwrap_or(0);
        let merged = result.get("merged").and_then(Value::as_u64).unwrap_or(0);
        self.message = if removed + merged == 0 {
            t!("status.cleanup_tracks.track_cleanup_found_nothing_to")
        } else {
            t!(
                "status.cleanup_tracks.track_cleanup_removed_removed_pieces",
                removed = removed,
                merged = merged
            )
        };
        self.refresh();
    }

    // ----- drawing and controls -----

    /// Selection outlines for a multiple selection, the drag box and the
    /// measure line.
    pub(super) fn paint_edit_overlay(&self, painter: &egui::Painter, rect: Rect, camera: Camera) {
        let accent = self.accent();
        let ids = self.selection();
        if ids.len() > 1 {
            for id in ids {
                if let Some((lo, hi)) = self.bounds_of(id) {
                    let r = Rect::from_two_pos(camera.screen(lo, rect), camera.screen(hi, rect));
                    painter.rect_stroke(
                        r.expand(3.0),
                        2.0,
                        Stroke::new(1.5, accent),
                        egui::StrokeKind::Outside,
                    );
                }
            }
        }
        if let (Some(a), Some(b)) = (self.edit.box_from, self.edit.box_to) {
            let r = Rect::from_two_pos(camera.screen(a, rect), camera.screen(b, rect));
            painter.rect_filled(r, 0.0, accent.gamma_multiply(0.12));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, accent), egui::StrokeKind::Inside);
        }
        if let Some((a, second)) = self.edit.measure {
            let from = camera.screen(a, rect);
            painter.circle_filled(from, 4.0, accent);
            if let Some(b) = second {
                let to = camera.screen(b, rect);
                painter.line_segment([from, to], Stroke::new(1.5, accent));
                painter.circle_filled(to, 4.0, accent);
                let mid = Pos2::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
                painter.text(
                    mid + Vec2::new(8.0, -8.0),
                    Align2::LEFT_BOTTOM,
                    format!("{:.2} mm", a.distance(b)),
                    FontId::monospace(12.0),
                    self.ink(),
                );
            }
        }
    }

    /// The pick filter chips and the measure button, in the tool strip.
    pub(super) fn editing_controls(&mut self, ui: &mut egui::Ui) {
        if ui
            .selectable_label(matches!(self.tool, Tool::Measure), "Measure")
            .on_hover_text(
                "Click two points to read the distance, the across and up-down parts and the angle. Holes, pads and pins snap. Shortcut U; Esc clears.",
            )
            .clicked()
        {
            self.arm_measure();
        }
        ui.separator();
        ui.label("Pick:")
            .on_hover_text("Choose which kinds of object a click or a drag box may pick up.");
        let chips = [
            (
                "Parts",
                "Parts and their symbols. Turn off to pick only the rest.",
            ),
            (
                "Copper",
                "Tracks, vias and copper zones. Turn off to leave wiring alone.",
            ),
            ("Graphics", "Drawn shapes, cutouts and holes."),
            ("Text", "Text labels and net labels."),
        ];
        for (label, tip) in chips {
            let f = &mut self.edit.filter;
            let flag = match label {
                "Parts" => &mut f.parts,
                "Copper" => &mut f.copper,
                "Graphics" => &mut f.graphics,
                _ => &mut f.text,
            };
            if ui
                .selectable_label(*flag, label)
                .on_hover_text(tip)
                .clicked()
            {
                *flag = !*flag;
            }
        }
    }
}
