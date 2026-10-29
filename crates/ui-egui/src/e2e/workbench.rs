//! Stories for the left rail, the library dock, the datasheet action and the
//! breathing room around every dock and tab body.
use super::super::*;
use super::support::{frame, scrolled_to};

const SCREEN: Vec2 = Vec2::new(1440.0, 940.0);

fn frame_plain(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
            events,
            ..Default::default()
        },
        |ui| app.show(ui),
    );
    output.textures_delta.clear();
    output
}

/// Painted text as (words, left edge, right edge, top).
fn texts(output: &egui::FullOutput) -> Vec<(String, f32, f32, f32)> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if shape.clip_rect.contains(t.pos) => Some((
                t.galley.text().to_owned(),
                t.pos.x,
                t.pos.x + t.galley.size().x,
                t.pos.y,
            )),
            _ => None,
        })
        .collect()
}

fn centre_of(output: &egui::FullOutput, label: &str, left_of: f32) -> Option<egui::Pos2> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(t) if t.galley.text() == label && t.pos.x < left_of => {
            Some(t.pos + t.galley.size() / 2.0)
        }
        _ => None,
    })
}

fn click(ctx: &egui::Context, app: &mut App, at: egui::Pos2) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame_plain(ctx, app, vec![egui::Event::PointerMoved(at)]);
    frame_plain(ctx, app, vec![press(true)]);
    frame_plain(ctx, app, vec![press(false)]);
    frame_plain(ctx, app, vec![]);
}

fn fresh() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app
}

fn settled(ctx: &egui::Context, app: &mut App) -> egui::FullOutput {
    for _ in 0..4 {
        frame_plain(ctx, app, vec![]);
    }
    frame_plain(ctx, app, vec![])
}

#[test]
fn finding_counts_split_by_loudness() {
    let checks = json!({
        "erc": [{"severity":"error"},{"severity":"warning"}],
        "drc": [{"severity":"error"},{"severity":"info"},{}]
    });
    assert_eq!(App::finding_counts(&checks), (2, 1, 2));
    assert_eq!(App::finding_counts(&json!({})), (0, 0, 0));
}

