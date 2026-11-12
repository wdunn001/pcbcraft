//! Stories for the Resistor and Capacitor generator cards in the Library dock (issue #757): a
//! card opens a form with the series, the size, a range and a value, the form says the part it
//! makes, and Place puts that part with its symbol and footprint in the project as one undo step.
use super::super::*;
use super::support::*;
use pcbcraft_library::generate_passive;
use pcbcraft_library::passive_series::PassiveKind;

const PASSIVE_CHIP: &str =
    "Show only resistors, capacitors, inductors, diodes and similar parts. Click again to clear.";

fn dock() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.region().left_tab = "library".to_owned();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    settle(&ctx, &mut app);
    (ctx, app)
}

fn settle(ctx: &egui::Context, app: &mut App) {
    for _ in 0..6 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

fn press(ctx: &egui::Context, app: &mut App, name: &str) -> Result<(), String> {
    tap_named_holding(ctx, app, name, egui::Modifiers::NONE)?;
    settle(ctx, app);
    Ok(())
}

fn open_card(ctx: &egui::Context, app: &mut App, card: &str) -> Result<(), String> {
    let chip = format!("button:{}", super::audit::fold_label(PASSIVE_CHIP));
    tap_id(ctx, app, &chip)?;
    let steady = waits_until(ctx, app, |app| {
        app.dock.requests_sent > 0 && app.dock.applied == app.dock.requests_sent
    });
    assert!(steady, "the dock finished reading passive parts");
    settle(ctx, app);
    tap_id(ctx, app, &format!("button:{card}"))?;
    settle(ctx, app);
    Ok(())
}

fn undo(app: &mut App) -> Result<(), String> {
    app.session
        .lock()
        .map_err(|_| "session is busy".to_owned())?
        .undo()
        .map_err(|e| e.to_string())?;
    app.refresh();
    Ok(())
}

#[test]
fn a_newcomer_makes_a_4k7_resistor_from_its_card_and_places_it_on_the_schematic()
-> Result<(), String> {
    let (ctx, mut app) = dock();
    open_card(&ctx, &mut app, "Resistor generator")?;
    assert!(
        paints_text(&ctx, &mut app, "Makes RES-0603-10K-10, 10 kohm")?,
        "the card opens on the common choice"
    );
    press(&ctx, &mut app, "Button E24")?;
    press(&ctx, &mut app, "Button 0805")?;
    press(&ctx, &mut app, "Button 1 kohm")?;
    press(&ctx, &mut app, "Button 4.7 kohm")?;
    let made = generate_passive(PassiveKind::Resistor, 4700.0, "0805", 5)?;
    assert!((app.dock.passive.value_for_test() - 4700.0).abs() < 1e-6);
    assert!(
        paints_text(&ctx, &mut app, &format!("Makes {}, 4.7 kohm", made.mpn))?,
        "the form shows the part number and the value it makes"
    );
    let before = app.project.components.len();
    tap_id(&ctx, &mut app, "button:Place on schematic")?;
    settle(&ctx, &mut app);
    assert_eq!(app.project.components.len(), before + 1, "{}", app.message);
    let placed = app.project.components.last().ok_or("a part was placed")?;
    assert_eq!(placed.value, "4.7 kohm");
    let footprint = app
        .project
        .library
        .footprints
        .iter()
        .find(|f| f.name == made.footprint.name)
        .ok_or("the generated footprint joined the project library")?;
    assert_eq!(footprint.pads.len(), 2);
    assert!(
        placed.library_footprint.is_some(),
        "the part carries the footprint so the Layout can show it"
    );
    undo(&mut app)?;
    assert_eq!(app.project.components.len(), before, "one undo removes it");
    assert!(
        !app.project
            .library
            .footprints
            .iter()
            .any(|f| f.name == made.footprint.name),
        "the footprint goes with it"
    );
    Ok(())
}

#[test]
fn a_newcomer_places_a_100_nanofarad_capacitor_on_the_board_in_one_undo_step() -> Result<(), String>
{
    let (ctx, mut app) = dock();
    app.change_tab(Tab::Pcb);
    app.region().left_tab = "library".to_owned();
    settle(&ctx, &mut app);
    open_card(&ctx, &mut app, "Capacitor generator")?;
    let made = generate_passive(PassiveKind::Capacitor, 1e-7, "0603", 10)?;
    assert!(
        paints_text(&ctx, &mut app, &format!("Makes {}, 100 nF", made.mpn))?,
        "the card opens on 100 nF"
    );
    let parts = app.project.components.len();
    let feet = app.project.board.footprints.len();
    tap_id(&ctx, &mut app, "button:Place on board")?;
    settle(&ctx, &mut app);
    assert_eq!(app.project.components.len(), parts + 1, "{}", app.message);
    assert_eq!(app.project.board.footprints.len(), feet + 1);
    undo(&mut app)?;
    assert_eq!(app.project.components.len(), parts);
    assert_eq!(app.project.board.footprints.len(), feet);
    Ok(())
}
