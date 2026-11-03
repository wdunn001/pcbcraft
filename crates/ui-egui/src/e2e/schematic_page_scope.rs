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
    let (part, at) = loneliest(&app, page)?;
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
    type_into(&ctx, &mut app, "textinput:Value", "SEL-TEST")?;
    let c = app.project.component(part).ok_or("part missing")?;
    assert_eq!(c.value, "SEL-TEST", "the edit reached the part");
    assert_eq!(c.sheet, page, "the part still belongs to its own page");
    assert_eq!(
        app.project
            .components
            .iter()
            .filter(|c| c.value == "SEL-TEST")
            .count(),
        1,
        "only that part changed"
    );
    Ok(())
}
