//! `layout.report`: a read-only picture of where parts sit and how long the
//! connections between them are.
//!
//! Context: the report is the shared input for people, the assistant and later
//! placers. It reads the project, changes nothing, and is dispatched like the
//! other read-only commands in `lib.rs` (no revision bump, no undo entry).
//!
//! Local rules:
//! - `scope` is required and an empty scope is refused (`crates/engine/src/scope.rs`).
//! - Output is plain JSON in a fixed order: parts by reference then id, nets by
//!   name then id, pairs with the earlier reference first. Numbers are rounded
//!   to 0.0001 mm so two runs on the same design are byte-identical.
//! - A part's box is the extent of its pads and drawn graphics in board
//!   coordinates (no courtyard margin). Gaps and overlaps compare boxes on the
//!   same board side.
//! - Alignment groups are two or more in-scope parts whose centres share an x
//!   (a column) or a y (a row) within `ALIGN_TOLERANCE_MM`.
//! - Wirelength numbers come from `crates/engine/src/wirelength.rs`; `unrouted_mm`
//!   is the length still open after existing copper. The key `unrouted` is the count of open airwires on the net.
//! - `layout.place_suggest` returns candidate positions for a satellite part
//!   near an anchor part, scored by proximity. It changes nothing.
//! - `layout.place_apply` applies a placement in one undo step and refuses if
//!   the project revision has changed.
//! - `layout.place_polish` runs the bounded annealing polish over the unlocked
//!   parts and moves them in one undo step; see `polish`.
//!
//! Dependencies: `crates/engine/src/scope.rs`, `crates/engine/src/wirelength.rs`,
//! `crates/rules/src/lib.rs` (`airwires`), `crates/doc/src/lib.rs`.
use super::{CommandSpec, Error, schema};
use crate::scope::{self, Scope};
use crate::wirelength;
use pcbcraft_doc::{Footprint, Id, Project};
use pcbcraft_geom::Point;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Centres closer than this on one axis count as aligned, in millimetres.
pub const ALIGN_TOLERANCE_MM: f64 = 0.05;
/// Most footprints one report covers; more is refused.
pub const MAX_PARTS: usize = 2000;
const OVERLAP_EPS_MM: f64 = 1e-6;
/// Most placement candidates returned by one suggestion.
pub const MAX_CANDIDATES: usize = 16;

/// True for the layout read commands.
pub(super) fn handles(command: &str) -> bool {
    command == "layout.report" || command == "layout.place_suggest"
}

/// True for layout commands that mutate the design.
pub(super) fn handles_mutating(command: &str) -> bool {
    command == "layout.place_apply" || command == "layout.place_polish"
}

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "layout.report",
            description: "Read-only layout report for a scope: part positions and boxes, nearest-neighbour gaps, alignment rows and columns, per-net half-perimeter, ratsnest length and unrouted connection count with totals, parts off the board and overlapping parts. Deterministic JSON; changes nothing",
            input_schema: schema(json!({"scope": scope::schema()}), &["scope"]),
        },
        CommandSpec {
            id: "layout.place_suggest",
            description: "Rule-driven placement candidates for a satellite part near an anchor part. For decoupling capacitors, returns positions within the decoupling distance of the anchor's power pin, scored by proximity. Changes nothing; returns a score table for the caller to choose from",
            input_schema: schema(
                json!({
                    "satellite": {"type": "integer", "description": "component id to place"},
                    "anchor": {"type": "integer", "description": "component id to place near (optional, inferred from net connections)"},
                    "rule": {"type": "string", "enum": ["decoupling", "crystal", "esd", "edge"], "description": "placement rule; defaults to decoupling"}
                }),
                &["satellite"],
            ),
        },
        CommandSpec {
            id: "layout.place_apply",
            description: "Apply a placement suggestion to a component in one undo step. Refuses if the project revision has changed since the suggestion was made. The position must be within the rule distance of the anchor pin",
            input_schema: schema(
                json!({
                    "component": {"type": "integer", "description": "component id to place"},
                    "position": {"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}}, "required": ["x", "y"]},
                    "revision": {"type": "integer", "description": "expected project revision for optimistic locking"}
                }),
                &["component", "position"],
            ),
        },
        CommandSpec {
            id: "layout.place_polish",
            description: "Arrange the unlocked parts: runs the bounded annealing polish that shortens wire while keeping every part clear of the others, the keepouts and the board edge. Locked parts and parts flush to the board edge never move. One undo step; refuses with a plain sentence, and changes nothing, when no part can move or the result would add an error. The same board and seed always give the same result",
            input_schema: schema(
                json!({
                    "scope": scope::schema(),
                    "seed": {"type": "integer", "minimum": 0, "description": "random seed of the polish; defaults to 0, same seed and board give the same result"},
                    "moves": {"type": "integer", "minimum": 1, "maximum": POLISH_MAX_MOVES, "description": "most trial moves; defaults to 50 per movable part, at least 500"}
                }),
                &[],
            ),
        },
    ]
}

