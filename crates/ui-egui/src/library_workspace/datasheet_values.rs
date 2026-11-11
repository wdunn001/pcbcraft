//! Values read from a datasheet, each one a button that jumps to where it was read.
//!
//! Context: issue #703. The extractor stores, for every rating and dimension it reads, the page
//! and the rectangle of the words it came from (`DatasheetSourceRecord`). This module turns the
//! stored bundles of a part into rows for the Datasheet page; pressing a row opens the reader
//! window on that page and flashes the rectangle through `Pane::show_source`.
//!
//! Local rules: nothing is read from disk here. The bundles are the `datasheet-extract-*.json`
//! files the project already carries, decoded once per change and kept in `ValuesCache`. A row
//! whose rectangle cannot be drawn has no known source and its button stays off.
//!
//! # Dependencies
//! - crates/library/src/datasheet_extract/ (`DatasheetExtractBundle`, `DatasheetSourceRecord`)
//! - crates/ui-egui/src/datasheet/pane.rs (`Pane::show_source`)
//! - crates/text/strings/en.txt (`libws.ds.value*`)
//!
//! # Do not
//! - Invent a page or a rectangle for a value that carries none.
use super::facts::PartFacts;
use pcbcraft_doc::EmbeddedFile;
use pcbcraft_library::datasheet_extract::{
    DatasheetExtractBundle, DatasheetSourceRecord, ReviewDecision, ReviewedField,
};
use pcbcraft_text::t;

/// Longest part name an extraction file name may carry.
const PART_NAME_MAX: usize = 64;

/// One extracted value and where it was read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SourcedValue {
    /// The words on the button, for example `VDD: 6.0 V`.
    pub(crate) label: String,
    /// The page and rectangle the value was read from.
    pub(crate) source: DatasheetSourceRecord,
}

/// A titled run of values.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValueGroup {
    /// The heading, for example Absolute maximum ratings.
    pub(crate) title: String,
    /// The values under it.
    pub(crate) rows: Vec<SourcedValue>,
}

/// Decoded groups with the files they came from, so the page decodes only when a file changes.
#[derive(Clone, Debug, Default)]
pub(crate) struct ValuesCache {
    key: Vec<(String, usize)>,
    groups: Vec<ValueGroup>,
}

/// A part name as the extractor accepts it: letters, digits, `_` and `-`, at most 64.
fn file_part(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(PART_NAME_MAX)
        .collect()
}

/// The extraction file names that can hold values for this part.
fn candidate_files(f: &PartFacts) -> Vec<String> {
    let mut names = Vec::new();
    for part in [
        file_part(&f.mpn),
        file_part(&f.name),
        "datasheet".to_owned(),
    ] {
        let name = format!("datasheet-extract-{part}.json");
        if !part.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The row a person should see: nothing for a rejected row, the edit for an edited one.
fn shown<T>(field: &ReviewedField<T>) -> Option<&T> {
    match field.decision {
        ReviewDecision::Rejected => None,
        ReviewDecision::Edited => field.edited.as_ref(),
        ReviewDecision::Pending | ReviewDecision::Accepted => Some(&field.value),
    }
}

fn label(name: &str, value: &str, unit: &str) -> String {
    t!(
        "libws.ds.value_button",
        parameter = name,
        value = value,
        unit = unit
    )
    .trim()
    .to_owned()
}

/// The groups of one bundle, empty ones left out.
fn bundle_groups(bundle: &DatasheetExtractBundle) -> Vec<ValueGroup> {
    let rating =
        |rows: &[ReviewedField<pcbcraft_library::datasheet_extract::ExtractedRatingRow>]| {
            rows.iter()
                .filter_map(shown)
                .map(|r| SourcedValue {
                    label: label(&r.parameter, &r.value, &r.unit),
                    source: r.source.clone(),
                })
                .collect::<Vec<_>>()
        };
    let dimensions = bundle
        .package_dimensions
        .iter()
        .filter_map(shown)
        .map(|d| SourcedValue {
            label: label(&d.name, &d.value, &d.unit),
            source: d.source.clone(),
        })
        .collect::<Vec<_>>();
    [
        (
            t!("libws.ds.values.rated"),
            rating(&bundle.absolute_maximum_ratings),
        ),
        (
            t!("libws.ds.values.operating"),
            rating(&bundle.recommended_operating_conditions),
        ),
        (t!("libws.ds.values.package"), dimensions),
    ]
    .into_iter()
    .filter(|(_, rows)| !rows.is_empty())
    .map(|(title, rows)| ValueGroup { title, rows })
    .collect()
}

/// The groups for a part from the bundles the project carries; a bundle that does not decode
/// adds nothing.
fn groups_for(files: &[EmbeddedFile], names: &[String]) -> Vec<ValueGroup> {
    let mut out: Vec<ValueGroup> = Vec::new();
    for file in files.iter().filter(|f| names.contains(&f.name)) {
        let Ok(bundle) = serde_json::from_str::<DatasheetExtractBundle>(&file.data) else {
            continue;
        };
        for group in bundle_groups(&bundle) {
            match out.iter_mut().find(|g| g.title == group.title) {
                Some(have) => have.rows.extend(group.rows),
                None => out.push(group),
            }
        }
    }
    out
}

impl ValuesCache {
    /// The groups for this part, decoded again only when a candidate file changed.
    pub(crate) fn groups(&mut self, files: &[EmbeddedFile], f: &PartFacts) -> &[ValueGroup] {
        let names = candidate_files(f);
        let key: Vec<(String, usize)> = files
            .iter()
            .filter(|file| names.contains(&file.name))
            .map(|file| (file.name.clone(), file.data.len()))
            .collect();
        if key != self.key {
            self.groups = groups_for(files, &names);
            self.key = key;
        }
        &self.groups
    }
}

/// The page index and the rectangle to flash, or `None` when the record cannot be drawn.
pub(crate) fn target(source: &DatasheetSourceRecord) -> Option<(usize, [f32; 4])> {
    let r = source.rect;
    let corners = [r.x0, r.y0, r.x1, r.y1];
    let drawable = corners.iter().all(|v| v.is_finite() && *v >= 0.0) && r.x1 > r.x0 && r.y1 > r.y0;
    drawable.then_some((source.page_index as usize, corners))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_library::datasheet_extract::DatasheetRect;

    fn record(rect: DatasheetRect) -> DatasheetSourceRecord {
        DatasheetSourceRecord {
            page_index: 3,
            page_label: 4,
            rect,
            quote: String::new(),
            confidence: None,
        }
    }

    #[test]
    fn a_drawable_rectangle_is_a_target() {
        let r = DatasheetRect {
            x0: 1.0,
            y0: 2.0,
            x1: 30.0,
            y1: 12.0,
        };
        assert_eq!(target(&record(r)), Some((3, [1.0, 2.0, 30.0, 12.0])));
    }

    #[test]
    fn an_empty_or_negative_rectangle_has_no_source() {
        let flat = DatasheetRect {
            x0: 5.0,
            y0: 2.0,
            x1: 5.0,
            y1: 12.0,
        };
        let negative = DatasheetRect {
            x0: -1.0,
            y0: 2.0,
            x1: 5.0,
            y1: 12.0,
        };
        assert_eq!(target(&record(flat)), None);
        assert_eq!(target(&record(negative)), None);
    }

    #[test]
    fn part_names_keep_only_what_the_extractor_accepts() {
        assert_eq!(file_part("LM317 (TO-220)"), "LM317TO-220");
        assert_eq!(file_part("   "), "");
    }
}
