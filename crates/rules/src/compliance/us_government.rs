use super::entity_match::{match_entity_name, EntityMatchConfidence};
use super::list_load::load_bundled_list;
use super::ruleset::{load_rule_set, ComplianceRuleSpec};
use super::{ComplianceFinding, EVAL_DAY};
use pcbcraft_doc::{Component, PartRole, Project};

pub const US_GOVERNMENT_NDAA_RULE_SET: &str =
    include_str!("../../data/compliance/rulesets/us_government_ndaa.toml");

pub fn evaluate(project: &Project) -> Vec<ComplianceFinding> {
    let set = match load_rule_set(US_GOVERNMENT_NDAA_RULE_SET) {
        Ok(s) => s,
        Err(e) => {
            return vec![ComplianceFinding {
                code: "us_government_ndaa_load".into(),
                state: "hit".into(),
                severity: "error".into(),
                message: format!("US government rule set failed to load: {e}"),
                rule: "us_government_ndaa".into(),
                evidence: e,
                citation: "PCBCraft bundled compliance data".into(),
                objects: vec![],
            }];
        }
    };
    let mut out = Vec::new();
    for rule in &set.rules {
        match rule.kind.as_str() {
            "entity_list" => entity_list_rule(project, rule, &mut out),
            "silicon_vendor" => silicon_vendor_rule(project, rule, &mut out),
            _ => {}
        }
    }
    if !set.description.is_empty() {
        for finding in &mut out {
            if finding.state == "hit" || finding.state == "possible" {
                finding.message.push_str(" Affiliation and substantial-component questions remain legal judgements.");
            }
        }
    }
    out
}

fn entity_list_rule(project: &Project, rule: &ComplianceRuleSpec, out: &mut Vec<ComplianceFinding>) {
    let Ok(snapshot) = load_bundled_list(&rule.list) else {
        return;
    };
    let entry_filter = if rule.entry_type.is_empty() {
        None
    } else {
        Some(rule.entry_type.as_str())
    };
    let staleness = snapshot_stale_note(&snapshot, EVAL_DAY);
    for component in &project.components {
        let Some(maker) = component_manufacturer(component) else {
            push_unverified(out, rule, component, &rule.message_unverified);
            continue;
        };
        let resolved = project
            .resolve_manufacturer(maker)
            .map(|r| r.name.as_str())
            .unwrap_or(maker);
        let Some(entity_match) = match_entity_name(&snapshot, resolved, entry_filter) else {
            continue;
        };
        let evidence = format!(
            "list {} retrieved {}, entry {}{}",
            snapshot.id,
            snapshot.retrieved_date,
            entity_match.entry_name,
            entry_type_suffix(&entity_match.entry_type, &entity_match.added_date)
        );
        let message = if entity_match.confidence == EntityMatchConfidence::Possible {
            format!(
                "Possible name match only (not a confirmed list hit): {}",
                rule.message_hit
            )
        } else {
            rule.message_hit.clone()
        };
        let state = if entity_match.confidence == EntityMatchConfidence::Possible {
            "possible"
        } else {
            "hit"
        };
        push_finding(
            out,
            rule,
            component,
            state,
            &message,
            &append_staleness(&evidence, staleness.as_deref()),
        );
    }
}

