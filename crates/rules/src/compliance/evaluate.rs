use super::ComplianceFinding;
use super::list_load::load_bundled_list;
use super::ruleset::{ComplianceRuleSet, ComplianceRuleSpec};
use pcbcraft_doc::{Component, DeclarationStatus, PartRole, Project, date_le, valid_date};

pub const EVAL_DAY: &str = "2026-06-15";

pub fn evaluate_ruleset(project: &Project, set: &ComplianceRuleSet) -> Vec<ComplianceFinding> {
    let mut out = Vec::new();
    for rule in &set.rules {
        match rule.kind.as_str() {
            "declaration_required" => declaration_required(project, rule, &mut out),
            "declaration_fresh" => declaration_fresh(project, rule, &mut out),
            "entity_list" => super::us_government::entity_list_rule(project, rule, &mut out),
            "silicon_vendor" => super::us_government::silicon_vendor_rule(project, rule, &mut out),
            "origin" => origin_rule(project, rule, &mut out),
            "attribute_limit" => attribute_limit(project, rule, &mut out),
            "classification_report" => classification_report(project, rule, &mut out),
            _ => {}
        }
    }
    out
}

pub(crate) fn apply_mutes(project: &Project, findings: &mut Vec<ComplianceFinding>) {
    findings.retain(|f| {
        let component = f.objects.first().copied().unwrap_or(0);
        !project.compliance_muted(&f.rule, component, f.manufacturer.as_deref())
    });
}

fn declaration_required(
    project: &Project,
    rule: &ComplianceRuleSpec,
    out: &mut Vec<ComplianceFinding>,
) {
    for component in &project.components {
        if !role_applies(rule, component) {
            continue;
        }
        let message = message_or_default(
            &rule.message_unverified,
            "No declaration is recorded for this rule.",
        );
        let decls = component.effective_declarations();
        let matching = decls
            .iter()
            .find(|d| d.regime.trim().eq_ignore_ascii_case(rule.regime.trim()));
        match matching {
            None => push_unverified(out, rule, component, &message),
            Some(d) if d.status == DeclarationStatus::Unknown => {
                push_unverified(out, rule, component, &message);
            }
            Some(d) if d.status == DeclarationStatus::DeclaredNoncompliant => {
                push_hit(
                    out,
                    rule,
                    component,
                    "declaration is non-compliant",
                    &message_or_default(&rule.message_hit, "The part is declared non-compliant."),
                );
            }
            Some(d) if d.status == DeclarationStatus::DeclaredExempt => {
                push_declared_ok(
                    out,
                    rule,
                    component,
                    &format!("declared exempt: {}", d.detail),
                );
            }
            Some(_) => {}
        }
    }
}

fn declaration_fresh(
    project: &Project,
    rule: &ComplianceRuleSpec,
    out: &mut Vec<ComplianceFinding>,
) {
    let snapshot = rule.list.as_str();
    let list = if snapshot.is_empty() {
        None
    } else {
        load_bundled_list(snapshot).ok()
    };
    for component in &project.components {
        if !role_applies(rule, component) {
            continue;
        }
        let Some(decl) = super::declaration_of(component, &rule.regime) else {
            continue;
        };
        if decl.status == DeclarationStatus::Unknown {
            continue;
        }
        let mut stale = false;
        let mut evidence = String::new();
        if let Some(list) = list.as_ref() {
            if list.stale_for_cadence(EVAL_DAY) {
                stale = true;
                evidence = format!(
                    "list {} retrieved {} is past its {}-day cadence",
                    list.id, list.retrieved_date, list.cadence_days
                );
            }
            if valid_date(&decl.as_of)
                && valid_date(&list.retrieved_date)
                && date_le(&decl.as_of, &list.retrieved_date)
                && decl.as_of != list.retrieved_date
            {
                stale = true;
                if !evidence.is_empty() {
                    evidence.push_str("; ");
                }
                evidence.push_str("declaration predates the list snapshot");
            }
        }
        if stale {
            push_stale(
                out,
                rule,
                component,
                &evidence,
                &message_or_default(
                    &rule.message_stale,
                    "Declaration is older than the list or standard version it refers to.",
                ),
            );
        } else if decl.status == DeclarationStatus::DeclaredExempt {
            push_declared_ok(out, rule, component, "declaration exempt and fresh");
        }
    }
}

