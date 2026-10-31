use super::super::*;
use super::support::*;

fn open_3d() -> Result<(egui::Context, App, Id), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.refresh();
    let j1 = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "J1")
        .map(|c| c.id)
        .ok_or("the practice design should carry J1")?;
    app.change_tab(Tab::Board3d);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    Ok((ctx, app, j1))
}

fn drag(ctx: &egui::Context, app: &mut App, from: egui::Pos2, by: egui::Vec2) {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(from)],
        egui::Modifiers::NONE,
    );
    frame(ctx, app, vec![button(from, true)], egui::Modifiers::NONE);
    for step in 1..=6 {
        let pos = from + by * (step as f32 / 6.0);
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(pos)],
            egui::Modifiers::NONE,
        );
    }
    frame(
        ctx,
        app,
        vec![button(from + by, false)],
        egui::Modifiers::NONE,
    );
    frame(ctx, app, vec![], egui::Modifiers::NONE);
}

#[test]
fn a_newcomer_opens_the_3d_view_and_sees_the_parts() -> Result<(), String> {
    let (ctx, mut app, _) = open_3d()?;
    assert!(
        paints_text(&ctx, &mut app, "Drag to turn the board.")?,
        "the 3D tab shows its hint with no GPU renderer"
    );
    assert!(
        paints_text(&ctx, &mut app, "J1")?,
        "the Layers tree names the connector"
    );
    assert!(
        paints_text(&ctx, &mut app, "default boxes")?,
        "the tab says bodies are default boxes"
    );
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        output
            .shapes
            .iter()
            .any(|s| matches!(s.shape, egui::Shape::Mesh(_))),
        "the board is drawn as triangles"
    );
    Ok(())
}

#[test]
fn a_newcomer_turns_the_board_around_in_3d() -> Result<(), String> {
    let (ctx, mut app, _) = open_3d()?;
    let before = (app.view3d.yaw, app.view3d.pitch);
    let rect = app.view3d.rect;
    assert!(rect.width() > 100.0, "the viewport has a size");
    drag(&ctx, &mut app, rect.center(), egui::vec2(80.0, 30.0));
    let after = (app.view3d.yaw, app.view3d.pitch);
    assert!(
        (after.0 - before.0).abs() > 0.1,
        "dragging sideways turns the board: {before:?} -> {after:?}"
    );
    assert!(
        (after.1 - before.1).abs() > 0.05,
        "dragging down tilts the board: {before:?} -> {after:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_clicks_a_part_in_3d_and_sees_its_properties() -> Result<(), String> {
    let (ctx, mut app, j1) = open_3d()?;
    let (lo, hi) = app
        .view3d
        .body_bounds(j1)
        .ok_or("J1 should have a body in the mesh")?;
    let top = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, hi[2]];
    let (pos, _) = app
        .view3d
        .project_point(app.view3d.rect, top)
        .ok_or("the top of J1 should be in view")?;
    assert!(app.selected != Some(j1), "nothing is selected at first");
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.selected, Some(j1), "a click on J1 selects J1");
    assert!(
        paints_text(&ctx, &mut app, "5V input")?,
        "the inspector shows J1's value"
    );
    Ok(())
}

#[test]
fn a_newcomer_picks_a_part_from_the_layers_tree_in_3d() -> Result<(), String> {
    let (ctx, mut app, j1) = open_3d()?;
    tap_left(&ctx, &mut app, "J1")?;
    let reference = app
        .selected
        .and_then(|id| app.project.footprint(id).map(|f| f.component))
        .or(app.selected)
        .and_then(|id| app.project.component(id))
        .map(|c| c.reference.clone());
    assert_eq!(
        reference.as_deref(),
        Some("J1"),
        "a Parts row selects the part in 3D too (J1 is {j1}, selected {:?})",
        app.selected
    );
    Ok(())
}

/// Give J1 a footprint that names `model_path`, then open the 3D tab and let
/// the loader settle. The edit goes through the session so a refresh keeps it.
fn open_3d_with_model(model_path: &str) -> Result<(egui::Context, App, Id), String> {
    let (ctx, mut app, j1) = open_3d()?;
    {
        let mut s = app.session.lock().map_err(|_| "session lock")?;
        let mut pads = vec![];
        for f in s
            .project
            .board
            .footprints
            .iter_mut()
            .filter(|f| f.component == j1)
        {
            for p in &mut f.pads {
                let mut g = authoring::new_pad(&p.number, p.diameter);
                g.position = p.offset;
                g.size = pcbcraft_geom::Point::new(p.diameter, p.diameter);
                p.geometry = Some(g.clone());
                pads.push(g);
            }
        }
        let c = s
            .project
            .components
            .iter_mut()
            .find(|c| c.id == j1)
            .ok_or("J1 vanished")?;
        c.library_footprint = Some(pcbcraft_doc::LibraryFootprint {
            library: "test".into(),
            definition: pcbcraft_library::FootprintDefinition {
                name: "WithModel".into(),
                description: String::new(),
                keywords: String::new(),
                pads,
                graphics: vec![],
                model: Some(pcbcraft_library::Model3D {
                    path: model_path.into(),
                    offset: [0.0; 3],
                    scale: [1.0; 3],
                    rotate: [0.0; 3],
                    cad_part: None,
                    raw_source: String::new(),
                }),
                rules: None,
                raw_source: String::new(),
                unsupported: vec![],
                retained: vec![],
            },
        });
        s.revision += 1;
    }
    app.refresh();
    for _ in 0..200 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        if app.view3d.model_status(j1) != Some(pcbcraft_io::ModelStatus::Loading) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    if let Some(e) = app.view3d.build_error() {
        return Err(format!("the 3D view could not be built: {e}"));
    }
    Ok((ctx, app, j1))
}

