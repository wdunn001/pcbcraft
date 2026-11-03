//! Stories for the status footer: the project name with its unsaved dot, and the plain
//! sentences it shows after a command.
//!
//! Context: `crates/ui-egui/src/project_status.rs`. Each story runs twice, as the desktop
//! app and as the browser build, because the footer is the same in both.
use super::super::*;
use super::modifiers::{click, screen_of};
use super::support::*;

/// The sample design in an app of the given kind, with a file of its own to save to.
fn app_with_sample(browser: bool, name: &str) -> (App, egui::Context, String) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let preferences = Preferences {
        setup_wizard_seen: true,
        ..Preferences::default()
    };
    let mut app = App::create(shared, preferences, None, None);
    app.browser = browser;
    if browser {
        // The browser build saves through the host: a download, no file path.
        app = app.with_project_host(Box::new(pcbcraft_mcp::MemHost::new()));
    }
    app.refresh();
    let file = std::env::temp_dir()
        .join(format!(
            "pcbcraft-footer-{name}-{}.pcbcraft",
            std::process::id()
        ))
        .to_string_lossy()
        .into_owned();
    app.path = file.clone();
    app.tool_call("sample_design", json!({}));
    app.tab = Tab::Schematic;
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (app, ctx, file)
}

/// How many unsaved dots the footer paints right now.
fn dots(ctx: &egui::Context, app: &mut App) -> usize {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let copper = theme::tokens(ctx).copper;
    output
        .shapes
        .iter()
        .filter(|clipped| {
            matches!(&clipped.shape, egui::Shape::Circle(c)
                if c.fill == copper && c.radius == 3.5 && c.center.y > 900.0)
        })
        .count()
}

/// Put a net label on a wired pin by clicking the real Label control, one more edit.
fn edit_the_design(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    tap_strip(ctx, app, "icon:tool-label")?;
    let pin = app
        .project
        .components
        .iter()
        .flat_map(|comp| comp.pins.iter().map(move |pin| (comp, pin)))
        .filter(|(_, pin)| pin.net.is_some())
        .filter_map(|(comp, pin)| comp.pin_position(&pin.number))
        .find(|at| app.canvas_rect.contains(screen_of(app, *at)))
        .ok_or("the sample has no wired pin on screen")?;
    let before = app.project.labels.len();
    let at = screen_of(app, pin);
    click(ctx, app, at, egui::Modifiers::NONE);
    if app.project.labels.len() == before {
        return Err(format!(
            "the click did not edit the design: {}",
            app.message
        ));
    }
    key(ctx, app, egui::Key::Escape);
    Ok(())
}

fn unsaved_dot_story(browser: bool) -> Result<(), String> {
    let (mut app, ctx, file) = app_with_sample(browser, if browser { "web" } else { "desk" });
    let name = app.footer_project_name();
    assert!(
        paints_text(&ctx, &mut app, &name)?,
        "the footer names the open project ({name})"
    );
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Save")?;
    assert_eq!(
        dots(&ctx, &mut app),
        0,
        "a saved project shows no dot: {}",
        app.message
    );
    edit_the_design(&ctx, &mut app)?;
    assert_eq!(
        dots(&ctx, &mut app),
        1,
        "editing the design shows one dot: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Save")?;
    assert_eq!(
        dots(&ctx, &mut app),
        0,
        "saving takes the dot away: {}",
        app.message
    );
    assert!(
        app.message.starts_with("Saved to"),
        "the footer says it plainly: {}",
        app.message
    );
    let _ = std::fs::remove_file(&file);
    Ok(())
}

#[test]
fn a_newcomer_sees_the_unsaved_dot_appear_and_disappear() -> Result<(), String> {
    unsaved_dot_story(false)
}

#[test]
fn a_newcomer_sees_the_unsaved_dot_in_the_browser_build() -> Result<(), String> {
    unsaved_dot_story(true)
}

fn plain_words_story(browser: bool) -> Result<(), String> {
    let (mut app, ctx, file) =
        app_with_sample(browser, if browser { "words-web" } else { "words" });
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "New")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.message, "New project started",
        "the footer says what happened in plain words"
    );
    assert!(
        paints_text(&ctx, &mut app, "New project started")?,
        "the footer shows the sentence"
    );
    for raw in ["new_design", "complete"] {
        assert!(
            !app.message.contains(raw),
            "the footer never shows {raw}: {}",
            app.message
        );
        assert!(
            !paints_text(&ctx, &mut app, raw)?,
            "nothing on screen shows {raw}"
        );
    }
    assert_eq!(dots(&ctx, &mut app), 0, "a blank project shows no dot");
    let _ = std::fs::remove_file(&file);
    Ok(())
}

#[test]
fn a_newcomer_sees_plain_words_in_the_footer() -> Result<(), String> {
    plain_words_story(false)
}

#[test]
fn a_newcomer_sees_plain_words_in_the_footer_of_the_browser_build() -> Result<(), String> {
    plain_words_story(true)
}
