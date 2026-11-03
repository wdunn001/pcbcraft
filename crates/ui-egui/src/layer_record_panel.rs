//! Properties for a selected layer, sublayer or manufacturing artifact: the
//! physical layer record (side, material, thickness, Dk, Df, tolerance, copper
//! weight, foil) and the facts of a drill artifact.
//!
//! Context: `layers_tree.rs` `tree_row_properties` calls these when the
//! one selection value (`selection.rs`) names a layer, a sublayer or an artifact.
//! Local rules: every value has a tooltip from the text catalog; a value the
//! record does not hold reads "Not set", never a guess.
//!
//! # Dependencies
//! - crates/ui-egui/src/selection.rs (the value and `DrillKey`)
//! - crates/ui-egui/src/ui_kit/form_row.rs (the row layout)
//! - crates/io/src/drill_table.rs (`drill_table` rows of an artifact)
//!
//! # Do not
//! - Edit the record here; editing stays in `stack_properties`.
use super::*;
use crate::ui_kit::form_row::{FormWidth, form_row};
use pcbcraft_doc::LayerRecord;

fn fact(ui: &mut egui::Ui, label: String, value: String, tip: String) {
    form_row(ui, FormWidth::Aside, &label, None, None, |ui| {
        ui.label(fonts::text(fonts::Role::Mono, &value))
            .on_hover_text(tip);
    });
}

fn number(v: Option<f64>, digits: usize) -> String {
    v.map_or_else(|| t!("layer.record.not_set"), |x| format!("{x:.digits$}"))
}

impl App {
    /// The physical record of one stackup row as read-only facts.
    ///
    /// # Rules
    /// - Shows side, kind, Dk, Df, thickness tolerance, copper weight and foil.
    /// - A value the row does not carry reads "Not set".
    ///
    /// # Dependencies
    /// - crates/doc/src/lib.rs (`LayerRecord`)
    ///
    /// # Do not
    /// - Fill a missing value from the board defaults here.
    pub(crate) fn layer_record_rows(&self, ui: &mut egui::Ui, rec: &LayerRecord) {
        let unset = || t!("layer.record.not_set");
        ui.add_space(8.0);
        ui.label(fonts::title(t!("layer.record.title")));
        fact(
            ui,
            t!("layer.record.side"),
            rec.side.clone().unwrap_or_else(unset),
            t!("layer.record.side.tip"),
        );
        fact(
            ui,
            t!("layer.record.kind"),
            rec.kind.clone(),
            t!("layer.record.kind.tip"),
        );
        fact(
            ui,
            t!("layer.record.dk"),
            number(rec.dk, 2),
            t!("layer.record.dk.tip"),
        );
        fact(
            ui,
            t!("layer.record.df"),
            number(rec.df, 4),
            t!("layer.record.df.tip"),
        );
        fact(
            ui,
            t!("layer.record.tolerance"),
            rec.thickness_tol_pct
                .map_or_else(unset, |x| format!("{x:.1} %")),
            t!("layer.record.tolerance.tip"),
        );
        fact(
            ui,
            t!("layer.record.copper_weight"),
            rec.copper_weight_oz
                .map_or_else(unset, |x| format!("{x:.2} oz")),
            t!("layer.record.copper_weight.tip"),
        );
        fact(
            ui,
            t!("layer.record.foil"),
            rec.foil.clone().unwrap_or_else(unset),
            t!("layer.record.foil.tip"),
        );
    }

    /// Properties of a sublayer row: its name, then the record of its layer.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/layers_tree.rs (`stack_properties` draws the layer record)
    ///
    /// # Do not
    /// - Edit anything here; the layer's own fields are edited on the layer.
    pub(crate) fn sublayer_properties(&mut self, ui: &mut egui::Ui, key: &str) {
        let (layer, sub) = key.split_once('/').unwrap_or((key, ""));
        let name = match sub.split('/').next().unwrap_or("") {
            "copper" => t!("layer.sub.copper"),
            "silk" => t!("layer.sub.silk"),
            "mask" => t!("layer.sub.mask"),
            "paste" => t!("layer.sub.paste"),
            "zones" => t!("layer.sub.zones"),
            other => other.to_owned(),
        };
        ui.label(fonts::title(name));
        ui.label(
            egui::RichText::new(t!("layer.sub.help"))
                .font(fonts::Role::Caption.font())
                .color(self.muted()),
        );
        ui.add_space(8.0);
        self.stack_properties(ui, layer);
    }

    /// Properties of an artifact: a record's file and state, or a drill artifact's
    /// layers it joins, its size and count.
    ///
    /// # Dependencies
    /// - crates/io/src/drill_table.rs (`drill_table` rows)
    ///
    /// # Do not
    /// - Compute counts here; ask the io crate.
    pub(crate) fn artifact_properties(&mut self, ui: &mut egui::Ui, rest: &str) {
        if let Some(id) = crate::artifact_rows::record(&self.project, rest).map(|a| a.id) {
            self.record_artifact_properties(ui, id);
            return;
        }
        ui.label(fonts::title(t!("artifact.drill.title")));
        let Some(key) = crate::selection::DrillKey::parse(rest) else {
            ui.label(t!("artifact.unknown"));
            return;
        };
        let rows = pcbcraft_io::drill_table(&self.project).unwrap_or_default();
        let row = rows.iter().find(|r| {
            r.from_layer == key.from + 1
                && r.to_layer == key.to + 1
                && (r.diameter_mm * 1000.0).round() as u64 == key.d_um
        });
        fact(
            ui,
            t!("drill_table.span"),
            format!("L{} to L{}", key.from + 1, key.to + 1),
            t!("drill_table.span.tip"),
        );
        fact(
            ui,
            t!("drill_table.diameter"),
            format!("{:.3} mm", key.d_um as f64 / 1000.0),
            t!("drill_table.diameter.tip"),
        );
        if let Some(r) = row {
            fact(
                ui,
                t!("drill_table.count"),
                r.count.to_string(),
                t!("drill_table.count.tip"),
            );
        }
        let vias = self
            .project
            .board
            .vias
            .iter()
            .filter(|v| key.matches_via(&self.project, v))
            .count();
        fact(
            ui,
            t!("artifact.drill.vias"),
            vias.to_string(),
            t!("artifact.drill.vias.tip"),
        );
    }
}
