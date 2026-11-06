//! Stories for the Analyze tab: it exists, each page opens from the aside, the pages with no data
//! source say what is missing, and the Checks and Net classes pages show the design's real values.
//!
//! Context: issue #1421 (the sixth tab) and the simulation epic #1360. No story here expects a
//! number the design did not produce.
use super::super::*;
use super::support::*;

/// A fresh window on the sample board, showing the Analyze tab.
fn analyze_open() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Analyze);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (ctx, app)
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

/// The control whose gate id is `kind:label` for any kind, with the label starting `label`.
fn control_named(
    ctx: &egui::Context,
    app: &App,
    label: &str,
) -> Result<super::audit::Control, String> {
    // The gate folds digits to `#`, so I2C is listed as I#C.
    let label = &super::audit::fold_label(label);
    super::audit::controls(ctx, app)
        .into_iter()
        .find(|c| {
            c.id.as_deref()
                .and_then(|id| id.split_once(':'))
                .is_some_and(|(_, name)| name == label || name.starts_with(&format!("{label},")))
        })
        .ok_or_else(|| format!("no control named {label}"))
}

/// Press the control named `label`, like a hand.
fn press(ctx: &egui::Context, app: &mut App, label: &str) -> Result<(), String> {
    // A tooltip left by the last control must not cover this one.
    for _ in 0..3 {
        frame(
            ctx,
            app,
            vec![egui::Event::PointerGone],
            egui::Modifiers::NONE,
        );
    }
    let control = control_named(ctx, app, label)?;
    tap_at(ctx, app, control.rect.center())?;
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

/// True when the control named `label` is painted and can be pressed. A disabled button is
/// painted but not listed as a control, so it reads as not enabled.
fn is_enabled(ctx: &egui::Context, app: &mut App, label: &str) -> Result<bool, String> {
    if !paints_text(ctx, app, label)? {
        return Err(format!("{label} is not painted: {}", painted(ctx, app)));
    }
    Ok(control_named(ctx, app, label).is_ok_and(|c| c.enabled))
}

#[test]
fn a_newcomer_finds_the_analyze_tab_after_layout_and_opens_it() -> Result<(), String> {
    use super::super::tour::anchor;
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let layout = anchor::lookup(&ctx, "shell.tab.layout").ok_or("no Layout tab")?;
    let analyze = anchor::lookup(&ctx, "shell.tab.analyze").ok_or("no Analyze tab")?;
    let manufacture = anchor::lookup(&ctx, "shell.tab.manufacture").ok_or("no Manufacture tab")?;
    assert!(
        layout.center().x < analyze.center().x && analyze.center().x < manufacture.center().x,
        "Analyze sits between Layout and Manufacture"
    );
    tap_at(&ctx, &mut app, analyze.center())?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.tab, Tab::Analyze, "pressing the tab opens it");
    assert!(
        paints_text(&ctx, &mut app, "Analyses")?,
        "the left aside is titled Analyses: {}",
        painted(&ctx, &mut app)
    );
    press(&ctx, &mut app, "Analyses")?;
    assert_eq!(
        app.tab,
        Tab::Analyze,
        "pressing the aside tab keeps the page"
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_analyze_from_the_view_menu() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    authoring::tests::tap(&ctx, &mut app, "View")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    press(&ctx, &mut app, "Analyze")?;
    assert_eq!(app.tab, Tab::Analyze, "the View menu entry opens the tab");
    Ok(())
}

#[test]
fn a_newcomer_opens_every_analyze_page_from_the_aside() -> Result<(), String> {
    let _tall = TallScope::enter();
    let (ctx, mut app) = analyze_open();
    // Each row of the aside, the entry it picks and a card title its page paints.
    let rows: [(&str, analyze::Entry, &str); 23] = [
        ("Design rules", analyze::Entry::DesignRules, "findings"),
        (
            "Electrical checks",
            analyze::Entry::ElectricalChecks,
            "What this check means",
        ),
        ("Placement", analyze::Entry::Placement, "No page yet"),
        (
            "Impedance targets",
            analyze::Entry::ImpedanceTargets,
            "No page yet",
        ),
        ("Return path", analyze::Entry::ReturnPath, "No page yet"),
        ("Crosstalk", analyze::Entry::Crosstalk, "No page yet"),
        (
            "Reflections and levels",
            analyze::Entry::Reflections,
            "Noise margin",
        ),
        ("Eye diagram", analyze::Entry::EyeDiagram, "No page yet"),
        ("Rails", analyze::Entry::Rails, "No page yet"),
        (
            "PDN impedance",
            analyze::Entry::PdnImpedance,
            "Load step response",
        ),
        ("Load step", analyze::Entry::LoadStep, "Load step response"),
        ("UART", analyze::Entry::Uart, "Decoded lanes"),
        ("SPI", analyze::Entry::Spi, "Decoded lanes"),
        ("I2C", analyze::Entry::I2c, "Decoded lanes"),
        ("CAN", analyze::Entry::Can, "Decoded lanes"),
        ("Scope", analyze::Entry::Scope, "Measurements"),
        ("Firmware", analyze::Entry::Firmware, "Console"),
        (
            "Hardware link",
            analyze::Entry::HardwareLink,
            "Simulated and measured trace",
        ),
        ("IDE link", analyze::Entry::IdeLink, "How it will work"),
        ("RF", analyze::Entry::Rf, "S11 on a Smith chart"),
        (
            "Antenna (future)",
            analyze::Entry::Antenna,
            "Radiation pattern",
        ),
        (
            "Rule sets",
            analyze::Entry::RuleSets,
            "Rule sets are edited on the Rules page",
        ),
        (
            "Net classes",
            analyze::Entry::NetClasses,
            "Rules for groups of nets",
        ),
    ];
    for (label, entry, text) in rows {
        press(&ctx, &mut app, label)?;
        assert_eq!(app.analyze.entry, entry, "pressing {label} picks its entry");
        assert!(
            paints_text(&ctx, &mut app, text)?,
            "{label} opens a page that paints {text}: {}",
            painted(&ctx, &mut app)
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_filters_the_analyses_by_name() -> Result<(), String> {
    let _tall = TallScope::enter();
    let (ctx, mut app) = analyze_open();
    assert!(
        paints_text(&ctx, &mut app, "Crosstalk")?,
        "Crosstalk is listed"
    );
    type_into(
        &ctx,
        &mut app,
        "textinput:Filter checks and analyses",
        "scope",
    )?;
    assert!(
        !paints_text(&ctx, &mut app, "Crosstalk")?,
        "a name that does not match drops out of the list"
    );
    assert!(
        paints_text(&ctx, &mut app, "Scope")?,
        "a name that matches stays: {}",
        painted(&ctx, &mut app)
    );
    Ok(())
}

#[test]
fn a_newcomer_is_told_what_is_missing_on_the_pages_with_no_data_source() -> Result<(), String> {
    let _tall = TallScope::enter();
    let (ctx, mut app) = analyze_open();
    for (label, title, sentence) in [
        ("Scope", "No waveform yet", "No simulation feeds it yet"),
        (
            "PDN impedance",
            "No rail analysed yet",
            "decoupling capacitors",
        ),
        (
            "Reflections and levels",
            "No line analysed yet",
            "driver, a receiver",
        ),
        ("RF", "No spectrum yet", "RF source model"),
        ("Antenna (future)", "No pattern yet", "later release"),
        (
            "Firmware",
            "No image loaded",
            "Running firmware is not built yet",
        ),
        ("Hardware link", "No link yet", "hardware connection"),
        ("IDE link", "Not connected", "None of this is built yet"),
    ] {
        press(&ctx, &mut app, label)?;
        assert!(
            paints_text(&ctx, &mut app, title)?,
            "{label} says {title}: {}",
            painted(&ctx, &mut app)
        );
        assert!(
            paints_text(&ctx, &mut app, sentence)?,
            "{label} says what is missing ({sentence}): {}",
            painted(&ctx, &mut app)
        );
        assert!(
            paints_text(&ctx, &mut app, "Not set")?,
            "{label} Properties rows read Not set, not a made-up value"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_sees_the_buttons_with_nothing_behind_them_disabled() -> Result<(), String> {
    let _tall = TallScope::enter();
    let (ctx, mut app) = analyze_open();
    press(&ctx, &mut app, "Scope")?;
    for label in [
        "Run",
        "Stop",
        "Single",
        "Probe",
        "Cursors",
        "FFT",
        "Export CSV",
        "Export image",
    ] {
        assert!(
            !is_enabled(&ctx, &mut app, label)?,
            "{label} on the Scope page does nothing yet, so it is disabled"
        );
    }
    press(&ctx, &mut app, "Hardware link")?;
    for label in ["Connect", "Disconnect"] {
        assert!(
            !is_enabled(&ctx, &mut app, label)?,
            "{label} on the Hardware link page is disabled"
        );
    }
    press(&ctx, &mut app, "Firmware")?;
    assert!(
        !is_enabled(&ctx, &mut app, "Choose image")?,
        "Choose image is disabled until firmware can run"
    );
    press(&ctx, &mut app, "IDE link")?;
    assert!(
        !is_enabled(&ctx, &mut app, "Run pushed image")?,
        "Run pushed image is disabled until a link exists"
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_the_tab_that_will_make_the_scope_data() -> Result<(), String> {
    let _tall = TallScope::enter();
    let (ctx, mut app) = analyze_open();
    press(&ctx, &mut app, "Scope")?;
    press(&ctx, &mut app, "Open Schematic")?;
    assert_eq!(
        app.tab,
        Tab::Schematic,
        "the empty state sends you to the Schematic"
    );
    app.change_tab(Tab::Analyze);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    press(&ctx, &mut app, "Reflections and levels")?;
    press(&ctx, &mut app, "Open Layout")?;
    assert_eq!(app.tab, Tab::Pcb, "the empty state sends you to the Layout");
    app.change_tab(Tab::Analyze);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    press(&ctx, &mut app, "Rule sets")?;
    press(&ctx, &mut app, "Open Rules")?;
    assert_eq!(app.tab, Tab::Rules, "Rule sets opens the Rules page");
    Ok(())
}

#[test]
fn a_newcomer_reads_the_real_finding_counts_on_the_checks_page() -> Result<(), String> {
    let _tall = TallScope::enter();
    let (ctx, mut app) = analyze_open();
    let set = app.findings();
    let electrical = set
        .visible(bottom_sheet::SeverityFilter::All)
        .filter(|r| r.key == "erc" || r.key == "peripheral")
        .count();
    let design = set
        .visible(bottom_sheet::SeverityFilter::All)
        .filter(|r| r.key == "drc" || r.key == "lib")
        .count();
    press(&ctx, &mut app, "Electrical checks")?;
    assert!(
        paints_text(&ctx, &mut app, &format!("{electrical} findings"))?,
        "the page counts the {electrical} electrical findings the design reports: {}",
        painted(&ctx, &mut app)
    );
    // The aside row carries the same number, in its accessible name.
    let row = control_named(&ctx, &app, "Electrical checks")?;
    assert!(row.id.is_some(), "the aside row is named");
    press(&ctx, &mut app, "Design rules")?;
    assert!(
        paints_text(&ctx, &mut app, &format!("{design} findings"))?,
        "the page counts the {design} design rule findings: {}",
        painted(&ctx, &mut app)
    );
    Ok(())
}

#[test]
fn a_newcomer_runs_the_checks_again_from_analyze() -> Result<(), String> {
    let _tall = TallScope::enter();
    let (ctx, mut app) = analyze_open();
    app.message.clear();
    press(&ctx, &mut app, "Run all")?;
    assert_eq!(
        app.message,
        t!("sheet.checks_ran"),
        "Run all runs the checks and says so"
    );
    app.message.clear();
    press(&ctx, &mut app, "Electrical checks")?;
    press(&ctx, &mut app, "Run selected")?;
    assert_eq!(
        app.message,
        t!("sheet.checks_ran"),
        "Run selected on a Checks page runs them too"
    );
    assert!(
        !is_enabled(&ctx, &mut app, "Export CSV")?,
        "Export CSV is disabled: findings cannot be saved as a spreadsheet yet"
    );
    Ok(())
}

#[test]
fn a_newcomer_shows_a_finding_and_reads_it_in_properties() -> Result<(), String> {
    let _tall = TallScope::enter();
    let (ctx, mut app) = analyze_open();
    let set = app.findings();
    let first = set
        .visible(bottom_sheet::SeverityFilter::All)
        .find(|r| matches!(r.key, "erc" | "peripheral"))
        .cloned();
    let Some(first) = first else {
        // The sample has no electrical finding: the page says so plainly.
        press(&ctx, &mut app, "Electrical checks")?;
        assert!(
            paints_text(&ctx, &mut app, "This check found nothing")?,
            "an empty check says so: {}",
            painted(&ctx, &mut app)
        );
        return Ok(());
    };
    press(&ctx, &mut app, "Electrical checks")?;
    for level in ["Errors", "Warnings", "Notes", "All"] {
        press(&ctx, &mut app, level)?;
    }
    press(&ctx, &mut app, "Show")?;
    assert!(
        app.analyze.shown.is_some(),
        "pressing Show keeps the finding for Properties"
    );
    assert!(
        paints_text(&ctx, &mut app, &first.code)?,
        "Properties names the check {}: {}",
        first.code,
        painted(&ctx, &mut app)
    );
    Ok(())
}

#[test]
fn a_newcomer_reads_the_net_classes_of_the_design() -> Result<(), String> {
    let _tall = TallScope::enter();
    let (ctx, mut app) = analyze_open();
    press(&ctx, &mut app, "Net classes")?;
    assert!(
        paints_text(&ctx, &mut app, "No net classes yet")?,
        "a design with no class says so: {}",
        painted(&ctx, &mut app)
    );
    app.tool_call(
        "netclass.add",
        json!({"name": "High speed", "clearance": 0.15, "width": 0.15}),
    );
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(app.project.net_classes.len(), 1, "the class was added");
    assert!(
        paints_text(&ctx, &mut app, "High speed")?,
        "the class is listed by name: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        paints_text(&ctx, &mut app, "0.15 mm")?,
        "its width and clearance come from the design: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        paints_text(&ctx, &mut app, "Not set")?,
        "a value the class does not set reads Not set"
    );
    press(&ctx, &mut app, "High speed")?;
    press(&ctx, &mut app, "Edit net classes")?;
    assert!(
        app.project_settings.open,
        "Edit net classes opens Project settings"
    );
    assert_eq!(
        app.project_settings.tab,
        project_settings::TAB_CLASSES,
        "on the Net classes tab"
    );
    Ok(())
}