fn silicon_vendor_rule(
    project: &Project,
    rule: &ComplianceRuleSpec,
    out: &mut Vec<ComplianceFinding>,
) {
    let Ok(snapshot) = load_bundled_list(&rule.list) else {
        return;
    };
    let staleness = snapshot_stale_note(&snapshot, EVAL_DAY);
    for component in &project.components {
        let Some(role) = component
            .compliance
            .as_ref()
            .and_then(|c| c.role)
        else {
            continue;
        };
        if !role_matches(rule, role) {
            continue;
        }
        let silicon = component
            .compliance
            .as_ref()
            .and_then(|c| c.silicon_vendor.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(vendor) = silicon else {
            push_unverified(out, rule, component, &rule.message_unverified);
            continue;
        };
        let Some(entity_match) =
            match_entity_name(&snapshot, vendor, Some("covered_semiconductor"))
        else {
            continue;
        };
        let evidence = format!(
            "list {} retrieved {}, entry {}{}",
            snapshot.id,
            snapshot.retrieved_date,
            entity_match.entry_name,
            entry_type_suffix(&entity_match.entry_type, &entity_match.added_date)
        );
        let state = if entity_match.confidence == EntityMatchConfidence::Possible {
            "possible"
        } else {
            "hit"
        };
        push_finding(
            out,
            rule,
            component,
            state,
            &rule.message_hit,
            &append_staleness(&evidence, staleness.as_deref()),
        );
    }
}

fn role_matches(rule: &ComplianceRuleSpec, role: PartRole) -> bool {
    if rule.roles.is_empty() {
        return matches!(role, PartRole::Processor | PartRole::Memory);
    }
    let text = pcbcraft_doc::part_role_text(role);
    rule.roles.iter().any(|r| r == text)
}

fn component_manufacturer(component: &Component) -> Option<&str> {
    component
        .fields
        .iter()
        .find(|f| f.key.eq_ignore_ascii_case("Manufacturer"))
        .map(|f| f.value.as_str())
        .filter(|v| !v.trim().is_empty())
}

fn entry_type_suffix(entry_type: &str, added: &str) -> String {
    let mut out = String::new();
    if !entry_type.is_empty() {
        out.push_str(&format!(", entry type {entry_type}"));
    }
    if !added.is_empty() {
        out.push_str(&format!(", added {added}"));
    }
    out
}

fn snapshot_stale_note(
    snapshot: &pcbcraft_doc::ComplianceListSnapshot,
    today: &str,
) -> Option<String> {
    if snapshot.cadence_days == 0 {
        return None;
    }
    let age = days_since(&snapshot.retrieved_date, today)?;
    if age > snapshot.cadence_days {
        Some(format!(
            " Snapshot retrieved {} is {} days old (cadence {} days).",
            snapshot.retrieved_date, age, snapshot.cadence_days
        ))
    } else {
        None
    }
}

fn days_since(retrieved: &str, today: &str) -> Option<u32> {
    let (ry, rm, rd) = parse_ymd(retrieved)?;
    let (ty, tm, td) = parse_ymd(today)?;
    let r = ymd_to_ordinal(ry, rm, rd)?;
    let t = ymd_to_ordinal(ty, tm, td)?;
    t.checked_sub(r)
}

fn parse_ymd(s: &str) -> Option<(u32, u32, u32)> {
    let mut parts = s.split('-');
    let y: u32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((y, m, d))
}

fn ymd_to_ordinal(y: u32, m: u32, d: u32) -> Option<u32> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let mut ordinal = d;
    for month in 1..m {
        ordinal += days_in_month(y, month);
    }
    for year in 1970..y {
        ordinal += if is_leap(year) { 366 } else { 365 };
    }
    Some(ordinal)
}

fn days_in_month(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn append_staleness(evidence: &str, note: Option<&str>) -> String {
    match note {
        Some(n) => format!("{evidence}{n}"),
        None => evidence.to_string(),
    }
}

fn push_unverified(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component: &Component,
    message: &str,
) {
    push_finding(out, rule, component, "unverified", message, "No list match; data missing.");
}

fn push_finding(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component: &Component,
    state: &str,
    message: &str,
    evidence: &str,
) {
    let severity = if state == "unverified" {
        rule.severity_unverified.clone()
    } else {
        rule.severity_hit.clone()
    };
    out.push(ComplianceFinding {
        code: rule.id.clone(),
        state: state.into(),
        severity,
        message: message.to_string(),
        rule: rule.id.clone(),
        evidence: format!("Part {}: {evidence}", component.reference),
        citation: rule.citation.clone(),
        objects: vec![component.id],
    });
}
