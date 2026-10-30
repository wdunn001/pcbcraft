//! Dated compliance claims on parts and project-level compliance settings.
use crate::Id;
use serde::{Deserialize, Serialize};

pub const MAX_DECLARATIONS: usize = 64;
pub const MAX_SUBSTANCES: usize = 64;
pub const MAX_COMPLIANCE_TEXT: usize = 256;
pub const MAX_COMPLIANCE_LINK: usize = 512;

/// One regulatory declaration on a part.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ComplianceDeclaration {
    pub regime: String,
    pub status: ComplianceStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    pub source: ComplianceSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceStatus {
    DeclaredCompliant,
    DeclaredExempt,
    DeclaredNoncompliant,
    NotApplicable,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ComplianceSource {
    pub kind: ComplianceSourceKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_ref: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceSourceKind {
    Manufacturer,
    Distributor,
    User,
    Imported,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeclaredSubstance {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cas: Option<String>,
    pub weight_percent: f64,
}

/// Compliance record on one schematic component.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Compliance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country_of_origin: Option<CountryOfOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declarations: Vec<ComplianceDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub substances: Vec<DeclaredSubstance>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CountryOfOrigin {
    pub code: String,
    pub basis: String,
    pub as_of: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ComplianceProjectSettings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rule_sets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_markets: Vec<String>,
    /// RoHS exemption number applied at board level when leaded finish is chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_rohs_exemption: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ManufacturerRecord {
    pub id: Id,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

pub fn valid_compliance_text(text: &str) -> bool {
    text.len() <= MAX_COMPLIANCE_TEXT
}

pub fn valid_compliance_link(text: &str) -> bool {
    text.len() <= MAX_COMPLIANCE_LINK
}

impl ComplianceDeclaration {
    pub fn valid(&self) -> bool {
        !self.regime.is_empty()
            && self.regime.len() <= MAX_COMPLIANCE_TEXT
            && valid_compliance_text(&self.detail)
            && self.source.valid()
            && self.as_of.as_ref().is_none_or(|d| valid_date(d))
    }
}

impl ComplianceSource {
    pub fn valid(&self) -> bool {
        valid_compliance_text(&self.label)
            && self.url.as_ref().is_none_or(|u| valid_compliance_link(u))
            && self
                .document_ref
                .as_ref()
                .is_none_or(|r| valid_compliance_text(r))
    }
}

impl DeclaredSubstance {
    pub fn valid(&self) -> bool {
        !self.name.is_empty()
            && valid_compliance_text(&self.name)
            && self.weight_percent.is_finite()
            && (0.0..=100.0).contains(&self.weight_percent)
            && self.cas.as_ref().is_none_or(|c| valid_compliance_text(c))
    }
}

impl Compliance {
    pub fn valid(&self) -> bool {
        self.declarations.len() <= MAX_DECLARATIONS
            && self.substances.len() <= MAX_SUBSTANCES
            && self.declarations.iter().all(ComplianceDeclaration::valid)
            && self.substances.iter().all(DeclaredSubstance::valid)
            && self
                .country_of_origin
                .as_ref()
                .is_none_or(|c| valid_compliance_text(&c.code) && valid_date(&c.as_of))
    }

    pub fn declaration_for(&self, regime: &str) -> Option<&ComplianceDeclaration> {
        self.declarations
            .iter()
            .find(|d| d.regime == regime)
    }
}

pub fn valid_date(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(|c| c.is_ascii_digit())
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[8..10].iter().all(|c| c.is_ascii_digit())
}

pub fn date_le(a: &str, b: &str) -> bool {
    valid_date(a) && valid_date(b) && a <= b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Project;

    #[test]
    fn compliance_records_survive_a_save_and_reload() {
        let mut p = Project::default();
        p.compliance_by_component.insert(
            1,
            Compliance {
                role: Some("other".into()),
                declarations: vec![ComplianceDeclaration {
                    regime: "eu.rohs".into(),
                    status: ComplianceStatus::DeclaredCompliant,
                    detail: String::new(),
                    source: ComplianceSource {
                        kind: ComplianceSourceKind::User,
                        label: "lab".into(),
                        url: None,
                        document_ref: None,
                    },
                    as_of: Some("2026-03-28".into()),
                }],
                ..Compliance::default()
            },
        );
        let json = serde_json::to_string(&p).expect("serialize");
        let back: Project = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.compliance_by_component.len(), 1);
        assert!(back.compliance_by_component.get(&1).is_some_and(Compliance::valid));
    }

    #[test]
    fn compliance_bounds_refuse_oversized_declarations() {
        let mut decls = Vec::new();
        for i in 0..=MAX_DECLARATIONS {
            decls.push(ComplianceDeclaration {
                regime: format!("r{i}"),
                status: ComplianceStatus::Unknown,
                detail: String::new(),
                source: ComplianceSource {
                    kind: ComplianceSourceKind::User,
                    label: String::new(),
                    url: None,
                    document_ref: None,
                },
                as_of: None,
            });
        }
        let c = Compliance {
            declarations: decls,
            ..Compliance::default()
        };
        assert!(!c.valid());
    }
}
