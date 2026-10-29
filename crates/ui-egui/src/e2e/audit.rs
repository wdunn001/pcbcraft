//! Control audit behind the e2e gate (`cargo xtask e2e`, issue #393).
//!
//! Two logs are written, only when the matching environment variable is set,
//! so ordinary test runs are unchanged:
//!
//! - `PCBCRAFT_E2E_TOUCH_LOG`: every `a_newcomer_*` story appends the id of
//!   each control it presses or types into (see [`record_touches`]).
//! - `PCBCRAFT_E2E_WALK_LOG`: [`e2e_audit_walk`] paints every page of the
//!   app, lists each control it sees (`P` lines) and each enabled control that
//!   shows no tooltip when hovered (`T` lines), and each control without a
//!   usable name (`N` lines).
//!
//! `cargo xtask e2e` compares the two logs. A control id is the widget kind
//! plus its label with digits folded to `#`, or `icon:<id>` for tool-strip
//! icons published through `App::record_icon`.

use super::super::*;
use super::support::{TALL_SCREEN, frame};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;

/// One interactive widget found on screen.
#[derive(Clone, Debug)]
pub(crate) struct Control {
    /// Stable name used by both logs. `None` when the widget has no label and
    /// is not a registered icon, which the gate reports as a gap.
    pub id: Option<String>,
    pub kind: String,
    pub widget: egui::Id,
    pub layer: egui::LayerId,
    pub rect: Rect,
    pub enabled: bool,
}

/// Digits become `#` so "R12" in a story and "R3" in the walk are one control.
pub(crate) fn fold_label(label: &str) -> String {
    let mut out = String::new();
    let mut last_digit = false;
    for ch in label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
    {
        if ch.is_ascii_digit() {
            if !last_digit {
                out.push('#');
            }
            last_digit = true;
        } else {
            out.push(ch);
            last_digit = false;
        }
        if out.chars().count() >= 60 {
            break;
        }
    }
    out
}

/// What the accessibility tree says about each widget of the last frame:
/// (role, label). egui fills it for every widget that calls `widget_info`,
/// in release and debug alike, which is why the audit reads it.
type Labels = std::collections::HashMap<u64, (egui::accesskit::Role, Option<String>, bool)>;

thread_local! {
    static LABELS: std::cell::RefCell<Labels> = std::cell::RefCell::new(Labels::new());
}

/// Called by the story `frame` helper after each frame.
pub(crate) fn remember_labels(output: &egui::FullOutput) {
    if let Some(update) = &output.platform_output.accesskit_update {
        let map: Labels = update
            .nodes
            .iter()
            .map(|(id, node)| {
                let label = node
                    .label()
                    .or_else(|| node.placeholder())
                    .map(str::to_owned);
                (id.0, (node.role(), label, node.is_disabled()))
            })
            .collect();
        LABELS.with(|labels| *labels.borrow_mut() = map);
    }
}

fn control_kind(role: egui::accesskit::Role) -> Option<String> {
    use egui::accesskit::Role as R;
    match role {
        R::Label
        | R::GenericContainer
        | R::Window
        | R::Pane
        | R::Unknown
        | R::ScrollBar
        | R::Splitter
        | R::Image
        | R::ProgressIndicator
        | R::Group
        | R::Dialog
        | R::Tooltip => None,
        other => Some(format!("{other:?}").to_lowercase()),
    }
}

fn near(a: Rect, b: Rect) -> bool {
    (a.min - b.min).abs().max_elem() < 1.5 && (a.max - b.max).abs().max_elem() < 1.5
}

