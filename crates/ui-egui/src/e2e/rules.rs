use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_adds_a_rule_set_and_tunes_its_numbers() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Rules")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "No rule sets yet")?,
        "a design without rule sets says so"
    );
    authoring::tests::tap(&ctx, &mut app, "New class")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.net_classes.len(),
        1,
        "one click adds one rule set: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Gap 0.200 mm")?,
        "the new rule set shows its starting numbers"
    );
    let id = app.project.net_classes[0].id;
    authoring::tests::tap(&ctx, &mut app, "Tune")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.rule_draft = Some((id, 0.5, 0.6));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Apply")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let class = &app.project.net_classes[0];
    assert!(
        (class.clearance - 0.5).abs() < 1e-9 && (class.width - 0.6).abs() < 1e-9,
        "the tuned numbers stick: {} {} · {}",
        class.clearance,
        class.width,
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Remove")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.net_classes.is_empty(),
        "the rule set is removed: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_puts_a_connection_under_a_rule_set_and_lets_it_go() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Rules")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "New class")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let vcc = app
        .project
        .nets
        .iter()
        .find(|n| n.name == "VCC")
        .map(|n| n.id)
        .ok_or("the sample should have a VCC connection")?;
    authoring::tests::tap(&ctx, &mut app, "choose a connection to put here")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "VCC")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Assign")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.net_classes[0].nets.contains(&vcc),
        "the connection joins the rule set: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "1 nets")?,
        "the row counts its connections"
    );
    authoring::tests::tap(&ctx, &mut app, "\u{d7}")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.net_classes[0].nets.is_empty(),
        "the connection is released to the defaults: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_opens_rules_from_the_top() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Rules")?;
    assert!(
        paints_text(&ctx, &mut app, "Design rules travel with the design")?,
        "the Rules tab explains what rules are for: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "New class")?,
        "the Rules tab offers a way to add a rule set"
    );
    authoring::tests::open_page(&ctx, &mut app, "Settings")?;
    assert!(
        paints_text(&ctx, &mut app, "APPEARANCE")?,
        "the Settings tab opens with the look of the app"
    );
    assert!(
        scrolled_to_at(&ctx, &mut app, "BOARD GEOMETRY", 700.0)?,
        "the board settings that used to crowd the aside now live on the Settings tab"
    );
    assert!(
        paints_text(&ctx, &mut app, "Nothing selected")?,
        "the aside no longer holds the project settings"
    );
    authoring::tests::tap(&ctx, &mut app, "Dark")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(app.preferences.dark, "the theme switch on the tab works");
    Ok(())
}

fn sample_in_rules() -> Result<(App, egui::Context), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    app.change_tab(Tab::Rules);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((app, ctx))
}

/// How many clearance findings the checks report right now.
fn clearance_findings(app: &App) -> usize {
    app.checks
        .get("drc")
        .and_then(Value::as_array)
        .map_or(0, |all| {
            all.iter()
                .filter(|f| f.get("code").and_then(Value::as_str) == Some("clearance"))
                .count()
        })
}

#[test]
fn a_newcomer_picks_a_fab_preset_and_sees_the_numbers_change() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Rules")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to_at(&ctx, &mut app, "Choose a maker", 700.0)?,
        "a fresh design offers a maker to pick"
    );
    app.project.board.rules = None;
    authoring::tests::tap(&ctx, &mut app, "Choose a maker")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "JLCPCB 1-2L")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let rules = app
        .project
        .board
        .rules
        .clone()
        .ok_or_else(|| format!("picking a maker fills the limits: {}", app.message))?;
    assert_eq!(rules.preset.as_deref(), Some("JLCPCB 1-2L"));
    assert!(rules.min_track.is_some(), "the minimums fill in");
    assert!(
        paints_text(&ctx, &mut app, "preset JLCPCB 1-2L")?,
        "each number names the preset it came from"
    );
    app.tool_call("undo", json!({}));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.board.rules.is_none(),
        "one Undo clears every limit"
    );
    assert!(
        !paints_text(&ctx, &mut app, "preset JLCPCB 1-2L")?,
        "the origin word goes with the numbers"
    );
    Ok(())
}

