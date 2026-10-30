//! One sorted list of findings and matching counts for the status strip, badge,
//! Output summary and Findings list.
//!
//! Context: board and wiring checks arrive as `erc` and `drc` arrays on the
//! session checks value; library footprint checks come from
//! `pcbcraft_library::check_footprint`. Every consumer should read counts and
//! rows only through [`FindingSet`].
//!
//! Local rules: loudness is the severity the check reported (project overrides
//! are already folded in when checks run). Nothing here re-judges a code with
//! `Project::rule_severity`. Errors sort above warnings, warnings above notes;
//! equal loudness keeps check order.

use pcbcraft_library::FootprintFinding;
use pcbcraft_library::FindingSeverity;
use serde_json::Value;

/// How loud a finding is. Lower sorts earlier in the list.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Loudness {
    Error,
    Warning,
    Note,
}

/// Errors, warnings and notes in a set or in what a filter shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Counts {
    pub errors: usize,
    pub warnings: usize,
    pub notes: usize,
}

impl Counts {
    /// Every finding counted.
    pub fn total(self) -> usize {
        self.errors + self.warnings + self.notes
    }

    /// The two numbers the status strip shows: errors, and warnings with notes
    /// folded in because the strip names only those two.
    pub fn strip(self) -> (usize, usize) {
        (self.errors, self.warnings + self.notes)
    }
}

/// Which findings a list or count includes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SeverityFilter {
    #[default]
    All,
    Errors,
    Warnings,
    Notes,
}

/// One finding flattened for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Where it came from: `erc`, `drc` or `lib`.
    pub bucket: &'static str,
    pub code: String,
    pub message: String,
    pub object_ids: Vec<u64>,
    pub loudness: Loudness,
}

/// All findings of a design, sorted for the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindingSet {
    rows: Vec<Row>,
}

impl FindingSet {
    /// Flattens board and wiring checks and optional library findings into one
    /// sorted list.
    pub fn build(checks: &Value, library: &[(&str, &[FootprintFinding])]) -> Self {
        let mut rows = Vec::new();
        for bucket in ["erc", "drc"] {
            let Some(list) = checks.get(bucket).and_then(Value::as_array) else {
                continue;
            };
            for f in list {
                rows.push(Row {
                    bucket,
                    code: string_field(f, "code").unwrap_or_else(|| "finding".into()),
                    message: string_field(f, "message").unwrap_or_default(),
                    object_ids: f
                        .get("objects")
                        .and_then(Value::as_array)
                        .map(|v| v.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default(),
                    loudness: loudness_from_severity(f.get("severity").and_then(Value::as_str)),
                });
            }
        }
        for (footprint, findings) in library {
            for f in *findings {
                let loudness = match f.severity {
                    FindingSeverity::Error => Loudness::Error,
                    FindingSeverity::Warning => Loudness::Warning,
                };
                rows.push(Row {
                    bucket: "lib",
                    code: f.code.clone(),
                    message: format!("{footprint}: {}", f.message),
                    object_ids: Vec::new(),
                    loudness,
                });
            }
        }
        rows.sort_by_key(|r| r.loudness);
        Self { rows }
    }

    /// Counts of the whole set.
    pub fn counts(&self) -> Counts {
        Self::count_rows(self.rows.iter())
    }

    /// Counts of what the filter lets through.
    pub fn visible_counts(&self, filter: SeverityFilter) -> Counts {
        Self::count_rows(self.visible(filter))
    }

    /// Rows the filter lets through, still errors first.
    pub fn visible(&self, filter: SeverityFilter) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(move |r| match filter {
            SeverityFilter::All => true,
            SeverityFilter::Errors => r.loudness == Loudness::Error,
            SeverityFilter::Warnings => r.loudness == Loudness::Warning,
            SeverityFilter::Notes => r.loudness == Loudness::Note,
        })
    }

    /// Every row, errors first.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    fn count_rows<'a>(rows: impl Iterator<Item = &'a Row>) -> Counts {
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

fn string_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn loudness_from_severity(severity: Option<&str>) -> Loudness {
    match severity {
        Some("error") => Loudness::Error,
        Some("warning") => Loudness::Warning,
        _ => Loudness::Note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn errors_sort_above_warnings_in_the_set() {
        let checks = json!({
            "erc": [],
            "drc": [
                {"code": "zone_island", "severity": "warning", "message": "a"},
                {"code": "silk_over_pad", "severity": "error", "message": "b"},
            ]
        });
        let set = FindingSet::build(&checks, &[]);
        assert_eq!(set.rows().len(), 2);
        assert_eq!(set.rows()[0].loudness, Loudness::Error);
        assert_eq!(set.counts(), Counts {
            errors: 1,
            warnings: 1,
            notes: 0
        });
    }
}
