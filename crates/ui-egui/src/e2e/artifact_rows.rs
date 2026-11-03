//! Stories for artifact rows in the Layers tree: the files a layer makes sit
//! under that layer, say when they are out of date, and open in Manufacture.
use super::super::*;
use super::layers::pcb_app;
use super::support::*;
use crate::selection::{Level, Selection};

fn add_plots(app: &mut App) -> Result<(Id, Id), String> {
    let folder = std::env::temp_dir().join(format!("pcbcraftrows-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    app.make_dir = folder.to_string_lossy().into_owned();
    app.run(
        "artifact.add",
        json!({"name": "Top copper plot", "kind": "copper_plot", "path": "pcb-F.Cu.gbr", "layers": ["F.Cu"]}),
    );
    app.run(
        "artifact.add",
        json!({"name": "Bottom placement", "kind": "placement", "path": "placement.txt", "settings": {"side": "bottom"}}),
    );
    app.refresh();
    let copper = app
        .project
        .artifacts
        .iter()
        .find(|a| a.kind == "copper_plot")
        .map(|a| a.id)
        .ok_or("the copper plot record should exist")?;
    let place = app
        .project
        .artifacts
        .iter()
        .find(|a| a.kind == "placement")
        .map(|a| a.id)
        .ok_or("the placement record should exist")?;
    Ok((copper, place))
}

fn tree_rows(app: &App) -> Vec<String> {
    let hidden: Vec<String> = vec![];
    let rel = crate::selection::relations(&app.project, &app.subject);
    let mut view = app.tree.clone();
    let stale = view.stale_ids(&app.project, app.revision);
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
        stale: Some(&stale),
    };
    layers_tree::rows(&env, &mut view)
        .iter()
        .map(|r| r.key.clone())
        .collect()
}

#[test]
fn a_newcomer_sees_which_files_a_layer_makes() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    let (copper, place) = add_plots(&mut app)?;
    app.tree.expanded.insert("side:bottom/components".into());
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let keys = tree_rows(&app);
    let want_copper = format!("side:top/copper/artifact:copper_plot:{copper}");
    let want_place = format!("side:bottom/components/artifact:placement:{place}");
    assert!(
        keys.contains(&want_copper),
        "the copper plot sits under top copper: {keys:?}"
    );
    assert!(
        keys.contains(&want_place),
        "the bottom placement file sits under the bottom parts: {keys:?}"
    );
    assert!(
        !keys
            .iter()
            .any(|k| k.starts_with("side:top/components/artifact:")),
        "nothing is listed under the top parts: {keys:?}"
    );
    assert!(
        paints_text(&ctx, &mut app, "Top copper plot")?,
        "the row is on screen: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Out of date")?,
        "a file never made reads out of date: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_an_artifact_go_stale_after_moving_a_track() -> Result<(), String> {
    // The plot stamps a creation time, so the clock is pinned to compare files.
    pcbcraft_io::with_fixed_clock(1_800_000_000, stale_story)
}

fn stale_story() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    let (copper, _) = add_plots(&mut app)?;
    tap_id(&ctx, &mut app, "button:Top copper plot, Out of date")?;
    assert_eq!(
        app.subject,
        Selection::Artifact(format!("copper_plot:{copper}")),
        "pressing the row picks the artifact"
    );
    for words in ["Copper plot", "pcb-F.Cu.gbr", "Out of date"] {
        assert!(
            scrolled_to(&ctx, &mut app, words)?,
            "Properties shows {words:?}: {}",
            app.message
        );
    }
    authoring::tests::tap(&ctx, &mut app, "Make this file")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert!(
        app.project
            .artifacts
            .iter()
            .any(|a| a.generated_hash.is_some()),
        "making the file stamps the record: {}",
        app.message
    );
    assert_eq!(
        app.subject,
        Selection::Artifact(format!("copper_plot:{copper}"))
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Up to date")?,
        "Properties says the file is up to date: {}",
        app.message
    );
    let net = app
        .project
        .nets
        .first()
        .map(|n| n.id)
        .ok_or("the practice design should have a net")?;
    app.run(
        "track.add",
        json!({"net": net, "start": [3.0, 3.0], "end": [9.0, 3.0], "width": 0.3, "layer": "F.Cu"}),
    );
    app.refresh();
    app.select_subject(Selection::Artifact(format!("copper_plot:{copper}")));
    assert!(
        scrolled_to(&ctx, &mut app, "Out of date")?,
        "a new track on the plotted layer marks the file out of date: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Make this file")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Up to date")?,
        "making the file again clears the mark: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_a_layers_drill_file_from_the_tree() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    let (copper, _) = add_plots(&mut app)?;
    let net = app
        .project
        .nets
        .first()
        .map(|n| n.id)
        .ok_or("the practice design should have a net")?;
    app.run(
        "via.place",
        json!({"net": net, "position": {"x": 12.0, "y": 12.0}, "diameter": 0.7, "drill": 0.3}),
    );
    app.select(None);
    app.refresh();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let drill = tree_rows(&app)
        .into_iter()
        .find(|k| k.starts_with("drills/artifact:drill:"))
        .ok_or("the drills row lists the drill file")?;
    let rest = drill.trim_start_matches("drills/artifact:").to_owned();
    assert!(
        paints_text(&ctx, &mut app, "Drill L1 to L2")?,
        "the drill file row is on screen: {}",
        app.message
    );
    // Pick the copper plot, then show it in Manufacture.
    tap_id(&ctx, &mut app, "button:Top copper plot, Out of date")?;
    authoring::tests::tap(&ctx, &mut app, "Show in Manufacture")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.tab, Tab::PlotFiles, "the Manufacture tab is open");
    assert_eq!(
        app.subject,
        Selection::Artifact(format!("copper_plot:{copper}")),
        "the pick follows to Manufacture"
    );
    assert!(
        paints_text(&ctx, &mut app, "Output files")?,
        "Manufacture lists the output files: {}",
        app.message
    );
    let record = app
        .project
        .artifacts
        .iter()
        .find(|a| a.id == copper)
        .ok_or("the record should exist")?;
    assert_eq!(
        crate::artifact_rows::level_for(&app.project, &app.subject, record, None),
        Level::Strong,
        "its Manufacture row is lit"
    );
    tap_id(&ctx, &mut app, "button:Output Top copper plot")?;
    // The drill file row is picked from the Manufacture list of holes.
    app.select_subject(Selection::Artifact(rest.clone()));
    let rel = crate::selection::relations(&app.project, &app.subject);
    assert!(
        rel.layers.is_some(),
        "a drill file reaches the layers of its span"
    );
    Ok(())
}
