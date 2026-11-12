//! Stories for the Library tab: reading a part, its prices and its sources.
//!
//! Context: issues #695 and #699. A newcomer opens the Library tab, picks a part in the browser
//! that is already on the left, and reads about it. Prices come from recorded answers kept in
//! `fixtures/supplier-quotes.json`; there is no network and no supplier connection.
use super::super::*;
use super::support::*;

/// The recorded supplier answers for the Library tab stories.
const QUOTES: &str = include_str!("fixtures/supplier-quotes.json");

/// The sample board on the Library tab, with the browser showing.
fn library_open() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    if let Ok(source) = pcbcraft_library::sources::FixtureSource::from_json(QUOTES) {
        app.use_supplier_quotes(Box::new(source));
    }
    app.change_tab(Tab::Library);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (ctx, app)
}

/// The id of the browser card of the built-in resistor.
const RESISTOR_CARD: &str = "button:Resistor, Built in \u{b7} MIT OR Apache-#.#";

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

#[test]
fn a_newcomer_opens_the_library_tab_and_reads_a_parts_details() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    assert!(
        paints_text(&ctx, &mut app, "Library")?,
        "the tab row has a Library tab: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        paints_text(&ctx, &mut app, "No part open")?,
        "before a pick the page says so: {}",
        painted(&ctx, &mut app)
    );
    tap_id(&ctx, &mut app, RESISTOR_CARD)?;
    assert_eq!(app.libws.parts.len(), 1, "a pick opens one part tab");
    for text in [
        "Key parameters",
        "Lifecycle and compliance",
        "Description",
        "Symbol",
        "Footprint",
    ] {
        assert!(
            paints_text(&ctx, &mut app, text)?,
            "the part's page shows {text}: {}",
            painted(&ctx, &mut app)
        );
    }
    assert!(
        paints_text(&ctx, &mut app, "Not listed")?,
        "a value no source gave is said to be not listed: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        paints_text(&ctx, &mut app, "Sourcing")? || paints_text(&ctx, &mut app, "SOURCING")?,
        "the part placed in the project shows its editable sourcing: {}",
        painted(&ctx, &mut app)
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_a_part_from_the_browser_into_the_library_tab() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    app.change_tab(Tab::Schematic);
    app.region().left_tab = "library".to_owned();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let card = super::audit::controls(&ctx, &app)
        .into_iter()
        .find(|c| c.id.as_deref() == Some(RESISTOR_CARD))
        .map(|c| c.rect.center())
        .ok_or("the Resistor card is in the browser")?;
    // Rest the pointer on the card so its open button shows, then press that button.
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(card)],
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(card)],
        egui::Modifiers::NONE,
    );
    let seen = super::audit::controls(&ctx, &app);
    let open = seen
        .iter()
        .find(|c| {
            c.id.as_deref()
                .is_some_and(|id| id.contains("Open this part"))
        })
        .map(|c| c.rect.center())
        .ok_or_else(|| {
            let near: Vec<_> = seen
                .iter()
                .filter(|c| c.rect.contains(card) || c.rect.center().distance(card) < 120.0)
                .filter_map(|c| c.id.clone())
                .collect();
            format!("the hovered card shows an open-in-Library button; controls near it: {near:?}")
        })?;
    tap_at(&ctx, &mut app, open)?;
    assert_eq!(app.tab, Tab::Library, "the button opens the Library tab");
    assert_eq!(app.libws.parts.len(), 1, "on that part");
    assert!(
        paints_text(&ctx, &mut app, "Key parameters")?,
        "its page is showing: {}",
        painted(&ctx, &mut app)
    );
    Ok(())
}

/// Give the sample board's resistor the part number the recorded answers are for.
fn record_resistor_number(app: &mut App) -> Result<pcbcraft_doc::Id, String> {
    let id = app
        .project
        .components
        .iter()
        .find(|c| c.kind == "resistor")
        .map(|c| c.id)
        .ok_or("the sample board has a resistor")?;
    app.tool_call(
        "component.set_sourcing",
        json!({"id": id, "manufacturer": "Northfield Components", "mpn": "NR1206-330R-F"}),
    );
    app.refresh();
    Ok(id)
}

