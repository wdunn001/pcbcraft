//! Stories for adding pads from the Layout and Schematic canvases (issue 1164).
//!
//! Context: the Pad tool in the Layout strip, Add pad on a placed part, and the
//! Pad item in the Schematic strip. Each story drives the real window and then
//! asserts what the design, the nets and the outputs say.
use super::super::*;
use super::board::right_click;
use super::support::*;

fn sample_app(tab: Tab) -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(tab);
    app.needs_fit = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}

fn pad_part(app: &App) -> Option<&pcbcraft_doc::Footprint> {
    app.project.board.footprints.iter().find(|f| {
        app.project
            .component(f.component)
            .is_some_and(|c| c.kind == "test_point")
    })
}

fn pick_net(ctx: &egui::Context, app: &mut App, name: &str) -> Result<(), String> {
    tap_properties(ctx, app, "combobox:Net")?;
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(ctx, app, name)?;
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

fn members_text(app: &App, net: Id) -> String {
    app.session
        .lock()
        .ok()
        .and_then(|s| s.peek("net.members", json!({"net": net})).ok())
        .map(|v| v.to_string())
        .unwrap_or_default()
}

#[test]
fn a_newcomer_adds_a_pad_to_the_board() -> Result<(), String> {
    let (ctx, mut app) = sample_app(Tab::Pcb);
    let parts = app.project.board.footprints.len();
    tap_strip(&ctx, &mut app, "button:Pad")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.canvas_click(Point::new(-25.0, -17.0), 1.0);
    assert_eq!(
        app.project.board.footprints.len(),
        parts + 1,
        "one click drops one pad: {}",
        app.message
    );
    // Back to Select so Properties shows the pad that was just dropped.
    tap_strip(&ctx, &mut app, "button:Select")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let id = pad_part(&app).map(|f| f.id).ok_or("no pad part")?;
    app.select(Some(id));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    type_into(&ctx, &mut app, "textinput:Number", "T1")?;
    type_into(&ctx, &mut app, "spinbutton:Width", "2.5")?;
    tap_properties(&ctx, &mut app, "combobox:Shape")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Rectangle")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    type_into(&ctx, &mut app, "spinbutton:Height", "1.2")?;
    let net = app.project.nets.first().ok_or("no net")?.clone();
    pick_net(&ctx, &mut app, &net.name)?;
    let f = pad_part(&app).ok_or("pad part gone")?;
    let pad = f.pads.first().ok_or("no pad")?;
    assert_eq!(
        pad.number, "T1",
        "number set in Properties: {}",
        app.message
    );
    let g = pad.geometry.as_ref().ok_or("no geometry")?;
    assert_eq!(g.shape, "rect");
    assert!((g.size.x - 2.5).abs() < 1e-9 && (g.size.y - 1.2).abs() < 1e-9);
    assert_eq!(
        pad.net,
        Some(net.id),
        "net set in Properties: {}",
        app.message
    );
    let reference = app
        .project
        .component(f.component)
        .map(|c| c.reference.clone())
        .ok_or("no part")?;
    assert!(
        members_text(&app, net.id).contains(&reference),
        "the net counts the new pad: {}",
        members_text(&app, net.id)
    );
    // The Layers tree lists the new part.
    app.region().left_tab = "layers".to_owned();
    open_tree_row(&ctx, &mut app, "components");
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, &reference)?,
        "the Layers tree lists the pad part {reference}"
    );
    // Undo walks the edits back and finally the pad itself.
    for _ in 0..8 {
        if pad_part(&app).is_none() {
            break;
        }
        app.tool_call("undo", json!({}));
    }
    assert!(pad_part(&app).is_none(), "undo removes the pad");
    assert_eq!(app.project.board.footprints.len(), parts);
    Ok(())
}

#[test]
fn a_newcomer_adds_a_through_hole_pad_with_a_drill() -> Result<(), String> {
    let (ctx, mut app) = sample_app(Tab::Pcb);
    tap_strip(&ctx, &mut app, "button:Pad")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_properties(&ctx, &mut app, "combobox:Side")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Through-hole")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    type_into(&ctx, &mut app, "spinbutton:Width", "2")?;
    type_into(&ctx, &mut app, "spinbutton:Drill", "1.1")?;
    let before = pcbcraft_io::drill_table(&app.project)?.len();
    app.canvas_click(Point::new(25.0, -17.0), 1.0);
    let f = pad_part(&app).ok_or_else(|| format!("no pad dropped: {}", app.message))?;
    let g = f.pads[0].geometry.as_ref().ok_or("no geometry")?;
    assert_eq!(g.pad_type, "thru_hole");
    assert!(
        g.drill
            .as_ref()
            .is_some_and(|d| (d.size.x - 1.1).abs() < 1e-9)
    );
    let rows = pcbcraft_io::drill_table(&app.project)?;
    assert!(rows.len() > before, "the drill table gains a row");
    assert!(
        rows.iter()
            .any(|r| r.plated && (r.diameter_mm - 1.1).abs() < 1e-6),
        "the drill output lists the 1.1 mm plated hole"
    );
    Ok(())
}

