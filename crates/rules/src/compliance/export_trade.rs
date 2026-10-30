//! Export-and-trade bill-of-materials summary for rule set `export_and_trade`.
use pcbcraft_doc::{Component, ExportTradeSettings, Project, TariffExposureRow};
use serde::Serialize;
use std::collections::BTreeMap;

use super::eccn::{control_rank, highest_eccn};
use super::list_snapshot::EntityListSnapshot;
use super::ruleset::{ComplianceRuleSet, bundled_export_and_trade};

/// Component field keys tried for ECCN values, in order.
pub const FIELD_ECCN: &[&str] = &["ECCN", "eccn", "Export Control Classification Number"];
/// Component field keys tried for HTS values, in order.
pub const FIELD_HTS: &[&str] = &["HTS", "hts", "HTS Code", "Harmonized Tariff"];
/// Component field keys tried for manufacturer names, in order.
pub const FIELD_MANUFACTURER: &[&str] = &["Manufacturer", "Mfg", "MFR"];
/// Component field keys tried for country-of-origin codes, in order.
pub const FIELD_ORIGIN: &[&str] = &["Country of Origin", "COO", "Origin", "country_of_origin"];

const PARTY_SCREENING_FIXTURE: &str =
    include_str!("../../data/compliance/lists/party_screening_fixture.toml");

const DISCLAIMER: &str = "Informational summary from part data and user-entered tables only. \
This is not legal advice and does not state licence or authorization requirements.";

/// One bill line whose manufacturer matched the screening list.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ScreeningHit {
    /// Designator of the matched part.
    pub reference: String,
    /// Manufacturer string that matched.
    pub manufacturer: String,
    /// Primary list entry name.
    pub list_entry: String,
    /// Id of the list file used.
    pub snapshot_id: String,
    /// Retrieved date of the list file.
    pub snapshot_retrieved: String,
    /// Citation copied from the list entry.
    pub citation: String,
}

/// Informational duty estimate for one bill line.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct TariffEstimate {
    /// Designator of the part.
    pub reference: String,
    /// Declared HTS code on the part.
    pub hts: String,
    /// Normalized origin country code.
    pub origin: String,
    /// Rate taken from the matching tariff row.
    pub rate_percent: f64,
    /// Index of the tariff row in the project table.
    pub row_index: usize,
    /// Note from the tariff row.
    pub note: String,
    /// True when the tariff row's effective dates are past.
    pub stale: bool,
}

/// Export-and-trade summary for one project bill of materials.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ExportTradeReport {
    /// Rule set id that produced the report.
    pub rule_set_id: String,
    /// Rule set title for display.
    pub rule_set_title: String,
    /// Highest declared ECCN on the bill, if any.
    pub highest_eccn: Option<String>,
    /// Bill lines without a usable ECCN declaration.
    pub unknown_eccn_count: usize,
    /// Bill lines without a declared HTS code.
    pub unknown_hts_count: usize,
    /// User ITAR reminder flag from the project.
    pub itar_flag: bool,
    /// Count of bill lines per ISO origin code.
    pub origin_totals: BTreeMap<String, u32>,
    /// Manufacturer screening matches.
    pub screening_hits: Vec<ScreeningHit>,
    /// Retrieved date of the screening list used.
    pub screening_snapshot_retrieved: String,
    /// True when the screening list is older than its cadence.
    pub screening_snapshot_stale: bool,
    /// Tariff estimates from the project table.
    pub tariff_estimates: Vec<TariffEstimate>,
    /// Indexes of tariff rows whose effective end date has passed.
    pub stale_tariff_row_indexes: Vec<usize>,
    /// Fixed disclaimer printed on every report.
    pub disclaimer: &'static str,
}

fn field_any<'a>(component: &'a Component, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(v) = component.field(key) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

fn bom_lines(project: &Project) -> Vec<&Component> {
    project.components.iter().filter(|c| c.in_bom()).collect()
}

fn load_screening_list(settings: &ExportTradeSettings) -> Result<EntityListSnapshot, String> {
    let id = settings.screening_list_id.trim();
    if id.is_empty() || id == "party_screening_fixture" {
        return EntityListSnapshot::parse(PARTY_SCREENING_FIXTURE);
    }
    Err(format!("no bundled screening list named {id}"))
}

fn tariff_row_active(row: &TariffExposureRow, today: &str) -> bool {
    if today < row.effective_from.as_str() {
        return false;
    }
    if !row.effective_to.is_empty() && today > row.effective_to.as_str() {
        return false;
    }
    true
}

fn tariff_row_stale(row: &TariffExposureRow, today: &str) -> bool {
    !row.effective_to.is_empty() && today > row.effective_to.as_str()
}

