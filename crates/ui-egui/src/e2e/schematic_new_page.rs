//! Stories for the first minute on a new schematic page of a big design (#1634, #1635):
//! adding a page and placing a part must answer within a frame and the part must land on
//! the page that is open.
//!
//! Fixture: the showcase board, which has three pages and many parts.

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

fn ms(start: &pcbcraft_clock::Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// The most one step may take, in milliseconds. The tests run unoptimised on busy machines.
/// A loaded CI runner reaches about 2.5 seconds. The freeze this guards against took 9 to 11
/// seconds even on an idle machine, so this budget still catches it.
const STEP_BUDGET_MS: f64 = 6000.0;

/// Draw `frames` frames and return the longest one, in milliseconds.
fn slowest_frame(ctx: &egui::Context, app: &mut App, frames: usize) -> f64 {
    (0..frames)
        .map(|_| {
            let start = pcbcraft_clock::instant_now();
            frame(ctx, app, vec![], egui::Modifiers::NONE);
            ms(&start)
        })
        .fold(0.0, f64::max)
}

#[test]
fn a_newcomer_adds_five_pages_to_a_big_design_and_the_window_keeps_answering() -> Result<(), String>
{
    let (ctx, mut app) = showcase()?;
    assert!(
        app.project.components.len() > 40,
        "the design holds many parts"
    );
    let known = app.project.sheets.len();
    let warm = slowest_frame(&ctx, &mut app, 3);
    assert!(warm < STEP_BUDGET_MS, "a quiet frame takes {warm:.0} ms");
    for n in 1..=5 {
        let start = pcbcraft_clock::instant_now();
        tap_id(&ctx, &mut app, "button:Add page")?;
        let click = ms(&start);
        let after = slowest_frame(&ctx, &mut app, 2);
        assert!(
            click < STEP_BUDGET_MS && after < STEP_BUDGET_MS,
            "adding page {n} took {click:.0} ms and the next frame {after:.0} ms"
        );
        assert_eq!(
            app.project.sheets.len(),
            known + n,
            "the page is listed: {} / tab {:?} / zoom {}",
            app.message,
            app.tab,
            app.schematic.zoom
        );
        let (id, name) = {
            let added = app.project.sheets.last().ok_or("no page was added")?;
            (added.id, added.name.clone())
        };
        assert_eq!(app.schematic_page(), id, "the new page is selected");
        assert!(
            paints_text(&ctx, &mut app, &name)?,
            "the new page has a tab"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_places_a_library_part_on_the_new_page_and_not_on_main() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let none = egui::Modifiers::NONE;
    tap_id(&ctx, &mut app, "button:Add page")?;
    let page = app.project.sheets.last().ok_or("no page was added")?.id;
    assert_eq!(app.schematic_page(), page, "the new page is open");
    app.region().left_tab = "library".to_owned();
    frame(&ctx, &mut app, vec![], none);
    // The open library answers from its recordings: no network in a story.
    tap_id(&ctx, &mut app, "button:Turn on Open library")?;
    use_recorded_library(&mut app, "new-page-chip")?;
    authoring::tests::tap(&ctx, &mut app, "Keyword or part number")?;
    for letter in "STM32F103R8".chars() {
        frame(&ctx, &mut app, vec![egui::Event::Text(letter.into())], none);
    }
    assert!(
        waits_for_text(&ctx, &mut app, "STM32F103R8Tx")?,
        "the chip number alone finds the part: {}",
        app.message
    );
    let known = app.project.components.len();
    let mut slowest = 0.0_f64;
    for _ in 0..4 {
        tap_id(
            &ctx,
            &mut app,
            "button:STM#F#R#Tx, Open library \u{b7} CC-BY-SA #.#",
        )?;
        let started = pcbcraft_clock::instant_now();
        while app.project.components.len() == known
            && started.elapsed() < std::time::Duration::from_secs(15)
        {
            let start = pcbcraft_clock::instant_now();
            frame(&ctx, &mut app, vec![], none);
            slowest = slowest.max(ms(&start));
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        if app.project.components.len() > known {
            break;
        }
    }
    let part = app
        .project
        .components
        .iter()
        .find(|c| {
            c.fields
                .iter()
                .any(|f| f.key == "MPN" && f.value == "STM32F103R8Tx")
        })
        .ok_or_else(|| format!("clicking the card places the part: {}", app.message))?;
    assert_eq!(
        part.sheet, page,
        "the part is on the page that was open, not on Main"
    );
    let on_main = app
        .project
        .components
        .iter()
        .filter(|c| c.sheet == 0)
        .any(|c| c.id == part.id);
    assert!(!on_main, "the part is not listed on Main");
    slowest = slowest.max(slowest_frame(&ctx, &mut app, 2));
    assert!(
        slowest < STEP_BUDGET_MS,
        "placing the part took a frame of {slowest:.0} ms"
    );
    Ok(())
}

#[test]
fn a_newcomer_places_a_part_by_click_on_a_new_page_of_a_big_design() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    tap_id(&ctx, &mut app, "button:Add page")?;
    let page = app.project.sheets.last().ok_or("no page was added")?.id;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.tool = Tool::Place("resistor".into());
    app.schematic.zoom = 8.0;
    let at = app
        .schematic
        .screen(Point::new(40.0, 10.0), app.canvas_rect);
    let start = pcbcraft_clock::instant_now();
    tap_at(&ctx, &mut app, at)?;
    let click = ms(&start);
    let after = slowest_frame(&ctx, &mut app, 2);
    assert!(
        click < STEP_BUDGET_MS && after < STEP_BUDGET_MS,
        "placing took {click:.0} ms and the next frame {after:.0} ms"
    );
    let part = app.project.components.last().ok_or("nothing was placed")?;
    assert_eq!(part.sheet, page, "the part is on the new page");
    Ok(())
}
