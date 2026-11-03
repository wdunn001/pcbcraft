//! Moving and renaming net labels and power symbols with the Select tool.
//!
//! Context: a label or power symbol was placed with `label.place` and could
//! never be touched again. This file adds the three missing paths: a press that
//! picks one up, a drag that moves it with `label.move`, and the Properties
//! section that renames the net it names or restyles a power symbol with
//! `label.set`.
//! Local rules: every change goes through `App::run`; the drag preview is
//! painted on a copy of the project, so the document only changes on release.
//!
//! # Dependencies
//! - crates/ui-egui/src/canvas/gestures.rs (press, drag and release hooks)
//! - crates/ui-egui/src/properties.rs (calls [`App::label_controls`])
//! - crates/ui-egui/src/targets.rs (`MIN_TARGET`, `text_field`)
//!
//! # Do not
//! - Keep label state outside the project; a typed draft lives in egui memory
//!   until the field commits.

use super::*;
use crate::a11y::Named;
use crate::theme;
use crate::ui_kit::{
    form_row::{FormWidth, form_row},
    section::section,
    segmented::{Segment, SegmentedStyle, segmented_styled},
};
use pcbcraft_doc::Label;

impl App {
    /// The label or power symbol under `pos` that a press would pick up.
    ///
    /// Why: labels were only reachable within 7 points; a finger or a mouse
    /// needs the minimum target size.
    ///
    /// # Rules
    /// - Reach is half of `MIN_TARGET` in screen points, so the target is at
    ///   least that big at every zoom.
    /// - Junction dots keep a small reach so they never shadow wire drags.
    /// - Only labels on the shown page count; the nearest one wins.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/targets.rs (`MIN_TARGET`)
    ///
    /// # Do not
    /// - Use this on the board tab.
    pub(crate) fn label_under(&self, pos: Point, zoom: f32) -> Option<&Label> {
        let page = self.schematic_page();
        let reach = |l: &Label| {
            if l.kind == "junction" {
                7.0
            } else {
                f64::from(crate::targets::MIN_TARGET) / 2.0
            }
        };
        self.project
            .labels
            .iter()
            .filter(|l| self.project.schematic_label_page(l) == page)
            .filter(|l| l.kind != "bus")
            .filter(|l| l.position.distance(pos) * f64::from(zoom) <= reach(l))
            .min_by(|a, b| {
                a.position
                    .distance(pos)
                    .total_cmp(&b.position.distance(pos))
            })
    }

    /// True when `label` is closer to `pos` than the pin is, so the label wins
    /// the press and the pin keeps starting wires only where no label sits.
    pub(crate) fn label_beats_pin(&self, label: &Label, pin: &PinRef, pos: Point) -> bool {
        self.project
            .pin_position(pin)
            .is_none_or(|at| label.position.distance(pos) < at.distance(pos))
    }

