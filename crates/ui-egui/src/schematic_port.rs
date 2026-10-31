//! Hierarchy ports on schematic pages and child sheet boxes on the parent canvas.

use super::*;
use crate::fonts;
use crate::ui_kit::{
    form_row::{FormWidth, form_row},
    section::section,
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

/// A default top-left corner for another child box on `parent`.
pub(crate) fn default_sheet_entry(project: &Project, parent: Id) -> Point {
    let n = project
        .sheets
        .iter()
        .filter(|s| s.parent == Some(parent))
        .count();
    Point::new(25.0 + (n as f64) * (CHILD_BOX_W + 5.0), 35.0)
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
        names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
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
            &t!("schematic.port.section"),
            &t!("schematic.port.section.tip"),
            |ui| {
                if choices.is_empty() {
                    ui.label(fonts::caption(t!("schematic.port.none")));
                    return;
                }
                let names: Vec<&str> = choices.iter().map(String::as_str).collect();
                let mut index = self.port_pick_index();
                let before = index;
                form_row(ui, ROW, &t!("schematic.port.name"), None, None, |ui| {
                    select(
                        ui,
                        "port-name",
                        &mut index,
                        &names,
                        &t!("schematic.port.name"),
                        &t!("schematic.port.name.tip"),
                        160.0,
                    );
                });
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
        let name = self.port_pick_name.trim();
        if name.is_empty() {
            self.message = t!("schematic.port.pick_a_name");
            return;
        }
        let Some(net) = self.port_net_for_name(name).or_else(|| {
            if self.tool_call("net.create", json!({"name": name})) {
                self.project
                    .nets
                    .iter()
                    .find(|n| n.name == name)
                    .map(|n| n.id)
            } else {
                None
            }
        }) else {
            self.message = t!("schematic.port.no_net");
            return;
        };
        let page = self.schematic_page();
        self.tool_call(
            "schematic.place_port",
            json!({
                "name": name,
                "net": net,
                "sheet": page,
                "position": snap(pos),
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
                FontId::proportional(11.0),
                sc.text,
            );
            let ports: Vec<_> = p.ports.iter().filter(|port| port.sheet == sheet.id).collect();
            for (i, port) in ports.iter().enumerate() {
                let at = Point::new(entry.x + 3.0, entry.y + 8.0 + (i as f64) * 5.08);
                painter.text(
                    camera.screen(at, rect),
                    Align2::LEFT_CENTER,
                    format!("[{}]", port.name),
                    FontId::monospace(11.0),
                    sc.wire,
                );
            }
        }
    }
}
