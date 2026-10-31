//! The accessibility gate behind `cargo xtask a11y`.
//!
//! Context: walks every tab, the start page, the About dialog and each menu through
//! the AccessKit tree and counts what a screen reader or keyboard user trips on. Names,
//! target size and contrast have their own tests (`shell.rs`, `../target_size.rs`,
//! `../contrast.rs`); this file adds duplicate names, keyboard reach and a per-screen
//! table. Docs: `docs/accessibility.md`.
//! Local rules: counts are compared with `a11y-ratchet.txt` and may only go down. Set
//! `PCBCRAFT_A11Y_REPORT=<dir>` to write `report.json` for the job summary.
//!
//! # Dependencies
//! - crates/ui-egui/src/e2e/a11y-ratchet.txt
//! - crates/ui-egui/src/e2e/support.rs
//! - crates/ui-egui/src/a11y.rs
//! - xtask/src/a11y.rs reads the report
//!
//! # Do not
//! - Raise a count to make the gate pass; fix the control or file an issue and say so in the line.

use super::super::*;
use super::support::*;
use std::collections::{BTreeMap, BTreeSet};

/// Counts for one screen.
#[derive(Debug, Default, Clone)]
pub(crate) struct Row {
    pub screen: String,
    pub controls: usize,
    pub unnamed: usize,
    pub duplicate_names: usize,
    pub undersized: usize,
    pub unreached: usize,
    pub trapped: bool,
    pub duplicate_list: Vec<String>,
    pub undersized_list: Vec<String>,
}

/// Interactive nodes of one frame: id, name, parent id, disabled.
fn interactive(
    out: &egui::FullOutput,
) -> Vec<(u64, egui::accesskit::Role, String, Option<u64>, bool)> {
    let Some(update) = &out.platform_output.accesskit_update else {
        return Vec::new();
    };
    let mut parent: BTreeMap<u64, u64> = BTreeMap::new();
    for (id, node) in &update.nodes {
        for child in node.children() {
            parent.insert(child.0, id.0);
        }
    }
    update
        .nodes
        .iter()
        .filter(|(_, n)| a11y::audit::needs_name(n.role()))
        .map(|(id, n)| {
            (
                id.0,
                n.role(),
                n.label().unwrap_or_default().trim().to_owned(),
                parent.get(&id.0).copied(),
                n.is_disabled(),
            )
        })
        .collect()
}

/// Measure the screen the app shows now.
fn measure(ctx: &egui::Context, app: &mut App, screen: &str) -> Row {
    let rects = interactive_rects(ctx, app);
    let out = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let nodes = interactive(&out);
    let mut row = Row {
        screen: screen.to_owned(),
        controls: nodes.len(),
        ..Row::default()
    };
    row.unnamed = nodes.iter().filter(|n| n.2.is_empty()).count();
    let mut groups: BTreeMap<(Option<u64>, String, String), usize> = BTreeMap::new();
    for (_, role, name, parent, _) in &nodes {
        if !name.is_empty() {
            *groups
                .entry((*parent, format!("{role:?}"), name.clone()))
                .or_default() += 1;
        }
    }
    row.duplicate_names = groups.values().filter(|n| **n > 1).count();
    row.duplicate_list = groups
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|((_, role, name), n)| format!("{role} `{name}` x{n}"))
        .collect();
    let exempt = crate::target_size::exemptions().unwrap_or_default();
    row.undersized_list = crate::target_size::undersized_targets(&rects, App::MIN_TARGET)
        .into_iter()
        .filter(|l| !exempt.iter().any(|(e, _)| l.contains(e.as_str())))
        .collect();
    row.undersized = row.undersized_list.len();
    // Keyboard reach: press Tab and note every control that takes focus.
    let wanted: BTreeSet<u64> = nodes.iter().filter(|n| !n.4).map(|n| n.0).collect();
    let mut reached = BTreeSet::new();
    let mut last = None;
    let mut stuck = 0;
    for _ in 0..(nodes.len() + 8).min(160) {
        let tab = egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(ctx, app, vec![tab], egui::Modifiers::NONE);
        let now = ctx.memory(|m| m.focused()).map(|id| id.accesskit_id().0);
        if let Some(id) = now {
            reached.insert(id);
        }
        stuck = if now == last { stuck + 1 } else { 0 };
        last = now;
        if stuck >= 3 {
            break;
        }
    }
    row.trapped = stuck >= 3 && wanted.len() > 2;
    row.unreached = wanted.difference(&reached).count();
    row
}

pub(crate) fn practice() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}