    /// A press on a label: select it and give back where it stands.
    pub(crate) fn label_press(&mut self, id: Id) -> Option<Point> {
        let at = self
            .project
            .labels
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.position)?;
        self.select(Some(id));
        Some(at)
    }

    /// Where a label dragged by a press that began at `from` lands at `to`.
    ///
    /// # Rules
    /// - The shift is the pointer travel, so the label does not jump to the
    ///   pointer; the result snaps to the grid unless `snapped` is false.
    pub(crate) fn label_drop_point(origin: Point, from: Point, to: Point, snapped: bool) -> Point {
        let at = origin.offset(Point::new(to.x - from.x, to.y - from.y));
        if snapped { canvas::snap(at) } else { at }
    }

    /// Run `label.move` for a finished drag; a drag that ends where it began
    /// does nothing.
    pub(crate) fn label_drag_end(&mut self, id: Id, origin: Point, to: Point) {
        if to != origin {
            self.run("label.move", json!({"id": id, "position": to}));
        }
    }

    /// A plain click on a label selects it. Returns true when it did.
    pub(crate) fn select_label_hit(&mut self, hit: &canvas::gestures::Hit) -> bool {
        if matches!(self.tool, Tool::Select)
            && let canvas::gestures::Hit::NetLabel(id) = hit
        {
            self.select(Some(*id));
            return true;
        }
        false
    }

    /// The Properties section of a selected net label or power symbol: the
    /// net it names, and for a power symbol its drawn style.
    ///
    /// # Rules
    /// - The name commits on Enter or when the field loses focus, as one
    ///   `label.set`; a name in use joins that net, a new name makes the net.
    /// - The style row shows for power symbols only and is one `label.set`.
    /// - Junctions and bus labels get no section.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (`label.set`)
    ///
    /// # Do not
    /// - Run a command while typing; only on commit.
    pub(crate) fn label_controls(&mut self, ui: &mut egui::Ui, id: Id) {
        let Some(label) = self.project.labels.iter().find(|l| l.id == id).cloned() else {
            return;
        };
        if label.kind == "junction" || label.kind == "bus" {
            return;
        }
        let current = self.project.net_name(label.net).to_owned();
        let draft_key = egui::Id::new(("label-net-draft", id));
        let mut commit: Option<String> = None;
        let mut style_pick: Option<usize> = None;
        section(
            ui,
            "label",
            &t!("props.label"),
            &t!("props.label.tip"),
            |ui| {
                let mut text: String = ui
                    .data(|d| d.get_temp(draft_key))
                    .unwrap_or_else(|| current.clone());
                form_row(
                    ui,
                    FormWidth::Aside,
                    &t!("props.label.net"),
                    None,
                    None,
                    |ui| {
                        let r = ui
                            .add(crate::targets::text_field(&mut text).desired_width(140.0))
                            .on_hover_text(t!("props.label.net.tip"))
                            .named(&t!("props.label.net"));
                        if r.changed() {
                            ui.data_mut(|d| d.insert_temp(draft_key, text.clone()));
                        }
                        if r.lost_focus() {
                            ui.data_mut(|d| d.remove::<String>(draft_key));
                            commit = Some(text.trim().to_owned());
                        } else if !r.has_focus() && !r.changed() {
                            ui.data_mut(|d| d.remove::<String>(draft_key));
                        }
                    },
                );
                if label.kind == "power" {
                    let mut pick = Self::power_style_index(&label, &current);
                    let names = [
                        t!("schematic.power.style.ground"),
                        t!("schematic.power.style.bar"),
                        t!("schematic.power.style.arrow"),
                        t!("schematic.power.style.negative"),
                    ];
                    let tips = [
                        t!("props.label.style.ground.tip"),
                        t!("props.label.style.bar.tip"),
                        t!("props.label.style.arrow.tip"),
                        t!("props.label.style.negative.tip"),
                    ];
                    let items: Vec<Segment<'_>> = names
                        .iter()
                        .zip(tips.iter())
                        .map(|(n, tip)| Segment::text(n, tip))
                        .collect();
                    let group = t!("props.label.style");
                    form_row(ui, FormWidth::Aside, &group, None, None, |ui| {
                        segmented_styled(
                            ui,
                            "label-style",
                            &mut pick,
                            &items,
                            SegmentedStyle {
                                group_name: Some(&group),
                                ..Default::default()
                            },
                        );
                    });
                    if pick != Self::power_style_index(&label, &current) {
                        style_pick = Some(pick);
                    }
                }
                let t = theme::tokens(ui.ctx());
                ui.label(crate::fonts::caption(t!("props.label.hint")).color(t.muted));
            },
        );
        if let Some(name) = commit
            && !name.is_empty()
            && name != current
        {
            self.run("label.set", json!({"id": id, "name": name}));
        }
        if let Some(pick) = style_pick
            && let Some(style) = Label::POWER_STYLES.get(pick)
        {
            self.run("label.set", json!({"id": id, "style": style}));
        }
    }

    /// Which power style a symbol shows now: its own, or the default the
    /// sheet draws (ground for a ground-like net name, else bar).
    fn power_style_index(label: &Label, net_name: &str) -> usize {
        let wanted = if label.style.is_empty() {
            let n = net_name.trim().trim_start_matches('+').to_ascii_uppercase();
            if n.starts_with("GND") || matches!(n.as_str(), "VSS" | "0V" | "EARTH") {
                "ground"
            } else {
                "bar"
            }
        } else {
            label.style.as_str()
        };
        Label::POWER_STYLES
            .iter()
            .position(|s| *s == wanted)
            .unwrap_or(1)
    }
}
