//! Built-in interface rule profiles (TOML) and apply/list commands.
use pcbcraft_doc::{
    DiffPair, Id, NetClass, NetPairRule, Project, RuleArea, RuleForbid, RuleRegion,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

const MAX_PROFILE_ID: usize = 64;
const MAX_PATH: usize = 4096;

pub struct BuiltInProfile {
    pub id: &'static str,
    pub toml: &'static str,
}

macro_rules! builtin {
    ($id:literal, $file:literal) => {
        BuiltInProfile {
            id: $id,
            toml: include_str!(concat!("../profiles/", $file)),
        }
    };
}

pub static BUILT_INS: &[BuiltInProfile] = &[
    builtin!("usb2", "usb2.toml"),
    builtin!("i2c", "i2c.toml"),
    builtin!("spi", "spi.toml"),
    builtin!("uart", "uart.toml"),
    builtin!("can", "can.toml"),
    builtin!("rs485", "rs485.toml"),
    builtin!("swd", "swd.toml"),
    builtin!("jtag", "jtag.toml"),
    builtin!("hdmi", "hdmi.toml"),
    builtin!("ethernet-100", "ethernet-100.toml"),
    builtin!("ethernet-1g", "ethernet-1g.toml"),
    builtin!("pcie-gen2", "pcie-gen2.toml"),
    builtin!("pcie-gen3", "pcie-gen3.toml"),
    builtin!("lvds", "lvds.toml"),
    builtin!("mipi-dsi", "mipi-dsi.toml"),
    builtin!("mipi-csi", "mipi-csi.toml"),
    builtin!("sdio", "sdio.toml"),
    builtin!("qspi", "qspi.toml"),
    builtin!("ddr3", "ddr3.toml"),
    builtin!("ddr4", "ddr4.toml"),
    builtin!("lpddr4", "lpddr4.toml"),
    builtin!("power-3v3", "power-3v3.toml"),
    builtin!("power-5v", "power-5v.toml"),
    builtin!("power-12v", "power-12v.toml"),
    builtin!("analog-audio", "analog-audio.toml"),
    builtin!("rf-50ohm", "rf-50ohm.toml"),
];

pub fn manifest_slugs() -> &'static [&'static str] {
    &[
        "usb2",
        "i2c",
        "spi",
        "uart",
        "can",
        "rs485",
        "swd",
        "jtag",
        "hdmi",
        "ethernet-100",
        "ethernet-1g",
        "pcie-gen2",
        "pcie-gen3",
        "lvds",
        "mipi-dsi",
        "mipi-csi",
        "sdio",
        "qspi",
        "ddr3",
        "ddr4",
        "lpddr4",
        "power-3v3",
        "power-5v",
        "power-12v",
        "analog-audio",
        "rf-50ohm",
    ]
}

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
    ]
}

#[derive(Debug, Deserialize)]
struct ProfileMeta {
    id: String,
    name: String,
    category: String,
    summary: String,
}

#[derive(Debug, Deserialize, Default)]
struct ClassLimits {
    max_routed_length: Option<f64>,
    max_stub: Option<f64>,
    max_vias: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct NetClassToml {
    name: String,
    #[serde(default)]
    clearance: Option<f64>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    min_width: Option<f64>,
    #[serde(default)]
    max_width: Option<f64>,
    #[serde(default)]
    target_length: Option<f64>,
    #[serde(default)]
    length_tolerance: Option<f64>,
    #[serde(default)]
    max_length: Option<f64>,
    #[serde(default)]
    patterns: Vec<String>,
    #[serde(default)]
    nets: Vec<String>,
    #[serde(default)]
    limits: Option<ClassLimits>,
}

#[derive(Debug, Deserialize)]
struct PairToml {
    name: String,
    net_p: String,
    net_n: String,
    gap: f64,
    #[serde(default)]
    length_tolerance: Option<f64>,
    #[serde(default)]
    equal_vias: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PairRuleToml {
    net_a: String,
    net_b: String,
    clearance: f64,
}

#[derive(Debug, Deserialize)]
struct RuleAreaToml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    net: Option<String>,
    #[serde(default)]
    layer: Option<String>,
    #[serde(default)]
    forbid: Option<String>,
    #[serde(default)]
    footprint_ref: Option<String>,
    clearance: f64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Deserialize)]
