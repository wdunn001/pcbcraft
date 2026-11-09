//! Hierarchy ports on schematic pages and child sheet boxes on the parent canvas.

use super::*;
use crate::canvas::snap;
use crate::canvas::text_style::{TextRole, role_font};
use crate::fonts;
use crate::tool_modifiers::Held;
use crate::ui_kit::{
    form_row::{FormWidth, form_row},
    section::section,
    segmented::{Segment, SegmentedStyle, segmented_styled},
    select::select,
};

/// Child page box width on the parent sheet, in millimetres.
pub(crate) const CHILD_BOX_W: f64 = 50.8;
/// Child page box height on the parent sheet, in millimetres.
pub(crate) const CHILD_BOX_H: f64 = 40.64;

const ROW: FormWidth = FormWidth::Aside;

fn box_rect(entry: Point) -> (Point, Point) {
    (
        entry,
        Point::new(entry.x + CHILD_BOX_W, entry.y + CHILD_BOX_H),
    )
}

fn inside_box(entry: Point, pos: Point) -> bool {
    let (a, b) = box_rect(entry);
    pos.x >= a.x && pos.x <= b.x && pos.y >= a.y && pos.y <= b.y
}

/// The child page whose box contains `pos` on parent page `parent`.
pub(crate) fn child_sheet_at(project: &Project, parent: Id, pos: Point) -> Option<Id> {
    project
        .sheets
        .iter()
        .filter(|s| s.parent == Some(parent))
        .find_map(|s| {
            s.entry
                .filter(|entry| inside_box(*entry, pos))
                .map(|_| s.id)
        })
}

impl App {
    /// Names a hierarchy port can take: every net name plus ports on other pages.
    fn port_name_choices(&self) -> Vec<String> {
        let page = self.schematic_page();
        let mut names: Vec<String> = self.project.nets.iter().map(|n| n.name.clone()).collect();
        for port in &self.project.ports {
            if port.sheet != page && !names.iter().any(|n| n == &port.name) {
                names.push(port.name.clone());
            }
        }
        names.sort_by_key(|a| a.to_lowercase());
        names
    }

    fn port_pick_index(&self) -> usize {
        let choices = self.port_name_choices();
        if choices.is_empty() {
            return 0;
        }
        choices
            .iter()
            .position(|n| n == &self.port_pick_name)
            .unwrap_or(0)
    }

    /// Net carried by placing a port named `name` on the current page.
    fn port_net_for_name(&self, name: &str) -> Option<Id> {
        if let Some(n) = self.project.nets.iter().find(|n| n.name == name) {
            return Some(n.id);
        }
        self.project
            .ports
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.net)
    }

    /// Properties for the Port tool: pick the port name, then click the canvas.
    pub(crate) fn port_tool_options(&mut self, ui: &mut egui::Ui) {
        let choices = self.port_name_choices();
        section(
            ui,
            "port-tool",
            &t!("schematic.port.props.section"),
            &t!("schematic.port.props.section.tip"),
            |ui| {
                if choices.is_empty() {
                    ui.label(fonts::caption(t!("schematic.port.none")));
                    return;
                }
                let names: Vec<&str> = choices.iter().map(String::as_str).collect();
                let mut index = self.port_pick_index();
                let before = index;
                form_row(
                    ui,
                    ROW,
                    &t!("schematic.port.props.name"),
                    None,
                    None,
                    |ui| {
                        select(
                            ui,
                            "port-name",
                            &mut index,
                            &names,
                            &t!("schematic.port.props.name"),
                            &t!("schematic.port.props.name.tip"),
                            160.0,
                        );
                    },
                );
                if index != before
                    && let Some(name) = choices.get(index)
                {
                    self.port_pick_name = name.clone();
                } else if self.port_pick_name.is_empty()
                    && let Some(name) = choices.first()
                {
                    self.port_pick_name.clone_from(name);
                }
            },
        );
    }

    pub(crate) fn place_hierarchy_port(&mut self, pos: Point) {
        let name = self.port_pick_name.trim().to_string();
        let name = name.as_str();
        if name.is_empty() {
            self.message = t!("schematic.port.pick_a_name");
            return;
        }
        let mut found = self.port_net_for_name(name);
        if found.is_none() && self.tool_call("net.create", json!({"name": name})) {
            found = self
                .project
                .nets
                .iter()
                .find(|n| n.name == name)
                .map(|n| n.id);
        }
        let Some(net) = found else {
            self.message = t!("schematic.port.no_net");
            return;
        };
        let (page, local) = self.spot_to_page(snap(pos));
        self.tool_call(
            "schematic.place_port",
            json!({
                "name": name,
                "net": net,
                "sheet": page,
                "position": local,
            }),
        );
    }

    /// Open a child page when the pointer double-clicks its box on the parent sheet.
    pub(crate) fn schematic_double_click(&mut self, pos: Point, zoom: f32, held: Held) {
        if self.tab == Tab::Schematic {
            let parent = self.schematic_page();
            if let Some(child) = child_sheet_at(&self.project, parent, pos) {
                self.show_page(child);
                return;
            }
        }
        self.double_click(pos, zoom, held);
    }

    pub(crate) fn paint_child_sheet_boxes(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        p: &Project,
        page: Id,
        sc: &canvas::sheet::SheetColours,
    ) {
        for sheet in p.sheets.iter().filter(|s| s.parent == Some(page)) {
            let Some(entry) = sheet.entry else {
                continue;
            };
            let (a, b) = box_rect(entry);
            let tl = camera.screen(a, rect);
            let br = camera.screen(b, rect);
            let frame = egui::Rect::from_min_max(tl, br);
            painter.rect_stroke(
                frame,
                0.0,
                Stroke::new(2.0, sc.wire),
                egui::StrokeKind::Inside,
            );
            painter.text(
                frame.left_top() + egui::vec2(4.0, 2.0),
                Align2::LEFT_TOP,
                &sheet.name,
                role_font(TextRole::Value, camera.zoom),
                sc.text,
            );
            let ports: Vec<_> = p
                .ports
                .iter()
                .filter(|port| port.sheet == sheet.id)
                .collect();
            for (i, port) in ports.iter().enumerate() {
                let at = Point::new(entry.x + 3.0, entry.y + 8.0 + (i as f64) * 5.08);
                painter.text(
                    camera.screen(at, rect),
                    Align2::LEFT_CENTER,
                    format!("[{}]", port.name),
                    role_font(TextRole::PortName, camera.zoom),
                    sc.wire,
                );
            }
        }
    }
}

