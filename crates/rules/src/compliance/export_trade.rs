//! Export-and-trade bill-of-materials summary for rule set `export_and_trade`.
use pcbcraft_doc::{Component, ExportTradeSettings, Project, TariffExposureRow};
use serde::Serialize;
use std::collections::BTreeMap;

use super::eccn::{control_rank, highest_eccn};
use super::list_snapshot::EntityListSnapshot;
use super::ruleset::{ComplianceRuleSet, bundled_export_and_trade};

pub const FIELD_ECCN: &[&str] = &["ECCN", "eccn", "Export Control Classification Number"];
pub const FIELD_HTS: &[&str] = &["HTS", "hts", "HTS Code", "Harmonized Tariff"];
pub const FIELD_MANUFACTURER: &[&str] = &["Manufacturer", "Mfg", "MFR"];
pub const FIELD_ORIGIN: &[&str] = &["Country of Origin", "COO", "Origin", "country_of_origin"];

const PARTY_SCREENING_FIXTURE: &str =
    include_str!("../../data/compliance/lists/party_screening_fixture.toml");

const DISCLAIMER: &str = "Informational summary from part data and user-entered tables only. \
This is not legal advice and does not state licence or authorization requirements.";

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ScreeningHit {
    pub reference: String,
    pub manufacturer: String,
    pub list_entry: String,
    pub snapshot_id: String,
    pub snapshot_retrieved: String,
    pub citation: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct TariffEstimate {
    pub reference: String,
    pub hts: String,
    pub origin: String,
    pub rate_percent: f64,
    pub row_index: usize,
    pub note: String,
    pub stale: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ExportTradeReport {
    pub rule_set_id: String,
    pub rule_set_title: String,
    pub highest_eccn: Option<String>,
    pub unknown_eccn_count: usize,
    pub unknown_hts_count: usize,
    pub itar_flag: bool,
    pub origin_totals: BTreeMap<String, u32>,
    pub screening_hits: Vec<ScreeningHit>,
    pub screening_snapshot_retrieved: String,
    pub screening_snapshot_stale: bool,
    pub tariff_estimates: Vec<TariffEstimate>,
    pub stale_tariff_row_indexes: Vec<usize>,
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
