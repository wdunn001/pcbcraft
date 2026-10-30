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
const RESISTOR_CARD: &str = "button:Resistor, Built in \u{b7} GPL-#.#-or-later";

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
    for text in [
        "Distributor A",
        "Distributor B",
        "Distributor C",
        "A-311-330FRCT-ND",
        "0.011 USD",
    ] {
        assert!(
            paints_text(&ctx, &mut app, text)?,
            "the vendor table shows {text}: {}",
            painted(&ctx, &mut app)
        );
    }
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
fn a_newcomer_compares_two_parts_and_sees_their_differences() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    tap_id(&ctx, &mut app, RESISTOR_CARD)?;
    tap_id(
        &ctx,
        &mut app,
        "button:LED, Built in \u{b7} GPL-#.#-or-later",
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

/// Answers a datasheet address with a small generated PDF, so the story needs no network.
///
/// Contract: crates/library/src/open_library.rs (`Fetcher`)
struct GeneratedPdf(Vec<u8>);

/// Contract: crates/library/src/open_library.rs
impl pcbcraft_library::open_library::Fetcher for GeneratedPdf {
    fn get(&self, _url: &str) -> Result<Vec<u8>, pcbcraft_library::open_library::FetchError> {
        Ok(self.0.clone())
    }
}

#[test]
fn a_newcomer_reads_a_datasheet_in_the_app() -> Result<(), String> {
    let (ctx, app) = library_open();
    let pdf = pcbcraft_library::datasheet::fixtures::all()
        .into_iter()
        .find(|f| f.name == "two-pages.pdf")
        .map(|f| f.bytes)
        .ok_or("the generated datasheet exists")?;
    let mut app = app.with_datasheet_fetcher(Box::new(GeneratedPdf(pdf)));
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    app.use_datasheet_cache(dir.path().to_path_buf());
    let id = record_resistor_number(&mut app)?;
    app.tool_call(
        "component.field",
        json!({"id": id, "key": "Datasheet", "value": "https://example.test/nr1206.pdf"}),
    );
    app.refresh();
    tap_id(&ctx, &mut app, RESISTOR_CARD)?;
    tap_id(&ctx, &mut app, "tab:Datasheet")?;
    assert!(
        paints_text(&ctx, &mut app, "not on this computer yet")?,
        "before the first fetch the page says the datasheet is not here: {}",
        painted(&ctx, &mut app)
    );
    tap_id(&ctx, &mut app, "button:Get datasheet")?;
    assert!(
        waits_for_text(&ctx, &mut app, "of 2")?,
        "after Get datasheet the reader shows the page count: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        dir.path().read_dir().map_err(|e| e.to_string())?.count() >= 2,
        "a copy is kept in the cache folder"
    );
    // Wait for the text layer of both pages to arrive, then search.
    for _ in 0..100 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        if app.libws.reader.as_ref().is_some_and(|r| r.text.len() == 2) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    type_into(
        &ctx,
        &mut app,
        "textinput:Search the datasheet",
        "absolute maximum",
    )?;
    let reader = app.libws.reader.as_ref().ok_or("the reader is open")?;
    assert_eq!(reader.hits.len(), 1, "the phrase is found once, on page 2");
    assert_eq!(reader.hits[0].0, 1);
    assert!(
        paints_text(&ctx, &mut app, "1 of 1")?,
        "the match count shows: {}",
        painted(&ctx, &mut app)
    );
    tap_id(&ctx, &mut app, "button:Next match")?;
    assert_eq!(
        app.libws.reader.as_ref().map(|r| r.page),
        Some(1),
        "going to the match shows its page"
    );
    // Select a run of text on the page and read it back as a person would paste it.
    if let Some(r) = app.libws.reader.as_mut() {
        let first = r.hits[0].1.first;
        let last = r.hits[0].1.last;
        r.selection = Some(library_workspace::reader::Selection {
            page: 1,
            anchor: first,
            head: last - 1,
        });
    }
    let copied = app.libws.reader.as_ref().and_then(|r| r.selected_text());
    assert_eq!(
        copied.as_deref().map(|s| s.replace(' ', "").to_lowercase()),
        Some("absolutemaximum".to_owned()),
        "the selected text is the matched words"
    );
    tap_id(&ctx, &mut app, "button:Zoom in")?;
    assert_eq!(
        app.libws.reader.as_ref().map(|r| r.zoom),
        Some(125),
        "zoom steps up"
    );
    Ok(())
}
