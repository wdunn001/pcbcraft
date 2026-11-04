//! Stories for net class signal integrity settings: rise time for crosstalk checks.
//!
//! Context: issue #841. A newcomer sets a rise time on a class, checks it is stored,
//! and clears it.
use super::super::*;
use super::support::*;

fn board() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (ctx, app)
}

fn open_tab(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    authoring::tests::tap(ctx, app, "File")?;
    authoring::tests::tap(ctx, app, "Project settings…")?;
    for _ in 0..3 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    tap_id(ctx, app, "tab:Net classes")
}

fn press(ctx: &egui::Context, app: &mut App, label: &str) -> Result<(), String> {
    tap_id(ctx, app, &format!("button:{label}"))
}

fn undo(app: &mut App) -> Result<(), String> {
    app.session
        .lock()
        .map_err(|_| "session is busy".to_owned())?
        .undo()
        .map_err(|e| e.to_string())?;
    app.refresh();
    Ok(())
}

#[test]
fn a_newcomer_sets_a_rise_time_and_the_class_remembers_it() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app)?;
    press(&ctx, &mut app, "New class")?;
    assert_eq!(
        app.project.net_classes.len(),
        1,
        "a class was added: {}",
        app.message
    );
    press(&ctx, &mut app, "Set a limit")?;
    type_into(&ctx, &mut app, "spinbutton:Signal rise time", "2.5")?;
    let rise = app.project.net_classes[0].rise_time;
    assert!(rise.is_some(), "the rise time was stored: {}", app.message);
    assert!(
        (rise.ok_or("rise time should be set")? - 2.5e-9).abs() < 1e-12,
        "the class stores seconds"
    );
    press(&ctx, &mut app, "Clear")?;
    assert!(
        app.project.net_classes[0].rise_time.is_none(),
        "Clear removed the rise time"
    );
    Ok(())
}

#[test]
fn a_newcomer_undoes_a_rise_time_change() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app)?;
    press(&ctx, &mut app, "New class")?;
    press(&ctx, &mut app, "Set a limit")?;
    assert!(
        app.project.net_classes[0].rise_time.is_some(),
        "the rise time was set"
    );
    undo(&mut app)?;
    assert!(
        app.project.net_classes[0].rise_time.is_none(),
        "one Undo removes the rise time"
    );
    Ok(())
}
