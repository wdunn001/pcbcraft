//! Licence compatibility between a publication choice and bundled parts.
//!
//! # Rules
//! - The table in `data/licence_compat.toml` lists every supported pair.
//! - Pairs missing from the table are `Review`.
//!
//! # Dependencies
//! - crates/publication/data/licence_compat.toml
//!
//! # Do not
//! - Treat a review outcome as permission to publish without a human decision.

use crate::ScanOutcome;
use serde::Deserialize;
use std::collections::BTreeMap;

const TABLE_TOML: &str = include_str!("../data/licence_compat.toml");
const RESTRICTED_TOML: &str = include_str!("../data/restricted_sources.toml");

/// Result of comparing two SPDX-style licence ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LicenceVerdict {
    Compatible,
    Incompatible,
    Review,
}

/// Loaded licence compatibility table.
#[derive(Clone, Debug)]
pub struct LicenceTable {
    pub licences: Vec<String>,
    compat: BTreeMap<String, BTreeMap<String, LicenceVerdict>>,
    restricted_sources: Vec<String>,
}

/// Parse the embedded licence table.
pub fn licence_table() -> LicenceTable {
    #[derive(Deserialize)]
    struct Root {
        licences: Vec<String>,
        compat: BTreeMap<String, BTreeMap<String, String>>,
    }
    #[derive(Deserialize)]
    struct Restricted {
        sources: Vec<String>,
    }
    let root: Root = toml::from_str(TABLE_TOML).unwrap_or(Root {
        licences: Vec::new(),
        compat: BTreeMap::new(),
    });
    let restricted: Restricted = toml::from_str(RESTRICTED_TOML).unwrap_or(Restricted {
        sources: Vec::new(),
    });
    let mut compat = BTreeMap::new();
    for (design, row) in root.compat {
        let mut mapped = BTreeMap::new();
        for (part, verdict) in row {
            mapped.insert(part, parse_verdict(&verdict));
        }
        compat.insert(design, mapped);
    }
    LicenceTable {
        licences: root.licences,
        compat,
        restricted_sources: restricted.sources,
    }
}

fn parse_verdict(raw: &str) -> LicenceVerdict {
    match raw {
        "compatible" => LicenceVerdict::Compatible,
        "incompatible" => LicenceVerdict::Incompatible,
        _ => LicenceVerdict::Review,
    }
}

/// Compare the publication licence with one bundled part licence.
pub fn design_licence(table: &LicenceTable, design: &str, part: &str) -> LicenceVerdict {
    table
        .compat
        .get(design)
        .and_then(|row| row.get(part))
        .copied()
        .unwrap_or(LicenceVerdict::Review)
}

/// Check one bundled part and append scan outcomes.
pub fn bundled_part(
    table: &LicenceTable,
    design: &str,
    part_id: &str,
    part_licence: &str,
    source: &str,
) -> Vec<ScanOutcome> {
    let mut out = Vec::new();
    if is_restricted_source(table, source) {
        out.push(ScanOutcome::HardFail {
            rule: "restricted_source",
            path: part_id.into(),
            line: None,
            column: None,
            detail: format!("source `{source}` is excluded from public offering"),
            action: "remove_and_retry",
        });
    }
    match design_licence(table, design, part_licence) {
        LicenceVerdict::Compatible => {}
        LicenceVerdict::Incompatible => out.push(ScanOutcome::HardFail {
            rule: "licence_incompatible",
            path: part_id.into(),
            line: None,
            column: None,
            detail: format!(
                "part licence `{part_licence}` contradicts publication licence `{design}`"
            ),
            action: "remove_and_retry",
        }),
        LicenceVerdict::Review => out.push(ScanOutcome::HumanQueue {
            rule: "licence_review",
            path: part_id.into(),
            detail: format!(
                "licence pair `{design}` + `{part_licence}` needs a human decision"
            ),
        }),
    }
    out
}

/// True when the source name is on the restricted list.
pub fn is_restricted_source(table: &LicenceTable, source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    table
        .restricted_sources
        .iter()
        .any(|s| lower.contains(&s.to_ascii_lowercase()))
}
