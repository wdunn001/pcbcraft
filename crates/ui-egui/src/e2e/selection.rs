//! Stories for the one selection value: pick in the Layers tree, on the
//! canvas or in Manufacture and see the others follow, with Properties
//! showing the layer record.
use super::super::*;
use super::layers::pcb_app;
use super::support::*;
use super::vias::{four_layer_app, place_via};
use crate::selection::{Level, Selection};

/// A two-layer practice board with one extra net routed on both sides and
/// joined by a via.
fn routed() -> Result<(egui::Context, App, Id, Id), String> {
    let (ctx, mut app) = pcb_app();
    app.run("net.create", json!({"name": "SIGX"}));
    app.refresh();
    let net = app
        .project
        .nets
        .iter()
        .find(|n| n.name == "SIGX")
        .map(|n| n.id)
        .ok_or("the new net should exist")?;
    for layer in ["F.Cu", "B.Cu"] {
        app.run(
            "track.add",
            json!({"net": net, "start": [5.0, 5.0], "end": [15.0, 5.0], "width": 0.3, "layer": layer}),
        );
    }
    app.run(
        "via.place",
        json!({"net": net, "position": {"x": 15.0, "y": 5.0}, "diameter": 0.6, "drill": 0.3}),
    );
    let via = app.selected.ok_or("the via should be selected")?;
    app.select(None);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((ctx, app, net, via))
}

