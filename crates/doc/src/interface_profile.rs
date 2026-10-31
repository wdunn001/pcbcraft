//! Interface rule profiles: protocol bus templates, net classes, pairs, and companion parts.
//!
//! # Rules
//! - Pure data and bounds checks. TOML import and export live in crates/io/src/interface_profile.rs.
//! - Schema 1 keeps the phase-1 tables; schema 2 adds bus, signals, companions, and checks.
//! - Refusals name the field and stay within the limits in crates/engine/profiles/SCHEMA.md.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs
//! - crates/engine/profiles/SCHEMA.md
fn valid_slug(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_PROFILE_ID
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn bounded(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max
}

fn optional_bounded(s: &str, max: usize) -> bool {
    s.len() <= max
}

fn finite_in(v: f64, lo: f64, hi: f64) -> bool {
    v.is_finite() && v >= lo && v <= hi
}

fn electrical_class_ok(s: &str) -> bool {
    matches!(s, "single-ended" | "open-drain" | "differential")
}

fn reference_plane_ok(s: &str) -> bool {
    matches!(s, "required" | "recommended" | "optional")
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Library record header for a shipped profile file.
pub struct ProfileRecordMeta {
    /// Record kind.
    pub kind: String,
    /// License SPDX or name.
    pub license: String,
    /// Provenance note.
    pub source: String,
    /// Optional SHA-256 hex digest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Identity and summary for one profile.
pub struct ProfileMeta {
    /// Profile slug.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Grouping label.
    pub category: String,
    /// Short description.
    pub summary: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Electrical and layout limits for a protocol bus.
pub struct BusLimits {
    /// Edge-rate band label.

    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_rate_band: Option<String>,
    /// Target differential impedance in ohms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impedance_ohm: Option<f64>,
    /// Impedance tolerance percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impedance_tolerance_percent: Option<f64>,
    /// Maximum routed length in millimetres.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_routed_length_mm: Option<f64>,
    /// Maximum stub length in millimetres.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_stub_mm: Option<f64>,
    /// Skew budget in picoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skew_budget_ps: Option<f64>,
    /// Reference plane requirement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_plane: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Named bus with schematic type and optional limits.
pub struct BusTemplate {
    /// Display name.
    pub name: String,
    /// Schematic bus or port type name.
    pub schematic_type: String,
    /// Optional bus limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<BusLimits>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// One signal line with electrical class.
pub struct SignalSpec {
    /// Display name.
    pub name: String,
    /// Fixed net name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<String>,
    /// Glob pattern for net names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// single-ended, open-drain, or differential.
    pub electrical_class: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Optional routed-length limits on a net class.
pub struct ClassLimits {
    /// Maximum routed length.

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_routed_length: Option<f64>,
    /// Maximum stub length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_stub: Option<f64>,
    /// Maximum via count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_vias: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Net class defaults and membership patterns.
pub struct NetClassSpec {
    /// Display name.
    pub name: String,
    /// Clearance in millimetres.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clearance: Option<f64>,
    /// Track width in millimetres.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// Minimum track width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f64>,
    /// Maximum track width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f64>,
    /// Target routed length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_length: Option<f64>,
    /// Length match tolerance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_tolerance: Option<f64>,
    /// Maximum routed length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<f64>,
    /// Net name globs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<String>,
    /// Fixed net names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nets: Vec<String>,
    /// Optional bus limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<ClassLimits>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Controlled differential or single-ended pair.
pub struct PairSpec {
    /// Display name.
    pub name: String,
    /// Positive or first net.
    pub net_p: String,
    /// Negative or second net.
    pub net_n: String,
    /// Pair gap in millimetres.
    pub gap: f64,
    /// Length match tolerance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_tolerance: Option<f64>,
    /// Require equal via counts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equal_vias: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Clearance between two named nets.
pub struct PairRuleSpec {
    /// First net.
    pub net_a: String,
    /// Second net.
    pub net_b: String,
    /// Clearance in millimetres.
    pub clearance: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Rectangular clearance region on a layer.
pub struct RuleAreaSpec {
    /// Display name.

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Fixed net name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<String>,
    /// Layer name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// Clearance in millimetres.
    pub clearance: f64,
    /// Left edge in millimetres.
    pub x: f64,
    /// Bottom edge in millimetres.
    pub y: f64,
    /// Track width in millimetres.
    pub width: f64,
    /// Height in millimetres.
    pub height: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Provenance note for one applied numeric field.
pub struct ValueSourceSpec {
    /// Dotted field path.
    pub field: String,
    /// Provenance note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Required companion part with sourced value range.
pub struct CompanionSpec {
    /// Companion role name.
    pub role: String,
    /// Minimum value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_min: Option<f64>,
    /// Maximum value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_max: Option<f64>,
    /// Unit label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Maximum placement distance in millimetres.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement_distance_max_mm: Option<f64>,
    /// Provenance note.
    pub source: String,
    /// Fixed net names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nets: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Named design check with parameters.
pub struct CheckSpec {
    /// Display name.
    pub name: String,
    /// Check parameters.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Matched-length group (unsupported until routed).
pub struct LengthGroupSpec {
    /// Display name.
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// One interface profile file, schema 1 or 2.
pub struct InterfaceProfileDocument {
    /// Schema version, 1 or 2.
    pub schema: u32,
    /// Profile identity.
    pub profile: ProfileMeta,
    /// Optional library record header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<ProfileRecordMeta>,
    /// Optional protocol bus template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus: Option<BusTemplate>,
    /// Signal definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signal: Vec<SignalSpec>,
    /// Net class tables.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub net_class: Vec<NetClassSpec>,
    /// Pair definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pair: Vec<PairSpec>,
    /// Pair clearance rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pair_rule: Vec<PairRuleSpec>,
    /// Rule areas.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rule_area: Vec<RuleAreaSpec>,
    /// Value provenance rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub value_source: Vec<ValueSourceSpec>,
    /// Length groups.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub length_group: Vec<LengthGroupSpec>,
    /// Companion parts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companion: Vec<CompanionSpec>,
    /// Named checks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub check: Vec<CheckSpec>,
}

/// One interface profile stored on the project library.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
/// One interface profile stored on the project library.
pub struct InterfaceProfileEntry {
    /// Profile slug.
    pub id: String,
    /// Embedded profile TOML text.
    pub toml: String,
}

impl InterfaceProfileEntry {
    /// Checks id slug and embedded TOML size.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_slug(&self.id) {
            return Err(
                "interface profile id must be 1 to 64 lowercase letters, digits or hyphens".into(),
            );
        }
        if self.toml.is_empty() || self.toml.len() > MAX_EMBEDDED_TOML {
            return Err("interface profile toml must be 1 to 262144 bytes".into());
        }
        Ok(())
    }
}

impl InterfaceProfileDocument {
    /// Checks schema, bounds, and cross-field rules.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 && self.schema != 2 {
            return Err(format!(
                "profile schema must be 1 or 2, not {}",
                self.schema
            ));
        }
        if !valid_slug(&self.profile.id) {
            return Err("profile id must be 1 to 64 lowercase letters, digits or hyphens".into());
        }
        if !bounded(&self.profile.name, MAX_PROFILE_NAME) {
            return Err("profile name must be 1 to 128 bytes".into());
        }
        if !bounded(&self.profile.category, MAX_CATEGORY) {
            return Err("profile category must be 1 to 64 bytes".into());
        }
        if !bounded(&self.profile.summary, MAX_SUMMARY) {
            return Err("profile summary must be 1 to 512 bytes".into());
        }
        if let Some(record) = &self.record {
            if record.kind != INTERFACE_PROFILE_RECORD_KIND {
                return Err(format!(
                    "record kind must be {INTERFACE_PROFILE_RECORD_KIND}"
                ));
            }
            if !optional_bounded(&record.license, MAX_LICENSE) {
                return Err("record license must be at most 128 bytes".into());
            }
            if !bounded(&record.source, MAX_SOURCE_NOTE) {
                return Err("record source must be 1 to 512 bytes".into());
            }
            if let Some(hash) = &record.content_hash {
                if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("record content_hash must be 64 hex digits".into());
                }
            }
        }
        if self.schema == 2 {
            let bus = self
                .bus
                .as_ref()
                .ok_or_else(|| "schema 2 profile needs a bus table".to_string())?;
            if !bounded(&bus.name, MAX_BUS_NAME) {
                return Err("bus name must be 1 to 128 bytes".into());
            }
            if !bounded(&bus.schematic_type, MAX_SCHEMATIC_TYPE) {
                return Err("bus schematic_type must be 1 to 128 bytes".into());
            }
            if self.signal.is_empty() {
                return Err("schema 2 profile needs at least one signal".into());
            }
            validate_bus_limits(bus.limits.as_ref())?;
        }
        if self.signal.len() > MAX_SIGNALS {
            return Err(format!("a profile defines at most {MAX_SIGNALS} signals"));
        }
        for signal in &self.signal {
            if !bounded(&signal.name, 128) {
                return Err("signal name must be 1 to 128 bytes".into());
            }
            if signal.net.is_none() && signal.pattern.is_none() {
                return Err(format!("signal {} needs net or pattern", signal.name));
            }
            if signal.net.as_ref().is_some_and(|n| !bounded(n, 128)) {
                return Err(format!("signal {} net name out of range", signal.name));
            }
            if signal.pattern.as_ref().is_some_and(|p| !bounded(p, 128)) {
                return Err(format!("signal {} pattern out of range", signal.name));
            }
            if !electrical_class_ok(&signal.electrical_class) {
                return Err(format!(
                    "signal {} electrical_class must be single-ended, open-drain or differential",
                    signal.name
                ));
            }
        }
        if self.net_class.len() > MAX_NET_CLASSES {
            return Err(format!(
                "a profile defines at most {MAX_NET_CLASSES} net classes"
            ));
        }
        for class in &self.net_class {
            validate_net_class(class)?;
        }
        if self.pair.len() > MAX_PAIRS {
            return Err(format!("a profile defines at most {MAX_PAIRS} pairs"));
        }
        for pair in &self.pair {
            validate_pair(pair)?;
        }
        if self.pair_rule.len() > MAX_PAIRS {
            return Err(format!("a profile defines at most {MAX_PAIRS} pair rules"));
        }
        for rule in &self.pair_rule {
            if !bounded(&rule.net_a, 128) || !bounded(&rule.net_b, 128) {
                return Err("pair rule net names must be 1 to 128 bytes".into());
            }
            if !finite_in(rule.clearance, MIN_CLEARANCE, MAX_CLEARANCE) {
                return Err("pair_rule clearance out of range".into());
            }
        }
        if self.rule_area.len() > MAX_RULE_AREAS {
            return Err(format!(
                "a profile defines at most {MAX_RULE_AREAS} rule areas"
            ));
        }
        for area in &self.rule_area {
            if area.name.as_ref().is_some_and(|n| !bounded(n, 128)) {
                return Err("rule area name out of range".into());
            }
            if !finite_in(area.clearance, MIN_CLEARANCE, MAX_CLEARANCE) {
                return Err("rule_area clearance out of range".into());
            }
            if area.width <= 0.0
                || area.height <= 0.0
                || !area.width.is_finite()
                || !area.height.is_finite()
            {
                return Err("rule area size out of range".into());
            }
        }
        if self.companion.len() > MAX_COMPANIONS {
            return Err(format!(
                "a profile defines at most {MAX_COMPANIONS} companions"
            ));
        }
        for (idx, companion) in self.companion.iter().enumerate() {
            validate_companion(companion, idx)?;
        }
        if self.check.len() > MAX_CHECKS {
            return Err(format!("a profile defines at most {MAX_CHECKS} checks"));
        }
        for check in &self.check {
            if !bounded(&check.name, MAX_CHECK_NAME) {
                return Err("check name must be 1 to 64 bytes".into());
            }
            if check.params.len() > MAX_CHECK_PARAMS {
                return Err("check params holds at most 32 entries".into());
            }
            for key in check.params.keys() {
                if !bounded(key, MAX_CHECK_PARAM_KEY) {
                    return Err(format!("check param key {key} out of range"));
                }
            }
        }
        if self.value_source.len() > MAX_VALUE_SOURCES {
            return Err(format!(
                "a profile defines at most {MAX_VALUE_SOURCES} value sources"
            ));
        }
        for vs in &self.value_source {
            if !bounded(&vs.field, 128) {
                return Err("value_source field must be 1 to 128 bytes".into());
            }
            if vs
                .source
                .as_ref()
                .is_some_and(|s| !optional_bounded(s, MAX_SOURCE_NOTE))
            {
                return Err(format!(
                    "value_source field {} source note out of range",
                    vs.field
                ));
            }
        }
        Ok(())
    }
}

fn validate_bus_limits(limits: Option<&BusLimits>) -> Result<(), String> {
    let Some(limits) = limits else {
        return Ok(());
    };
    if limits
        .edge_rate_band
        .as_ref()
        .is_some_and(|s| !bounded(s, 64))
    {
        return Err("bus.limits edge_rate_band out of range".into());
    }
    if limits
        .impedance_ohm
        .is_some_and(|v| !finite_in(v, MIN_IMPEDANCE, MAX_IMPEDANCE))
    {
        return Err("bus.limits impedance_ohm out of range".into());
    }
    if limits
        .impedance_tolerance_percent
        .is_some_and(|v| !finite_in(v, MIN_PERCENT, MAX_PERCENT))
    {
        return Err("bus.limits impedance_tolerance_percent out of range".into());
    }
    if limits
        .max_routed_length_mm
        .is_some_and(|v| !finite_in(v, MIN_LENGTH_MM, MAX_LENGTH_MM))
    {
        return Err("bus.limits max_routed_length_mm out of range".into());
    }
    if limits
        .max_stub_mm
        .is_some_and(|v| !finite_in(v, MIN_LENGTH_MM, MAX_LENGTH_MM))
    {
        return Err("bus.limits max_stub_mm out of range".into());
    }
    if limits
        .skew_budget_ps
        .is_some_and(|v| !finite_in(v, MIN_SKEW_PS, MAX_SKEW_PS))
    {
        return Err("bus.limits skew_budget_ps out of range".into());
    }
    if limits
        .reference_plane
        .as_ref()
        .is_some_and(|s| !reference_plane_ok(s))
    {
        return Err("bus.limits reference_plane must be required, recommended or optional".into());
    }
    Ok(())
}

fn validate_net_class(class: &NetClassSpec) -> Result<(), String> {
    if !bounded(&class.name, 128) {
        return Err("net class name must be 1 to 128 bytes".into());
    }
    if class
        .clearance
        .is_some_and(|v| !finite_in(v, MIN_CLEARANCE, MAX_CLEARANCE))
    {
        return Err(format!("net_class.{}.clearance out of range", class.name));
    }
    if class
        .width
        .is_some_and(|v| !finite_in(v, MIN_WIDTH, MAX_WIDTH))
    {
        return Err(format!("net_class.{}.width out of range", class.name));
    }
    if class
        .min_width
        .is_some_and(|v| !finite_in(v, MIN_WIDTH, MAX_WIDTH))
    {
        return Err(format!("net_class.{}.min_width out of range", class.name));
    }
    if class
        .max_width
        .is_some_and(|v| !finite_in(v, MIN_WIDTH, MAX_WIDTH))
    {
        return Err(format!("net_class.{}.max_width out of range", class.name));
    }
    if class
        .max_length
        .is_some_and(|v| !finite_in(v, MIN_LENGTH_MM, MAX_LENGTH_MM))
    {
        return Err(format!("net_class.{}.max_length out of range", class.name));
    }
    if class.patterns.len() > 128 || class.nets.len() > 128 {
        return Err(format!(
            "net_class.{} lists at most 128 patterns and nets",
            class.name
        ));
    }
    for pattern in &class.patterns {
        if !bounded(pattern, 128) {
            return Err(format!("net_class.{}.pattern out of range", class.name));
        }
    }
    for net in &class.nets {
        if !bounded(net, 128) {
            return Err(format!("net_class.{}.net out of range", class.name));
        }
    }
    if let Some(limits) = &class.limits {
        if limits
            .max_routed_length
            .is_some_and(|v| !finite_in(v, MIN_LENGTH_MM, MAX_LENGTH_MM))
        {
            return Err(format!(
                "net_class.{}.limits.max_routed_length out of range",
                class.name
            ));
        }
        if limits
            .max_stub
            .is_some_and(|v| !finite_in(v, MIN_LENGTH_MM, MAX_LENGTH_MM))
        {
            return Err(format!(
                "net_class.{}.limits.max_stub out of range",
                class.name
            ));
        }
        if limits.max_vias.is_some_and(|v| v > 256) {
            return Err(format!(
                "net_class.{}.limits.max_vias out of range",
                class.name
            ));
        }
    }
    Ok(())
}

fn validate_pair(pair: &PairSpec) -> Result<(), String> {
    if !bounded(&pair.name, 128) {
        return Err("pair name must be 1 to 128 bytes".into());
    }
    if !bounded(&pair.net_p, 128) || !bounded(&pair.net_n, 128) {
        return Err(format!("pair {} net names out of range", pair.name));
    }
    if !finite_in(pair.gap, MIN_GAP, MAX_GAP) {
        return Err(format!("pair {} gap out of range", pair.name));
    }
    if pair
        .length_tolerance
        .is_some_and(|t| !finite_in(t, 0.0, 100.0))
    {
        return Err(format!("pair {} length_tolerance out of range", pair.name));
    }
    Ok(())
}

fn validate_companion(companion: &CompanionSpec, idx: usize) -> Result<(), String> {
    if !bounded(&companion.role, MAX_ROLE) {
        return Err(format!("companion[{idx}] role out of range"));
    }
    if !bounded(&companion.source, MAX_SOURCE_NOTE) {
        return Err(format!("companion[{idx}] source note out of range"));
    }
    if companion
        .value_min
        .is_some_and(|v| !v.is_finite() || v < 0.0 || v > 1e9)
    {
        return Err(format!("companion[{idx}] value_min out of range"));
    }
    if companion
        .value_max
        .is_some_and(|v| !v.is_finite() || v < 0.0 || v > 1e9)
    {
        return Err(format!("companion[{idx}] value_max out of range"));
    }
    if let (Some(min), Some(max)) = (companion.value_min, companion.value_max) {
        if min > max {
            return Err(format!("companion[{idx}] value_min exceeds value_max"));
        }
    }
    if companion
        .placement_distance_max_mm
        .is_some_and(|v| !finite_in(v, MIN_LENGTH_MM, MAX_LENGTH_MM))
    {
        return Err(format!(
            "companion[{idx}] placement_distance_max_mm out of range"
        ));
    }
    if companion
        .unit
        .as_ref()
        .is_some_and(|u| !optional_bounded(u, 16))
    {
        return Err(format!("companion[{idx}] unit out of range"));
    }
    if companion.nets.len() > 16 {
        return Err(format!("companion[{idx}] lists at most 16 nets"));
    }
    for net in &companion.nets {
        if !bounded(net, 128) {
            return Err(format!("companion[{idx}] net name out of range"));
        }
    }
    Ok(())
}

/// Fields the engine can apply today; everything else is reported under unsupported.
pub fn known_value_source_fields() -> &'static [&'static str] {
    &[
        "net_class.clearance",
        "net_class.width",
        "net_class.min_width",
        "net_class.max_width",
        "net_class.target_length",
        "net_class.length_tolerance",
        "pair.gap",
        "pair.length_tolerance",
        "pair_rule.clearance",
        "rule_area.clearance",
        "bus.limits.impedance_ohm",
        "bus.limits.impedance_tolerance_percent",
        "bus.limits.max_routed_length_mm",
        "bus.limits.max_stub_mm",
        "bus.limits.skew_budget_ps",
        "bus.limits.edge_rate_band",
        "bus.limits.reference_plane",
    ]
}

/// Lists profile fields the engine cannot enforce yet.
pub fn collect_unsupported(doc: &InterfaceProfileDocument) -> Vec<String> {
    let mut out = Vec::new();
    if !doc.length_group.is_empty() {
        out.push("length_group".into());
    }
    if !doc.companion.is_empty() {
        out.push("companion".into());
    }
    if !doc.check.is_empty() {
        out.push("check".into());
    }
    if doc.schema == 2 {
        if doc.bus.is_some() {
            out.push("bus".into());
        }
        for signal in &doc.signal {
            out.push(format!("signal.{}", signal.name));
        }
    }
    for class in &doc.net_class {
        if class.max_length.is_some() {
            out.push(format!("net_class.{}.max_length", class.name));
        }
        if let Some(limits) = &class.limits {
            if limits.max_routed_length.is_some() {
                out.push(format!("net_class.{}.limits.max_routed_length", class.name));
            }
            if limits.max_stub.is_some() {
                out.push(format!("net_class.{}.limits.max_stub", class.name));
            }
            if limits.max_vias.is_some() {
                out.push(format!("net_class.{}.limits.max_vias", class.name));
            }
        }
    }
    for pair in &doc.pair {
        if pair.equal_vias.is_some() {
            out.push(format!("pair.{}.equal_vias", pair.name));
        }
    }
    if let Some(bus) = &doc.bus {
        if let Some(limits) = &bus.limits {
            if limits.edge_rate_band.is_some() {
                out.push("bus.limits.edge_rate_band".into());
            }
            if limits.impedance_ohm.is_some() {
                out.push("bus.limits.impedance_ohm".into());
            }
            if limits.impedance_tolerance_percent.is_some() {
                out.push("bus.limits.impedance_tolerance_percent".into());
            }
            if limits.max_routed_length_mm.is_some() {
                out.push("bus.limits.max_routed_length_mm".into());
            }
            if limits.max_stub_mm.is_some() {
                out.push("bus.limits.max_stub_mm".into());
            }
            if limits.skew_budget_ps.is_some() {
                out.push("bus.limits.skew_budget_ps".into());
            }
            if limits.reference_plane.is_some() {
                out.push("bus.limits.reference_plane".into());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Returns companion role and source pairs for preview and apply plans.
pub fn companion_plan(doc: &InterfaceProfileDocument) -> Vec<(String, String)> {
    doc.companion
        .iter()
        .map(|c| (c.role.clone(), c.source.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_v2_requires_bus_and_signals() {
        let mut doc = InterfaceProfileDocument {
            schema: 2,
            profile: ProfileMeta {
                id: "t".into(),
                name: "T".into(),
                category: "bus".into(),
                summary: "test".into(),
            },
            ..Default::default()
        };
        assert!(doc.validate().is_err());
        doc.bus = Some(BusTemplate {
            name: "T bus".into(),
            schematic_type: "T".into(),
            limits: None,
        });
        assert!(doc.validate().is_err());
        doc.signal.push(SignalSpec {
            name: "CLK".into(),
            net: Some("CLK".into()),
            pattern: None,
            electrical_class: "single-ended".into(),
        });
        assert!(doc.validate().is_ok());
    }
}
