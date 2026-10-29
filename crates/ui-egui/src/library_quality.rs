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
                format!("{errors} to fix"),
                Color32::LIGHT_RED,
                "This footprint has mistakes that can make a bad board. Open the Checks tab for what to change.",
            )
        } else if warnings > 0 {
            (
                format!("{warnings} to review"),
                Color32::from_rgb(230, 180, 60),
                "This footprint is usable but breaks a common good habit. Open the Checks tab for the details.",
            )
        } else {
            (
                "Clean".to_owned(),
                self.accent(),
                "The library checks found nothing wrong with this footprint.",
            )
        };
        ui.label(egui::RichText::new(text).color(colour).small())
            .on_hover_text(tip);
    }

    /// Library findings on the Checks tab, grouped by rule like the board
    /// and wiring checks, each place with a plain way to fix it.
    pub(super) fn library_checks_section(&mut self, ui: &mut egui::Ui) {
        self.refresh_footprint_quality();
        ui.label(egui::RichText::new("Library").size(18.0).strong());
        ui.label(
            egui::RichText::new(
                "Whether the footprints in this project's library follow good drawing habits: pad numbers, silkscreen, courtyard and the like.",
            )
            .small()
            .color(self.muted()),
        );
        let findings = self.library.quality.findings.clone();
        if findings.is_empty() {
            ui.label(
                egui::RichText::new("No findings in the implemented checks").color(self.accent()),
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
                "A mistake in a library footprint. Fix it before parts using this footprint go on a board."
            } else {
                "Worth a look, but it does not stop a board being built."
            });
            for f in rows {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(14.0);
                    if ui
                        .button(format!("{}: {}", f.footprint, f.message))
                        .on_hover_text("Open this footprint in the library editor.")
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
                        egui::RichText::new(format!("Fix: {}", f.hint))
                            .small()
                            .color(self.muted()),
                    );
                });
            }
        }
        ui.add_space(20.0);
    }
}
