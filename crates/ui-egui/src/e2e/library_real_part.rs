//! Story: a newcomer types a chip number, clicks the card the open library
//! offers and gets a part with its pins, package and sourcing fields.
use super::super::*;
use super::support::*;

fn field(session: &pcbcraft_engine::Session, key: &str) -> Option<String> {
    session
        .project
        .components
        .first()?
        .fields
        .iter()
        .find(|f| f.key == key)
        .map(|f| f.value.clone())
}

#[test]
fn a_newcomer_types_a_chip_number_clicks_the_card_and_gets_a_real_part() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared.clone(), Preferences::default(), None, None);
    app.refresh();
    app.region().left_tab = "library".to_owned();
    let ctx = egui::Context::default();
    let none = egui::Modifiers::NONE;
    frame(&ctx, &mut app, vec![], none);
    app.change_tab(Tab::Schematic);
    frame(&ctx, &mut app, vec![], none);
    // The open library answers from its recordings: no network in a story.
    tap_id(&ctx, &mut app, "button:Turn on Open library")?;
    use_recorded_library(&mut app, "real-chip")?;
    authoring::tests::tap(&ctx, &mut app, "Keyword or part number")?;
    for letter in "STM32F103R8".chars() {
        frame(&ctx, &mut app, vec![egui::Event::Text(letter.into())], none);
    }
    assert!(
        waits_for_text(&ctx, &mut app, "STM32F103R8Tx")?,
        "the chip number alone finds the part: {}",
        app.message
    );
    tap_id(
        &ctx,
        &mut app,
        "button:STM#F#R#Tx, Open library \u{b7} CC-BY-SA #.#",
    )?;
    for _ in 0..600 {
        frame(&ctx, &mut app, vec![], none);
        let placed = !shared
            .lock()
            .map_err(|_| "session")?
            .project
            .components
            .is_empty();
        if placed {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let session = shared.lock().map_err(|_| "session")?;
    let part = session
        .project
        .components
        .first()
        .ok_or_else(|| format!("clicking the card places the part: {}", app.message))?;
    assert_eq!(part.pins.len(), 64, "all 64 pins come from the parent file");
    let package = part
        .library_footprint
        .as_ref()
        .ok_or("the package comes with the part")?;
    assert_eq!(package.definition.name, "LQFP-64_10x10mm_P0.5mm");
    assert_eq!(package.definition.pads.len(), 64);
    assert_eq!(field(&session, "MPN").as_deref(), Some("STM32F103R8Tx"));
    assert_eq!(
        field(&session, "Datasheet").as_deref(),
        Some("https://www.st.com/resource/en/datasheet/stm32f103r8.pdf")
    );
    assert!(
        field(&session, "Description").is_some_and(|d| d.contains("Cortex-M3")),
        "the description comes with the part"
    );
    assert!(
        session
            .project
            .library
            .notices
            .iter()
            .any(|n| n.contains("STM32F103R_8-B_Tx")),
        "the notices name the parent file the pins came from"
    );
    Ok(())
}
