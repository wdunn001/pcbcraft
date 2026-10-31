//! Schematic wire drafting, net conflict resolution, and bus/label tools (#291).

use crate::canvas::snap;
use crate::{App, Tab, Tool};
use egui;
use pcbcraft_doc::{Id, PinRef, Point};
use pcbcraft_geom::point_segment;
use serde_json::{json, Value};

/// Which schematic label tool is armed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SchematicLabelKind {
    Local,
    Power,
}

/// A `wire.place` the engine refused until the user picks or names a net.
#[derive(Clone, Debug)]
pub(super) struct WirePlacePending {
    pub params: Value,
    pub candidates: Vec<(Id, String)>,
    pub pick: usize,
    pub new_name: String,
}

/// Bus polyline waiting for a stem*count name.
#[derive(Clone, Debug)]
pub(super) struct BusNamePrompt {
    pub points: Vec<Point>,
    pub text: String,
}

impl App {
    pub(super) fn clear_wire_draft(&mut self) {
        self.wire_route.clear();
        self.wire_place_pending = None;
        self.bus_name_prompt = None;
    }

    pub(super) fn arm_wire_tool(&mut self) {
        self.clear_wire_draft();
        self.tool = Tool::Wire(None);
    }

    pub(super) fn cancel_schematic_gestures(&mut self) {
        self.clear_wire_draft();
    }

    /// Snap a schematic click for wire/bus drafting, honouring live angle locks.
    pub(super) fn wire_click_point(&self, pos: Point) -> Point {
        let at = snap(pos);
        if let Tool::Wire(Some(_)) = &self.tool {
            if let Some(anchor) = self.wire_route.last().copied() {
                if self.edit.live.contains(crate::tool_modifiers::ModAction::ConstrainAngle) {
                    let (x, y) =
                        crate::tool_modifiers::constrain_angle((anchor.x, anchor.y), (at.x, at.y));
                    return Point::new(x, y);
                }
            }
        }
        at
    }

    fn execute_raw(&mut self, command: &str, params: Value) -> Result<Value, String> {
        self.session
            .lock()
            .map_err(|_| "session unavailable".to_string())
            .and_then(|mut s| {
                s.execute(command, params, Some(self.revision))
                    .map_err(|e| e.to_string())
            })
    }

