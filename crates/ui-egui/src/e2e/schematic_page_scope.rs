//! Stories for page isolation on the Schematic tab: every page is its own
//! schematic, and only the Main page shows all of them on one sheet.
//!
//! Fixture: the showcase board, which has three pages (Main, Compute and
//! Sensors and indicators) with distinct parts on each.

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
    // These stories are about the items of every page on Main, so every page is opened up.
    app.main_open.all = true;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((ctx, app))
}

fn page_id(app: &App, name: &str) -> Result<Id, String> {
    app.project
        .sheets
        .iter()
        .find(|s| s.name == name)
        .map(|s| s.id)
        .ok_or_else(|| format!("the design has no page {name}"))
}

fn refs_on(app: &App, page: Id) -> Vec<String> {
    app.project
        .components
        .iter()
        .filter(|c| c.sheet == page)
        .map(|c| c.reference.clone())
        .collect()
}

/// Text the schematic canvas itself paints on the current page.
fn canvas_text(app: &App) -> Vec<String> {
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1440.0, 940.0),
            )),
            ..Default::default()
        },
        |root_ui| {
            egui::CentralPanel::default().show(root_ui, |ui| {
                let rect = ui.max_rect();
                app.paint_schematic(ui.painter(), rect, app.schematic, &app.project);
            });
        },
    );
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn paints(texts: &[String], reference: &str) -> bool {
    texts
        .iter()
        .any(|t| t.split_whitespace().any(|w| w == reference))
}

/// The part of `page` that sits farthest from every part of any other page,
/// so a click on it cannot land on a neighbour.
fn loneliest(app: &App, page: Id) -> Result<(Id, Point), String> {
    let others: Vec<Point> = app
        .project
        .components
        .iter()
        .filter(|c| c.sheet != page)
        .map(|c| c.position)
        .collect();
    app.project
        .components
        .iter()
        .filter(|c| c.sheet == page)
        .map(|c| {
            let near = others
                .iter()
                .map(|o| o.distance(c.position))
                .fold(f64::INFINITY, f64::min);
            (near, c.id, c.position)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id, at)| (id, at))
        .ok_or_else(|| "the page has no parts".to_string())
}

/// Centre the view on `at` and click there with the real pointer.
fn click_world(ctx: &egui::Context, app: &mut App, at: Point) -> Result<(), String> {
    // Zoom in to working size: the whole circuit is too small to click one part.
    app.schematic.zoom = 8.0;
    app.schematic.center = at;
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let screen = app.schematic.screen(at, app.canvas_rect);
    if !app.canvas_rect.contains(screen) {
        return Err("the part is not inside the drawing area".into());
    }
    tap_at(ctx, app, screen)
}