#[test]
fn a_newcomer_keeps_two_classes_further_apart() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let (net_a, net_b) = {
        let mut ids = app.project.nets.iter().map(|n| n.id);
        (
            ids.next().ok_or("the sample has a first connection")?,
            ids.next().ok_or("the sample has a second connection")?,
        )
    };
    app.tool_call(
        "netclass.add",
        json!({"name": "Signal", "clearance": 0.2, "width": 0.25, "nets": [net_a]}),
    );
    app.tool_call(
        "netclass.add",
        json!({"name": "Power", "clearance": 0.2, "width": 0.25, "nets": [net_b]}),
    );
    app.tool_call(
        "track.add",
        json!({"net": net_a, "start": {"x": 2.0, "y": 2.0}, "end": {"x": 10.0, "y": 2.0}, "width": 0.25}),
    );
    app.tool_call(
        "track.add",
        json!({"net": net_b, "start": {"x": 2.0, "y": 2.55}, "end": {"x": 10.0, "y": 2.55}, "width": 0.25}),
    );
    app.refresh();
    let before = clearance_findings(&app);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Rules")?;
    assert!(
        scrolled_to_at(&ctx, &mut app, "Add gap", 700.0)?,
        "the Rules page has a place to add a gap between classes"
    );
    authoring::tests::tap(&ctx, &mut app, "Add gap")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.class_pairs.len(),
        1,
        "one click adds one gap: {}",
        app.message
    );
    assert!(
        clearance_findings(&app) > before,
        "wires of the two classes closer than the new gap become a finding"
    );
    assert!(
        paints_text(&ctx, &mut app, "class pair")?,
        "the new gap says it is a class pair"
    );
    Ok(())
}

#[test]
fn a_newcomer_names_a_pair_of_wires_and_picks_both_sides() -> Result<(), String> {
    let (mut app, ctx) = sample_in_rules()?;
    type_into(&ctx, &mut app, "textinput:USB", "Audio")?;
    // Opening each list shows the connections to choose from.
    for list in ["combobox:Positive net", "combobox:Negative net"] {
        tap_id(&ctx, &mut app, list)?;
        tap_id(&ctx, &mut app, list)?;
    }
    type_into(&ctx, &mut app, "spinbutton:Run together gap", "0.35")?;
    tap_id(&ctx, &mut app, "button:Declare pair")?;
    app.refresh();
    assert!(
        app.project
            .board
            .diff_pairs
            .iter()
            .any(|d| d.name == "Audio" && (d.gap - 0.35).abs() < 1e-9 && d.net_p != d.net_n),
        "the pair carries the typed name and gap: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_picks_two_connections_to_keep_apart_by_a_set_distance() -> Result<(), String> {
    let (mut app, ctx) = sample_in_rules()?;
    for list in [
        "combobox:First net of the pair",
        "combobox:Second net of the pair",
    ] {
        tap_id(&ctx, &mut app, list)?;
        tap_id(&ctx, &mut app, list)?;
    }
    type_into(&ctx, &mut app, "spinbutton:Keep apart by", "0.45")?;
    tap_id(&ctx, &mut app, "button:Set spacing")?;
    app.refresh();
    assert!(
        app.project
            .board
            .pair_rules
            .iter()
            .any(|r| (r.clearance - 0.45).abs() < 1e-9),
        "the spacing rule keeps the typed distance: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_marks_a_stricter_corner_with_a_height_limit() -> Result<(), String> {
    let (mut app, ctx) = sample_in_rules()?;
    let areas = app.project.board.rule_areas.len();
    type_into(&ctx, &mut app, "spinbutton:Area corner X", "2")?;
    type_into(&ctx, &mut app, "spinbutton:Area corner Y", "3")?;
    type_into(&ctx, &mut app, "spinbutton:Area width", "12")?;
    type_into(&ctx, &mut app, "spinbutton:Area height", "9")?;
    type_into(&ctx, &mut app, "spinbutton:Copper stays apart by", "0.4")?;
    tap_id(&ctx, &mut app, "checkbox:Parts here stand no taller than")?;
    assert!(app.area_limited, "the height limit is switched on");
    tap_id(&ctx, &mut app, "button:Add area")?;
    app.refresh();
    let area = app
        .project
        .board
        .rule_areas
        .get(areas)
        .ok_or_else(|| format!("one click marks one area: {}", app.message))?;
    assert!(
        (area.clearance - 0.4).abs() < 1e-9
            && (area.region.x - 2.0).abs() < 1e-9
            && (area.region.width - 12.0).abs() < 1e-9,
        "the area sits where and as strict as typed"
    );
    Ok(())
}

#[test]
fn a_newcomer_writes_their_own_check_and_hardens_the_joints() -> Result<(), String> {
    let (mut app, ctx) = sample_in_rules()?;
    type_into(&ctx, &mut app, "spinbutton:Teardrop depth", "0.5")?;
    assert!(
        (app.teardrop_depth - 0.5).abs() < 1e-9,
        "the teardrop depth is the one typed"
    );
    let checks = app.project.custom_checks.len();
    type_into(
        &ctx,
        &mut app,
        "textinput:What this check means, in your words",
        "keep power wires meaty",
    )?;
    tap_id(&ctx, &mut app, "combobox:Custom check measure")?;
    tap_id(&ctx, &mut app, "combobox:Custom check measure")?;
    type_into(&ctx, &mut app, "spinbutton:Custom check limit", "0.4")?;
    tap_id(&ctx, &mut app, "button:a nudge")?;
    assert!(!app.custom_loud, "a nudge is only a mention");
    tap_id(&ctx, &mut app, "button:must fix")?;
    assert!(app.custom_loud, "must fix counts as a real problem");
    tap_id(&ctx, &mut app, "button:Add this check")?;
    app.refresh();
    assert_eq!(
        app.project.custom_checks.len(),
        checks + 1,
        "the new check is saved with the design: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_turns_a_built_in_check_up_to_error_and_down_to_warning() -> Result<(), String> {
    let (mut app, ctx) = sample_in_rules()?;
    tap_id(&ctx, &mut app, "button:error")?;
    app.refresh();
    let codes = [
        "clearance",
        "copper_short",
        "board_edge",
        "track_width",
        "track_length",
        "unrouted",
        "missing_footprint",
    ];
    assert!(
        codes
            .iter()
            .any(|code| app.project.rule_severity(code, "warning") == "error"),
        "pressing error makes one check a must-fix: {}",
        app.message
    );
    tap_id(&ctx, &mut app, "button:warning")?;
    app.refresh();
    assert!(
        codes
            .iter()
            .any(|code| app.project.rule_severity(code, "warning") == "warning"),
        "pressing warning turns a check back into a mention: {}",
        app.message
    );
    Ok(())
}

/// How many clearance findings the checks report right now.
fn clearance_findings(app: &App) -> usize {
    app.checks
        .get("drc")
        .and_then(Value::as_array)
        .map_or(0, |all| {
            all.iter()
                .filter(|f| f.get("code").and_then(Value::as_str) == Some("clearance"))
                .count()
        })
}

#[test]
fn a_newcomer_picks_a_fab_preset_and_sees_the_numbers_change() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Rules")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to_at(&ctx, &mut app, "Choose a maker", 700.0)?,
        "a fresh design offers a maker to pick"
    );
    app.project.board.rules = None;
    authoring::tests::tap(&ctx, &mut app, "Choose a maker")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "JLCPCB 1-2L")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let rules = app
        .project
        .board
        .rules
        .clone()
        .ok_or_else(|| format!("picking a maker fills the limits: {}", app.message))?;
    assert_eq!(rules.preset.as_deref(), Some("JLCPCB 1-2L"));
    assert!(rules.min_track.is_some(), "the minimums fill in");
    assert!(
        paints_text(&ctx, &mut app, "preset JLCPCB 1-2L")?,
        "each number names the preset it came from"
    );
    for field in [
        "Copper to cut edge",
        "Copper to scored edge",
        "Deepest hole ratio",
        "Narrowest silkscreen line",
        "Narrowest track",
        "Pad hole to copper",
        "Pad hole to hole",
        "Pad ring",
        "Plating allowance",
        "Silkscreen to pad",
        "Smallest copper gap",
        "Smallest drill",
        "Smallest silkscreen text",
        "Smallest unplated hole",
        "Solder mask bridge",
        "Unplated hole to copper",
        "Via hole to copper",
        "Via hole to hole",
        "Via ring",
    ] {
        tap_id(&ctx, &mut app, &format!("spinbutton:{field}"))?;
    }
    app.tool_call("undo", json!({}));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.board.rules.is_none(),
        "one Undo clears every limit"
    );
    assert!(
        !paints_text(&ctx, &mut app, "preset JLCPCB 1-2L")?,
        "the origin word goes with the numbers"
    );
    Ok(())
}

