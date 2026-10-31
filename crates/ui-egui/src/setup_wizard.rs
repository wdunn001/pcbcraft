//! Guided first-time board setup: size, stack, fab preset, defaults, and starter net classes.
use super::*;
use pcbcraft_doc::FAB_PRESET_NAMES;
use pcbcraft_engine::Command;

type StackLayer = (&'static str, &'static str, f64);

const TWO_LAYERS: &[StackLayer] = &[
    ("Top copper", "copper", 0.035),
    ("Core", "core", 1.53),
    ("Bottom copper", "copper", 0.035),
];

const FOUR_LAYERS: &[StackLayer] = &[
    ("Top copper", "copper", 0.035),
    ("Prepreg", "prepreg", 0.2),
    ("Inner 1", "copper", 0.035),
    ("Core", "core", 0.86),
    ("Inner 2", "copper", 0.035),
    ("Prepreg 2", "prepreg", 0.2),
    ("Bottom copper", "copper", 0.035),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    BoardSize,
    Layers,
    FabPreset,
    Defaults,
    NetClasses,
    Summary,
}

/// Draft values for the setup wizard; lives on the app shell, not in the design file.
pub(super) struct State {
    pub open: bool,
    step: Step,
    width: f64,
    height: f64,
    four_layers: bool,
    preset_index: usize,
    default_clearance: f64,
    default_width: f64,
    power_clearance: f64,
    power_width: f64,
    signal_clearance: f64,
    signal_width: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            step: Step::BoardSize,
            width: 60.0,
            height: 40.0,
            four_layers: true,
            preset_index: 0,
            default_clearance: 0.2,
            default_width: 0.25,
            power_clearance: 0.25,
            power_width: 0.4,
            signal_clearance: 0.2,
            signal_width: 0.25,
        }
    }
}

impl State {
    pub fn open_for_project(project: &Project) -> Self {
        let mut s = Self::default();
        s.width = project.board.width;
        s.height = project.board.height;
        s.open = true;
        s
    }

    fn preset_name(&self) -> Option<&'static str> {
        FAB_PRESET_NAMES.get(self.preset_index).copied()
    }

    fn stack_layers(&self) -> &'static [StackLayer] {
        if self.four_layers {
            FOUR_LAYERS
        } else {
            TWO_LAYERS
        }
    }

    fn board_size_ok(&self) -> Result<(), String> {
        if !self.width.is_finite() || !(5.0..=1000.0).contains(&self.width) {
            return Err("board width must be between 5 and 1000 millimetres".into());
        }
        if !self.height.is_finite() || !(5.0..=1000.0).contains(&self.height) {
            return Err("board height must be between 5 and 1000 millimetres".into());
        }
        Ok(())
    }

    fn mm_default_ok(v: f64, label: &str) -> Result<(), String> {
        if !v.is_finite() || !(0.01..=10.0).contains(&v) {
            return Err(format!("{label} must be between 0.01 and 10 millimetres"));
        }
        Ok(())
    }

    fn validate_step(&self) -> Result<(), String> {
        match self.step {
            Step::BoardSize => self.board_size_ok(),
            Step::Layers => Ok(()),
            Step::FabPreset => {
                if self.preset_name().is_none() {
                    return Err("pick a fabrication preset".into());
                }
                Ok(())
            }
            Step::Defaults => {
                Self::mm_default_ok(self.default_clearance, "default clearance")?;
                Self::mm_default_ok(self.default_width, "default track width")?;
                Ok(())
            }
            Step::NetClasses => {
                Self::mm_default_ok(self.power_clearance, "power clearance")?;
                Self::mm_default_ok(self.power_width, "power track width")?;
                Self::mm_default_ok(self.signal_clearance, "signal clearance")?;
                Self::mm_default_ok(self.signal_width, "signal track width")?;
                Ok(())
            }
            Step::Summary => Ok(()),
        }
    }

    fn finish_commands(&self) -> Result<Vec<Command>, String> {
        self.board_size_ok()?;
        let preset = self
            .preset_name()
            .ok_or_else(|| "pick a fabrication preset".to_string())?;
        Self::mm_default_ok(self.default_clearance, "default clearance")?;
        Self::mm_default_ok(self.default_width, "default track width")?;
        Self::mm_default_ok(self.power_clearance, "power clearance")?;
        Self::mm_default_ok(self.power_width, "power track width")?;
        Self::mm_default_ok(self.signal_clearance, "signal clearance")?;
        Self::mm_default_ok(self.signal_width, "signal track width")?;

        let mut commands = vec![
            Command {
                command: "board.resize".into(),
                params: json!({"width": self.width, "height": self.height}),
            },
            Command {
                command: "board.clear_layer_records".into(),
                params: json!({}),
            },
        ];
        for (name, kind, thickness) in self.stack_layers() {
            commands.push(Command {
                command: "board.set_layer_record".into(),
                params: json!({"name": name, "kind": kind, "thickness": thickness}),
            });
        }
        commands.push(Command {
            command: "rules.preset_apply".into(),
            params: json!({"name": preset}),
        });
        commands.push(Command {
            command: "board.rules.set".into(),
            params: json!({"clearance": self.default_clearance}),
        });
        commands.push(Command {
            command: "netclass.add".into(),
            params: json!({
                "name": "Power",
                "clearance": self.power_clearance,
                "width": self.power_width,
                "nets": [],
            }),
        });
        commands.push(Command {
            command: "netclass.add".into(),
            params: json!({
                "name": "Signal",
                "clearance": self.signal_clearance,
                "width": self.signal_width,
                "nets": [],
            }),
        });
        Ok(commands)
    }

    fn advance(&mut self) -> Result<(), String> {
        self.validate_step()?;
        self.step = match self.step {
            Step::BoardSize => Step::Layers,
            Step::Layers => Step::FabPreset,
            Step::FabPreset => Step::Defaults,
            Step::Defaults => Step::NetClasses,
            Step::NetClasses => Step::Summary,
            Step::Summary => Step::Summary,
        };
        Ok(())
    }

    fn retreat(&mut self) {
        self.step = match self.step {
            Step::BoardSize => Step::BoardSize,
            Step::Layers => Step::BoardSize,
            Step::FabPreset => Step::Layers,
            Step::Defaults => Step::FabPreset,
            Step::NetClasses => Step::Defaults,
            Step::Summary => Step::NetClasses,
        };
    }
}

