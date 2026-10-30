use super::{
    is_unknown, load_list_snapshot, load_rule_set, push_hit, ComplianceFinding,
    ComplianceRuleSpec,
};
use pcbcraft_doc::{ComplianceStatus, Project};
use std::collections::BTreeSet;

pub const RESPONSIBLE_SOURCING_RULE_SET: &str =
    include_str!("../../data/compliance/rulesets/responsible_sourcing.toml");
const PROP65_LIST: &str =
    include_str!("../../data/compliance/lists/prop65_chemicals_fixture.toml");

const EVAL_DAY: &str = "2026-03-28";

pub fn evaluate(project: &Project) -> Vec<ComplianceFinding> {
    let set = match load_rule_set(RESPONSIBLE_SOURCING_RULE_SET) {
        Ok(s) => s,
        Err(e) => {
            return vec![ComplianceFinding {
                code: "responsible_sourcing_load".into(),
                state: "hit".into(),
                severity: "warning".into(),
                message: format!("Responsible sourcing rule set failed to load: {e}"),
                rule: "responsible_sourcing".into(),
                evidence: e,
                citation: "PCBCraft bundled compliance data".into(),
                objects: vec![],
            }];
        }
    };
    let prop65 = load_list_snapshot(PROP65_LIST).ok();
    let rule_as_of = rule_set_as_of(RESPONSIBLE_SOURCING_RULE_SET);
    let mut out = Vec::new();
    for rule in &set.rules {
        match rule.kind.as_str() {
            "manufacturer_minerals_report" => {
                manufacturer_minerals_report(project, rule, &mut out);
            }
            "scoped_declaration" => {
                scoped_declaration(project, rule, rule_as_of, &mut out);
            }
            "listed_substance" => {
                listed_substance(project, rule, prop65.as_ref(), &mut out);
            }
            _ => {}
        }
    }
    cap_severity(&mut out);
    out
}

fn rule_set_as_of(toml_text: &str) -> &str {
    toml_text
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("as_of = ")
                .map(|rest| rest.trim_matches('"'))
        })
        .unwrap_or(EVAL_DAY)
}

fn cap_severity(out: &mut [ComplianceFinding]) {
    for finding in out {
        if finding.severity == "error" {
            finding.severity = "warning".into();
        }
    }
}

fn manufacturers_on_bom(project: &Project) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for component in &project.components {
        if !component.in_bom() {
            continue;
        }
        if let Some(name) = component.field("Manufacturer") {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                names.insert(trimmed.to_string());
            }
        }
    }
    names
}

fn manufacturer_needs_emrt(project: &Project, manufacturer: &str) -> bool {
    for component in &project.components {
        if !component.in_bom() {
            continue;
        }
        if component.field("Manufacturer") != Some(manufacturer) {
            continue;
        }
        let role = project
            .compliance_by_component
            .get(&component.id)
            .and_then(|c| c.role.as_deref())
            .unwrap_or("");
        if role == "battery" {
            return true;
        }
    }
    false
}

fn manufacturer_minerals_report(
    project: &Project,
    rule: &ComplianceRuleSpec,
    out: &mut Vec<ComplianceFinding>,
) {
    let required_template = if !rule.template.is_empty() {
        rule.template.as_str()
    } else {
        rule.field.as_str()
    };
    if required_template.is_empty() {
        return;
    }
    for name in manufacturers_on_bom(project) {
        if rule.role == "battery" && !manufacturer_needs_emrt(project, &name) {
            continue;
        }
        if rule.role != "battery" && !rule.role.is_empty() {
            continue;
        }
        let record = project.resolve_manufacturer(&name);
        let on_file = record.is_some_and(|r| {
            r.minerals_report
                .as_ref()
                .is_some_and(|m| m.valid() && m.template == required_template)
        });
        if on_file {
            continue;
        }
        let mut message = if rule.message_unverified.is_empty() {
            format!(
                "Manufacturer {name} has no {required_template} minerals report on file with version and date."
            )
        } else {
            rule.message_unverified.clone()
        };
        if !rule.message_reference.is_empty() {
            message.push(' ');
            message.push_str(&rule.message_reference);
        }
        push_manufacturer(
            out,
            rule,
            &name,
            "unverified",
            &rule.severity_unverified,
            &message,
            &format!("Manufacturer {name} is on the bill of materials without a current {required_template} template on file."),
        );
    }
}

