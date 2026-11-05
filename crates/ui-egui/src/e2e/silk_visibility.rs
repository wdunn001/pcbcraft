//! Stories for silkscreen on the Layout canvas: it is a sub-layer of the Top and the Bottom
//! layer only, it hides with its layer or on its own, and the far side is ghosted.
use super::super::*;
use super::support::*;
use crate::layer_visibility as lv;
use pcbcraft_doc::{ArtSide, Layer};

/// The showcase board on the Layout tab, with the top view and a Logo silk layer.
fn showcase() -> Result<(egui::Context, App), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.preferences.reduced_motion = true;
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase.pcbcraft");
    app.tool_call("open", json!({"path": path.to_string_lossy()}));
    app.change_tab(Tab::Pcb);
    app.region().left_tab = "layers".to_owned();
    app.refresh();
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    Ok((ctx, app))
}

/// Alphas of the silk marks painted now: the colour a drawn silk layer uses.
fn silk_marks(ctx: &egui::Context, app: &mut App) -> Vec<u8> {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let ink = app.colours().ink.to_srgba_unmultiplied();
    canvas_marks_of(app, &output, [ink[0], ink[1], ink[2]])
}

fn silk_layer_id(app: &App, side: ArtSide) -> Result<u64, String> {
    app.project
        .board
        .user_layers
        .iter()
        .find(|l| l.kind == pcbcraft_doc::ArtKind::Silk && l.side == side)
        .map(|l| l.id)
        .ok_or_else(|| format!("no {side:?} silk layer"))
}

#[test]
fn a_newcomer_hides_the_top_layer_and_its_silkscreen_marks_go_with_it() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let shown = silk_marks(&ctx, &mut app);
    assert!(!shown.is_empty(), "the showcase paints silk marks on Top");
    tap_id(&ctx, &mut app, "button:Hide Layer # \u{b7} Top")?;
    let after = silk_marks(&ctx, &mut app);
    assert!(
        after.len() < shown.len(),
        "hiding Top removes its silk marks: {} before, {} after",
        shown.len(),
        after.len()
    );
    assert!(
        !app.art_layer_shown(
            app.project
                .board
                .user_layers
                .iter()
                .find(|l| l.kind == pcbcraft_doc::ArtKind::Silk)
                .ok_or("a silk layer")?
        ),
        "the Logo silk layer sits on Top and is hidden with it"
    );
    Ok(())
}

#[test]
fn a_newcomer_hides_only_the_top_silkscreen_and_keeps_top_copper() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let shown = silk_marks(&ctx, &mut app);
    let tracks = app.layer_counts(Layer::Front).1.tracks;
    assert!(tracks > 0, "Top has copper tracks");
    lv::set_sub_stored(
        &mut app.preferences.hidden_layers,
        "side:top",
        lv::SubLayer::Silk,
        false,
    );
    let after = silk_marks(&ctx, &mut app);
    assert!(
        after.len() < shown.len(),
        "the silkscreen eye removes the marks: {} before, {} after",
        shown.len(),
        after.len()
    );
    assert_eq!(
        app.layer_counts(Layer::Front).1.tracks,
        tracks,
        "Top copper stays painted"
    );
    assert!(app.layer_group_shown(Layer::Front));
    Ok(())
}

#[test]
fn a_newcomer_hides_every_layer_and_no_silkscreen_stays_on_the_board() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let hidden = &mut app.preferences.hidden_layers;
    for path in ["side:top", "side:bottom", "inner:1", "inner:2"] {
        lv::set_group_stored(hidden, path, false);
    }
    let marks = silk_marks(&ctx, &mut app);
    assert!(marks.is_empty(), "silk marks still painted: {marks:?}");
    Ok(())
}

#[test]
fn a_newcomer_sees_silkscreen_under_top_and_bottom_and_never_under_an_inner_layer()
-> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    let _tall = TallScope::enter();
    for key in ["side:top", "side:bottom", "inner:1"] {
        app.tree.expanded.insert(key.to_owned());
    }
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let silk_rows = output
        .shapes
        .iter()
        .filter(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "Silkscreen"))
        .count();
    assert_eq!(
        silk_rows, 2,
        "one silkscreen row under Top and one under Bottom"
    );
    Ok(())
}

#[test]
fn a_newcomer_looks_from_below_and_sees_top_silkscreen_ghosted_and_bottom_at_full()
-> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    app.run(
        "board.layer_add",
        json!({"name": "Bottom note", "kind": "silk", "side": "bottom"}),
    );
    app.refresh();
    let bottom_id = silk_layer_id(&app, ArtSide::Bottom)?;
    app.run(
        "board.layer_line_add",
        json!({"layer": bottom_id, "x1": 20.0, "y1": 20.0, "x2": 30.0, "y2": 20.0}),
    );
    app.refresh();
    let top = silk_marks(&ctx, &mut app);
    assert!(
        top.contains(&255),
        "from above, top silkscreen reads at full strength: {top:?}"
    );
    assert!(
        top.iter().any(|a| *a < 255),
        "from above, bottom silkscreen is ghosted: {top:?}"
    );
    authoring::tests::tap(&ctx, &mut app, "Bottom")?;
    let below = silk_marks(&ctx, &mut app);
    assert!(app.pcb.mirror, "the board is viewed from below");
    assert!(
        below.contains(&255),
        "from below, bottom silkscreen is at full strength: {below:?}"
    );
    assert!(
        below.iter().any(|a| *a < 255),
        "from below, top silkscreen is ghosted: {below:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_cannot_put_silkscreen_on_an_inner_layer() -> Result<(), String> {
    let (_ctx, mut app) = showcase()?;
    let before = app.project.board.user_layers.len();
    app.run(
        "board.layer_add",
        json!({"name": "Inner note", "kind": "silk", "side": "inner"}),
    );
    app.run(
        "text.place",
        json!({"text": "inner", "position": {"x": 5.0, "y": 5.0}, "layer": "In1.Cu"}),
    );
    app.refresh();
    assert_eq!(
        app.project.board.user_layers.len(),
        before,
        "no inner silk layer"
    );
    assert!(
        app.project
            .board
            .texts
            .iter()
            .all(|t| t.layer == Layer::Front || t.layer == app.project.bottom_layer()),
        "no text sits on an inner layer"
    );
    Ok(())
}