#[test]
fn a_newcomer_sees_only_their_own_parts_on_page_two() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let page = page_id(&app, "Sensors and indicators")?;
    authoring::tests::tap(&ctx, &mut app, "Sensors and indicators")?;
    assert_eq!(app.schematic_page(), page, "the tab opens page two");
    let texts = canvas_text(&app);
    let own = refs_on(&app, page);
    assert!(
        own.iter().any(|r| paints(&texts, r)),
        "page two paints its own parts: {texts:?}"
    );
    for other in [0, page_id(&app, "Compute")?] {
        for r in refs_on(&app, other) {
            assert!(
                !paints(&texts, &r),
                "page two paints {r} of another page: {texts:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_newcomer_cannot_select_another_pages_parts() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let (part, at) = loneliest(&app, 0)?;
    click_world(&ctx, &mut app, at)?;
    assert_eq!(
        app.selected,
        Some(part),
        "on Main the click selects the part"
    );
    authoring::tests::tap(&ctx, &mut app, "Sensors and indicators")?;
    assert_eq!(
        app.selected, None,
        "a page change starts with nothing selected"
    );
    click_world(&ctx, &mut app, at)?;
    assert_eq!(
        app.selected, None,
        "clicking where a Main part sits selects nothing on page two"
    );
    let world = at;
    assert!(
        matches!(
            app.hit_at(world, app.schematic.zoom),
            canvas::gestures::Hit::Empty
        ),
        "the pointer finds nothing there on page two"
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_every_page_on_the_main_page() -> Result<(), String> {
    let (_ctx, app) = showcase()?;
    assert_eq!(app.schematic_page(), 0, "the schematic opens on Main");
    let texts = canvas_text(&app);
    for page in [
        0,
        page_id(&app, "Compute")?,
        page_id(&app, "Sensors and indicators")?,
    ] {
        assert!(
            refs_on(&app, page).iter().any(|r| paints(&texts, r)),
            "Main paints parts of page {page}: {texts:?}"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_sees_the_whole_circuit_as_one_sheet_on_main() -> Result<(), String> {
    let (_ctx, app) = showcase()?;
    let texts = canvas_text(&app);
    let title_blocks = texts
        .iter()
        .filter(|t| **t == t!("schematic.sheet.title").to_uppercase())
        .count();
    assert_eq!(
        title_blocks, 1,
        "Main paints one frame and one title block, not one per page: {texts:?}"
    );
    let named_pages = app
        .project
        .sheets
        .iter()
        .filter(|s| texts.contains(&s.name))
        .count();
    assert_eq!(named_pages, 0, "no page panel is labelled on Main");
    assert!(
        crate::page_scope::main_instances(&app.project).len() == app.project.sheets.len() + 1,
        "Main is built from one instance per page"
    );
    Ok(())
}

#[test]
fn a_newcomer_selects_a_part_from_another_page_on_main() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let page = page_id(&app, "Sensors and indicators")?;
    let (part, _) = loneliest(&app, page)?;
    let at = app
        .sheet_view()
        .component(part)
        .ok_or("placed part missing")?
        .position;
    click_world(&ctx, &mut app, at)?;
    assert_eq!(app.schematic_page(), 0, "the view stays on Main");
    assert_eq!(
        app.selected,
        Some(part),
        "Main selects the part of page two"
    );
    let reference = app
        .project
        .component(part)
        .map(|c| c.reference.clone())
        .ok_or("part missing")?;
    assert!(
        paints_text(&ctx, &mut app, &reference)?,
        "Properties show {reference}"
    );
    let value_before = app
        .project
        .component(part)
        .ok_or("part missing")?
        .value
        .clone();
    type_into(&ctx, &mut app, "textinput:Value", "SEL-TEST")?;
    let c = app.project.component(part).ok_or("part missing")?;
    assert_eq!(
        c.value, value_before,
        "Main does not change a part that belongs to another page"
    );
    assert_eq!(c.sheet, page, "the part still belongs to its own page");
    assert_eq!(
        app.message,
        t!("schematic.main.read_only"),
        "Main says where the part can be changed"
    );
    Ok(())
}

/// The box around every part of each page, as Main places them.
fn placed_boxes(app: &App) -> Vec<(Id, Id, (Point, Point))> {
    app.project
        .components
        .iter()
        .filter_map(|c| app.bounds_of(c.id).map(|b| (c.sheet, c.id, b)))
        .collect()
}

fn touch(a: (Point, Point), b: (Point, Point)) -> bool {
    a.0.x <= b.1.x && a.1.x >= b.0.x && a.0.y <= b.1.y && a.1.y >= b.0.y
}

#[test]
fn a_newcomer_sees_no_overlap_between_pages_on_main() -> Result<(), String> {
    let (_ctx, app) = showcase()?;
    let boxes = placed_boxes(&app);
    assert_eq!(
        boxes.len(),
        app.project.components.len(),
        "every part has a box"
    );
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            if a.0 != b.0 {
                assert!(
                    !touch(a.2, b.2),
                    "part {} of page {} touches part {} of page {} on Main",
                    a.1,
                    a.0,
                    b.1,
                    b.0
                );
            }
        }
    }
    let instances = crate::page_scope::main_instances(&app.project);
    assert!(
        instances.iter().skip(1).all(|i| i.offset.x > 0.0),
        "later pages sit to the right of Main: {instances:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_selects_the_right_part_on_main_after_placement() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let page = page_id(&app, "Sensors and indicators")?;
    // The page-two part whose own coordinates sit closest to a part of another page.
    let mut best: Option<(f64, Id)> = None;
    for c in app.project.components.iter().filter(|c| c.sheet == page) {
        for o in app.project.components.iter().filter(|o| o.sheet != page) {
            let d = o.position.distance(c.position);
            if best.is_none_or(|(k, _)| d < k) {
                best = Some((d, c.id));
            }
        }
    }
    let (_, part) = best.ok_or("no parts to compare")?;
    let own = app.project.component(part).ok_or("part missing")?.position;
    assert!(
        app.project
            .components
            .iter()
            .any(|o| o.sheet != page && o.position.distance(own) < 12.0),
        "a part of another page sits near the same page coordinates"
    );
    let placed = {
        let sheet = app.sheet_view();
        sheet.component(part).ok_or("placed part missing")?.position
    };
    assert!(placed.x > own.x + 10.0, "Main moved the part to the right");
    click_world(&ctx, &mut app, placed)?;
    assert_eq!(
        app.selected,
        Some(part),
        "the click selects the part of page two"
    );
    assert_eq!(app.schematic_page(), 0, "the view stays on Main");
    Ok(())
}

#[test]
fn a_newcomer_cannot_drag_another_pages_part_on_main() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let page = page_id(&app, "Sensors and indicators")?;
    let part = {
        // A part whose middle is not covered by a pin or a label.
        let ids: Vec<Id> = app
            .project
            .components
            .iter()
            .filter(|c| c.sheet == page)
            .map(|c| c.id)
            .collect();
        let mut found = None;
        for id in ids {
            let (lo, hi) = app.bounds_of(id).ok_or("part has no box")?;
            let mid = Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
            if matches!(app.hit_at(mid, 3.0), canvas::gestures::Hit::Part(h) if h == id) {
                found = Some(id);
                break;
            }
        }
        found.ok_or("no part can be pressed in its middle")?
    };
    let before = app.project.component(part).ok_or("part missing")?.position;
    let placed = {
        let sheet = app.sheet_view();
        sheet.component(part).ok_or("placed part missing")?.position
    };
    app.schematic.zoom = 8.0;
    app.schematic.center = placed;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let from = super::modifiers::centre_of(&app, part)?;
    let world = app.schematic.world(from, app.canvas_rect);
    assert!(
        matches!(
            app.hit_at(world, app.schematic.zoom),
            canvas::gestures::Hit::Part(_)
        ),
        "the press lands on the part"
    );
    let to = from + Vec2::new(5.0 * app.schematic.zoom, 0.0);
    super::modifiers::drag(&ctx, &mut app, from, to, egui::Modifiers::NONE);
    let after = app.project.component(part).ok_or("part missing")?.position;
    assert_eq!(
        after, before,
        "Main does not move a part that belongs to another page"
    );
    assert_eq!(
        app.project.component(part).map(|c| c.sheet),
        Some(page),
        "the part keeps its own page"
    );
    assert_eq!(
        app.message,
        t!("schematic.main.read_only"),
        "Main says where the part can be moved"
    );
    Ok(())
}

#[test]
fn a_newcomer_adds_a_part_to_main_and_it_stays_on_main() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    assert_eq!(app.schematic_page(), 0, "the view is Main");
    let known = app.project.components.len();
    // Aim the click over another page's area: the part still belongs to Main.
    let page = page_id(&app, "Sensors and indicators")?;
    let (other, _) = loneliest(&app, page)?;
    let over = app
        .sheet_view()
        .component(other)
        .ok_or("placed part missing")?
        .position;
    app.tool = Tool::Place("resistor".into());
    click_world(&ctx, &mut app, Point::new(over.x + 40.0, over.y + 40.0))?;
    assert_eq!(
        app.project.components.len(),
        known + 1,
        "Main takes a part of its own"
    );
    let added = app.project.components.last().ok_or("nothing was added")?;
    assert_eq!(
        added.sheet, 0,
        "the part belongs to Main, not to the page under it"
    );
    let id = added.id;
    let before = added.position;
    app.tool = Tool::Select;
    app.selected = Some(id);
    app.run(
        "component.move",
        json!({"id": id, "position": Point::new(before.x + 10.0, before.y)}),
    );
    assert_ne!(
        app.project.component(id).map(|c| c.position),
        Some(before),
        "a part of Main can be moved on Main"
    );
    Ok(())
}

/// Heights in points of every text on the sheet whose words read `label`, painted at `zoom`.
fn text_heights(app: &mut App, label: &str, zoom: f32) -> Vec<f32> {
    app.schematic.zoom = zoom;
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1440.0, 940.0),
            )),
            ..Default::default()
        },
        |root_ui| {
            egui::CentralPanel::default().show(root_ui, |ui| {
                let rect = ui.max_rect();
                app.paint_schematic(ui.painter(), rect, app.schematic, &app.project);
            });
        },
    );
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => Some(text.galley.size().y),
            _ => None,
        })
        .collect()
}

