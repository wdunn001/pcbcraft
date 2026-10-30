//! Stories for the pick and place machine: which nozzle each part uses, and changing one.
use super::super::*;
use super::support::*;

fn showcase_app() -> Result<App, String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    if !app.open_showcase() {
        return Err(format!("the showcase did not open: {}", app.message));
    }
    app.home_open = false;
    Ok(app)
}

/// Every line of text on screen this frame, for failure messages.
fn painted(ctx: &egui::Context, app: &mut App) -> String {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let mut lines: Vec<String> = Vec::new();
    for shape in &output.shapes {
        if let egui::Shape::Text(text) = &shape.shape {
            lines.push(text.galley.text().to_string());
        }
    }
    lines.join(" | ")
}

fn open_machines(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    app.change_tab(Tab::PlotFiles);
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(ctx, app, "Machines")?;
    authoring::tests::tap(ctx, app, "Add NeoDen YY1")?;
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

/// A newcomer adds the pick and place machine and reads which nozzle each part gets.
#[test]
fn a_newcomer_sees_which_nozzle_each_part_uses() -> Result<(), String> {
    let mut app = showcase_app()?;
    let ctx = egui::Context::default();
    open_machines(&ctx, &mut app)?;
    assert!(
        app.project
            .machine_profiles
            .iter()
            .any(|m| m.id == "neoden-yy1"),
        "the machine joins the design: {}",
        app.message
    );
    assert!(paints_text(&ctx, &mut app, "NOZZLES AND FEEDERS")?);
    assert!(
        paints_text(&ctx, &mut app, "CN040")?,
        "the 0402 nozzle is listed"
    );
    assert!(
        scrolled_to_at(&ctx, &mut app, "PARTS", 700.0)?,
        "the part list is on the machine page: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        paints_text(&ctx, &mut app, "CN220")?,
        "the SOIC and QFN parts show their nozzle"
    );
    assert!(paints_text(&ctx, &mut app, "NOZZLE RULES")?);
    Ok(())
}

/// A newcomer gives one part another nozzle and writes the machine file.
#[test]
fn a_newcomer_changes_a_parts_nozzle_and_exports_the_machine_file() -> Result<(), String> {
    let mut app = showcase_app()?;
    let folder = std::env::temp_dir().join(format!("pcbcraft-pnp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    app.make_dir = folder.to_string_lossy().into_owned();
    let ctx = egui::Context::default();
    open_machines(&ctx, &mut app)?;
    let u4 = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "U4")
        .map(|c| c.id)
        .ok_or("the showcase has U4")?;
    app.change_tab(Tab::Pcb);
    app.selected = Some(u4);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "PLACEMENT MACHINE")?,
        "the part's Properties has a Placement machine section: {}",
        painted(&ctx, &mut app)
    );
    // The rules chose CN220 for a SOIC-8; the row lists every tip the machine has.
    assert!(
        scrolled_to(&ctx, &mut app, "CN400")?,
        "the nozzle row lists every tip the machine has: {}",
        painted(&ctx, &mut app)
    );
    authoring::tests::tap(&ctx, &mut app, "CN400")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let own = app
        .project
        .component(u4)
        .and_then(|c| c.field("Placement nozzle").map(str::to_string));
    assert_eq!(own.as_deref(), Some("CN400"), "{}", app.message);
    app.change_tab(Tab::PlotFiles);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Write job file")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let top = std::fs::read_to_string(folder.join("pick-and-place").join("Top.csv"))
        .map_err(|e| format!("{e}: {}", app.message))?;
    let sheet = std::fs::read_to_string(folder.join("pick-and-place").join("FeederSheet.csv"))
        .map_err(|e| e.to_string())?;
    assert!(top.starts_with("NEODEN,YY1,P&P FILE"), "{top}");
    assert!(top.contains("U4,Flash,SOIC-8,"), "{top}");
    assert!(
        sheet.contains("Flash,SOIC-8,,CN400,1"),
        "the feeder sheet names the new nozzle: {sheet}"
    );
    let _ = std::fs::remove_dir_all(&folder);
    Ok(())
}
