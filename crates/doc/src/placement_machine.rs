//! Pick and place planning: which nozzle, feeder, speed and heights each surface-mount
//! part gets on a `pick_place` machine profile, from rules, with per-part overrides.
//!
//! Context: a pick and place machine cares which nozzle picks which part. The profile
//! holds the nozzle set and a table of rules (package, size, body height to nozzle);
//! a part may override any value through typed component properties named
//! "Placement nozzle", "Placement feeder", "Placement speed", "Placement pick height",
//! "Placement place height" and "Placement skip".
//!
//! # Rules
//! - Pure planning from a `Project` and a `PickPlaceSection`; no file or UI code.
//! - An override beats the rule; a rule beats the profile default.
//! - Through-hole parts are never planned.
//! - Problems are plain sentences naming the part or the machine limit.
//!
//! # Dependencies
//! - crates/doc/src/machine_profile.rs (`PickPlaceSection`, `PlacementRuleRow`)
//! - crates/doc/src/lib.rs (`Project`, `Component`, `Footprint`)
//!
//! # Do not
//! - Write a file here; crates/io/src/neoden_yy1.rs writes the machine job.
//! - Guess a nozzle that the profile does not list.

use crate::{
    Component, Footprint, Id, Layer, PickPlaceSection, PlacementRuleRow, Project, Variant,
};
use std::collections::{BTreeMap, BTreeSet};

/// Property name of the nozzle override.
pub const FIELD_NOZZLE: &str = "Placement nozzle";
/// Property name of the feeder override.
pub const FIELD_FEEDER: &str = "Placement feeder";
/// Property name of the speed override, in percent.
pub const FIELD_SPEED: &str = "Placement speed";
/// Property name of the pick height override, in millimetres.
pub const FIELD_PICK_HEIGHT: &str = "Placement pick height";
/// Property name of the place height override, in millimetres.
pub const FIELD_PLACE_HEIGHT: &str = "Placement place height";
/// Property name of the skip override.
pub const FIELD_SKIP: &str = "Placement skip";

/// Machine speed used when neither an override, a rule nor the profile says one.
pub const FALLBACK_SPEED_PCT: u32 = 75;

/// Per-part placement overrides, typed. Every value is optional; none means
/// "use the rule".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartPlacement {
    /// Nozzle id.
    pub nozzle: Option<String>,
    /// Feeder number.
    pub feeder: Option<u32>,
    /// Speed in percent, 1 to 100.
    pub speed_pct: Option<u32>,
    /// Pick height offset in millimetres.
    pub pick_height_mm: Option<f64>,
    /// Place height offset in millimetres.
    pub place_height_mm: Option<f64>,
    /// Leave the part out of the run.
    pub skip: Option<bool>,
}

fn field_text<'a>(c: &'a Component, key: &str) -> Option<&'a str> {
    c.field(key).map(str::trim).filter(|v| !v.is_empty())
}

fn parse_height(key: &str, v: &str) -> Result<f64, String> {
    let n: f64 = v
        .trim_end_matches("mm")
        .trim()
        .parse()
        .map_err(|_| format!("{key} must be a number of millimetres"))?;
    if !n.is_finite() || n.abs() > 50.0 {
        return Err(format!("{key} must be within 50 mm either way"));
    }
    Ok(n)
}

