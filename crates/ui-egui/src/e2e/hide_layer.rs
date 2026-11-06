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
    assert_eq!(have, shown, "everything on Top is painted before hiding");
    let bottom_before = painted(&app, Layer::Back);
    tap_id(&ctx, &mut app, "button:Hide Layer # \u{b7} Top")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        painted(&app, Layer::Front),
        LayerCount::default(),
        "nothing of Top is painted after the eye is pressed: {}",
        app.message
    );
    assert_eq!(
        painted(&app, Layer::Back),
        bottom_before,
        "Bottom is untouched"
    );
    assert!(bottom_before.tracks > 0, "Bottom still has its track");
    Ok(())
}

#[test]
fn a_newcomer_selects_a_layer_and_sees_its_elements_lit() -> Result<(), String> {
    let (ctx, mut app) = layout_with_extras()?;
    assert!(app.layer_is_lit(Layer::Front) && app.layer_is_lit(Layer::Back));
    tap_id(&ctx, &mut app, "button:Layer # \u{b7} Top, shown, unlocked")?;
    assert!(app.layer_is_lit(Layer::Front), "Top is lit");
    assert!(!app.layer_is_lit(Layer::Back), "Bottom is dimmed");
    assert!(app.lit_alpha(Layer::Front) > app.lit_alpha(Layer::Back));
    assert!(
        app.message.contains("tracks") && app.message.contains("pads"),
        "the hint says what the row holds: {}",
        app.message
    );
    tap_id(&ctx, &mut app, "button:Layer # \u{b7} Top, shown, unlocked")?;
    assert!(
        app.layer_is_lit(Layer::Back),
        "a second click puts the lights back: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_hides_the_bottom_layer_with_a_part_on_it() -> Result<(), String> {
    let (ctx, mut app) = layout_with_extras()?;
    let part = app.project.board.footprints.first().ok_or("a part")?.id;
    app.run("footprint.flip", json!({"id": part}));
    app.refresh();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let on_bottom = app
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.id == part)
        .ok_or("the part")?;
    assert_eq!(on_bottom.board_side(), Layer::Back, "{}", app.message);
    assert!(app.part_shown(on_bottom), "the part shows with Bottom on");
    tap_id(&ctx, &mut app, "button:Hide Layer # \u{b7} Bottom")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let on_bottom = app
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.id == part)
        .ok_or("the part")?;
    assert!(
        !app.part_shown(on_bottom)
            && !app.part_pads_shown(on_bottom)
            && !app.part_artwork_shown(on_bottom)
            && !app.part_labels_shown(on_bottom),
        "the part goes with Bottom as one thing"
    );
    let top = app.layer_counts(Layer::Front);
    assert_eq!(top.0, top.1, "Top is untouched");
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
    // The Tracks eye of the Top layer is stored as a hidden sub-layer; the tree stories press it.
    let key = crate::layer_visibility::SubLayer::Tracks.storage_key(&app.layer_path(Layer::Front));
    app.preferences.hidden_layers.push(key);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let (_, shown) = app.layer_counts(Layer::Front);
    assert_eq!(shown.tracks, 0, "the Top tracks are hidden");

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