fn match_tariff_row<'a>(
    rows: &'a [TariffExposureRow],
    hts: &str,
    origin: &str,
    today: &str,
) -> Option<(usize, &'a TariffExposureRow)> {
    let hts = hts.trim();
    let origin = origin.trim().to_ascii_uppercase();
    if hts.is_empty() || origin.len() != 2 {
        return None;
    }
    let mut best: Option<(usize, usize, &TariffExposureRow)> = None;
    for (index, row) in rows.iter().enumerate() {
        if row.origin.to_ascii_uppercase() != origin {
            continue;
        }
        if !hts.starts_with(row.hts_prefix.trim()) {
            continue;
        }
        if !tariff_row_active(row, today) {
            continue;
        }
        let prefix_len = row.hts_prefix.trim().len();
        let replace = best.is_none_or(|(_, len, _)| prefix_len > len);
        if replace {
            best = Some((index, prefix_len, row));
        }
    }
    best.map(|(index, _, row)| (index, row))
}

/// Build the export-and-trade report for the project's bill of materials.
pub fn export_trade_report(project: &Project, today: &str) -> Result<ExportTradeReport, String> {
    export_trade_report_with_ruleset(project, today, &bundled_export_and_trade()?)
}

/// Build a report using an explicit rule set definition.
pub fn export_trade_report_with_ruleset(
    project: &Project,
    today: &str,
    ruleset: &ComplianceRuleSet,
) -> Result<ExportTradeReport, String> {
    project.validate()?;
    if ruleset.id != "export_and_trade" {
        return Err("expected export_and_trade rule set".into());
    }
    let lines = bom_lines(project);
    let eccn_codes: Vec<&str> = lines
        .iter()
        .filter_map(|c| field_any(c, FIELD_ECCN))
        .collect();
    let (highest_eccn, unknown_eccn_count) = highest_eccn(eccn_codes.iter().copied());

    let mut unknown_hts_count = 0usize;
    let mut origin_totals: BTreeMap<String, u32> = BTreeMap::new();
    for component in &lines {
        if field_any(component, FIELD_HTS).is_none() {
            unknown_hts_count += 1;
        }
        if let Some(origin) = field_any(component, FIELD_ORIGIN) {
            let code = origin.trim().to_ascii_uppercase();
            if code.len() == 2 {
                *origin_totals.entry(code).or_insert(0) += 1;
            }
        }
    }

    let screening = load_screening_list(&project.export_trade)?;
    let screening_snapshot_stale = screening.is_stale(today);
    let mut screening_hits = Vec::new();
    for component in &lines {
        let Some(mfg) = field_any(component, FIELD_MANUFACTURER) else {
            continue;
        };
        if let Some(entry) = screening.match_manufacturer(mfg) {
            screening_hits.push(ScreeningHit {
                reference: component.reference.clone(),
                manufacturer: mfg.to_string(),
                list_entry: entry.name.clone(),
                snapshot_id: screening.id.clone(),
                snapshot_retrieved: screening.retrieved.clone(),
                citation: entry.citation.clone(),
            });
        }
    }

    let mut stale_tariff_row_indexes = Vec::new();
    for (index, row) in project.export_trade.tariff_rows.iter().enumerate() {
        if tariff_row_stale(row, today) {
            stale_tariff_row_indexes.push(index);
        }
    }

    let mut tariff_estimates = Vec::new();
    for component in &lines {
        let Some(hts) = field_any(component, FIELD_HTS) else {
            continue;
        };
        let Some(origin) = field_any(component, FIELD_ORIGIN) else {
            continue;
        };
        let Some((row_index, row)) =
            match_tariff_row(&project.export_trade.tariff_rows, hts, origin, today)
        else {
            continue;
        };
        tariff_estimates.push(TariffEstimate {
            reference: component.reference.clone(),
            hts: hts.to_string(),
            origin: origin.trim().to_ascii_uppercase(),
            rate_percent: row.rate_percent,
            row_index,
            note: row.note.clone(),
            stale: stale_tariff_row_indexes.contains(&row_index),
        });
    }

    Ok(ExportTradeReport {
        rule_set_id: ruleset.id.clone(),
        rule_set_title: ruleset.title.clone(),
        highest_eccn,
        unknown_eccn_count,
        unknown_hts_count,
        itar_flag: project.export_trade.itar_flag,
        origin_totals,
        screening_hits,
        screening_snapshot_retrieved: screening.retrieved.clone(),
        screening_snapshot_stale,
        tariff_estimates,
        stale_tariff_row_indexes,
        disclaimer: DISCLAIMER,
    })
}

/// True when `code` parses as a known ECCN token for summary purposes.
pub fn eccn_known(code: &str) -> bool {
    control_rank(code).is_some()
}