struct KeepoutRowToml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    net: Option<String>,
    #[serde(default)]
    layer: Option<String>,
    forbid: String,
    #[serde(default)]
    footprint_ref: Option<String>,
    clearance: f64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Deserialize)]
struct ValueSourceToml {
    field: String,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProfileDocument {
    schema: u32,
    profile: ProfileMeta,
    #[serde(default)]
    net_class: Vec<NetClassToml>,
    #[serde(default)]
    pair: Vec<PairToml>,
    #[serde(default)]
    pair_rule: Vec<PairRuleToml>,
    #[serde(default)]
    rule_area: Vec<RuleAreaToml>,
    #[serde(default)]
    keepout: Vec<KeepoutRowToml>,
    #[serde(default)]
    placement: Vec<KeepoutRowToml>,
    #[serde(default)]
    value_source: Vec<ValueSourceToml>,
    #[serde(default)]
    length_group: Vec<toml::Value>,
    #[serde(default)]
    companion: Vec<toml::Value>,
}

fn parse_toml(text: &str) -> Result<ProfileDocument, String> {
    toml::from_str(text).map_err(|e| format!("TOML profile: {e}"))
}

fn validate_profile_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > MAX_PROFILE_ID {
        return Err("profile id must be 1 to 64 bytes".into());
    }
    Ok(())
}

fn collect_unsupported(doc: &ProfileDocument) -> Vec<String> {
    let mut out = Vec::new();
    if !doc.length_group.is_empty() {
        out.push("length_group".into());
    }
    if !doc.companion.is_empty() {
        out.push("companion".into());
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
    out.sort();
    out.dedup();
    out
}

fn net_id_by_name(p: &Project, name: &str) -> Option<Id> {
    p.nets.iter().find(|n| n.name == name).map(|n| n.id)
}

struct RuleAreaRow<'a> {
    name: Option<String>,
    net: Option<String>,
    layer: Option<String>,
    forbid: Option<&'a str>,
    footprint_ref: Option<String>,
    clearance: f64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn apply_rule_area_row(
    p: &mut Project,
    next: &mut Id,
    profile_id: &str,
    row: RuleAreaRow<'_>,
) -> Result<(), String> {
    let layer = row.layer.as_deref().map(parse_layer).transpose()?;
    let net = if let Some(name) = &row.net {
        Some(net_id_by_name(p, name).ok_or_else(|| format!("rule area needs net {name}"))?)
    } else {
        None
    };
    if !row.clearance.is_finite() || row.clearance <= 0.0 || row.clearance > 100.0 {
        return Err("invalid rule area clearance".into());
    }
    if row.width <= 0.0 || row.height <= 0.0 {
        return Err("invalid rule area size".into());
    }
    let forbid = if let Some(raw) = row.forbid {
        Some(RuleForbid::parse(raw)?)
    } else {
        None
    };
    let footprint_ref = row.footprint_ref;
    if footprint_ref
        .as_ref()
        .is_some_and(|r| r.is_empty() || r.len() > 64)
    {
        return Err("footprint reference must be 1 to 64 bytes".into());
    }
    let areas = &mut p.board.rule_areas;
    let name_key = row.name.as_deref().unwrap_or("");
    let region = RuleRegion {
        x: row.x,
        y: row.y,
        width: row.width,
        height: row.height,
    };
    if let Some(existing) = areas.iter_mut().find(|a| {
        a.profile.as_deref() == Some(profile_id) && a.name.as_deref().unwrap_or("") == name_key
    }) {
        existing.net = net;
        existing.layer = layer;
        existing.clearance = row.clearance;
        existing.region = region;
        existing.forbid = forbid;
        existing.footprint_ref = footprint_ref;
        existing.profile = Some(profile_id.to_string());
    } else {
        if areas.len() >= 64 {
            return Err("a board carries at most 64 rule areas".into());
        }
        let id = allocate(next)?;
        areas.push(RuleArea {
            id,
            name: row.name,
            net,
            layer,
            clearance: row.clearance,
            keepout: None,
            max_height: None,
            region,
            profile: Some(profile_id.to_string()),
            forbid,
            footprint_ref,
        });
    }
    Ok(())
}

pub fn class_pattern_matches(pattern: &str, name: &str) -> bool {
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

fn resolve_nets_for_class(
    p: &Project,
    class: &NetClassToml,
    nets_override: Option<&[String]>,
) -> (Vec<Id>, Vec<String>) {
    let mut ids = BTreeSet::new();
    let mut unmatched = Vec::new();
    let filter: BTreeSet<&str> = nets_override
        .map(|v| v.iter().map(String::as_str).collect())
        .unwrap_or_default();
    for name in &class.nets {
        if !filter.is_empty() && !filter.contains(name.as_str()) {
            continue;
        }
        if let Some(id) = net_id_by_name(p, name) {
            ids.insert(id);
        } else {
            unmatched.push(name.clone());
        }
    }
    for pattern in &class.patterns {
        for net in &p.nets {
            if !filter.is_empty() && !filter.contains(net.name.as_str()) {
                continue;
            }
            if class_pattern_matches(pattern, &net.name) {
                ids.insert(net.id);
            }
        }
    }
    (ids.into_iter().collect(), unmatched)
}

fn allocate(next: &mut Id) -> Result<Id, String> {
    let id = *next;
    if id == 0 || id > u32::MAX as u64 {
        return Err("object ID space exhausted".into());
    }
    *next = next.saturating_add(1);
    Ok(id)
}

fn find_or_create_class<'a>(
    p: &'a mut Project,
    next: &mut Id,
    profile_id: &str,
    spec: &NetClassToml,
) -> Result<&'a mut NetClass, String> {
    if spec.name.is_empty() || spec.name.len() > 128 {
        return Err("net class name must be 1 to 128 bytes".into());
    }
    if let Some(existing) = p
        .net_classes
        .iter()
        .position(|c| c.name == spec.name && c.profile.as_deref().is_some_and(|t| t == profile_id))
    {
        return Ok(&mut p.net_classes[existing]);
    }
    if p.net_classes.iter().any(|c| c.name == spec.name) {
        let idx = p
            .net_classes
            .iter()
            .position(|c| c.name == spec.name)
            .ok_or_else(|| "missing class".to_string())?;
        p.net_classes[idx].profile = Some(profile_id.to_string());
        return Ok(&mut p.net_classes[idx]);
    }
    if p.net_classes.len() >= 64 {
        return Err("net class limit reached (64)".into());
    }
    let clearance = spec.clearance.unwrap_or(0.15);
    let width = spec.width.unwrap_or(0.2);
    if !clearance.is_finite() || clearance <= 0.0 || clearance > 100.0 {
        return Err("invalid class clearance".into());
    }
    if !width.is_finite() || width <= 0.0 || width > 100.0 {
        return Err("invalid class width".into());
    }
    let id = allocate(next)?;
    p.net_classes.push(NetClass {
        id,
        name: spec.name.clone(),
        clearance,
        width,
        min_width: None,
        max_width: None,
        target_length: None,
        length_tolerance: None,
        max_stub: None,
        via_diameter: None,
        via_drill: None,
        pair_width: None,
        pair_gap: None,
        nets: Vec::new(),
        profile: Some(profile_id.to_string()),
    });
    p.net_classes
        .last_mut()
        .ok_or_else(|| "class push".to_string())
}

