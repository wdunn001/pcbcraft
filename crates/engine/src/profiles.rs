//! Interface rule profiles: parse TOML, plan, and apply net classes and pairs.

use crate::Error;
use pcbcraft_doc::{DiffPair, Id, Layer, NetClass, NetPairRule, Project, RuleArea, RuleRegion};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use toml::Value as Toml;

pub const SCHEMA_KEYS: &str = include_str!("../profiles/schema-keys.txt");
pub const SOURCE_SLUGS: &str = include_str!("../profiles/source-slugs.txt");

struct BuiltIn {
    id: &'static str,
    text: &'static str,
}

const BUILT_INS: &[BuiltIn] = &[
    BuiltIn {
        id: "power-rail-current",
        text: include_str!("../profiles/power-rail-current.toml"),
    },
    BuiltIn {
        id: "decoupling",
        text: include_str!("../profiles/decoupling.toml"),
    },
    BuiltIn {
        id: "ldo",
        text: include_str!("../profiles/ldo.toml"),
    },
    BuiltIn {
        id: "buck",
        text: include_str!("../profiles/buck.toml"),
    },
    BuiltIn {
        id: "power-entry",
        text: include_str!("../profiles/power-entry.toml"),
    },
    BuiltIn {
        id: "usb2",
        text: include_str!("../profiles/usb2.toml"),
    },
    BuiltIn {
        id: "usb3",
        text: include_str!("../profiles/usb3.toml"),
    },
    BuiltIn {
        id: "usbc-cc",
        text: include_str!("../profiles/usbc-cc.toml"),
    },
    BuiltIn {
        id: "i2c",
        text: include_str!("../profiles/i2c.toml"),
    },
    BuiltIn {
        id: "spi-qspi",
        text: include_str!("../profiles/spi-qspi.toml"),
    },
    BuiltIn {
        id: "uart",
        text: include_str!("../profiles/uart.toml"),
    },
    BuiltIn {
        id: "can",
        text: include_str!("../profiles/can.toml"),
    },
    BuiltIn {
        id: "rs485",
        text: include_str!("../profiles/rs485.toml"),
    },
    BuiltIn {
        id: "ethernet",
        text: include_str!("../profiles/ethernet.toml"),
    },
    BuiltIn {
        id: "hdmi-dp",
        text: include_str!("../profiles/hdmi-dp.toml"),
    },
    BuiltIn {
        id: "mipi",
        text: include_str!("../profiles/mipi.toml"),
    },
    BuiltIn {
        id: "lvds",
        text: include_str!("../profiles/lvds.toml"),
    },
    BuiltIn {
        id: "pcie",
        text: include_str!("../profiles/pcie.toml"),
    },
    BuiltIn {
        id: "ddr3",
        text: include_str!("../profiles/ddr3.toml"),
    },
    BuiltIn {
        id: "sd-emmc",
        text: include_str!("../profiles/sd-emmc.toml"),
    },
    BuiltIn {
        id: "swd-jtag",
        text: include_str!("../profiles/swd-jtag.toml"),
    },
    BuiltIn {
        id: "crystal",
        text: include_str!("../profiles/crystal.toml"),
    },
    BuiltIn {
        id: "rf-antenna",
        text: include_str!("../profiles/rf-antenna.toml"),
    },
    BuiltIn {
        id: "esp32",
        text: include_str!("../profiles/esp32.toml"),
    },
    BuiltIn {
        id: "rp2040",
        text: include_str!("../profiles/rp2040.toml"),
    },
    BuiltIn {
        id: "component-kinds",
        text: include_str!("../profiles/component-kinds.toml"),
    },
];

#[derive(Clone, Debug, Deserialize)]
struct ProfileMeta {
    id: String,
    name: String,
    category: String,
    summary: String,
}