/// Every interactive widget painted in the last frame.
pub(crate) fn controls(ctx: &egui::Context, app: &App) -> Vec<Control> {
    ctx.enable_accesskit();
    let labels = LABELS.with(|labels| labels.borrow().clone());
    ctx.viewport(|viewport| {
        let widgets = &viewport.this_pass.widgets;
        let mut out = Vec::new();
        for (layer, rects) in widgets.layers() {
            for w in rects {
                if !w.sense.interactive() || w.interact_rect.width() < 1.0 {
                    continue;
                }
                let icon = app
                    .icon_rects
                    .iter()
                    .find(|(_, rect)| near(**rect, w.rect))
                    .map(|(id, _)| id.clone());
                let node = labels.get(&w.id.value());
                let kind = node.and_then(|(role, _, _)| control_kind(*role));
                let disabled = node.is_some_and(|(_, _, off)| *off);
                let (id, kind) = if let Some(icon) = icon {
                    (Some(format!("icon:{icon}")), "icon".to_owned())
                } else if let (Some((_, label, _)), Some(kind)) = (node, kind.clone()) {
                    let label = label.clone().filter(|l| !l.trim().is_empty());
                    (label.map(|l| format!("{kind}:{}", fold_label(&l))), kind)
                } else if node.is_some() && kind.is_none() {
                    // Text, groups and panes that only sense a click because egui
                    // lets people select words. A screen reader reads their text.
                    continue;
                } else if w.sense.senses_click() && w.rect.size().max_elem() <= 48.0 {
                    (None, "small".to_owned())
                } else {
                    continue;
                };
                out.push(Control {
                    id,
                    kind,
                    widget: w.id,
                    layer: *layer,
                    rect: w.interact_rect,
                    enabled: w.enabled && !disabled,
                });
            }
        }
        out
    })
}

fn append(path: &Path, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let mut text = lines.join("\n");
        text.push('\n');
        let _ = file.write_all(text.as_bytes());
    }
}

/// Note which controls the events act on. Called by the story `frame` helpers
/// before each frame, so ids come from the widgets of the previous frame,
/// which is what a real click lands on.
pub(crate) fn record_touches(ctx: &egui::Context, app: &App, events: &[egui::Event]) {
    if let Some(path) = std::env::var_os("PCBCRAFT_E2E_TOUCH_LOG") {
        record_touches_to(Path::new(&path), ctx, app, events);
    }
}

pub(crate) fn record_touches_to(
    path: &Path,
    ctx: &egui::Context,
    app: &App,
    events: &[egui::Event],
) {
    let found = controls(ctx, app);
    let mut ids = BTreeSet::new();
    for event in events {
        let hit = match event {
            egui::Event::PointerButton {
                pos, pressed: true, ..
            } => {
                let top = ctx.layer_id_at(*pos);
                found
                    .iter()
                    .rev()
                    .find(|c| c.enabled && c.rect.contains(*pos) && Some(c.layer) == top)
                    .cloned()
            }
            egui::Event::Text(_) | egui::Event::Key { pressed: true, .. } => {
                let focused = ctx.memory(|m| m.focused());
                found.iter().find(|c| Some(c.widget) == focused).cloned()
            }
            _ => None,
        };
        if let Some(Control { id: Some(id), .. }) = hit {
            ids.insert(id);
        }
    }
    append(path, &ids.into_iter().collect::<Vec<_>>());
}

fn sample_app(tab: Option<&str>) -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, tab);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app
}

fn settle(ctx: &egui::Context, app: &mut App) {
    for _ in 0..3 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

/// Hover the control for two frames and say whether a tooltip appeared.
/// The control is looked up again after the pointer leaves, because panels
/// can still be moving when the page is first listed. The hover runs on a
/// screen tall enough for the whole page, so a control below the usual fold
/// still has an area to hover (#520).
fn shows_tooltip(ctx: &egui::Context, app: &mut App, control: &Control) -> bool {
    TALL_SCREEN.with(|tall| tall.set(true));
    let shown = shows_tooltip_on_tall_screen(ctx, app, control);
    TALL_SCREEN.with(|tall| tall.set(false));
    shown
}

fn shows_tooltip_on_tall_screen(ctx: &egui::Context, app: &mut App, control: &Control) -> bool {
    // Clear the previous control's tooltip first: it can sit over this control.
    for _ in 0..2 {
        frame(
            ctx,
            app,
            vec![egui::Event::PointerGone],
            egui::Modifiers::NONE,
        );
    }
    let rect = controls(ctx, app)
        .into_iter()
        .find(|c| c.widget == control.widget)
        .map_or(control.rect, |c| c.rect);
    if rect.width() < 1.0 || rect.height() < 1.0 {
        return false;
    }
    for _ in 0..2 {
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(rect.center())],
            egui::Modifiers::NONE,
        );
    }
    ctx.viewport(|v| {
        v.this_pass
            .tooltips
            .widget_tooltips
            .contains_key(&control.widget)
    })
}

