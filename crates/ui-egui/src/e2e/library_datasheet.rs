//! Stories for the Library tab Datasheet page: reading a PDF, searching it and copying text.
//!
//! Context: issue #713. A newcomer opens a part in the Library tab, picks the Datasheet page and
//! reads the datasheet beside the details. The PDFs are drawn in code by the datasheet crate and
//! saved in a temporary folder; nothing is fetched.
use super::super::*;
use super::support::*;
use pcbcraft_datasheet::fixtures;

/// The id of the browser card of the built-in resistor.
const RESISTOR_CARD: &str = "button:Resistor, Built in \u{b7} MIT OR Apache-#.#";

/// The Library tab on the Datasheet page of the resistor, with the named fixture PDF open.
///
/// The PDF is saved in a folder that lives as long as the returned guard, the resistor's
/// Datasheet field points at it, and the person presses Read here.
fn reading(fixture: &str) -> Result<(egui::Context, App, tempfile::TempDir), String> {
    let bytes = fixtures::all()
        .into_iter()
        .find(|f| f.name == fixture)
        .map(|f| f.bytes)
        .ok_or("the fixture PDF exists")?;
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = dir.path().join("part.pdf");
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let url = format!("file:///{}", path.display().to_string().replace('\\', "/"));
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let resistor = app
        .project
        .components
        .iter()
        .find(|c| c.reference.starts_with('R'))
        .map(|c| c.id)
        .ok_or("the practice design carries a resistor")?;
    app.tool_call(
        "component.field",
        json!({"id": resistor, "key": "Datasheet", "value": url}),
    );
    app.change_tab(Tab::Library);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    tap_id(&ctx, &mut app, RESISTOR_CARD)?;
    tap_id(&ctx, &mut app, "tab:Datasheet")?;
    tap_id(&ctx, &mut app, "button:Read here")?;
    if !app.libws.datasheet.is_open() {
        return Err("Read here should open the PDF".into());
    }
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    Ok((ctx, app, dir))
}

/// Press, drag across the page and let go, like a hand selecting text.
fn drag_across_page(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    let _tall = TallScope::enter();
    let page = super::audit::controls(ctx, app)
        .into_iter()
        .find(|c| c.id.as_deref() == Some("button:Datasheet page"))
        .map(|c| c.rect)
        .ok_or("the datasheet page is on screen")?;
    let from = page.min + (page.max - page.min) * 0.1;
    let to = page.min + (page.max - page.min) * 0.9;
    let mouse = |pos: egui::Pos2, pressed: Option<bool>| {
        let mut events = vec![egui::Event::PointerMoved(pos)];
        if let Some(pressed) = pressed {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        events
    };
    frame(ctx, app, mouse(from, None), egui::Modifiers::NONE);
    frame(ctx, app, mouse(from, Some(true)), egui::Modifiers::NONE);
    for step in 1..=4 {
        let pos = from + (to - from) * (step as f32 / 4.0);
        frame(ctx, app, mouse(pos, None), egui::Modifiers::NONE);
    }
    frame(ctx, app, mouse(to, Some(false)), egui::Modifiers::NONE);
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

#[test]
fn a_newcomer_turns_the_pages_of_a_datasheet_in_the_library_tab() -> Result<(), String> {
    let (ctx, mut app, _dir) = reading("two-pages.pdf")?;
    assert!(
        paints_text_tall(&ctx, &mut app, "Page 1 of 2")?,
        "the reader says which page is on show"
    );
    tap_id(&ctx, &mut app, "button:Next page")?;
    assert_eq!(app.libws.datasheet.page, 1, "Next page shows page two");
    assert!(
        paints_text_tall(&ctx, &mut app, "Page 2 of 2")?,
        "the page count follows"
    );
    tap_id(&ctx, &mut app, "button:Previous page")?;
    assert_eq!(app.libws.datasheet.page, 0, "Previous page goes back");
    Ok(())
}

#[test]
fn a_newcomer_searches_a_datasheet_and_steps_through_the_results() -> Result<(), String> {
    let (ctx, mut app, _dir) = reading("two-pages.pdf")?;
    type_into(&ctx, &mut app, "textinput:Search the datasheet", "Pin")?;
    tap_id(&ctx, &mut app, "button:Find")?;
    let total = app.libws.datasheet.search.count();
    assert!(total >= 1, "the word Pin is in the datasheet");
    assert!(
        paints_text_tall(&ctx, &mut app, &format!("1 of {total} results"))?,
        "the reader counts the results"
    );
    tap_id(&ctx, &mut app, "button:Next result")?;
    assert!(
        paints_text_tall(
            &ctx,
            &mut app,
            &format!("{} of {total} results", 1 % total + 1)
        )?,
        "Next result moves to the following one"
    );
    tap_id(&ctx, &mut app, "button:Previous result")?;
    assert_eq!(app.libws.datasheet.search.active, 0, "Previous goes back");
    type_into(&ctx, &mut app, "textinput:Search the datasheet", "Absolute")?;
    tap_id(&ctx, &mut app, "button:Find")?;
    assert_eq!(
        app.libws.datasheet.page, 1,
        "a result on page two shows page two"
    );
    type_into(&ctx, &mut app, "textinput:Search the datasheet", "zzzqx")?;
    tap_id(&ctx, &mut app, "button:Find")?;
    assert!(
        paints_text_tall(&ctx, &mut app, "No results.")?,
        "words that are not there are said to have no results"
    );
    Ok(())
}

#[test]
fn a_newcomer_selects_datasheet_text_and_copies_it() -> Result<(), String> {
    let (ctx, mut app, _dir) = reading("text-one-page.pdf")?;
    assert!(
        !on_screen(&ctx, &mut app, "button:Copy"),
        "Copy waits until some text is selected"
    );
    drag_across_page(&ctx, &mut app)?;
    let chosen = app.libws.datasheet.selected();
    assert!(
        chosen.contains("LM317"),
        "dragging across the page selects its words in reading order: {chosen}"
    );
    let mut copied = false;
    {
        let _tall = TallScope::enter();
        let pos = interactive_rects(&ctx, &mut app)
            .into_iter()
            .find(|(n, _)| n == "Button Copy")
            .map(|(_, r)| r.center())
            .ok_or("Copy is on screen")?;
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerMoved(pos)],
            egui::Modifiers::NONE,
        );
        for pressed in [true, false] {
            let out = frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            );
            copied |= out
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t.contains("LM317")));
        }
    }
    assert!(copied, "Copy puts the selected words on the clipboard");
    Ok(())
}

#[test]
fn a_newcomer_is_told_when_a_datasheet_page_has_no_text() -> Result<(), String> {
    let (ctx, mut app, _dir) = reading("ink-only.pdf")?;
    assert!(
        paints_text_tall(&ctx, &mut app, "This page has no text to select or search.")?,
        "a page of pictures says it has no text"
    );
    Ok(())
}

#[test]
fn a_newcomer_closes_the_datasheet_reader_and_keeps_the_link_page() -> Result<(), String> {
    let (ctx, mut app, _dir) = reading("text-one-page.pdf")?;
    tap_id(&ctx, &mut app, "button:Close")?;
    assert!(!app.libws.datasheet.is_open(), "Close shuts the document");
    assert!(
        paints_text_tall(&ctx, &mut app, "Details for this part")?,
        "the details stay on the page"
    );
    Ok(())
}