#[test]
fn a_newcomer_compares_prices_for_a_part() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    record_resistor_number(&mut app)?;
    tap_id(&ctx, &mut app, RESISTOR_CARD)?;
    tap_id(&ctx, &mut app, "tab:Vendors")?;
    assert!(
        paints_text(&ctx, &mut app, "No supplier connection is set up")?,
        "with no connection the page says so plainly: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        paints_text(&ctx, &mut app, "cached prices from 2026-10-21")?,
        "and names the date of the cached prices: {}",
        painted(&ctx, &mut app)
    );
    // The table shows the price at the chosen order quantity, one unit to begin with.
    for text in [
        "Distributor A",
        "Distributor B",
        "Distributor C",
        "A-311-330FRCT-ND",
        "0.028 USD",
    ] {
        assert!(
            paints_text(&ctx, &mut app, text)?,
            "the vendor table shows {text}: {}",
            painted(&ctx, &mut app)
        );
    }
    assert!(
        paints_text(&ctx, &mut app, "Order quantity")?,
        "the page offers an order quantity: {}",
        painted(&ctx, &mut app)
    );
    // Choosing a larger quantity shows that price break, and the largest shows the lowest.
    app.libws.vendors_qty = 2;
    assert!(
        paints_text(&ctx, &mut app, "0.011 USD")?,
        "the vendor table shows the hundred piece price: {}",
        painted(&ctx, &mut app)
    );
    app.libws.vendors_qty = 3;
    assert!(
        paints_text(&ctx, &mut app, "0.008 USD")?,
        "the vendor table shows the thousand piece price: {}",
        painted(&ctx, &mut app)
    );
    tap_id(&ctx, &mut app, "button:Refresh prices")?;
    assert!(
        app.message.contains("no supplier connection"),
        "Refresh prices says why nothing changed: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_adds_a_supplier_source_to_a_part() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    let id = record_resistor_number(&mut app)?;
    tap_id(&ctx, &mut app, RESISTOR_CARD)?;
    tap_id(&ctx, &mut app, "tab:Vendors")?;
    type_into(&ctx, &mut app, "textinput:Supplier name", "Shop Z")?;
    type_into(
        &ctx,
        &mut app,
        "textinput:Supplier part number",
        "Z-330-1206",
    )?;
    tap_id(&ctx, &mut app, "button:Add supplier")?;
    app.refresh();
    let suppliers = app
        .project
        .component(id)
        .map(|c| c.sourcing.suppliers.clone())
        .unwrap_or_default();
    assert!(
        suppliers
            .iter()
            .any(|s| s.supplier == "Shop Z" && s.part_number == "Z-330-1206"),
        "the supplier is recorded on the part: {suppliers:?}"
    );
    assert!(
        paints_text(&ctx, &mut app, "Z-330-1206")?,
        "the vendor table lists it: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        paints_text(&ctx, &mut app, "Never")?,
        "with no cached price and no date: {}",
        painted(&ctx, &mut app)
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_the_protection_diode_from_the_part_list() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    tap_id(
        &ctx,
        &mut app,
        "button:Protection diode, Built in \u{b7} MIT OR Apache-#.#",
    )?;
    assert_eq!(app.libws.parts.len(), 1, "the part opens as a tab");
    Ok(())
}

#[test]
fn a_newcomer_compares_two_parts_and_sees_their_differences() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    tap_id(&ctx, &mut app, RESISTOR_CARD)?;
    tap_id(
        &ctx,
        &mut app,
        "button:LED, Built in \u{b7} MIT OR Apache-#.#",
    )?;
    assert_eq!(app.libws.parts.len(), 2, "two parts are open as tabs");
    assert!(
        paints_text(&ctx, &mut app, "Compare")?,
        "a Compare tab appears beside the two part tabs: {}",
        painted(&ctx, &mut app)
    );
    tap_id(&ctx, &mut app, "tab:Compare")?;
    for text in ["Compare parts", "PARAMETER", "Differs", "Not listed"] {
        assert!(
            paints_text(&ctx, &mut app, text)?,
            "the compare table shows {text}: {}",
            painted(&ctx, &mut app)
        );
    }
    let before = app.libws.compare_only_diff;
    tap_id(&ctx, &mut app, "checkbox:Only differences")?;
    assert_ne!(
        app.libws.compare_only_diff, before,
        "Only differences hides the rows that match"
    );
    Ok(())
}