/// Paint each page, record its controls and check each for a tooltip.
fn walk(path: &Path) {
    let ctx = egui::Context::default();
    ctx.all_styles_mut(|style| {
        style.interaction.tooltip_delay = 0.0;
        style.interaction.tooltip_grace_time = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
    });
    let pages: Vec<(&str, Option<&str>, Option<Tab>)> = vec![
        ("files", None, Some(Tab::Files)),
        ("schematic", None, Some(Tab::Schematic)),
        ("pcb", None, Some(Tab::Pcb)),
        ("3d", None, Some(Tab::Board3d)),
        ("rules", None, Some(Tab::Rules)),
        ("checks", None, Some(Tab::Checks)),
        ("settings", None, Some(Tab::Settings)),
        ("libraries", None, Some(Tab::Libraries)),
        ("symbol-editor", Some("symbol-editor"), None),
        ("footprint-editor", Some("footprint-editor"), None),
        ("project-library", Some("project-library"), None),
    ];
    let mut painted: BTreeMap<String, &str> = BTreeMap::new();
    let mut no_tip: BTreeMap<String, String> = BTreeMap::new();
    let mut unnamed: BTreeMap<String, usize> = BTreeMap::new();
    for (name, arg, tab) in pages {
        let mut app = sample_app(arg);
        if let Some(tab) = tab {
            app.tab = tab;
        }
        TALL_SCREEN.with(|tall| tall.set(true));
        settle(&ctx, &mut app);
        let listed = controls(&ctx, &app);
        TALL_SCREEN.with(|tall| tall.set(false));
        let mut checked = BTreeSet::new();
        for control in listed {
            if !control.enabled {
                continue;
            }
            let Some(id) = control.id.clone() else {
                let key = format!(
                    "{} on {name} at {:.0},{:.0}",
                    control.kind, control.rect.min.x, control.rect.min.y
                );
                *unnamed.entry(key).or_default() += 1;
                continue;
            };
            painted.entry(id.clone()).or_insert(name);
            if !checked.insert(id.clone()) || no_tip.contains_key(&id) {
                continue;
            }
            if !shows_tooltip(&ctx, &mut app, &control) {
                no_tip.insert(id, name.to_owned());
            }
        }
    }
    let mut lines: Vec<String> = painted
        .iter()
        .map(|(id, page)| format!("P\t{id}\t{page}"))
        .collect();
    lines.extend(no_tip.iter().map(|(id, page)| format!("T\t{id}\t{page}")));
    lines.extend(unnamed.keys().map(|key| format!("N\t{key}")));
    append(path, &lines);
}

/// Part of `cargo xtask e2e`: does nothing unless the walk log is requested.
#[test]
fn e2e_audit_walk() -> Result<(), String> {
    if let Some(path) = std::env::var_os("PCBCRAFT_E2E_WALK_LOG") {
        walk(Path::new(&path));
    }
    Ok(())
}

#[test]
fn story_run_records_touched_ids() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("pcbcraft-touch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let log = dir.join("touch.log");
    let _ = std::fs::remove_file(&log);
    let ctx = egui::Context::default();
    let mut app = sample_app(None);
    settle(&ctx, &mut app);
    let target = controls(&ctx, &app)
        .into_iter()
        .find(|c| c.enabled && c.id.is_some())
        .ok_or("the app painted no named control")?;
    let pos = target.rect.center();
    let press = egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    };
    record_touches_to(&log, &ctx, &app, &[egui::Event::PointerMoved(pos), press]);
    let text = std::fs::read_to_string(&log).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    let want = target.id.unwrap_or_default();
    assert!(
        text.lines().any(|l| !l.is_empty()),
        "pressing {want} should log a control id"
    );
    Ok(())
}