fn size_field(ui: &mut egui::Ui, label: &str, tip: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::DragValue::new(value)
                .speed(0.5)
                .range(5.0..=1000.0)
                .suffix(" mm"),
        )
        .named(label)
        .on_hover_text(tip);
    });
}

fn mm_field(ui: &mut egui::Ui, label: &str, tip: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::DragValue::new(value)
                .speed(0.01)
                .range(0.01..=10.0)
                .suffix(" mm"),
        )
        .named(label)
        .on_hover_text(tip);
    });
}

impl App {
    pub(super) fn setup_wizard_window(&mut self, ctx: &egui::Context) {
        if !self.setup_wizard.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Design setup")
            .open(&mut open)
            .collapsible(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(
                    "Walk through the first board choices in one place. You can reopen this from Settings.",
                );
                ui.add_space(6.0);
                let w = &mut self.setup_wizard;
                match w.step {
                    Step::BoardSize => {
                        ui.heading("Board size");
                        size_field(
                            ui,
                            "Width",
                            "How wide the board is, from 5 to 1000 millimetres.",
                            &mut w.width,
                        );
                        size_field(
                            ui,
                            "Height",
                            "How tall the board is, from 5 to 1000 millimetres.",
                            &mut w.height,
                        );
                    }
                    Step::Layers => {
                        ui.heading("Copper layers");
                        ui.label("Pick a starter stack. You can tune layers later in Settings.");
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(!w.four_layers, "Two layers")
                                .on_hover_text(
                                    "Top and bottom copper with a core, the usual starter.",
                                )
                                .clicked()
                            {
                                w.four_layers = false;
                            }
                            if ui
                                .selectable_label(w.four_layers, "Four layers")
                                .on_hover_text(
                                    "Two inner planes for ground and cleaner signals.",
                                )
                                .clicked()
                            {
                                w.four_layers = true;
                            }
                        });
                    }
                    Step::FabPreset => {
                        ui.heading("Fabrication preset");
                        ui.label("Apply minimum track, space, and drill rules from a named preset.");
                        let selected = w
                            .preset_name()
                            .map(str::to_owned)
                            .unwrap_or_else(|| "Choose a preset".to_owned());
                        egui::ComboBox::from_id_salt("setup-wizard-preset")
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                for (i, name) in FAB_PRESET_NAMES.iter().enumerate() {
                                    if ui
                                        .selectable_value(&mut w.preset_index, i, *name)
                                        .on_hover_text(
                                            "Named fabrication minimums stored with the design.",
                                        )
                                        .clicked()
                                    {
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .named("Fabrication preset")
                            .on_hover_text(
                                "Pick a preset; its minimums show on the Rules tab after you finish.",
                            );
                    }
                    Step::Defaults => {
                        ui.heading("Default spacing");
                        mm_field(
                            ui,
                            "Clearance",
                            "Default copper gap, from 0.01 to 10 millimetres.",
                            &mut w.default_clearance,
                        );
                        mm_field(
                            ui,
                            "Track width",
                            "Default routed width, from 0.01 to 10 millimetres.",
                            &mut w.default_width,
                        );
                    }
                    Step::NetClasses => {
                        ui.heading("Starter net classes");
                        ui.label("Power and Signal classes you can assign nets to later.");
                        ui.label("Power");
                        mm_field(
                            ui,
                            "Power clearance",
                            "Clearance for the Power class, 0.01 to 10 millimetres.",
                            &mut w.power_clearance,
                        );
                        mm_field(
                            ui,
                            "Power width",
                            "Track width for the Power class, 0.01 to 10 millimetres.",
                            &mut w.power_width,
                        );
                        ui.label("Signal");
                        mm_field(
                            ui,
                            "Signal clearance",
                            "Clearance for the Signal class, 0.01 to 10 millimetres.",
                            &mut w.signal_clearance,
                        );
                        mm_field(
                            ui,
                            "Signal width",
                            "Track width for the Signal class, 0.01 to 10 millimetres.",
                            &mut w.signal_width,
                        );
                    }
                    Step::Summary => {
                        ui.heading("Summary");
                        let layers = if w.four_layers {
                            "Four copper layers"
                        } else {
                            "Two copper layers"
                        };
                        ui.label(format!(
                            "Board {:.1} x {:.1} mm, {layers}.",
                            w.width, w.height
                        ));
                        if let Some(preset) = w.preset_name() {
                            ui.label(format!("Preset: {preset}"));
                        }
                        ui.label(format!(
                            "Default gap {:.3} mm, track {:.3} mm.",
                            w.default_clearance, w.default_width
                        ));
                        ui.label(format!(
                            "Classes Power ({:.3}/{:.3} mm) and Signal ({:.3}/{:.3} mm).",
                            w.power_clearance,
                            w.power_width,
                            w.signal_clearance,
                            w.signal_width
                        ));
                    }
                }
                if let Err(msg) = w.validate_step() {
                    ui.colored_label(egui::Color32::LIGHT_RED, msg);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button("Skip")
                        .on_hover_text("Close the wizard without changing the design.")
                        .clicked()
                    {
                        self.preferences.setup_wizard_seen = true;
                        self.setup_wizard.open = false;
                    }
                    if w.step != Step::BoardSize
                        && ui
                            .button("Back")
                            .on_hover_text("Return to the previous step.")
                            .clicked()
                    {
                        w.retreat();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w.step == Step::Summary {
                            if ui
                                .button("Finish")
                                .on_hover_text("Apply these choices as one undoable step.")
                                .clicked()
                            {
                                match w.finish_commands() {
                                    Ok(commands) => {
                                        self.run_batch(commands);
                                        self.preferences.setup_wizard_seen = true;
                                        self.setup_wizard.open = false;
                                        self.width = w.width;
                                        self.height = w.height;
                                    }
                                    Err(e) => self.message = e,
                                }
                            }
                        } else if ui
                            .button("Next")
                            .on_hover_text("Continue to the next step.")
                            .clicked()
                        {
                            if let Err(e) = w.advance() {
                                self.message = e;
                            }
                        }
                    });
                });
            });
        self.setup_wizard.open = open;
    }

    pub(super) fn open_setup_wizard(&mut self) {
        self.setup_wizard = State::open_for_project(&self.project);
    }

    fn run_batch(&mut self, commands: Vec<Command>) {
        let args = json!({
            "commands": commands,
            "expected_revision": self.revision,
        });
        let result = pcbcraft_mcp::call_tool(&self.session, "batch", &args);
        let success = result.get("isError").and_then(Value::as_bool) != Some(true);
        self.message = if success {
            "Design setup applied".into()
        } else {
            result
                .pointer("/content/0/text")
                .and_then(Value::as_str)
                .map_or_else(|| "Design setup failed".to_string(), refusal_text)
        };
        self.refresh();
    }
}