#[test]
fn a_newcomer_adds_a_pad_to_a_placed_part() -> Result<(), String> {
    let (ctx, mut app) = sample_app(Tab::Pcb);
    let f = app.project.board.footprints[0].clone();
    let pads = f.pads.len();
    let component = f.component;
    let screen = pcb_screen(&app, Point::new(f.position.x, f.position.y));
    right_click(&ctx, &mut app, screen);
    assert!(
        paints_text(&ctx, &mut app, "Add pad")?,
        "the part's menu offers Add pad: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Add pad")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let after = app
        .project
        .board
        .footprints
        .iter()
        .find(|q| q.id == f.id)
        .ok_or("part gone")?;
    assert_eq!(
        after.pads.len(),
        pads + 1,
        "the part has the extra pad: {}",
        app.message
    );
    let pins = app.project.component(component).map_or(0, |c| c.pins.len());
    assert_eq!(pins, pads + 1, "the part gained the matching pin");
    app.tool_call("undo", json!({}));
    let back = app
        .project
        .board
        .footprints
        .iter()
        .find(|q| q.id == f.id)
        .map_or(0, |q| q.pads.len());
    assert_eq!(back, pads, "undo takes the pad off again");
    Ok(())
}

#[test]
fn a_newcomer_places_a_test_pad_in_the_schematic() -> Result<(), String> {
    let (ctx, mut app) = sample_app(Tab::Schematic);
    let parts = app.project.components.len();
    tap_strip(&ctx, &mut app, "icon:tool-pad")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.canvas_click(Point::new(90.0, 60.0), 1.0);
    assert_eq!(
        app.project.components.len(),
        parts + 1,
        "the Pad item adds one part to the sheet: {}",
        app.message
    );
    let pad = app
        .project
        .components
        .iter()
        .find(|c| c.kind == "test_point")
        .cloned()
        .ok_or("no pad part")?;
    assert_eq!(pad.pins.len(), 1, "the pad part has one connectable pin");
    // Wire it to a pin that already sits on a net.
    let (other, pin, net) = app
        .project
        .components
        .iter()
        .filter(|c| c.id != pad.id)
        .find_map(|c| {
            c.pins
                .iter()
                .find_map(|p| p.net.map(|n| (c.id, p.number.clone(), n)))
        })
        .ok_or("no wired pin in the sample")?;
    app.tool = Tool::Select;
    app.run(
        "wire.connect",
        json!({"from":{"component":pad.id,"pin":"1"},"to":{"component":other,"pin":pin}}),
    );
    let on_board = app
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.component == pad.id)
        .ok_or_else(|| format!("the pad is not on the board: {}", app.message))?;
    assert_eq!(on_board.pads.len(), 1);
    assert_eq!(
        on_board.pads[0].net,
        Some(net),
        "the board pad is on the wired net"
    );
    assert!(members_text(&app, net).contains(&pad.reference));
    Ok(())
}

/// Flash operations in a Gerber file: one per pad opening.
fn flashes(gerber: &str) -> usize {
    gerber.lines().filter(|l| l.ends_with("D03*")).count()
}

