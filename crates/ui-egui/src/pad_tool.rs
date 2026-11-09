//! The Pad tool and the pad rows of Properties: add a pad to the board, to a
//! placed part or to the schematic, and change a placed pad.
//!
//! Context: issue 1164. A free pad is a one-pad part made by the engine command
//! `pad.add`; a pad of a placed part is changed by `pad.edit` and removed by
//! `pad.remove`. This file draws the form for both and sends those commands.
//! Local rules: the form edits a `PadFields` value; the Pad tool keeps one as
//! its draft, the Pads section of a placed part builds one from each pad and
//! sends only what the person changed. Every control has a tooltip and a plain
//! label; nothing here paints a glyph from text.
//!
//! # Dependencies
//! - crates/engine/src/pads.rs for the commands.
//! - crates/ui-egui/src/ui_kit/ (form_row, number_field, select, section).
//!
//! # Do not
//! - Change the design here; every edit goes through `App::run`.
use super::*;
use crate::fonts;
use crate::ui_kit::{
    button::{Size, secondary_button},
    form_row::{FormWidth, form_row},
    number_field::{NumberSpec, number_field},
    section::section,
    select::select,
};
use pcbcraft_library::{NotchPlace, PadKind};

const ROW: FormWidth = FormWidth::Aside;

/// Copper shapes, in the order of the shape list.
const SHAPES: [&str; 4] = ["circle", "rect", "oval", "roundrect"];
/// Sides, in the order of the side list.
const SIDES: [&str; 3] = ["top", "bottom", "through"];
/// Pad kinds, in the order of the kind list.
const KINDS: [PadKind; 3] = [PadKind::Plain, PadKind::Exposed, PadKind::Notched];

/// The values of the pad form: the draft of the Pad tool, or one placed pad.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PadFields {
    /// Pad number or name; empty lets the engine pick the next free number.
    pub number: String,
    /// Index into the side list: top, bottom or through-hole.
    pub side: usize,
    /// Index into the shape list.
    pub shape: usize,
    /// Copper width in millimetres.
    pub width: f64,
    /// Copper height in millimetres.
    pub height: f64,
    /// Drill diameter in millimetres, used when the side is through-hole.
    pub drill: f64,
    /// The net the pad joins, if any.
    pub net: Option<Id>,
    /// Index into the kind list: plain, exposed or notched.
    pub kind: usize,
    /// Index into the paste list: full or none (notched pads only).
    pub paste: usize,
    /// Index into `NotchPlace::ALL`.
    pub notch_place: usize,
    /// Notch width in millimetres.
    pub notch_width: f64,
    /// Notch depth in millimetres.
    pub notch_depth: f64,
}

impl Default for PadFields {
    fn default() -> Self {
        Self {
            number: String::new(),
            side: 0,
            shape: 0,
            width: 1.6,
            height: 1.6,
            drill: 0.8,
            net: None,
            kind: 0,
            paste: 0,
            notch_place: 0,
            notch_width: 0.4,
            notch_depth: 0.4,
        }
    }
}

/// Which fields the person changed in one frame.
#[derive(Default, Clone, Copy)]
pub(crate) struct Changed {
    pub number: bool,
    pub side: bool,
    pub shape: bool,
    pub size: bool,
    pub drill: bool,
    pub net: bool,
    pub kind: bool,
    pub paste: bool,
    pub notch: bool,
}

impl Changed {
    fn any(self) -> bool {
        self.number
            || self.side
            || self.shape
            || self.size
            || self.drill
            || self.net
            || self.kind
            || self.paste
            || self.notch
    }
}

