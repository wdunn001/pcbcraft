//! Stories for hiding a layer and picking a layer row in the Layout Layers tree.
use super::super::*;
use super::support::*;
use crate::board_visibility::LayerCount;
use pcbcraft_doc::Layer;

/// The sample board with a track, a drawn rectangle and the Layers tree open.
fn layout_with_extras() -> Result<(egui::Context, App), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.region().left_tab = "layers".to_owned();
    app.refresh();
    let net = app.project.nets.first().ok_or("sample has nets")?.id;
    app.run(
        "track.add",
        json!({"net": net, "start": {"x": 10.0, "y": 30.0}, "end": {"x": 30.0, "y": 30.0}, "width": 0.25}),
    );
    app.run(
        "track.add",
        json!({"net": net, "start": {"x": 10.0, "y": 34.0}, "end": {"x": 30.0, "y": 34.0}, "width": 0.25, "layer": "B.Cu"}),
    );
    app.run(
        "graphic.add",
        json!({"layer": "board", "shape": "rect", "points": [{"x": 2.0, "y": 2.0}, {"x": 9.0, "y": 8.0}], "width": 0.2}),
    );
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((ctx, app))
}

fn painted(app: &App, layer: Layer) -> LayerCount {
    app.layer_counts(layer).1
}

#[test]
fn a_newcomer_hides_the_top_layer_and_nothing_of_it_stays_on_the_board() -> Result<(), String> {
    let (ctx, mut app) = layout_with_extras()?;
    let (have, shown) = app.layer_counts(Layer::Front);
    assert!(
        have.tracks > 0 && have.pads > 0 && have.silkscreen > 0,
        "{have:?}"
    );
    assert!(
        have.drawn > 0 && have.ratsnest > 0 && have.labels > 0,
        "{have:?}"
    );
    assert_eq!(shown.tracks, have.tracks, "all tracks show by default");
    tap_id(&ctx, &mut app, "button:Hide Layer # \u{b7} Top")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let (_, shown) = app.layer_counts(Layer::Front);
    assert_eq!(
        shown,
        LayerCount::default(),
        "hiding the layer makes nothing show"
    );
    Ok(())
}

#[test]
fn a_newcomer_hides_the_bottom_layer_too() -> Result<(), String> {
    let (ctx, mut app) = layout_with_extras()?;
    let (have, shown) = app.layer_counts(Layer::Back);
    assert!(have.tracks > 0 && have.pads > 0, "{have:?}");
    assert_eq!(shown.tracks, have.tracks, "bottom tracks show by default");
    tap_id(&ctx, &mut app, "button:Hide Layer # \u{b7} Bottom")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let (_, shown) = app.layer_counts(Layer::Back);
    assert_eq!(shown, LayerCount::default(), "bottom layer is hidden");
    Ok(())
}

#[test]
fn a_newcomer_hides_just_the_tracks() -> Result<(), String> {
    let (ctx, mut app) = layout_with_extras()?;
    let (have, _shown) = app.layer_counts(Layer::Front);
    assert!(have.tracks > 0 && have.pads > 0, "{have:?}");
    tap_id(&ctx, &mut app, "button:Tracks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let (_, shown) = app.layer_counts(Layer::Front);
    assert_eq!(shown.tracks, 0, "tracks are hidden");
    assert_eq!(shown.pads, have.pads, "pads still show");
    Ok(())
}

/// Hide a layer, click where its track sits, and verify nothing is selected.
/// Show the layer again and verify the track is selectable.
/// Box select skips hidden objects.
#[test]
fn a_newcomer_hidden_layers_are_not_pickable() -> Result<(), String> {
    let (ctx, mut app) = layout_with_extras()?;

    // Find the top-layer track we just created (at y=30.0)
    let track = app
        .project
        .board
        .tracks
        .iter()
        .find(|t| t.start.y == 30.0 && t.end.y == 30.0)
        .ok_or("track created")?
        .id;

    // Pick at the middle of the track
    let pick_pos = Point::new(20.0, 30.0);

    // 1. Verify the track is pickable when layer is shown
    let picked = app.pick_object(pick_pos, 10.0);
    assert_eq!(picked, Some(track), "track is pickable when layer is shown");

    // 2. Hide the layer via the layers tree
    tap_id(&ctx, &mut app, "button:Hide Layer # \u{b7} Top")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);

    // Verify the layer is hidden
    let (_, shown) = app.layer_counts(Layer::Front);
    assert_eq!(shown, LayerCount::default(), "layer is hidden");

    // Verify nothing is pickable when layer is hidden
    let picked = app.pick_object(pick_pos, 10.0);
    assert_eq!(picked, None, "nothing is pickable when layer is hidden");

    // 3. Show the layer again
    tap_id(&ctx, &mut app, "button:Show Layer # \u{b7} Top")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);

    // Verify the track is pickable again
    let picked = app.pick_object(pick_pos, 10.0);
    assert_eq!(
        picked,
        Some(track),
        "track is pickable again when layer is shown"
    );

    // 4. Hide the tracks sub-layer
    tap_id(&ctx, &mut app, "button:Tracks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);

    // Verify the track is not pickable
    let picked = app.pick_object(pick_pos, 10.0);
    assert!(
        picked != Some(track),
        "hidden track sub-layer is not pickable"
    );

    // 5. Box select skips hidden objects
    let lo = pick_pos.offset(Point::new(-1.0, -1.0));
    let hi = pick_pos.offset(Point::new(1.0, 1.0));
    let boxed = app.ids_in_rect(lo, hi);
    assert!(!boxed.contains(&track), "box select skips hidden track");

    Ok(())
}
