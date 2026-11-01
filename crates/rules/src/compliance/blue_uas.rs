//! Blue UAS-style drone rule set: origin checks for drone roles against a
//! covered-country list, dated NDAA declarations, framework part numbers and
//! radio band limits per target market. Fixture list data only; findings are
//! evidence, never a listing or certification.
//!
//! # Dependencies
//! - crates/rules/src/compliance/lists.rs parses the three bundled lists.
//! - `crates/doc/src/compliance.rs` supplies the part data and settings.
use super::lists::{
    CoveredCountriesList, FrameworkList, RadioBandsList, band_allowed, country_covered,
    framework_entry, parse_covered_countries, parse_framework, parse_radio_bands,
};
use super::{ComplianceFinding, ComplianceRuleSpec, load_failure, load_rule_set};
use pcbcraft_doc::{Component, DeclarationStatus, OriginInterpretation, PartRole, Project};

pub const BLUE_UAS_STYLE_DRONE_RULE_SET: &str =
    include_str!("../../data/compliance/rulesets/blue_uas_style_drone.toml");
const COVERED_COUNTRIES: &str =
    include_str!("../../data/compliance/lists/synthetic_covered_origin_countries.toml");
const BLUE_UAS_FRAMEWORK: &str =
    include_str!("../../data/compliance/lists/synthetic_blue_uas_framework.toml");
const RADIO_BANDS: &str = include_str!("../../data/compliance/lists/synthetic_radio_bands.toml");

struct Lists {
    covered: CoveredCountriesList,
    framework: FrameworkList,
    radio: RadioBandsList,
}

fn bundled_lists() -> Result<Lists, String> {
    Ok(Lists {
        covered: parse_covered_countries(COVERED_COUNTRIES)?,
        framework: parse_framework(BLUE_UAS_FRAMEWORK)?,
        radio: parse_radio_bands(RADIO_BANDS)?,
    })
}

/// Findings of the Blue UAS-style rule set for the project's bill of materials.
pub fn evaluate(project: &Project) -> Vec<ComplianceFinding> {
    let set = match load_rule_set(BLUE_UAS_STYLE_DRONE_RULE_SET) {
        Ok(s) => s,
        Err(e) => {
            return vec![load_failure(
                "blue_uas_style_drone",
                "Blue UAS-style",
                "error",
                e,
            )];
        }
    };
    let lists = match bundled_lists() {
        Ok(l) => l,
        Err(e) => {
            return vec![load_failure(
                "blue_uas_style_drone",
                "Blue UAS-style list",
                "error",
                e,
            )];
        }
    };
    let interpretation = if set.interpretation.default == "silicon" {
        OriginInterpretation::Silicon
    } else {
        project.compliance_settings.origin_interpretation
    };
    let mut out = Vec::new();
    for component in project.components.iter().filter(|c| c.in_bom()) {
        let role = component.compliance.role.unwrap_or(PartRole::Other);
        for rule in &set.rules {
            match rule.kind.as_str() {
                "origin" => origin(
                    project,
                    component,
                    rule,
                    role,
                    interpretation,
                    &lists,
                    &mut out,
                ),
                "declaration_required" => declaration(component, rule, role, &mut out),
                "classification_report" if !rule.framework_list.is_empty() => {
                    framework(component, rule, role, &lists, &mut out);
                }
                "classification_report" if !rule.markets.is_empty() => {
                    if role_matches(rule, role)
                        && project
                            .compliance_settings
                            .target_markets
                            .iter()
                            .any(|m| rule.markets.iter().any(|rm| rm.eq_ignore_ascii_case(m)))
                    {
                        push(
                            &mut out,
                            rule,
                            component,
                            "declared_ok",
                            "EU RED and cybersecurity delegated act documentation items apply to radios; review product files.",
                        );
                    }
                }
                "attribute_limit" => radio_band(project, component, rule, role, &lists, &mut out),
                _ => {}
            }
        }
    }
    out
}

fn role_matches(rule: &ComplianceRuleSpec, role: PartRole) -> bool {
    rule.roles.is_empty()
        || rule
            .roles
            .iter()
            .any(|r| r.eq_ignore_ascii_case(role.token()))
}

fn maker_country(project: &Project, component: &Component) -> Option<String> {
    let name = component
        .field("Manufacturer")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| Some(component.sourcing.manufacturer.trim()).filter(|s| !s.is_empty()))?;
    let record = project.resolve_manufacturer(name)?;
    Some(record.country.clone()).filter(|c| !c.is_empty())
}

