//! Typed compliance fields on parts and project settings for regulatory checks.
use serde::{Deserialize, Serialize};

/// Part role for drone and radio rule targeting.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceRole {
    FlightController,
    Esc,
    RadioLink,
    Camera,
    Gimbal,
    Gnss,
    Battery,
    Motor,
    CompanionComputer,
    #[default]
    Other,
}

impl ComplianceRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FlightController => "flight_controller",
            Self::Esc => "esc",
            Self::RadioLink => "radio_link",
            Self::Camera => "camera",
            Self::Gimbal => "gimbal",
            Self::Gnss => "gnss",
            Self::Battery => "battery",
            Self::Motor => "motor",
            Self::CompanionComputer => "companion_computer",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "flight_controller" => Self::FlightController,
            "esc" => Self::Esc,
            "radio_link" => Self::RadioLink,
            "camera" => Self::Camera,
            "gimbal" => Self::Gimbal,
            "gnss" => Self::Gnss,
            "battery" => Self::Battery,
            "motor" => Self::Motor,
            "companion_computer" => Self::CompanionComputer,
            _ => Self::Other,
        }
    }
}

/// ISO 3166-1 alpha-2 country claim with basis and date.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct CountryOfOrigin {
    pub code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub basis: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub as_of: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationStatus {
    DeclaredCompliant,
    DeclaredExempt,
    DeclaredNoncompliant,
    NotApplicable,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationSourceKind {
    Manufacturer,
    Distributor,
    Imported,
    #[default]
    User,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct DeclarationSource {
    pub kind: DeclarationSourceKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_ref: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ComplianceDeclaration {
    pub regime: String,
    pub status: DeclarationStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    #[serde(default)]
    pub source: DeclarationSource,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub as_of: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct DeclaredSubstance {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cas: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RadioGrantKind {
    FccId,
    CeRed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct RadioGrant {
    pub kind: RadioGrantKind,
    pub reference: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ComponentCompliance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country_of_origin: Option<CountryOfOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maker_country: Option<String>,
    #[serde(default)]
    pub role: ComplianceRole,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manufacturer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mpn: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declarations: Vec<ComplianceDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub substances: Vec<DeclaredSubstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub radio_bands_mhz: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radio_max_power_dbm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radio_grant: Option<RadioGrant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silicon_vendor_country: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ManufacturerRecord {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OriginInterpretation {
    #[default]
    Module,
    Silicon,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ComplianceSettings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rule_sets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_markets: Vec<String>,
    #[serde(default)]
    pub origin_interpretation: OriginInterpretation,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub board_assembler_country: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manufacturers: Vec<ManufacturerRecord>,
}

impl ComplianceSettings {
    pub fn is_empty(&self) -> bool {
        self.rule_sets.is_empty()
            && self.target_markets.is_empty()
            && self.origin_interpretation == OriginInterpretation::Module
            && self.board_assembler_country.is_empty()
            && self.manufacturers.is_empty()
    }
}

impl ComponentCompliance {
    pub fn declaration_for(&self, regime: &str) -> Option<&ComplianceDeclaration> {
        self.declarations
            .iter()
            .find(|d| d.regime.eq_ignore_ascii_case(regime))
    }
}

pub const MAX_DECLARATIONS: usize = 64;
pub const MAX_DECLARATION_TEXT: usize = 256;
pub const MAX_DECLARATION_LINK: usize = 512;
pub const MAX_SUBSTANCES: usize = 64;
pub const MAX_MANUFACTURERS: usize = 256;

pub fn validate_component_compliance(c: &ComponentCompliance) -> Result<(), String> {
    if c.declarations.len() > MAX_DECLARATIONS {
        return Err(format!(
            "a part carries at most {MAX_DECLARATIONS} compliance declarations"
        ));
    }
    for d in &c.declarations {
        if d.regime.len() > 64 {
            return Err("a declaration regime name is too long".into());
        }
        if d.detail.len() > MAX_DECLARATION_TEXT {
            return Err("a declaration detail is too long".into());
        }
        if d.source.label.len() > MAX_DECLARATION_TEXT {
            return Err("a declaration source label is too long".into());
        }
        if d.source
            .url
            .as_ref()
            .is_some_and(|u| u.len() > MAX_DECLARATION_LINK)
        {
            return Err("a declaration source url is too long".into());
        }
    }
    if c.substances.len() > MAX_SUBSTANCES {
        return Err(format!(
            "a part carries at most {MAX_SUBSTANCES} declared substances"
        ));
    }
    if c.country_of_origin.as_ref().is_some_and(|o| o.code.len() != 2) {
        return Err("country of origin must be a two-letter ISO code".into());
    }
    Ok(())
}

pub fn validate_compliance_settings(s: &ComplianceSettings) -> Result<(), String> {
    if s.rule_sets.len() > 32 {
        return Err("a project enables at most 32 compliance rule sets".into());
    }
    if s.target_markets.len() > 16 {
        return Err("a project lists at most 16 target markets".into());
    }
    if s.manufacturers.len() > MAX_MANUFACTURERS {
        return Err(format!(
            "a project carries at most {MAX_MANUFACTURERS} manufacturer records"
        ));
    }
    Ok(())
}