    pub(super) fn try_wire_place(&mut self, params: Value) -> bool {
        match self.execute_raw("wire.place", params.clone()) {
            Ok(_) => {
                self.message = "Wire placed.".into();
                self.clear_wire_draft();
                self.tool = Tool::Select;
                self.refresh();
                true
            }
            Err(message) => {
                if let Ok(body) = serde_json::from_str::<Value>(&message)
                    && body.get("code").and_then(Value::as_str) == Some("net_conflict")
                {
                    let candidates = body
                        .get("candidates")
                        .and_then(Value::as_array)
                        .map(|rows| {
                            rows.iter()
                                .filter_map(|row| {
                                    let id = row.get("id")?.as_u64()?;
                                    let name = row
                                        .get("name")
                                        .and_then(Value::as_str)
                                        .unwrap_or("")
                                        .to_string();
                                    Some((id, name))
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if candidates.is_empty() {
                        self.message = message;
                        return false;
                    }
                    self.wire_place_pending = Some(WirePlacePending {
                        params,
                        candidates,
                        pick: 0,
                        new_name: String::new(),
                    });
                    return false;
                }
                self.message = message;
                false
            }
        }
    }

    fn finish_wire_to_pin(&mut self, end: PinRef, end_pos: Point) {
        let Tool::Wire(Some(start)) = self.tool.clone() else {
            return;
        };
        let mut points = self.wire_route.clone();
        if points.is_empty() {
            if let Some(at) = self.project.pin_position(&start) {
                points.push(at);
            }
        }
        if points.last().copied() != Some(end_pos) {
            points.push(end_pos);
        }
        if points.len() == 2 && start != end {
            self.run(
                "wire.connect",
                json!({"from": start, "to": end}),
            );
            self.clear_wire_draft();
            self.tool = Tool::Select;
            return;
        }
        let params = json!({
            "points": points,
            "pins": [{"component": start.component, "pin": start.pin}, {"component": end.component, "pin": end.pin}]
        });
        if !self.try_wire_place(params) && self.wire_place_pending.is_none() {
            // error already in message
        }
    }

    fn finish_wire_tee(&mut self, wire_id: Id, at: Point) {
        let Tool::Wire(Some(_start)) = self.tool.clone() else {
            return;
        };
        let mut points = self.wire_route.clone();
        if points.last().copied() != Some(at) {
            points.push(at);
        }
        let mut params = json!({
            "points": points,
            "tee": {"wire": wire_id, "at": {"x": at.x, "y": at.y}}
        });
        if let Tool::Wire(Some(start)) = &self.tool {
            params["pins"] = json!([{"component": start.component, "pin": start.pin}]);
        }
        let _ = self.try_wire_place(params);
    }

    pub(super) fn schematic_wire_click(&mut self, pos: Point, zoom: f32) {
        if self.tab != Tab::Schematic {
            return;
        }
        match &self.tool {
            Tool::Wire(start) => self.wire_tool_click(pos, zoom, start.clone()),
            Tool::SchematicLabel(kind) => self.schematic_label_click(pos, zoom, *kind),
            Tool::Bus => self.bus_tool_click(pos),
            _ => {}
        }
    }

    fn wire_tool_click(&mut self, pos: Point, zoom: f32, start: Option<PinRef>) {
        if let Some(pin) = self.pick_pin(pos, zoom) {
            if let Some(start_pin) = start {
                if start_pin == pin {
                    return;
                }
                let Some(at) = self.project.pin_position(&pin) else {
                    self.message = "That pin has no position.".into();
                    return;
                };
                self.finish_wire_to_pin(pin, at);
            } else if let Some(at) = self.project.pin_position(&pin) {
                self.wire_route = vec![at];
                self.tool = Tool::Wire(Some(pin));
            }
            return;
        }
        if let Some((wire, at)) = self.pick_wire_tee(pos, zoom) {
            if start.is_some() && !self.wire_route.is_empty() {
                let bend = self.wire_click_point(at);
                self.wire_route.push(bend);
                self.finish_wire_tee(wire, bend);
            }
            return;
        }
        let Some(_start_pin) = start else {
            self.message = "Pick a pin to start the wire.".into();
            return;
        };
        let bend = self.wire_click_point(pos);
        if self.wire_route.len() >= 256 {
            self.message = "A wire can hold at most 256 vertices.".into();
            return;
        }
        self.wire_route.push(bend);
    }

    fn pick_wire_tee(&self, pos: Point, zoom: f32) -> Option<(Id, Point)> {
        let reach = 5.0 / zoom as f64;
        let mut best: Option<(Id, Point, f64)> = None;
        for w in &self.project.drawn_wires {
            for pair in w.points.windows(2) {
                let dist = point_segment(pos, pair[0], pair[1]);
                if dist > reach {
                    continue;
                }
                let at = snap(project_on_segment(pos, pair[0], pair[1]));
                let score = dist;
                if best.as_ref().is_none_or(|(_, _, d)| score < *d) {
                    best = Some((w.id, at, score));
                }
            }
        }
        best.map(|(id, at, _)| (id, at))
    }

    fn schematic_label_click(&mut self, pos: Point, zoom: f32, kind: SchematicLabelKind) {
        let at = snap(pos);
        let mut net = self
            .net_at_schematic_point(at, zoom)
            .or_else(|| self.highlight_net);
        if net.is_none() {
            net = self.bus_member_at(at, zoom);
        }
        let Some(net) = net else {
            self.message = "Pick a net on the drawing to label.".into();
            return;
        };
        let label_kind = match kind {
            SchematicLabelKind::Local => "label",
            SchematicLabelKind::Power => "power",
        };
        self.run(
            "label.place",
            json!({"kind": label_kind, "net": net, "position": {"x": at.x, "y": at.y}, "rotation": 0.0}),
        );
        self.tool = Tool::Select;
    }

    fn bus_member_at(&self, pos: Point, zoom: f32) -> Option<Id> {
        let reach = 5.0 / zoom as f64;
        for w in &self.project.drawn_wires {
            if w.net.is_some() {
                continue;
            }
            let on = w.points.windows(2).any(|pair| point_segment(pos, pair[0], pair[1]) <= reach);
            if !on {
                continue;
            }
            let head = w.points.first().copied()?;
            let bus_label = self.project.labels.iter().find(|l| {
                l.kind == "bus" && l.position.distance(head) < 0.05
            })?;
            let (stem, _count) = parse_bus_notation(&bus_label.text)?;
            let name = format!("{stem}0");
            return self.project.nets.iter().find(|n| n.name == name).map(|n| n.id);
        }
        None
    }

    fn net_at_schematic_point(&self, pos: Point, zoom: f32) -> Option<Id> {
        if let Some(pin) = self.pick_pin(pos, zoom) {
            return self.project.pin(&pin).ok()?.net;
        }
        let reach = 5.0 / zoom as f64;
        for w in &self.project.drawn_wires {
            for pair in w.points.windows(2) {
                if point_segment(pos, pair[0], pair[1]) <= reach {
                    return w.net;
                }
            }
        }
        None
    }

    fn bus_tool_click(&mut self, pos: Point) {
        let at = self.wire_click_point(pos);
        if self.bus_name_prompt.is_some() {
            return;
        }
        if self.bus_draft.len() >= 256 {
            self.message = "A bus can hold at most 256 vertices.".into();
            return;
        }
        self.bus_draft.push(at);
        if self.bus_draft.len() >= 2 {
            self.bus_name_prompt = Some(BusNamePrompt {
                points: self.bus_draft.clone(),
                text: "DATA*8".into(),
            });
        }
    }

    pub(super) fn wire_place_prompt(&mut self, ctx: &egui::Context) {
        let Some(mut pending) = self.wire_place_pending.take() else {
            return;
        };
        let mut keep = true;
        let mut apply_net = None::<Id>;
        let mut apply_new = false;
        egui::Window::new("Choose a net")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("These pins belong to different nets. Pick one or type a new name.");
                ui.add_space(4.0);
                for (i, (_, name)) in pending.candidates.iter().enumerate() {
                    if ui
                        .selectable_label(pending.pick == i, name)
                        .on_hover_text("Use this net for the new wire.")
                        .clicked()
                    {
                        pending.pick = i;
                    }
                }
                ui.add_space(6.0);
                ui.label("Or name a new net:");
                ui.text_edit_singleline(&mut pending.new_name)
                    .on_hover_text("Creates a net with this name and merges the pins onto it.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button("Use selected net")
                        .on_hover_text("Commit the wire on the highlighted net.")
                        .clicked()
                        && pending.pick < pending.candidates.len()
                    {
                        apply_net = Some(pending.candidates[pending.pick].0);
                    }
                    if ui
                        .button("Create net")
                        .on_hover_text("Create a new net with the typed name.")
                        .clicked()
                        && !pending.new_name.trim().is_empty()
                    {
                        apply_new = true;
                    }
                    if ui.button("Cancel").on_hover_text("Drop the wire.").clicked() {
                        keep = false;
                    }
                });
            });
        if !keep {
            self.clear_wire_draft();
            self.tool = Tool::Select;
            return;
        }
        if let Some(net) = apply_net {
            let mut params = pending.params;
            if let Some(obj) = params.as_object_mut() {
                obj.insert("net".into(), json!(net));
            }
            let _ = self.try_wire_place(params);
            return;
        }
        if apply_new {
            let name = pending.new_name.trim().to_string();
            let created = self.run_and_keep("net.create", json!({"name": name}));
            if let Some(net) = created.get("id").and_then(Value::as_u64) {
                let mut params = pending.params;
                if let Some(obj) = params.as_object_mut() {
                    obj.insert("net".into(), json!(net));
                }
                let _ = self.try_wire_place(params);
                return;
            }
        }
        self.wire_place_pending = Some(pending);
    }

    pub(super) fn bus_name_prompt(&mut self, ctx: &egui::Context) {
        let Some(mut ask) = self.bus_name_prompt.take() else {
            return;
        };
        let mut keep = true;
        let mut apply = false;
        egui::Window::new("Name the bus")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Use stem*count notation, for example DATA*8.");
                ui.add_space(4.0);
                ui.text_edit_singleline(&mut ask.text)
                    .on_hover_text("The bus name at the first vertex.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button("Draw bus")
                        .on_hover_text("Place the bus wire and member nets.")
                        .clicked()
                    {
                        apply = true;
                    }
                    if ui.button("Cancel").on_hover_text("Drop the bus.").clicked() {
                        keep = false;
                    }
                });
            });
        if !keep {
            self.bus_draft.clear();
            self.tool = Tool::Select;
            return;
        }
        if apply {
            let name = ask.text.trim().to_string();
            if name.is_empty() {
                self.message = "Enter a bus name.".into();
                self.bus_name_prompt = Some(ask);
                return;
            }
            let points: Vec<Value> = ask
                .points
                .iter()
                .map(|p| json!({"x": p.x, "y": p.y}))
                .collect();
            self.run("wire.draw_bus", json!({"points": points, "name": name}));
            self.bus_draft.clear();
            self.tool = Tool::Select;
            return;
        }
        self.bus_name_prompt = Some(ask);
    }

    pub(super) fn wire_draft_preview(&self, cursor: Point) -> Vec<Point> {
        let mut out = self.wire_route.clone();
        if let Tool::Wire(Some(_)) = &self.tool {
            if let Some(from) = out.last().copied() {
                let tail = self.wire_preview_points(from, cursor);
                if tail.len() >= 2 {
                    out.extend_from_slice(&tail[1..]);
                }
            }
        }
        out
    }
}

fn parse_bus_notation(name: &str) -> Option<(String, u32)> {
    let (stem, count) = name.split_once('*')?;
    let count: u32 = count.parse().ok()?;
    if stem.is_empty() || count == 0 {
        return None;
    }
    Some((stem.to_string(), count))
}

fn project_on_segment(p: Point, a: Point, b: Point) -> Point {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return a;
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    Point::new(a.x + t * dx, a.y + t * dy)
}
