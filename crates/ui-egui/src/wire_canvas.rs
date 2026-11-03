//! Schematic wire drafting, net conflict resolution, and bus/label tools (#291).

use crate::canvas::snap;
use crate::{App, Tab, Tool};
use pcbcraft_doc::{DrawnWire, Id, Label, PinRef};
use pcbcraft_geom::Point;
use pcbcraft_geom::point_segment;
use pcbcraft_text::t;
use serde_json::{Value, json};

/// Which schematic label tool is armed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SchematicLabelKind {
    Local,
    Power,
}

/// What the armed Label or Power tool will place with the next click.
///
/// # Rules
/// - `name` is the net name typed for a Label; empty means use the net under
///   the click.
/// - `power_net` is the net a Power symbol sets, and `style` indexes
///   `Label::POWER_STYLES`.
///
/// # Dependencies
/// - crates/ui-egui/src/editing.rs (`EditState` holds one)
/// - crates/doc/src/lib.rs (`Label::POWER_STYLES` names the symbols)
///
/// # Do not
/// - Keep a net id here: only names, so a deleted net cannot dangle.
#[derive(Clone, Debug)]
pub(super) struct LabelDraft {
    pub name: String,
    pub power_net: String,
    pub style: usize,
}

impl Default for LabelDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            power_net: "3V3".to_string(),
            style: 1,
        }
    }
}