/// The name of the first tree row of a via: its label and span, with its net
/// row opened so the row is on screen.
fn via_button(app: &mut App, id: Id) -> Result<String, String> {
    let via = app
        .project
        .board
        .vias
        .iter()
        .find(|v| v.id == id)
        .cloned()
        .ok_or("the via should exist")?;
    app.tree
        .expanded
        .insert(format!("side:top/copper/net:{}", via.net));
    let name = format!(
        "button:Via {:.2} mm, {}",
        via.drill,
        crate::via_span::via_tree_span(&app.project, &via)
    );
    // Control names carry a number as a hash sign.
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_digit() {
            if !out.ends_with('#') {
                out.push('#');
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

#[test]
fn a_newcomer_selects_a_net_in_the_tree_and_sees_its_copper_on_every_layer() -> Result<(), String> {
    let (ctx, mut app, net, via) = routed()?;
    tap_id(&ctx, &mut app, "button:SIGX")?;
    assert_eq!(
        app.subject,
        Selection::Net(net),
        "pressing a net row selects that net: {}",
        app.message
    );
    assert_eq!(
        app.highlight_net,
        Some(net),
        "the canvas lights the net's copper"
    );
    let rel = crate::selection::relations(&app.project, &app.subject);
    let layers: Vec<u8> = rel.layers.iter().flatten().copied().collect();
    assert_eq!(
        layers,
        vec![0, 1],
        "the net runs on the top and bottom layers"
    );
    assert!(
        rel.objects.contains(&via)
            && app
                .project
                .board
                .tracks
                .iter()
                .all(|t| t.net != net || rel.objects.contains(&t.id)),
        "its via and both tracks light weakly"
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Connection")?,
        "Properties shows the net: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_selects_a_via_and_sees_which_layers_it_crosses() -> Result<(), String> {
    let (mut app, ctx) = four_layer_app()?;
    let via = place_via(&mut app, json!({"to_layer": "L2"}))?;
    app.select(None);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let button = via_button(&mut app, via)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_id(&ctx, &mut app, &button)?;
    assert_eq!(
        app.subject,
        Selection::Object(via),
        "the via row selects the via"
    );
    let rel = crate::selection::relations(&app.project, &app.subject);
    let layers: Vec<u8> = rel.layers.iter().flatten().copied().collect();
    assert_eq!(layers, vec![0, 1], "a via from L1 to L2 crosses two layers");
    let hidden: Vec<String> = vec![];
    let env = layers_tree::Env {
        project: &app.project,
        tab: layers_tree::TreeTab::Layout,
        colours: app.colours(),
        hidden: &hidden,
        focus: None,
        selected: app.selected,
        subject: Some(&app.subject),
        relations: Some(&rel),
        target: app.current_target(),
        revision: app.revision,
        parts_selected: 0,
        show_empty: false,
        stale: None,
    };
    let mut view = app.tree.clone();
    view.query = "via".to_owned();
    let rows = layers_tree::rows(&env, &mut view);
    let copies = rows
        .iter()
        .filter(|r| r.key.ends_with(&format!("via:{via}")))
        .count();
    assert!(
        copies >= 3,
        "the via shows under L1, L2 and the drill list: {copies}"
    );
    let inner = layers_tree::Row {
        key: "inner:2".into(),
        ..rows.first().cloned().ok_or("the tree has rows")?
    };
    assert!(
        layers_tree::outside_reach(&env, &inner),
        "a layer outside the span dims"
    );
    assert!(
        scrolled_to(&ctx, &mut app, "L1 to L2")?,
        "Properties names the span: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_clicks_a_track_and_the_tree_scrolls_to_its_net() -> Result<(), String> {
    let (ctx, mut app, net, _) = routed()?;
    let id = app
        .project
        .board
        .tracks
        .iter()
        .find(|t| t.net == net && t.layer == pcbcraft_doc::Layer::Front)
        .map(|t| t.id)
        .ok_or("a top track should exist")?;
    app.canvas_click(Point::new(10.0, 5.0), 10.0);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.subject,
        Selection::Object(id),
        "the click picks the track"
    );
    assert_eq!(
        app.tree.cursor.as_deref(),
        Some(format!("side:top/copper/net:{net}/track:{id}").as_str()),
        "the tree moves to the track under its net"
    );
    assert!(
        crate::selection::relations(&app.project, &app.subject).net == Some(net),
        "the track's net is related"
    );
    assert!(
        paints_text(&ctx, &mut app, "SIGX")?,
        "the net row is on screen"
    );
    Ok(())
}

#[test]
fn a_newcomer_reads_a_layers_record_in_properties() -> Result<(), String> {
    let (mut app, ctx) = four_layer_app()?;
    app.run(
        "board.set_layer_record",
        json!({"name": "L1", "kind": "copper", "thickness": 0.035, "dk": 4.2, "copper_weight_oz": 1.0}),
    );
    app.change_tab(Tab::Pcb);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_id(&ctx, &mut app, "button:Layer # \u{b7} Top, shown, unlocked")?;
    assert!(
        matches!(app.subject, Selection::Layer(_)),
        "pressing a layer row selects the layer: {:?}",
        app.subject
    );
    for words in ["Physical record", "Dk", "4.20", "1.00 oz"] {
        assert!(
            scrolled_to(&ctx, &mut app, words)?,
            "Properties shows {words:?} for the layer: {}",
            app.message
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_picks_a_via_then_finds_its_holes_lit_in_manufacture() -> Result<(), String> {
    let (ctx, mut app, _net, via) = routed()?;
    let button = via_button(&mut app, via)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_id(&ctx, &mut app, &button)?;
    tap_id(&ctx, &mut app, "tab:Manufacture")?;
    let rows = pcbcraft_io::drill_table(&app.project)?;
    let row = rows.first().ok_or("the via makes a drill row")?;
    let key = crate::selection::DrillKey::new(row.from_layer, row.to_layer, row.diameter_mm);
    let rel = crate::selection::relations(&app.project, &app.subject);
    assert_eq!(
        app.subject,
        Selection::Object(via),
        "the via stays selected in Manufacture"
    );
    assert_eq!(
        crate::selection::drill_level(&app.project, &app.subject, &rel, key),
        Level::Strong,
        "its drill row lights"
    );
    let name = format!(
        "button:Drill L# to L# {} #.# mm",
        row.kind.label().to_lowercase(),
    );
    tap_id(&ctx, &mut app, &name)?;
    assert_eq!(
        app.subject,
        Selection::Artifact(key.text()),
        "pressing the drill row selects the artifact: {}",
        app.message
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Drill artifact")?,
        "Properties names the artifact: {}",
        app.message
    );
    Ok(())
}