#[derive(Clone, Debug, Deserialize)]
struct NetClassSpec {
    name: String,
    clearance: f64,
    width: f64,
    #[serde(default)]
    min_width: Option<f64>,
    #[serde(default)]
    max_width: Option<f64>,
    #[serde(default)]
    target_length: Option<f64>,
    #[serde(default)]
    length_tolerance: Option<f64>,
    #[serde(default)]
    patterns: Vec<String>,
    #[serde(default)]
    nets: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PairSpec {
    name: String,
    net_p: String,
    net_n: String,
    gap: f64,
    #[serde(default)]
    length_tolerance: Option<f64>,
    #[serde(default)]
    equal_vias: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
struct PairRuleSpec {
    net_a: String,
    net_b: String,
    clearance: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct RuleAreaSpec {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    net: Option<String>,
    #[serde(default)]
    layer: Option<String>,
    clearance: f64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct ValueSourceRow {
    field: String,
    source: String,
    #[serde(default)]
    confidence: Option<String>,
}

#[derive(Clone, Debug)]
pub struct UnsupportedField {
    pub path: String,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub struct ParsedProfile {
    pub meta: ProfileMeta,
    pub classes: Vec<NetClassSpec>,
    pub pairs: Vec<PairSpec>,
    pub pair_rules: Vec<PairRuleSpec>,
    pub areas: Vec<RuleAreaSpec>,
    pub unsupported: Vec<UnsupportedField>,
    pub value_sources: Vec<ValueSourceRow>,
}

#[derive(Clone, Debug)]
pub struct ProfilePlan {
    pub profile_id: String,
    pub classes: Vec<PlannedClass>,
    pub pairs: Vec<PlannedPair>,
    pub pair_rules: Vec<PlannedPairRule>,
    pub areas: Vec<PlannedArea>,
    pub unsupported: Vec<UnsupportedField>,
    pub value_sources: Vec<ValueSourceRow>,
    pub unmatched_patterns: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct PlannedClass {
    pub name: String,
    pub clearance: f64,
    pub width: f64,
    pub min_width: Option<f64>,
    pub max_width: Option<f64>,
    pub target_length: Option<f64>,
    pub length_tolerance: Option<f64>,
    pub net_ids: Vec<Id>,
    pub net_names: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct PlannedPair {
    pub name: String,
    pub net_p: Id,
    pub net_n: Id,
    pub net_p_name: String,
    pub net_n_name: String,
    pub gap: f64,
    pub length_tolerance: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct PlannedPairRule {
    pub net_a: Id,
    pub net_b: Id,
    pub clearance: f64,
}

#[derive(Clone, Debug)]
pub struct PlannedArea {
    pub name: Option<String>,
    pub net: Option<Id>,
    pub layer: Option<Layer>,
    pub clearance: f64,
    pub region: RuleRegion,
}

pub fn list_built_ins() -> Value {
    let profiles: Vec<Value> = BUILT_INS
        .iter()
        .filter_map(|entry| {
            let parsed = parse_profile_text(entry.text, entry.id).ok()?;
            Some(json!({
                "id": parsed.meta.id,
                "name": parsed.meta.name,
                "category": parsed.meta.category,
                "summary": parsed.meta.summary,
            }))
        })
        .collect();
    json!({"profiles": profiles})
}

pub fn load_profile_source(
    id: Option<String>,
    path: Option<String>,
) -> Result<(String, String), Error> {
    if id.is_some() == path.is_some() {
        return Err(Error::Invalid(
            "give exactly one of id or path for the profile".into(),
        ));
    }
    if let Some(id) = id {
        if id.is_empty() || id.len() > 64 {
            return Err(Error::Invalid(
                "profile id must be 1 to 64 bytes".into(),
            ));
        }
        return Ok((built_in_text(&id)?.to_string(), id));
    }
    let path = path.ok_or_else(|| Error::Invalid("profile path missing".into()))?;
    if path.is_empty() || path.len() > 1024 {
        return Err(Error::Invalid(
            "profile path must be 1 to 1024 bytes".into(),
        ));
    }
    let bytes = pcbcraft_io::read_text(std::path::Path::new(&path)).map_err(Error::Invalid)?;
    Ok((bytes, path))
}

pub fn validate_nets_override(names: Option<&[String]>) -> Result<(), Error> {
    let Some(names) = names else {
        return Ok(());
    };
    if names.len() > 256 {
        return Err(Error::Invalid(
            "nets_override holds at most 256 names".into(),
        ));
    }
    for name in names {
        if name.is_empty() || name.len() > 128 {
            return Err(Error::Invalid(
                "each nets_override name must be 1 to 128 bytes".into(),
            ));
        }
    }
    Ok(())
}

pub fn built_in_text(id: &str) -> Result<&'static str, Error> {
    BUILT_INS
        .iter()
        .find(|b| b.id == id)
        .map(|b| b.text)
        .ok_or_else(|| Error::Invalid(format!("unknown built-in profile {id}")))
}

pub fn parse_profile_text(text: &str, label: &str) -> Result<ParsedProfile, Error> {
    let root: Toml = text.parse().map_err(|e| format!("{label}: {e}"))?;
    let schema = root
        .get("schema")
        .and_then(Toml::as_integer)
        .ok_or_else(|| format!("{label}: schema must be 1"))?;
    if schema != 1 {
        return Err(Error::Invalid(format!("{label}: schema must be 1")));
    }
    let meta: ProfileMeta = root
        .get("profile")
        .ok_or_else(|| format!("{label}: missing [profile]"))?
        .clone()
        .try_into()
        .map_err(|e: toml::de::Error| format!("{label}: {e}"))?;
    if meta.id.is_empty() || meta.id.len() > 64 {
        return Err(Error::Invalid(format!(
            "{label}: profile id must be 1 to 64 bytes"
        )));
    }
    let classes = load_table_array::<NetClassSpec>(&root, "net_class", label)?;
    let pairs = load_table_array::<PairSpec>(&root, "pair", label)?;
    let pair_rules = load_table_array::<PairRuleSpec>(&root, "pair_rule", label)?;
    let areas = load_table_array::<RuleAreaSpec>(&root, "rule_area", label)?;
    let value_sources = load_table_array::<ValueSourceRow>(&root, "value_source", label)?;
    if classes.len() > 64 || pairs.len() > 64 || areas.len() > 64 {
        return Err(Error::Invalid(format!(
            "{label}: at most 64 classes, pairs and areas per profile"
        )));
    }
    let pattern_count: usize = classes
        .iter()
        .map(|c| c.patterns.len() + c.nets.len())
        .sum();
    if pattern_count > 128 {
        return Err(Error::Invalid(format!(
            "{label}: at most 128 net patterns per profile"
        )));
    }
    let mut unsupported = collect_unsupported(&root);
    unsupported.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(ParsedProfile {
        meta,
        classes,
        pairs,
        pair_rules,
        areas,
        unsupported,
        value_sources,
    })
}

fn load_table_array<T: for<'de> Deserialize<'de>>(
    root: &Toml,
    key: &str,
    label: &str,
) -> Result<Vec<T>, Error> {
    let Some(value) = root.get(key) else {
        return Ok(vec![]);
    };
    match value {
        Toml::Array(rows) => rows
            .iter()
            .map(|row| {
                row.clone()
                    .try_into()
                    .map_err(|e: toml::de::Error| Error::Invalid(format!("{label}: {e}")))
            })
            .collect(),
        Toml::Table(_) => value
            .clone()
            .try_into()
            .map(|one: T| vec![one])
            .map_err(|e: toml::de::Error| Error::Invalid(format!("{label}: {e}"))),
        _ => Err(Error::Invalid(format!("{label}: {key} must be a table or array"))),
    }
}

fn collect_unsupported(root: &Toml) -> Vec<UnsupportedField> {
    let mut out = Vec::new();
    if let Some(limits) = root.get("limits").and_then(Toml::as_table) {
        for key in ["max_routed_length", "max_stub", "max_vias"] {
            if let Some(v) = limits.get(key) {
                out.push(UnsupportedField {
                    path: format!("limits.{key}"),
                    value: toml_to_json(v),
                });
            }
        }
    }
    push_table_array(root, "length_group", &mut out);
    push_table_array(root, "companion", &mut out);
    push_table_array(root, "role", &mut out);
    push_table_array(root, "termination", &mut out);
    push_table_array(root, "placement", &mut out);
    push_table_array(root, "variant", &mut out);
    if let Some(class) = root.get("class") {
        out.push(UnsupportedField {
            path: "class".into(),
            value: toml_to_json(class),
        });
    }
    let pair_rows: Vec<&Toml> = match root.get("pair") {
        Some(Toml::Array(a)) => a.iter().collect(),
        Some(Toml::Table(_)) => root.get("pair").into_iter().collect(),
        _ => vec![],
    };
    for pair in pair_rows {
        if let Some(table) = pair.as_table() {
            if table.contains_key("equal_vias") {
                if let Some(v) = pair.get("equal_vias") {
                    out.push(UnsupportedField {
                        path: "pair.equal_vias".into(),
                        value: toml_to_json(v),
                    });
                }
            }
            if table.contains_key("impedance") {
                if let Some(v) = pair.get("impedance") {
                    out.push(UnsupportedField {
                        path: "pair.impedance".into(),
                        value: toml_to_json(v),
                    });
                }
            }
        }
    }
    for row in classes_with_max_fields(root) {
        out.push(row);
    }
    out
}

fn classes_with_max_fields(root: &Toml) -> Vec<UnsupportedField> {
    let mut out = Vec::new();
    let Some(rows) = root.get("net_class").and_then(Toml::as_array) else {
        return out;
    };
    for row in rows {
        let Some(table) = row.as_table() else {
            continue;
        };
        for key in ["max_length", "max_stub", "max_vias"] {
            if let Some(v) = table.get(key) {
                out.push(UnsupportedField {
                    path: format!("net_class.{key}"),
                    value: toml_to_json(v),
                });
            }
        }
    }
    out
}

fn push_table_array(root: &Toml, key: &str, out: &mut Vec<UnsupportedField>) {
    let Some(rows) = root.get(key).and_then(Toml::as_array) else {
        return;
    };
    for (index, row) in rows.iter().enumerate() {
        out.push(UnsupportedField {
            path: format!("{key}[{index}]"),
            value: toml_to_json(row),
        });
    }
}

fn toml_to_json(v: &Toml) -> Value {
    match v {
        Toml::String(s) => json!(s),
        Toml::Integer(i) => json!(i),
        Toml::Float(f) => json!(f),
        Toml::Boolean(b) => json!(b),
        Toml::Array(a) => json!(a.iter().map(toml_to_json).collect::<Vec<_>>()),
        Toml::Table(t) => {
            let map: HashMap<String, Value> = t
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_json(v)))
                .collect();
            json!(map)
        }
        Toml::Datetime(d) => json!(d.to_string()),
    }
}

fn class_pattern_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let mut p = 0usize;
    let mut n = 0usize;
    let mut star: Option<usize> = None;
    let mut star_name = 0usize;
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            star_name = n;
            p += 1;
        } else if let Some(sp) = star {
            p = sp + 1;
            star_name += 1;
            n = star_name;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

fn net_allowed(name: &str, override_names: Option<&BTreeSet<String>>) -> bool {
    override_names.is_none_or(|set| set.contains(name))
}

fn resolve_net(p: &Project, name: &str) -> Result<Id, Error> {
    p.nets
        .iter()
        .find(|n| n.name == name)
        .map(|n| n.id)
        .ok_or_else(|| Error::Invalid(format!("no net named {name}")))
}

pub fn plan_profile(
    p: &Project,
    parsed: &ParsedProfile,
    nets_override: Option<&[String]>,
) -> Result<ProfilePlan, Error> {
    let override_set = nets_override.map(|names| names.iter().cloned().collect());
    let mut unmatched_patterns = Vec::new();
    let mut planned_classes = Vec::new();
    for class in &parsed.classes {
        if class.name.is_empty() || class.name.len() > 128 {
            return Err(Error::Invalid("net class name must be 1 to 128 bytes".into()));
        }
        let mut net_ids = Vec::new();
        let mut net_names = Vec::new();
        let mut seen = BTreeSet::new();
        for name in &class.nets {
            if !net_allowed(name, override_set.as_ref()) {
                continue;
            }
            if p.nets.iter().any(|n| n.name == *name) {
                let id = resolve_net(p, name)?;
                if seen.insert(id) {
                    net_ids.push(id);
                    net_names.push(name.clone());
                }
            } else {
                unmatched_patterns.push(name.clone());
            }
        }
        for pattern in &class.patterns {
            if pattern.is_empty() || pattern.len() > 64 {
                return Err(Error::Invalid("pattern must be 1 to 64 bytes".into()));
            }
            let matched: Vec<_> = p
                .nets
                .iter()
                .filter(|n| {
                    net_allowed(&n.name, override_set.as_ref())
                        && class_pattern_matches(pattern, &n.name)
                })
                .collect();
            if matched.is_empty() {
                unmatched_patterns.push(pattern.clone());
                continue;
            }
            for net in matched {
                if seen.insert(net.id) {
                    net_ids.push(net.id);
                    net_names.push(net.name.clone());
                }
            }
        }
        planned_classes.push(PlannedClass {
            name: class.name.clone(),
            clearance: class.clearance,
            width: class.width,
            min_width: class.min_width,
            max_width: class.max_width,
            target_length: class.target_length,
            length_tolerance: class.length_tolerance,
            net_ids,
            net_names,
        });
    }
    let mut planned_pairs = Vec::new();
    for pair in &parsed.pairs {
        let net_p = resolve_net(p, &pair.net_p)?;
        let net_n = resolve_net(p, &pair.net_n)?;
        planned_pairs.push(PlannedPair {
            name: pair.name.clone(),
            net_p,
            net_n,
            net_p_name: pair.net_p.clone(),
            net_n_name: pair.net_n.clone(),
            gap: pair.gap,
            length_tolerance: pair.length_tolerance,
        });
    }
    let mut planned_rules = Vec::new();
    for rule in &parsed.pair_rules {
        let net_a = resolve_net(p, &rule.net_a)?;
        let net_b = resolve_net(p, &rule.net_b)?;
        planned_rules.push(PlannedPairRule {
            net_a,
            net_b,
            clearance: rule.clearance,
        });
    }
    let mut planned_areas = Vec::new();
    for area in &parsed.areas {
        let layer = area
            .layer
            .as_deref()
            .map(parse_layer)
            .transpose()?;
        let net = area
            .net
            .as_deref()
            .map(|n| resolve_net(p, n))
            .transpose()?;
        planned_areas.push(PlannedArea {
            name: area.name.clone(),
            net,
            layer,
            clearance: area.clearance,
            region: RuleRegion {
                x: area.x,
                y: area.y,
                width: area.width,
                height: area.height,
            },
        });
    }
    if p.net_classes.len() + planned_classes.len() > 64 {
        return Err(Error::Invalid("net class limit reached (64)".into()));
    }
    Ok(ProfilePlan {
        profile_id: parsed.meta.id.clone(),
        classes: planned_classes,
        pairs: planned_pairs,
        pair_rules: planned_rules,
        areas: planned_areas,
        unsupported: parsed.unsupported.clone(),
        value_sources: parsed.value_sources.clone(),
        unmatched_patterns,
    })
}

fn parse_layer(raw: &str) -> Result<Layer, Error> {
    match raw {
        "F.Cu" => Ok(Layer::Front),
        "B.Cu" => Ok(Layer::Back),
        _ => Err(Error::Invalid(format!("unknown layer {raw}"))),
    }
}

pub fn plan_to_json(plan: &ProfilePlan) -> Value {
    json!({
        "profile_id": plan.profile_id,
        "classes": plan.classes.iter().map(|c| json!({
            "name": c.name,
            "clearance": c.clearance,
            "width": c.width,
            "min_width": c.min_width,
            "max_width": c.max_width,
            "target_length": c.target_length,
            "length_tolerance": c.length_tolerance,
            "nets": c.net_names,
        })).collect::<Vec<_>>(),
        "pairs": plan.pairs.iter().map(|p| json!({
            "name": p.name,
            "net_p": p.net_p_name,
            "net_n": p.net_n_name,
            "gap": p.gap,
            "length_tolerance": p.length_tolerance,
        })).collect::<Vec<_>>(),
        "pair_rules": plan.pair_rules.iter().map(|r| json!({
            "net_a": r.net_a,
            "net_b": r.net_b,
            "clearance": r.clearance,
        })).collect::<Vec<_>>(),
        "areas": plan.areas.len(),
        "unsupported": plan.unsupported.iter().map(|u| json!({"path": u.path, "value": u.value})).collect::<Vec<_>>(),
        "value_sources": plan.value_sources.iter().map(|v| json!({
            "field": v.field,
            "source": v.source,
            "confidence": v.confidence,
        })).collect::<Vec<_>>(),
        "unmatched_patterns": plan.unmatched_patterns,
    })
}

pub fn apply_plan(p: &mut Project, next: &mut Id, plan: &ProfilePlan) -> Result<(), Error> {
    let tag = plan.profile_id.clone();
    for class in &plan.classes {
        apply_class(p, next, class, &tag)?;
    }
    for pair in &plan.pairs {
        apply_pair(p, next, pair, &tag)?;
    }
    for rule in &plan.pair_rules {
        apply_pair_rule(p, rule, &tag)?;
    }
    for area in &plan.areas {
        apply_area(p, next, area, &tag)?;
    }
    Ok(())
}

fn allocate(next: &mut Id) -> Result<Id, Error> {
    if *next == 0 || *next > u32::MAX as u64 {
        return Err(Error::Invalid("object id space exhausted".into()));
    }
    let id = *next;
    *next = next.saturating_add(1);
    Ok(id)
}

fn find_class<'a>(p: &'a Project, profile: &str, name: &str) -> Option<&'a NetClass> {
    p.net_classes.iter().find(|c| {
        c.profile.as_deref() == Some(profile) && c.name == name
    })
}

fn apply_class(
    p: &mut Project,
    next: &mut Id,
    planned: &PlannedClass,
    profile: &str,
) -> Result<(), Error> {
    if !planned.clearance.is_finite()
        || planned.clearance <= 0.0
        || planned.clearance > 100.0
        || !planned.width.is_finite()
        || planned.width <= 0.0
        || planned.width > 100.0
    {
        return Err(Error::Invalid("invalid class clearance or width".into()));
    }
    let class_id = if let Some(existing) = find_class(p, profile, &planned.name) {
        existing.id
    } else if p.net_classes.iter().any(|c| c.name == planned.name) {
        return Err(Error::Invalid(format!(
            "net class {} already exists outside this profile",
            planned.name
        )));
    } else {
        let id = allocate(next)?;
        p.net_classes.push(NetClass {
            id,
            name: planned.name.clone(),
            clearance: planned.clearance,
            width: planned.width,
            min_width: None,
            max_width: None,
            target_length: None,
            length_tolerance: None,
            nets: vec![],
            profile: Some(profile.to_string()),
        });
        id
    };
    {
        let class = p
            .net_classes
            .iter_mut()
            .find(|c| c.id == class_id)
            .ok_or_else(|| Error::Invalid("missing class".into()))?;
        class.clearance = planned.clearance;
        class.width = planned.width;
        class.min_width = planned.min_width;
        class.max_width = planned.max_width;
        class.target_length = planned.target_length;
        class.length_tolerance = planned.length_tolerance;
        class.profile = Some(profile.to_string());
    }
    for net in &planned.net_ids {
        for class in p.net_classes.iter_mut() {
            class.nets.retain(|n| n != net);
        }
    }
    let class = p
        .net_classes
        .iter_mut()
        .find(|c| c.id == class_id)
        .ok_or_else(|| Error::Invalid("missing class".into()))?;
    for net in &planned.net_ids {
        if !class.nets.contains(net) {
            class.nets.push(*net);
        }
    }
    Ok(())
}

fn apply_pair(p: &mut Project, next: &mut Id, planned: &PlannedPair, profile: &str) -> Result<(), Error> {
    if !planned.gap.is_finite() || !(0.02..=10.0).contains(&planned.gap) {
        return Err(Error::Invalid(
            "the coupling gap must be 0.02 to 10 millimetres".into(),
        ));
    }
    if planned
        .length_tolerance
        .is_some_and(|t| !t.is_finite() || !(0.0..=100.0).contains(&t))
    {
        return Err(Error::Invalid(
            "the length tolerance must be 0 to 100 millimetres".into(),
        ));
    }
    if let Some(existing) = p
        .board
        .diff_pairs
        .iter()
        .find(|d| d.profile.as_deref() == Some(profile) && d.name == planned.name)
    {
        let id = existing.id;
        let pair = p
            .board
            .diff_pairs
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| Error::Invalid("missing pair".into()))?;
        pair.net_p = planned.net_p;
        pair.net_n = planned.net_n;
        pair.gap = planned.gap;
        pair.length_tolerance = planned.length_tolerance;
        pair.profile = Some(profile.to_string());
        return Ok(());
    }
    if p.board.diff_pairs.iter().any(|d| d.name == planned.name) {
        return Err(Error::Invalid(format!(
            "differential pair {} already exists outside this profile",
            planned.name
        )));
    }
    if p.board.diff_pairs.len() >= 64 {
        return Err(Error::Invalid(
            "a board carries at most 64 differential pairs".into(),
        ));
    }
    let id = allocate(next)?;
    p.board.diff_pairs.push(DiffPair {
        id,
        name: planned.name.clone(),
        net_p: planned.net_p,
        net_n: planned.net_n,
        gap: planned.gap,
        length_tolerance: planned.length_tolerance,
        profile: Some(profile.to_string()),
    });
    Ok(())
}

fn apply_pair_rule(p: &mut Project, planned: &PlannedPairRule, profile: &str) -> Result<(), Error> {
    if !planned.clearance.is_finite() || planned.clearance <= 0.0 || planned.clearance > 100.0 {
        return Err(Error::Invalid(
            "clearance must be between 0 and 100 millimetres".into(),
        ));
    }
    let key = (planned.net_a.min(planned.net_b), planned.net_a.max(planned.net_b));
    if let Some(existing) = p
        .board
        .pair_rules
        .iter_mut()
        .find(|r| (r.a.min(r.b), r.a.max(r.b)) == key)
    {
        existing.clearance = planned.clearance;
        existing.profile = Some(profile.to_string());
        return Ok(());
    }
    if p.board.pair_rules.len() >= 64 {
        return Err(Error::Invalid(
            "a board carries at most 64 net pair clearance rules".into(),
        ));
    }
    p.board.pair_rules.push(NetPairRule {
        a: key.0,
        b: key.1,
        clearance: planned.clearance,
        profile: Some(profile.to_string()),
    });
    p.board.pair_rules.sort_by_key(|r| (r.a, r.b));
    Ok(())
}

fn apply_area(p: &mut Project, next: &mut Id, planned: &PlannedArea, profile: &str) -> Result<(), Error> {
    if p.board.rule_areas.len() >= 64 {
        return Err(Error::Invalid(
            "a board carries at most 64 rule areas".into(),
        ));
    }
    let id = allocate(next)?;
    p.board.rule_areas.push(RuleArea {
        id,
        name: planned.name.clone(),
        net: planned.net,
        layer: planned.layer,
        clearance: planned.clearance,
        max_height: None,
        region: planned.region.clone(),
        profile: Some(profile.to_string()),
    });
    Ok(())
}

pub fn every_built_in_parses() -> Result<(), Error> {
    for entry in BUILT_INS {
        parse_profile_text(entry.text, entry.id)?;
    }
    Ok(())
}

pub fn for_each_built_in(
    mut visit: impl FnMut(&str, &ParsedProfile) -> Result<(), Error>,
) -> Result<(), Error> {
    for entry in BUILT_INS {
        let parsed = parse_profile_text(entry.text, entry.id)?;
        visit(entry.id, &parsed)?;
    }
    Ok(())
}