#[test]
fn a_newcomer_zooms_out_and_the_power_symbol_names_shrink_with_the_drawing() -> Result<(), String> {
    let (_ctx, mut app) = showcase()?;
    for label in ["GND", "3V3", "VBAT"] {
        let far = text_heights(&mut app, label, 3.0);
        assert!(!far.is_empty(), "{label} is painted on the sheet");
        let tallest = far.iter().copied().fold(0.0_f32, f32::max);
        assert!(
            tallest < 10.0,
            "every {label} is small when zoomed far out, none stays at a fixed size; sizes over 10: {:?}",
            far.iter()
                .copied()
                .filter(|h| *h >= 10.0)
                .collect::<Vec<_>>()
        );
        let near = text_heights(&mut app, label, 14.0);
        let near_tallest = near.iter().copied().fold(0.0_f32, f32::max);
        assert!(
            near_tallest > tallest * 1.8,
            "{label} is larger when zoomed in: {near_tallest} against {tallest}"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_works_on_main_and_a_frame_does_not_stall() -> Result<(), String> {
    // Main lays out every page. A frame there once took seconds, because each item asked for the
    // whole layout again. The limits are far above a healthy run and far below the broken one.
    let (ctx, mut app) = showcase()?;
    assert_eq!(app.schematic_page(), 0, "the view is Main");
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let start = pcbcraft_clock::instant_now();
    for _ in 0..10 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let frame_ms = start.elapsed().as_secs_f64() * 100.0;
    assert!(
        frame_ms < 1000.0,
        "a frame on Main takes {frame_ms:.0} ms; it should take a few tens"
    );
    let start = pcbcraft_clock::instant_now();
    for _ in 0..10 {
        std::hint::black_box(app.hit_at(Point::new(100.0, 100.0), 1.0));
    }
    let hit_ms = start.elapsed().as_secs_f64() * 100.0;
    assert!(
        hit_ms < 250.0,
        "a hit test on Main takes {hit_ms:.0} ms; it should take well under one"
    );
    Ok(())
}

/// Drag the primary button from `from` to `to` over the canvas, with `space` held or not.
fn drag_on_canvas(
    ctx: &egui::Context,
    app: &mut App,
    from: egui::Pos2,
    to: egui::Pos2,
    space: bool,
) {
    let key = |pressed: bool| egui::Event::Key {
        key: egui::Key::Space,
        physical_key: Some(egui::Key::Space),
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let button = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    if space {
        frame(ctx, app, vec![key(true)], egui::Modifiers::NONE);
    }
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(from)],
        egui::Modifiers::NONE,
    );
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(from), button(from, true)],
        egui::Modifiers::NONE,
    );
    for step in 1..=6 {
        let at = from + (to - from) * (step as f32 / 6.0);
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(at)],
            egui::Modifiers::NONE,
        );
    }
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(to), button(to, false)],
        egui::Modifiers::NONE,
    );
    if space {
        frame(ctx, app, vec![key(false)], egui::Modifiers::NONE);
    }
    frame(ctx, app, vec![], egui::Modifiers::NONE);
}

