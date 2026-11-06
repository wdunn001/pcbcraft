//! Stories for the Add tool ghost (#TRACKER): while an Add tool is armed and
//! the pointer is over the canvas, a translucent copy of the item follows it.

use super::super::*;
use super::modifiers::sample;
use super::support::*;
use crate::canvas::snap;
use canvas::ghost::GhostKind;
use egui::{Event, Modifiers, Pos2};

fn hover(ctx: &egui::Context, app: &mut App, at: Pos2) -> egui::FullOutput {
    frame(ctx, app, vec![Event::PointerMoved(at)], Modifiers::NONE);
    frame(ctx, app, vec![Event::PointerMoved(at)], Modifiers::NONE)
}

/// Marks in the ghost colour that sit on the canvas this frame.
fn ghost_marks(app: &App, output: &egui::FullOutput) -> usize {
    fn solid(c: &egui::epaint::ColorMode) -> Option<egui::Color32> {
        match c {
            egui::epaint::ColorMode::Solid(c) => Some(*c),
            egui::epaint::ColorMode::UV(_) => None,
        }
    }
    fn walk(shape: &egui::Shape, tint: egui::Color32, canvas: Rect, n: &mut usize) {
        match shape {
            egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, tint, canvas, n)),
            egui::Shape::Path(p) => {
                if solid(&p.stroke.color) == Some(tint) || p.fill == tint {
                    *n += usize::from(p.points.first().is_some_and(|q| canvas.contains(*q)));
                }
            }
            egui::Shape::LineSegment { stroke, points } => {
                if stroke.color == tint && canvas.contains(points[0]) {
                    *n += 1;
                }
            }
            egui::Shape::Circle(c) => {
                if (c.stroke.color == tint || c.fill == tint) && canvas.contains(c.center) {
                    *n += 1;
                }
            }
            egui::Shape::Text(t) if t.fallback_color == tint && canvas.contains(t.pos) => {
                *n += 1;
            }
            _ => {}
        }
    }
    let mut n = 0;
    for clipped in &output.shapes {
        walk(&clipped.shape, app.ghost_tint(), app.canvas_rect, &mut n);
    }
    n
}

fn camera(app: &App) -> Camera {
    if app.tab == Tab::Pcb {
        app.pcb
    } else {
        app.schematic
    }
}

#[test]
fn a_newcomer_sees_every_schematic_add_tool_under_the_pointer() -> Result<(), String> {
    let cases = [
        ("icon:tool-label", GhostKind::Label),
        ("icon:tool-power", GhostKind::Power),
        ("icon:tool-bus", GhostKind::Bus),
        ("icon:tool-busentry", GhostKind::BusEntry),
        ("icon:tool-port", GhostKind::Port),
        ("icon:tool-noconnect", GhostKind::NoConnect),
        ("icon:tool-text", GhostKind::Text),
    ];
    for (id, kind) in cases {
        let (mut app, ctx) = sample(Tab::Schematic);
        tap_strip(&ctx, &mut app, id)?;
        let at = app.canvas_rect.center() + egui::vec2(60.0, 40.0);
        let out = hover(&ctx, &mut app, at);
        let ghost = app
            .ghost
            .ok_or(format!("{id}: no ghost under the pointer"))?;
        assert_eq!(ghost.kind, kind, "{id} shows its own item");
        assert!(ghost_marks(&app, &out) > 0, "{id}: the ghost is painted");
    }
    Ok(())
}

#[test]
fn a_newcomer_sees_a_shape_tool_under_the_pointer() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    tap_strip(&ctx, &mut app, "icon:tool-add")?;
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    crate::authoring::tests::tap(&ctx, &mut app, "Line")?;
    let at = app.canvas_rect.center();
    let out = hover(&ctx, &mut app, at);
    let ghost = app.ghost.ok_or("no ghost for the Line tool")?;
    assert_eq!(ghost.kind, GhostKind::Shape);
    assert!(ghost_marks(&app, &out) > 0, "the Line ghost is painted");
    Ok(())
}

#[test]
fn a_newcomer_sees_the_pad_via_and_hole_on_the_layout() -> Result<(), String> {
    for (id, kind) in [
        ("icon:tool-pad", GhostKind::Pad),
        ("icon:tool-via", GhostKind::Via),
        ("icon:tool-hole", GhostKind::Hole),
    ] {
        let (mut app, ctx) = sample(Tab::Pcb);
        tap_strip(&ctx, &mut app, id)?;
        let at = app.canvas_rect.center();
        hover(&ctx, &mut app, at);
        let ghost = app
            .ghost
            .ok_or(format!("{id}: no ghost under the pointer"))?;
        assert_eq!(ghost.kind, kind, "{id} shows its own item");
    }
    Ok(())
}

