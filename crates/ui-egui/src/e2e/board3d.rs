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
    open_3d_with_model_part(model_path, None)
}

/// [`open_3d_with_model`] where the footprint also names one part inside the file.
fn open_3d_with_model_part(
    model_path: &str,
    cad_part: Option<&str>,
) -> Result<(egui::Context, App, Id), String> {
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
                    cad_part: cad_part.map(str::to_owned),
                    raw_source: String::new(),
                    body_id: Default::default(),
                    embedded: Default::default(),
                }),
                rules: None,
                box_body: None,
                raw_source: String::new(),
                unsupported: vec![],
                retained: vec![],
            },
        });
        s.revision += 1;
    }
    app.refresh();
    // The model file loads on a worker thread. Until the view has noted the
    // part there is no status at all, and then it reads Loading, so wait for
    // an answer (loaded or failed) or for the build to report its error.
    waits_until(&ctx, &mut app, |app| {
        app.view3d.build_error().is_some()
            || !matches!(
                app.view3d.model_status(j1),
                None | Some(pcbcraft_io::ModelStatus::Loading)
            )
    });
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

/// A model file with two parts: a 1 by 1 millimetre triangle on the root and a
/// 4 by 3 millimetre triangle on the part named "Lid".
fn two_part_model_file() -> String {
    json!({
        "format": "solidcraft/22",
        "name": "Two parts",
        "units": "mm",
        "next_id": 2,
        "components": [
            {"id": 0, "name": "Root", "parent": 0},
            {"id": 5, "name": "Lid", "parent": 0}
        ],
        "features": [
            {"id": 1, "name": "Base", "type": "mesh_import", "file": "t",
             "meshes": [{"name": "Base",
                "positions": [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                "triangles": [0, 1, 2]}]},
            {"id": 2, "name": "Lid", "type": "mesh_import", "file": "t", "component": 5,
             "meshes": [{"name": "Lid",
                "positions": [0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 3.0, 0.0],
                "triangles": [0, 1, 2]}]}
        ]
    })
    .to_string()
}

#[test]
fn a_newcomer_sees_the_named_part_of_a_model_file_in_3d() -> Result<(), String> {
    let path = scratch_file("two-parts.solidcraft", two_part_model_file().as_bytes())?;
    let (_ctx, app, j1) = open_3d_with_model_part(&path, Some("Lid"))?;
    assert_eq!(
        app.view3d.model_status(j1),
        Some(pcbcraft_io::ModelStatus::Loaded),
        "the named part loads from the file"
    );
    assert_eq!(app.view3d.body_family(j1).as_deref(), Some("model"));
    let (lo, hi) = app.view3d.body_bounds(j1).ok_or("bounds")?;
    assert!(
        (hi[0] - lo[0] - 4.0).abs() < 1e-3 && (hi[1] - lo[1] - 3.0).abs() < 1e-3,
        "the 4 by 3 millimetre Lid is on the board, not the root: {lo:?} {hi:?}"
    );
    let (_ctx, whole, j1) = open_3d_with_model_part(&path, None)?;
    let (lo, hi) = whole.view3d.body_bounds(j1).ok_or("bounds")?;
    assert!(
        (hi[0] - lo[0] - 1.0).abs() < 1e-3 && (hi[1] - lo[1] - 1.0).abs() < 1e-3,
        "without a part name the root loads: {lo:?} {hi:?}"
    );
    let output_has_mesh = {
        let ctx = egui::Context::default();
        let mut app = app;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE)
            .shapes
            .iter()
            .any(|s| matches!(s.shape, egui::Shape::Mesh(_)))
    };
    assert!(output_has_mesh, "the 3D view output draws triangles");
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
    assert!(
        app.message.contains("Model load failed") || app.message.contains("could not be read"),
        "the status line shows the model load failure: {}",
        app.message
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

#[test]
fn a_newcomer_sees_a_flipped_part_hang_below_the_board() -> Result<(), String> {
    let (ctx, mut app, _) = open_3d()?;
    // Find J1's footprint id; fall back to R1 if J1 is through-hole and flip fails.
    let component_ids: Vec<Id> = app
        .project
        .components
        .iter()
        .filter(|c| c.reference == "J1" || c.reference == "R1")
        .map(|c| c.id)
        .collect();
    let fp_id = component_ids
        .iter()
        .find_map(|&comp_id| {
            app.project
                .board
                .footprints
                .iter()
                .find(|f| f.component == comp_id)
                .map(|f| f.id)
        })
        .ok_or("the practice design should place J1 or R1")?;
    let before = app
        .view3d
        .body_bounds(component_ids[0])
        .ok_or("the first part should have a body in the mesh")?;
    let top = before.1[2];
    assert!(
        top >= 0.0,
        "the part sits on the board top face before the flip (z={top})"
    );
    app.tool_call("footprint.flip", json!({"id": fp_id}));
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let after = app
        .view3d
        .body_bounds(component_ids[0])
        .ok_or("the flipped part should still have a body in the mesh")?;
    assert!(
        after.1[2] <= 1e-6,
        "the flipped part hangs below the board bottom face (z={:.4})",
        after.1[2]
    );
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        output
            .shapes
            .iter()
            .any(|s| matches!(s.shape, egui::Shape::Mesh(_))),
        "the flipped board still draws as triangles"
    );
    app.tool_call("undo", json!({}));
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let restored = app
        .view3d
        .body_bounds(component_ids[0])
        .ok_or("the part should still have a body after undo")?;
    assert!(
        restored.0[2] >= -1e-6,
        "undo puts the part back above the board (z={:.4})",
        restored.0[2]
    );
    Ok(())
}

#[test]
fn a_newcomer_shows_an_enclosure_around_the_board() -> Result<(), String> {
    let (ctx, mut app, _) = open_3d()?;
    assert!(
        !paints_text(&ctx, &mut app, "Enclosure")?,
        "the Enclosure toggle stays hidden while the board has no enclosure"
    );
    let mesh = pcbcraft_io::board_mesh(&app.project, pcbcraft_io::MeshLayers::default())?;
    let json = pcbcraft_io::solidcraft_board(
        &app.project,
        &mesh,
        pcbcraft_io::SolidcraftLayers::default(),
    )?;
    let path =
        std::env::temp_dir().join(format!("pcbcraft-case-{}.solidcraft", std::process::id()));
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    let imported = app.tool_call(
        "board.enclosure.import",
        json!({ "path": path.to_string_lossy() }),
    );
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    assert!(imported, "the enclosure file imports");
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let visible = |app: &App| app.project.board.enclosure.as_ref().map(|e| e.visible);
    let enclosure_parts = |app: &App| app.view3d.part_count_in_family("enclosure");
    assert_eq!(
        visible(&app),
        Some(true),
        "an imported enclosure starts shown"
    );
    assert!(
        enclosure_parts(&app) > 0,
        "the 3D view draws the enclosure bodies"
    );
    tap_id(&ctx, &mut app, "checkbox:Enclosure")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(visible(&app), Some(false), "the toggle hides the enclosure");
    assert_eq!(enclosure_parts(&app), 0, "a hidden enclosure draws nothing");
    tap_id(&ctx, &mut app, "checkbox:Enclosure")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(visible(&app), Some(true), "the toggle shows it again");
    assert!(enclosure_parts(&app) > 0, "the bodies are back");
    Ok(())
}

/// Turns J1 into an eight-pin dual in-line chip marked LM358, the way a library
/// part would arrive, so its body is large enough to carry printed text.
fn make_j1_a_marked_ic(app: &mut App, j1: Id) -> Result<(), String> {
    {
        let mut s = app.session.lock().map_err(|_| "session lock")?;
        let mut geometry = vec![];
        for i in 0..4 {
            let x = -3.81 + 2.54 * f64::from(i);
            for (number, y) in [(i + 1, -3.81), (i + 5, 3.81)] {
                let mut g = authoring::new_pad(&number.to_string(), 0.0);
                g.pad_type = "thru_hole".into();
                g.layers = vec!["*.Cu".into()];
                g.position = pcbcraft_geom::Point::new(x, y);
                g.size = pcbcraft_geom::Point::new(1.6, 1.6);
                g.drill = Some(pcbcraft_library::Drill {
                    size: pcbcraft_geom::Point::new(0.8, 0.8),
                    offset: pcbcraft_geom::Point::new(0.0, 0.0),
                });
                geometry.push(g);
            }
        }
        let c = s
            .project
            .components
            .iter_mut()
            .find(|c| c.id == j1)
            .ok_or("J1 vanished")?;
        // Pins the connector already had keep their nets, so its wires stay valid.
        let old_pins = c.pins.clone();
        let pins: Vec<pcbcraft_doc::Pin> = geometry
            .iter()
            .map(|g| {
                old_pins
                    .iter()
                    .find(|p| p.number == g.number)
                    .cloned()
                    .unwrap_or_else(|| pcbcraft_doc::Pin {
                        number: g.number.clone(),
                        name: g.number.clone(),
                        offset: pcbcraft_geom::Point::new(0.0, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                        appearances: Default::default(),
                    })
            })
            .collect();
        let pads: Vec<pcbcraft_doc::Pad> = geometry
            .iter()
            .map(|g| pcbcraft_doc::Pad {
                number: g.number.clone(),
                offset: g.position,
                diameter: 1.6,
                net: pins
                    .iter()
                    .find(|p| p.number == g.number)
                    .and_then(|p| p.net),
                castellated: None,
                geometry: Some(g.clone()),
            })
            .collect();
        c.kind = "ic".into();
        c.value = "LM358".into();
        c.pins = pins;
        c.library_footprint = Some(pcbcraft_doc::LibraryFootprint {
            library: "test".into(),
            definition: pcbcraft_library::FootprintDefinition {
                name: "T".into(),
                description: String::new(),
                keywords: String::new(),
                pads: geometry,
                graphics: vec![],
                model: None,
                rules: None,
                box_body: None,
                raw_source: String::new(),
                unsupported: vec![],
                retained: vec![],
            },
        });
        for f in s
            .project
            .board
            .footprints
            .iter_mut()
            .filter(|f| f.component == j1)
        {
            f.pads = pads.clone();
        }
        s.revision += 1;
    }
    app.refresh();
    Ok(())
}

const PART_MARKING: &str = "button:Show or hide the part number or value printed on the top of ";

#[test]
fn a_newcomer_sees_the_chip_marking_in_3d_and_switches_it_off() -> Result<(), String> {
    let (ctx, mut app, j1) = open_3d()?;
    make_j1_a_marked_ic(&mut app, j1)?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    if let Some(e) = app.view3d.build_error() {
        return Err(format!("the 3D view could not be built: {e}"));
    }
    let shown = app
        .view3d
        .marking_triangles(j1)
        .ok_or("the chip should have a body in the 3D output")?;
    tap_id(&ctx, &mut app, PART_MARKING)?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let hidden = app
        .view3d
        .marking_triangles(j1)
        .ok_or("the chip should keep its body with marking off")?;
    assert!(
        shown > hidden,
        "the printed text adds triangles to the body ({shown} with it, {hidden} without)"
    );
    tap_id(&ctx, &mut app, PART_MARKING)?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(
        app.view3d.marking_triangles(j1),
        Some(shown),
        "switching the option back on prints the text again"
    );
    Ok(())
}