/// Pick a value in a Properties drop-down by its label and visible option.
fn choose(ctx: &egui::Context, app: &mut App, combo: &str, option: &str) -> Result<(), String> {
    tap_properties(ctx, app, combo)?;
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(ctx, app, option)?;
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

#[test]
fn a_newcomer_draws_an_exposed_pad_with_the_mask_open() -> Result<(), String> {
    let (ctx, mut app) = sample_app(Tab::Pcb);
    let mask_before = flashes(&pcbcraft_io::gerber_mask(
        &app.project,
        pcbcraft_doc::Layer::Front,
    )?);
    let paste_before = flashes(&pcbcraft_io::gerber_paste(
        &app.project,
        pcbcraft_doc::Layer::Front,
    )?);
    tap_strip(&ctx, &mut app, "button:Pad")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    choose(&ctx, &mut app, "combobox:Kind", "Exposed")?;
    choose(&ctx, &mut app, "combobox:Shape", "Rectangle")?;
    type_into(&ctx, &mut app, "spinbutton:Width", "2")?;
    type_into(&ctx, &mut app, "spinbutton:Height", "1")?;
    let board = &app.project.board;
    app.canvas_click(Point::new(6.0, board.height - 5.0), 1.0);
    let f = pad_part(&app).ok_or_else(|| format!("no pad dropped: {}", app.message))?;
    let id = f.id;
    let g = f.pads[0].geometry.as_ref().ok_or("no geometry")?;
    assert_eq!(
        g.kind(),
        pcbcraft_library::PadKind::Exposed,
        "the dropped pad is exposed: {}",
        app.message
    );
    assert!(g.mask_open(), "the mask is open over the pad");
    assert!(!g.has_paste(), "an exposed pad takes no paste");
    let mask = pcbcraft_io::gerber_mask(&app.project, pcbcraft_doc::Layer::Front)?;
    assert_eq!(
        flashes(&mask),
        mask_before + 1,
        "the mask output has the opening"
    );
    let paste = pcbcraft_io::gerber_paste(&app.project, pcbcraft_doc::Layer::Front)?;
    assert_eq!(
        flashes(&paste),
        paste_before,
        "the paste output skips the exposed pad"
    );
    let copper = pcbcraft_io::gerber_copper(&app.project, pcbcraft_doc::Layer::Front)?;
    assert!(
        copper.contains("R,2X1"),
        "the copper output has the 2 by 1 pad"
    );
    let own: Vec<_> = pcbcraft_rules::drc(&app.project)
        .into_iter()
        .filter(|finding| finding.objects.contains(&id) && finding.severity == "error")
        .collect();
    assert!(own.is_empty(), "the exposed pad raises no error: {own:?}");
    Ok(())
}

#[test]
fn a_newcomer_draws_a_notched_pad() -> Result<(), String> {
    let (ctx, mut app) = sample_app(Tab::Pcb);
    let paste_before = flashes(&pcbcraft_io::gerber_paste(
        &app.project,
        pcbcraft_doc::Layer::Front,
    )?);
    tap_strip(&ctx, &mut app, "button:Pad")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    choose(&ctx, &mut app, "combobox:Kind", "Notched")?;
    choose(&ctx, &mut app, "combobox:Notch place", "Top edge")?;
    type_into(&ctx, &mut app, "spinbutton:Width", "2")?;
    type_into(&ctx, &mut app, "spinbutton:Height", "1.4")?;
    type_into(&ctx, &mut app, "spinbutton:Notch width", "0.6")?;
    type_into(&ctx, &mut app, "spinbutton:Notch depth", "0.4")?;
    let board = &app.project.board;
    app.canvas_click(Point::new(12.0, board.height - 5.0), 1.0);
    let f = pad_part(&app).ok_or_else(|| format!("no pad dropped: {}", app.message))?;
    let id = f.id;
    let g = f.pads[0].geometry.as_ref().ok_or("no geometry")?;
    assert_eq!(
        g.kind(),
        pcbcraft_library::PadKind::Notched,
        "{}",
        app.message
    );
    assert_eq!(g.polygon.len(), 8, "the outline has the notch");
    let (place, w, d) = g.notch().ok_or("the notch reads back")?;
    assert_eq!(place, pcbcraft_library::NotchPlace::EdgeN);
    assert!((w - 0.6).abs() < 1e-9 && (d - 0.4).abs() < 1e-9);
    // A save and load keeps the notch.
    let saved = serde_json::to_string(&app.project).map_err(|e| e.to_string())?;
    let loaded: pcbcraft_doc::Project = serde_json::from_str(&saved).map_err(|e| e.to_string())?;
    let back = loaded
        .board
        .footprints
        .iter()
        .find(|q| q.id == id)
        .and_then(|q| q.pads.first())
        .and_then(|q| q.geometry.as_ref())
        .ok_or("pad lost on load")?;
    assert_eq!(
        back.polygon,
        g.polygon.clone(),
        "the notch survives a save and load"
    );
    // The copper and the mask both draw the cut outline; paste prints on it.
    let copper = pcbcraft_io::gerber_copper(&app.project, pcbcraft_doc::Layer::Front)?;
    assert!(
        copper.contains("%AMPAD"),
        "the copper output draws the outline"
    );
    let mask = pcbcraft_io::gerber_mask(&app.project, pcbcraft_doc::Layer::Front)?;
    assert!(
        mask.contains("%AMPAD"),
        "the mask opening follows the outline"
    );
    let paste = pcbcraft_io::gerber_paste(&app.project, pcbcraft_doc::Layer::Front)?;
    assert_eq!(
        flashes(&paste),
        paste_before + 1,
        "a notched pad keeps its paste"
    );
    let own: Vec<_> = pcbcraft_rules::drc(&app.project)
        .into_iter()
        .filter(|finding| finding.objects.contains(&id) && finding.severity == "error")
        .collect();
    assert!(own.is_empty(), "the notched pad raises no error: {own:?}");
    // A notch finer than the shop can etch is reported.
    app.run(
        "pad.edit",
        json!({"id": id, "pad": "1", "notch_width": 0.06, "notch_place": "edge_n"}),
    );
    assert!(
        pcbcraft_rules::drc(&app.project)
            .iter()
            .any(|finding| finding.code == "notch_feature" && finding.objects.contains(&id)),
        "a 0.06 mm slot is reported: {}",
        app.message
    );
    Ok(())
}