impl PadFields {
    /// The fields of a placed pad.
    fn of_pad(
        project: &Project,
        footprint: &pcbcraft_doc::Footprint,
        pad: &pcbcraft_doc::Pad,
    ) -> Self {
        let g = pad.geometry.as_ref();
        let through = g.is_some_and(|g| g.drill.is_some());
        let shape = g
            .and_then(|g| SHAPES.iter().position(|s| *s == g.shape))
            .unwrap_or(0);
        let _ = project;
        let kind = g.map_or(PadKind::Plain, |g| g.kind());
        let notch = g.and_then(|g| g.notch());
        let defaults = Self::default();
        Self {
            number: pad.number.clone(),
            side: if through {
                2
            } else {
                usize::from(footprint.board_side() != pcbcraft_doc::Layer::Front)
            },
            shape,
            width: g.map_or(pad.diameter, |g| g.size.x),
            height: g.map_or(pad.diameter, |g| g.size.y),
            drill: g.and_then(|g| g.drill.as_ref()).map_or(0.8, |d| d.size.x),
            net: pad.net,
            kind: KINDS.iter().position(|k| *k == kind).unwrap_or(0),
            paste: usize::from(g.is_some_and(|g| !g.has_paste())),
            notch_place: notch
                .and_then(|(p, _, _)| NotchPlace::ALL.iter().position(|q| *q == p))
                .unwrap_or(defaults.notch_place),
            notch_width: notch.map_or(defaults.notch_width, |(_, w, _)| w),
            notch_depth: notch.map_or(defaults.notch_depth, |(_, _, d)| d),
        }
    }

    /// The kind, paste and notch parameters of this form, shared by add and edit.
    fn kind_params(&self, params: &mut Value) {
        let kind = KINDS[self.kind.min(2)];
        params["kind"] = json!(kind.word());
        if kind == PadKind::Notched {
            params["paste"] = json!(if self.paste == 0 { "full" } else { "none" });
            params["notch_place"] = json!(NotchPlace::ALL[self.notch_place.min(7)].word());
            params["notch_width"] = json!(self.notch_width);
            params["notch_depth"] = json!(self.notch_depth);
            params["shape"] = json!("rect");
        }
    }

    /// The `pad.add` parameters for this draft.
    pub(crate) fn add_params(&self) -> Value {
        let mut params = json!({
            "shape": SHAPES[self.shape.min(3)],
            "width": self.width,
            "height": self.height,
            "side": SIDES[self.side.min(2)],
        });
        if self.side == 2 {
            params["drill"] = json!(self.drill);
        }
        self.kind_params(&mut params);
        if !self.number.trim().is_empty() {
            params["number"] = json!(self.number.trim());
        }
        if let Some(net) = self.net {
            params["net"] = json!(net);
        }
        params
    }
}

