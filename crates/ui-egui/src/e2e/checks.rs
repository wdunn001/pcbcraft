use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_sees_findings_as_rows_in_the_bottom_sheet() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        !app.region().bottom_open,
        "the sheet starts collapsed to its tab strip"
    );
    assert!(
        paints_text(&ctx, &mut app, "Console")?,
        "the collapsed sheet still shows its tabs"
    );
    authoring::tests::open_page(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(app.region().bottom_open, "View > Checks opens the sheet");
    for word in ["Run checks", "All findings", "Show", "Hide", "Mute"] {
        assert!(
            paints_text(&ctx, &mut app, word)?,
            "the Findings tab offers {word}: {}",
            app.message
        );
    }
    let first = app
        .checks
        .get("erc")
        .and_then(Value::as_array)
        .and_then(|v| v.first())
        .or_else(|| {
            app.checks
                .get("drc")
                .and_then(Value::as_array)
                .and_then(|v| v.first())
        })
        .and_then(|f| f.get("message"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or("the practice board should have findings")?;
    assert!(
        paints_text(&ctx, &mut app, &first)?,
        "a finding is one row with its message: {first}"
    );
    Ok(())
}
#[test]
fn a_newcomer_quiets_a_check_that_nags() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Rules")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for _ in 0..2 {
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(1180.0, 500.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -4.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    assert!(
        paints_text(&ctx, &mut app, "HOW STRICT TO BE")?,
        "the rules dock should also say how loud each check is: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "off")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert!(
        app.project.rule_disabled("missing_footprint"),
        "the bottom check is silenced by one click: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Auto")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert!(
        !app.project.rule_severity.contains_key("missing_footprint"),
        "Auto hands the choice back to the built-in default: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_silences_a_check_around_a_spot() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let total = |app: &App| -> usize {
        ["erc", "drc"]
            .iter()
            .map(|k| {
                app.checks
                    .get(*k)
                    .and_then(Value::as_array)
                    .map_or(0, |v| v.len())
            })
            .sum()
    };
    let before = total(&app);
    assert!(
        before > 0,
        "the practice board has findings to quiet: {}",
        app.message
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Mute")?,
        "every finding offers to quiet its corner of the board: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Mute")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let code = app
        .project
        .rule_regions
        .keys()
        .next()
        .cloned()
        .ok_or("one click should draw a quiet zone")?;
    assert!(
        total(&app) < before,
        "the finding inside that zone stops nagging: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Quiet zones")?,
        "the quiet zones are listed so they can be cleared: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, &format!("Clear {code} zones"))?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert!(
        app.project
            .rule_regions
            .get(&code)
            .is_none_or(|v| v.is_empty()),
        "clearing hands the whole board back to the check: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_hides_a_finding_they_have_checked() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tool_call(
        "component.add",
        json!({"kind":"resistor","position":{"x":20.0,"y":20.0},"reference":"R9"}),
    );
    let r9 = |app: &App| -> usize {
        app.checks
            .get("erc")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter(|r| {
                        r.get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .contains("R9")
                    })
                    .count()
            })
            .unwrap_or(0)
    };
    let total = |app: &App| -> usize {
        ["erc", "drc"]
            .iter()
            .map(|k| {
                app.checks
                    .get(*k)
                    .and_then(Value::as_array)
                    .map_or(0, |v| v.len())
            })
            .sum()
    };
    assert!(
        r9(&app) == 2 && total(&app) > 2,
        "the stray part adds two findings to a board that already has some copper to flag: {}",
        app.message
    );
    let before = total(&app);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Hide")?,
        "every finding offers to be hidden: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Hide")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert!(
        total(&app) < before,
        "hiding one finding takes it out of the list: {}",
        app.message
    );
    let code = app
        .project
        .rule_exclusions
        .keys()
        .next()
        .cloned()
        .ok_or("a hidden finding should be remembered by its check")?;
    let label = format!("Bring back {code}");
    assert!(
        paints_text(&ctx, &mut app, &label)?,
        "the app keeps a way back for everything hidden: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, &label)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        total(&app),
        before,
        "bringing it back shows every hidden finding again: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_writes_their_own_check() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Rules")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for _ in 0..1 {
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(1180.0, 500.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -4.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    assert!(
        paints_text(&ctx, &mut app, "CHECKS YOU WRITE")?,
        "the rules dock invites a check of your own: {}",
        app.message
    );
    app.custom_name = "keep it small".into();
    app.custom_measure = 2;
    app.custom_limit = 2.0;
    app.custom_loud = true;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Add this check")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert!(
        app.project
            .custom_checks
            .iter()
            .any(|c| c.name == "keep it small"),
        "the check is written into the design: {}",
        app.message
    );
    authoring::tests::open_page(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let named = app
        .checks
        .get("drc")
        .and_then(Value::as_array)
        .into_iter()
        .chain(app.checks.get("erc").and_then(Value::as_array))
        .flatten()
        .find(|f| {
            f.get("code")
                .and_then(Value::as_str)
                .is_some_and(|c| c.starts_with("custom_"))
        })
        .and_then(|f| f.get("message"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or("the custom check should fire on the practice board")?;
    assert!(
        paints_text(&ctx, &mut app, &named)?,
        "the sheet lists the check's finding: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "1 error")?,
        "a breach of a must-fix check lands in the footer count: {}",
        app.message
    );
    authoring::tests::open_page(&ctx, &mut app, "Rules")?;
    scrolled_to_at(&ctx, &mut app, "Remove this check", 700.0)?;
    authoring::tests::tap(&ctx, &mut app, "Remove this check")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert!(
        app.project.custom_checks.is_empty(),
        "one click takes the check away again: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_jumps_from_a_finding_to_the_copper_it_names() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    for name in ["GND", "LED", "VCC"] {
        app.change_tab(Tab::Checks);
        app.selected = None;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap_id(
            &ctx,
            &mut app,
            &format!("button:{name} has disconnected copper islands"),
        )?;
        assert_eq!(app.tab, Tab::Pcb, "{name}: the finding opens the board");
        assert!(
            app.selected.is_some(),
            "{name}: the finding picks its copper"
        );
    }
    Ok(())
}

/// A newcomer opens the sample board that carries three known problems and
/// reads them on the Checks sheet: one error and two warnings.
#[test]
fn a_newcomer_reads_the_showcase_findings() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
        .join("showcase-with-findings.pcbcraft");
    let path = path.to_str().ok_or("the example path is text")?.to_owned();
    assert!(
        app.tool_call("open", json!({ "path": path })),
        "the example opens: {}",
        app.message
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let level = |name: &str| -> usize {
        ["erc", "drc"]
            .iter()
            .filter_map(|key| app.checks.get(*key).and_then(serde_json::Value::as_array))
            .flatten()
            .filter(|f| f.get("severity").and_then(serde_json::Value::as_str) == Some(name))
            .count()
    };
    assert_eq!(level("error"), 1, "one error is waiting");
    assert_eq!(level("warning"), 2, "two warnings are waiting");
    authoring::tests::open_page(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let error_message = ["erc", "drc"]
        .iter()
        .filter_map(|key| app.checks.get(*key).and_then(serde_json::Value::as_array))
        .flatten()
        .find(|f| f.get("severity").and_then(serde_json::Value::as_str) == Some("error"))
        .and_then(|f| f.get("message"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or("the error finding has a message")?;
    assert!(
        paints_text(&ctx, &mut app, &error_message)?,
        "the error is named on the Checks sheet: {error_message}"
    );
    Ok(())
}

/// A design with two outputs, U1 pin 1 and U2 pin 1, wired to one net.
fn two_outputs_on_one_net() -> Result<(App, egui::Context), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.run("net.create", json!({"name": "BUS"}));
    app.refresh();
    let net = app
        .project
        .nets
        .first()
        .map(|n| n.id)
        .ok_or("net missing")?;
    let mut parts = vec![];
    for (reference, x) in [("U1", 0.0), ("U2", 10.0)] {
        app.run(
            "component.add",
            json!({"kind":"resistor","reference":reference,"position":{"x":x,"y":0.0}}),
        );
        app.refresh();
        let id = app
            .project
            .components
            .iter()
            .find(|c| c.reference == reference)
            .map(|c| c.id)
            .ok_or("part missing")?;
        parts.push(id);
    }
    {
        let mut session = app.session.lock().map_err(|_| "session busy")?;
        for component in session.project.components.iter_mut() {
            for pin in component.pins.iter_mut().filter(|p| p.number == "1") {
                pin.electrical_type = "output".into();
            }
        }
    }
    app.run(
        "net.connect",
        json!({"net": net, "pins": [{"component": parts[0], "pin": "1"}, {"component": parts[1], "pin": "1"}]}),
    );
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((app, ctx))
}

/// Click the matrix cell for two pin types, scrolling the grid into view.
fn tap_matrix_cell(
    ctx: &egui::Context,
    app: &mut App,
    type_a: &str,
    type_b: &str,
) -> Result<(), String> {
    if !scrolled_to_at(ctx, app, "Pin connections", 700.0)? {
        return Err("the Checks tab should offer a Pin connections grid".into());
    }
    if !scrolled_to_at(ctx, app, "Unspecified", 700.0)? {
        return Err("the whole grid should be reachable by scrolling".into());
    }
    for _ in 0..3 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    let rect = ctx
        .data(|d| d.get_temp::<egui::Rect>(pin_matrix::cell_memory_id(type_a, type_b)))
        .ok_or("the grid cell was never painted")?;
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(rect.center())],
        egui::Modifiers::NONE,
    );
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    app.refresh();
    Ok(())
}

#[test]
fn a_newcomer_sees_two_outputs_fighting() -> Result<(), String> {
    let (mut app, ctx) = two_outputs_on_one_net()?;
    let (errors, _, _) = App::finding_counts(&app.checks);
    assert!(
        errors >= 1,
        "two outputs on one net should count as an error in the footer: {}",
        app.message
    );
    authoring::tests::open_page(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Pin conflict · 1 place")?
            || scrolled_to_at(&ctx, &mut app, "Pin conflict · 1 place", 700.0)?,
        "the review list should group the clash under its own heading"
    );
    assert!(
        scrolled_to_at(
            &ctx,
            &mut app,
            "U1 pin 1 (output) and U2 pin 1 (output) drive the same net; keep one driver or add a resistor",
            700.0
        )?,
        "the review list should name both pins: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_lets_two_outputs_share_a_net() -> Result<(), String> {
    let (mut app, ctx) = two_outputs_on_one_net()?;
    let conflicts = |app: &App| -> usize {
        app.checks
            .get("erc")
            .and_then(serde_json::Value::as_array)
            .map_or(0, |v| {
                v.iter()
                    .filter(|f| {
                        f.get("code").and_then(serde_json::Value::as_str) == Some("pin_conflict")
                    })
                    .count()
            })
    };
    assert_eq!(conflicts(&app), 1, "the clash starts out reported");
    authoring::tests::open_page(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // The cell starts at error; one click cycles error to ok.
    tap_matrix_cell(&ctx, &mut app, "output", "output")?;
    assert_eq!(
        app.project.pin_matrix_level("output", "output"),
        "ok",
        "one click on the output cell lets two outputs share a net: {}",
        app.message
    );
    assert_eq!(conflicts(&app), 0, "the finding disappears");
    assert!(
        scrolled_to_at(&ctx, &mut app, "Reset to default", 700.0)?,
        "the Reset button stays on the page"
    );
    authoring::tests::tap(&ctx, &mut app, "Reset to default")?;
    app.refresh();
    assert_eq!(conflicts(&app), 1, "Reset brings the finding back");
    assert!(app.project.pin_matrix.is_empty());
    Ok(())
}

#[test]
fn a_newcomer_reads_every_matrix_cell_aloud() {
    for a in pcbcraft_doc::PIN_ELECTRICAL_TYPES {
        for b in pcbcraft_doc::PIN_ELECTRICAL_TYPES {
            let level = pcbcraft_doc::default_pin_matrix_level(a, b);
            let next = pin_matrix::next_level(level);
            assert_ne!(next, level, "a click always changes {a} with {b}");
            assert_eq!(
                pin_matrix::next_level(pin_matrix::next_level(next)),
                level,
                "three clicks come back to the start for {a} with {b}"
            );
        }
    }
}