fn scratch_file(name: &str, bytes: &[u8]) -> Result<String, String> {
    let dir = std::env::temp_dir().join(format!("pcbcraft-3d-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(name);
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    file.to_str().map(str::to_string).ok_or("path".into())
}

#[test]
fn a_newcomer_sees_the_real_part_model_in_3d() -> Result<(), String> {
    let (_, _, j1) = open_3d()?;
    let box_triangles = {
        let (_, app, _) = open_3d()?;
        app.view3d.body_triangles(j1).ok_or("J1 box")?
    };
    let path = scratch_file(
        "wedge.gltf",
        include_bytes!("../../../io/tests/fixtures/wedge.gltf"),
    )?;
    let (_ctx, app, j1) = open_3d_with_model(&path)?;
    assert_eq!(
        app.view3d.model_status(j1),
        Some(pcbcraft_io::ModelStatus::Loaded)
    );
    let triangles = app.view3d.body_triangles(j1).ok_or("J1 body")?;
    assert_eq!(
        triangles, 2,
        "the wedge has two faces, the box had {box_triangles}"
    );
    assert_ne!(triangles, box_triangles);
    assert_eq!(app.view3d.body_family(j1).as_deref(), Some("model"));
    let (lo, hi) = app.view3d.body_bounds(j1).ok_or("bounds")?;
    assert!(
        (hi[0] - lo[0] - 4.0).abs() < 1e-3 && (hi[1] - lo[1] - 3.0).abs() < 1e-3,
        "the 4 by 3 millimetre wedge sits on the board: {lo:?} {hi:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_why_a_model_did_not_load() -> Result<(), String> {
    let path = scratch_file("broken.gltf", b"this is not a model")?;
    let (ctx, mut app, j1) = open_3d_with_model(&path)?;
    assert!(
        matches!(
            app.view3d.model_status(j1),
            Some(pcbcraft_io::ModelStatus::Failed(_))
        ),
        "a corrupt file is a failure, not a crash"
    );
    assert_eq!(
        app.view3d.body_family(j1).as_deref().map(|f| f != "model"),
        Some(true),
        "the default box stays"
    );
    assert!(
        paints_text(&ctx, &mut app, "Box kept.")?,
        "the 3D tab says the box was kept"
    );
    assert!(
        paints_text(&ctx, &mut app, "could not be read")?,
        "the 3D tab says why in plain words"
    );
    Ok(())
}

#[test]
fn a_newcomer_with_many_missing_models_still_sees_the_board() -> Result<(), String> {
    let (ctx, mut app, _j1) = open_3d()?;
    let before = app.view3d.rect.height();
    for n in 0..150u64 {
        app.view3d
            .note_failed(10_000 + n, "The model file part.step was not found.");
    }
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        paints_text(&ctx, &mut app, "150 part models not found")?,
        "one line says how many models are missing"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Box kept.")?,
        "the long list is not painted until it is asked for"
    );
    let after = app.view3d.rect.height();
    assert!(
        after > 200.0 && after + 1.0 >= before - 40.0,
        "the board viewport stays tall: {before} before, {after} after"
    );
    app.view3d.notes_open = true;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        app.view3d.rect.height() > 120.0,
        "an open list is bounded and leaves the board visible"
    );
    Ok(())
}

#[test]
fn a_newcomer_looks_from_the_top_and_resets_the_3d_view() -> Result<(), String> {
    let (ctx, mut app, _j1) = open_3d()?;
    app.view3d.yaw = 0.3;
    tap_id(
        &ctx,
        &mut app,
        "button:Top view. Look straight down at the board.",
    )?;
    assert!(
        (app.view3d.yaw + std::f64::consts::FRAC_PI_2).abs() < 1e-9,
        "Top view looks straight down at the board"
    );
    app.view3d.yaw = 0.9;
    tap_id(
        &ctx,
        &mut app,
        "button:Reset view. Fit the whole board in the window again.",
    )?;
    assert!(
        (app.view3d.yaw - 0.9).abs() > 1e-9,
        "Reset view frames the board again"
    );
    Ok(())
}