impl App {
    /// The form rows for a pad. `salt` keeps the controls of several pads apart;
    /// `live` sends number edits on every key, otherwise on leaving the field.
    fn pad_form(
        &self,
        ui: &mut egui::Ui,
        salt: impl std::hash::Hash + std::fmt::Debug + Copy,
        f: &mut PadFields,
        live: bool,
    ) -> Changed {
        let mut changed = Changed::default();
        form_row(
            ui,
            ROW,
            &t!("pad.number"),
            Some(&t!("pad.number.tip")),
            None,
            |ui| {
                let memory = egui::Id::new((salt, "pad-number"));
                let mut text: String = ui
                    .data(|d| d.get_temp(memory))
                    .unwrap_or_else(|| f.number.clone());
                let r = ui
                    .add(crate::targets::text_field(&mut text).desired_width(f32::INFINITY))
                    .named(&t!("pad.number"))
                    .on_hover_text(t!("pad.number.tip"));
                if live {
                    if text != f.number {
                        f.number = text;
                        changed.number = true;
                    }
                } else if r.lost_focus() {
                    ui.data_mut(|d| d.remove_temp::<String>(memory));
                    if text != f.number {
                        f.number = text;
                        changed.number = true;
                    }
                } else if r.has_focus() {
                    ui.data_mut(|d| d.insert_temp(memory, text));
                }
            },
        );
        let sides = [
            t!("pad.side.top"),
            t!("pad.side.bottom"),
            t!("pad.side.through"),
        ];
        let side_names: Vec<&str> = sides.iter().map(String::as_str).collect();
        form_row(ui, ROW, &t!("pad.side"), None, None, |ui| {
            let before = f.side;
            select(
                ui,
                (salt, "pad-side"),
                &mut f.side,
                &side_names,
                &t!("pad.side"),
                &t!("pad.side.tip"),
                150.0,
            );
            changed.side = before != f.side;
        });
        let kinds = [
            t!("pad.kind.plain"),
            t!("pad.kind.exposed"),
            t!("pad.kind.notched"),
        ];
        let kind_names: Vec<&str> = kinds.iter().map(String::as_str).collect();
        form_row(ui, ROW, &t!("pad.kind"), None, None, |ui| {
            let before = f.kind;
            select(
                ui,
                (salt, "pad-kind"),
                &mut f.kind,
                &kind_names,
                &t!("pad.kind"),
                &t!("pad.kind.tip"),
                150.0,
            );
            changed.kind = before != f.kind;
        });
        let notched = f.kind == 2;
        let shapes = [
            t!("pad.shape.circle"),
            t!("pad.shape.rect"),
            t!("pad.shape.oval"),
            t!("pad.shape.roundrect"),
        ];
        let shape_names: Vec<&str> = shapes.iter().map(String::as_str).collect();
        if !notched {
            form_row(ui, ROW, &t!("pad.shape"), None, None, |ui| {
                let before = f.shape;
                select(
                    ui,
                    (salt, "pad-shape"),
                    &mut f.shape,
                    &shape_names,
                    &t!("pad.shape"),
                    &t!("pad.shape.tip"),
                    150.0,
                );
                changed.shape = before != f.shape;
            });
        }
        let spec = NumberSpec {
            min: 0.05,
            max: 100.0,
            step: 0.1,
            width: 110.0,
            ..NumberSpec::default()
        };
        form_row(ui, ROW, &t!("pad.width"), None, None, |ui| {
            changed.size |= number_field(
                ui,
                (salt, "pad-width"),
                &mut f.width,
                &spec,
                &t!("pad.width"),
                &t!("pad.width.tip"),
            )
            .changed();
        });
        if f.shape != 0 || notched {
            form_row(ui, ROW, &t!("pad.height"), None, None, |ui| {
                changed.size |= number_field(
                    ui,
                    (salt, "pad-height"),
                    &mut f.height,
                    &spec,
                    &t!("pad.height"),
                    &t!("pad.height.tip"),
                )
                .changed();
            });
        }
        if notched {
            let places = [
                t!("pad.notch.corner_nw"),
                t!("pad.notch.corner_ne"),
                t!("pad.notch.corner_se"),
                t!("pad.notch.corner_sw"),
                t!("pad.notch.edge_n"),
                t!("pad.notch.edge_e"),
                t!("pad.notch.edge_s"),
                t!("pad.notch.edge_w"),
            ];
            let place_names: Vec<&str> = places.iter().map(String::as_str).collect();
            form_row(ui, ROW, &t!("pad.notch"), None, None, |ui| {
                let before = f.notch_place;
                select(
                    ui,
                    (salt, "pad-notch"),
                    &mut f.notch_place,
                    &place_names,
                    &t!("pad.notch"),
                    &t!("pad.notch.tip"),
                    150.0,
                );
                changed.notch |= before != f.notch_place;
            });
            form_row(ui, ROW, &t!("pad.notch_width"), None, None, |ui| {
                changed.notch |= number_field(
                    ui,
                    (salt, "pad-notch-width"),
                    &mut f.notch_width,
                    &spec,
                    &t!("pad.notch_width"),
                    &t!("pad.notch_width.tip"),
                )
                .changed();
            });
            form_row(ui, ROW, &t!("pad.notch_depth"), None, None, |ui| {
                changed.notch |= number_field(
                    ui,
                    (salt, "pad-notch-depth"),
                    &mut f.notch_depth,
                    &spec,
                    &t!("pad.notch_depth"),
                    &t!("pad.notch_depth.tip"),
                )
                .changed();
            });
        }
        if notched && f.side != 2 {
            let pastes = [t!("pad.paste.full"), t!("pad.paste.none")];
            let paste_names: Vec<&str> = pastes.iter().map(String::as_str).collect();
            form_row(ui, ROW, &t!("pad.paste"), None, None, |ui| {
                let before = f.paste;
                select(
                    ui,
                    (salt, "pad-paste"),
                    &mut f.paste,
                    &paste_names,
                    &t!("pad.paste"),
                    &t!("pad.paste.tip"),
                    150.0,
                );
                changed.paste = before != f.paste;
            });
        }
        if f.side == 2 {
            form_row(ui, ROW, &t!("pad.drill"), None, None, |ui| {
                changed.drill |= number_field(
                    ui,
                    (salt, "pad-drill"),
                    &mut f.drill,
                    &spec,
                    &t!("pad.drill"),
                    &t!("pad.drill.tip"),
                )
                .changed();
            });
        }
        let none = t!("pad.net.none");
        let mut names: Vec<&str> = vec![none.as_str()];
        names.extend(self.project.nets.iter().map(|n| n.name.as_str()));
        let mut index = f
            .net
            .and_then(|id| self.project.nets.iter().position(|n| n.id == id))
            .map_or(0, |i| i + 1);
        form_row(ui, ROW, &t!("pad.net"), None, None, |ui| {
            let before = index;
            select(
                ui,
                (salt, "pad-net"),
                &mut index,
                &names,
                &t!("pad.net"),
                &t!("pad.net.tip"),
                150.0,
            );
            if before != index {
                f.net = index
                    .checked_sub(1)
                    .and_then(|i| self.project.nets.get(i))
                    .map(|n| n.id);
                changed.net = true;
            }
        });
        changed
    }