fn origin_rule(project: &Project, rule: &ComplianceRuleSpec, out: &mut Vec<ComplianceFinding>) {
    for component in &project.components {
        if !role_applies(rule, component) {
            continue;
        }
        let Some(origin) = component.compliance.country_of_origin.as_ref() else {
            push_unverified(
                out,
                rule,
                component,
                &message_or_default(
                    &rule.message_unverified,
                    "Country of origin is not recorded.",
                ),
            );
            continue;
        };
        let code = origin.code.trim().to_ascii_uppercase();
        if rule.status.eq_ignore_ascii_case("restricted")
            && rule.attribute.eq_ignore_ascii_case("country")
            && code == rule.field.trim().to_ascii_uppercase()
        {
            push_hit(
                out,
                rule,
                component,
                &format!("country of origin {code}"),
                &message_or_default(
                    &rule.message_hit,
                    "Country of origin matches the restricted code in this rule.",
                ),
            );
        }
    }
}

fn attribute_limit(project: &Project, rule: &ComplianceRuleSpec, out: &mut Vec<ComplianceFinding>) {
    for component in &project.components {
        if !role_applies(rule, component) {
            continue;
        }
        let count = component.compliance.substances.len() as f64;
        if count > rule.limit {
            push_hit(
                out,
                rule,
                component,
                &format!("{count} substances listed, limit {}", rule.limit),
                &message_or_default(&rule.message_hit, "Substance count exceeds the rule limit."),
            );
        }
    }
}

fn classification_report(
    project: &Project,
    rule: &ComplianceRuleSpec,
    out: &mut Vec<ComplianceFinding>,
) {
    for component in &project.components {
        if !role_applies(rule, component) {
            continue;
        }
        let field = if rule.field.is_empty() {
            "Manufacturer"
        } else {
            rule.field.as_str()
        };
        let Some(value) = component
            .field(field)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            push_unverified(
                out,
                rule,
                component,
                &message_or_default(
                    &rule.message_unverified,
                    "Classification field is missing on this part.",
                ),
            );
            continue;
        };
        push_finding(
            out,
            rule,
            component,
            "unverified",
            &message_or_default(
                &rule.message_reference,
                "Classification is present but not verified against an external register.",
            ),
            &format!("{field} is {value}"),
        );
    }
}

pub(super) fn role_applies(rule: &ComplianceRuleSpec, component: &Component) -> bool {
    let roles: Vec<&str> = if rule.roles.is_empty() {
        if rule.role.is_empty() {
            return true;
        }
        vec![rule.role.as_str()]
    } else {
        rule.roles.iter().map(String::as_str).collect()
    };
    if roles.is_empty() || roles == [""] {
        return true;
    }
    let Some(role) = component.compliance.role else {
        return false;
    };
    roles.iter().any(|want| part_role_matches(want, role))
}

fn part_role_matches(want: &str, role: PartRole) -> bool {
    let text = pcbcraft_doc::part_role_text(role);
    want.eq_ignore_ascii_case(text) || want.eq_ignore_ascii_case(&snake_case_role(role))
}

fn snake_case_role(role: PartRole) -> String {
    role.token().to_owned()
}

fn component_manufacturer(component: &Component) -> Option<&str> {
    component
        .field("Manufacturer")
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn message_or_default(text: &str, default: &str) -> String {
    if text.is_empty() {
        default.to_owned()
    } else {
        text.to_owned()
    }
}

fn push_finding(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component: &Component,
    state: &str,
    message: &str,
    evidence: &str,
) {
    let severity = match state {
        "hit" => rule.severity_hit.clone(),
        "stale" => rule
            .severity_stale
            .clone()
            .unwrap_or_else(|| rule.severity_hit.clone()),
        "declared_ok" => "info".into(),
        _ => rule.severity_unverified.clone(),
    };
    out.push(ComplianceFinding {
        code: rule.id.clone(),
        state: state.into(),
        severity,
        message: message.to_owned(),
        rule: rule.id.clone(),
        evidence: format!("Part {}: {evidence}", component.reference),
        citation: rule.citation.clone(),
        objects: vec![component.id],
        manufacturer: component_manufacturer(component).map(str::to_owned),
    });
}

fn push_unverified(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component: &Component,
    message: &str,
) {
    push_finding(out, rule, component, "unverified", message, "missing data");
}

fn push_hit(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component: &Component,
    evidence: &str,
    message: &str,
) {
    push_finding(out, rule, component, "hit", message, evidence);
}

fn push_stale(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component: &Component,
    evidence: &str,
    message: &str,
) {
    push_finding(out, rule, component, "stale", message, evidence);
}

fn push_declared_ok(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component: &Component,
    evidence: &str,
) {
    push_finding(
        out,
        rule,
        component,
        "declared_ok",
        "Declaration records an exemption or acceptable status.",
        evidence,
    );
}