fn apply_class_fields(class: &mut NetClass, spec: &NetClassToml) {
    if let Some(c) = spec.clearance {
        class.clearance = c;
    }
    if let Some(w) = spec.width {
        class.width = w;
    }
    class.min_width = spec.min_width;
    class.max_width = spec.max_width;
    class.target_length = spec.target_length;
    class.length_tolerance = spec.length_tolerance;
}

fn assign_nets_to_class(p: &mut Project, class_id: Id, nets: &[Id]) -> Result<(), String> {
    for net in nets {
        if !p.nets.iter().any(|n| n.id == *net) {
            return Err("unknown net in class membership".into());
        }
        if let Some(existing) = p.net_class(*net)
            && existing.id != class_id
        {
            return Err(format!("net already belongs to class {}", existing.name));
        }
    }
    for class in p.net_classes.iter_mut() {
        class.nets.retain(|n| !nets.contains(n));
    }
    let class = p
        .net_classes
        .iter_mut()
        .find(|c| c.id == class_id)
        .ok_or_else(|| "missing class".to_string())?;
    for net in nets {
        if class.nets.len() >= 256 {
            return Err("a class holds at most 256 nets".into());
        }
        if !class.nets.contains(net) {
            class.nets.push(*net);
        }
    }
    Ok(())
}

