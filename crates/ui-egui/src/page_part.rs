//! The Main sheet shows every other page as one part with its outward pins, and
//! a toggle opens a page up to show its items.
//!
//! Context: a page keeps its own parts, wires and labels. On Main a page that is
//! closed is one part named after the page, with one pin for each public net of
//! that page, so Main reads as the top level of the design. Opening a page (one
//! toggle on its part, or one toggle for all pages in the tool bar) shows its
//! items on Main as before. Both are off by default and never saved in the file.
//!
//! # Dependencies
//! - crates/ui-egui/src/page_scope.rs (`main_instances`, `page_extent`, `apply_main_layout`)
//! - crates/ui-egui/src/wire_canvas.rs (`finish_wire_to_pin` calls `finish_wire_with_page_part`)
//! - crates/doc/src/lib.rs (`Port`, `Label`, `Project::schematic_label_page`)
//!
//! # Do not
//! - Send the page part to the engine or save it; it exists only in Main's view copy.
//! - Treat a net as private in a way the document does not say; until the document marks nets
//!   private, every net with a port, a power label or a global label on the page is public.

use super::*;
use crate::canvas::sheet::SheetColours;
use crate::canvas::text_style::{TextRole, role_font};
use crate::page_scope::{apply_main_layout, main_instances, page_extent};
use pcbcraft_doc::{Component, Pin};
use std::collections::{BTreeSet, HashSet};

/// Ids at or above this number belong to page parts, which exist only in Main's view copy.
pub(crate) const PART_BASE: Id = 1 << 62;
/// Width of a page part on the sheet, in millimetres.
pub(crate) const PART_W: f64 = 30.0;
/// Height of the name band at the top of a page part, in millimetres.
const HEADER: f64 = 8.0;
/// Distance between two pins of one side of a page part, in millimetres.
const PITCH: f64 = 5.08;
/// Side of the toggle square on a page part, in screen points.
const TOGGLE: f32 = 18.0;

/// Which pages Main has opened up to show their items.
///
/// Why: a page part is closed until a person opens it, and the choice is a view setting,
/// so it lives in the app and not in the design file.
///
/// # Rules
/// - `all` opens every page whatever `pages` holds.
/// - A page that is in neither is closed.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MainOpen {
    /// True when the tool bar toggle opened every page.
    pub(crate) all: bool,
    /// The pages opened one by one from their part.
    pub(crate) pages: BTreeSet<Id>,
}

impl MainOpen {
    /// True when the items of `page` show on Main.
    pub(crate) fn is_open(&self, page: Id) -> bool {
        self.all || self.pages.contains(&page)
    }

    /// Open `page` when it is closed, close it when it is open.
    pub(crate) fn toggle(&mut self, page: Id) {
        if !self.pages.remove(&page) {
            self.pages.insert(page);
        }
    }

    /// A number that changes whenever the choice changes, for the view cache key.
    pub(crate) fn stamp(&self) -> u64 {
        self.pages.iter().fold(u64::from(self.all), |h, page| {
            h.wrapping_mul(31).wrapping_add(*page).wrapping_add(1)
        })
    }
}

/// One non-Main page as Main shows it: where it sits and whether it is open.
///
/// Why: the toggles on the canvas need the spot of each page part, open or closed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PagePart {
    /// The page this part stands for.
    pub(crate) page: Id,
    /// The page name, shown on the part.
    pub(crate) name: String,
    /// Top left corner of the page's place on the Main sheet, in millimetres.
    pub(crate) origin: Point,
    /// True when the page's items show on Main instead of the part.
    pub(crate) open: bool,
}

/// True when `id` names a page part of Main's view copy and not an object of the design.
pub(crate) fn is_page_part(id: Id) -> bool {
    id >= PART_BASE
}

/// The net a pin of a page part carries, or None for any other pin.
///
/// Why: the pin number of a page part is the id of the net it stands for.
pub(crate) fn part_pin_net(pin: &PinRef) -> Option<Id> {
    if is_page_part(pin.component) {
        pin.pin.parse().ok()
    } else {
        None
    }
}

/// Height of a page part with `pins` pins, in millimetres.
pub(crate) fn part_height(pins: usize) -> f64 {
    HEADER + PITCH * (pins.div_ceil(2).max(1) as f64) + 2.0
}