    /// Properties for the Pad tool: the pad every click drops.
    pub(crate) fn pad_tool_options(&mut self, ui: &mut egui::Ui) {
        let mut draft = self.pad_draft.clone();
        section(
            ui,
            "pad-tool",
            &t!("pad.section"),
            &t!("pad.section.tip"),
            |ui| {
                self.pad_form(ui, "pad-draft", &mut draft, true);
            },
        );
        self.pad_draft = draft;
    }

    /// The outline of the pad under the pointer, drawn from the draft.
    pub(crate) fn paint_pad_ghost(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        at: Point,
    ) {
        let d = &self.pad_draft;
        let stroke = Stroke::new(1.5, self.accent());
        let centre = camera.screen(at, rect);
        let zoom = camera.zoom;
        if d.kind == 2
            && let Ok((ring, _)) = pcbcraft_library::notched_rectangle(
                d.width,
                d.height,
                NotchPlace::ALL[d.notch_place.min(7)],
                d.notch_width,
                d.notch_depth,
            )
        {
            let points: Vec<egui::Pos2> = ring
                .iter()
                .map(|v| camera.screen(Point::new(at.x + v.x, at.y + v.y), rect))
                .collect();
            painter.add(egui::Shape::closed_line(points, stroke));
            if d.side == 2 {
                painter.circle_stroke(centre, (d.drill / 2.0) as f32 * zoom, stroke);
            }
            return;
        }
        let half_w = (d.width / 2.0) as f32 * zoom;
        let half_h = (if d.shape == 0 { d.width } else { d.height } / 2.0) as f32 * zoom;
        match d.shape {
            0 => {
                painter.circle_stroke(centre, half_w, stroke);
            }
            2 => {
                let r = half_w.min(half_h);
                painter.rect_stroke(
                    Rect::from_center_size(centre, Vec2::new(half_w * 2.0, half_h * 2.0)),
                    r,
                    stroke,
                    egui::StrokeKind::Middle,
                );
            }
            3 => {
                painter.rect_stroke(
                    Rect::from_center_size(centre, Vec2::new(half_w * 2.0, half_h * 2.0)),
                    half_w.min(half_h) * 0.5,
                    stroke,
                    egui::StrokeKind::Middle,
                );
            }
            _ => {
                painter.rect_stroke(
                    Rect::from_center_size(centre, Vec2::new(half_w * 2.0, half_h * 2.0)),
                    0.0,
                    stroke,
                    egui::StrokeKind::Middle,
                );
            }
        }
        if d.side == 2 {
            painter.circle_stroke(centre, (d.drill / 2.0) as f32 * zoom, stroke);
        }
    }

