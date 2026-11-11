//! Stories for the selection outline of a schematic part: it wraps the whole
//! drawn part, carries a square handle on each corner, and dragging a corner
//! resizes the part.
//!
//! Fixture: the showcase board, one part of each kind it has.

use super::super::*;
use super::support::*;

fn showcase() -> Result<(egui::Context, App), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.home_open = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Showcase board")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Schematic")?;
    // Every page's parts sit on Main, so open them all up to click any of them.
    app.main_open.all = true;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((ctx, app))
}

/// One part per kind (a handful at most): the one standing farthest from every
/// other part, so a click on it cannot land on a neighbour. Connectors come first.
fn one_of_each_kind(app: &App) -> Vec<(String, Id, Point)> {
    let mut kinds: Vec<String> = app
        .project
        .components
        .iter()
        .map(|c| c.kind.clone())
        .collect();
    kinds.sort();
    kinds.dedup();
    kinds.sort_by_key(|k| k != "connector");
    kinds
        .into_iter()
        .take(5)
        .filter_map(|kind| {
            // Main places each page's parts at their own spot on the one sheet, so the
            // spot to click is where the sheet view puts the part, not the part's own position.
            let spot = |id: Id| app.sheet_view().component(id).map(|c| c.position);
            app.project
                .components
                .iter()
                .filter(|c| c.kind == kind)
                .filter_map(|c| {
                    let at = spot(c.id)?;
                    let near = app
                        .project
                        .components
                        .iter()
                        .filter(|o| o.id != c.id)
                        .filter_map(|o| spot(o.id))
                        .map(|o| o.distance(at))
                        .fold(f64::INFINITY, f64::min);
                    Some((near, c.id, at))
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, id, at)| (kind, id, at))
        })
        .collect()
}

#[test]
fn a_newcomer_sees_the_outline_wrap_each_part_and_show_corner_handles() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let parts = one_of_each_kind(&app);
    assert!(parts.len() >= 2, "the showcase has several kinds of part");
    for (kind, id, at) in parts {
        app.schematic.zoom = 8.0;
        app.schematic.center = at;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let screen = app.schematic.screen(at, app.canvas_rect);
        tap_at(&ctx, &mut app, screen)?;
        assert_eq!(app.selected, Some(id), "a click on the {kind} selects it");
        canvas::LAST_OUTLINE.with(|cell| cell.set(None));
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let (outline, drawn) = canvas::LAST_OUTLINE
            .with(|cell| cell.get())
            .ok_or_else(|| format!("the selected {kind} paints an outline"))?;
        // The outline holds the whole drawn part: symbol body and pins.
        assert!(
            outline.contains_rect(drawn),
            "the {kind} outline {outline:?} misses part of the drawn part {drawn:?}"
        );
        // And it stays close: no more than the small fixed margin beside the part.
        assert!(
            outline.left() >= drawn.left() - 10.0 && outline.right() <= drawn.right() + 10.0,
            "the {kind} outline {outline:?} reaches far beside the drawn part {drawn:?}"
        );
        // A resize handle: a 9 pixel square is painted on every outline corner.
        let corners = [
            outline.left_top(),
            outline.right_top(),
            outline.right_bottom(),
            outline.left_bottom(),
        ];
        for corner in corners {
            let handle = output.shapes.iter().any(|clipped| match &clipped.shape {
                egui::Shape::Rect(square) => {
                    (square.rect.width() - 9.0).abs() < 0.5
                        && (square.rect.height() - 9.0).abs() < 0.5
                        && square.rect.center().distance(corner) < 1.0
                }
                _ => false,
            });
            assert!(handle, "the {kind} outline has no handle on corner {corner:?}");
        }
    }
    Ok(())
}

/// The part to resize: one on Main's own page (a part of another page is
/// read-only there) that draws a body rectangle, and stands farthest from the
/// others so a click on it cannot land on a neighbour.
fn resizable_part(app: &App) -> Result<(Id, Point), String> {
    let spot = |id: Id| app.sheet_view().component(id).map(|c| c.position);
    app.project
        .components
        .iter()
        .filter(|c| c.sheet == 0 && app.symbol_body_half(c.id).is_some())
        .filter_map(|c| {
            let at = spot(c.id)?;
            let near = app
                .project
                .components
                .iter()
                .filter(|o| o.id != c.id)
                .filter_map(|o| spot(o.id))
                .map(|o| o.distance(at))
                .fold(f64::INFINITY, f64::min);
            Some((near, c.id, at))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id, at)| (id, at))
        .ok_or_else(|| "the showcase has a part with a resizable body".to_string())
}

fn session_revision(app: &App) -> Result<u64, String> {
    app.session
        .lock()
        .map(|s| s.revision)
        .map_err(|_| "session unavailable".to_string())
}

#[test]
fn a_newcomer_drags_a_corner_to_resize_a_part() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let (id, at) = resizable_part(&app)?;
    app.schematic.zoom = 8.0;
    app.schematic.center = at;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let screen = app.schematic.screen(at, app.canvas_rect);
    tap_at(&ctx, &mut app, screen)?;
    assert_eq!(app.selected, Some(id), "a click on the part selects it");
    canvas::LAST_OUTLINE.with(|cell| cell.set(None));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let (outline, _) = canvas::LAST_OUTLINE
        .with(|cell| cell.get())
        .ok_or("the selected part paints an outline")?;
    let before = app.symbol_body_half(id).ok_or("the part draws a body")?;
    let revision = session_revision(&app)?;

    // Take the bottom right square and pull it outward.
    let from = outline.right_bottom();
    let to = from + egui::vec2(24.0, 24.0);
    super::modifiers::drag(&ctx, &mut app, from, to, egui::Modifiers::NONE);

    let grown = app.symbol_body_half(id).ok_or("the part still draws a body")?;
    assert!(
        grown.x > before.x + 0.5 && grown.y > before.y + 0.5,
        "the body grew, got {grown:?} from {before:?}"
    );
    assert_eq!(
        session_revision(&app)?,
        revision + 1,
        "the whole drag is one undo step"
    );

    app.tool_call("undo", json!({}));
    let back = app.symbol_body_half(id).ok_or("the part still draws a body")?;
    assert!(
        (back.x - before.x).abs() < 1e-6 && (back.y - before.y).abs() < 1e-6,
        "undo restored the size, got {back:?} from {before:?}"
    );
    Ok(())
}
