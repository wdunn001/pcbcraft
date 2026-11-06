//! The Checks page of the Analyze tab: the findings of the design's own checks.
//!
//! Context: issue #1421. The rows come from `App::findings`, the one list every count in the window
//! uses, so the numbers here are the numbers in the status bar and the Findings tab.
//!
//! Local rules: Show selects the finding's object on the sheet or board and Mute asks the check to
//! leave the area alone; both are the commands the Findings tab already runs. Mute is disabled
//! for a finding that names no object.
//!
//! # Dependencies
//! - crates/ui-egui/src/finding_set.rs (`FindingSet`, `Row`, `Loudness`)
//! - crates/ui-egui/src/bottom_sheet.rs (`show_finding`, `mute_area`, `SeverityFilter`)
//! - crates/ui-egui/src/ui_kit/card.rs (`card_flush`, `card`)
//! - crates/ui-egui/src/ui_kit/table.rs (`head`, `row`)
//! - crates/ui-egui/src/ui_kit/segmented.rs (`segmented`)
//!
//! # Do not
//! - Count findings here; read the counts from the `FindingSet`.
use super::*;
use crate::ui_kit::{
    card::{card, card_flush},
    form_row::{FormWidth, form_row},
    segmented::{Segment, segmented},
    table::{self, Column},
};
use finding_set::{Loudness, Row};

/// The finding sources an entry reads.
fn sources(entry: Entry) -> &'static [&'static str] {
    match entry {
        Entry::DesignRules => &["drc", "lib"],
        _ => &["erc", "peripheral"],
    }
}

/// Where a finding lives, in words.
fn where_text(row: &Row) -> String {
    match row.key {
        "erc" | "peripheral" => t!("analyze.checks.where_schematic"),
        "lib" => t!("analyze.checks.where_library"),
        _ => t!("analyze.checks.where_layout"),
    }
}

/// The word for a loudness, so severity is never colour alone.
fn level_text(loudness: Loudness) -> String {
    match loudness {
        Loudness::Error => t!("analyze.checks.level_error"),
        Loudness::Warning => t!("analyze.checks.level_warning"),
        Loudness::Note => t!("analyze.checks.level_note"),
    }
}