    /// A click with the Pad tool: a free pad on the board, or a pad part on the sheet.
    pub(crate) fn place_pad_click(&mut self, pos: Point) {
        let mut params = self.pad_draft.add_params();
        let at = crate::canvas::snap(pos);
        if self.tab == Tab::Schematic {
            let (page, local) = self.spot_to_page(at);
            params["schematic_position"] = json!({"x": local.x, "y": local.y});
            params["sheet"] = json!(page);
            self.run("pad.add", params);
            // The sheet shows the part, not the footprint the board made for it.
            if let Some(component) = self
                .project
                .components
                .iter()
                .map(|c| c.id)
                .max()
                .filter(|_| self.project.footprint(self.selected.unwrap_or(0)).is_some())
            {
                self.select(Some(component));
            }
        } else {
            params["position"] = json!({"x": at.x, "y": at.y});
            self.run("pad.add", params);
        }
    }

    /// Add a pad to a placed footprint at a board position, from the right-click menu.
    pub(crate) fn add_pad_to_part(&mut self, footprint: Id, at: Point) {
        let mut params = self.pad_draft.add_params();
        if let Some(f) = self.project.footprint(footprint)
            && self.pad_draft.side != 2
        {
            params["side"] = json!(if f.board_side() == pcbcraft_doc::Layer::Front {
                "top"
            } else {
                "bottom"
            });
        }
        params["footprint"] = json!(footprint);
        params["position"] = json!({"x": at.x, "y": at.y});
        // The pad number of a part is its own; the draft number would clash on the second click.
        params.as_object_mut().map(|m| m.remove("number"));
        self.run("pad.add", params);
    }

    /// The Pads section of a placed part: one editable block per pad.
    pub(crate) fn pads_section(&mut self, ui: &mut egui::Ui, footprint_id: Id) {
        let Some(footprint) = self.project.footprint(footprint_id).cloned() else {
            return;
        };
        section(ui, "pads", &t!("pad.pads"), &t!("pad.pads.tip"), |ui| {
            for (i, pad) in footprint.pads.iter().enumerate() {
                let mut fields = PadFields::of_pad(&self.project, &footprint, pad);
                let before = fields.clone();
                ui.label(fonts::caption(t!(
                    "pad.pad_number",
                    number = pad.number.as_str()
                )));
                let changed = self.pad_form(ui, (footprint_id, i), &mut fields, false);
                if changed.any() {
                    let mut params = json!({"id": footprint_id, "pad": pad.number});
                    if changed.number {
                        params["number"] = json!(fields.number.trim());
                    }
                    if changed.shape {
                        params["shape"] = json!(SHAPES[fields.shape.min(3)]);
                    }
                    if changed.kind || changed.notch || changed.paste {
                        fields.kind_params(&mut params);
                    }
                    if changed.size {
                        params["width"] = json!(fields.width);
                        params["height"] = json!(fields.height);
                    }
                    if changed.side || changed.drill {
                        params["side"] = json!(SIDES[fields.side.min(2)]);
                        if fields.side == 2 {
                            params["drill"] = json!(fields.drill);
                        }
                    }
                    if changed.net {
                        match fields.net {
                            Some(net) => params["net"] = json!(net),
                            None => params["release_net"] = json!(true),
                        }
                    }
                    let _ = before;
                    self.run("pad.edit", params);
                    return;
                }
                if secondary_button(
                    ui,
                    None,
                    &t!("pad.remove"),
                    &t!("pad.remove.tip"),
                    Size::Small,
                    footprint.pads.len() > 1,
                )
                .clicked()
                {
                    self.run("pad.remove", json!({"id": footprint_id, "pad": pad.number}));
                    return;
                }
            }
        });
    }
}
