use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_sees_findings_grouped_by_check() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let mut best: Option<(String, usize)> = None;
    for key in ["erc", "drc"] {
        if let Some(findings) = app.checks.get(key).and_then(serde_json::Value::as_array) {
            let mut counts: Vec<(String, usize)> = Vec::new();
            for finding in findings {
                let code = finding
                    .get("code")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("finding")
                    .to_owned();
                if let Some(row) = counts.iter_mut().find(|(c, _)| *c == code) {
                    row.1 += 1;
                } else {
                    counts.push((code, 1));
                }
            }
            if let Some(top) = counts.iter().max_by_key(|(_, n)| *n)
                && best.as_ref().is_none_or(|(_, n)| top.1 > *n)
            {
                best = Some(top.clone());
            }
        }
    }
    let (code, count) = best.ok_or("the practice board should have findings")?;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Checks")?;
    let words = code.replace('_', " ");
    let mut chars = words.chars();
    let human = match chars.next() {
        Some(first) => first
            .to_uppercase()
            .next()
            .into_iter()
            .chain(chars)
            .collect::<String>(),
        None => words,
    };
    let title = format!("{human} · {count} places");
    let mut shown = false;
    for _ in 0..10 {
        if paints_text(&ctx, &mut app, &title)? {
            shown = true;
            break;
        }
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(720.0, 500.0)),
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
        shown,
        "the checks list should group every place one check fired under one heading ({title}): {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Whether the wiring makes electrical sense")?,
        "each checks group should say in plain words what it looks at: {}",
        app.message
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
    authoring::tests::tap(&ctx, &mut app, "Rules")?;
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
    authoring::tests::tap(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Quiet this area")?,
        "every finding offers to quiet its corner of the board: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Quiet this area")?;
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
    authoring::tests::tap(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Hide this")?,
        "every finding offers to be hidden: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Hide this")?;
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
    authoring::tests::tap(&ctx, &mut app, "Rules")?;
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
    authoring::tests::tap(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Your check: keep it small · 1 place")?,
        "the review shows the check under the name it was given: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "1 to fix")?,
        "a breach of a must-fix check lands in the footer count: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Rules")?;
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
    authoring::tests::tap(&ctx, &mut app, "Checks")?;
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
    authoring::tests::tap(&ctx, &mut app, "Checks")?;
    // Fold the grid away and open it again, like someone tidying the page.
    authoring::tests::tap(&ctx, &mut app, "Pin connections")?;
    authoring::tests::tap(&ctx, &mut app, "Pin connections")?;
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