impl PartPlacement {
    /// Read the overrides from a part's properties.
    ///
    /// # Rules
    /// - A blank property counts as not set.
    /// - A value out of range is an error naming the property.
    pub fn from_component(c: &Component) -> Result<Self, String> {
        let mut out = PartPlacement::default();
        if let Some(v) = field_text(c, FIELD_NOZZLE) {
            if v.len() > 64 {
                return Err(format!("{FIELD_NOZZLE} is too long"));
            }
            out.nozzle = Some(v.to_string());
        }
        if let Some(v) = field_text(c, FIELD_FEEDER) {
            let n: u32 = v
                .parse()
                .map_err(|_| format!("{FIELD_FEEDER} must be a whole number"))?;
            if !(1..=999).contains(&n) {
                return Err(format!("{FIELD_FEEDER} must be between 1 and 999"));
            }
            out.feeder = Some(n);
        }
        if let Some(v) = field_text(c, FIELD_SPEED) {
            let n: u32 = v
                .trim_end_matches('%')
                .trim()
                .parse()
                .map_err(|_| format!("{FIELD_SPEED} must be a whole number of percent"))?;
            if !(1..=100).contains(&n) {
                return Err(format!("{FIELD_SPEED} must be between 1 and 100"));
            }
            out.speed_pct = Some(n);
        }
        if let Some(v) = field_text(c, FIELD_PICK_HEIGHT) {
            out.pick_height_mm = Some(parse_height(FIELD_PICK_HEIGHT, v)?);
        }
        if let Some(v) = field_text(c, FIELD_PLACE_HEIGHT) {
            out.place_height_mm = Some(parse_height(FIELD_PLACE_HEIGHT, v)?);
        }
        if let Some(v) = field_text(c, FIELD_SKIP) {
            out.skip = Some(matches!(
                v.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "y" | "skip"
            ));
        }
        Ok(out)
    }

    /// True when no value is set.
    pub fn is_empty(&self) -> bool {
        *self == PartPlacement::default()
    }
}

/// What a part looks like to the rules.
#[derive(Clone, Debug, PartialEq)]
pub struct PartFacts {
    /// Short package name after the profile's aliases.
    pub package: String,
    /// Largest pad dimension in millimetres (the footprint's span across its pads).
    pub size_mm: f64,
    /// Body height in millimetres when the part states it (property "height").
    pub height_mm: Option<f64>,
}

/// Apply the profile's first matching alias row. A pattern `^abc.*` matches any
/// name that starts with `abc`; `^abc$` or plain `abc` match that name exactly.
pub fn alias_package(pp: &PickPlaceSection, name: &str) -> String {
    for row in &pp.footprint_alias {
        let pat = row.pattern.strip_prefix('^').unwrap_or(&row.pattern);
        let hit = if let Some(prefix) = pat.strip_suffix(".*") {
            name.starts_with(prefix)
        } else {
            name == pat.strip_suffix('$').unwrap_or(pat)
        };
        if hit {
            return row.replacement.clone();
        }
    }
    name.to_string()
}

