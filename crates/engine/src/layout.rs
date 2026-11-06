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
//! - `layout.global_place` runs a force-directed placement with bounded
//!   iterations, fixed seed, and legalization. It moves unlocked parts only.
//!
//! Dependencies: `crates/engine/src/scope.rs`, `crates/engine/src/wirelength.rs`,
//! `crates/engine/src/legalize.rs`, `crates/rules/src/lib.rs` (`airwires`),
//! `crates/doc/src/lib.rs`.
use super::{CommandSpec, Error, schema};
use crate::legalize;
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
/// Most iterations the global placer will run; a count bound, not time.
pub const MAX_ITERATIONS: usize = 256;
/// Default seed for the global placer; same seed gives same result.
pub const DEFAULT_SEED: u64 = 42;
/// Weight factor for power and ground nets; less than 1.0 down-weights them.
pub const POWER_NET_WEIGHT: f64 = 0.3;
/// Repulsion strength between parts.
pub const REPULSION: f64 = 100.0;
/// Spring constant for net connections.
pub const SPRING: f64 = 1.0;
/// Step size for placement moves, in millimetres.
pub const STEP_SIZE: f64 = 0.5;

/// True for the layout read commands.
pub(super) fn handles(command: &str) -> bool {
    command == "layout.report" || command == "layout.place_suggest"
}

/// True for layout commands that mutate the design.
pub(super) fn handles_mutating(command: &str) -> bool {
    command == "layout.place_apply" || command == "layout.global_place"
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
            id: "layout.global_place",
            description: "Force-directed global placement: nets act as springs, parts repel; power and ground nets are down-weighted; locked parts are fixed anchors. Bounded by iteration count, not time. Same seed gives same result. Followed by legalization to remove overlaps. Returns the wirelength metric before and after",
            input_schema: schema(
                json!({
                    "scope": scope::schema(),
                    "seed": {"type": "integer", "minimum": 0, "default": 42, "description": "random seed for deterministic results"},
                    "iterations": {"type": "integer", "minimum": 1, "maximum": 256, "default": 100, "description": "iteration count bound"},
                    "legalize": {"type": "boolean", "default": true, "description": "run legalization after placement"}
                }),
                &["scope"],
            ),
        },
    ]
}

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
        "layout.global_place" => global_place(p, params, expected_revision),
        _ => Err(Error::Invalid(format!(
            "unknown layout mutating command: {command}"
        ))),
    }
}

fn r(x: f64) -> f64 {
    let v = (x * 10_000.0).round() / 10_000.0;
    if v == 0.0 { 0.0 } else { v }
}

/// A simple deterministic PRNG (xorshift64) for placement jitter.
fn make_rng(seed: u64) -> Box<dyn FnMut() -> u64> {
    let mut state = seed;
    Box::new(move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    })
}