pub fn profile_list() -> Value {
    let profiles: Vec<Value> = BUILT_INS
        .iter()
        .filter_map(|b| {
            let doc = parse_toml(b.toml).ok()?;
            Some(json!({
                "id": doc.profile.id,
                "name": doc.profile.name,
                "category": doc.profile.category,
                "summary": doc.profile.summary,
            }))
        })
        .collect();
    json!({"profiles": profiles})
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileApplyParams {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub nets_override: Vec<String>,
    #[serde(default)]
    pub dry_run: bool,
}

pub fn profile_apply(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, String> {
    let a: ProfileApplyParams = serde_json::from_value(params.clone())
        .map_err(|e| format!("profile apply parameters: {e}"))?;
    let has_id = a.id.as_ref().is_some_and(|s| !s.is_empty());
    let has_path = a.path.as_ref().is_some_and(|s| !s.is_empty());
    if has_id == has_path {
        return Err("give exactly one of id or path".into());
    }
    if a.nets_override.len() > 128 {
        return Err("nets_override lists at most 128 names".into());
    }
    for name in &a.nets_override {
        if name.is_empty() || name.len() > 128 {
            return Err("nets_override names must be 1 to 128 bytes".into());
        }
    }
    let (text, profile_id) = if let Some(id) = a.id {
        validate_profile_id(&id)?;
        let builtin = BUILT_INS
            .iter()
            .find(|b| b.id == id)
            .ok_or_else(|| format!("unknown built-in profile {id}"))?;
        (builtin.toml.to_string(), id)
    } else {
        let path = a.path.as_ref().ok_or_else(|| "missing path".to_string())?;
        if path.len() > MAX_PATH {
            return Err("profile path is too long".into());
        }
        let text =
            std::fs::read_to_string(Path::new(path)).map_err(|e| format!("read profile: {e}"))?;
        let doc = parse_toml(&text)?;
        validate_profile_id(&doc.profile.id)?;
        (text, doc.profile.id)
    };
    let doc = parse_toml(&text)?;
    if doc.schema != 1 {
        return Err(format!("unsupported profile schema {}", doc.schema));
    }
    if doc.profile.id != profile_id {
        return Err("profile id mismatch between parameter and file".into());
    }
    if doc.net_class.len() > 64 || doc.pair.len() > 64 || doc.rule_area.len() > 64 {
        return Err("a profile defines at most 64 classes, pairs and areas each".into());
    }
    let unsupported = collect_unsupported(&doc);
    let override_slice = if a.nets_override.is_empty() {
        None
    } else {
        Some(a.nets_override.as_slice())
    };
    let mut plan_classes = Vec::new();
    for class in &doc.net_class {
        let (nets, unmatched) = resolve_nets_for_class(p, class, override_slice);
        plan_classes.push(json!({
            "name": class.name,
            "nets": nets,
            "unmatched_patterns": unmatched,
        }));
    }
    let value_sources: Vec<Value> = doc
        .value_source
        .iter()
        .map(|v| {
            json!({
                "field": v.field,
                "source": v.source,
            })
        })
        .collect();
    let plan = json!({
        "profile": profile_id,
        "classes": plan_classes,
        "pairs": doc.pair.len(),
        "pair_rules": doc.pair_rule.len(),
        "rule_areas": doc.rule_area.len() + doc.keepout.len() + doc.placement.len(),
        "unsupported": unsupported,
        "value_source": value_sources,
    });
    if a.dry_run {
        return Ok(json!({"dry_run": true, "plan": plan}));
    }
    let snapshot = p.clone();
    let apply_result = (|| -> Result<Value, String> {
        for class in &doc.net_class {
            let (net_ids, _) = resolve_nets_for_class(p, class, override_slice);
            let handle = find_or_create_class(p, next, &profile_id, class)?;
            let class_id = handle.id;
            apply_class_fields(handle, class);
            handle.profile = Some(profile_id.clone());
            if !net_ids.is_empty() {
                assign_nets_to_class(p, class_id, &net_ids)?;
            }
        }
        for pair in &doc.pair {
            let net_p = net_id_by_name(p, &pair.net_p)
                .ok_or_else(|| format!("pair {} needs net {}", pair.name, pair.net_p))?;
            let net_n = net_id_by_name(p, &pair.net_n)
                .ok_or_else(|| format!("pair {} needs net {}", pair.name, pair.net_n))?;
            if !pair.gap.is_finite() || !(0.02..=10.0).contains(&pair.gap) {
                return Err("invalid pair gap".into());
            }
            let tolerance = pair.length_tolerance;
            if tolerance.is_some_and(|t| !t.is_finite() || !(0.0..=100.0).contains(&t)) {
                return Err("invalid pair length tolerance".into());
            }
            let pairs = &mut p.board.diff_pairs;
            if let Some(existing) = pairs.iter_mut().find(|d| {
                d.name == pair.name && d.profile.as_deref().is_some_and(|t| t == profile_id)
            }) {
                existing.net_p = net_p;
                existing.net_n = net_n;
                existing.gap = pair.gap;
                existing.length_tolerance = tolerance;
                existing.profile = Some(profile_id.clone());
            } else if let Some(existing) = pairs.iter_mut().find(|d| d.name == pair.name) {
                existing.net_p = net_p;
                existing.net_n = net_n;
                existing.gap = pair.gap;
                existing.length_tolerance = tolerance;
                existing.profile = Some(profile_id.clone());
            } else {
                if pairs.len() >= 64 {
                    return Err("a board carries at most 64 differential pairs".into());
                }
                let id = allocate(next)?;
                pairs.push(DiffPair {
                    id,
                    name: pair.name.clone(),
                    net_p,
                    net_n,
                    gap: pair.gap,
                    length_tolerance: tolerance,
                    profile: Some(profile_id.clone()),
                });
            }
        }
        for rule in &doc.pair_rule {
            let a_id = net_id_by_name(p, &rule.net_a)
                .ok_or_else(|| format!("pair rule needs net {}", rule.net_a))?;
            let b_id = net_id_by_name(p, &rule.net_b)
                .ok_or_else(|| format!("pair rule needs net {}", rule.net_b))?;
            if a_id == b_id {
                return Err("pair rule needs two different nets".into());
            }
            if !rule.clearance.is_finite() || rule.clearance <= 0.0 || rule.clearance > 100.0 {
                return Err("invalid pair rule clearance".into());
            }
            let key = (a_id.min(b_id), a_id.max(b_id));
            let rules = &mut p.board.pair_rules;
            if let Some(existing) = rules.iter_mut().find(|r| (r.a, r.b) == key) {
                existing.clearance = rule.clearance;
                existing.profile = Some(profile_id.clone());
            } else {
                if rules.len() >= 64 {
                    return Err("a board carries at most 64 net pair clearance rules".into());
                }
                rules.push(NetPairRule {
                    a: key.0,
                    b: key.1,
                    clearance: rule.clearance,
                    profile: Some(profile_id.clone()),
                });
                rules.sort_by_key(|r| (r.a, r.b));
            }
        }
        for area in &doc.rule_area {
            apply_rule_area_row(
                p,
                next,
                &profile_id,
                RuleAreaRow {
                    name: area.name.clone(),
                    net: area.net.clone(),
                    layer: area.layer.clone(),
                    forbid: area.forbid.as_deref(),
                    footprint_ref: area.footprint_ref.clone(),
                    clearance: area.clearance,
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: area.height,
                },
            )?;
        }
        for row in doc.keepout.iter().chain(doc.placement.iter()) {
            apply_rule_area_row(
                p,
                next,
                &profile_id,
                RuleAreaRow {
                    name: row.name.clone(),
                    net: row.net.clone(),
                    layer: row.layer.clone(),
                    forbid: Some(row.forbid.as_str()),
                    footprint_ref: row.footprint_ref.clone(),
                    clearance: row.clearance,
                    x: row.x,
                    y: row.y,
                    width: row.width,
                    height: row.height,
                },
            )?;
        }
        p.validate()?;
        Ok(json!({"applied": true, "plan": plan}))
    })();
    match apply_result {
        Ok(v) => Ok(v),
        Err(e) => {
            *p = snapshot;
            Err(e)
        }
    }
}

fn parse_layer(name: &str) -> Result<pcbcraft_doc::Layer, String> {
    match name {
        "F.Cu" => Ok(pcbcraft_doc::Layer::Front),
        "B.Cu" => Ok(pcbcraft_doc::Layer::Back),
        other => Err(format!("unsupported layer {other}")),
    }
}

pub fn parse_builtin(id: &str) -> Result<&'static str, String> {
    BUILT_INS
        .iter()
        .find(|b| b.id == id)
        .map(|b| b.toml)
        .ok_or_else(|| format!("unknown profile {id}"))
}