/// Strip what would break a comma separated field.
pub fn clean_cell(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, ',' | '\r' | '\n' | '"') {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn is_through_hole(f: &Footprint) -> bool {
    f.pads.iter().any(|pad| {
        pad.geometry
            .as_ref()
            .is_some_and(|g| g.pad_type == "thru_hole" || g.pad_type == "np_thru_hole")
    })
}

fn pad_span_mm(f: &Footprint) -> f64 {
    let mut lo = (f64::MAX, f64::MAX);
    let mut hi = (f64::MIN, f64::MIN);
    for pad in &f.pads {
        let (at, half) = match &pad.geometry {
            Some(g) => (g.position, (g.size.x / 2.0, g.size.y / 2.0)),
            None => (pad.offset, (pad.diameter / 2.0, pad.diameter / 2.0)),
        };
        lo = (lo.0.min(at.x - half.0), lo.1.min(at.y - half.1));
        hi = (hi.0.max(at.x + half.0), hi.1.max(at.y + half.1));
    }
    if lo.0 > hi.0 {
        return 0.0;
    }
    (hi.0 - lo.0).max(hi.1 - lo.1)
}

fn body_height(c: &Component) -> Option<f64> {
    c.fields
        .iter()
        .find(|f| f.key.eq_ignore_ascii_case("height") || f.key.eq_ignore_ascii_case("body height"))
        .and_then(|f| {
            f.value
                .trim()
                .trim_end_matches("mm")
                .trim()
                .parse::<f64>()
                .ok()
        })
        .filter(|h| h.is_finite() && *h >= 0.0)
}

/// The package name a part is placed with: a variant swap, the library
/// footprint name, or the part kind.
pub fn package_name(c: &Component, variant: Option<&Variant>) -> String {
    variant
        .and_then(|v| v.footprints.iter().find(|o| o.component == c.id))
        .map(|o| o.footprint.clone())
        .unwrap_or_else(|| match &c.library_footprint {
            Some(fp) => fp.definition.name.clone(),
            None => c.kind.clone(),
        })
}

/// What the rules see of a placed part.
pub fn part_facts(
    c: &Component,
    f: &Footprint,
    pp: &PickPlaceSection,
    variant: Option<&Variant>,
) -> PartFacts {
    PartFacts {
        package: clean_cell(&alias_package(pp, &package_name(c, variant))),
        size_mm: pad_span_mm(f),
        height_mm: body_height(c),
    }
}

fn rule_matches(r: &PlacementRuleRow, facts: &PartFacts) -> bool {
    if !r.package.is_empty() && !facts.package.starts_with(&r.package) {
        return false;
    }
    if r.min_size_mm > 0.0 && facts.size_mm < r.min_size_mm {
        return false;
    }
    if r.max_size_mm > 0.0 && facts.size_mm > r.max_size_mm {
        return false;
    }
    if r.min_height_mm > 0.0 && facts.height_mm.is_none_or(|h| h < r.min_height_mm) {
        return false;
    }
    if r.max_height_mm > 0.0 && facts.height_mm.is_some_and(|h| h > r.max_height_mm) {
        return false;
    }
    true
}

/// Index of the first rule that fits the part.
pub fn matching_rule(pp: &PickPlaceSection, facts: &PartFacts) -> Option<usize> {
    pp.placement_rule
        .iter()
        .position(|r| rule_matches(r, facts))
}

/// The values a part gets, and where the nozzle came from.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedPlacement {
    /// Chosen nozzle id, if any rule or override names one.
    pub nozzle: Option<String>,
    /// True when the nozzle is the part's own override.
    pub nozzle_overridden: bool,
    /// Index of the rule that matched, if any.
    pub rule: Option<usize>,
    /// Speed in percent.
    pub speed_pct: u32,
    /// Pick height offset in millimetres.
    pub pick_height_mm: f64,
    /// Place height offset in millimetres.
    pub place_height_mm: f64,
    /// True when the matching rule leaves the part out by default.
    pub rule_skip: bool,
}

/// Choose the values for one part.
///
/// # Rules
/// - Override first, then the first matching rule, then the profile default.
///
/// # Do not
/// - Look at the feeder here; feeders are planned over all parts together.
pub fn resolve_placement(
    pp: &PickPlaceSection,
    facts: &PartFacts,
    over: &PartPlacement,
) -> ResolvedPlacement {
    let rule = matching_rule(pp, facts);
    let row = rule.and_then(|i| pp.placement_rule.get(i));
    let default_speed = if pp.default_speed_pct == 0 {
        FALLBACK_SPEED_PCT
    } else {
        pp.default_speed_pct
    };
    ResolvedPlacement {
        nozzle: over
            .nozzle
            .clone()
            .or_else(|| row.map(|r| r.nozzle.clone())),
        nozzle_overridden: over.nozzle.is_some(),
        rule,
        speed_pct: over
            .speed_pct
            .or_else(|| row.map(|r| r.speed_pct).filter(|s| *s > 0))
            .unwrap_or(default_speed),
        pick_height_mm: over
            .pick_height_mm
            .or_else(|| row.map(|r| r.pick_height_mm))
            .unwrap_or(0.0),
        place_height_mm: over
            .place_height_mm
            .or_else(|| row.map(|r| r.place_height_mm))
            .unwrap_or(0.0),
        rule_skip: row.is_some_and(|r| r.skip),
    }
}

/// One surface-mount part with everything the machine needs.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedPart {
    /// Component id.
    pub component: Id,
    /// Footprint id.
    pub footprint: Id,
    /// Reference designator, cleaned for a comma separated file.
    pub reference: String,
    /// Value, cleaned.
    pub comment: String,
    /// Short package name.
    pub package: String,
    /// True when the part sits on the bottom face.
    pub bottom: bool,
    /// True when the part is not fitted (variant, property, skip override or a skip rule).
    pub skip: bool,
    /// True when the part's own "Placement skip" property is set to skip.
    pub skip_override: bool,
    /// What the rules saw.
    pub facts: PartFacts,
    /// Nozzle, speed and heights.
    pub resolved: ResolvedPlacement,
    /// Feeder number; none for a skipped part or when the banks are full.
    pub feeder: Option<u32>,
}

