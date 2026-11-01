//! The Via tool: a click anywhere on the board drops a via.
//!
//! Context: the tool used to place a via only on a track or pad that already had a net, so on a
//! board with no tracks it looked dead. Now the net is the one under the cursor, else the one
//! picked in the tool options, else none (an unassigned via the design checks report).
//! Rules: placement always goes through the typed `via.place` command; the plan here only
//! decides the arguments and refuses early, in plain words, what the checks would flag anyway
//! (outside the board, too close to the edge, ring too thin, too close to other copper).
//! The options sit in the right aside for now; the canvas options strip of #1169 will carry them.
//! Do not: read the clock or draw with text glyphs; the ghost is plain circles in layer colours.

use super::*;
use crate::a11y::Named;
use crate::canvas::gestures::Hit;
use crate::canvas::snap;
use pcbcraft_doc::{Layer, ViaKind};

/// What the person chose in the Via tool options. Empty choices follow the board rules.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ViaToolOptions {
    /// Net picked for clicks that land on no copper; none means Auto (no net there).
    pub net: Option<Id>,
    /// Copper span as (upper, lower) layer indices; none means through.
    pub span: Option<(u8, u8)>,
    /// Pad diameter in millimetres; none follows the net class or the fabrication rules.
    pub diameter: Option<f64>,
    /// Drill in millimetres; none follows the net class or the fabrication rules.
    pub drill: Option<f64>,
    /// Snap the via to the grid.
    pub snap: bool,
}

impl Default for ViaToolOptions {
    fn default() -> Self {
        Self {
            net: None,
            span: None,
            diameter: None,
            drill: None,
            snap: true,
        }
    }
}

/// Where a via's net came from, for the messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NetSource {
    /// A track, pad or via under the cursor.
    Cursor,
    /// The net picked in the options.
    Options,
    /// Nothing gave one.
    None,
}

/// Everything one click would hand to `via.place`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ViaPlan {
    /// Centre in millimetres, snapped when snap is on.
    pub position: Point,
    /// Net id, 0 for none.
    pub net: Id,
    /// Where the net came from.
    pub source: NetSource,
    /// Upper and lower copper layer indices.
    pub span: (u8, u8),
    /// Pad diameter in millimetres.
    pub diameter: f64,
    /// Drill in millimetres.
    pub drill: f64,
}

/// Through, blind or buried for a span on a board with `count` copper layers.
fn kind_of(span: (u8, u8), count: u8) -> ViaKind {
    let last = count.saturating_sub(1);
    match (span.0 == 0, span.1 >= last) {
        (true, true) => ViaKind::Through,
        (false, false) => ViaKind::Buried,
        _ => ViaKind::Blind,
    }
}

/// Plain word for a via kind in the options.
fn kind_word(kind: ViaKind) -> String {
    match kind {
        ViaKind::Blind => t!("via_tool.type_blind"),
        ViaKind::Buried => t!("via_tool.type_buried"),
        _ => t!("via_tool.type_through"),
    }
}

impl App {
    /// Size a via takes when the options leave it open: the net class when it names one, else
    /// the fabrication minimums with a margin.
    pub(crate) fn via_default_size(&self, net: Id) -> (f64, f64) {
        let class = self.project.net_class(net);
        let rules = self.project.board.rules.as_ref();
        let drill = class
            .and_then(|c| c.via_drill)
            .unwrap_or_else(|| rules.and_then(|r| r.min_drill).unwrap_or(0.0).max(0.3));
        let diameter = class.and_then(|c| c.via_diameter).unwrap_or_else(|| {
            let ring = rules.and_then(|r| r.min_ring_via).unwrap_or(0.0).max(0.15);
            ((drill + 2.0 * ring) * 20.0).ceil() / 20.0
        });
        (diameter, drill)
    }

    /// The span the options select, or through when none is chosen or the board no longer allows it.
    fn via_option_span(&self) -> (u8, u8) {
        let last = self.project.copper_layer_count().saturating_sub(1);
        let allowed = self.project.allowed_via_spans();
        match self.via_tool.span {
            Some(s) if allowed.iter().any(|(a, b)| (a.index(), b.index()) == s) => s,
            _ => (0, last),
        }
    }

