use super::super::*;
use super::support::*;

/// A four-layer board with one via net, ready for span stories.
fn four_layer_app() -> Result<(App, egui::Context), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    {
        let mut session = shared.lock().map_err(|_| "session lock unavailable")?;
        for i in 1..=4 {
            session
                .project
                .board
                .layer_records
                .push(pcbcraft_doc::LayerRecord {
                    name: format!("L{i}"),
                    kind: "copper".into(),
                    thickness: 0.035,
                    copper_role: None,
                });
            if i < 4 {
                session
                    .project
                    .board
                    .layer_records
                    .push(pcbcraft_doc::LayerRecord {
                        name: format!("P{i}"),
                        kind: "prepreg".into(),
                        thickness: 0.4,
                        copper_role: None,
                    });
            }
        }
    }
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.change_tab(Tab::Pcb);
    app.run("net.create", json!({"name": "SIG"}));
    app.refresh();
    Ok((app, egui::Context::default()))
}

fn place_via(app: &mut App, extra: Value) -> Result<Id, String> {
    let net = app.project.nets.first().ok_or("net should exist")?.id;
    let mut params = json!({
        "net": net,
        "position": {"x": 10.0, "y": 10.0},
        "diameter": 0.6,
        "drill": 0.3
    });
    if let (Some(p), Some(e)) = (params.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            p.insert(k.clone(), v.clone());
        }
    }
    app.run("via.place", params);
    app.selected
        .ok_or_else(|| format!("the via should be selected: {}", app.message))
}

#[test]
fn a_newcomer_sees_a_blind_via_span_in_the_inspector() -> Result<(), String> {
    let (mut app, ctx) = four_layer_app()?;
    place_via(&mut app, json!({"to_layer": "L2"}))?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "L1 to L2")?,
        "the inspector names the layers the via joins: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Blind")?,
        "the inspector says what kind of via it is: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_moves_the_end_of_a_via_with_the_layer_picker() -> Result<(), String> {
    let (mut app, ctx) = four_layer_app()?;
    let id = place_via(&mut app, json!({}))?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "End layer")?,
        "the inspector offers an end layer: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "L4")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "L2")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let via = app
        .project
        .board
        .vias
        .iter()
        .find(|v| v.id == id)
        .ok_or("the via should still exist")?;
    assert_eq!(
        via.to_layer.map(|l| l.index()),
        Some(1),
        "picking L2 shortens the via: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Blind")?,
        "the kind follows the new span: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_is_warned_about_a_via_too_deep_for_its_drill() -> Result<(), String> {
    let (mut app, ctx) = four_layer_app()?;
    place_via(&mut app, json!({"drill": 0.1, "diameter": 0.3}))?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "hard to plate")?,
        "a deep, narrow via gets a plain warning: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_allows_and_removes_a_buried_via_span() -> Result<(), String> {
    let (mut app, ctx) = four_layer_app()?;
    app.select(None);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    open_layers_aside(&ctx, &mut app)?;
    scrolled_to(&ctx, &mut app, "MOUNTING HOLES")?;
    assert!(
        scrolled_to(&ctx, &mut app, "VIA SPANS")?,
        "the board lists the spans a via may use: {}",
        app.message
    );
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("via_span_add"), (1usize, 2usize)));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Allow span")?,
        "the board offers a way to allow a span: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Allow span")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.via_spans,
        vec![(1, 2)],
        "the buried span is now allowed: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "L2 to L3")?,
        "the new span shows in the list: {}",
        app.message
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Remove")?,
        "an allowed extra span can be taken away again: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Remove")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.board.via_spans.is_empty(),
        "removing it puts the list back: {}",
        app.message
    );
    Ok(())
}
