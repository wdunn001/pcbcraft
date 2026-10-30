//! The Sourcing section of a part's Properties: maker, numbers, lifecycle and suppliers.
//!
//! Context: issues #227 and #804. The section edits `Component::sourcing`, the typed
//! record the bill of materials reads. It sits under Manufacturing in `part_properties`.
//! Local rules: every edit is one engine command (`component.set_sourcing`,
//! `component.set_supplier`); a text box commits on Enter or when it loses focus; every
//! control has a tooltip and an accessible name from the catalog. The text being typed
//! lives in egui memory keyed by part and field, so no `App` field is added; it resets
//! whenever the document value changes (undo, redo, import).
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/section.rs, form_row.rs, button.rs
//! - crates/doc/src/sourcing.rs (`Sourcing`, `SupplierPart`)
//! - crates/engine/src/lib.rs (`component.set_sourcing`, `component.set_supplier`)
//!
//! # Do not
//! - Write `Component::sourcing` directly; go through `App::run`.
//! - Show the free-form fields here; they stay in their own place.
use super::*;
use crate::fonts;
use crate::theme;
use crate::ui_kit::{
    button::{Size, ghost_button, secondary_button},
    form_row::{FormWidth, form_row},
    section::section,
};
use pcbcraft_library::listing::Lifecycle;

const ROW: FormWidth = FormWidth::Aside;

/// The box text and the document text it was last copied from.
type Draft = (String, String);

impl App {
    /// One text box in a form row. Returns the trimmed text when the person finished
    /// editing and it differs from the document.
    fn sourcing_text(
        ui: &mut egui::Ui,
        salt: &str,
        label: &str,
        name: &str,
        tip: &str,
        current: &str,
    ) -> Option<String> {
        let key = ui.id().with(("sourcing-draft", salt));
        let (mut text, seen): Draft = ui
            .data_mut(|d| d.get_temp(key))
            .unwrap_or_else(|| (current.to_owned(), current.to_owned()));
        if seen != current {
            text = current.to_owned();
        }
        let mut done = None;
        form_row(ui, ROW, label, None, None, |ui| {
            let r = ui
                .add(crate::targets::text_field(&mut text).desired_width(f32::INFINITY))
                .named(name)
                .on_hover_text(tip);
            if r.lost_focus() && text.trim() != current {
                done = Some(text.trim().to_owned());
            }
        });
        ui.data_mut(|d| d.insert_temp(key, (text, current.to_owned())));
        done
    }

    /// A drop-down in a form row. `options` are the labels; returns the index chosen.
    fn sourcing_choice(
        ui: &mut egui::Ui,
        salt: &str,
        label: &str,
        tip: &str,
        options: &[String],
        current: usize,
    ) -> Option<usize> {
        let mut chosen = None;
        form_row(ui, ROW, label, None, None, |ui| {
            egui::ComboBox::from_id_salt(("sourcing-choice", salt))
                .selected_text(options.get(current).cloned().unwrap_or_default())
                .show_ui(ui, |ui| {
                    for (i, text) in options.iter().enumerate() {
                        if ui
                            .selectable_label(i == current, text)
                            .on_hover_text(tip)
                            .clicked()
                        {
                            chosen = Some(i);
                        }
                    }
                })
                .response
                .named(label)
                .on_hover_text(tip);
        });
        chosen
    }