impl App {
    /// The port nearest `pos` on the shown page, if within 8 screen points.
    ///
    /// # Rules
    /// - Only ports on the shown page count; the nearest one wins.
    ///
    /// # Dependencies
    /// - crates/doc/src/lib.rs (`Port`)
    ///
    /// # Do not
    /// - Use this on the board tab.
    pub(crate) fn port_under(&self, pos: Point, zoom: f32) -> Option<Id> {
        let page = self.schematic_page();
        let sheet = self.sheet_view();
        let reach = 8.0;
        sheet
            .ports
            .iter()
            .filter(|p| crate::page_scope::in_view(&sheet, page, p.sheet))
            .filter(|p| p.position.distance(pos) * f64::from(zoom) <= reach)
            .min_by(|a, b| {
                a.position
                    .distance(pos)
                    .total_cmp(&b.position.distance(pos))
            })
            .map(|p| p.id)
    }

    /// A plain click on a port selects it. Returns true when it did.
    ///
    /// # Rules
    /// - Only in Select mode.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/lib.rs (`select`)
    ///
    /// # Do not
    /// - Select in any other tool mode.
    pub(crate) fn select_port_hit(&mut self, hit: &canvas::gestures::Hit) -> bool {
        if matches!(self.tool, Tool::Select)
            && let canvas::gestures::Hit::Port(id) = hit
        {
            self.select(Some(*id));
            return true;
        }
        false
    }

    /// Properties for a selected port: its name and signal direction.
    ///
    /// # Rules
    /// - The direction picker runs `schematic.set_port_direction` on change.
    /// - The name is read-only; use the rename command for that.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (`schematic.set_port_direction`)
    ///
    /// # Do not
    /// - Edit the name here.
    pub(crate) fn port_controls(&mut self, ui: &mut egui::Ui, id: Id) {
        let Some(port) = self.project.ports.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        section(
            ui,
            "port-direction",
            &t!("schematic.port.props.section"),
            &t!("schematic.port.props.section.tip"),
            |ui| {
                form_row(
                    ui,
                    ROW,
                    &t!("schematic.port.props.name"),
                    None,
                    None,
                    |ui| {
                        ui.label(fonts::text(fonts::Role::Mono, &port.name));
                    },
                );
                let directions = ["input", "output", "bidirectional", "passive"];
                let in_label = t!("schematic.port.direction.input");
                let out_label = t!("schematic.port.direction.output");
                let both_label = t!("schematic.port.direction.bidirectional");
                let passive_label = t!("schematic.port.direction.passive");
                let items = [
                    Segment::text(&in_label, ""),
                    Segment::text(&out_label, ""),
                    Segment::text(&both_label, ""),
                    Segment::text(&passive_label, ""),
                ];
                let mut index = directions
                    .iter()
                    .position(|d| *d == port.direction)
                    .unwrap_or(2);
                let before = index;
                form_row(ui, ROW, &t!("schematic.port.direction"), None, None, |ui| {
                    segmented_styled(
                        ui,
                        "port-direction",
                        &mut index,
                        &items,
                        SegmentedStyle {
                            full_width: true,
                            group_name: Some("schematic.port.direction"),
                        },
                    );
                });
                if index != before {
                    let dir = directions[index];
                    self.run(
                        "schematic.set_port_direction",
                        json!({"id": id, "direction": dir}),
                    );
                }
            },
        );
    }
}
