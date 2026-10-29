//! Properties section for a selected track: the width that would meet a
//! target impedance, and the Tune length action with its before and after.
use super::*;

/// What the last Tune length run reported, kept for display under the button.
#[derive(Clone, Debug, Default)]
pub(crate) struct TuneReport {
    pub before: f64,
    pub after: f64,
    pub target: f64,
    pub delay_before: Option<f64>,
    pub delay_after: Option<f64>,
    pub warnings: Vec<String>,
    pub note: String,
}

/// Fields the section needs; held on the app as one value.
#[derive(Clone, Debug)]
pub(crate) struct ImpedanceUi {
    pub target_ohm: f64,
    pub tune_target: f64,
    pub tune_tolerance: f64,
    pub tune_rounded: bool,
    pub report: Option<TuneReport>,
}

impl Default for ImpedanceUi {
    fn default() -> Self {
        Self {
            target_ohm: 50.0,
            tune_target: 0.0,
            tune_tolerance: 0.1,
            tune_rounded: false,
            report: None,
        }
    }
}

impl App {
    /// Read-only engine call that leaves the status line alone.
    fn quiet_report(&self, command: &str, params: Value) -> Option<Value> {
        let s = self.session.lock().ok()?;
        s.peek(command, params).ok()
    }

    pub(crate) fn impedance_section(&mut self, ui: &mut egui::Ui, track: Id, net: Id) {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("IMPEDANCE")
                .size(10.0)
                .color(self.muted()),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Target");
            ui.add(
                egui::DragValue::new(&mut self.impedance_ui.target_ohm)
                    .range(1.0..=1000.0)
                    .speed(0.5)
                    .suffix(" ohm"),
            )
            .on_hover_text(
                "The impedance this connection should have, for example 50 ohm for a radio line. The suggestion below is worked out for it.",
            );
        });
        let target = self.impedance_ui.target_ohm;
        let row = self
            .quiet_report("net.impedance", json!({"net": net, "target": target}))
            .and_then(|v| v.get("rows").and_then(|r| r.get(0)).cloned());
        match row {
            Some(r) if r.get("impedance_ohm").is_some() => {
                let num = |k: &str| r.get(k).and_then(Value::as_f64);
                let width = num("width").unwrap_or(0.0);
                let ohms = num("impedance_ohm").unwrap_or(0.0);
                ui.label(format!("Now {ohms:.1} ohm at {width:.3} mm wide"))
                    .on_hover_text(
                        "Estimated from this connection's widest wire, the board thickness and dielectric constant you recorded, and the copper thickness.",
                    );
                match num("recommended_width") {
                    Some(w) => {
                        ui.label(
                            egui::RichText::new(format!("Suggested width {w:.3} mm")).strong(),
                        )
                        .on_hover_text(
                            "The wire width that reaches the target on this board. Fabs usually hold plus or minus 10 percent, so allow that band.",
                        );
                        let (lo, hi) = (target * 0.9, target * 1.1);
                        ui.label(
                            egui::RichText::new(format!(
                                "Fab band {lo:.0} to {hi:.0} ohm (plus or minus 10 percent)"
                            ))
                            .small()
                            .color(self.muted()),
                        );
                        if ui
                            .button("Use suggested width")
                            .on_hover_text(
                                "Set the width for new wires to the suggestion. Undo does not apply; it only changes the width box.",
                            )
                            .clicked()
                        {
                            self.track_width = w;
                        }
                    }
                    None => {
                        ui.label(
                            r.get("advice")
                                .and_then(Value::as_str)
                                .unwrap_or("No width reaches that target on this board."),
                        );
                    }
                }
                let inputs = format!(
                    "Inputs: dielectric {:.2} mm, er {:.2}, copper {:.3} mm ({}), effective er {:.2}",
                    self.project.board.thickness,
                    self.project.board.er,
                    num("copper_mm").unwrap_or(0.0),
                    r.get("copper_source")
                        .and_then(Value::as_str)
                        .unwrap_or("default"),
                    num("eeff").unwrap_or(0.0),
                );
                ui.label(egui::RichText::new(inputs).small().color(self.muted()))
                    .on_hover_text(
                        "Every number the estimate used. Change the board thickness or dielectric constant in the stackup to change the answer.",
                    );
                if let Some(note) = r.get("validity").and_then(Value::as_str) {
                    ui.label(egui::RichText::new(note).small().color(self.muted()));
                }
            }
            _ => {
                ui.label("No estimate yet; this connection has no routed wire.");
            }
        }
        self.tune_section(ui, track, net);
    }

    fn net_length(&self, net: Id) -> f64 {
        self.project
            .board
            .tracks
            .iter()
            .filter(|t| t.net == net)
            .map(track_length)
            .sum()
    }

    fn tune_section(&mut self, ui: &mut egui::Ui, track: Id, net: Id) {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("TUNE LENGTH")
                .size(10.0)
                .color(self.muted()),
        );
        ui.add_space(4.0);
        let now = self.net_length(net);
        if self.impedance_ui.tune_target <= 0.0 {
            self.impedance_ui.tune_target = (now + 5.0).clamp(0.1, 10000.0);
        }
        ui.label(format!("Connection is {now:.2} mm long"))
            .on_hover_text(
                "Total of all copper wires on this connection. Via barrels are not counted.",
            );
        ui.horizontal(|ui| {
            ui.label("Target");
            ui.add(
                egui::DragValue::new(&mut self.impedance_ui.tune_target)
                    .range(0.1..=10000.0)
                    .speed(0.1)
                    .suffix(" mm"),
            )
            .on_hover_text(
                "How long the whole connection should end up, so it matches its partners.",
            );
        });
        ui.horizontal(|ui| {
            ui.label("Allowed miss");
            ui.add(
                egui::DragValue::new(&mut self.impedance_ui.tune_tolerance)
                    .range(0.01..=10.0)
                    .speed(0.01)
                    .suffix(" mm"),
            )
            .on_hover_text("How far from the target the result may land.");
        });
        ui.checkbox(&mut self.impedance_ui.tune_rounded, "Rounded bends")
            .on_hover_text(
                "Round the corners of the zigzag instead of cutting them at 45 degrees. Legs are kept at least 3 times the board thickness apart.",
            );
        if ui
            .button("Tune length")
            .on_hover_text(
                "Replace this straight wire with a zigzag so the whole connection reaches the target length. Nothing changes if there is no room. Undo puts the straight wire back in one step.",
            )
            .clicked()
        {
            let params = json!({
                "track": track,
                "target": self.impedance_ui.tune_target,
                "tolerance": self.impedance_ui.tune_tolerance,
                "style": if self.impedance_ui.tune_rounded { "rounded" } else { "trombone" },
            });
            let outcome = self
                .session
                .lock()
                .map_err(|_| "Session unavailable".to_string())
                .and_then(|mut s| {
                    s.execute("track.tune_length", params, Some(self.revision))
                        .map_err(|e| e.to_string())
                });
            match outcome {
                Ok(v) => {
                    let r = v.pointer("/result").cloned().unwrap_or(Value::Null);
                    let f = |k: &str| r.get(k).and_then(Value::as_f64);
                    self.impedance_ui.report = Some(TuneReport {
                        before: f("before").unwrap_or(0.0),
                        after: f("after").unwrap_or(0.0),
                        target: f("target").unwrap_or(0.0),
                        delay_before: f("delay_ps_before"),
                        delay_after: f("delay_ps_after"),
                        warnings: r
                            .get("warnings")
                            .and_then(Value::as_array)
                            .map(|a| {
                                a.iter().filter_map(Value::as_str).map(String::from).collect()
                            })
                            .unwrap_or_default(),
                        note: "Via barrels are not counted in the length.".into(),
                    });
                    self.message = "track.tune_length · committed".into();
                    if let Some(first) = r
                        .get("ids")
                        .and_then(|i| i.get(0))
                        .and_then(Value::as_u64)
                    {
                        self.selected = Some(first);
                    }
                }
                Err(e) => {
                    self.impedance_ui.report = None;
                    self.message = e;
                }
            }
            self.refresh();
        }
        if let Some(rep) = self.impedance_ui.report.clone() {
            ui.add_space(4.0);
            ui.label(format!(
                "Before {:.2} mm, after {:.2} mm (target {:.2} mm)",
                rep.before, rep.after, rep.target
            ));
            if let (Some(a), Some(b)) = (rep.delay_before, rep.delay_after) {
                ui.label(
                    egui::RichText::new(format!("Delay {a:.1} ps to {b:.1} ps"))
                        .small()
                        .color(self.muted()),
                );
            }
            for w in &rep.warnings {
                ui.label(egui::RichText::new(w).small().color(self.muted()));
            }
            ui.label(egui::RichText::new(&rep.note).small().color(self.muted()));
        }
    }
}