/// Something the operator must fix before the machine can run the job.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanProblem {
    /// Finding code: `placement_nozzle`, `placement_feeders` or `placement_nozzle_types`.
    pub code: &'static str,
    /// One plain sentence.
    pub message: String,
    /// Components the problem is about.
    pub components: Vec<Id>,
}

/// Every planned part and every problem.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlacementPlan {
    /// Parts in natural reference order.
    pub parts: Vec<PlannedPart>,
    /// Problems found while planning.
    pub problems: Vec<PlanProblem>,
}

fn natural(reference: &str) -> (String, u64, String) {
    let split = reference
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(reference.len());
    let (prefix, rest) = reference.split_at(split);
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let number = digits.parse().unwrap_or(0);
    (prefix.to_string(), number, rest[digits.len()..].to_string())
}

fn is_chip(package: &str) -> bool {
    package.len() == 4 && package.bytes().all(|b| b.is_ascii_digit())
}

fn truthy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "x" | "dnp"
    )
}

/// Plan every surface-mount part of a design on one pick and place profile.
///
/// # Rules
/// - Feeders: an override first, then the profile's feeder map, then the next free
///   tape feeder for chip packages or the next free flexible feeder for the rest.
///   Parts of one value and package share a feeder.
/// - A part with no nozzle, an unlisted nozzle, an unreadable override, a feeder
///   outside every bank, too many feeder types or too many nozzle types gives a problem.
///
/// # Dependencies
/// - crates/doc/src/machine_profile.rs
///
/// # Do not
/// - Fail the whole plan for one bad part; report a problem and carry on.
pub fn plan_placement(
    p: &Project,
    pp: &PickPlaceSection,
    variant: Option<&Variant>,
) -> PlacementPlan {
    let components: BTreeMap<Id, &Component> = p.components.iter().map(|c| (c.id, c)).collect();
    let mut plan = PlacementPlan::default();
    for f in &p.board.footprints {
        let Some(c) = components.get(&f.component) else {
            continue;
        };
        if is_through_hole(f) {
            continue;
        }
        let over = match PartPlacement::from_component(c) {
            Ok(o) => o,
            Err(e) => {
                plan.problems.push(PlanProblem {
                    code: "placement_nozzle",
                    message: format!("{}: {e}", c.reference),
                    components: vec![c.id],
                });
                PartPlacement::default()
            }
        };
        let facts = part_facts(c, f, pp, variant);
        let resolved = resolve_placement(pp, &facts, &over);
        let excluded = c.fields.iter().any(|x| {
            (x.key.eq_ignore_ascii_case("DNP") || x.key.eq_ignore_ascii_case("Exclude from BOM"))
                && truthy(&x.value)
        });
        let skip = over.skip.unwrap_or(resolved.rule_skip)
            || excluded
            || variant.is_some_and(|v| v.dnp.contains(&c.id));
        plan.parts.push(PlannedPart {
            component: c.id,
            footprint: f.id,
            reference: clean_cell(&c.reference),
            comment: clean_cell(&c.value),
            package: facts.package.clone(),
            bottom: f.board_side() == Layer::Back,
            skip,
            skip_override: over.skip == Some(true),
            facts,
            resolved,
            feeder: over.feeder,
        });
    }
    plan.parts.sort_by_key(|a| natural(&a.reference));
    nozzle_problems(pp, &mut plan);
    assign_feeders(pp, &mut plan);
    plan
}