#[test]
fn a_newcomer_creates_a_new_part_in_the_library_tab() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    tap_id(&ctx, &mut app, "button:New part")?;
    assert_eq!(
        app.libws.page,
        library_workspace::Page::Edit,
        "New part opens the editor page"
    );
    assert!(
        matches!(app.library.editor.draft, Some(authoring::Draft::Symbol(_))),
        "with a new symbol to draw"
    );
    type_into(&ctx, &mut app, "textinput:Name", "Status LED")?;
    tap_id(&ctx, &mut app, "button:Save symbol to project library")?;
    app.refresh();
    assert!(
        app.project
            .library
            .symbols
            .iter()
            .any(|s| s.name == "Status LED"),
        "the part is saved in the project's library: {:?}",
        app.project
            .library
            .symbols
            .iter()
            .map(|s| &s.name)
            .collect::<Vec<_>>()
    );
    assert!(
        paints_text(&ctx, &mut app, "Status LED")?,
        "and the browser on the left lists it: {}",
        painted(&ctx, &mut app)
    );
    Ok(())
}

/// A project part whose footprint carries a model file, open on the Library tab.
fn open_part_with_model(ctx: &egui::Context, app: &mut App) {
    let mut fp = authoring::new_footprint();
    fp.name = "Test_FP".into();
    app.tool_call("library.footprint.upsert", json!({"definition": fp}));
    app.tool_call(
        "library.footprint.set_model",
        json!({"name": "Test_FP", "path": "models/test.step"}),
    );
    let mut sym = authoring::new_symbol();
    sym.name = "Test part".into();
    sym.footprint = "Test_FP".into();
    app.tool_call("library.symbol.upsert", json!({"definition": sym.clone()}));
    app.refresh();
    app.library_open_part(ctx, &library_dock::Card::Project(Box::new(sym)));
    for _ in 0..3 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

fn model_offset(app: &App) -> Option<[f64; 3]> {
    app.project
        .library
        .footprints
        .iter()
        .find(|f| f.name == "Test_FP")
        .and_then(|f| f.model.as_ref())
        .map(|m| m.offset)
}

#[test]
fn a_newcomer_moves_a_parts_3d_model_against_its_pads() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    open_part_with_model(&ctx, &mut app);
    tap_id(&ctx, &mut app, "tab:#D model")?;
    for text in ["Model file", "Alignment", "Pad overlay", "models/test.step"] {
        assert!(
            paints_text(&ctx, &mut app, text)?,
            "the 3D page shows {text}: {}",
            painted(&ctx, &mut app)
        );
    }
    type_into(&ctx, &mut app, "spinbutton:Offset X", "1.5")?;
    app.refresh();
    assert_eq!(
        model_offset(&app),
        Some([1.5, 0.0, 0.0]),
        "typing an offset moves the model in the project's library"
    );
    app.tool_call("undo", json!({}));
    app.refresh();
    assert_eq!(
        model_offset(&app),
        Some([0.0, 0.0, 0.0]),
        "the move is one undo step"
    );
    Ok(())
}

#[test]
fn a_newcomer_checks_which_pad_each_symbol_pin_reaches() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    open_part_with_model(&ctx, &mut app);
    tap_id(&ctx, &mut app, "tab:Edit")?;
    for text in ["Pin map", "2 of 2 mapped", "Mapped"] {
        assert!(
            scrolled_to_at(&ctx, &mut app, text, 700.0)?,
            "the Edit page shows {text}: {}",
            painted(&ctx, &mut app)
        );
    }
    Ok(())
}

/// Open a picker by its spoken name and choose one of its entries.
fn pick_entry(ctx: &egui::Context, app: &mut App, picker: &str, entry: &str) -> Result<(), String> {
    tap_named_holding(ctx, app, picker, egui::Modifiers::NONE)?;
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(ctx, app, entry)?;
    for _ in 0..3 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    Ok(())
}

/// The role stored for pad `pad` of the part named `Test part`.
fn stored_role(app: &App, pad: &str) -> Option<pcbcraft_library::PadRole> {
    app.project
        .library
        .part_pin_maps
        .get("Test part")
        .map(|m| m.pad_role(pad))
}