/// Supply names offered as one-click presets next to the Power net field.
const POWER_PRESETS: [&str; 5] = ["3V3", "5V", "12V", "VCC", "GND"];

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
        if let Tool::Wire(Some(_)) = &self.tool
            && let Some(anchor) = self.wire_route.last().copied()
            && self
                .edit
                .live
                .contains(crate::tool_modifiers::ModAction::ConstrainAngle)
        {
            let (x, y) = crate::tool_modifiers::constrain_angle((anchor.x, anchor.y), (at.x, at.y));
            return Point::new(x, y);
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
        if points.is_empty()
            && let Some(at) = self.project.pin_position(&start)
        {
            points.push(at);
        }
        if points.last().copied() != Some(end_pos) {
            points.push(end_pos);
        }
        if points.len() == 2 && start != end {
            self.run("wire.connect", json!({"from": start, "to": end}));
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

    /// The drawn wires that sit on schematic page `page`, so a pick or a snap
    /// never reaches a wire of another page.
    fn on_this_page_wires(&self, page: Id) -> impl Iterator<Item = &DrawnWire> {
        self.project
            .drawn_wires
            .iter()
            .filter(move |w| self.project.schematic_drawn_wire_page(w) == page)
    }

    fn pick_wire_tee(&self, pos: Point, zoom: f32) -> Option<(Id, Point)> {
        let reach = 5.0 / zoom as f64;
        let mut best: Option<(Id, Point, f64)> = None;
        let page = self.schematic_page();
        for w in self.on_this_page_wires(page) {
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
        let position = json!({"x": at.x, "y": at.y});
        let clicked = self.net_at_schematic_point(at, zoom);
        if kind == SchematicLabelKind::Power {
            let name = self.edit.label.power_net.trim().to_string();
            if name.is_empty() {
                self.message = t!("schematic.power.need_net");
                return;
            }
            let style = Label::POWER_STYLES
                .get(self.edit.label.style)
                .copied()
                .unwrap_or("bar");
            let mut params = json!({"kind": "power", "name": name, "style": style,
                "position": position, "rotation": 0.0});
            if let Some(net) = clicked {
                params["net"] = json!(net);
            }
            self.run("label.place", params);
            return;
        }
        let typed = self.edit.label.name.trim().to_string();
        if !typed.is_empty() {
            let mut params = json!({"kind": "label", "name": typed,
                "position": position, "rotation": 0.0});
            if let Some(net) = clicked {
                params["net"] = json!(net);
            }
            self.run("label.place", params);
            return;
        }
        let mut net = clicked.or(self.highlight_net);
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
    }

    fn bus_member_at(&self, pos: Point, zoom: f32) -> Option<Id> {
        let reach = 5.0 / zoom as f64;
        for w in self.on_this_page_wires(self.schematic_page()) {
            if w.net.is_some() {
                continue;
            }
            let on = w
                .points
                .windows(2)
                .any(|pair| point_segment(pos, pair[0], pair[1]) <= reach);
            if !on {
                continue;
            }
            let head = w.points.first().copied()?;
            let bus_label = self
                .project
                .labels
                .iter()
                .find(|l| l.kind == "bus" && l.position.distance(head) < 0.05)?;
            let (stem, _count) = parse_bus_notation(&bus_label.text)?;
            let name = format!("{stem}0");
            return self
                .project
                .nets
                .iter()
                .find(|n| n.name == name)
                .map(|n| n.id);
        }
        None
    }

    fn net_at_schematic_point(&self, pos: Point, zoom: f32) -> Option<Id> {
        if let Some(pin) = self.pick_pin(pos, zoom) {
            return self.project.pin(&pin)?.net;
        }
        let reach = 5.0 / zoom as f64;
        for w in self.on_this_page_wires(self.schematic_page()) {
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
                    if ui
                        .button("Cancel")
                        .on_hover_text("Drop the wire.")
                        .clicked()
                    {
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

    /// The small panel over the sheet while the Label or Power tool is armed:
    /// the net name to type, or the power style and net to pick.
    ///
    /// # Rules
    /// - Shown only on the Schematic tab with a label tool armed.
    /// - Edits the draft only; the next canvas click places the label.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/a11y.rs (`Named` gives each field its spoken name)
    /// - crates/text/strings/en.txt (every label and tooltip)
    ///
    /// # Do not
    /// - Place anything from here; placing is the canvas click.
    pub(super) fn label_options(&mut self, ctx: &egui::Context) {
        use crate::a11y::Named;
        let Tool::SchematicLabel(kind) = self.tool else {
            return;
        };
        if self.tab != Tab::Schematic {
            return;
        }
        let at = self.canvas_rect.right_top() + egui::vec2(-12.0, 12.0);
        egui::Area::new(egui::Id::new("label-options"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_TOP)
            .fixed_pos(at)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| match kind {
                    SchematicLabelKind::Local => {
                        ui.horizontal(|ui| {
                            ui.label(t!("schematic.label.name"));
                            ui.add(
                                crate::targets::text_field(&mut self.edit.label.name)
                                    .hint_text(t!("schematic.label.name.hint"))
                                    .desired_width(160.0),
                            )
                            .on_hover_text(t!("schematic.label.name.tip"))
                            .named(&t!("schematic.label.name"));
                        });
                    }
                    SchematicLabelKind::Power => {
                        ui.horizontal(|ui| {
                            ui.label(t!("schematic.power.net"));
                            ui.add(
                                crate::targets::text_field(&mut self.edit.label.power_net)
                                    .desired_width(90.0),
                            )
                            .on_hover_text(t!("schematic.power.net.tip"))
                            .named(&t!("schematic.power.net"));
                        });
                        ui.horizontal(|ui| {
                            for preset in POWER_PRESETS {
                                if ui
                                    .button(preset)
                                    .on_hover_text(t!("schematic.power.preset.tip", name = preset))
                                    .clicked()
                                {
                                    self.edit.label.power_net = preset.to_string();
                                }
                            }
                        });
                        let names = [
                            t!("schematic.power.style.ground"),
                            t!("schematic.power.style.bar"),
                            t!("schematic.power.style.arrow"),
                            t!("schematic.power.style.negative"),
                        ];
                        let chosen = self.edit.label.style.min(names.len() - 1);
                        egui::ComboBox::from_label(t!("schematic.power.style"))
                            .selected_text(names[chosen].clone())
                            .show_ui(ui, |ui| {
                                for (i, name) in names.iter().enumerate() {
                                    ui.selectable_value(&mut self.edit.label.style, i, name);
                                }
                            })
                            .response
                            .named(&t!("schematic.power.style"))
                            .on_hover_text(t!("schematic.power.style.tip"));
                    }
                });
            });
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
        if let Tool::Wire(Some(_)) = &self.tool
            && let Some(from) = out.last().copied()
        {
            let tail = self.wire_preview_points(from, cursor);
            if tail.len() >= 2 {
                out.extend_from_slice(&tail[1..]);
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
