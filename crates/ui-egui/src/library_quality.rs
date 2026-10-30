//! Library footprint quality: a badge beside each project footprint and a
//! list of findings in the Findings sheet (rows are built in `finding_set.rs`). Findings come from the
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
    pub(super) findings: Vec<QualityFinding>,
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
    /// the last look. A footprint that fails validation stays unlisted.
    pub(super) fn refresh_footprint_quality(&mut self) {
        if self.library.quality.revision == Some(self.revision) {
            return;
        }
        // Read straight from the library crate: the check never touches the
        // session, so asking for it every frame cannot disturb history or
        // the revision other commands are checked against.
        let mut findings = Vec::new();
        for def in &self.project.library.footprints {
            if def.validate().is_err() {
                continue;
            }
            for f in pcbcraft_library::check_footprint(def) {
                findings.push(QualityFinding {
                    footprint: def.name.clone(),
                    code: f.code,
                    error: f.severity == pcbcraft_library::FindingSeverity::Error,
                    message: f.message,
                    hint: f.hint,
                });
            }
        }
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
}
