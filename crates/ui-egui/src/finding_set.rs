//! The one list of findings every count and row in the window comes from.
//!
//! Context: the status strip, the Findings tab badge, the Output summary and
//! the Findings list in the bottom sheet (`bottom_sheet.rs`) used to count
//! separately and disagreed. Now each of them reads a `FindingSet` built here
//! from `App::checks` (board and wiring checks, already without hidden and
//! quiet-zone findings) and the library footprint checks
//! (`library_quality.rs`).
//!
//! Local rules: severity is the one the check reported, which already
//! includes the project's severity overrides; nothing here re-judges it.
//! Errors sort above warnings, warnings above notes, and findings of equal
//! loudness keep the order the checks gave them.

use super::*;

/// How loud a finding is. The order is the order of the list.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Loudness {
    Error,
    Warning,
    Note,
}

/// One finding flattened for display.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    /// Where it came from: "erc", "drc" or "lib".
    pub(crate) key: &'static str,
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) objects: Vec<Id>,
    pub(crate) loudness: Loudness,
    /// Library findings name the footprint they are about.
    pub(crate) footprint: Option<String>,
    /// Library findings carry a plain way to fix them.
    pub(crate) hint: String,
}

/// Errors, warnings and notes in a set or in what a filter shows of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Counts {
    pub(crate) errors: usize,
    pub(crate) warnings: usize,
    pub(crate) notes: usize,
}

impl Counts {
    /// Every finding counted.
    pub(crate) fn total(self) -> usize {
        self.errors + self.warnings + self.notes
    }

    /// The two numbers the status strip shows: errors, and warnings with
    /// the notes folded in, because the strip names only those two.
    pub(crate) fn strip(self) -> (usize, usize) {
        (self.errors, self.warnings + self.notes)
    }
}

/// All findings of the open design, sorted for the list.
///
/// # Rules
/// Build it with `FindingSet::build` and read counts and rows only from it.
/// Counting findings anywhere else brings the disagreement back.
///
/// # Dependencies
/// `crates/ui-egui/src/bottom_sheet.rs`, `crates/ui-egui/src/library_quality.rs`.
///
/// # Do not
/// Look at a finding's own `severity` outside this file.
#[derive(Clone, Debug, Default)]
pub(crate) struct FindingSet {
    rows: Vec<Row>,
}

impl FindingSet {
    /// Flattens the check results and the library findings into one sorted
    /// list.
    pub(crate) fn build(checks: &Value, library: &[library_quality::QualityFinding]) -> Self {
        let mut rows = Vec::new();
        for key in ["erc", "drc"] {
            let Some(list) = checks.get(key).and_then(Value::as_array) else {
                continue;
            };
            for f in list {
                let loudness = match f.get("severity").and_then(Value::as_str) {
                    Some("error") => Loudness::Error,
                    Some("warning") => Loudness::Warning,
                    _ => Loudness::Note,
                };
                rows.push(Row {
                    key,
                    code: f
                        .get("code")
                        .and_then(Value::as_str)
                        .unwrap_or("finding")
                        .to_owned(),
                    message: f
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    objects: f
                        .get("objects")
                        .and_then(Value::as_array)
                        .map(|v| v.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default(),
                    loudness,
                    footprint: None,
                    hint: String::new(),
                });
            }
        }
        for f in library {
            rows.push(Row {
                key: "lib",
                code: f.code.clone(),
                message: format!("{}: {}", f.footprint, f.message),
                objects: Vec::new(),
                loudness: if f.error {
                    Loudness::Error
                } else {
                    Loudness::Warning
                },
                footprint: Some(f.footprint.clone()),
                hint: f.hint.clone(),
            });
        }
        rows.sort_by_key(|r| r.loudness);
        Self { rows }
    }

    /// Counts of the whole set.
    pub(crate) fn counts(&self) -> Counts {
        Self::count(self.rows.iter())
    }

    /// Counts of what the filter lets through.
    #[cfg(test)]
    pub(crate) fn visible_counts(&self, filter: bottom_sheet::SeverityFilter) -> Counts {
        Self::count(self.visible(filter))
    }

    /// The rows the filter lets through, errors first.
    pub(crate) fn visible(
        &self,
        filter: bottom_sheet::SeverityFilter,
    ) -> impl Iterator<Item = &Row> {
        use bottom_sheet::SeverityFilter as F;
        self.rows.iter().filter(move |r| match filter {
            F::All => true,
            F::Errors => r.loudness == Loudness::Error,
            F::Warnings => r.loudness == Loudness::Warning,
            F::Notes => r.loudness == Loudness::Note,
        })
    }

    fn count<'a>(rows: impl Iterator<Item = &'a Row>) -> Counts {
        let mut c = Counts::default();
        for r in rows {
            match r.loudness {
                Loudness::Error => c.errors += 1,
                Loudness::Warning => c.warnings += 1,
                Loudness::Note => c.notes += 1,
            }
        }
        c
    }
}

impl App {
    /// The findings of the open design, library checks included.
    ///
    /// # Rules
    /// The one place counts and rows come from; call it, do not count.
    pub(crate) fn findings(&mut self) -> FindingSet {
        self.refresh_footprint_quality();
        FindingSet::build(&self.checks, &self.library.quality.findings)
    }
}
