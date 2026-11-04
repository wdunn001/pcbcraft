//! Stories for the Shape section of Board settings (#1173).
use super::super::*;
use super::layers::pcb_app;
use super::support::*;

fn board_settings() -> Result<(egui::Context, App), String> {
    let (ctx, mut app) = pcb_app();
    // The practice board has a cutout and parts that sit across the whole
    // board; start from a clean empty board so only the shape is under test.
    app.tool_call("new_design", json!({}));
    app.refresh();
    app.change_tab(Tab::Pcb);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    open_tree_row(&ctx, &mut app, "root");
    assert!(
        paints_text(&ctx, &mut app, "Board settings")?,
        "the board row opens board settings: {}",
        app.message
    );
    Ok((ctx, app))
}

fn ring_area(app: &App) -> f64 {
    let b = &app.project.board;
    let ring =
        pcbcraft_doc::flatten_outline(b.outline.as_deref().unwrap_or(&[]), &b.outline_arcs, 0.005);
    pcbcraft_doc::outline_area(&ring)
}

#[test]
fn a_newcomer_picks_a_round_board_shape_in_board_settings() -> Result<(), String> {
    let (ctx, mut app) = board_settings()?;
    assert!(
        scrolled_to(&ctx, &mut app, "SHAPE")?,
        "board settings has a Shape section: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Circle")?;
    assert_eq!(
        app.project.board.outline.as_ref().map(Vec::len),
        Some(4),
        "picking Circle traces a round outline: {}",
        app.message
    );
    type_into(&ctx, &mut app, "spinbutton:Shape diameter", "40")?;
    let b = &app.project.board;
    assert!(
        (b.width - 40.0).abs() < 1e-6 && (b.height - 40.0).abs() < 1e-6,
        "a 40 mm diameter makes a 40 by 40 board, got {} by {}: {}",
        b.width,
        b.height,
        app.message
    );
    let want = std::f64::consts::PI * 400.0;
    assert!(
        (ring_area(&app) - want).abs() / want < 0.001,
        "the board area is that of a 40 mm disc"
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Area")?,
        "board settings shows the live area: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_makes_an_l_shaped_board() -> Result<(), String> {
    let (ctx, mut app) = board_settings()?;
    assert!(
        scrolled_to(&ctx, &mut app, "SHAPE")?,
        "board settings has a Shape section: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "L shape")?;
    assert!(
        scrolled_to(&ctx, &mut app, "Notch width")?,
        "the L shape asks for its notch size: {}",
        app.message
    );
    type_into(&ctx, &mut app, "spinbutton:Shape width", "50")?;
    type_into(&ctx, &mut app, "spinbutton:Shape height", "30")?;
    type_into(&ctx, &mut app, "spinbutton:Shape notch width", "20")?;
    type_into(&ctx, &mut app, "spinbutton:Shape notch height", "10")?;
    assert_eq!(
        app.project.board.outline.as_ref().map(Vec::len),
        Some(6),
        "an L has six corners: {}",
        app.message
    );
    assert!(
        (ring_area(&app) - 1300.0).abs() < 1e-6,
        "the notch comes out of the area: {}",
        ring_area(&app)
    );
    // The notch is empty board: a point in it is outside the real shape.
    let in_notch = Point::new(45.0, 25.0);
    let outline = app.project.board.outline.clone().unwrap_or_default();
    assert!(
        !pcbcraft_geom::point_in_polygon(in_notch, &outline),
        "the notch corner is outside the outline"
    );
    // Edge clearance follows the real shape: copper in the notch is off the
    // board even though it is inside the bounding rectangle.
    app.project.nets.push(pcbcraft_doc::Net {
        id: 1,
        name: "gnd".into(),
        ..Default::default()
    });
    let track = |x0: f64, x1: f64| pcbcraft_doc::Track {
        preserved: pcbcraft_doc::PreservedSource::default(),
        id: 9_001,
        net: 1,
        start: Point::new(x0, 5.0),
        end: Point::new(x1, 5.0),
        width: 0.2,
        layer: pcbcraft_doc::Layer::Front,
        arc: None,
    };
    app.project.board.tracks = vec![track(10.0, 15.0)];
    assert!(
        !pcbcraft_rules::drc(&app.project)
            .iter()
            .any(|f| f.code == "board_edge"),
        "copper well inside the L is clean"
    );
    app.project.board.tracks = vec![pcbcraft_doc::Track {
        start: Point::new(45.0, 25.0),
        end: Point::new(48.0, 25.0),
        ..track(0.0, 0.0)
    }];
    assert!(
        pcbcraft_rules::drc(&app.project)
            .iter()
            .any(|f| f.code == "board_edge"),
        "copper in the notch is reported as off the board"
    );
    Ok(())
}

#[test]
fn a_newcomer_edits_a_vertex_in_the_vertex_table() -> Result<(), String> {
    let (ctx, mut app) = board_settings()?;
    assert!(
        scrolled_to(&ctx, &mut app, "SHAPE")?,
        "board settings has a Shape section: {}",
        app.message
    );
    // Pick a preset that makes the outline editable.
    authoring::tests::tap(&ctx, &mut app, "L shape")?;
    assert!(
        scrolled_to(&ctx, &mut app, "Point 1")?,
        "the vertex table appears after picking a shape: {}",
        app.message
    );
    // Record the outline, then type a new X into a vertex row.
    let before = app.project.board.outline.clone().unwrap_or_default();
    type_into(&ctx, &mut app, "spinbutton:X of point #", "5")?;
    let after = app.project.board.outline.clone().unwrap_or_default();
    assert_eq!(
        before.len(),
        after.len(),
        "editing a vertex keeps the point count"
    );
    // Digits are folded in control ids, so which row the typing reaches is the helper's choice.
    // Exactly one vertex moved, to the typed X, and its Y stayed where it was.
    let moved: Vec<usize> = (0..before.len())
        .filter(|i| {
            (before[*i].x - after[*i].x).abs() > 1e-6 || (before[*i].y - after[*i].y).abs() > 1e-6
        })
        .collect();
    assert_eq!(moved.len(), 1, "exactly one vertex moved: {}", app.message);
    let i = moved[0];
    assert!(
        (after[i].x - 5.0).abs() < 1e-6,
        "the vertex moved to the typed X: {}",
        app.message
    );
    assert!(
        (after[i].y - before[i].y).abs() < 1e-6,
        "the vertex kept its Y"
    );
    Ok(())
}
