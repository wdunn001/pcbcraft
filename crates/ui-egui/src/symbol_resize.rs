//! Drag a corner of a selected symbol to resize its body.
//!
//! The canvas paints a 9 point square on each corner of the selection outline
//! of a schematic part (`paint_selection`). Dragging one of those squares
//! resizes the part's body: the drag is read here and applied through the
//! engine command `component.symbol_body_resize`, one undo step per drag.
//!
//! # Rules
//! - The squares are hit-tested on the corners of the very rectangle that is
//!   painted, `canvas::symbol_outline`, never on a box worked out here.
//! - The body is mirrored about the part's own origin, as the engine command
//!   does, so the corner follows the pointer and the opposite corner moves
//!   with it. Pins on the sides ride out with the edge they belong to.
//! - Only parts that draw a rectangle body from a library symbol can be
//!   resized; the built-in shapes have no body to stretch, and a drag on one
//!   of their squares does nothing.
//!
//! # Dependencies
//! - crates/ui-egui/src/canvas.rs (`symbol_outline` gives the corners)
//! - crates/ui-egui/src/canvas/sheet.rs (`paint_selection` draws the handles)
//! - crates/engine/src/authoring.rs (`component.symbol_body_resize` applies it)
//!
//! # Do not
//! - Resize anything but the part under the pointer. The handle belongs to
//!   the selected symbol, not to whatever the cursor later passes over.
use super::*;
use pcbcraft_doc::Component;
use pcbcraft_geom::Point;

/// How far from a handle's centre still grabs it, in points.
const GRAB_PX: f32 = 8.0;
/// Smallest body a drag may leave, in millimetres.
const FLOOR_MM: f64 = 1.27;

/// A symbol corner held by the pointer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SymbolGrab {
    /// The part being resized.
    id: Id,
    /// Where the pointer went down, in sheet coordinates.
    press: Point,
    /// Which corner of the body was grabbed, as plus or minus one on each of
    /// the part's own axes.
    sign: Point,
    /// The part's turn, in degrees, to bring a sheet offset onto its own axes.
    rotation: f64,
    /// Body width and height when the drag began, in millimetres.
    size: Point,
}

impl App {
    /// Half the body extent of the symbol a part draws, in millimetres.
    pub(crate) fn symbol_body_half(&self, id: Id) -> Option<Point> {
        let c = self.project.component(id)?;
        let symbol = c.library_symbol.as_ref()?;
        symbol
            .definition
            .graphics
            .iter()
            .filter_map(|g| match &g.primitive {
                pcbcraft_library::Graphic::Rectangle { start, end, .. } => Some(Point::new(
                    (end.x - start.x).abs() / 2.0,
                    (end.y - start.y).abs() / 2.0,
                )),
                _ => None,
            })
            .max_by(|a, b| (a.x * a.y).total_cmp(&(b.x * b.y)))
    }

    /// The selected part as the schematic draws it now, with the four screen
    /// corners of its selection outline.
    ///
    /// # Rules
    /// - Returns nothing for a part that cannot be resized.
    /// - Positions follow the sheet view, so on Main a part sits where Main
    ///   draws it, not where its own page does.
    fn symbol_handles(
        &self,
        id: Id,
        camera: Camera,
        rect: egui::Rect,
    ) -> Option<(Component, [egui::Pos2; 4])> {
        self.symbol_body_half(id)?;
        let sheet = self.sheet_view();
        let view = sheet
            .component(id)?
            .schematic_views()
            .into_iter()
            .find(|v| {
                crate::page_scope::in_view(&self.project, self.schematic_page(), v.sheet)
                    && v.library_symbol.as_ref().map(|s| s.unit).unwrap_or(1) == self.selected_unit
            })?;
        let (_, outline) = canvas::symbol_outline(&view, camera, rect);
        let corners = [
            outline.left_top(),
            outline.right_top(),
            outline.right_bottom(),
            outline.left_bottom(),
        ];
        Some((view, corners))
    }

    /// Which handle of the selected part is under `screen`, if any.
    fn symbol_handle_at(
        &self,
        screen: egui::Pos2,
        camera: Camera,
        rect: egui::Rect,
    ) -> Option<(Component, egui::Pos2)> {
        let id = self.selected?;
        let (view, corners) = self.symbol_handles(id, camera, rect)?;
        corners
            .into_iter()
            .find(|corner| corner.distance(screen) <= GRAB_PX)
            .map(|corner| (view, corner))
    }

    /// Start, follow and finish a drag on a symbol's corner handle.
    ///
    /// # Rules
    /// - Only on the schematic, only with the select tool, only on the part
    ///   that is already selected.
    /// - The corner follows the pointer. The body is mirrored about the
    ///   part's own origin, so the opposite corner moves with it.
    /// - A drag that would leave less than `FLOOR_MM` is clamped rather than
    ///   refused, so the symbol never collapses out of reach.
    pub(crate) fn symbol_resize_drag(
        &mut self,
        response: &egui::Response,
        cursor: Option<Point>,
        press: Option<Point>,
        camera: Camera,
        rect: egui::Rect,
    ) {
        if self.tab != Tab::Schematic || !matches!(self.tool, Tool::Select) {
            self.symbol_resize = None;
            return;
        }
        let Some(pos) = cursor else { return };
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(id) = self.selected
        {
            self.symbol_resize_press(id, press.unwrap_or(pos), camera, rect);
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.symbol_resize_release(pos);
        }
    }