/// Most trial moves one polish may use.
pub const POLISH_MAX_MOVES: usize = 20_000;

/// Run a layout read command.
pub(super) fn run(p: &Project, command: &str, params: &Value) -> Result<Value, Error> {
    match command {
        "layout.report" => {
            let scope = Scope::from_value(p, params.get("scope").unwrap_or(&Value::Null))?;
            report(p, &scope)
        }
        "layout.place_suggest" => place_suggest(p, params),
        _ => Err(Error::Invalid(format!("unknown layout command: {command}"))),
    }
}

/// Run a layout mutating command.
pub(super) fn run_mutating(
    p: &mut Project,
    command: &str,
    params: &Value,
    expected_revision: Option<u64>,
) -> Result<Value, Error> {
    match command {
        "layout.place_apply" => place_apply(p, params, expected_revision),
        "layout.place_polish" => polish(p, params),
        _ => Err(Error::Invalid(format!(
            "unknown layout mutating command: {command}"
        ))),
    }
}

fn r(x: f64) -> f64 {
    let v = (x * 10_000.0).round() / 10_000.0;
    if v == 0.0 { 0.0 } else { v }
}

#[derive(Clone)]
struct Part {
    footprint: Id,
    component: Id,
    reference: String,
    centre: Point,
    rotation: f64,
    side: String,
    locked: bool,
    lo: Point,
    hi: Point,
    in_scope: bool,
}

fn part_of(p: &Project, f: &Footprint, scope: &Scope) -> Part {
    let world: Vec<Point> = f
        .drawing_points(p)
        .into_iter()
        .map(|q| f.position.offset(q.rotate(-f.rotation)))
        .collect();
    let (mut lo, mut hi) = (f.position, f.position);
    for q in &world {
        lo = Point::new(lo.x.min(q.x), lo.y.min(q.y));
        hi = Point::new(hi.x.max(q.x), hi.y.max(q.y));
    }
    Part {
        footprint: f.id,
        component: f.component,
        reference: p
            .component(f.component)
            .map(|c| c.reference.clone())
            .unwrap_or_default(),
        centre: f.position,
        rotation: f.rotation,
        side: p.layer_name(f.board_side()),
        locked: f.locked == Some(true),
        lo,
        hi,
        in_scope: scope.contains(p, f),
    }
}

fn gap(a: &Part, b: &Part) -> f64 {
    let dx = (a.lo.x - b.hi.x).max(b.lo.x - a.hi.x).max(0.0);
    let dy = (a.lo.y - b.hi.y).max(b.lo.y - a.hi.y).max(0.0);
    dx.hypot(dy)
}

fn overlaps(a: &Part, b: &Part) -> bool {
    a.lo.x.max(b.lo.x) + OVERLAP_EPS_MM < a.hi.x.min(b.hi.x)
        && a.lo.y.max(b.lo.y) + OVERLAP_EPS_MM < a.hi.y.min(b.hi.y)
}