impl App {
    /// The findings card and the card that says what the shown finding means.
    pub(super) fn analyze_checks_center(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        let entry = self.analyze.entry;
        let set = self.findings();
        let keys = sources(entry);
        let mine: Vec<Row> = set
            .visible(bottom_sheet::SeverityFilter::All)
            .filter(|r| keys.contains(&r.key))
            .cloned()
            .collect();
        let count = |l: Loudness| mine.iter().filter(|r| r.loudness == l).count();
        let (errors, warnings, notes) = (
            count(Loudness::Error),
            count(Loudness::Warning),
            count(Loudness::Note),
        );
        let title = entry.words().0;
        let caption = t!("analyze.checks.count", count = mine.len());
        card_flush(
            ui,
            &title,
            Some(&caption),
            |_| {},
            |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    let labels = [
                        t!("analyze.checks.filter_all"),
                        t!("analyze.checks.filter_errors"),
                        t!("analyze.checks.filter_warnings"),
                        t!("analyze.checks.filter_notes"),
                    ];
                    let tips = [
                        t!("analyze.checks.filter_all.tip"),
                        t!("analyze.checks.filter_errors.tip"),
                        t!("analyze.checks.filter_warnings.tip"),
                        t!("analyze.checks.filter_notes.tip"),
                    ];
                    let counts = [mine.len(), errors, warnings, notes];
                    let segments: Vec<Segment<'_>> = labels
                        .iter()
                        .zip(&tips)
                        .zip(counts)
                        .map(|((label, tip), n)| Segment {
                            count: Some(n),
                            ..Segment::text(label, tip)
                        })
                        .collect();
                    segmented(
                        ui,
                        "analyze-severity",
                        &mut self.analyze.severity,
                        &segments,
                    );
                });
                ui.add_space(8.0);
                let wanted = match self.analyze.severity {
                    1 => Some(Loudness::Error),
                    2 => Some(Loudness::Warning),
                    3 => Some(Loudness::Note),
                    _ => None,
                };
                let rows: Vec<&Row> = mine
                    .iter()
                    .filter(|r| wanted.is_none_or(|w| r.loudness == w))
                    .collect();
                if rows.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        let message = if mine.is_empty() {
                            t!("analyze.checks.none")
                        } else {
                            t!("analyze.checks.none_here")
                        };
                        ui.label(fonts::text(fonts::Role::Body, message).color(tokens.muted));
                    });
                    ui.add_space(12.0);
                    return;
                }
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(12, 0))
                    .show(ui, |ui| {
                        let free = (ui.available_width() - 90.0 - 150.0 - 140.0).max(120.0);
                        let columns = [
                            Column {
                                title: &t!("analyze.checks.col_level"),
                                width: 90.0,
                            },
                            Column {
                                title: &t!("analyze.checks.col_finding"),
                                width: free,
                            },
                            Column {
                                title: &t!("analyze.checks.col_where"),
                                width: 150.0,
                            },
                            Column {
                                title: "",
                                width: 0.0,
                            },
                        ];
                        table::head(ui, &columns);
                        let mut act: Option<(Row, bool)> = None;
                        for row in rows {
                            table::row(ui, &columns, |cells| {
                                cells.cell(|ui| {
                                    ui.label(fonts::strong(level_text(row.loudness)).color(
                                        match row.loudness {
                                            Loudness::Error => tokens.red,
                                            Loudness::Warning => tokens.gold,
                                            Loudness::Note => tokens.muted,
                                        },
                                    ));
                                });
                                cells.cell(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            fonts::text(fonts::Role::Body, &row.message)
                                                .color(tokens.text),
                                        )
                                        .truncate(),
                                    );
                                });
                                cells.cell(|ui| {
                                    ui.label(
                                        fonts::text(fonts::Role::Body, where_text(row))
                                            .color(tokens.muted),
                                    );
                                });
                                cells.cell(|ui| {
                                    if ghost_button(
                                        ui,
                                        None,
                                        &t!("analyze.checks.mute"),
                                        &t!("analyze.checks.mute.tip"),
                                        Size::Small,
                                        !row.objects.is_empty(),
                                    )
                                    .clicked()
                                    {
                                        act = Some((row.clone(), true));
                                    }
                                    if secondary_button(
                                        ui,
                                        None,
                                        &t!("analyze.checks.show"),
                                        &t!("analyze.checks.show.tip"),
                                        Size::Small,
                                        true,
                                    )
                                    .clicked()
                                    {
                                        act = Some((row.clone(), false));
                                    }
                                });
                            });
                        }
                        if let Some((row, mute)) = act {
                            if mute {
                                self.mute_area(&row);
                            } else {
                                self.show_finding(&row);
                            }
                            self.analyze.shown = Some(row);
                        }
                    });
            },
        );
        ui.add_space(12.0);
        let means = match &self.analyze.shown {
            Some(row) if !row.hint.is_empty() => row.hint.clone(),
            Some(_) => t!("analyze.checks.no_hint"),
            None => t!("analyze.checks.means_empty"),
        };
        let subtitle = self.analyze.shown.as_ref().map(|r| r.code.clone());
        card(
            ui,
            &t!("analyze.checks.means"),
            subtitle.as_deref(),
            |_| {},
            |ui| {
                ui.label(fonts::text(fonts::Role::Body, means).color(tokens.text));
            },
        );
    }

    /// Properties of the shown finding: its check, its level and where it is.
    pub(super) fn analyze_checks_properties(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        ui.label(fonts::section_caption(t!("analyze.checks.props_check")).color(tokens.muted));
        let Some(row) = self.analyze.shown.clone() else {
            ui.label(
                fonts::text(fonts::Role::Body, t!("analyze.checks.props_empty"))
                    .color(tokens.muted),
            );
            return;
        };
        for (label, value) in [
            (t!("analyze.checks.props_name"), row.code.clone()),
            (t!("analyze.checks.props_level"), level_text(row.loudness)),
            (t!("analyze.checks.props_where"), where_text(&row)),
        ] {
            form_row(ui, FormWidth::Aside, &label, None, None, |ui| {
                ui.label(fonts::text(fonts::Role::Body, &value).color(tokens.text));
            });
        }
        ui.label(fonts::section_caption(t!("analyze.checks.props_settings")).color(tokens.muted));
        ui.label(
            fonts::text(fonts::Role::Body, t!("analyze.checks.props_settings_note"))
                .color(tokens.muted),
        );
        ui.label(fonts::section_caption(t!("analyze.checks.props_finding")).color(tokens.muted));
        ui.horizontal(|ui| {
            if primary_button(
                ui,
                None,
                &t!("analyze.checks.show_on_sheet"),
                &t!("analyze.checks.show_on_sheet.tip"),
                Size::Normal,
                !row.objects.is_empty() || row.footprint.is_some(),
            )
            .clicked()
            {
                self.show_finding(&row);
            }
        });
    }
}
