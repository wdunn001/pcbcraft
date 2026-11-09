use super::super::*;
use super::support::*;

/// Number of painted lines and outlines within `reach` pixels of a screen point.
fn marks_near(output: &egui::FullOutput, at: egui::Pos2, reach: f32) -> usize {
    fn walk(shape: &egui::Shape, at: egui::Pos2, reach: f32, n: &mut usize) {
        let close = |p: &egui::Pos2| p.distance(at) <= reach;
        match shape {
            egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, at, reach, n)),
            egui::Shape::Path(path) if path.points.iter().any(close) => *n += 1,
            egui::Shape::LineSegment { points, .. } if points.iter().any(close) => *n += 1,
            egui::Shape::Circle(c) if close(&c.center) => *n += 1,
            egui::Shape::Rect(r) if close(&r.rect.center()) => *n += 1,
            _ => {}
        }
    }
    let mut n = 0;
    for clipped in &output.shapes {
        walk(&clipped.shape, at, reach, &mut n);
    }
    n
}

fn open_schematic(prefs: Preferences) -> Result<(egui::Context, App), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, prefs, None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.refresh();
    app.change_tab(Tab::Schematic);
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    Ok((ctx, app))
}

fn every_part_draws(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    for reference in ["J1", "R1", "D1"] {
        let part = app
            .project
            .components
            .iter()
            .find(|c| c.reference == reference)
            .ok_or(format!("the practice design should carry {reference}"))?;
        let at = app.schematic.screen(part.position, app.canvas_rect);
        let marks = marks_near(&output, at, 6.0);
        let labelled = output.shapes.iter().any(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => {
                text.galley.text() == reference && app.canvas_rect.contains(text.pos)
            }
            _ => false,
        });
        if marks == 0 && !labelled {
            return Err(format!("{reference} draws nothing on the schematic"));
        }
    }
    Ok(())
}

#[test]
fn a_newcomer_sees_every_part_symbol_on_the_schematic() -> Result<(), String> {
    let (ctx, mut app) = open_schematic(Preferences::default())?;
    every_part_draws(&ctx, &mut app)?;
    Ok(())
}

#[test]
fn a_newcomer_who_hides_the_symbols_row_sees_no_part_symbol() -> Result<(), String> {
    let (ctx, mut app) = open_schematic(Preferences::default())?;
    tap_id(&ctx, &mut app, "button:Hide Symbols")?;
    match every_part_draws(&ctx, &mut app) {
        Err(_) => Ok(()),
        Ok(()) => Err("hiding the Symbols row should hide the part symbols".into()),
    }
}

#[test]
fn a_newcomer_with_an_old_hidden_symbols_setting_still_sees_the_parts() -> Result<(), String> {
    let mut prefs = Preferences {
        hidden_layers: vec!["Symbols".into(), "Net labels".into(), "Graphics".into()],
        ..Preferences::default()
    };
    // The app does this when it loads saved settings.
    prefs.normalize_hidden_layers();
    let (ctx, mut app) = open_schematic(prefs)?;
    every_part_draws(&ctx, &mut app)
}

#[test]
fn a_newcomer_sees_no_move_buttons_over_a_hidden_part() -> Result<(), String> {
    let (ctx, mut app) = open_schematic(Preferences::default())?;
    let j1 = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "J1")
        .map(|c| c.id)
        .ok_or("the practice design should carry J1")?;
    app.select(Some(j1));
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let names = |ctx: &egui::Context, app: &mut App| {
        let out = frame(ctx, app, vec![], egui::Modifiers::NONE);
        crate::a11y::audit::interactive_nodes(&out)
            .iter()
            .filter(|(role, label)| *role == egui::accesskit::Role::Button && label == "Move")
            .count()
    };
    if names(&ctx, &mut app) == 0 {
        return Err("a drawn, selected part shows its move button".into());
    }
    tap_id(&ctx, &mut app, "button:Hide Symbols")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    if names(&ctx, &mut app) != 0 {
        return Err("a hidden part must not keep a move button floating".into());
    }
    Ok(())
}
