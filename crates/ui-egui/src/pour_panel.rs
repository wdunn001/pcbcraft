//! Pour settings, the plain-words pour report and the keepout drawing tool.
//!
//! Context: the pour engine (`zone.pour`, `zone.fill`, `rules.area_add` with
//! `keepout`) lives in `crates/engine/src/lib.rs`; this module is only the
//! face a person sees. Local rules: every change goes through a typed engine
//! command, the report is read with `Session::peek` (no undo entry), and the
//! hooks in `lib.rs` and `canvas.rs` stay one call each.

use super::*;
use pcbcraft_doc::{ZoneConnect, ZoneIslands};

/// How a report line should look: a plain fact or something to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Note,
    Warning,
}

/// Report sentences with their tone.
pub(crate) type ReportLines = Vec<(String, Tone)>;

/// An editable copy of one zone's pour settings, held until Apply.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PourDraft {
    pub pads: ZoneConnect,
    pub vias: ZoneConnect,
    pub gap: f64,
    pub spoke: f64,
    pub spokes: u8,
    /// True while the zone follows the narrowest track on the board.
    pub follow_min: bool,
    pub min_width: f64,
    pub islands: ZoneIslands,
    pub island_area: f64,
}

impl PourDraft {
    /// Copy the zone's stored settings. The engine bounds are 0.05 to 2 mm
    /// for the gap and 0.05 to 5 mm for spokes and the narrowest copper.
    pub(crate) fn from_zone(zone: &pcbcraft_doc::CopperZone) -> Self {
        let pour = zone.pour_settings();
        Self {
            pads: pour.pad_connect,
            vias: pour.via_connect,
            gap: pour.relief_gap,
            spoke: pour.spoke_width,
            spokes: pour.spokes,
            follow_min: pour.min_width.is_none(),
            min_width: pour.min_width.unwrap_or(0.2),
            islands: pour.islands,
            island_area: match pour.islands {
                ZoneIslands::RemoveBelow(area) => area,
                _ => 1.0,
            },
        }
    }

    /// The `zone.pour` parameters for this draft.
    ///
    /// # Rules
    /// - A zone that follows the narrowest track sends no `min_width`, which
    ///   leaves the stored value alone.
    ///
    /// # Do not
    /// - Clamp here: the engine is the one place that refuses bad numbers.
    pub(crate) fn params(&self, id: Id) -> Value {
        let connect = |c: ZoneConnect| match c {
            ZoneConnect::Solid => "solid",
            ZoneConnect::Relief => "relief",
            ZoneConnect::None => "none",
        };
        let mut params = json!({
            "id": id,
            "pad_connect": connect(self.pads),
            "via_connect": connect(self.vias),
            "relief_gap": self.gap,
            "spoke_width": self.spoke,
            "spokes": self.spokes,
        });
        match self.islands {
            ZoneIslands::Keep => params["islands"] = json!("keep"),
            ZoneIslands::Remove => params["islands"] = json!("remove"),
            ZoneIslands::RemoveBelow(_) => {
                params["islands"] = json!("remove_below");
                params["island_area"] = json!(self.island_area);
            }
        }
        if !self.follow_min {
            params["min_width"] = json!(self.min_width);
        }
        params
    }
}

/// What the Rules tab and the canvas remember about pours and keepouts.
#[derive(Default)]
pub(crate) struct PourState {
    /// The draft for the zone shown, with the stored settings it was copied
    /// from; a different stored value (Apply, undo) replaces the draft.
    pub draft: Option<(Id, PourDraft, PourDraft)>,
    /// The last report, keyed by zone and revision so it refreshes itself.
    pub report: Option<(Id, u64, ReportLines)>,
    /// Rules tab: the area being added is a keepout.
    pub area_keepout: bool,
    /// Limit a drawn keepout to the routing layer.
    pub keepout_layer_only: bool,
    /// Corner where the keepout drag began.
    pub keepout_from: Option<Point>,
}