/// The nets of `page` that leave it: those with a port, a power label or a global label on it.
///
/// Why: a page part has one pin for each public net. The document has no private flag yet, so
/// the nets that already cross a page boundary are the public ones.
///
/// # Rules
/// - Sorted by net name, then id; each net once.
fn public_nets(project: &Project, page: Id) -> Vec<Id> {
    let mut nets: Vec<Id> = project
        .ports
        .iter()
        .filter(|p| p.sheet == page)
        .map(|p| p.net)
        .collect();
    nets.extend(
        project
            .labels
            .iter()
            .filter(|l| matches!(l.kind.as_str(), "power" | "global"))
            .filter(|l| project.schematic_label_page(l) == page)
            .map(|l| l.net),
    );
    nets.sort_by(|a, b| {
        project
            .net_name(*a)
            .to_lowercase()
            .cmp(&project.net_name(*b).to_lowercase())
            .then(a.cmp(b))
    });
    nets.dedup();
    nets
}

/// The part that stands for `page`, with its top left corner at `origin`.
fn page_part_component(project: &Project, page: Id, name: &str, origin: Point) -> Component {
    let nets = public_nets(project, page);
    let height = part_height(nets.len());
    let pins = nets
        .iter()
        .enumerate()
        .map(|(i, net)| {
            let left = i % 2 == 0;
            let row = (i / 2) as f64;
            Pin {
                number: net.to_string(),
                name: project.net_name(*net).to_owned(),
                offset: Point::new(
                    if left { -PART_W / 2.0 } else { PART_W / 2.0 },
                    -height / 2.0 + HEADER + PITCH * (row + 0.5),
                ),
                net: Some(*net),
                ..Pin::default()
            }
        })
        .collect();
    Component {
        id: PART_BASE + page,
        reference: name.to_owned(),
        kind: "page".into(),
        position: Point::new(origin.x + PART_W / 2.0, origin.y + height / 2.0),
        pins,
        sheet: 0,
        ..Component::default()
    }
}

/// True when page `sheet` is drawn as a box on its parent, which keeps that box on Main.
fn has_sheet_box(project: &Project, page: Id) -> bool {
    project
        .sheets
        .iter()
        .any(|s| s.id == page && s.parent.is_some() && s.entry.is_some())
}

/// Move every schematic item to its place on Main, then fold each closed page into one part.
///
/// Why: Main is the top level of the design. A page that is not opened shows as one part named
/// after the page with one pin for each public net, and its items leave the view, so they
/// cannot be painted, picked or snapped to there.
///
/// # Rules
/// - Items of an open page stay as `apply_main_layout` placed them.
/// - A page that owns nothing, and a page drawn as a box on its parent, are not folded.
/// - Returns one entry per page that owns items, in tab order, for the canvas toggles.
///
/// # Dependencies
/// - crates/ui-egui/src/page_scope.rs (`apply_main_layout`, `main_instances`, `page_extent`)
///
/// # Do not
/// - Save this copy or send it to the engine.
pub(crate) fn apply_main_view(project: &mut Project, open: &MainOpen) -> Vec<PagePart> {
    let placed = main_instances(project);
    let mut parts: Vec<PagePart> = Vec::new();
    let mut folded: Vec<Component> = Vec::new();
    let mut closed: Vec<Id> = Vec::new();
    for instance in placed.iter().filter(|i| i.page != 0) {
        if has_sheet_box(project, instance.page) {
            continue;
        }
        let Some((lo, _)) = page_extent(project, instance.page) else {
            continue;
        };
        let origin = lo.offset(instance.offset);
        let name = project
            .sheets
            .iter()
            .find(|s| s.id == instance.page)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let is_open = open.is_open(instance.page);
        if !is_open {
            closed.push(instance.page);
            folded.push(page_part_component(project, instance.page, &name, origin));
        }
        parts.push(PagePart {
            page: instance.page,
            name,
            origin,
            open: is_open,
        });
    }
    if closed.is_empty() {
        apply_main_layout(project);
        return parts;
    }
    let label_gone: Vec<bool> = project
        .labels
        .iter()
        .map(|l| closed.contains(&project.schematic_label_page(l)))
        .collect();
    let wire_gone: Vec<bool> = project
        .drawn_wires
        .iter()
        .map(|w| closed.contains(&project.schematic_drawn_wire_page(w)))
        .collect();
    apply_main_layout(project);
    project.components.retain(|c| !closed.contains(&c.sheet));
    project.ports.retain(|q| !closed.contains(&q.sheet));
    let mut gone = label_gone.into_iter();
    project.labels.retain(|_| !gone.next().unwrap_or(false));
    let mut gone = wire_gone.into_iter();
    project.drawn_wires.retain(|_| !gone.next().unwrap_or(false));
    let kept: HashSet<Id> = project.components.iter().map(|c| c.id).collect();
    project
        .wires
        .retain(|w| kept.contains(&w.from.component) && kept.contains(&w.to.component));
    project
        .no_connects
        .retain(|f| kept.contains(&f.pin.component));
    project.components.extend(folded);
    parts
}