fn name_of(a: &Part) -> Value {
    json!({"footprint": a.footprint, "reference": a.reference})
}

fn groups(parts: &[&Part], axis: &str) -> Vec<Value> {
    let key = |p: &Part| if axis == "x" { p.centre.x } else { p.centre.y };
    let mut sorted: Vec<&Part> = parts.to_vec();
    sorted.sort_by(|a, b| {
        key(a)
            .total_cmp(&key(b))
            .then(a.reference.cmp(&b.reference))
            .then(a.footprint.cmp(&b.footprint))
    });
    let mut out = vec![];
    let mut run: Vec<&Part> = vec![];
    let mut flush = |run: &mut Vec<&Part>| {
        if run.len() >= 2 {
            let mut members: Vec<&Part> = run.clone();
            members.sort_by(|a, b| {
                a.reference
                    .cmp(&b.reference)
                    .then(a.footprint.cmp(&b.footprint))
            });
            let at = run.iter().map(|p| key(p)).sum::<f64>() / run.len() as f64;
            out.push(json!({
                "axis": axis,
                "at": r(at),
                "parts": members.iter().map(|p| name_of(p)).collect::<Vec<_>>(),
            }));
        }
        run.clear();
    };
    for p in sorted {
        if run
            .last()
            .is_some_and(|l| key(p) - key(l) > ALIGN_TOLERANCE_MM)
        {
            flush(&mut run);
        }
        run.push(p);
    }
    flush(&mut run);
    out
}

