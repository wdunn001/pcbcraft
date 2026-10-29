//! The "Generate footprint" form of the library editor: pick a package
//! family, type the datasheet numbers, see the pads, and load the result
//! into the footprint editor like any hand-made footprint.
use super::*;
use pcbcraft_library::{FootprintDefinition, LandPatternDensity, LandPatternParams};

const FAMILIES: [(&str, &str); 11] = [
    ("chip_2terminal", "Chip (two pins)"),
    ("sot", "SOT"),
    ("soic", "SOIC"),
    ("qfp", "QFP"),
    ("qfn", "QFN"),
    ("bga", "BGA"),
    ("dip", "DIP"),
    ("header_single", "Header, one row"),
    ("header_double", "Header, two rows"),
    ("radial", "Radial leads"),
    ("axial", "Axial leads"),
];

/// What the person has typed so far.
pub(super) struct Wizard {
    pub open: bool,
    pub family: &'static str,
    pub density: char,
    pub name: String,
    pub pitch: f64,
    pub pin_count: u32,
    pub body_length: f64,
    pub body_width: f64,
    pub row_spacing: f64,
    pub lead_diameter: f64,
    pub grid_rows: u32,
    pub grid_cols: u32,
    pub thermal_size: f64,
}

impl Default for Wizard {
    fn default() -> Self {
        Self {
            open: false,
            family: "qfn",
            density: 'B',
            name: "QFN_generated".into(),
            pitch: 0.5,
            pin_count: 32,
            body_length: 5.0,
            body_width: 5.0,
            row_spacing: 0.0,
            lead_diameter: 0.0,
            grid_rows: 8,
            grid_cols: 8,
            thermal_size: 0.0,
        }
    }
}

impl Wizard {
    /// Build the footprint these numbers describe, or say in plain words
    /// what is wrong with them.
    pub fn build(&self) -> Result<FootprintDefinition, String> {
        let density = LandPatternDensity::parse(&self.density.to_string())
            .ok_or_else(|| "pick a density".to_string())?;
        if !self.pitch.is_finite() || !(0.1..=25.4).contains(&self.pitch) {
            return Err("pitch must be between 0.1 and 25.4 millimetres".into());
        }
        let params = LandPatternParams {
            name: self.name.trim().to_string(),
            pitch: self.pitch,
            pin_count: self.pin_count,
            body_length: self.body_length,
            body_width: self.body_width,
            row_spacing: self.row_spacing,
            lead_diameter: self.lead_diameter,
            grid_rows: self.grid_rows,
            grid_cols: self.grid_cols,
            thermal_size: self.thermal_size,
            ..Default::default()
        };
        pcbcraft_library::generate_footprint(self.family, density, &params)
    }
}

fn field(ui: &mut egui::Ui, label: &str, tip: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(0.05).suffix(" mm"))
            .on_hover_text(tip);
    });
}

fn count(ui: &mut egui::Ui, label: &str, tip: &str, value: &mut u32, max: u32) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).range(1..=max))
            .on_hover_text(tip);
    });
}

impl App {
    /// The form, drawn inside the library editor tab while it is open.
    pub(super) fn footprint_wizard(&mut self, ui: &mut egui::Ui) {
        if !self.library.editor.wizard.open {
            return;
        }
        let mut use_it = false;
        let mut close = false;
        let built = self.library.editor.wizard.build();
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                let w = &mut self.library.editor.wizard;
                ui.heading("Generate footprint");
                ui.label("Type the numbers from the package drawing in the datasheet. Dimensions are millimetres.");
                ui.horizontal(|ui| {
                    ui.label("Package");
                    egui::ComboBox::from_id_salt("wizard-family")
                        .selected_text(
                            FAMILIES
                                .iter()
                                .find(|f| f.0 == w.family)
                                .map(|f| f.1)
                                .unwrap_or("Package"),
                        )
                        .show_ui(ui, |ui| {
                            for (id, label) in FAMILIES {
                                ui.selectable_value(&mut w.family, id, label)
                                    .on_hover_text("Package family to build the pads for.");
                            }
                        })
                        .response
                        .on_hover_text("Package family to build the pads for.");
                });
                ui.horizontal(|ui| {
                    ui.label("Density");
                    for (d, label, tip) in [
                        ('A', "Most", "Largest pads, easiest to hand solder."),
                        ('B', "Nominal", "The usual choice."),
                        ('C', "Least", "Smallest pads, for crowded boards."),
                    ] {
                        ui.selectable_value(&mut w.density, d, label)
                            .on_hover_text(tip);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(crate::targets::text_field(&mut w.name))
                        .on_hover_text("Name the footprint will have in the project library.");
                });
                field(ui, "Pitch", "Distance from one pin centre to the next.", &mut w.pitch);
                if w.family == "bga" {
                    count(ui, "Rows", "Balls from top to bottom.", &mut w.grid_rows, 40);
                    count(ui, "Columns", "Balls from left to right.", &mut w.grid_cols, 40);
                    field(ui, "Ball size", "Ball diameter; leave 0 for the default.", &mut w.lead_diameter);
                } else if !matches!(w.family, "chip_2terminal" | "radial" | "axial") {
                    count(ui, "Pins", "Total number of pins.", &mut w.pin_count, 256);
                }
                if w.family != "header_single" {
                    field(ui, "Body length", "Package body, left to right.", &mut w.body_length);
                    field(ui, "Body width", "Package body, top to bottom.", &mut w.body_width);
                }
                if matches!(w.family, "dip" | "header_double") {
                    field(ui, "Row spacing", "Distance between the two pin rows; 0 picks the standard.", &mut w.row_spacing);
                }
                if matches!(w.family, "dip" | "header_single" | "header_double" | "radial" | "axial") {
                    field(ui, "Lead size", "Lead diameter; 0 picks a default.", &mut w.lead_diameter);
                }
                if w.family == "qfn" {
                    field(ui, "Thermal pad", "Centre pad size; 0 sizes it from the body.", &mut w.thermal_size);
                }
                match &built {
                    Ok(def) => {
                        let thermal = def.pads.iter().any(|p| p.number == "EP");
                        ui.label(format!(
                            "{} pads{}",
                            def.pads.len(),
                            if thermal { ", with a thermal pad" } else { "" }
                        ));
                    }
                    Err(e) => {
                        ui.colored_label(Color32::LIGHT_RED, e);
                    }
                }
                if let Ok(def) = &built {
                    self.library_preview(ui, None, Some(def));
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(built.is_ok(), egui::Button::new("Create footprint"))
                        .on_hover_text("Load these pads into the footprint editor, ready to adjust and save.")
                        .clicked()
                    {
                        use_it = true;
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_text("Close this form without making anything.")
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        if use_it && let Ok(def) = built {
            self.library.editor.draft = Some(super::authoring::Draft::Footprint(def));
            self.library.editor.notice = None;
            self.library.editor.wizard.open = false;
        }
        if close {
            self.library.editor.wizard.open = false;
        }
    }
}