/// Visit every screen a person reaches without typing: each tab, the lower dock's
/// second page, the start page, the About dialog and each menu. `setup` runs on a
/// fresh app before it is drawn; `visit` is called with the screen's name.
pub(crate) fn each_screen(
    setup: &dyn Fn(&mut App),
    visit: &mut dyn FnMut(&str, &egui::Context, &mut App),
) -> Result<(), String> {
    for tab in [
        Tab::Files,
        Tab::Schematic,
        Tab::Pcb,
        Tab::Board3d,
        Tab::Rules,
        Tab::Checks,
        Tab::Settings,
        Tab::Libraries,
        Tab::PlotFiles,
    ] {
        let (ctx, mut app) = practice();
        setup(&mut app);
        app.change_tab(tab);
        visit(&format!("{tab:?}"), &ctx, &mut app);
    }
    let (ctx, mut app) = practice();
    setup(&mut app);
    app.change_tab(Tab::Pcb);
    app.preferences.aside_lower_tab = 1;
    visit("Pcb, lower dock page 2", &ctx, &mut app);
    let (ctx, mut app) = practice();
    setup(&mut app);
    app.home_open = true;
    visit("Start page", &ctx, &mut app);
    let (ctx, mut app) = practice();
    setup(&mut app);
    app.about_open = true;
    visit("About dialog", &ctx, &mut app);
    for menu in ["File", "Edit", "View", "Help"] {
        let (ctx, mut app) = practice();
        authoring::tests::tap(&ctx, &mut app, menu)?;
        setup(&mut app);
        visit(&format!("{menu} menu"), &ctx, &mut app);
    }
    Ok(())
}

/// Every screen, measured.
pub(crate) fn walk() -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    each_screen(&|_| {}, &mut |name, ctx, app| {
        rows.push(measure(ctx, app, name))
    })?;
    Ok(rows)
}

/// `kind | screen | count | reason` lines of the ratchet file.
fn ratchet() -> Vec<(String, String, usize)> {
    include_str!("a11y-ratchet.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let p: Vec<&str> = l.splitn(4, '|').map(str::trim).collect();
            Some((
                p.first()?.to_string(),
                p.get(1)?.to_string(),
                p.get(2)?.parse().ok()?,
            ))
        })
        .collect()
}

/// Write the per-screen table for the job summary when asked.
fn write_report(rows: &[Row]) {
    let Ok(dir) = std::env::var("PCBCRAFT_A11Y_REPORT") else {
        return;
    };
    let list: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "screen": r.screen, "controls": r.controls, "unnamed": r.unnamed,
                "duplicate_names": r.duplicate_names, "undersized": r.undersized,
                "unreached": r.unreached, "trapped": r.trapped,
                "duplicates": r.duplicate_list,
            })
        })
        .collect();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(
        std::path::Path::new(&dir).join("report.json"),
        serde_json::to_string_pretty(&list).unwrap_or_default(),
    );
}

#[test]
fn a11y_walk_stays_within_the_ratchet() -> Result<(), String> {
    let rows = walk()?;
    write_report(&rows);
    assert!(
        rows.iter().map(|r| r.controls).sum::<usize>() > 100,
        "the walk should find real controls"
    );
    let allowed = ratchet();
    let limit = |kind: &str, screen: &str| {
        allowed
            .iter()
            .find(|(k, s, _)| k == kind && s == screen)
            .map_or(0, |(_, _, n)| *n)
    };
    let mut now_lines = Vec::new();
    let mut problems = Vec::new();
    for r in &rows {
        if r.undersized > limit("undersized", &r.screen) {
            problems.push(format!("{}: {}", r.screen, r.undersized_list.join("; ")));
        }
        if r.duplicate_names > limit("duplicate-name", &r.screen) {
            problems.push(format!("{}: {}", r.screen, r.duplicate_list.join("; ")));
        }
        for (kind, now) in [
            ("duplicate-name", r.duplicate_names),
            ("unreached-by-tab", r.unreached),
            ("focus-trap", usize::from(r.trapped)),
            ("undersized", r.undersized),
        ] {
            if now > 0 {
                now_lines.push(format!("{kind} | {} | {now} | ", r.screen));
            }
            let max = limit(kind, &r.screen);
            if now > max {
                problems.push(format!("{} on {}: {now}, allowed {max}", kind, r.screen));
            } else if now < max {
                problems.push(format!(
                    "{} on {}: now {now}, file says {max}; lower the count",
                    kind, r.screen
                ));
            }
        }
    }
    for (kind, screen, _) in &allowed {
        if !rows.iter().any(|r| &r.screen == screen) {
            problems.push(format!("ratchet names unknown screen `{screen}` ({kind})"));
        }
    }
    assert!(
        problems.is_empty(),
        "accessibility counts moved:\n{}\ncurrent lines for a11y-ratchet.txt:\n{}",
        problems.join("\n"),
        now_lines.join("\n")
    );
    Ok(())
}

#[test]
fn reduce_motion_switches_off_fades_and_collapses() -> Result<(), String> {
    let (ctx, mut app) = practice();
    app.preferences.reduced_motion = true;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(ctx.global_style().animation_time, 0.0);
    app.preferences.reduced_motion = false;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(ctx.global_style().animation_time > 0.0);
    Ok(())
}