impl App {
    /// Paint every page part of the shown sheet: a framed body, the page name and one pin for
    /// each public net.
    ///
    /// # Rules
    /// - Reads only components that `is_page_part` names; the rest is painted by `symbol`.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/canvas.rs (`paint_schematic` calls it)
    ///
    /// # Do not
    /// - Paint a selection outline here; a page part cannot be selected.
    pub(crate) fn paint_page_parts(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        p: &Project,
    ) {
        let sc = SheetColours::for_theme(self.colours().dark);
        for c in p.components.iter().filter(|c| is_page_part(c.id)) {
            let height = part_height(c.pins.len());
            let top_left = camera.screen(
                Point::new(c.position.x - PART_W / 2.0, c.position.y - height / 2.0),
                rect,
            );
            let bottom_right = camera.screen(
                Point::new(c.position.x + PART_W / 2.0, c.position.y + height / 2.0),
                rect,
            );
            let body = egui::Rect::from_min_max(top_left, bottom_right);
            painter.rect_filled(body, 0.0, sc.sheet);
            painter.rect_stroke(
                body,
                0.0,
                Stroke::new(2.2, sc.body),
                egui::StrokeKind::Inside,
            );
            painter.text(
                body.left_top() + egui::vec2(4.0, 3.0),
                Align2::LEFT_TOP,
                &c.reference,
                role_font(TextRole::Reference, camera.zoom),
                sc.text,
            );
            for pin in &c.pins {
                let at = camera.screen(c.position.offset(pin.offset), rect);
                painter.circle_filled(at, 3.5, sc.sheet);
                painter.circle_stroke(at, 3.5, Stroke::new(1.6, sc.pin));
                let left = pin.offset.x < 0.0;
                painter.text(
                    at + egui::vec2(if left { 7.0 } else { -7.0 }, 0.0),
                    if left {
                        Align2::LEFT_CENTER
                    } else {
                        Align2::RIGHT_CENTER
                    },
                    &pin.name,
                    role_font(TextRole::PinName, camera.zoom),
                    sc.muted,
                );
            }
        }
    }