#[test]
fn a_newcomer_holds_space_and_drags_to_pan_the_view() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    for _ in 0..5 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let from = app.canvas_rect.center();
    let to = from + Vec2::new(120.0, 60.0);
    let parts = app.project.components.len();
    let first = app.project.components.first().map(|c| c.position);
    let before = app.schematic.center;

    drag_on_canvas(&ctx, &mut app, from, to, true);
    let after = app.schematic.center;
    assert!(
        (after.x - before.x).abs() > 1.0 || (after.y - before.y).abs() > 1.0,
        "dragging with Space held moves the view: {before:?} to {after:?}"
    );
    assert_eq!(app.project.components.len(), parts, "the pan adds no part");
    assert_eq!(
        app.project.components.first().map(|c| c.position),
        first,
        "the pan moves no part"
    );

    // The same drag without Space is not a pan.
    let before = app.schematic.center;
    drag_on_canvas(&ctx, &mut app, from, to, false);
    let after = app.schematic.center;
    assert!(
        (after.x - before.x).abs() < 0.01 && (after.y - before.y).abs() < 0.01,
        "a plain drag leaves the view where it is: {before:?} to {after:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_fits_the_whole_circuit_on_main() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    // Push the view away so the Fit control shows.
    app.schematic.center = Point::new(900.0, 900.0);
    app.schematic.zoom *= 2.0;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_id(&ctx, &mut app, "icon:canvas-fit")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let sheet = app.sheet_view();
    for c in &sheet.components {
        let at = app.schematic.screen(c.position, app.canvas_rect);
        assert!(
            app.canvas_rect.contains(at),
            "part {} of page {} is inside the window after Fit",
            c.id,
            c.sheet
        );
    }
    Ok(())
}