#[test]
fn a_newcomer_toggles_the_library_dock_from_the_left_rail() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    let out = settled(&ctx, &mut app);
    let at = centre_of(&out, "Lib", 44.0).ok_or("the rail should offer the library toggle")?;
    assert!(
        app.preferences.library_dock_open,
        "the library dock starts open"
    );
    click(&ctx, &mut app, at);
    assert!(
        !app.preferences.library_dock_open,
        "the rail toggle hides the dock"
    );
    click(&ctx, &mut app, at);
    assert!(
        app.preferences.library_dock_open,
        "the rail toggle shows it again"
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_three_stacked_check_badges_and_opens_checks() -> Result<(), String> {
    let mut app = fresh();
    app.checks = json!({
        "erc": [{"severity":"error"},{"severity":"error"},{"severity":"warning"}],
        "drc": [{"severity":"info"},{"severity":"info"},{"severity":"info"},{"severity":"info"}]
    });
    let ctx = egui::Context::default();
    let out = settled(&ctx, &mut app);
    let errors = centre_of(&out, "2", 44.0).ok_or("an errors badge reads 2")?;
    centre_of(&out, "1", 44.0).ok_or("a warnings badge reads 1")?;
    centre_of(&out, "4", 44.0).ok_or("a notes badge reads 4")?;
    click(&ctx, &mut app, errors);
    assert_eq!(app.tab, Tab::Checks, "a badge opens the Checks tab");
    Ok(())
}

#[test]
fn a_newcomer_finds_no_board_statistics_in_the_library_dock() -> Result<(), String> {
    let mut app = fresh();
    app.tool_call("sample_design", json!({}));
    app.refresh();
    let ctx = egui::Context::default();
    let out = settled(&ctx, &mut app);
    for (words, left, _, _) in texts(&out) {
        if left > 44.0 && left < 304.0 {
            let lower = words.to_lowercase();
            for banned in ["board size", "dimensions", "layer count", "parts placed"] {
                assert!(
                    !lower.contains(banned),
                    "left dock shows a statistic: {words}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn datasheet_addresses_must_be_plain_web_links() {
    assert_eq!(
        App::datasheet_url(Some(" https://example.com/a.pdf ")).as_deref(),
        Some("https://example.com/a.pdf")
    );
    assert!(App::datasheet_url(Some("HTTP://example.com/a")).is_some());
    for bad in [
        "",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "https://a b",
    ] {
        assert!(App::datasheet_url(Some(bad)).is_none(), "{bad} is refused");
    }
    assert!(App::datasheet_url(None).is_none());
}

#[test]
fn a_newcomer_opens_a_part_datasheet_from_properties() -> Result<(), String> {
    let mut app = fresh();
    app.tool_call("sample_design", json!({}));
    let resistor = app
        .project
        .components
        .iter()
        .find(|c| c.reference.starts_with('R'))
        .map(|c| c.id)
        .ok_or("the practice design carries a resistor")?;
    app.select(Some(resistor));
    let ctx = egui::Context::default();
    settled(&ctx, &mut app);
    assert!(
        scrolled_to(&ctx, &mut app, "View datasheet")?,
        "a chosen part offers its datasheet: {}",
        app.message
    );
    let opens = |app: &mut App, ctx: &egui::Context| -> Result<bool, String> {
        let out = frame(ctx, app, vec![], egui::Modifiers::NONE);
        let at = centre_of(&out, "View datasheet", f32::MAX)
            .ok_or("the datasheet action stays visible")?;
        let mut any = false;
        for pressed in [None, Some(true), Some(false)] {
            let event = match pressed {
                None => egui::Event::PointerMoved(at),
                Some(pressed) => egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            };
            let out = frame(ctx, app, vec![event], egui::Modifiers::NONE);
            any |= out
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::OpenUrl(_)));
        }
        Ok(any)
    };
    assert!(
        !opens(&mut app, &ctx)?,
        "without a web link in the Datasheet field the action is disabled"
    );
    app.tool_call(
        "component.field",
        json!({"id": resistor, "key": "Datasheet", "value": "https://example.com/r.pdf"}),
    );
    app.select(None);
    app.select(Some(resistor));
    settled(&ctx, &mut app);
    assert!(
        scrolled_to(&ctx, &mut app, "View datasheet")?,
        "the datasheet action stays visible"
    );
    assert!(
        opens(&mut app, &ctx)?,
        "with a web link in the Datasheet field the button opens it"
    );
    Ok(())
}

#[test]
fn a_newcomer_reads_every_dock_and_tab_body_with_room_to_spare() -> Result<(), String> {
    let mut app = fresh();
    app.tool_call("sample_design", json!({}));
    app.refresh();
    let ctx = egui::Context::default();
    let out = settled(&ctx, &mut app);
    let (rail, dock) = (44.0, 44.0 + 260.0);
    // The aside may grow past its 280 px minimum, so find its edge from
    // the panel's own heading, which sits one 16 px margin inside it.
    let aside = texts(&out)
        .into_iter()
        .find(|(w, l, _, _)| w == "Details" && *l > SCREEN.x / 2.0)
        .map(|(_, l, _, _)| l - 16.0)
        .ok_or("the right aside should show its Details heading")?;
    assert!(
        aside <= SCREEN.x - 280.0 && aside > SCREEN.x - 420.0,
        "the right aside is at least 280 px wide: starts at {aside}"
    );
    for (words, left, right, top) in texts(&out) {
        if words.trim().is_empty() || top < 80.0 || top > 900.0 {
            continue;
        }
        if left > rail && left < dock {
            assert!(
                left >= rail + 12.0,
                "left dock text hugs the edge: {words} at {left}"
            );
        }
        if left > aside {
            assert!(
                left >= aside + 12.0,
                "right aside text hugs the edge: {words} at {left},{top}"
            );
            assert!(
                right <= SCREEN.x - 12.0,
                "right aside text hugs the far edge: {words} to {right}"
            );
        }
    }
    for tab in [Tab::Files, Tab::Libraries, Tab::Checks] {
        app.change_tab(tab);
        let out = settled(&ctx, &mut app);
        for (words, left, _, top) in texts(&out) {
            if words.trim().is_empty() || top < 110.0 || top > 900.0 {
                continue;
            }
            if left > dock && left < SCREEN.x - 280.0 - 40.0 {
                assert!(
                    left >= dock + 12.0,
                    "{tab:?} body text hugs the dock edge: {words} at {left},{top}"
                );
            }
        }
    }
    Ok(())
}