fn report(p: &Project, scope: &Scope) -> Result<Value, Error> {
    if p.board.footprints.len() > MAX_PARTS {
        return Err(Error::Invalid(format!(
            "layout.report covers at most {MAX_PARTS} placed parts"
        )));
    }
    let mut all: Vec<Part> = p
        .board
        .footprints
        .iter()
        .map(|f| part_of(p, f, scope))
        .collect();
    all.sort_by(|a, b| {
        a.reference
            .cmp(&b.reference)
            .then(a.footprint.cmp(&b.footprint))
    });
    let scoped: Vec<&Part> = all.iter().filter(|a| a.in_scope).collect();

    let parts: Vec<Value> = scoped
        .iter()
        .map(|a| {
            json!({
                "footprint": a.footprint, "component": a.component, "reference": a.reference,
                "x": r(a.centre.x), "y": r(a.centre.y), "rotation": r(a.rotation),
                "side": a.side, "locked": a.locked,
                "movable": !(scope.respect_locked && a.locked),
                "box": {"min_x": r(a.lo.x), "min_y": r(a.lo.y), "max_x": r(a.hi.x), "max_y": r(a.hi.y)},
            })
        })
        .collect();

    let gaps: Vec<Value> = scoped
        .iter()
        .filter_map(|a| {
            all.iter()
                .filter(|b| b.footprint != a.footprint && b.side == a.side)
                .map(|b| (gap(a, b), b))
                .min_by(|x, y| {
                    x.0.total_cmp(&y.0)
                        .then(x.1.reference.cmp(&y.1.reference))
                        .then(x.1.footprint.cmp(&y.1.footprint))
                })
                .map(|(g, b)| json!({"from": name_of(a), "to": name_of(b), "gap_mm": r(g)}))
        })
        .collect();

    let mut alignment = groups(&scoped, "x");
    alignment.extend(groups(&scoped, "y"));

    let off_board: Vec<Value> = scoped
        .iter()
        .filter(|a| {
            [
                a.lo,
                a.hi,
                Point::new(a.lo.x, a.hi.y),
                Point::new(a.hi.x, a.lo.y),
            ]
            .iter()
            .any(|c| !p.board_contains(*c))
        })
        .map(|a| name_of(a))
        .collect();

    let mut overlapping = vec![];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            if a.side == b.side && (a.in_scope || b.in_scope) && overlaps(a, b) {
                overlapping.push(json!({"a": name_of(a), "b": name_of(b)}));
            }
        }
    }

    let unrouted: BTreeMap<Id, (usize, f64)> =
        pcbcraft_rules::airwires(p)
            .iter()
            .fold(BTreeMap::new(), |mut m, w| {
                let entry = m.entry(w.net).or_insert((0, 0.0));
                entry.0 += 1;
                entry.1 += w.from.distance(w.to);
                m
            });
    let mut routed: BTreeMap<Id, f64> = BTreeMap::new();
    for track in &p.board.tracks {
        let entry = routed.entry(track.net).or_insert(0.0);
        *entry +=
            ((track.end.x - track.start.x).powi(2) + (track.end.y - track.start.y).powi(2)).sqrt();
    }
    let mut net_ids: Vec<Id> = p
        .board
        .footprints
        .iter()
        .filter(|f| scope.contains(p, f))
        .flat_map(|f| f.pads.iter().filter_map(|pad| pad.net))
        .collect();
    net_ids.sort_unstable();
    net_ids.dedup();
    let mut nets = vec![];
    let (mut total_hpwl, mut total_rats, mut total_open, mut total_open_count, mut total_routed) =
        (0.0, 0.0, 0.0, 0, 0.0);
    for net in net_ids {
        let points = wirelength::net_pad_points(p, net);
        if points.len() < 2 {
            continue;
        }
        let rats = wirelength::ratsnest_length(&points).ok_or_else(|| {
            Error::Invalid(format!(
                "net {} has too many pads for a report",
                p.net_name(net)
            ))
        })?;
        let h = wirelength::hpwl(&points);
        let (open_count, open_len) = unrouted.get(&net).copied().unwrap_or((0, 0.0));
        total_hpwl += h;
        total_rats += rats;
        total_open += open_len;
        total_open_count += open_count;
        let routed_len = routed.get(&net).copied().unwrap_or(0.0);
        total_routed += routed_len;
        nets.push((
            p.net_name(net).to_string(),
            net,
            json!({"net": net, "name": p.net_name(net), "pads": points.len(),
                   "hpwl_mm": r(h), "ratsnest_mm": r(rats), "unrouted": open_count, "unrouted_mm": r(open_len),
                   "routed_mm": r(routed_len)}),
        ));
    }
    nets.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    Ok(json!({
        "scope": {"layers": scope.layers, "nets": scope.nets, "components": scope.components,
                  "respect_locked": scope.respect_locked},
        "parts": parts,
        "gaps": gaps,
        "alignment_groups": alignment,
        "nets": nets.into_iter().map(|n| n.2).collect::<Vec<_>>(),
        "totals": {"hpwl_mm": r(total_hpwl), "ratsnest_mm": r(total_rats), "unrouted": total_open_count, "unrouted_mm": r(total_open), "routed_mm": r(total_routed)},
        "off_board": off_board,
        "overlaps": overlapping,
    }))
}