#[test]
fn a_newcomer_reassigns_a_pin_to_another_pad_and_the_finding_clears() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    open_part_with_model(&ctx, &mut app);
    tap_id(&ctx, &mut app, "tab:Edit")?;
    assert!(
        paints_text_tall(&ctx, &mut app, "Pad assignments")?,
        "the Edit page shows the pad assignments: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        !paints_text_tall(&ctx, &mut app, "is mapped to both pin")?,
        "a part whose pins meet their pads by number has no shared pad"
    );
    // Give pad 1 to pin 2 as well: pad 1 now has two pins.
    pick_entry(&ctx, &mut app, "ComboBox Pad driven by pin 2", "Pad 1")?;
    assert!(
        paints_text_tall(&ctx, &mut app, "1 is mapped to both pin 1 and pin 2")?,
        "the shared pad is reported: {} / {}",
        app.message,
        painted(&ctx, &mut app)
    );
    assert!(
        paints_text_tall(
            &ctx,
            &mut app,
            "2 on footprint Test_FP is not mapped to a pin"
        )?,
        "pad 2 is left with no pin and that is reported: {}",
        painted(&ctx, &mut app)
    );
    // Send pin 2 back to its own pad: both findings go away.
    pick_entry(&ctx, &mut app, "ComboBox Pad driven by pin 2", "Pad 2")?;
    assert!(
        !paints_text_tall(&ctx, &mut app, "is mapped to both pin")?,
        "the shared pad finding clears: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        !paints_text_tall(&ctx, &mut app, "is not mapped to a pin")?,
        "the unclaimed pad finding clears: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        paints_text_tall(&ctx, &mut app, "No problems")?,
        "the card says there are no problems: {}",
        painted(&ctx, &mut app)
    );
    // Give pad 2 a role; the stored map follows and one undo puts it back.
    pick_entry(&ctx, &mut app, "ComboBox Role of pad 2", "Thermal")?;
    assert_eq!(
        stored_role(&app, "2"),
        Some(pcbcraft_library::PadRole::Thermal),
        "the role picker stores the role: {}",
        app.message
    );
    app.tool_call("undo", json!({}));
    app.refresh();
    assert_eq!(
        stored_role(&app, "2"),
        Some(pcbcraft_library::PadRole::Electrical),
        "the role change is one undo step"
    );
    Ok(())
}

#[test]
fn a_newcomer_manages_alternates_and_second_sources() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    // A built-in part explains that substitutes belong to the project library.
    tap_id(&ctx, &mut app, RESISTOR_CARD)?;
    tap_id(&ctx, &mut app, "tab:Alternates")?;
    assert!(
        paints_text(
            &ctx,
            &mut app,
            "Copy the part into the project library first."
        )?,
        "a built-in part says substitutes need a project part: {}",
        painted(&ctx, &mut app)
    );
    // A project part shows both sections.
    let mut sym = authoring::new_symbol();
    sym.name = "Test part".into();
    app.tool_call("library.symbol.upsert", json!({"definition": sym.clone()}));
    app.refresh();
    app.library_open_part(&ctx, &library_dock::Card::Project(Box::new(sym)));
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    tap_id(&ctx, &mut app, "tab:Alternates")?;
    assert!(
        paints_text(&ctx, &mut app, "Second sources")?,
        "the Alternates page shows the Second sources section: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        paints_text(&ctx, &mut app, "Alternates")?,
        "the Alternates page shows the Alternates section: {}",
        painted(&ctx, &mut app)
    );

    // Add a second source
    type_into(&ctx, &mut app, "textinput:Manufacturer", "Acme Corp")?;
    type_into(&ctx, &mut app, "textinput:Part number", "ACME-1206-330")?;
    tap_id(&ctx, &mut app, "button:Add source")?;
    app.refresh();

    // Verify the second source was added
    let has_source = app
        .project
        .library
        .part_substitutes
        .get("Test part")
        .map(|s| {
            s.second_sources
                .iter()
                .any(|src| src.manufacturer == "Acme Corp" && src.mpn == "ACME-1206-330")
        })
        .unwrap_or(false);
    assert!(
        has_source,
        "the second source was recorded: {} / {:?}",
        app.message,
        app.project
            .library
            .part_substitutes
            .keys()
            .collect::<Vec<_>>()
    );

    // Add an alternate: it names another symbol of the project library.
    let mut other = authoring::new_symbol();
    other.name = "Low cost resistor".into();
    app.tool_call("library.symbol.upsert", json!({"definition": other}));
    app.refresh();
    type_into(&ctx, &mut app, "textinput:Symbol name", "Low cost resistor")?;
    tap_id(&ctx, &mut app, "button:Add alternate")?;
    app.refresh();

    // Verify the alternate was added
    let has_alt = app
        .project
        .library
        .part_substitutes
        .get("Test part")
        .map(|s| s.alternates.iter().any(|a| a.symbol == "Low cost resistor"))
        .unwrap_or(false);
    assert!(has_alt, "the alternate was recorded");
    Ok(())
}

