//! Library footprint quality: a badge beside each project footprint and a
//! grouped list of findings on the Checks tab. Findings come from the
//! engine command `library.footprint.check`; nothing is judged here.
use super::*;

/// One finding about one library footprint, as the engine reported it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct QualityFinding {
    pub(super) footprint: String,
    pub(super) code: String,
    pub(super) error: bool,
    pub(super) message: String,
    pub(super) hint: String,
}

/// Cached results for the whole project library, valid for one revision.
#[derive(Default)]
pub(super) struct Quality {
    revision: Option<u64>,
    findings: Vec<QualityFinding>,
}

impl Quality {
    #[cfg(test)]
    pub(super) fn findings_for(&self, name: &str) -> Vec<&QualityFinding> {
        self.findings
            .iter()
            .filter(|f| f.footprint == name)
            .collect()
    }
}

impl App {
    /// Re-check every project footprint when the design has changed since
    /// the last look. A failed check leaves that footprint unlisted.
    pub(super) fn refresh_footprint_quality(&mut self) {
        if self.library.quality.revision == Some(self.revision) {
            return;
        }
        let names: Vec<String> = self
            .project
            .library
            .footprints
            .iter()
            .map(|d| d.name.clone())
            .collect();
        let message = self.message.clone();
        let mut findings = Vec::new();
        for name in names {
            let result = self.run_and_keep("library.footprint.check", json!({"name": name}));
            let rows = result
                .get("findings")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for f in rows {
                let text = |key: &str| {
                    f.get(key)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                findings.push(QualityFinding {
                    footprint: name.clone(),
                    code: text("code"),
                    error: f.get("severity").and_then(Value::as_str) == Some("error"),
                    message: text("message"),
                    hint: text("hint"),
                });
            }
        }
        self.message = message;
        self.library.quality = Quality {
            revision: Some(self.revision),
            findings,
        };
    }

    /// The small verdict shown beside a footprint in the project library.
    pub(super) fn quality_badge(&self, ui: &mut egui::Ui, name: &str) {
        let (errors, warnings) = self
            .library
            .quality
            .findings
            .iter()
            .filter(|f| f.footprint == name)
            .fold(
                (0, 0),
                |(e, w), f| {
                    if f.error { (e + 1, w) } else { (e, w + 1) }
                },
            );
        let (text, colour, tip) = if errors > 0 {
            (
                t!("library.quality_badge.errors_to_fix", errors = errors),
                Color32::LIGHT_RED,
                t!("library.quality_badge.this_footprint_has_mistakes_that"),
            )
        } else if warnings > 0 {
            (
                t!(
                    "library.quality_badge.warnings_to_review",
                    warnings = warnings
                ),
                Color32::from_rgb(230, 180, 60),
                t!("library.quality_badge.this_footprint_is_usable_but"),
            )
        } else {
            (
                t!("library.quality_badge.clean"),
                self.accent(),
                t!("library.quality_badge.the_library_checks_found_nothing"),
            )
        };
        ui.label(egui::RichText::new(text).color(colour).small())
            .on_hover_text(tip);
    }

    /// Library findings on the Checks tab, grouped by rule like the board
    /// and wiring checks, each place with a plain way to fix it.
    pub(super) fn library_checks_section(&mut self, ui: &mut egui::Ui) {
        self.refresh_footprint_quality();
        ui.label(
            egui::RichText::new(t!("library.library_checks_section.library"))
                .size(App::SECTION)
                .strong(),
        );
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(t!(
                "library.library_checks_section.whether_the_footprints_in_this"
            ))
            .size(App::CAPTION)
            .color(self.muted()),
        );
        ui.add_space(8.0);
        let findings = self.library.quality.findings.clone();
        if findings.is_empty() {
            ui.label(
                egui::RichText::new(t!("checks.checks_view.no_problems"))
                    .size(App::BODY)
                    .color(self.accent()),
            );
        }
        let mut groups: Vec<(String, Vec<QualityFinding>)> = Vec::new();
        for f in findings {
            if let Some(group) = groups.iter_mut().find(|(c, _)| *c == f.code) {
                group.1.push(f);
            } else {
                groups.push((f.code.clone(), vec![f]));
            }
        }
        groups.sort_by(|a, b| {
            let loud = |g: &(String, Vec<QualityFinding>)| !g.1.iter().any(|f| f.error);
            loud(a).cmp(&loud(b)).then(a.0.cmp(&b.0))
        });
        for (code, rows) in groups {
            let must = rows.iter().any(|f| f.error);
            let title = format!(
                "{} · {} {}",
                human_code(&code),
                rows.len(),
                if rows.len() == 1 { "place" } else { "places" }
            );
            ui.add_space(6.0);
            let title = if must {
                egui::RichText::new(title).strong()
            } else {
                egui::RichText::new(title)
            };
            ui.label(title).on_hover_text(if must {
                t!("library.library_checks_section.a_mistake_in_a_library")
            } else {
                t!("library.library_checks_section.worth_a_look_but_it")
            });
            for f in rows {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(14.0);
                    if ui
                        .button(t!(
                            "library.library_checks_section.footprint_message",
                            footprint = f.footprint,
                            message = f.message
                        ))
                        .on_hover_text(t!("library.library_checks_section.footprint_message.tip"))
                        .clicked()
                        && let Some(def) = self
                            .project
                            .library
                            .footprints
                            .iter()
                            .find(|d| d.name == f.footprint)
                            .cloned()
                    {
                        self.library.editor.draft = Some(authoring::Draft::Footprint(def));
                        self.library.editor.mode = 2;
                        self.library.editor.notice = None;
                        self.change_tab(Tab::Libraries);
                    }
                    ui.label(
                        egui::RichText::new(t!(
                            "library.library_checks_section.fix_hint",
                            hint = f.hint
                        ))
                        .small()
                        .color(self.muted()),
                    );
                });
            }
        }
        ui.add_space(20.0);
    }
}
