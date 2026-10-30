//! Regulatory compliance rule evaluation (bundled rule sets and list snapshots).
mod lists;
mod ruleset;

use lists::{
    CoveredCountriesList, FrameworkList, RadioBandsList, band_allowed, country_covered,
    framework_entry, parse_covered_countries, parse_framework, parse_radio_bands,
};
use pcbcraft_doc::{
    ComplianceRole, ComplianceSettings, Component, DeclarationStatus, OriginInterpretation,
    Project,
};
use ruleset::{RuleSetDoc, parse_ruleset};
use serde::Serialize;

pub use lists::{CoveredCountriesList as ComplianceCoveredCountriesList, ListMeta};
pub use ruleset::RuleSetDoc as ComplianceRuleSetDoc;

const COVERED_COUNTRIES: &str =
    include_str!("../../data/compliance/lists/synthetic_covered_origin_countries.toml");
const BLUE_UAS_FRAMEWORK: &str =
    include_str!("../../data/compliance/lists/synthetic_blue_uas_framework.toml");
const RADIO_BANDS: &str = include_str!("../../data/compliance/lists/synthetic_radio_bands.toml");
const BLUE_UAS_RULESET: &str =
    include_str!("../../data/compliance/rulesets/blue_uas_style_drone.toml");

pub const BLUE_UAS_STYLE_DRONE_RULESET_ID: &str = "blue_uas_style_drone";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceState {
    Hit,
    Unverified,
    Stale,
    DeclaredOk,
}

