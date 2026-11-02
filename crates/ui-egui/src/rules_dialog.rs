//! Edit > Rules and checks: one window for the check results and how strict each check is.
//!
//! Context: issue #1052. The window is reached from the Edit menu or Ctrl+Shift+R; it is
//! not a main tab. This first slice holds two tabs. Results lists every finding the checks
//! reported, grouped by area, with Show, Hide and Mute on each. Rules lists the checks that
//! take a strictness, each with an on switch and a Built-in, Warning or Error choice.
//! Rule sets and Net classes arrive in later slices.
//!
//! Local rules: strictness is the project's `rule_severity` map, written only by the typed
//! engine command `rules.severity`, so it is saved with the project and applies to the next
//! run. The window reads findings through `App::findings`, never counting by itself. The tab
//! in view is kept when the window closes, so it opens on the last used tab.
//!
//! # Dependencies
//! - crates/ui-egui/src/finding_set.rs (`FindingSet`, `Row`, `Loudness`)
//! - crates/ui-egui/src/bottom_sheet.rs (`show_finding`, `mute_area`)
//! - crates/ui-egui/src/ui_kit/dialog.rs (the modal frame and Escape handling)
//! - crates/engine/src/lib.rs (`rules.severity`, `rules.exclude`)
//!
//! # Do not
//! - Hold strictness here; the project is the only copy.
//! - Add a main tab for this window.
use crate::bottom_sheet::SeverityFilter;
use crate::finding_set::{Loudness, Row};
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::checkbox::checkbox;
use crate::ui_kit::dialog::{self, Dialog, Outcome};
use crate::ui_kit::rows::{self, Finding, Severity};
use crate::ui_kit::segmented::{Segment, segmented};
use crate::ui_kit::select::select;
use crate::ui_kit::tab_bar::TabItem;
use crate::{App, fonts, human_code, theme};
use pcbcraft_text::t;
use serde_json::json;

/// Index of the Results tab.
pub(crate) const TAB_RESULTS: usize = 0;

/// Stems of the tabs in catalog keys, in tab order.
const TABS: [&str; 2] = ["results", "rules"];

/// Areas of the Results list: catalog stem and the finding sources it takes.
const DOMAINS: [(&str, &[&str]); 4] = [
    ("all", &["erc", "drc", "peripheral", "lib"]),
    ("electrical", &["erc", "peripheral"]),
    ("board", &["drc"]),
    ("library", &["lib"]),
];

/// Checks whose strictness can be chosen here.
const RULE_CODES: [&str; 14] = [
    "clearance",
    "copper_short",
    "board_edge",
    "track_width",
    "track_length",
    "unrouted",
    "missing_footprint",
    "min_track",
    "min_space",
    "min_drill",
    "annular_ring",
    "silk_text_height",
    "single_pin_net",
    "unassigned_pin",
];

/// Most findings drawn at once; the rest are counted in a line under the list.
const LIST_CAP: usize = 200;

/// What the window remembers between frames and between openings.
#[derive(Default)]
pub(crate) struct State {
    /// Whether the window is showing.
    pub open: bool,
    /// Tab in view; kept when the window closes.
    pub tab: usize,
    /// Area chosen in Results, an index into `DOMAINS`.
    pub domain: usize,
    /// Filter chosen in Rules: 0 All, 1 Changed, 2 Off.
    pub filter: usize,
    /// The engine's last refusal, shown at the top of the body.
    pub error: Option<String>,
}

impl App {
    /// Opens the window on the tab it was last left on.
    ///
    /// # Rules
    /// - Never resets the tab, so a second opening lands where the person left off.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/lib.rs (the Edit menu and `KeyAction::RulesAndChecks` call it)
    ///
    /// # Do not
    /// - Call it from a command path; programmatic paths never open windows.
    pub(crate) fn open_rules_dialog(&mut self) {
        self.rules_dialog.open = true;
        self.rules_dialog.error = None;
        self.rules_dialog.tab = self.rules_dialog.tab.min(TABS.len() - 1);
    }

    /// Draws the window for one frame when it is open.
    ///
    /// # Rules
    /// - Escape or Done closes it; nothing is lost because every edit already ran.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/dialog.rs
    ///
    /// # Do not
    /// - Draw anything when closed.
    pub(crate) fn rules_dialog_window(&mut self, ctx: &egui::Context) {
        if !self.rules_dialog.open {
            return;
        }
        let labels: Vec<(String, String)> = TABS
            .iter()
            .map(|stem| {
                (
                    pcbcraft_text::lookup(&format!("rules.dialog.tab.{stem}"), &[]),
                    pcbcraft_text::lookup(&format!("rules.dialog.tab.{stem}.tip"), &[]),
                )
            })
            .collect();
        let tabs: Vec<TabItem<'_>> = labels
            .iter()
            .map(|(label, tip)| TabItem::new(label, tip))
            .collect();
        let title = t!("rules.dialog.title");
        let note = t!("rules.dialog.note");
        let done = (t!("rules.dialog.done"), t!("rules.dialog.done.tip"));
        let width = (ctx.content_rect().width() - 80.0).clamp(640.0, 980.0);
        let spec = Dialog {
            id: "rules-and-checks",
            title: &title,
            width,
            tabs: &tabs,
            note: Some(&note),
            cancel: None,
            primary: (&done.0, &done.1),
        };
        let mut tab = self.rules_dialog.tab;
        let shown = tab;
        let outcome = dialog::show(ctx, &spec, &mut tab, |ui| {
            if let Some(error) = self.rules_dialog.error.clone() {
                let red = theme::tokens(ui.ctx()).red;
                ui.label(fonts::text(fonts::Role::Body, error).color(red));
            }
            match shown {
                TAB_RESULTS => self.rules_results(ui),
                _ => self.rules_rules(ui),
            }
        });
        self.rules_dialog.tab = tab;
        if outcome != Outcome::Open {
            self.rules_dialog.open = false;
        }
    }