fn origin(
    project: &Project,
    component: &Component,
    rule: &ComplianceRuleSpec,
    role: PartRole,
    interpretation: OriginInterpretation,
    lists: &Lists,
    out: &mut Vec<ComplianceFinding>,
) {
    if !role_matches(rule, role) {
        return;
    }
    if interpretation == OriginInterpretation::Module && role == PartRole::Other {
        return;
    }
    let origin_code = component
        .compliance
        .country_of_origin
        .as_ref()
        .map(|c| c.code.as_str());
    let maker = maker_country(project, component);
    let list = &lists.covered.meta;
    let mut parts = Vec::new();
    if let Some(code) = origin_code {
        if country_covered(&lists.covered, code) {
            parts.push(format!(
                "Country of origin {code} is on list {} retrieved {}.",
                list.id, list.retrieved
            ));
        }
    }
    if let Some(code) = maker.as_deref() {
        if country_covered(&lists.covered, code) {
            parts.push(format!(
                "Maker country {code} is on list {} retrieved {}.",
                list.id, list.retrieved
            ));
        }
    }
    if interpretation == OriginInterpretation::Silicon {
        if let Some(code) = component.compliance.silicon_vendor_country.as_deref() {
            if country_covered(&lists.covered, code) {
                parts.push(format!(
                    "Silicon vendor country {code} is on list {} retrieved {}.",
                    list.id, list.retrieved
                ));
            }
        }
    }
    if !parts.is_empty() {
        push(out, rule, component, "hit", &parts.join(" "));
    } else if origin_code.is_none() && maker.is_none() {
        push(
            out,
            rule,
            component,
            "unverified",
            "Origin and maker country are not declared.",
        );
    } else {
        push(
            out,
            rule,
            component,
            "declared_ok",
            "Declared origin and maker countries are not on the covered list.",
        );
    }
}

fn declaration(
    component: &Component,
    rule: &ComplianceRuleSpec,
    role: PartRole,
    out: &mut Vec<ComplianceFinding>,
) {
    if !role_matches(rule, role) {
        return;
    }
    match super::declaration_of(component, &rule.regime) {
        Some(d) => match d.status {
            DeclarationStatus::DeclaredCompliant | DeclarationStatus::DeclaredExempt => push(
                out,
                rule,
                component,
                "declared_ok",
                &format!(
                    "Declaration {} ({:?}) dated {} from {} (statement, not certification).",
                    d.regime, d.status, d.as_of, d.source.label
                ),
            ),
            DeclarationStatus::DeclaredNoncompliant => push(
                out,
                rule,
                component,
                "hit",
                &format!(
                    "Declaration {} marked noncompliant dated {}.",
                    d.regime, d.as_of
                ),
            ),
            DeclarationStatus::NotApplicable => push(
                out,
                rule,
                component,
                "declared_ok",
                &format!("Declaration {} marked not applicable.", d.regime),
            ),
            DeclarationStatus::Unknown => push(
                out,
                rule,
                component,
                "unverified",
                &format!("Declaration {} status is unknown.", d.regime),
            ),
        },
        None => push(
            out,
            rule,
            component,
            "unverified",
            &format!("No {} declaration on this part.", rule.regime),
        ),
    }
}

fn framework(
    component: &Component,
    rule: &ComplianceRuleSpec,
    role: PartRole,
    lists: &Lists,
    out: &mut Vec<ComplianceFinding>,
) {
    if !role_matches(rule, role) {
        return;
    }
    let mpn = if component.sourcing.mpn.is_empty() {
        component.field("MPN").unwrap_or("")
    } else {
        component.sourcing.mpn.as_str()
    };
    if let Some(entry) = framework_entry(&lists.framework, mpn) {
        push(
            out,
            rule,
            component,
            "hit",
            &format!(
                "Part number {} appears on list {} retrieved {} (row added {}).",
                entry.mpn, lists.framework.meta.id, lists.framework.meta.retrieved, entry.added
            ),
        );
    }
}

fn radio_band(
    project: &Project,
    component: &Component,
    rule: &ComplianceRuleSpec,
    role: PartRole,
    lists: &Lists,
    out: &mut Vec<ComplianceFinding>,
) {
    if !role_matches(rule, role) {
        return;
    }
    let compliance = &component.compliance;
    if compliance.radio_bands_mhz.is_empty() {
        push(
            out,
            rule,
            component,
            "unverified",
            "Radio centre frequency is not declared.",
        );
        return;
    }
    let markets = &project.compliance_settings.target_markets;
    for mhz in &compliance.radio_bands_mhz {
        if !band_allowed(&lists.radio, markets, *mhz) {
            push(
                out,
                rule,
                component,
                "hit",
                &format!(
                    "{mhz} MHz is outside allowed bands for markets {} per list {} retrieved {}.",
                    markets.join(", "),
                    lists.radio.meta.id,
                    lists.radio.meta.retrieved
                ),
            );
        } else if let Some(grant) = &compliance.radio_grant {
            push(
                out,
                rule,
                component,
                "declared_ok",
                &format!(
                    "{mhz} MHz is within allowed bands; grant {:?} {} recorded as evidence.",
                    grant.kind, grant.reference
                ),
            );
        }
    }
}

fn push(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component: &Component,
    state: &str,
    evidence: &str,
) {
    let severity = match state {
        "hit" => rule.severity_hit.clone(),
        "declared_ok" => "info".into(),
        _ => rule.severity_unverified.clone(),
    };
    out.push(ComplianceFinding {
        code: rule.id.clone(),
        state: state.into(),
        severity,
        message: rule.text.clone(),
        rule: rule.id.clone(),
        evidence: evidence.to_string(),
        citation: rule.citation.clone(),
        objects: vec![component.id],
        manufacturer: component
            .field("Manufacturer")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    });
}