    /// The net of the track, pad or via under a board point, if it has one.
    pub(crate) fn via_net_under(&self, pos: Point, zoom: f32) -> Option<Id> {
        let net = match self.hit_at(pos, zoom) {
            Hit::TrackSegment(id) | Hit::TrackEnd(id, _) => self
                .project
                .board
                .tracks
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.net),
            Hit::Via(id) => self
                .project
                .board
                .vias
                .iter()
                .find(|v| v.id == id)
                .map(|v| v.net),
            Hit::Pad(f, pad) => self
                .project
                .footprint(f)
                .and_then(|fp| fp.pads.iter().find(|p| p.number == pad).and_then(|p| p.net)),
            _ => None,
        };
        net.filter(|n| *n != 0)
    }

    /// What a click at `pos` would place.
    pub(crate) fn via_plan(&self, pos: Point, zoom: f32) -> ViaPlan {
        let (net, source) = match self.via_net_under(pos, zoom) {
            Some(n) => (n, NetSource::Cursor),
            None => match self.via_tool.net {
                Some(n) if self.project.nets.iter().any(|x| x.id == n) => (n, NetSource::Options),
                _ => (0, NetSource::None),
            },
        };
        let (diameter, drill) = self.via_default_size(net);
        let diameter = self.via_tool.diameter.unwrap_or(diameter);
        let drill = self.via_tool.drill.unwrap_or(drill);
        let position = if self.via_tool.snap { snap(pos) } else { pos };
        ViaPlan {
            position,
            net,
            source,
            span: self.via_option_span(),
            diameter,
            drill,
        }
    }

    /// The plain reason this via cannot go here, or none when it can.
    pub(crate) fn via_refusal(&self, plan: &ViaPlan) -> Option<String> {
        let at = plan.position;
        let radius = plan.diameter / 2.0;
        let board = &self.project.board;
        let where_ = (format!("{:.1}", at.x), format!("{:.1}", at.y));
        let ring = board.edge_ring(0.05);
        let (inside, edge) = if ring.len() >= 3 {
            (
                pcbcraft_geom::point_in_polygon(at, &ring),
                pcbcraft_geom::polygon_boundary_distance(at, &ring),
            )
        } else {
            (
                at.x >= 0.0 && at.y >= 0.0 && at.x <= board.width && at.y <= board.height,
                [at.x, at.y, board.width - at.x, board.height - at.y]
                    .into_iter()
                    .fold(f64::INFINITY, f64::min),
            )
        };
        if !inside {
            return Some(t!("via_tool.refuse_outside", x = where_.0, y = where_.1));
        }
        let edge_gap = board
            .rules
            .as_ref()
            .and_then(|r| r.edge_routed)
            .unwrap_or(0.0);
        if edge < radius + edge_gap {
            return Some(t!(
                "via_tool.refuse_edge",
                gap = format!("{:.2}", radius + edge_gap)
            ));
        }
        if let Some(min) = board.rules.as_ref().and_then(|r| r.min_ring_via) {
            let have = (plan.diameter - plan.drill) / 2.0;
            if have + 1e-9 < min {
                return Some(t!(
                    "via_tool.refuse_ring",
                    have = format!("{have:.2}"),
                    need = format!("{min:.2}")
                ));
            }
        }
        self.via_crowding(plan, radius)
    }

    /// Refusal when other-net copper sits closer than the clearance rule allows.
    fn via_crowding(&self, plan: &ViaPlan, radius: f64) -> Option<String> {
        let p = &self.project;
        let count = p.copper_layer_count();
        let last = count.saturating_sub(1);
        let slot = |l: Layer| {
            if l == Layer::Back { last } else { l.index() }
        };
        let in_span = |s: u8| s >= plan.span.0 && s <= plan.span.1;
        let at = plan.position;
        let too_close = |other: Id, gap: f64| {
            if other != 0 && other == plan.net {
                return None;
            }
            let need = p.required_clearance(plan.net, other);
            (gap + 1e-9 < need).then_some(need)
        };
        let report = |need: f64| Some(t!("via_tool.refuse_clearance", need = format!("{need:.2}")));
        for t in p.board.tracks.iter().filter(|t| in_span(slot(t.layer))) {
            let gap = canvas::segment_distance(at, t.start, t.end) - t.width / 2.0 - radius;
            if let Some(need) = too_close(t.net, gap) {
                return report(need);
            }
        }
        for v in &p.board.vias {
            let (a, b) = p.via_span(v);
            if b.index() < plan.span.0 || a.index() > plan.span.1 {
                continue;
            }
            let gap = at.distance(v.position) - v.diameter / 2.0 - radius;
            if let Some(need) = too_close(v.net, gap) {
                return report(need);
            }
        }
        for f in &p.board.footprints {
            for pad in &f.pads {
                if !(plan.span.0..=plan.span.1).any(|s| {
                    let layer = if s == last {
                        Layer::Back
                    } else {
                        Layer::from_index(s).unwrap_or(Layer::Front)
                    };
                    f.pad_on_copper(pad, layer)
                }) {
                    continue;
                }
                let ring: Vec<Point> = pad
                    .outline_points()
                    .into_iter()
                    .map(|q| f.position.offset(q.rotate(-f.rotation)))
                    .collect();
                if ring.len() < 3 {
                    continue;
                }
                let gap = if pcbcraft_geom::point_in_polygon(at, &ring) {
                    -radius
                } else {
                    pcbcraft_geom::polygon_boundary_distance(at, &ring) - radius
                };
                if let Some(need) = too_close(pad.net.unwrap_or(0), gap) {
                    return report(need);
                }
            }
        }
        None
    }

    /// Via tool click: put a via at the click, on the net under it, else the net picked in the
    /// options, else no net.
    pub(crate) fn place_via_click(&mut self, pos: Point, zoom: f32) {
        let plan = self.via_plan(pos, zoom);
        if let Some(why) = self.via_refusal(&plan) {
            self.message = why;
            return;
        }
        let (from, to) = plan.span;
        let mut params = json!({
            "position": plan.position,
            "diameter": plan.diameter,
            "drill": plan.drill,
        });
        if plan.net != 0 {
            params["net"] = json!(plan.net);
        }
        let last = self.project.copper_layer_count().saturating_sub(1);
        if (from, to) != (0, last) {
            let name = |i: u8| {
                Layer::from_index(i)
                    .map(|l| self.project.layer_name(l))
                    .unwrap_or_default()
            };
            params["from_layer"] = json!(name(from));
            params["to_layer"] = json!(name(to));
        }
        let before = self.project.board.vias.len();
        self.run("via.place", params);
        if self.project.board.vias.len() > before {
            let (x, y) = (
                format!("{:.1}", plan.position.x),
                format!("{:.1}", plan.position.y),
            );
            self.message = if plan.net == 0 {
                t!("via_tool.placed_no_net", x = x, y = y)
            } else {
                t!(
                    "via_tool.placed",
                    net = self.project.net_name(plan.net).to_string(),
                    x = x,
                    y = y
                )
            };
        }
    }

    /// The ghost via at the cursor: pad and drill at the sizes that would be placed, in the
    /// colours of the layers it spans, red with the reason beside it when it would be refused.
    pub(crate) fn paint_via_ghost(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        cursor: Point,
    ) {
        let plan = self.via_plan(cursor, camera.zoom);
        let refusal = self.via_refusal(&plan);
        let at = camera.screen(plan.position, rect);
        let pad = ((plan.diameter / 2.0) as f32 * camera.zoom).max(3.0);
        let hole = ((plan.drill / 2.0) as f32 * camera.zoom).max(1.0);
        let canvas = self.colours().canvas;
        let last = self.project.copper_layer_count().saturating_sub(1);
        let (top, bottom) = (plan.span.0 == 0, plan.span.1 >= last);
        let red = Color32::from_rgb(230, 70, 70);
        let (fill, ring) = match (&refusal, top, bottom) {
            (Some(_), _, _) => (red.linear_multiply(0.35), red),
            (None, true, true) => (canvas.front.linear_multiply(0.5), canvas.back),
            (None, true, false) => (canvas.front.linear_multiply(0.5), canvas.front),
            (None, false, true) => (canvas.back.linear_multiply(0.5), canvas.back),
            (None, false, false) => (self.muted().linear_multiply(0.5), self.muted()),
        };
        painter.circle_filled(at, pad, fill);
        painter.circle_stroke(at, pad, Stroke::new(1.5, ring));
        painter.circle_stroke(at, hole, Stroke::new(1.0, self.accent()));
        let note = match &refusal {
            Some(why) => why.clone(),
            None => match plan.net {
                0 => t!("via_tool.ghost_no_net"),
                n => self.project.net_name(n).to_string(),
            },
        };
        painter.text(
            at + Vec2::new(pad + 6.0, 0.0),
            Align2::LEFT_CENTER,
            note,
            FontId::monospace(10.0),
            if refusal.is_some() { red } else { self.muted() },
        );
    }

    /// The Via tool options in the right aside.
    pub(crate) fn via_tool_options(&mut self, ui: &mut egui::Ui) {
        if self.tab != Tab::Pcb {
            return;
        }
        let count = self.project.copper_layer_count();
        let last = count.saturating_sub(1);
        ui.horizontal(|ui| {
            ui.label(t!("via_tool.net"));
            let shown = match self.via_tool.net {
                Some(n) => self.project.net_name(n).to_string(),
                None => t!("via_tool.net_auto"),
            };
            egui::ComboBox::from_id_salt("via_tool_net")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.via_tool.net.is_none(), t!("via_tool.net_auto"))
                        .clicked()
                    {
                        self.via_tool.net = None;
                    }
                    for n in self.project.nets.clone() {
                        if ui
                            .selectable_label(self.via_tool.net == Some(n.id), n.name.clone())
                            .clicked()
                        {
                            self.via_tool.net = Some(n.id);
                        }
                    }
                })
                .response
                .named("Via net")
                .on_hover_text(t!("via_tool.net.tip"));
        });
        let allowed: Vec<(u8, u8)> = self
            .project
            .allowed_via_spans()
            .iter()
            .map(|(a, b)| (a.index(), b.index()))
            .collect();
        let span = self.via_option_span();
        let kind = kind_of(span, count);
        ui.horizontal(|ui| {
            ui.label(t!("via_tool.type"));
            egui::ComboBox::from_id_salt("via_tool_type")
                .selected_text(kind_word(kind))
                .show_ui(ui, |ui| {
                    for wanted in [ViaKind::Through, ViaKind::Blind, ViaKind::Buried] {
                        let first = allowed
                            .iter()
                            .copied()
                            .find(|s| kind_of(*s, count) == wanted);
                        let ok = first.is_some();
                        if ui
                            .add_enabled(
                                ok,
                                egui::Button::selectable(kind == wanted, kind_word(wanted)),
                            )
                            .clicked()
                            && let Some(s) = first
                        {
                            self.via_tool.span = (s != (0, last)).then_some(s);
                        }
                    }
                })
                .response
                .named("Via type")
                .on_hover_text(t!("via_tool.type.tip"));
        });
        let name = |i: u8| {
            Layer::from_index(i)
                .map(|l| self.project.layer_name(l))
                .unwrap_or_default()
        };
        let same_kind: Vec<(u8, u8)> = allowed
            .iter()
            .copied()
            .filter(|s| kind_of(*s, count) == kind)
            .collect();
        if same_kind.len() > 1 {
            ui.horizontal(|ui| {
                ui.label(t!("via_tool.span"));
                let mut pick = None;
                egui::ComboBox::from_id_salt("via_tool_span")
                    .selected_text(format!("{} - {}", name(span.0), name(span.1)))
                    .show_ui(ui, |ui| {
                        for s in &same_kind {
                            if ui
                                .selectable_label(
                                    *s == span,
                                    format!("{} - {}", name(s.0), name(s.1)),
                                )
                                .clicked()
                            {
                                pick = Some(*s);
                            }
                        }
                    })
                    .response
                    .named("Via layers")
                    .on_hover_text(t!("via_tool.span.tip"));
                if let Some(s) = pick {
                    self.via_tool.span = (s != (0, last)).then_some(s);
                }
            });
        } else if kind != ViaKind::Through {
            ui.label(format!("{} - {}", name(span.0), name(span.1)))
                .on_hover_text(t!("via_tool.span.tip"));
        }
        let net = self.via_tool.net.unwrap_or(0);
        let (default_diameter, default_drill) = self.via_default_size(net);
        let mut diameter = self.via_tool.diameter.unwrap_or(default_diameter);
        let mut drill = self.via_tool.drill.unwrap_or(default_drill);
        ui.horizontal(|ui| {
            ui.label(t!("via_tool.size"));
            let r = ui
                .add(crate::number_entry::localized_active(
                    egui::DragValue::new(&mut diameter)
                        .speed(0.05)
                        .range(0.1..=10.0)
                        .suffix(format!(" {}", t!("unit.mm"))),
                    3,
                ))
                .on_hover_text(t!("via_tool.size.tip"))
                .named("Via size");
            if r.changed() {
                self.via_tool.diameter = Some(diameter);
                if drill >= diameter {
                    self.via_tool.drill = Some((diameter * 0.5).max(0.05));
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(t!("via_tool.drill"));
            let r = ui
                .add(crate::number_entry::localized_active(
                    egui::DragValue::new(&mut drill)
                        .speed(0.05)
                        .range(0.05..=9.9)
                        .suffix(format!(" {}", t!("unit.mm"))),
                    3,
                ))
                .on_hover_text(t!("via_tool.drill.tip"))
                .named("Via drill");
            if r.changed() {
                self.via_tool.drill = Some(drill);
            }
        });
        if (self.via_tool.diameter.is_some() || self.via_tool.drill.is_some())
            && ui
                .button(t!("via_tool.use_rule_size"))
                .on_hover_text(t!("via_tool.use_rule_size.tip"))
                .clicked()
        {
            self.via_tool.diameter = None;
            self.via_tool.drill = None;
        }
        ui.checkbox(&mut self.via_tool.snap, t!("via_tool.snap"))
            .on_hover_text(t!("via_tool.snap.tip"));
        ui.separator();
    }
}
