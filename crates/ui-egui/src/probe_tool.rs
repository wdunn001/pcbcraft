//! The Probe tool: put a virtual probe on a net or pin and show its DC voltage on the schematic
//! and on the layout.
//!
//! Why: a person wants to see a voltage where the circuit is drawn. The value comes from the
//! engine command `probe.read`, which runs the same solve as `sim.dc`; nothing is invented here.
//!
//! # Rules
//! - The solver runs once per design revision, in `refresh`, never per frame.
//! - On the schematic a click on a pin probes the pin and a click on a wire probes its net.
//! - On the layout a click on a pad probes the component pin of that pad, and a click on a via
//!   or a track probes its net. Anything else says so.
//! - A net probe is drawn on the first visible track, via or pad of its net; a net with no
//!   copper has no badge and stays in the Analyze table.
//! - The cross on a badge removes the probe, as one undo step.
//! - When the solve cannot run, the badge says there is no reading and the tooltip says why.
//!
//! # Dependencies
//! - crates/engine/src/probe.rs (`probe.add`, `probe.remove`, `probe.read`)
//! - crates/ui-egui/src/canvas.rs (`pick_pin`, `pick_pad`, `pick_via`, `pick_track`)
//! - crates/ui-egui/src/wire_canvas.rs (`net_at_schematic_point`, `on_this_page_wires`)
//!
//! # Do not
//! - Run the solver from a paint function.
//! - Touch real hardware; these probes are virtual.
use super::*;
use crate::theme;
use egui::{Rect, Sense, Stroke, Vec2};

/// One probe's last reading.
#[derive(Clone, Debug)]
pub(crate) struct ProbeRow {
    /// The probe's id.
    pub(crate) id: Id,
    /// What the probe is called: its label, or its point when it has none.
    pub(crate) name: String,
    /// The probed net name, or `R1.2` for a pin.
    pub(crate) point: String,
    /// The reference net name, `None` for ground.
    pub(crate) reference: Option<String>,
    /// The voltage in volts, only when the solver produced one.
    pub(crate) volts: Option<f64>,
    /// A plain sentence that says why there is no voltage.
    pub(crate) note: Option<String>,
}

/// What every probe read at one revision.
#[derive(Clone, Debug)]
pub(crate) struct ProbeReading {
    /// The design revision the reading belongs to.
    pub(crate) revision: u64,
    /// The plain reason the solve could not run, when it could not.
    pub(crate) reason: Option<String>,
    /// One row per probe.
    pub(crate) rows: Vec<ProbeRow>,
}

/// A voltage in words, with three decimals and the unit.
pub(crate) fn volts_text(volts: f64) -> String {
    let volts = if volts.abs() < 0.0005 { 0.0 } else { volts };
    t!("probe.volts", volts = format!("{volts:.3}"))
}

impl ProbeRow {
    /// The reading in words, or that there is none.
    pub(crate) fn value_text(&self) -> String {
        match self.volts {
            Some(v) => volts_text(v),
            None => t!("probe.no_reading"),
        }
    }
}