#[test]
fn a_newcomer_keeps_two_classes_further_apart() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let (net_a, net_b) = {
        let mut ids = app.project.nets.iter().map(|n| n.id);
        (
            ids.next().ok_or("the sample has a first connection")?,
            ids.next().ok_or("the sample has a second connection")?,
        )
    };
    app.tool_call(
        "netclass.add",
        json!({"name": "Signal", "clearance": 0.2, "width": 0.25, "nets": [net_a]}),
    );
    app.tool_call(
        "netclass.add",
        json!({"name": "Power", "clearance": 0.2, "width": 0.25, "nets": [net_b]}),
    );
    app.tool_call(
        "track.add",
        json!({"net": net_a, "start": {"x": 2.0, "y": 2.0}, "end": {"x": 10.0, "y": 2.0}, "width": 0.25}),
    );
    app.tool_call(
        "track.add",
        json!({"net": net_b, "start": {"x": 2.0, "y": 2.55}, "end": {"x": 10.0, "y": 2.55}, "width": 0.25}),
    );
    app.refresh();
    let before = clearance_findings(&app);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Rules")?;
    assert!(
        scrolled_to_at(&ctx, &mut app, "Add gap", 700.0)?,
        "the Rules page has a place to add a gap between classes"
    );
    authoring::tests::tap(&ctx, &mut app, "Add gap")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.class_pairs.len(),
        1,
        "one click adds one gap: {}",
        app.message
    );
    assert!(
        clearance_findings(&app) > before,
        "wires of the two classes closer than the new gap become a finding"
    );
    assert!(
        paints_text(&ctx, &mut app, "class pair")?,
        "the new gap says it is a class pair"
    );
    Ok(())
}