fn scoped_declaration(
    project: &Project,
    rule: &ComplianceRuleSpec,
    rule_as_of: &str,
    out: &mut Vec<ComplianceFinding>,
) {
    if rule.role.is_empty() || rule.regime.is_empty() {
        return;
    }
    for component in &project.components {
        if !component.in_bom() {
            continue;
        }
        let role = project
            .compliance_by_component
            .get(&component.id)
            .and_then(|c| c.role.as_deref())
            .unwrap_or("");
        if role != rule.role {
            continue;
        }
        let comp = match project.compliance_by_component.get(&component.id) {
            Some(c) => c,
            None => {
                push_scoped_unknown(
                    out,
                    rule,
                    component.id,
                    &component.reference,
                    rule_as_of,
                    "no compliance record",
                );
                continue;
            }
        };
        let decl = comp.declaration_for(&rule.regime);
        match decl {
            None | Some(d) if is_unknown(d) => {
                push_scoped_unknown(
                    out,
                    rule,
                    component.id,
                    &component.reference,
                    rule_as_of,
                    "PFAS status unknown",
                );
            }
            Some(d) if d.status == ComplianceStatus::DeclaredNoncompliant => {
                push_hit(
                    out,
                    rule,
                    component.id,
                    &component.reference,
                    "declared PFAS present or not exempt",
                    "The part declares PFAS or a reporting obligation; confirm customer communications.",
                );
            }
            Some(_) => {}
        }
    }
}

fn push_scoped_unknown(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component_id: u64,
    reference: &str,
    rule_as_of: &str,
    evidence: &str,
) {
    let mut message = if rule.message_unverified.is_empty() {
        "Reporting status is unknown for this part.".to_string()
    } else {
        rule.message_unverified.clone()
    };
    if !rule.message_reference.is_empty() {
        message.push(' ');
        message.push_str(&rule.message_reference);
        message.push_str(" Rule file date ");
        message.push_str(rule_as_of);
        message.push('.');
    }
    out.push(ComplianceFinding {
        code: rule.id.clone(),
        state: "unverified".into(),
        severity: rule.severity_unverified.clone(),
        message,
        rule: rule.id.clone(),
        evidence: format!("Part {reference}: {evidence} (rule file date {rule_as_of})."),
        citation: rule.citation.clone(),
        objects: vec![component_id],
    });
}

fn listed_substance(
    project: &Project,
    rule: &ComplianceRuleSpec,
    list: Option<&super::ListSnapshot>,
    out: &mut Vec<ComplianceFinding>,
) {
    let Some(list) = list else {
        return;
    };
    for component in &project.components {
        if !component.in_bom() {
            continue;
        }
        let Some(comp) = project.compliance_by_component.get(&component.id) else {
            continue;
        };
        for substance in &comp.substances {
            let Some(entry) = list.substances.iter().find(|e| {
                substance_matches(substance, &e.name, &e.cas)
            }) else {
                continue;
            };
            let evidence = format!(
                "declared {} matches list snapshot {} (retrieved {})",
                substance.name, list.version, list.retrieved
            );
            let message = if rule.message_hit.is_empty() {
                format!(
                    "List entry {} ({}) may require a Proposition 65 warning decision.",
                    entry.name, list.version
                )
            } else {
                format!("{} (list snapshot {}).", rule.message_hit, list.version)
            };
            push_hit(
                out,
                rule,
                component.id,
                &component.reference,
                &evidence,
                &message,
            );
        }
    }
}

fn substance_matches(
    declared: &pcbcraft_doc::DeclaredSubstance,
    entry_name: &str,
    entry_cas: &str,
) -> bool {
    if let Some(cas) = &declared.cas {
        if !cas.is_empty() && cas.eq_ignore_ascii_case(entry_cas) {
            return true;
        }
    }
    normalize_substance_name(&declared.name) == normalize_substance_name(entry_name)
}

fn normalize_substance_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn push_manufacturer(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    _manufacturer: &str,
    state: &str,
    severity: &str,
    message: &str,
    evidence: &str,
) {
    out.push(ComplianceFinding {
        code: rule.id.clone(),
        state: state.into(),
        severity: severity.into(),
        message: message.to_string(),
        rule: rule.id.clone(),
        evidence: evidence.to_string(),
        citation: rule.citation.clone(),
        objects: vec![],
    });
}