impl App {
    /// Read every probe once for the current revision, when there are probes and no fresh reading.
    ///
    /// # Rules
    /// - Cheap when nothing changed: one comparison.
    pub(crate) fn ensure_probe_reading(&mut self) {
        if self.project.probes.is_empty() {
            self.probe_reading = None;
            return;
        }
        if self
            .probe_reading
            .as_ref()
            .is_some_and(|r| r.revision == self.revision)
        {
            return;
        }
        let out = self
            .session
            .lock()
            .ok()
            .and_then(|s| s.query("probe.read", json!({}), None).ok());
        let Some(result) = out.as_ref().and_then(|o| o.get("result")) else {
            self.probe_reading = None;
            return;
        };
        let reason = result
            .get("reason")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let rows = result
            .get("probes")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        let id = row.get("id").and_then(Value::as_u64)?;
                        let text =
                            |key: &str| row.get(key).and_then(Value::as_str).map(str::to_owned);
                        let point = text("point").unwrap_or_default();
                        Some(ProbeRow {
                            id,
                            name: text("label").unwrap_or_else(|| point.clone()),
                            point,
                            reference: text("reference"),
                            volts: row.get("volts").and_then(Value::as_f64),
                            note: text("note"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.probe_reading = Some(ProbeReading {
            revision: self.revision,
            reason,
            rows,
        });
    }

    /// The first label `P1`, `P2`, ... that no probe uses yet.
    fn next_probe_label(&self) -> String {
        (1..=pcbcraft_doc::MAX_PROBES + 1)
            .map(|n| t!("probe.default_label", n = n))
            .find(|label| {
                !self
                    .project
                    .probes
                    .iter()
                    .any(|p| p.label.as_deref() == Some(label.as_str()))
            })
            .unwrap_or_default()
    }

    /// A click with the Probe tool: on the schematic the pin under the pointer, else the net of
    /// the wire; on the layout the pad's pin, else the net of the via or track.
    pub(crate) fn probe_click(&mut self, pos: Point, zoom: f32) {
        let found = if self.tab == Tab::Pcb {
            self.layout_probe_target(pos, zoom)
        } else if let Some(pin) = self.pick_pin(pos, zoom) {
            Some(json!({"pin": pin}))
        } else {
            self.net_at_schematic_point(pos, zoom)
                .map(|net| json!({"net": net}))
        };
        let Some(mut params) = found else {
            self.message = if self.tab == Tab::Pcb {
                t!("probe.nothing_here.layout")
            } else {
                t!("probe.nothing_here")
            };
            return;
        };
        let label = self.next_probe_label();
        params["label"] = json!(label);
        let result = self
            .session
            .lock()
            .map_err(|_| t!("dialogs.run.session_unavailable"))
            .and_then(|mut s| {
                s.execute("probe.add", params, Some(self.revision))
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(_) => {
                self.message = t!("probe.placed", label = label);
                self.refresh();
            }
            Err(e) => self.message = e,
        }
    }

    /// What a click on the layout probes, as `probe.add` parameters: the component pin behind a
    /// pad (pad numbers are pin numbers), else the net of the via or track under the pointer.
    fn layout_probe_target(&self, pos: Point, zoom: f32) -> Option<Value> {
        if let Some((footprint, number)) = self.pick_pad(pos, zoom) {
            let f = self.project.footprint(footprint)?;
            let on_pin = self
                .project
                .component(f.component)
                .is_some_and(|c| c.pins.iter().any(|p| p.number == number));
            if on_pin {
                return Some(json!({"pin": {"component": f.component, "pin": number}}));
            }
            let net = f.pads.iter().find(|p| p.number == number)?.net?;
            return Some(json!({"net": net}));
        }
        if let Some(id) = self.pick_via(pos, zoom) {
            let net = self.project.board.vias.iter().find(|v| v.id == id)?.net;
            return (net != 0).then(|| json!({"net": net}));
        }
        let id = self.pick_track(pos, zoom)?;
        let net = self.project.board.tracks.iter().find(|t| t.id == id)?.net;
        Some(json!({"net": net}))
    }

    /// Where a probe is drawn on the board: its pad, or the middle of the first visible track of
    /// its net, else the first visible via, else the first visible pad on the net.
    fn probe_anchor_board(&self, probe: &pcbcraft_doc::Probe) -> Option<Point> {
        let board = &self.project.board;
        if let Some(pin) = &probe.pin {
            let f = board
                .footprints
                .iter()
                .find(|f| f.component == pin.component)?;
            return self
                .part_pads_shown(f)
                .then(|| f.pad_position(&pin.pin))
                .flatten();
        }
        let net = probe.net?;
        if let Some(t) = board
            .tracks
            .iter()
            .find(|t| t.net == net && self.pcb_object_shown(t.id))
        {
            return Some(t.arc.unwrap_or_else(|| {
                Point::new((t.start.x + t.end.x) / 2.0, (t.start.y + t.end.y) / 2.0)
            }));
        }
        if let Some(v) = board
            .vias
            .iter()
            .find(|v| v.net == net && self.via_shown(v))
        {
            return Some(v.position);
        }
        board
            .footprints
            .iter()
            .filter(|f| self.part_pads_shown(f))
            .find_map(|f| {
                f.pads
                    .iter()
                    .find(|p| p.net == Some(net))
                    .map(|p| f.pad_world_position(p))
            })
    }

    /// Where a probe is drawn on the tab that is showing.
    fn probe_anchor_shown(&self, probe: &pcbcraft_doc::Probe) -> Option<Point> {
        if self.tab == Tab::Pcb {
            self.probe_anchor_board(probe)
        } else {
            self.probe_anchor(probe)
        }
    }

    /// Where a probe is drawn on the shown schematic page: its pin, or the middle of the
    /// first wire of its net, or the first pin on its net.
    fn probe_anchor(&self, probe: &pcbcraft_doc::Probe) -> Option<Point> {
        let page = self.schematic_page();
        let sheet = self.sheet_view();
        let shown = |c: &pcbcraft_doc::Component| {
            crate::page_scope::in_view(&sheet, page, c.sheet) && !self.sheet_row_hidden(c.id)
        };
        if let Some(pin) = &probe.pin {
            let c = sheet.components.iter().find(|c| c.id == pin.component)?;
            return shown(c).then(|| c.pin_position(&pin.pin)).flatten();
        }
        let net = probe.net?;
        for w in self.on_this_page_wires(page) {
            if w.net == Some(net)
                && let [a, b, ..] = w.points.as_slice()
            {
                return Some(Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0));
            }
        }
        sheet.components.iter().filter(|c| shown(c)).find_map(|c| {
            c.pins
                .iter()
                .find(|p| p.net == Some(net))
                .and_then(|p| c.pin_position(&p.number))
        })
    }

    /// Draw each probe's point and its badge, and handle removing one.
    ///
    /// # Rules
    /// - Reads the cached reading; never solves.
    pub(crate) fn probe_badges(&mut self, ui: &mut egui::Ui, rect: Rect, camera: Camera) {
        if self.project.probes.is_empty() {
            return;
        }
        let tokens = theme::tokens(ui.ctx());
        let painter = ui.painter().with_clip_rect(rect);
        let probes = self.project.probes.clone();
        let mut taken: Vec<egui::Pos2> = Vec::new();
        let mut remove: Option<Id> = None;
        for probe in &probes {
            let Some(at) = self.probe_anchor_shown(probe) else {
                continue;
            };
            let spot = camera.screen(at, rect);
            if !rect.expand(60.0).contains(spot) {
                continue;
            }
            let stacked = taken.iter().filter(|p| p.distance(spot) < 2.0).count();
            taken.push(spot);
            let row = self
                .probe_reading
                .as_ref()
                .and_then(|r| r.rows.iter().find(|x| x.id == probe.id));
            let name = row.map_or_else(
                || probe.label.clone().unwrap_or_default(),
                |r| r.name.clone(),
            );
            let value = row.map_or_else(|| t!("probe.no_reading"), ProbeRow::value_text);
            let text = t!("probe.badge", name = name.clone(), value = value);
            let tip = match row.and_then(|r| r.note.clone()) {
                Some(note) => note,
                None => match self.probe_reading.as_ref().and_then(|r| r.reason.clone()) {
                    Some(reason) => reason,
                    None => t!("probe.badge.tip"),
                },
            };
            painter.circle_filled(spot, 4.0, tokens.copper);
            painter.circle_stroke(spot, 6.0, Stroke::new(1.5, tokens.copper));
            let galley =
                painter.layout_no_wrap(text.clone(), crate::fonts::Role::Mono.font(), tokens.text);
            let close = 16.0;
            let size = Vec2::new(galley.size().x + 14.0 + close, galley.size().y + 6.0);
            let badge = Rect::from_min_size(
                spot + Vec2::new(10.0, -size.y - 6.0 - stacked as f32 * (size.y + 2.0)),
                size,
            );
            painter.rect_filled(badge, 3.0, tokens.raised);
            painter.rect_stroke(
                badge,
                3.0,
                Stroke::new(1.0, tokens.line),
                egui::StrokeKind::Inside,
            );
            painter.galley(badge.min + Vec2::new(5.0, 3.0), galley, tokens.text);
            let body = ui.interact(
                badge,
                ui.id().with(("probe-badge", probe.id)),
                Sense::click(),
            );
            crate::a11y::name_control(&body, &text, None);
            body.on_hover_text(tip);
            let x_rect = Rect::from_min_size(
                egui::pos2(
                    badge.max.x - close - 2.0,
                    badge.min.y + (size.y - close) / 2.0,
                ),
                Vec2::splat(close),
            );
            let x = ui.interact(
                x_rect,
                ui.id().with(("probe-remove", probe.id)),
                Sense::click(),
            );
            crate::a11y::name_control(&x, &t!("probe.remove_named", name = name), None);
            let ink = if x.hovered() {
                tokens.red
            } else {
                tokens.muted
            };
            let c = x_rect.center();
            painter.line_segment(
                [c + Vec2::new(-3.5, -3.5), c + Vec2::new(3.5, 3.5)],
                Stroke::new(1.5, ink),
            );
            painter.line_segment(
                [c + Vec2::new(-3.5, 3.5), c + Vec2::new(3.5, -3.5)],
                Stroke::new(1.5, ink),
            );
            if x.on_hover_text(t!("probe.remove.tip")).clicked() {
                remove = Some(probe.id);
            }
        }
        if let Some(id) = remove {
            self.remove_probe(id);
        }
    }

    /// Remove one probe, as one undo step.
    pub(crate) fn remove_probe(&mut self, id: Id) {
        self.run("probe.remove", json!({"id": id}));
    }
}