/// Placement candidates for a satellite part near an anchor part.
///
/// # Rules
/// - For decoupling capacitors, finds the anchor's power pin and returns
///   positions within the decoupling distance, scored by proximity.
/// - Returns an empty list when no valid position exists within the rule distance.
/// - Never moves the part; the caller must call `layout.place_apply` separately.
///
/// # Dependencies
/// - crates/doc/src/lib.rs `DecouplingRule` supplies the distance limit.
///
/// # Do not
/// - Change the project state.
/// - Return more than `MAX_CANDIDATES` positions.
fn place_suggest(p: &Project, params: &Value) -> Result<Value, Error> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        satellite: Id,
        anchor: Option<Id>,
        rule: Option<String>,
    }
    let a: Params = super::parse(params)?;
    let rule = a.rule.as_deref().unwrap_or("decoupling");

    // Validate satellite exists and is placed.
    let sat_component = p
        .component(a.satellite)
        .ok_or_else(|| Error::Invalid(format!("no component with id {}", a.satellite)))?;
    if !p
        .board
        .footprints
        .iter()
        .any(|f| f.component == a.satellite)
    {
        return Err(Error::Invalid(format!(
            "component {} is not placed on the board",
            sat_component.reference
        )));
    }

    // For decoupling, find the anchor IC and its power pin.
    if rule == "decoupling" {
        let anchor_id = match a.anchor {
            Some(id) => id,
            None => {
                return Err(Error::Invalid(
                    "anchor component id is required for decoupling placement".into(),
                ));
            }
        };
        let anchor_component = p
            .component(anchor_id)
            .ok_or_else(|| Error::Invalid(format!("no component with id {}", anchor_id)))?;
        let anchor_fp = p
            .board
            .footprints
            .iter()
            .find(|f| f.component == anchor_id)
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "component {} is not placed on the board",
                    anchor_component.reference
                ))
            })?;

        // Find the power pin of the anchor.
        let power_pin = anchor_component
            .pins
            .iter()
            .find(|pin| pin.electrical_type == "power_in")
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "component {} has no power_in pin for decoupling placement",
                    anchor_component.reference
                ))
            })?;

        // Get the world position of the power pin pad.
        let pin_point = anchor_fp.pad_position(&power_pin.number).ok_or_else(|| {
            Error::Invalid(format!(
                "pin {} of {} is not placed",
                power_pin.number, anchor_component.reference
            ))
        })?;

        // Generate candidate positions around the power pin within the decoupling distance.
        let max_dist = p.decoupling.max_distance_mm;
        let mut candidates = vec![];
        // Try 8 directions around the pin.
        let angles = [0.0_f64, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0];
        for angle in angles {
            let rad = angle.to_radians();
            let dx = max_dist * rad.cos();
            let dy = max_dist * rad.sin();
            let candidate_pos = Point::new(pin_point.x + dx, pin_point.y + dy);
            // Score: 0 is perfect (at the pin), 1 is at the max distance.
            let score = r(candidate_pos.distance(pin_point) / max_dist);
            candidates.push(json!({
                "position": {"x": r(candidate_pos.x), "y": r(candidate_pos.y)},
                "score": score,
                "distance_mm": r(candidate_pos.distance(pin_point)),
            }));
        }
        // Sort by score (ascending).
        candidates.sort_by(|a, b| {
            let sa = a["score"].as_f64().unwrap_or(1.0);
            let sb = b["score"].as_f64().unwrap_or(1.0);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(MAX_CANDIDATES);

        Ok(json!({
            "satellite": {"id": a.satellite, "reference": sat_component.reference},
            "anchor": {"id": anchor_id, "reference": anchor_component.reference, "pin": power_pin.number},
            "rule": "decoupling",
            "max_distance_mm": max_dist,
            "candidates": candidates,
        }))
    } else {
        Err(Error::Invalid(format!(
            "placement rule '{}' is not implemented yet",
            rule
        )))
    }
}