#[test]
fn a_newcomer_saves_a_part_to_the_project_library() -> Result<(), String> {
    let (_ctx, mut app) = library_open();
    // A project part with a footprint, placed on the board with a manufacturer field.
    let mut fp = authoring::new_footprint();
    fp.name = "Test_FP".into();
    app.tool_call("library.footprint.upsert", json!({"definition": fp}));
    let mut sym = authoring::new_symbol();
    sym.name = "Test Part".into();
    sym.footprint = "Test_FP".into();
    app.tool_call("library.symbol.upsert", json!({"definition": sym}));
    app.tool_call(
        "component.place_custom",
        json!({"name": "Test Part", "position": {"x": 10.0, "y": 10.0}, "reference": "U1"}),
    );
    app.refresh();
    let id = app
        .project
        .components
        .last()
        .ok_or("the part was placed")?
        .id;
    app.tool_call(
        "footprint.assign_custom",
        json!({"id": id, "name": "Test_FP"}),
    );
    app.tool_call(
        "component.field",
        json!({"id": id, "key": "Manufacturer", "value": "TestCo"}),
    );
    app.refresh();
    let saved = app.tool_call("part.save", json!({"id": id, "target": "project"}));
    assert!(saved, "part.save succeeded: {}", app.message);
    app.refresh();
    let part = app
        .project
        .library
        .part_sidecars
        .iter()
        .find(|p| p.symbol == "Test Part")
        .ok_or("the part was saved to the project library")?;
    assert!(
        part.fields
            .iter()
            .any(|f| f.key == "Manufacturer" && f.value == "TestCo"),
        "the saved part keeps its manufacturer field"
    );
    Ok(())
}

#[test]
fn a_newcomer_copies_a_part_to_the_personal_library() -> Result<(), String> {
    // Set up a personal library folder.
    let personal = std::env::temp_dir().join(format!(
        "pcbcraft-personal-{}-{}",
        std::process::id(),
        pcbcraft_clock::system_now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis()
    ));
    std::fs::create_dir_all(&personal).map_err(|e| e.to_string())?;
    let personal_str = personal.to_string_lossy().to_string();

    let (config, _config_folder) = private_config("personal")?;
    let shared = session_with(&config);
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let added = app.tool_call(
        "library.add_folder",
        json!({"path": personal_str, "name": "My parts"}),
    );
    assert!(added, "the personal folder was added: {}", app.message);
    // No tool marks a folder personal yet, so the story marks it through the engine.
    pcbcraft_engine::library::set_source_role(&config, &personal_str, Some("personal"))?;

    // Create and place a project part.
    let mut fp = authoring::new_footprint();
    fp.name = "PersonalTest_FP".into();
    app.tool_call("library.footprint.upsert", json!({"definition": fp}));
    let mut sym = authoring::new_symbol();
    sym.name = "Personal Test".into();
    sym.footprint = "PersonalTest_FP".into();
    app.tool_call("library.symbol.upsert", json!({"definition": sym}));
    app.tool_call(
        "component.place_custom",
        json!({"name": "Personal Test", "position": {"x": 10.0, "y": 10.0}, "reference": "U1"}),
    );
    app.refresh();
    let id = app
        .project
        .components
        .last()
        .ok_or("the part was placed")?
        .id;
    app.tool_call(
        "footprint.assign_custom",
        json!({"id": id, "name": "PersonalTest_FP"}),
    );
    app.refresh();

    // Copy the part to the personal library.
    let saved = app.tool_call("part.save", json!({"id": id, "target": "personal"}));
    assert!(saved, "part.save to personal succeeded: {}", app.message);

    // Verify the files were written.
    let symbol_path = personal.join("Personal Test.kicad_sym");
    assert!(
        symbol_path.exists(),
        "the symbol file was written to the personal folder"
    );
    let sidecar_path = personal.join("Personal Test.pcbcraft-part.json");
    assert!(
        sidecar_path.exists(),
        "the sidecar file was written to the personal folder"
    );

    // Verify the sidecar has a provenance notice.
    let sidecar_text = std::fs::read_to_string(&sidecar_path).map_err(|e| e.to_string())?;
    assert!(
        sidecar_text.contains("Copied from project"),
        "the sidecar has a provenance notice"
    );

    // Clean up.
    let _ = std::fs::remove_dir_all(&personal);
    Ok(())
}
