//! Drag a corner of a selected symbol to resize its body.
//!
//! The canvas paints a square handle on each corner of a selected symbol.
//! Corner handles are the standard shape for "drag me to resize", and for a
//! long time nothing read them, so the only thing they did was waste a
//! reader's time. This is what reads them.
//!
//! # Dependencies
//! - crates/ui-egui/src/canvas/sheet.rs (`paint_selection` draws the handles)
//! - crates/engine/src/authoring.rs (`component.symbol_body_resize` applies it)
//!
//! # Do not
//! - Resize anything but the part under the pointer. The handle belongs to
//!   the selected symbol, not to whatever the cursor later passes over.
use super::*;
use pcbcraft_geom::Point;

/// How close to a corner counts as grabbing it, in millimetres at zoom 1.
const GRAB_MM: f64 = 2.0;
/// Smallest body a drag may leave, in millimetres.
const FLOOR_MM: f64 = 1.27;

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
        zoom: f32,
    ) {
        if self.tab != Tab::Schematic || !matches!(self.tool, Tool::Select) {
            self.symbol_resize = None;
            return;
        }
        let Some(pos) = cursor else { return };
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(id) = self.selected
            && let Some(half) = self.symbol_body_half(id)
            && let Some(c) = self.project.component(id)
        {
            let grabbed = press.unwrap_or(pos);
            let _ = (half, c);
            self.symbol_resize_fake_press(id, grabbed, zoom);
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.symbol_resize_fake_release(pos);
        }
    }

    /// Grab the corner handle of `id` at `at`, if that is where it is.
    ///
    /// # Rules
    /// - Shared with `symbol_resize_drag` so the test drives the same rule
    ///   the pointer does.
    pub(crate) fn symbol_resize_fake_press(&mut self, id: Id, at: Point, zoom: f32) {
        let Some(half) = self.symbol_body_half(id) else {
            return;
        };
        let Some(c) = self.project.component(id) else {
            return;
        };
        let reach = (GRAB_MM / zoom.max(0.05) as f64).max(0.5);
        let corner_x = (at.x - c.position.x).abs();
        let corner_y = (at.y - c.position.y).abs();
        if (corner_x - half.x).abs() <= reach && (corner_y - half.y).abs() <= reach {
            self.symbol_resize = Some((id, half));
        }
    }

    /// Finish a grabbed drag at `at`.
    pub(crate) fn symbol_resize_fake_release(&mut self, at: Point) {
        let Some((id, _)) = self.symbol_resize.take() else {
            return;
        };
        let Some(c) = self.project.component(id) else {
            return;
        };
        let width = ((at.x - c.position.x).abs() * 2.0).max(FLOOR_MM);
        let height = ((at.y - c.position.y).abs() * 2.0).max(FLOOR_MM);
        self.run(
            "component.symbol_body_resize",
            json!({"id": id, "width": width, "height": height}),
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
        Ok((app, id))
    }

    #[test]
    fn a_corner_drag_resizes_the_body() -> Result<(), String> {
        let (mut app, id) = placed()?;
        let before = app.symbol_body_half(id).ok_or("the part draws a body")?;
        let centre = app
            .project
            .component(id)
            .map(|c| c.position)
            .ok_or("the part has a position")?;
        // Grab the corner the handle is painted on, drop it further out.
        let grabbed = Point::new(centre.x + before.x, centre.y + before.y);
        let dropped = Point::new(centre.x + 20.0, centre.y + 10.0);
        app.symbol_resize = None;
        app.symbol_resize_fake_press(id, grabbed, 1.0);
        assert!(app.symbol_resize_active(), "the corner was grabbed");
        app.symbol_resize_fake_release(dropped);
        let after = app
            .symbol_body_half(id)
            .ok_or("the part still draws a body")?;
        assert!(
            (after.x - 20.0).abs() < 1e-6 && (after.y - 10.0).abs() < 1e-6,
            "the corner followed the pointer, got {after:?} from {before:?}"
        );
        Ok(())
    }

    #[test]
    fn a_drag_away_from_a_corner_is_not_a_resize() -> Result<(), String> {
        let (mut app, id) = placed()?;
        let centre = app
            .project
            .component(id)
            .map(|c| c.position)
            .ok_or("the part has a position")?;
        app.symbol_resize_fake_press(id, Point::new(centre.x + 40.0, centre.y + 40.0), 1.0);
        assert!(
            !app.symbol_resize_active(),
            "a press in open space grabs no handle"
        );
        Ok(())
    }
}