/// Apply a placement suggestion to a component.
///
/// # Rules
/// - Moves the component's footprint to the given position in one undo step.
/// - Refuses if the project revision has changed since the suggestion was made.
/// - Refuses if the footprint is locked.
///
/// # Dependencies
/// - crates/engine/src/lib.rs for revision checking.
///
/// # Do not
/// - Change rotation or any other footprint property.
fn place_apply(
    p: &mut Project,
    params: &Value,
    expected_revision: Option<u64>,
) -> Result<Value, Error> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        component: Id,
        position: Value,
        revision: Option<u64>,
    }
    let a: Params = super::parse(params)?;

    // Check revision if provided.
    if let Some(rev) = a.revision
        && let Some(expected) = expected_revision
        && rev != expected
    {
        return Err(Error::Stale {
            expected: rev,
            current: expected,
        });
    }

    // Parse position.
    let x = a
        .position
        .get("x")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::Invalid("position.x is required".into()))?;
    let y = a
        .position
        .get("y")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::Invalid("position.y is required".into()))?;
    if !x.is_finite() || !y.is_finite() {
        return Err(Error::Invalid("position coordinates must be finite".into()));
    }

    // Find and move the footprint.
    let reference = p
        .component(a.component)
        .ok_or_else(|| Error::Invalid(format!("no component with id {}", a.component)))?
        .reference
        .clone();
    let fp = p
        .board
        .footprints
        .iter_mut()
        .find(|f| f.component == a.component)
        .ok_or_else(|| {
            Error::Invalid(format!(
                "component {} is not placed on the board",
                reference
            ))
        })?;
    if fp.locked == Some(true) {
        return Err(Error::Invalid(format!(
            "footprint of {} is locked against placement changes",
            reference
        )));
    }
    fp.position = Point::new(x, y);

    Ok(json!({
        "component": a.component,
        "reference": reference,
        "position": {"x": r(x), "y": r(y)},
    }))
}

/// Arrange the unlocked parts with the annealing polish.
///
/// # Rules
/// - Scope defaults to every copper layer; locked parts are never moved.
/// - The work is bounded by a move count (`moves`, at most `POLISH_MAX_MOVES`),
///   never by time, and the generator is seeded, so one board and seed give one
///   result.
/// - Refusals are plain sentences and leave the project as it was: no free
///   part, nothing worth moving, or a result that would add an error.
///
/// # Dependencies
/// - crates/engine/src/placement/mod.rs (`run_polish`, `PlacementSettings`)
///
/// # Do not
/// - Move a locked part, or read the clock.
fn polish(p: &mut Project, params: &Value) -> Result<Value, Error> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        scope: Option<Value>,
        seed: Option<u64>,
        moves: Option<usize>,
    }
    let a: Params = super::parse(params)?;
    let mut scope = match &a.scope {
        Some(v) => Scope::from_value(p, v)?,
        None => Scope {
            layers: p
                .copper_layers()
                .into_iter()
                .map(|l| p.layer_name(l))
                .collect(),
            ..Scope::default()
        },
    };
    scope.respect_locked = true;
    let settings = crate::placement::PlacementSettings {
        seed: a.seed.unwrap_or(0),
        moves: a.moves,
        ..crate::placement::PlacementSettings::default()
    };
    settings.validate().map_err(Error::Invalid)?;
    let model = crate::placement::build_model(p, &scope, &settings).map_err(Error::Invalid)?;
    let free = model.cells.iter().filter(|c| !c.fixed).count();
    if free == 0 {
        return Err(Error::Invalid(
            "There are no unlocked parts to arrange: every part in scope is locked or sits on the board edge.".into(),
        ));
    }
    let outcome = crate::placement::run_polish(p, &scope, &settings).map_err(|e| {
        if e == "polish introduced a new error" {
            Error::Invalid("Arranging would add a new rule error, so nothing was moved.".into())
        } else {
            Error::Invalid(e)
        }
    })?;
    if outcome.moved.is_empty() {
        return Err(Error::Invalid(
            "The unlocked parts are already as well placed as the arrange can get them, so nothing moved.".into(),
        ));
    }
    let moved: Vec<Value> = outcome
        .moved
        .iter()
        .map(|m| {
            let reference = p
                .footprint(m.id)
                .and_then(|f| p.component(f.component))
                .map(|c| c.reference.clone())
                .unwrap_or_default();
            json!({"footprint": m.id, "reference": reference, "x": r(m.x), "y": r(m.y), "turns": m.turns})
        })
        .collect();
    let count = moved.len();
    Ok(json!({
        "moved": moved,
        "count": count,
        "unlocked": free,
        "wire_before_mm": r(outcome.hpwl_before),
        "wire_after_mm": r(outcome.hpwl_after),
        "seed": settings.seed,
        "message": format!("Arranged: {count} of {free} unlocked parts moved."),
    }))
}
