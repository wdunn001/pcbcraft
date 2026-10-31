use super::{
    exemption_number, finish_is_leaded, is_exempt, is_unknown, list_version_stale,
    load_list_snapshot, load_rule_set, push_board_hit, push_hit, push_stale, push_unverified,
    ComplianceFinding, ComplianceRuleSpec,
};
use pcbcraft_doc::{DeclarationStatus, PartRole, Project};

pub const EU_MARKET_RULE_SET: &str = include_str!("../../data/compliance/rulesets/eu_market.toml");
const REACH_LIST: &str = include_str!("../../data/compliance/lists/reach_svhc_candidate.toml");
const ROHS_EXEMPTIONS: &str = include_str!("../../data/compliance/lists/rohs_exemptions.toml");

/// Fixed "today" for bundled evaluation so tests stay deterministic.
const EVAL_DAY: &str = "2026-03-28";

pub fn evaluate(project: &Project) -> Vec<ComplianceFinding> {
    let set = match load_rule_set(EU_MARKET_RULE_SET) {
        Ok(s) => s,
        Err(e) => {
            return vec![ComplianceFinding {
                code: "eu_market_load".into(),
                state: "hit".into(),
                severity: "error".into(),
                message: format!("EU market rule set failed to load: {e}"),
                rule: "eu_market".into(),
                evidence: e,
                citation: "PCBCraft bundled compliance data".into(),
                objects: vec![],
            }];
        }
    };
    let reach = load_list_snapshot(REACH_LIST).ok();
    let rohs_ex = load_list_snapshot(ROHS_EXEMPTIONS).ok();
    let mut out = Vec::new();
    for rule in &set.rules {
        match rule.kind.as_str() {
            "declaration_required" => {
                declaration_required(project, rule, &mut out);
            }
            "declaration_fresh" => {
                declaration_fresh(project, rule, reach.as_ref(), rohs_ex.as_ref(), &mut out);
            }
            "board_finish" => {
                board_finish(project, rule, &mut out);
            }
            "classification_report" => {
                classification_report(project, rule, &mut out);
            }
            _ => {}
        }
    }
    if let Some(reach) = reach.as_ref()
        && reach.stale_for_cadence(EVAL_DAY)
    {
        for finding in &mut out {
            if finding.rule.contains("reach") {
                finding.evidence.push_str(&format!(
                    " (REACH snapshot retrieved {} is past its {}-day cadence.)",
                    reach.retrieved, reach.cadence_days
                ));
            }
        }
    }
    out
}

fn effective_declaration(
    component: &pcbcraft_doc::Component,
    regime: &str,
) -> Option<pcbcraft_doc::Declaration> {
    component.effective_declarations().into_iter().find(|d| {
        d.regime
            .trim()
            .eq_ignore_ascii_case(regime.trim())
    })
}

fn declaration_required(project: &Project, rule: &ComplianceRuleSpec, out: &mut Vec<ComplianceFinding>) {
    let mut msg = if rule.message_unverified.is_empty() {
        "No declaration is recorded for this rule.".to_string()
    } else {
        rule.message_unverified.clone()
    };
    if !rule.message_reference.is_empty() {
        msg.push(' ');
        msg.push_str(&rule.message_reference);
    }
    for component in &project.components {
        if !component.in_bom() {
            continue;
        }
        let decl = effective_declaration(component, &rule.regime);
        match decl.as_ref() {
            None | Some(d) if is_unknown(d) => {
                push_unverified(out, rule, component.id, &component.reference, &msg);
            }
            Some(d) if d.status == DeclarationStatus::DeclaredNoncompliant => {
                push_hit(
                    out,
                    rule,
                    component.id,
                    &component.reference,
                    "declaration is non-compliant",
                    "The part is declared non-compliant for this regime.",
                );
            }
            Some(_) => {}
        }
    }
}

fn declaration_fresh(
    project: &Project,
    rule: &ComplianceRuleSpec,
    reach: Option<&super::ListSnapshot>,
    rohs_ex: Option<&super::ListSnapshot>,
    out: &mut Vec<ComplianceFinding>,
) {
    for component in &project.components {
        if !component.in_bom() {
            continue;
        }
        let Some(decl) = effective_declaration(component, &rule.regime) else {
            continue;
        };
        if rule.status == "declared_exempt" && !is_exempt(&decl) {
            continue;
        }
        if rule.list == "rohs_exemptions" {
            let Some(list) = rohs_ex else {
                continue;
            };
            let Some(number) = exemption_number(&decl) else {
                push_stale(
                    out,
                    rule,
                    component.id,
                    &component.reference,
                    "exempt declaration without exemption number in detail",
                    &rule.message_stale,
                );
                continue;
            };
            if !list.exemption_active(number, EVAL_DAY) {
                push_stale(
                    out,
                    rule,
                    component.id,
                    &component.reference,
                    &format!("exemption {number} is expired or not in the bundled list"),
                    &rule.message_stale,
                );
            }
        }
        if rule.list == "reach_svhc_candidate" {
            let Some(list) = reach else {
                continue;
            };
            if list_version_stale(&decl.detail, &list.version, &list.retrieved, &decl.as_of) {
                push_stale(
                    out,
                    rule,
                    component.id,
                    &component.reference,
                    &format!(
                        "declaration cites list version {} but bundled snapshot is {}",
                        decl.detail, list.version
                    ),
                    &rule.message_stale,
                );
            }
        }
    }
}

fn board_finish(project: &Project, rule: &ComplianceRuleSpec, out: &mut Vec<ComplianceFinding>) {
    if !finish_is_leaded(project) {
        return;
    }
    if project
        .compliance_settings
        .board_rohs_exemption
        .as_ref()
        .is_some_and(|n| !n.is_empty())
    {
        return;
    }
    let finish = &project.board.finish;
    let solder = project
        .board
        .rules
        .as_ref()
        .and_then(|r| r.solder.as_deref())
        .unwrap_or("(not set)");
    push_board_hit(
        out,
        rule,
        &format!("Board finish {finish:?}, solder {solder:?}."),
        &rule.message_hit,
    );
}

fn classification_report(project: &Project, rule: &ComplianceRuleSpec, out: &mut Vec<ComplianceFinding>) {
    let want_battery = rule.role == "battery";
    for component in &project.components {
        if !component.in_bom() {
            continue;
        }
        let is_battery = component.compliance.role == Some(PartRole::Battery);
        if want_battery && is_battery {
            push_hit(
                out,
                rule,
                component.id,
                &component.reference,
                "part role is battery",
                &rule.message_hit,
            );
        }
    }
}
