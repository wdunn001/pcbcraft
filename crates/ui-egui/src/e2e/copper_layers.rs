//! Stories for routing and pouring on any copper layer of a multilayer board
//! (#141): clicking a copper layer row in the Layers tree makes it the layer
//! new tracks and pours go on, and the layer keys step through the stackup.
use super::super::*;
use super::layers::pcb_app;
use super::support::*;

/// The practice board with two inner layers added from the engine, so it has
/// four copper layers: Top, In1.Cu, In2.Cu, Bottom.
fn four_layer_app() -> Result<(egui::Context, App), String> {
    let (ctx, mut app) = pcb_app();
    app.run("board.stackup_add_pair", json!({"after": "F.Cu"}));
    app.refresh();
    if app.project.copper_layer_count() != 4 {
        return Err(format!(
            "the board should have four layers: {}",
            app.message
        ));
    }
    Ok((ctx, app))
}

/// Draw one straight track with the Line tool between two canvas clicks.
fn draw_line(ctx: &egui::Context, app: &mut App, y: f64) -> Result<(), String> {
    app.highlight_net = app.project.nets.first().map(|n| n.id);
    tap_strip(ctx, app, "icon:tool-line")?;
    app.canvas_click(Point::new(30.0, y), 10.0);
    app.canvas_click(Point::new(40.0, y), 10.0);
    Ok(())
}

/// The words of a copper layer row in the Layers tree.
fn inner_row(n: usize, name: &str) -> String {
    t!("tree.layer.inner", n = n, name = name)
}

#[test]
fn a_newcomer_picks_an_inner_layer_in_the_tree_and_draws_a_track_on_it() -> Result<(), String> {
    let (ctx, mut app) = four_layer_app()?;
    tap_left(&ctx, &mut app, &inner_row(2, "In1.Cu"))?;
    let inner = pcbcraft_doc::Layer::from_index(1)?;
    assert_eq!(
        app.route_layer, inner,
        "clicking In1.Cu makes it the routing layer: {}",
        app.message
    );
    draw_line(&ctx, &mut app, 10.0)?;
    let track = app
        .project
        .board
        .tracks
        .last()
        .cloned()
        .ok_or_else(|| format!("the line should make a track: {}", app.message))?;
    assert_eq!(
        track.layer, inner,
        "a track drawn while In1.Cu is picked lands on In1.Cu, not the bottom: {}",
        app.message
    );
    let plot = pcbcraft_io::gerber_copper(&app.project, inner)?;
    assert!(
        plot.contains("Copper,L2,Inr") && plot.contains("D01*"),
        "the track is in the second copper plot"
    );
    // Back to the outer layers: Bottom is the last layer, index 3.
    tap_left(&ctx, &mut app, &t!("tree.layer.bottom", n = 4))?;
    draw_line(&ctx, &mut app, 20.0)?;
    let bottom = app
        .project
        .board
        .tracks
        .last()
        .cloned()
        .ok_or("the second line should make a track")?;
    assert_eq!(
        bottom.layer,
        app.project.bottom_layer(),
        "Bottom is the last copper layer of the board: {}",
        app.message
    );
    assert_eq!(
        bottom.layer.index(),
        3,
        "the bottom of four layers is layer 4"
    );
    Ok(())
}

#[test]
fn a_newcomer_steps_through_every_copper_layer_with_the_layer_keys() -> Result<(), String> {
    let (ctx, mut app) = four_layer_app()?;
    assert_eq!(
        app.route_layer.index(),
        0,
        "routing starts on the top layer"
    );
    for want in [1u8, 2, 3, 0] {
        key(&ctx, &mut app, egui::Key::CloseBracket);
        assert_eq!(
            app.route_layer.index(),
            want,
            "the next-layer key moves one layer down the stackup: {}",
            app.message
        );
    }
    key(&ctx, &mut app, egui::Key::OpenBracket);
    assert_eq!(
        app.route_layer.index(),
        3,
        "the previous-layer key wraps from the top to the bottom: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_pours_copper_on_an_inner_layer() -> Result<(), String> {
    let (ctx, mut app) = four_layer_app()?;
    tap_left(&ctx, &mut app, &inner_row(3, "In2.Cu"))?;
    assert_eq!(
        app.route_layer.index(),
        2,
        "clicking In2.Cu makes it the routing layer: {}",
        app.message
    );
    app.change_tab(Tab::Settings);
    scrolled_to_at(&ctx, &mut app, "Pour copper", 700.0)?;
    authoring::tests::tap(&ctx, &mut app, "Pour copper")?;
    let zone = app
        .project
        .board
        .zones
        .last()
        .cloned()
        .ok_or_else(|| format!("Pour copper should make a pour: {}", app.message))?;
    assert_eq!(
        zone.layer.index(),
        2,
        "the pour goes on the layer picked in the Layers tree, In2.Cu: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_keeps_routing_on_the_bottom_when_inner_layers_are_added() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    tap_left(&ctx, &mut app, &t!("tree.layer.bottom", n = 2))?;
    assert_eq!(
        app.route_layer.index(),
        1,
        "a two-layer board's bottom is layer 2"
    );
    app.run("board.stackup_add_pair", json!({"after": "F.Cu"}));
    app.refresh();
    assert_eq!(
        app.route_layer,
        app.project.bottom_layer(),
        "someone routing on the bottom stays on the bottom after two inner layers appear: {}",
        app.message
    );
    Ok(())
}