    /// The toggle on each page of Main: it opens a closed page part and closes an open page.
    ///
    /// Why: a person opens one page up without opening the rest. The tool bar toggle opens all
    /// of them, and while it is on the single toggles are not drawn.
    ///
    /// # Rules
    /// - Every toggle has an accessible name that includes the page name and a tooltip.
    /// - A toggle that is off screen is not registered.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/canvas.rs (`canvas` calls it after painting the sheet)
    ///
    /// # Do not
    /// - Run it off the Main page.
    pub(crate) fn page_part_controls(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        camera: Camera,
        parts: &[PagePart],
    ) {
        if self.main_open.all {
            return;
        }
        let sc = SheetColours::for_theme(self.colours().dark);
        let mut toggled = None;
        for part in parts {
            let anchor = if part.open {
                camera.screen(part.origin, rect) + egui::vec2(2.0, 2.0)
            } else {
                camera.screen(Point::new(part.origin.x + PART_W, part.origin.y), rect)
                    + egui::vec2(-(TOGGLE + 4.0), 3.0)
            };
            let spot = egui::Rect::from_min_size(anchor, Vec2::splat(TOGGLE));
            if !rect.intersects(spot) {
                continue;
            }
            let (name, tip) = if part.open {
                (
                    t!("schematic.page_part.close", page = part.name.clone()),
                    t!("schematic.page_part.close.tip"),
                )
            } else {
                (
                    t!("schematic.page_part.open", page = part.name.clone()),
                    t!("schematic.page_part.open.tip"),
                )
            };
            let id = ui.id().with(("page-part-toggle", part.page));
            let response = ui.interact(spot, id, egui::Sense::click_and_drag());
            crate::a11y::name_control(&response, &name, Some(part.open));
            let ink = if response.hovered() {
                sc.select
            } else {
                sc.body
            };
            let painter = ui.painter();
            painter.rect_filled(spot, 2.0, sc.sheet);
            painter.rect_stroke(spot, 2.0, Stroke::new(1.6, ink), egui::StrokeKind::Inside);
            let c = spot.center();
            painter.line_segment(
                [c + egui::vec2(-5.0, 0.0), c + egui::vec2(5.0, 0.0)],
                Stroke::new(1.8, ink),
            );
            if !part.open {
                painter.line_segment(
                    [c + egui::vec2(0.0, -5.0), c + egui::vec2(0.0, 5.0)],
                    Stroke::new(1.8, ink),
                );
            }
            if response.on_hover_text(tip).clicked() {
                toggled = Some(part.page);
            }
        }
        if let Some(page) = toggled {
            self.main_open.toggle(page);
        }
    }

    /// The tool bar toggle that opens every page on Main, drawn at the right of the bar.
    ///
    /// # Rules
    /// - Only on Main while the design has pages; the caller checks `main_is_grouping`.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/toolbar.rs (`toolbar_row` calls it)
    ///
    /// # Do not
    /// - Draw it on a page other than Main.
    pub(crate) fn toolbar_main_open(&mut self, ui: &mut egui::Ui, t: &crate::theme::Tokens) {
        use crate::ui_kit::checkbox::checkbox;
        let name = t!("schematic.main.open_all");
        let tip = t!("schematic.main.open_all.tip");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add_space(16.0);
            ui.label(crate::fonts::text(crate::fonts::Role::Body, name.clone()).color(t.text))
                .on_hover_text(&tip);
            checkbox(ui, &mut self.main_open.all, &name, &tip);
        });
    }

    /// Join the net of a page part pin to the part pin at the other end of a wire.
    ///
    /// Why: a wire from a pin of Main to a pin of a page part means "this pin is on that net".
    /// The page part is not in the design, so the wire is placed with the net named, the same
    /// way a wire to a power symbol is.
    ///
    /// # Rules
    /// - Returns false when neither end is a page part pin, so the caller carries on as before.
    /// - The other end must be a part of Main; a page part to a page part wire is refused.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/wire_canvas.rs (`finish_wire_to_pin`, `try_wire_place`)
    ///
    /// # Do not
    /// - Send the page part's id to the engine.
    pub(crate) fn finish_wire_with_page_part(
        &mut self,
        start: &PinRef,
        end: &PinRef,
        end_pos: Point,
    ) -> bool {
        let (real, net, part_is_start) = match (part_pin_net(start), part_pin_net(end)) {
            (None, None) => return false,
            (Some(net), None) => (end, net, true),
            (None, Some(net)) => (start, net, false),
            (Some(_), Some(_)) => {
                self.message = t!("schematic.wire.other_page");
                self.clear_wire_draft();
                return true;
            }
        };
        let on_main = self
            .project
            .component(real.component)
            .is_some_and(|c| c.sheet == 0);
        if !on_main {
            self.message = t!("schematic.main.read_only");
            self.clear_wire_draft();
            return true;
        }
        let mut points = self.wire_route.clone();
        if points.is_empty()
            && let Some(at) = self.sheet_view().pin_position(start)
        {
            points.push(at);
        }
        if points.last().copied() != Some(end_pos) {
            points.push(end_pos);
        }
        if part_is_start {
            points.reverse();
        }
        let params = json!({
            "points": points,
            "pins": [{"component": real.component, "pin": real.pin}],
            "net": net
        });
        let _ = self.try_wire_place(params);
        true
    }
}