    /// Runs one engine command and keeps its refusal for the body of the window.
    fn rules_run(&mut self, command: &str, params: serde_json::Value) {
        let before = self.revision;
        self.run(command, params);
        self.rules_dialog.error = (self.revision == before).then(|| self.message.clone());
    }

    /// The Results tab: summary cards, the areas, and the findings of the area in view.
    fn rules_results(&mut self, ui: &mut egui::Ui) {
        let set = self.findings();
        let rows: Vec<Row> = set.visible(SeverityFilter::All).cloned().collect();
        ui.horizontal(|ui| {
            let (errors, warnings, notes) = tally(rows.iter());
            for (key, n) in [
                ("rules.dialog.card.errors", errors),
                ("rules.dialog.card.warnings", warnings),
                ("rules.dialog.card.notes", notes),
            ] {
                let label = pcbcraft_text::lookup(key, &[]);
                let tip = t!("rules.dialog.card.tip");
                let text = theme::tokens(ui.ctx()).text;
                ui.label(fonts::strong(format!("{label} {n}")).color(text))
                    .on_hover_text(tip);
                ui.add_space(theme::SPACE[2]);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if button::secondary_button(
                    ui,
                    None,
                    &t!("rules.dialog.run"),
                    &t!("rules.dialog.run.tip"),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    self.rules_run_checks();
                }
            });
        });
        ui.add_space(theme::SPACE[1]);
        let names: Vec<String> = DOMAINS
            .iter()
            .map(|(stem, sources)| {
                let name = pcbcraft_text::lookup(&format!("rules.dialog.domain.{stem}"), &[]);
                let count = rows.iter().filter(|r| sources.contains(&r.key)).count();
                t!("rules.dialog.domain.row", name = name, count = count)
            })
            .collect();
        let tips: Vec<String> = DOMAINS
            .iter()
            .map(|(stem, _)| {
                let name = pcbcraft_text::lookup(&format!("rules.dialog.domain.{stem}"), &[]);
                t!("rules.dialog.domain.tip", name = name.to_lowercase())
            })
            .collect();
        let items: Vec<Segment<'_>> = names
            .iter()
            .zip(&tips)
            .map(|(label, tip)| Segment::text(label, tip))
            .collect();
        let mut at = self.rules_dialog.domain.min(DOMAINS.len() - 1);
        segmented(ui, "rules-domain", &mut at, &items);
        self.rules_dialog.domain = at;
        ui.add_space(theme::SPACE[1]);
        let sources = DOMAINS[at].1;
        let shown: Vec<&Row> = rows.iter().filter(|r| sources.contains(&r.key)).collect();
        if shown.is_empty() {
            let muted = theme::tokens(ui.ctx()).muted;
            ui.label(fonts::text(fonts::Role::Body, t!("rules.dialog.empty")).color(muted));
            return;
        }
        let mut act: Option<(Act, Row)> = None;
        for row in shown.iter().take(LIST_CAP) {
            let (severity, word) = match row.loudness {
                Loudness::Error => (Severity::Error, t!("rules.dialog.word.error")),
                Loudness::Warning => (Severity::Warning, t!("rules.dialog.word.warning")),
                Loudness::Note => (Severity::Warning, t!("rules.dialog.word.note")),
            };
            let heading = format!("{word}: {}", human_code(&row.code));
            let net = row.objects.first().map(|id| self.project.net_name(*id));
            let chip = net.filter(|n| *n != "unassigned");
            let answer = rows::finding_row(
                ui,
                &Finding {
                    severity,
                    title: &heading,
                    detail: &row.message,
                    chip,
                    selected: false,
                    compact: true,
                },
            );
            if answer.show {
                act = Some((Act::Show, (*row).clone()));
            } else if answer.mute {
                act = Some((Act::Mute, (*row).clone()));
            } else if answer.hide {
                act = Some((Act::Hide, (*row).clone()));
            }
        }
        if shown.len() > LIST_CAP {
            let muted = theme::tokens(ui.ctx()).muted;
            let more = t!("rules.dialog.more", count = shown.len() - LIST_CAP);
            ui.label(fonts::caption(more).color(muted));
        }
        match act {
            Some((Act::Show, row)) => {
                self.show_finding(&row);
                self.rules_dialog.open = false;
            }
            Some((Act::Mute, row)) => self.mute_area(&row),
            Some((Act::Hide, row)) => self.rules_run(
                "rules.exclude",
                json!({"code": row.code, "ids": row.objects}),
            ),
            None => {}
        }
    }

    /// Checks the design again now, as Run checks does.
    fn rules_run_checks(&mut self) {
        let fresh = self.session.lock().ok().map(|s| s.checks());
        if let Some(checks) = fresh {
            self.checks = checks;
            self.message = t!("rules.dialog.ran");
        }
    }

    /// The Rules tab: the filter, then one row per check.
    fn rules_rules(&mut self, ui: &mut egui::Ui) {
        let muted = theme::tokens(ui.ctx()).muted;
        ui.label(fonts::caption(t!("rules.dialog.rules.intro")).color(muted));
        ui.add_space(theme::SPACE[1]);
        let filters = [
            (
                t!("rules.dialog.filter.all"),
                t!("rules.dialog.filter.all.tip"),
            ),
            (
                t!("rules.dialog.filter.changed"),
                t!("rules.dialog.filter.changed.tip"),
            ),
            (
                t!("rules.dialog.filter.off"),
                t!("rules.dialog.filter.off.tip"),
            ),
        ];
        let items: Vec<Segment<'_>> = filters
            .iter()
            .map(|(label, tip)| Segment::text(label, tip))
            .collect();
        let mut filter = self.rules_dialog.filter.min(2);
        segmented(ui, "rules-filter", &mut filter, &items);
        self.rules_dialog.filter = filter;
        ui.add_space(theme::SPACE[1]);
        let mut any = false;
        for code in RULE_CODES {
            let level = self.project.rule_severity.get(code).cloned();
            let off = level.as_deref() == Some("off");
            let keep = match filter {
                1 => level.is_some(),
                2 => off,
                _ => true,
            };
            if !keep {
                continue;
            }
            any = true;
            self.rules_row(ui, code, level.as_deref());
        }
        if !any {
            ui.label(fonts::text(fonts::Role::Body, t!("rules.dialog.rules.none")).color(muted));
        }
    }

    /// One check: its on switch, its name, its state word and its strictness choice.
    fn rules_row(&mut self, ui: &mut egui::Ui, code: &str, level: Option<&str>) {
        let name = human_code(code);
        let off = level == Some("off");
        let mut on = !off;
        let mut write: Option<serde_json::Value> = None;
        ui.horizontal(|ui| {
            ui.set_min_height(32.0);
            if checkbox(
                ui,
                &mut on,
                &t!("rules.dialog.rules.on", name = name),
                &t!("rules.dialog.rules.on.tip", name = name.to_lowercase()),
            )
            .changed()
            {
                write = Some(if on {
                    serde_json::Value::Null
                } else {
                    json!("off")
                });
            }
            let text = theme::tokens(ui.ctx()).text;
            ui.label(fonts::text(fonts::Role::Body, &name).color(text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let labels = [
                    t!("rules.dialog.level.default"),
                    t!("rules.dialog.level.warning"),
                    t!("rules.dialog.level.error"),
                ];
                let options: Vec<&str> = labels.iter().map(String::as_str).collect();
                let mut at = match level {
                    Some("warning") => 1,
                    Some("error") => 2,
                    _ => 0,
                };
                let before = at;
                ui.add_enabled_ui(!off, |ui| {
                    select(
                        ui,
                        ("rules-level", code),
                        &mut at,
                        &options,
                        &name,
                        &t!("rules.dialog.level.tip", name = name.to_lowercase()),
                        140.0,
                    );
                });
                if at != before {
                    write = Some(match at {
                        1 => json!("warning"),
                        2 => json!("error"),
                        _ => serde_json::Value::Null,
                    });
                }
                let state = match level {
                    Some("off") => t!("rules.dialog.rules.state.off"),
                    Some(_) => t!("rules.dialog.rules.state.changed"),
                    None => t!("rules.dialog.rules.state.default"),
                };
                let muted = theme::tokens(ui.ctx()).muted;
                ui.label(fonts::caption(state).color(muted))
                    .on_hover_text(t!("rules.dialog.rules.state.tip"));
            });
        });
        if let Some(severity) = write {
            // Clearing an override that does not exist is refused by the engine, and
            // there is nothing to clear, so skip the call.
            if severity.is_null() && level.is_none() {
                return;
            }
            self.rules_run(
                "rules.severity",
                json!({"code": code, "severity": severity}),
            );
        }
    }
}

/// What a finding row asked for this frame.
enum Act {
    Show,
    Hide,
    Mute,
}

/// Errors, warnings and notes among `rows`.
fn tally<'a>(rows: impl Iterator<Item = &'a Row>) -> (usize, usize, usize) {
    let mut n = (0, 0, 0);
    for r in rows {
        match r.loudness {
            Loudness::Error => n.0 += 1,
            Loudness::Warning => n.1 += 1,
            Loudness::Note => n.2 += 1,
        }
    }
    n
}
