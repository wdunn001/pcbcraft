//! Stories for the left rail, the library dock, the datasheet action and the
//! breathing room around every dock and tab body.
use super::super::*;
use super::support::{
    frame, go_to_tab, paints_text, scrolled_to, show_layers_tab, show_library_tab,
};

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
    settled(&ctx, &mut app);
    assert_eq!(
        app.preferences.left_tab, 0,
        "the layers tree is the default left-aside tab"
    );
    show_library_tab(&ctx, &mut app);
    assert_eq!(
        app.preferences.left_tab, 1,
        "the Library tab shows the parts browser"
    );
    show_layers_tab(&ctx, &mut app);
    assert_eq!(
        app.preferences.left_tab, 0,
        "the Layers tab brings the tree back"
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
    settled(&ctx, &mut app);
    let errors_label = app.catalog.plural("board.footer.errors", 2);
    let warnings_label = app.catalog.plural("board.footer.warnings", 5);
    assert!(
        paints_text(&ctx, &mut app, &errors_label)?,
        "the status bar counts errors"
    );
    assert!(
        paints_text(&ctx, &mut app, &warnings_label)?,
        "warnings and notes share one count in the footer"
    );
    authoring::tests::tap(&ctx, &mut app, &errors_label)?;
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
        aside <= SCREEN.x - 240.0 && aside > SCREEN.x - 450.0,
        "the right aside stays roughly 280 px wide beside the wider left tree: starts at {aside}"
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

/// A focused control that a window drawn above it covers on all five probe
/// points (centre and four inset corners), as (screen, control name).
fn obscured_focus(ctx: &egui::Context, screen: &str, found: &mut Vec<String>) -> bool {
    let Some(id) = ctx.memory(|m| m.focused()) else {
        return false;
    };
    let Some(widget) = ctx.read_response(id) else {
        return false;
    };
    let rect = widget.rect;
    if !rect.is_finite() || rect.width() < 2.0 || rect.height() < 2.0 {
        return false;
    }
    let inner = rect.shrink2(rect.size() * 0.2);
    let probes = [
        rect.center(),
        inner.left_top(),
        inner.right_top(),
        inner.left_bottom(),
        inner.right_bottom(),
    ];
    let covered = probes.iter().all(|p| {
        ctx.layer_id_at(*p)
            .is_some_and(|top| top != widget.layer_id && top.order > widget.layer_id.order)
    });
    if covered {
        found.push(format!(
            "{screen}: control at {rect:?} is under another window"
        ));
    }
    true
}

#[test]
fn tabbing_through_the_docks_never_leaves_focus_under_a_floating_window() -> Result<(), String> {
    let tab_key = egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: Some(egui::Key::Tab),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let mut found = Vec::new();
    let mut walked = Vec::new();
    let mut checked = 0usize;
    for (tab, palette) in [
        (Tab::Schematic, false),
        (Tab::Pcb, false),
        (Tab::Pcb, true),
        (Tab::Rules, false),
        (Tab::Checks, false),
        (Tab::Settings, false),
    ] {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.select(Some(app.project.components[0].id));
        app.change_tab(tab);
        app.command_open = palette;
        let ctx = egui::Context::default();
        frame_plain(&ctx, &mut app, vec![]);
        let screen = format!(
            "{tab:?}{}",
            if palette {
                " with the command console"
            } else {
                ""
            }
        );
        for _ in 0..90 {
            frame_plain(&ctx, &mut app, vec![tab_key.clone()]);
            checked += usize::from(obscured_focus(&ctx, &screen, &mut found));
        }
        walked.push(screen);
    }
    assert!(
        checked > 100,
        "the walk really lands on controls: {checked}"
    );
    found.sort();
    found.dedup();
    assert!(
        found.is_empty(),
        "walked {walked:?}; focus hidden under a floating window:\n{}",
        found.join("\n")
    );
    Ok(())
}