#[derive(Clone, Debug, Serialize)]
pub struct ComplianceFinding {
    pub rule_id: String,
    pub state: ComplianceState,
    pub severity: String,
    pub message: String,
    pub evidence: String,
    pub citation: String,
    pub objects: Vec<pcbcraft_doc::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

struct BundledLists {
    covered: CoveredCountriesList,
    framework: FrameworkList,
    radio: RadioBandsList,
}

fn bundled_lists() -> Result<BundledLists, String> {
    Ok(BundledLists {
        covered: parse_covered_countries(COVERED_COUNTRIES)?,
        framework: parse_framework(BLUE_UAS_FRAMEWORK)?,
        radio: parse_radio_bands(RADIO_BANDS)?,
    })
}

fn ruleset_blue_uas() -> Result<RuleSetDoc, String> {
    parse_ruleset(BLUE_UAS_RULESET)
}

fn role_matches(rule_roles: &[String], role: &ComplianceRole) -> bool {
    if rule_roles.is_empty() {
        return true;
    }
    rule_roles
        .iter()
        .any(|r| r == role.as_str())
}

fn origin_unit(settings: &ComplianceSettings) -> OriginInterpretation {
    settings.origin_interpretation.clone()
}

fn maker_country(project: &Project, component: &Component, compliance: &pcbcraft_doc::ComponentCompliance) -> Option<String> {
    if let Some(code) = &compliance.maker_country {
        if !code.is_empty() {
            return Some(code.clone());
        }
    }
    let name = if !compliance.manufacturer.is_empty() {
        compliance.manufacturer.as_str()
    } else {
        component.field("Manufacturer").unwrap_or("")
    };
    if name.is_empty() {
        return None;
    }
    let norm = normalize_name(name);
    for record in &project.compliance.manufacturers {
        if normalize_name(&record.name) == norm
            || record.aliases.iter().any(|a| normalize_name(a) == norm)
        {
            return Some(record.country.clone());
        }
    }
    None
}

fn normalize_name(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn component_role(compliance: &pcbcraft_doc::ComponentCompliance) -> ComplianceRole {
    compliance.role.clone()
}

fn applies_origin_rule(
    interpretation: &OriginInterpretation,
    role: &ComplianceRole,
    rule_roles: &[String],
) -> bool {
    if !role_matches(rule_roles, role) {
        return false;
    }
    if *interpretation == OriginInterpretation::Module && *role == ComplianceRole::Other {
        return false;
    }
    true
}

/// Evaluate bundled compliance rule sets enabled on the project.
pub fn compliance_findings(project: &Project) -> Result<Vec<ComplianceFinding>, String> {
    let lists = bundled_lists()?;
    let mut out = Vec::new();
    for ruleset_id in &project.compliance.rule_sets {
        if ruleset_id == BLUE_UAS_STYLE_DRONE_RULESET_ID {
            let doc = ruleset_blue_uas()?;
            evaluate_blue_uas(project, &doc, &lists, &mut out)?;
        }
    }
    Ok(out)
}

fn evaluate_blue_uas(
    project: &Project,
    doc: &RuleSetDoc,
    lists: &BundledLists,
    out: &mut Vec<ComplianceFinding>,
) -> Result<(), String> {
    let interpretation = if doc.interpretation.default == "silicon" {
        OriginInterpretation::Silicon
    } else {
        origin_unit(&project.compliance)
    };
    for component in project.components.iter().filter(|c| c.in_bom()) {
        let Some(compliance) = &component.compliance else {
            continue;
        };
        let role = component_role(compliance);
        for rule in &doc.rules {
            match rule.kind.as_str() {
                "origin" => {
                    if !applies_origin_rule(&interpretation, &role, &rule.roles) {
                        continue;
                    }
                    let origin_code = compliance
                        .country_of_origin
                        .as_ref()
                        .map(|c| c.code.as_str());
                    let maker = maker_country(project, component, compliance);
                    let silicon = compliance.silicon_vendor_country.as_deref();
                    let mut hit = false;
                    let mut evidence = String::new();
                    if let Some(code) = origin_code {
                        if country_covered(&lists.covered, code) {
                            hit = true;
                            evidence.push_str(&format!(
                                "Country of origin {code} is on list {} retrieved {}.",
                                lists.covered.meta.id, lists.covered.meta.retrieved
                            ));
                        }
                    }
                    if let Some(code) = maker.as_deref() {
                        if country_covered(&lists.covered, code) {
                            hit = true;
                            if !evidence.is_empty() {
                                evidence.push(' ');
                            }
                            evidence.push_str(&format!(
                                "Maker country {code} is on list {} retrieved {}.",
                                lists.covered.meta.id, lists.covered.meta.retrieved
                            ));
                        }
                    }
                    if interpretation == OriginInterpretation::Silicon {
                        if let Some(code) = silicon {
                            if country_covered(&lists.covered, code) {
                                hit = true;
                                if !evidence.is_empty() {
                                    evidence.push(' ');
                                }
                                evidence.push_str(&format!(
                                    "Silicon vendor country {code} is on list {} retrieved {}.",
                                    lists.covered.meta.id, lists.covered.meta.retrieved
                                ));
                            }
                        }
                    }
                    if hit {
                        push_hit(out, rule, component, Some(role.as_str()), evidence);
                    } else if origin_code.is_none() && maker.is_none() {
                        push_unverified(
                            out,
                            rule,
                            component,
                            Some(role.as_str()),
                            "Origin and maker country are not declared.".into(),
                        );
                    } else {
                        push_declared_ok(
                            out,
                            rule,
                            component,
                            Some(role.as_str()),
                            "Declared origin and maker countries are not on the covered list.".into(),
                        );
                    }
                }
                "declaration_required" => {
                    if !role_matches(&rule.roles, &role) {
                        continue;
                    }
                    match compliance.declaration_for(&rule.regime) {
                        Some(d) => match d.status {
                            DeclarationStatus::DeclaredCompliant | DeclarationStatus::DeclaredExempt => {
                                push_declared_ok(
                                    out,
                                    rule,
                                    component,
                                    Some(role.as_str()),
                                    format!(
                                        "Declaration {} ({}) dated {} from {} (statement, not certification).",
                                        d.regime,
                                        status_label(&d.status),
                                        d.as_of,
                                        d.source.label
                                    ),
                                );
                            }
                            DeclarationStatus::DeclaredNoncompliant => push_hit(
                                out,
                                rule,
                                component,
                                Some(role.as_str()),
                                format!(
                                    "Declaration {} marked noncompliant dated {}.",
                                    d.regime, d.as_of
                                ),
                            ),
                            DeclarationStatus::NotApplicable => push_declared_ok(
                                out,
                                rule,
                                component,
                                Some(role.as_str()),
                                format!("Declaration {} marked not applicable.", d.regime),
                            ),
                            DeclarationStatus::Unknown => push_unverified(
                                out,
                                rule,
                                component,
                                Some(role.as_str()),
                                format!("Declaration {} status is unknown.", d.regime),
                            ),
                        },
                        None => push_unverified(
                            out,
                            rule,
                            component,
                            Some(role.as_str()),
                            format!("No {} declaration on this part.", rule.regime),
                        ),
                    }
                }
                "classification_report" if !rule.framework_list.is_empty() => {
                    if !role_matches(&rule.roles, &role) {
                        continue;
                    }
                    if let Some(entry) = framework_entry(&lists.framework, &compliance.mpn) {
                        push_hit(
                            out,
                            rule,
                            component,
                            Some(role.as_str()),
                            format!(
                                "Part number {} appears on list {} retrieved {} (row added {}).",
                                entry.mpn,
                                lists.framework.meta.id,
                                lists.framework.meta.retrieved,
                                entry.added
                            ),
                        );
                    }
                }
                "classification_report" if !rule.markets.is_empty() => {
                    if !role_matches(&rule.roles, &role) {
                        continue;
                    }
                    if project
                        .compliance
                        .target_markets
                        .iter()
                        .any(|m| rule.markets.iter().any(|rm| rm.eq_ignore_ascii_case(m)))
                    {
                        push_declared_ok(
                            out,
                            rule,
                            component,
                            Some(role.as_str()),
                            "EU RED and cybersecurity delegated act documentation items apply to radios; review product files.".into(),
                        );
                    }
                }
                "attribute_limit" => {
                    if !role_matches(&rule.roles, &role) {
                        continue;
                    }
                    if compliance.radio_bands_mhz.is_empty() {
                        push_unverified(
                            out,
                            rule,
                            component,
                            Some(role.as_str()),
                            "Radio centre frequency is not declared.".into(),
                        );
                        continue;
                    }
                    let markets = &project.compliance.target_markets;
                    for mhz in &compliance.radio_bands_mhz {
                        if !band_allowed(&lists.radio, markets, *mhz) {
                            push_hit(
                                out,
                                rule,
                                component,
                                Some(role.as_str()),
                                format!(
                                    "{mhz} MHz is outside allowed bands for markets {} per list {} retrieved {}.",
                                    markets.join(", "),
                                    lists.radio.meta.id,
                                    lists.radio.meta.retrieved
                                ),
                            );
                        } else if let Some(grant) = &compliance.radio_grant {
                            push_declared_ok(
                                out,
                                rule,
                                component,
                                Some(role.as_str()),
                                format!(
                                    "{mhz} MHz is within allowed bands; grant {:?} {} recorded as evidence.",
                                    grant.kind, grant.reference
                                ),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn status_label(status: &DeclarationStatus) -> &'static str {
    match status {
        DeclarationStatus::DeclaredCompliant => "declared_compliant",
        DeclarationStatus::DeclaredExempt => "declared_exempt",
        DeclarationStatus::DeclaredNoncompliant => "declared_noncompliant",
        DeclarationStatus::NotApplicable => "not_applicable",
        DeclarationStatus::Unknown => "unknown",
    }
}

fn push_hit(
    out: &mut Vec<ComplianceFinding>,
    rule: &ruleset::RuleDoc,
    component: &Component,
    role: Option<&str>,
    evidence: String,
) {
    out.push(ComplianceFinding {
        rule_id: rule.id.clone(),
        state: ComplianceState::Hit,
        severity: rule.severity_hit.clone(),
        message: rule.text.clone(),
        evidence,
        citation: rule.citation.clone(),
        objects: vec![component.id],
        role: role.map(str::to_string),
    });
}

fn push_unverified(
    out: &mut Vec<ComplianceFinding>,
    rule: &ruleset::RuleDoc,
    component: &Component,
    role: Option<&str>,
    evidence: String,
) {
    out.push(ComplianceFinding {
        rule_id: rule.id.clone(),
        state: ComplianceState::Unverified,
        severity: rule.severity_unverified.clone(),
        message: rule.text.clone(),
        evidence,
        citation: rule.citation.clone(),
        objects: vec![component.id],
        role: role.map(str::to_string),
    });
}

fn push_declared_ok(
    out: &mut Vec<ComplianceFinding>,
    rule: &ruleset::RuleDoc,
    component: &Component,
    role: Option<&str>,
    evidence: String,
) {
    out.push(ComplianceFinding {
        rule_id: rule.id.clone(),
        state: ComplianceState::DeclaredOk,
        severity: "info".into(),
        message: rule.text.clone(),
        evidence,
        citation: rule.citation.clone(),
        objects: vec![component.id],
        role: role.map(str::to_string),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{
        ComplianceDeclaration, ComplianceRole, ComplianceSettings, ComponentCompliance,
        CountryOfOrigin, DeclarationSource, DeclarationStatus, Project,
    };

    fn part(id: u64, reference: &str, compliance: ComponentCompliance) -> Component {
        Component {
            id,
            reference: reference.into(),
            value: reference.into(),
            kind: "ic".into(),
            position: Default::default(),
            rotation: 0.0,
            pins: vec![],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
            compliance: Some(compliance),
        }
    }

    #[test]
    fn a_newcomer_checks_a_drone_bill_of_materials_by_part_role() -> Result<(), String> {
        let mut project = Project::default();
        project.compliance = ComplianceSettings {
            rule_sets: vec![BLUE_UAS_STYLE_DRONE_RULESET_ID.into()],
            target_markets: vec!["EU".into()],
            ..Default::default()
        };
        project.components.push(part(
            1,
            "U1",
            ComponentCompliance {
                role: ComplianceRole::FlightController,
                country_of_origin: Some(CountryOfOrigin {
                    code: "ZZ".into(),
                    basis: "label".into(),
                    as_of: "2026-01-01".into(),
                }),
                maker_country: Some("US".into()),
                ..Default::default()
            },
        ));
        project.components.push(part(
            2,
            "U2",
            ComponentCompliance {
                role: ComplianceRole::RadioLink,
                radio_bands_mhz: vec![915.0],
                declarations: vec![ComplianceDeclaration {
                    regime: "ndaa_848".into(),
                    status: DeclarationStatus::DeclaredCompliant,
                    detail: "Manufacturer letter on file.".into(),
                    source: DeclarationSource {
                        label: "Example Radio Works".into(),
                        ..Default::default()
                    },
                    as_of: "2025-12-01".into(),
                }],
                ..Default::default()
            },
        ));
        project.components.push(part(
            3,
            "U3",
            ComponentCompliance {
                role: ComplianceRole::Camera,
                ..Default::default()
            },
        ));
        project.components.push(part(
            4,
            "U4",
            ComponentCompliance {
                role: ComplianceRole::Other,
                country_of_origin: Some(CountryOfOrigin {
                    code: "ZZ".into(),
                    basis: "label".into(),
                    as_of: "2026-01-01".into(),
                }),
                ..Default::default()
            },
        ));
        project.validate()?;
        let findings = compliance_findings(&project)?;
        let fc = findings
            .iter()
            .filter(|f| f.objects == [1])
            .collect::<Vec<_>>();
        assert!(
            fc.iter()
                .any(|f| f.rule_id == "ndaa848_origin" && f.state == ComplianceState::Hit),
            "flight controller origin hit"
        );
        let radio = findings
            .iter()
            .filter(|f| f.objects == [2])
            .collect::<Vec<_>>();
        assert!(
            radio.iter().any(
                |f| f.rule_id == "ndaa848_declaration"
                    && f.state == ComplianceState::DeclaredOk
            ),
            "radio declared NDAA row"
        );
        assert!(
            radio.iter().any(
                |f| f.rule_id == "radio_band_market" && f.state == ComplianceState::Hit
            ),
            "915 MHz EU band finding"
        );
        let camera = findings
            .iter()
            .filter(|f| f.objects == [3])
            .collect::<Vec<_>>();
        assert!(
            camera.iter().any(
                |f| f.rule_id == "ndaa848_origin" && f.state == ComplianceState::Unverified
            ),
            "camera unverified origin"
        );
        assert!(
            !findings.iter().any(|f| f.objects == [4] && f.rule_id == "ndaa848_origin"),
            "generic silicon/module without drone role skips origin under default interpretation"
        );
        Ok(())
    }
}