#[test]
fn a_newcomer_sees_a_library_part_painted_like_the_library_preview() -> Result<(), String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../library/tests/fixtures/device.kicad_sym")
        .to_string_lossy()
        .into_owned();
    let def = pcbcraft_engine::library::symbol(std::path::Path::new(&path), "R")?;
    let (mut app, ctx) = sample(Tab::Schematic);
    app.tool = Tool::LibraryPlace(path, "R".into(), Box::new(def.clone()));
    let at = app.canvas_rect.center();
    let out = hover(&ctx, &mut app, at);
    let ghost = app.ghost.ok_or("no ghost for the armed part")?;
    assert_eq!(ghost.kind, GhostKind::Part);
    let (_, _, paths) = app.ghost_paths.as_ref().ok_or("no outline paths")?;
    assert_eq!(
        *paths,
        crate::library::library_geometry_paths(Some(&def), None, 1),
        "the ghost outline comes from the function the Library preview uses"
    );
    assert!(ghost_marks(&app, &out) > 0, "the part outline is painted");
    let first = paths.as_ptr();
    hover(&ctx, &mut app, at + egui::vec2(30.0, 10.0));
    let (_, _, again) = app.ghost_paths.as_ref().ok_or("no outline paths")?;
    assert_eq!(again.as_ptr(), first, "moving the pointer rebuilds nothing");
    Ok(())
}

#[test]
fn a_newcomer_sees_the_ghost_follow_and_snap_to_the_grid() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    tap_strip(&ctx, &mut app, "icon:tool-noconnect")?;
    let first = app.canvas_rect.center() + egui::vec2(13.0, 7.0);
    hover(&ctx, &mut app, first);
    let a = app.ghost.ok_or("no ghost at the first spot")?;
    let want = snap(camera(&app).world(first, app.canvas_rect));
    assert_eq!(
        a.at, want,
        "the ghost sits on the grid spot a click would use"
    );
    let second = first + egui::vec2(120.0, 60.0);
    hover(&ctx, &mut app, second);
    let b = app.ghost.ok_or("no ghost at the second spot")?;
    assert_ne!(a.at, b.at, "the ghost follows the pointer");
    assert_eq!(b.at, snap(camera(&app).world(second, app.canvas_rect)));
    Ok(())
}

#[test]
fn a_newcomer_loses_the_ghost_on_esc_a_new_tool_or_leaving_the_canvas() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    tap_strip(&ctx, &mut app, "icon:tool-port")?;
    let at = app.canvas_rect.center();
    hover(&ctx, &mut app, at);
    assert!(app.ghost.is_some(), "the port ghost is there");
    let outside = Pos2::new(app.canvas_rect.left() - 20.0, at.y);
    hover(&ctx, &mut app, outside);
    assert!(app.ghost.is_none(), "leaving the canvas hides the ghost");
    hover(&ctx, &mut app, at);
    assert!(app.ghost.is_some(), "coming back shows it again");
    key(&ctx, &mut app, egui::Key::Escape);
    hover(&ctx, &mut app, at);
    assert!(matches!(app.tool, Tool::Select));
    assert!(app.ghost.is_none(), "Esc ends the tool and the ghost");
    tap_strip(&ctx, &mut app, "icon:tool-label")?;
    hover(&ctx, &mut app, at);
    assert_eq!(app.ghost.map(|g| g.kind), Some(GhostKind::Label));
    tap_strip(&ctx, &mut app, "icon:tool-select")?;
    hover(&ctx, &mut app, at);
    assert!(app.ghost.is_none(), "picking Select hides the ghost");
    Ok(())
}

#[test]
fn a_newcomer_hovering_with_an_add_tool_changes_nothing() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    tap_strip(&ctx, &mut app, "icon:tool-label")?;
    let project = app.project.clone();
    let (revision, undoable) = {
        let s = app.session.lock().map_err(|_| "session is busy")?;
        (s.revision, s.can_undo())
    };
    for step in 0..6 {
        let at = app.canvas_rect.center() + egui::vec2(25.0 * step as f32, 10.0);
        hover(&ctx, &mut app, at);
    }
    assert!(app.ghost.is_some());
    assert_eq!(app.project, project, "the document is unchanged");
    let s = app.session.lock().map_err(|_| "session is busy")?;
    assert_eq!(s.revision, revision, "no edit was made");
    assert_eq!(s.can_undo(), undoable, "the undo history is unchanged");
    Ok(())
}

/// The shortest wait any viewport asks for after a quiet frame.
fn quiet_wait(ctx: &egui::Context, app: &mut App) -> std::time::Duration {
    let mut asked = std::time::Duration::MAX;
    for _ in 0..3 {
        let out = frame(ctx, app, vec![], Modifiers::NONE);
        asked = out
            .viewport_output
            .values()
            .map(|v| v.repaint_delay)
            .min()
            .unwrap_or(std::time::Duration::MAX);
    }
    asked
}

#[test]
fn a_still_pointer_with_a_ghost_asks_for_no_more_repaints_than_without() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let at = app.canvas_rect.center();
    hover(&ctx, &mut app, at);
    let without = quiet_wait(&ctx, &mut app);
    tap_strip(&ctx, &mut app, "icon:tool-noconnect")?;
    hover(&ctx, &mut app, at);
    assert!(app.ghost.is_some());
    let with = quiet_wait(&ctx, &mut app);
    assert!(
        with >= without,
        "the ghost adds no repaint request: {with:?} against {without:?}"
    );
    Ok(())
}