    /// The Sourcing section for the part `c`.
    ///
    /// Why: the bill of materials, the part search and the assembler files all need the
    /// maker, its part number and each supplier's number, in one editable place.
    ///
    /// # Rules
    /// - Each edit is one undo step through an engine command; a refused edit shows the
    ///   engine's message in the status line and leaves the document alone.
    /// - Clearing a text box clears the typed value; the free-form property stays.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/properties.rs `part_properties` calls it.
    ///
    /// # Do not
    /// - Keep the edited values on `App`.
    pub(crate) fn sourcing_section(&mut self, ui: &mut egui::Ui, c: &pcbcraft_doc::Component) {
        let id = c.id;
        let s = &c.sourcing;
        section(
            ui,
            "sourcing",
            &t!("props.sourcing"),
            &t!("props.sourcing.tip"),
            |ui| {
                let mut set: Vec<(&str, Value)> = vec![];
                if let Some(v) = Self::sourcing_text(
                    ui,
                    "manufacturer",
                    &t!("props.sourcing.manufacturer"),
                    &t!("props.sourcing.manufacturer"),
                    &t!("props.sourcing.manufacturer.tip"),
                    c.manufacturer(),
                ) {
                    set.push(("manufacturer", json!(v)));
                }
                if let Some(v) = Self::sourcing_text(
                    ui,
                    "mpn",
                    &t!("props.sourcing.mpn"),
                    &t!("props.sourcing.mpn.name"),
                    &t!("props.sourcing.mpn.tip"),
                    c.mpn(),
                ) {
                    set.push(("mpn", json!(v)));
                }
                if let Some(v) = Self::sourcing_text(
                    ui,
                    "description",
                    &t!("props.sourcing.description"),
                    &t!("props.sourcing.description"),
                    &t!("props.sourcing.description.tip"),
                    c.description(),
                ) {
                    set.push(("description", json!(v)));
                }
                let lifecycles = [
                    (t!("props.sourcing.lifecycle.none"), Value::Null),
                    (t!("props.sourcing.lifecycle.active"), json!("active")),
                    (t!("props.sourcing.lifecycle.nrnd"), json!("nrnd")),
                    (
                        t!("props.sourcing.lifecycle.last_time_buy"),
                        json!("last_time_buy"),
                    ),
                    (t!("props.sourcing.lifecycle.obsolete"), json!("obsolete")),
                ];
                let at = match s.lifecycle {
                    Some(Lifecycle::Active) => 1,
                    Some(Lifecycle::Nrnd) => 2,
                    Some(Lifecycle::LastTimeBuy) => 3,
                    Some(Lifecycle::Obsolete) => 4,
                    Some(Lifecycle::Unknown) | None => 0,
                };
                let labels: Vec<String> = lifecycles.iter().map(|(l, _)| l.clone()).collect();
                if let Some(i) = Self::sourcing_choice(
                    ui,
                    "lifecycle",
                    &t!("props.sourcing.lifecycle"),
                    &t!("props.sourcing.lifecycle.tip"),
                    &labels,
                    at,
                ) {
                    set.push(("lifecycle", lifecycles[i].1.clone()));
                }
                let mut levels = vec![t!("props.sourcing.moisture.none")];
                levels.extend((1..=6).map(|n| t!("props.sourcing.moisture.level", level = n)));
                if let Some(i) = Self::sourcing_choice(
                    ui,
                    "moisture",
                    &t!("props.sourcing.moisture"),
                    &t!("props.sourcing.moisture.tip"),
                    &levels,
                    s.moisture_level.map_or(0, usize::from),
                ) {
                    set.push((
                        "moisture_level",
                        if i == 0 { Value::Null } else { json!(i) },
                    ));
                }
                let yes_no = vec![
                    t!("props.sourcing.lifecycle.none"),
                    t!("props.sourcing.yes"),
                    t!("props.sourcing.no"),
                ];
                let index = |v: Option<bool>| match v {
                    None => 0,
                    Some(true) => 1,
                    Some(false) => 2,
                };
                for (key, label, tip, value) in [
                    (
                        "rohs",
                        t!("props.sourcing.rohs"),
                        t!("props.sourcing.rohs.tip"),
                        s.rohs,
                    ),
                    (
                        "reach",
                        t!("props.sourcing.reach"),
                        t!("props.sourcing.reach.tip"),
                        s.reach,
                    ),
                    (
                        "halogen_free",
                        t!("props.sourcing.halogen"),
                        t!("props.sourcing.halogen.tip"),
                        s.halogen_free,
                    ),
                ] {
                    if let Some(i) =
                        Self::sourcing_choice(ui, key, &label, &tip, &yes_no, index(value))
                    {
                        set.push((
                            key,
                            match i {
                                0 => Value::Null,
                                1 => json!(true),
                                _ => json!(false),
                            },
                        ));
                    }
                }
                if !set.is_empty() {
                    let mut params = json!({"id": id});
                    for (key, value) in set {
                        params[key] = value;
                    }
                    self.run("component.set_sourcing", params);
                }
                self.supplier_rows(ui, c);
            },
        );
    }