    /// Show what a handle does while the pointer rests on one.
    pub(crate) fn symbol_resize_hint(
        &self,
        response: &egui::Response,
        camera: Camera,
        rect: egui::Rect,
    ) {
        if self.tab != Tab::Schematic || !matches!(self.tool, Tool::Select) {
            return;
        }
        let Some(hover) = response.hover_pos() else {
            return;
        };
        if let Some((view, corner)) = self.symbol_handle_at(hover, camera, rect) {
            let slant = (corner.x - camera.screen(view.position, rect).x)
                * (corner.y - camera.screen(view.position, rect).y);
            response.ctx.set_cursor_icon(if slant >= 0.0 {
                egui::CursorIcon::ResizeNwSe
            } else {
                egui::CursorIcon::ResizeNeSw
            });
            response
                .clone()
                .on_hover_text(t!("schematic.resize_corner.tip"));
        }
    }

    /// Grab the corner handle of `id` where the pointer went down at `at`, if
    /// that is where one is.
    ///
    /// # Rules
    /// - Shared with `symbol_resize_drag` so the test drives the same rule
    ///   the pointer does.
    pub(crate) fn symbol_resize_press(
        &mut self,
        id: Id,
        at: Point,
        camera: Camera,
        rect: egui::Rect,
    ) {
        let Some(half) = self.symbol_body_half(id) else {
            return;
        };
        let Some((view, corner)) = self.symbol_handles(id, camera, rect).and_then(|(v, c)| {
            let screen = camera.screen(at, rect);
            c.into_iter()
                .find(|k| k.distance(screen) <= GRAB_PX)
                .map(|k| (v, k))
        }) else {
            return;
        };
        let grabbed = camera.world(corner, rect);
        let local = Point::new(grabbed.x - view.position.x, grabbed.y - view.position.y)
            .rotate(view.rotation);
        let side = |v: f64| if v < 0.0 { -1.0 } else { 1.0 };
        self.symbol_resize = Some(SymbolGrab {
            id,
            press: at,
            sign: Point::new(side(local.x), side(local.y)),
            rotation: view.rotation,
            size: Point::new(half.x * 2.0, half.y * 2.0),
        });
    }

    /// Finish a grabbed drag with the pointer at `at`.
    pub(crate) fn symbol_resize_release(&mut self, at: Point) {
        let Some(grab) = self.symbol_resize.take() else {
            return;
        };
        let moved = Point::new(at.x - grab.press.x, at.y - grab.press.y).rotate(grab.rotation);
        let width = (grab.size.x + 2.0 * grab.sign.x * moved.x).max(FLOOR_MM);
        let height = (grab.size.y + 2.0 * grab.sign.y * moved.y).max(FLOOR_MM);
        if (width - grab.size.x).abs() < 1e-9 && (height - grab.size.y).abs() < 1e-9 {
            return;
        }
        self.run(
            "component.symbol_body_resize",
            json!({"id": grab.id, "width": width, "height": height}),
        );
    }

    /// Whether a symbol corner is being dragged right now.
    pub(crate) fn symbol_resize_active(&self) -> bool {
        self.symbol_resize.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;

    /// A part placed from a library symbol, selected, on the schematic.
    fn placed() -> Result<(App, Id), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/device.kicad_sym");
        app.tool_call(
            "component.place_library",
            json!({
                "path": fixture.to_string_lossy(),
                "symbol": "R",
                "position": {"x": 50.0, "y": 40.0}
            }),
        );
        let id = app
            .project
            .components
            .iter()
            .find(|c| c.library_symbol.is_some())
            .map(|c| c.id)
            .ok_or("a part placed from a library symbol")?;
        app.selected = Some(id);
        app.tab = Tab::Schematic;
        app.tool = Tool::Select;
        // A real screen box, so the camera maps sheet points to finite screen points.
        app.canvas_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0));
        app.schematic.center = Point::new(50.0, 40.0);
        app.schematic.zoom = 8.0;
        Ok((app, id))
    }

    #[test]
    fn a_corner_drag_resizes_the_body() -> Result<(), String> {
        let (mut app, id) = placed()?;
        let (camera, rect) = (app.schematic, app.canvas_rect);
        let before = app.symbol_body_half(id).ok_or("the part draws a body")?;
        // Grab the corner square that is painted, drop it further out.
        let (_, corners) = app
            .symbol_handles(id, camera, rect)
            .ok_or("the part shows handles")?;
        let grabbed = camera.world(corners[2], rect);
        let dropped = Point::new(grabbed.x + 10.0, grabbed.y + 5.0);
        app.symbol_resize = None;
        app.symbol_resize_press(id, grabbed, camera, rect);
        assert!(app.symbol_resize_active(), "the corner was grabbed");
        app.symbol_resize_release(dropped);
        let after = app
            .symbol_body_half(id)
            .ok_or("the part still draws a body")?;
        assert!(
            (after.x - before.x - 10.0).abs() < 1e-6 && (after.y - before.y - 5.0).abs() < 1e-6,
            "the corner followed the pointer, got {after:?} from {before:?}"
        );
        Ok(())
    }

    #[test]
    fn a_drag_away_from_a_corner_is_not_a_resize() -> Result<(), String> {
        let (mut app, id) = placed()?;
        let (camera, rect) = (app.schematic, app.canvas_rect);
        let centre = app
            .project
            .component(id)
            .map(|c| c.position)
            .ok_or("the part has a position")?;
        app.symbol_resize_press(
            id,
            Point::new(centre.x + 400.0, centre.y + 400.0),
            camera,
            rect,
        );
        assert!(
            !app.symbol_resize_active(),
            "a press in open space grabs no handle"
        );
        Ok(())
    }
}
