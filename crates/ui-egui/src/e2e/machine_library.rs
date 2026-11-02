//! Stories for the machine library: built-in machines as cards, adding one to the project,
//! duplicating, picking a machine for an output, and a project copy that is behind.
//!
//! Context: issue #1167. Every machine, built in or the person's own, is a card in the
//! Machines view of the Library tab. The project keeps its own copy of each machine it uses.
use super::super::*;
use super::support::*;

fn library_app() -> Result<(egui::Context, App, std::path::PathBuf), String> {
    let (config, folder) = private_config("machines")?;
    let mut app = App::create(session_with(&config), Preferences::default(), None, None);
    app.refresh();
    if !app.open_showcase() {
        return Err(format!("the showcase did not open: {}", app.message));
    }
    app.home_open = false;
    app.change_tab(Tab::Library);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    authoring::tests::tap(&ctx, &mut app, "Machines")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((ctx, app, folder))
}

/// Every line of text painted this frame, for failure messages.
fn painted(ctx: &egui::Context, app: &mut App) -> String {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// A newcomer opens the Machines view and sees the built-in machines as ordinary cards,
/// narrows them by kind and by search, hides one and brings it back.
#[test]
fn a_newcomer_sees_built_in_machines_as_ordinary_library_cards() -> Result<(), String> {
    let (ctx, mut app, folder) = library_app()?;
    assert!(
        paints_text(&ctx, &mut app, "NeoDen YY1")?,
        "the pick and place machine is a card: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        paints_text(&ctx, &mut app, "Built in")?,
        "the card says where the machine comes from"
    );
    authoring::tests::tap(&ctx, &mut app, "Laser")?;
    assert!(
        !paints_text(&ctx, &mut app, "NeoDen YY1")?,
        "the laser filter leaves out pick and place machines"
    );
    authoring::tests::tap(&ctx, &mut app, "All kinds")?;
    authoring::tests::tap(&ctx, &mut app, "Built in")?;
    authoring::tests::tap(&ctx, &mut app, "All sources")?;
    type_into(&ctx, &mut app, "textinput:Search machines", "neoden 4")?;
    assert!(
        paints_text(&ctx, &mut app, "NeoDen 4")?,
        "{}",
        painted(&ctx, &mut app)
    );
    authoring::tests::tap(&ctx, &mut app, "Hide")?;
    assert!(
        !paints_text(&ctx, &mut app, "Hidden from the lists.")?,
        "a hidden machine leaves the list"
    );
    tap_id(&ctx, &mut app, "checkbox:Show hidden machines")?;
    assert!(
        paints_text(&ctx, &mut app, "Hidden from the lists.")?,
        "Show hidden machines lists it again, marked: {}",
        painted(&ctx, &mut app)
    );
    authoring::tests::tap(&ctx, &mut app, "Show")?;
    assert!(
        !paints_text(&ctx, &mut app, "Hidden from the lists.")?,
        "Show puts the machine back"
    );
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

/// A newcomer adds their placer from a built-in card, picks it for the pick and place
/// output in Manufacture and writes the file.
#[test]
fn a_newcomer_adds_their_placer_from_a_built_in_and_picks_it_for_an_output() -> Result<(), String>
{
    let (ctx, mut app, folder) = library_app()?;
    let out = folder.join("out");
    app.make_dir = out.to_string_lossy().into_owned();
    type_into(&ctx, &mut app, "textinput:Search machines", "neoden yy1")?;
    authoring::tests::tap(&ctx, &mut app, "Add to this project")?;
    assert!(
        app.project
            .machine_profiles
            .iter()
            .any(|m| m.id == "neoden-yy1"),
        "the project holds its own copy: {}",
        app.message
    );
    app.change_tab(Tab::PlotFiles);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Pick and place file")?,
        "Manufacture has a pick and place row: {}",
        painted(&ctx, &mut app)
    );
    tap_id(&ctx, &mut app, "combobox:Machine")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "NeoDen YY1")?;
    authoring::tests::tap(&ctx, &mut app, "Write job file")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let top = std::fs::read_to_string(out.join("pick-and-place").join("Top.csv"))
        .map_err(|e| format!("{e}: {}", app.message))?;
    assert!(top.starts_with("NEODEN,YY1,P&P FILE"), "{top}");
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

/// A newcomer duplicates a machine, finds the copy in their machines and the project, and
/// gives the copy's first rule another nozzle without touching the original.
#[test]
fn a_newcomer_duplicates_a_machine_and_changes_a_nozzle() -> Result<(), String> {
    let (ctx, mut app, folder) = library_app()?;
    type_into(&ctx, &mut app, "textinput:Search machines", "neoden yy1")?;
    authoring::tests::tap(&ctx, &mut app, "Duplicate")?;
    let copy = app
        .project
        .machine_profiles
        .iter()
        .find(|m| m.id == "neoden-yy1-copy")
        .cloned()
        .ok_or_else(|| format!("the copy joins the project: {}", app.message))?;
    assert_eq!(copy.name, "NeoDen YY1 copy");
    assert_eq!(
        copy.based_on.as_ref().map(|b| b.id.as_str()),
        Some("neoden-yy1"),
        "the copy remembers where it came from"
    );
    assert!(
        folder.join("machines").join("neoden-yy1-copy.toml").is_file(),
        "the copy is kept in the person's machines"
    );
    app.change_tab(Tab::PlotFiles);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Machines")?;
    authoring::tests::tap(&ctx, &mut app, "NeoDen YY1 copy")?;
    let first_rule = copy
        .pick_place
        .placement_rule
        .iter()
        .position(|r| !r.skip)
        .ok_or("the machine has a rule that picks a nozzle")?;
    let other = copy
        .pick_place
        .nozzle
        .iter()
        .map(|n| n.id.clone())
        .find(|id| *id != copy.pick_place.placement_rule[first_rule].nozzle)
        .ok_or("the machine has a second nozzle")?;
    // The nozzle lists are drawn once per rule; open one and pick the other nozzle.
    tap_id(&ctx, &mut app, "combobox:Nozzle for this rule")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, &other)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let changed = app
        .project
        .machine_profiles
        .iter()
        .find(|m| m.id == "neoden-yy1-copy")
        .ok_or("the copy is still on the project")?;
    assert_ne!(
        changed.pick_place.placement_rule, copy.pick_place.placement_rule,
        "a rule of the copy changed: {}",
        app.message
    );
    assert!(
        !app.project
            .machine_profiles
            .iter()
            .any(|m| m.id == "neoden-yy1"),
        "the built-in machine was never copied or changed"
    );
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

/// A newcomer sees that the project copy of one of their machines is behind the library
/// and updates it.
#[test]
fn a_newcomer_sees_that_a_project_copy_is_behind_the_library() -> Result<(), String> {
    let (ctx, mut app, folder) = library_app()?;
    app.run(
        "machine.add",
        json!({"kind": "cnc_mill", "name": "Shop mill", "keep_in_my_machines": true}),
    );
    let file = folder.join("machines").join("shop-mill.toml");
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{e}: {}", app.message))?;
    let mut newer = pcbcraft_io::machine_profile_from_toml(&text)?;
    newer.version += 1;
    newer.notes = "Re-measured the spindle.".into();
    std::fs::write(&file, pcbcraft_io::machine_profile_to_toml(&newer)?)
        .map_err(|e| e.to_string())?;
    app.libws.machines.stale = true;
    type_into(&ctx, &mut app, "textinput:Search machines", "shop mill")?;
    assert!(
        paints_text(&ctx, &mut app, "The project copy is behind the library.")?,
        "the card says the copy is behind: {}",
        painted(&ctx, &mut app)
    );
    authoring::tests::tap(&ctx, &mut app, "Update from library")?;
    let copy = app
        .project
        .machine_profiles
        .iter()
        .find(|m| m.id == "shop-mill")
        .ok_or("the machine is on the project")?;
    assert_eq!(copy.notes, "Re-measured the spindle.", "{}", app.message);
    assert!(
        !paints_text(&ctx, &mut app, "The project copy is behind the library.")?,
        "the line goes once the copy is current"
    );
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

/// A newcomer makes a blank machine from the Add machine window and keeps it in their machines.
#[test]
fn a_newcomer_adds_a_blank_machine_from_the_add_machine_window() -> Result<(), String> {
    let (ctx, mut app, folder) = library_app()?;
    authoring::tests::tap(&ctx, &mut app, "Add machine")?;
    tap_id(&ctx, &mut app, "combobox:Machine to copy")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "NeoDen YY1")?;
    authoring::tests::tap(&ctx, &mut app, "Blank machine")?;
    type_into(&ctx, &mut app, "textinput:Name of the new machine", "Bench mill")?;
    tap_id(&ctx, &mut app, "checkbox:Keep in my machines")?;
    tap_id(&ctx, &mut app, "combobox:Kind of machine")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Mill")?;
    authoring::tests::tap(&ctx, &mut app, "Create")?;
    assert!(
        app.project
            .machine_profiles
            .iter()
            .any(|m| m.id == "bench-mill" && m.kind == "cnc_mill"),
        "the new machine joins the project: {}",
        app.message
    );
    assert!(folder.join("machines").join("bench-mill.toml").is_file());
    authoring::tests::tap(&ctx, &mut app, "Add machine")?;
    authoring::tests::tap(&ctx, &mut app, "Cancel")?;
    assert!(
        !paints_text(&ctx, &mut app, "Create")?,
        "Cancel closes the window"
    );
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}