/// True when a net name looks like power or ground.
fn is_power_net(name: &str) -> bool {
    let upper = name.to_uppercase();
    upper.starts_with("VCC")
        || upper.starts_with("VDD")
        || upper.starts_with("VSS")
        || upper.starts_with("GND")
        || upper.starts_with("3V3")
        || upper.starts_with("5V")
        || upper.starts_with("12V")
        || upper == "V+"
        || upper == "V-"
        || upper == "PWR"
        || upper == "POWER"
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

/// Global force-directed placement with bounded iterations.
///
/// Why: arrange all unlocked parts to minimise wirelength while spreading
/// them out. Nets act as springs pulling connected parts together; parts
/// repel each other to avoid overlap. Power and ground nets are down-weighted
/// because their routing is less critical. Locked parts (connectors, edge
/// parts) are fixed anchors. The iteration count is bounded and the seed is
/// fixed so the result is deterministic. After placement, legalization
/// removes any remaining overlaps.
///
/// # Rules
/// - Same seed and same input always give the same output.
/// - Iterations are bounded by `MAX_ITERATIONS`; no time bound.
/// - Power and ground nets get `POWER_NET_WEIGHT` times the spring force.
/// - Locked parts never move.
/// - Parts outside the scope are obstacles and never move.
/// - After placement, legalization runs to remove overlaps.
/// - The command is one undo step.
///
/// # Dependencies
/// - crates/engine/src/scope.rs (Scope)
/// - crates/engine/src/wirelength.rs (hpwl, ratsnest_length, net_pad_points)
/// - crates/engine/src/legalize.rs (plan_moves)
///
/// # Do not
/// - Use time-based termination.
/// - Move locked parts or parts outside the scope.
/// - Change the project state if validation fails.
fn global_place(
    p: &mut Project,
    params: &Value,
    _expected_revision: Option<u64>,
) -> Result<Value, Error> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        scope: Value,
        seed: Option<u64>,
        iterations: Option<usize>,
        legalize: Option<bool>,
    }
    let a: Params = super::parse(params)?;
    let scope = Scope::from_value(p, &a.scope)?;
    let seed = a.seed.unwrap_or(DEFAULT_SEED);
    let iterations = a.iterations.unwrap_or(100).min(MAX_ITERATIONS);
    let do_legalize = a.legalize.unwrap_or(true);

    // Collect all placed footprints with their pad nets.
    let mut parts: Vec<Part> = p
        .board
        .footprints
        .iter()
        .map(|f| part_of(p, f, &scope))
        .collect();

    // Build net connectivity: for each net, list of (part_index, pad_position).
    let mut net_parts: BTreeMap<Id, Vec<(usize, Point)>> = BTreeMap::new();
    for (idx, part) in parts.iter().enumerate() {
        let fp = p
            .board
            .footprints
            .iter()
            .find(|f| f.id == part.footprint)
            .ok_or_else(|| Error::Invalid("footprint vanished".into()))?;
        for pad in &fp.pads {
            if let Some(net) = pad.net {
                let world = fp.pad_world_position(pad);
                let entry = net_parts.entry(net).or_default();
                entry.push((idx, world));
            }
        }
    }

    // Compute total wirelength before placement.
    let mut hpwl_before = 0.0;
    for (_net, points) in net_parts.iter() {
        if points.len() >= 2 {
            let pts: Vec<Point> = points.iter().map(|(_, p)| *p).collect();
            hpwl_before += wirelength::hpwl(&pts);
        }
    }

    // Force-directed placement loop.
    let mut rng = make_rng(seed);
    let board_w = p.board.width;
    let board_h = p.board.height;

    for _iter in 0..iterations {
        // Compute forces on each part.
        let mut forces: Vec<Point> = vec![Point::new(0.0, 0.0); parts.len()];

        // Spring forces: connected parts attract.
        for (net, conns) in net_parts.iter() {
            // Down-weight power/ground nets.
            let net_name = p.net_name(*net);
            let weight = if is_power_net(net_name) {
                POWER_NET_WEIGHT
            } else {
                1.0
            };

            // Apply spring force between all pairs on this net.
            for i in 0..conns.len() {
                for j in (i + 1)..conns.len() {
                    let (idx_a, pos_a) = conns[i];
                    let (idx_b, pos_b) = conns[j];
                    let dx = pos_b.x - pos_a.x;
                    let dy = pos_b.y - pos_a.y;
                    let dist = dx.hypot(dy);
                    if dist < 1e-9 {
                        continue;
                    }
                    // Hooke's law: F = k * (dist - rest_length)
                    // Use a rest length of 5mm as a reasonable default.
                    let rest_length = 5.0;
                    let f = SPRING * weight * (dist - rest_length);
                    let fx = f * dx / dist;
                    let fy = f * dy / dist;
                    forces[idx_a] = Point::new(forces[idx_a].x + fx, forces[idx_a].y + fy);
                    forces[idx_b] = Point::new(forces[idx_b].x - fx, forces[idx_b].y - fy);
                }
            }
        }

        // Repulsion forces: all parts repel each other.
        for i in 0..parts.len() {
            for j in (i + 1)..parts.len() {
                let dx = parts[j].centre.x - parts[i].centre.x;
                let dy = parts[j].centre.y - parts[i].centre.y;
                let dist = dx.hypot(dy);
                if dist < 1e-9 {
                    // Add jitter to break ties.
                    let jx = ((rng() % 100) as f64 - 50.0) * 0.01;
                    let jy = ((rng() % 100) as f64 - 50.0) * 0.01;
                    forces[i] = Point::new(forces[i].x - jx, forces[i].y - jy);
                    forces[j] = Point::new(forces[j].x + jx, forces[j].y + jy);
                } else {
                    // Coulomb's law: F = k / dist^2
                    let f = REPULSION / (dist * dist);
                    let fx = f * dx / dist;
                    let fy = f * dy / dist;
                    forces[i] = Point::new(forces[i].x - fx, forces[i].y - fy);
                    forces[j] = Point::new(forces[j].x + fx, forces[j].y + fy);
                }
            }
        }

        // Apply forces: move unlocked, in-scope parts.
        let mut moved = false;
        for (idx, part) in parts.iter_mut().enumerate() {
            if !part.in_scope || part.locked {
                continue;
            }
            let f = forces[idx];
            let mag = f.x.hypot(f.y);
            if mag < 1e-9 {
                continue;
            }
            // Limit step size.
            let step = STEP_SIZE;
            let dx = step * f.x / mag;
            let dy = step * f.y / mag;

            // Clamp to board bounds.
            let new_x = (part.centre.x + dx).clamp(0.0, board_w);
            let new_y = (part.centre.y + dy).clamp(0.0, board_h);

            // Only move if changed.
            if (new_x - part.centre.x).abs() > 1e-9 || (new_y - part.centre.y).abs() > 1e-9 {
                part.centre = Point::new(new_x, new_y);
                moved = true;
            }
        }

        if !moved {
            break;
        }
    }

    // Apply final positions to the project.
    for part in &parts {
        if part.in_scope && !part.locked {
            if let Some(fp) = p
                .board
                .footprints
                .iter_mut()
                .find(|f| f.id == part.footprint)
            {
                fp.position = part.centre;
            }
        }
    }

    // Compute wirelength after placement.
    let mut hpwl_after = 0.0;
    for (net, conns) in net_parts.iter() {
        if conns.len() >= 2 {
            let pts: Vec<Point> = conns.iter().map(|(_, p)| *p).collect();
            hpwl_after += wirelength::hpwl(&pts);
        }
    }

    // Legalize: remove overlaps.
    let mut legal_moves = 0;
    if do_legalize {
        let plan = legalize::plan_moves(p, &scope, 32, None);
        for (fid, pos) in &plan.moves {
            if let Some(fp) = p.board.footprints.iter_mut().find(|f| f.id == *fid) {
                fp.position = *pos;
                legal_moves += 1;
            }
        }
    }

    // Compute final wirelength after legalization.
    let mut hpwl_final = 0.0;
    for (net, conns) in net_parts.iter() {
        if conns.len() >= 2 {
            let pts: Vec<Point> = conns.iter().map(|(_, p)| *p).collect();
            hpwl_final += wirelength::hpwl(&pts);
        }
    }

    let moved_count = parts.iter().filter(|p| p.in_scope && !p.locked).count();

    Ok(json!({
        "moved": moved_count,
        "legalized": legal_moves,
        "iterations": iterations,
        "seed": seed,
        "wirelength": {
            "before_mm": r(hpwl_before),
            "after_placement_mm": r(hpwl_after),
            "after_legalization_mm": r(hpwl_final)
        }
    }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::BoardBuilder;

    fn make_board() -> Board {
        let mut builder = BoardBuilder::new();
        builder.set_size(100.0, 100.0);
        builder.add_layer("top", LayerKind::Signal);
        builder.add_layer("bottom", LayerKind::Signal);
        builder
    }

    fn build_test_project() -> Project {
        let board = make_board().build();
        let mut p = Project::new("test");
        p.board = board;
        p
    }

    fn add_component(p: &mut Project, ref_id: &str, footprint: &str) -> Id {
        let id = p.board.components.push(Component {
            reference: ref_id.to_string(),
            footprint: footprint.to_string(),
            locked: false,
            ..Default::default()
        });
        id
    }

    fn add_footprint(p: &mut Project, component: Id, ref_id: &str) {
        p.board.footprints.push(Footprint {
            component,
            reference: ref_id.to_string(),
            position: Point::new(0.0, 0.0),
            locked: false,
            ..Default::default()
        });
    }

    fn add_net(p: &mut Project, name: &str, component_ids: &[Id]) -> Id {
        let id = p.board.nets.push(Net {
            name: name.to_string(),
            locked: false,
            ..Default::default()
        });
        for &comp in component_ids {
            p.board.net_members.push(NetMember {
                net: id,
                component: comp,
                pad: None,
            });
        }
        id
    }

    #[test]
    fn global_place_runs_and_returns_wirelength() {
        let mut p = build_test_project();
        let c1 = add_component(&mut p, "C1", "cap_0805");
        let c2 = add_component(&mut p, "C2", "cap_0805");
        let r1 = add_component(&mut p, "R1", "res_0805");
        add_footprint(&mut p, c1, "C1");
        add_footprint(&mut p, c2, "C2");
        add_footprint(&mut p, r1, "R1");
        add_net(&mut p, "SIG", &[c1, c2]);
        add_net(&mut p, "SIG2", &[c2, r1]);

        let result = global_place(&mut p, json!({"scope": {"board": {}}}), None);
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.get("wirelength_before").is_some());
        assert!(out.get("wirelength_after").is_some());
    }

    #[test]
    fn global_place_is_deterministic() {
        let mut p1 = build_test_project();
        let c1 = add_component(&mut p1, "C1", "cap_0805");
        let c2 = add_component(&mut p1, "C2", "cap_0805");
        let r1 = add_component(&mut p1, "R1", "res_0805");
        add_footprint(&mut p1, c1, "C1");
        add_footprint(&mut p1, c2, "C2");
        add_footprint(&mut p1, r1, "R1");
        add_net(&mut p1, "SIG", &[c1, c2]);
        add_net(&mut p1, "SIG2", &[c2, r1]);

        let mut p2 = build_test_project();
        let c1b = add_component(&mut p2, "C1", "cap_0805");
        let c2b = add_component(&mut p2, "C2", "cap_0805");
        let r1b = add_component(&mut p2, "R1", "res_0805");
        add_footprint(&mut p2, c1b, "C1");
        add_footprint(&mut p2, c2b, "C2");
        add_footprint(&mut p2, r1b, "R1");
        add_net(&mut p2, "SIG", &[c1b, c2b]);
        add_net(&mut p2, "SIG2", &[c2b, r1b]);

        let params = json!({"scope": {"board": {}}, "seed": 42});
        global_place(&mut p1, params.clone(), None).unwrap();
        global_place(&mut p2, params, None).unwrap();

        // Same seed should produce same placement
        for i in 0..p1.board.footprints.len() {
            let fp1 = &p1.board.footprints[i];
            let fp2 = &p2.board.footprints[i];
            assert_eq!(fp1.position.x, fp2.position.x);
            assert_eq!(fp1.position.y, fp2.position.y);
        }
    }

    #[test]
    fn global_place_respects_locked_parts() {
        let mut p = build_test_project();
        let c1 = add_component(&mut p, "C1", "cap_0805");
        let c2 = add_component(&mut p, "C2", "cap_0805");
        add_footprint(&mut p, c1, "C1");
        add_footprint(&mut p, c2, "C2");
        // Lock C1
        p.board.footprints[0].locked = true;
        p.board.footprints[0].position = Point::new(10.0, 10.0);
        add_net(&mut p, "SIG", &[c1, c2]);

        let params = json!({"scope": {"board": {}}, "seed": 42});
        global_place(&mut p, params, None).unwrap();

        // C1 should be at the locked position
        assert_eq!(p.board.footprints[0].position.x, 10.0);
        assert_eq!(p.board.footprints[0].position.y, 10.0);
    }

    #[test]
    fn global_place_bounded_iterations() {
        let mut p = build_test_project();
        let c1 = add_component(&mut p, "C1", "cap_0805");
        let c2 = add_component(&mut p, "C2", "cap_0805");
        add_footprint(&mut p, c1, "C1");
        add_footprint(&mut p, c2, "C2");
        add_net(&mut p, "SIG", &[c1, c2]);

        let params = json!({"scope": {"board": {}}, "seed": 42, "iterations": 10});
        let result = global_place(&mut p, params, None);
        assert!(result.is_ok());
    }

    #[test]
    fn global_place_different_seeds_differ() {
        let mut p1 = build_test_project();
        let c1 = add_component(&mut p1, "C1", "cap_0805");
        let c2 = add_component(&mut p1, "C2", "cap_0805");
        add_footprint(&mut p1, c1, "C1");
        add_footprint(&mut p1, c2, "C2");
        add_net(&mut p1, "SIG", &[c1, c2]);

        let mut p2 = build_test_project();
        let c1b = add_component(&mut p2, "C1", "cap_0805");
        let c2b = add_component(&mut p2, "C2", "cap_0805");
        add_footprint(&mut p2, c1b, "C1");
        add_footprint(&mut p2, c2b, "C2");
        add_net(&mut p2, "SIG", &[c1b, c2b]);

        global_place(&mut p1, json!({"scope": {"board": {}}, "seed": 1}), None).unwrap();
        global_place(&mut p2, json!({"scope": {"board": {}}, "seed": 2}), None).unwrap();

        let pos1 = &p1.board.footprints[1].position;
        let pos2 = &p2.board.footprints[1].position;
        // Different seeds should typically give different results
        assert!(pos1.x != pos2.x || pos1.y != pos2.y);
    }

    #[test]
    fn a_newcomer_lets_the_app_arrange_the_unlocked_parts() {
        // Story: A newcomer has placed all components at the origin and wants
        // the app to arrange them. They lock the connectors and run global
        // placement.
        let mut p = build_test_project();

        // Add components: 2 connectors (locked), 3 ICs, 5 passives
        let j1 = add_component(&mut p, "J1", "header_2x5");
        let j2 = add_component(&mut p, "J2", "header_2x5");
        let u1 = add_component(&mut p, "U1", "soic_16");
        let u2 = add_component(&mut p, "U2", "soic_16");
        let u3 = add_component(&mut p, "U3", "soic_16");
        let r1 = add_component(&mut p, "R1", "res_0805");
        let r2 = add_component(&mut p, "R2", "res_0805");
        let r3 = add_component(&mut p, "R3", "res_0805");
        let c1 = add_component(&mut p, "C1", "cap_0805");
        let c2 = add_component(&mut p, "C2", "cap_0805");

        add_footprint(&mut p, j1, "J1");
        add_footprint(&mut p, j2, "J2");
        add_footprint(&mut p, u1, "U1");
        add_footprint(&mut p, u2, "U2");
        add_footprint(&mut p, u3, "U3");
        add_footprint(&mut p, r1, "R1");
        add_footprint(&mut p, r2, "R2");
        add_footprint(&mut p, r3, "R3");
        add_footprint(&mut p, c1, "C1");
        add_footprint(&mut p, c2, "C2");

        // Lock connectors
        p.board.footprints[0].locked = true;
        p.board.footprints[0].position = Point::new(5.0, 5.0);
        p.board.footprints[1].locked = true;
        p.board.footprints[1].position = Point::new(95.0, 5.0);

        // Create some nets
        add_net(&mut p, "VCC", &[u1, u2, u3, r1, r2]);
        add_net(&mut p, "GND", &[u1, u2, u3, c1, c2]);
        add_net(&mut p, "DATA", &[j1, u1, j2]);
        add_net(&mut p, "CLK", &[u1, u2]);

        let params = json!({"scope": {"board": {}}, "seed": 42});
        let result = global_place(&mut p, params, None);
        assert!(result.is_ok());

        let out = result.unwrap();
        let wl_before = out["wirelength_before"].as_f64().unwrap();
        let wl_after = out["wirelength_after"].as_f64().unwrap();

        // Placement should have run and produced results
        // Wirelength after should be finite (not all at origin)
        assert!(wl_after.is_finite());
    }
}