/// Turn a `zone.fill` result into plain sentences, one per fact.
///
/// # Rules
/// - A starved pad, a detached piece or a zone touching nothing is a
///   `Tone::Warning` with a hint for what to change.
/// - `name_of` turns the id of a starved footprint into words.
///
/// # Dependencies
/// - `crates/engine/src/lib.rs` (`zone.fill` result keys)
///
/// # Do not
/// - Read the project here; the function stays pure so tests can feed it.
pub(crate) fn pour_report_lines(
    fill: &Value,
    name_of: &dyn Fn(Id) -> String,
) -> Vec<(String, Tone)> {
    let mut lines = Vec::new();
    if !fill.is_object() {
        return lines;
    }
    let number = |key: &str| fill.get(key).and_then(Value::as_u64).unwrap_or(0);
    let area = fill.get("area").and_then(Value::as_f64).unwrap_or(0.0);
    lines.push((
        t!(
            "board.inspector.pour_report_area",
            area = format!("{area:.1}"),
            pieces = number("pieces")
        ),
        Tone::Note,
    ));
    let spokes = number("spokes");
    if spokes > 0 {
        lines.push((
            t!("board.inspector.pour_report_spokes", spokes = spokes),
            Tone::Note,
        ));
    } else {
        lines.push((t!("board.inspector.pour_report_no_spokes"), Tone::Note));
    }
    if let Some(starved) = fill.get("starved").and_then(Value::as_array) {
        for id in starved.iter().filter_map(Value::as_u64) {
            lines.push((
                t!("board.inspector.pour_report_starved", name = name_of(id)),
                Tone::Warning,
            ));
        }
    }
    let islands = number("islands");
    if islands > 0 {
        lines.push((
            t!(
                "board.inspector.pour_report_islands",
                islands = islands,
                removed = number("removed")
            ),
            Tone::Warning,
        ));
    }
    if fill.get("unconnected").and_then(Value::as_bool) == Some(true) {
        lines.push((t!("board.inspector.pour_report_unconnected"), Tone::Warning));
    }
    lines
}

/// The rectangle of a keepout drag as `rules.area_add` parameters, or `None`
/// when the drag is too small to be a rectangle.
///
/// # Do not
/// - Validate range here; the engine refuses rectangles it cannot take.
pub(crate) fn keepout_params(a: Point, b: Point, layer: Option<Value>) -> Option<Value> {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    let (width, height) = ((a.x - b.x).abs(), (a.y - b.y).abs());
    if width < 0.05 || height < 0.05 {
        return None;
    }
    let mut params = json!({
        "keepout": true,
        "clearance": 0.1,
        "x": x,
        "y": y,
        "width": width,
        "height": height,
    });
    if let Some(layer) = layer {
        params["layer"] = layer;
    }
    Some(params)
}