    /// The supplier list: one editable row each, then a row to add another.
    pub(crate) fn supplier_rows(&mut self, ui: &mut egui::Ui, c: &pcbcraft_doc::Component) {
        let t = theme::tokens(ui.ctx());
        ui.add_space(4.0);
        ui.label(fonts::text(fonts::Role::Body, t!("props.sourcing.suppliers")).color(t.text))
            .on_hover_text(t!("props.sourcing.suppliers.tip"));
        let mut commands: Vec<(&str, Value)> = vec![];
        for supplier in &c.sourcing.suppliers {
            let tip = t!(
                "props.sourcing.supplier.number.tip",
                supplier = supplier.supplier
            );
            if let Some(v) = Self::sourcing_text(
                ui,
                &format!("supplier-{}", supplier.supplier),
                &supplier.supplier,
                &format!("{} part number", supplier.supplier),
                &tip,
                &supplier.part_number,
            ) && !v.is_empty()
            {
                commands.push((
                    "component.set_supplier",
                    json!({"id": c.id, "supplier": supplier.supplier, "part_number": v,
                           "url": supplier.url}),
                ));
            }
            if ghost_button(
                ui,
                None,
                &t!("props.sourcing.supplier.remove"),
                &t!(
                    "props.sourcing.supplier.remove.tip",
                    supplier = supplier.supplier
                ),
                Size::Small,
                true,
            )
            .clicked()
            {
                commands.push((
                    "component.set_supplier",
                    json!({"id": c.id, "supplier": supplier.supplier, "part_number": null}),
                ));
            }
        }
        let key = ui.id().with(("sourcing-new", c.id));
        let (mut name, mut number): (String, String) =
            ui.data_mut(|d| d.get_temp(key)).unwrap_or_default();
        form_row(
            ui,
            ROW,
            &t!("props.sourcing.new_supplier"),
            None,
            None,
            |ui| {
                ui.add(crate::targets::text_field(&mut name).desired_width(f32::INFINITY))
                    .named(&t!("props.sourcing.new_supplier.name"))
                    .on_hover_text(t!("props.sourcing.new_supplier.name.tip"));
            },
        );
        form_row(
            ui,
            ROW,
            &t!("props.sourcing.new_supplier.number"),
            None,
            None,
            |ui| {
                ui.add(crate::targets::text_field(&mut number).desired_width(f32::INFINITY))
                    .named(&t!("props.sourcing.new_supplier.number.name"))
                    .on_hover_text(t!("props.sourcing.new_supplier.number.tip"));
            },
        );
        let ready = !name.trim().is_empty() && !number.trim().is_empty();
        if secondary_button(
            ui,
            None,
            &t!("props.sourcing.new_supplier.add"),
            &t!("props.sourcing.new_supplier.add.tip"),
            Size::Small,
            ready,
        )
        .clicked()
        {
            commands.push((
                "component.set_supplier",
                json!({"id": c.id, "supplier": name.trim(), "part_number": number.trim()}),
            ));
            name.clear();
            number.clear();
        }
        ui.data_mut(|d| d.insert_temp(key, (name, number)));
        for (command, params) in commands {
            self.run(command, params);
        }
    }
}