fn nozzle_problems(pp: &PickPlaceSection, plan: &mut PlacementPlan) {
    for part in plan.parts.iter().filter(|q| !q.skip) {
        match &part.resolved.nozzle {
            None => plan.problems.push(PlanProblem {
                code: "placement_nozzle",
                message: format!(
                    "{} ({}) has no nozzle on this machine: no rule fits a {} part of {:.1} mm, so pick one in its Placement machine section",
                    part.reference, part.package, part.package, part.facts.size_mm
                ),
                components: vec![part.component],
            }),
            Some(id) if !pp.nozzle.iter().any(|n| n.id == *id) => plan.problems.push(PlanProblem {
                code: "placement_nozzle",
                message: format!(
                    "{} uses nozzle {id}, which this machine does not have",
                    part.reference
                ),
                components: vec![part.component],
            }),
            Some(_) => {}
        }
    }
    if pp.nozzle_slots > 0 {
        for bottom in [false, true] {
            let kinds: BTreeSet<&str> = plan
                .parts
                .iter()
                .filter(|q| !q.skip && q.bottom == bottom)
                .filter_map(|q| q.resolved.nozzle.as_deref())
                .collect();
            if kinds.len() as u32 > pp.nozzle_slots {
                plan.problems.push(PlanProblem {
                    code: "placement_nozzle_types",
                    message: format!(
                        "the {} side needs {} nozzle types and the machine runs {} without an operator",
                        if bottom { "bottom" } else { "top" },
                        kinds.len(),
                        pp.nozzle_slots
                    ),
                    components: vec![],
                });
            }
        }
    }
}

fn assign_feeders(pp: &PickPlaceSection, plan: &mut PlacementPlan) {
    let in_bank = |n: u32| pp.feeders.iter().any(|b| n >= b.first && n <= b.last);
    let mut used: BTreeSet<u32> = pp.feeder_map.iter().map(|r| r.feeder_no).collect();
    for part in plan.parts.iter().filter(|q| !q.skip) {
        if let Some(n) = part.feeder {
            used.insert(n);
        }
    }
    let mut keys: Vec<(String, String)> = Vec::new();
    for part in plan.parts.iter().filter(|q| !q.skip && q.feeder.is_none()) {
        let key = (part.comment.clone(), part.package.clone());
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    let mut assigned: BTreeMap<(String, String), u32> = BTreeMap::new();
    let mut full: BTreeMap<&'static str, Vec<Id>> = BTreeMap::new();
    for key in keys {
        if let Some(row) = pp
            .feeder_map
            .iter()
            .find(|r| r.footprint == key.1 && (r.comment.is_empty() || r.comment == key.0))
        {
            assigned.insert(key, row.feeder_no);
            continue;
        }
        let name = if is_chip(&key.1) { "tape" } else { "flexible" };
        let found = pp
            .feeders
            .iter()
            .find(|b| b.name == name)
            .and_then(|b| (b.first..=b.last).find(|n| !used.contains(n)));
        match found {
            Some(n) => {
                used.insert(n);
                assigned.insert(key, n);
            }
            None => {
                let ids = plan
                    .parts
                    .iter()
                    .filter(|q| q.comment == key.0 && q.package == key.1 && !q.skip)
                    .map(|q| q.component)
                    .collect::<Vec<_>>();
                full.entry(name).or_default().extend(ids);
            }
        }
    }
    let mut bad: Vec<PlanProblem> = Vec::new();
    for part in plan.parts.iter_mut().filter(|q| !q.skip) {
        if part.feeder.is_none() {
            part.feeder = assigned
                .get(&(part.comment.clone(), part.package.clone()))
                .copied();
        } else if let Some(n) = part.feeder.filter(|n| !in_bank(*n)) {
            bad.push(PlanProblem {
                code: "placement_feeders",
                message: format!(
                    "{} is set to feeder {n}, which is outside every feeder bank of this machine",
                    part.reference
                ),
                components: vec![part.component],
            });
        }
    }
    plan.problems.extend(bad);
    for (bank, components) in full {
        let (first, last) = pp
            .feeders
            .iter()
            .find(|b| b.name == bank)
            .map_or((0, 0), |b| (b.first, b.last));
        plan.problems.push(PlanProblem {
            code: "placement_feeders",
            message: format!(
                "the {bank} feeders ({first} to {last}) are all taken, so this board needs more part types than the machine has"
            ),
            components,
        });
    }
}