impl App {
    /// The "Pour settings" panel and the report under it, for a selected zone.
    ///
    /// # Rules
    /// - Settings are copied from the zone and re-copied whenever the project
    ///   revision changes (Apply, undo), so the panel never shows stale numbers.
    /// - One Apply sends one `zone.pour`; the engine's refusal shows in the
    ///   status line and leaves the zone unchanged.
    ///
    /// # Dependencies
    /// - `crates/engine/src/lib.rs` (`zone.pour`, `zone.fill`)
    ///
    /// # Do not
    /// - Send a command per field: Apply is the single undo step.
    pub(crate) fn pour_panel(&mut self, ui: &mut egui::Ui, id: Id) {
        let Some(stored) = self
            .project
            .board
            .zones
            .iter()
            .find(|z| z.id == id)
            .map(PourDraft::from_zone)
        else {
            return;
        };
        let stale = !matches!(&self.pours.draft, Some((d, base, _)) if *d == id && *base == stored);
        if stale {
            self.pours.draft = Some((id, stored.clone(), stored));
        }
        let Some((_, _, mut draft)) = self.pours.draft.clone() else {
            return;
        };
        let stored_follows = self
            .project
            .board
            .zones
            .iter()
            .find(|z| z.id == id)
            .is_some_and(|z| z.pour_settings().min_width.is_none());
        let before = draft.clone();
        let mm = format!(" {}", t!("unit.mm"));
        let muted = self.muted();
        ui.add_space(10.0);
        ui.label(t!("board.inspector.pour_settings"));
        ui.add_space(4.0);
        let connect_row =
            |ui: &mut egui::Ui, label: String, tip: String, value: &mut ZoneConnect| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(label.clone()).on_hover_text(tip.clone());
                    for (choice, word) in [
                        (ZoneConnect::Solid, t!("board.inspector.pour_solid")),
                        (ZoneConnect::Relief, t!("board.inspector.pour_relief")),
                        (ZoneConnect::None, t!("board.inspector.pour_none")),
                    ] {
                        let response = ui
                            .selectable_value(value, choice, word.clone())
                            .on_hover_text(tip.clone());
                        a11y::name_control(
                            &response,
                            &format!("{label}: {word}"),
                            Some(*value == choice),
                        );
                    }
                });
            };
        connect_row(
            ui,
            t!("board.inspector.pour_pads"),
            t!("board.inspector.pour_pads.tip"),
            &mut draft.pads,
        );
        connect_row(
            ui,
            t!("board.inspector.pour_vias"),
            t!("board.inspector.pour_vias.tip"),
            &mut draft.vias,
        );
        let number_row = |ui: &mut egui::Ui,
                          label: String,
                          tip: String,
                          value: &mut f64,
                          range: std::ops::RangeInclusive<f64>| {
            ui.horizontal(|ui| {
                ui.label(label.clone()).on_hover_text(tip.clone());
                let response = ui
                    .add(
                        egui::DragValue::new(value)
                            .speed(0.05)
                            .range(range)
                            .suffix(mm.clone()),
                    )
                    .on_hover_text(tip);
                a11y::name_control(&response, &label, None);
            });
        };
        number_row(
            ui,
            t!("board.inspector.pour_gap"),
            t!("board.inspector.pour_gap.tip"),
            &mut draft.gap,
            0.05..=2.0,
        );
        number_row(
            ui,
            t!("board.inspector.pour_spoke"),
            t!("board.inspector.pour_spoke.tip"),
            &mut draft.spoke,
            0.05..=5.0,
        );
        ui.horizontal(|ui| {
            let tip = t!("board.inspector.pour_spokes.tip");
            ui.label(t!("board.inspector.pour_spokes"))
                .on_hover_text(tip.clone());
            for count in [2_u8, 4] {
                let response = ui
                    .selectable_value(&mut draft.spokes, count, count.to_string())
                    .on_hover_text(tip.clone());
                a11y::name_control(
                    &response,
                    &t!("board.inspector.pour_spokes_count", count = count),
                    Some(draft.spokes == count),
                );
            }
        });
        if stored_follows {
            let response = ui
                .checkbox(&mut draft.follow_min, t!("board.inspector.pour_follow"))
                .on_hover_text(t!("board.inspector.pour_follow.tip"));
            a11y::name_control(
                &response,
                &t!("board.inspector.pour_follow"),
                Some(draft.follow_min),
            );
        } else {
            draft.follow_min = false;
        }
        if !draft.follow_min {
            number_row(
                ui,
                t!("board.inspector.pour_min_width"),
                t!("board.inspector.pour_min_width.tip"),
                &mut draft.min_width,
                0.05..=5.0,
            );
        }
        ui.horizontal_wrapped(|ui| {
            let tip = t!("board.inspector.pour_islands.tip");
            ui.label(t!("board.inspector.pour_islands"))
                .on_hover_text(tip.clone());
            let area = draft.island_area;
            for (choice, word) in [
                (ZoneIslands::Keep, t!("board.inspector.pour_keep")),
                (ZoneIslands::Remove, t!("board.inspector.pour_remove")),
                (
                    ZoneIslands::RemoveBelow(area),
                    t!("board.inspector.pour_remove_small"),
                ),
            ] {
                let on = std::mem::discriminant(&draft.islands) == std::mem::discriminant(&choice);
                let response = ui
                    .selectable_label(on, word.clone())
                    .on_hover_text(tip.clone());
                a11y::name_control(&response, &word, Some(on));
                if response.clicked() {
                    draft.islands = choice;
                }
            }
        });
        if matches!(draft.islands, ZoneIslands::RemoveBelow(_)) {
            ui.horizontal(|ui| {
                let tip = t!("board.inspector.pour_island_area.tip");
                ui.label(t!("board.inspector.pour_island_area"))
                    .on_hover_text(tip.clone());
                let response = ui
                    .add(
                        egui::DragValue::new(&mut draft.island_area)
                            .speed(0.5)
                            .range(0.0..=10000.0)
                            .suffix(" mm\u{b2}".to_string()),
                    )
                    .on_hover_text(tip);
                a11y::name_control(&response, &t!("board.inspector.pour_island_area"), None);
                draft.islands = ZoneIslands::RemoveBelow(draft.island_area);
            });
        }
        if draft != before
            && let Some(slot) = self.pours.draft.as_mut()
        {
            slot.2 = draft.clone();
        }
        ui.add_space(6.0);
        let apply = ui
            .button(t!("board.inspector.pour_apply"))
            .on_hover_text(t!("board.inspector.pour_apply.tip"));
        a11y::name_control(&apply, &t!("board.inspector.pour_apply"), None);
        if apply.clicked() {
            self.run("zone.pour", draft.params(id));
        }
        self.pour_report(ui, id, muted);
    }

    /// Read `zone.fill` when the zone or revision changed and print it.
    fn pour_report(&mut self, ui: &mut egui::Ui, id: Id, muted: egui::Color32) {
        let revision = self.revision;
        let fresh = matches!(&self.pours.report, Some((z, r, _)) if *z == id && *r == revision);
        if !fresh {
            // `peek` runs on a scratch copy: no revision bump, no undo entry.
            let fill = self
                .session
                .lock()
                .ok()
                .and_then(|s| s.peek("zone.fill", json!({"id": id})).ok())
                .unwrap_or(Value::Null);
            let project = &self.project;
            let name_of = |target: Id| {
                project
                    .board
                    .footprints
                    .iter()
                    .find(|f| f.id == target)
                    .and_then(|f| project.component(f.component))
                    .map_or_else(|| format!("#{target}"), |c| c.reference.clone())
            };
            let lines = pour_report_lines(&fill, &name_of);
            self.pours.report = Some((id, revision, lines));
        }
        let Some((_, _, lines)) = self.pours.report.clone() else {
            return;
        };
        ui.add_space(6.0);
        for (text, tone) in lines {
            let colour = if tone == Tone::Warning {
                ui.visuals().warn_fg_color
            } else {
                muted
            };
            ui.label(crate::fonts::caption(text).color(colour));
        }
    }

    /// Rules tab: the "Keep all copper out" checkbox and its layer limit.
    /// Returns true when the stricter-gap fields should be hidden.
    ///
    /// # Do not
    /// - Add the area here; the shared Add area button sends it.
    pub(crate) fn keepout_checkbox(&mut self, ui: &mut egui::Ui) -> bool {
        ui.horizontal_wrapped(|ui| {
            let response = ui
                .checkbox(&mut self.pours.area_keepout, t!("rules.rules_tab.keepout"))
                .on_hover_text(t!("rules.rules_tab.keepout.tip"));
            a11y::name_control(
                &response,
                &t!("rules.rules_tab.keepout"),
                Some(self.pours.area_keepout),
            );
            if self.pours.area_keepout {
                let layer = ui
                    .checkbox(
                        &mut self.pours.keepout_layer_only,
                        t!("rules.rules_tab.keepout_layer_only"),
                    )
                    .on_hover_text(t!("rules.rules_tab.keepout_layer_only.tip"));
                a11y::name_control(
                    &layer,
                    &t!("rules.rules_tab.keepout_layer_only"),
                    Some(self.pours.keepout_layer_only),
                );
            }
            let draw = ui
                .button(t!("rules.rules_tab.draw_keepout"))
                .on_hover_text(t!("rules.rules_tab.draw_keepout.tip"));
            a11y::name_control(&draw, &t!("rules.rules_tab.draw_keepout"), None);
            if draw.clicked() {
                // Going to the board resets the tool, so arm it afterwards.
                self.change_tab(Tab::Pcb);
                self.tool = Tool::Keepout;
                self.pours.keepout_from = None;
            }
        });
        self.pours.area_keepout
    }

    /// Tool bar, while the Keepout tool is active: the two keepout toggles.
    ///
    /// # Rules
    /// - Drawn right to left inside the tool bar's options group, so "This layer only"
    ///   (shown only when the keepout is on) ends up right of "Keep all copper out".
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/checkbox.rs
    ///
    /// # Do not
    /// - Add a second copy of these flags; they live in `PourState`.
    pub(crate) fn keepout_options(&mut self, ui: &mut egui::Ui, t: &crate::theme::Tokens) {
        use crate::ui_kit::checkbox::checkbox;
        let layer_text = t!("rules.rules_tab.keepout_layer_only");
        let keep_text = t!("rules.rules_tab.keepout");
        if self.pours.area_keepout {
            ui.label(
                crate::fonts::text(crate::fonts::Role::Body, layer_text.clone()).color(t.text),
            );
            checkbox(
                ui,
                &mut self.pours.keepout_layer_only,
                &layer_text,
                &t!("rules.rules_tab.keepout_layer_only.tip"),
            );
        }
        ui.label(crate::fonts::text(crate::fonts::Role::Body, keep_text.clone()).color(t.text));
        checkbox(
            ui,
            &mut self.pours.area_keepout,
            &keep_text,
            &t!("rules.rules_tab.keepout.tip"),
        );
    }

    /// Canvas: press-drag-release with the Keepout tool draws one rectangle,
    /// then the tool returns to Select.
    ///
    /// # Rules
    /// - Corners are snapped; a drag smaller than the engine minimum is
    ///   ignored and the tool stays armed.
    ///
    /// # Do not
    /// - Draw or run anything for any other tool.
    pub(crate) fn keepout_drag(&mut self, response: &egui::Response, cursor: Option<Point>) {
        if !matches!(self.tool, Tool::Keepout) {
            self.pours.keepout_from = None;
            return;
        }
        let Some(pos) = cursor else {
            return;
        };
        if response.drag_started_by(egui::PointerButton::Primary) {
            self.pours.keepout_from = Some(crate::canvas::snap(pos));
        }
        if response.drag_stopped_by(egui::PointerButton::Primary)
            && let Some(from) = self.pours.keepout_from.take()
        {
            let layer = self
                .pours
                .keepout_layer_only
                .then(|| json!(self.route_layer));
            if let Some(params) = keepout_params(from, crate::canvas::snap(pos), layer) {
                self.run("rules.area_add", params);
                self.tool = Tool::Select;
            }
        }
    }

    /// Canvas: the ghost rectangle while a keepout is being dragged.
    pub(crate) fn paint_keepout_ghost(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        cursor: Point,
    ) {
        if let Some(from) = self.pours.keepout_from {
            let ghost = Rect::from_two_pos(camera.screen(from, rect), camera.screen(cursor, rect));
            painter.rect_stroke(
                ghost,
                0.0,
                Stroke::new(1.5, self.accent()),
                egui::StrokeKind::Inside,
            );
        }
    }
}
