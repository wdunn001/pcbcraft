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
//!   is what is still open after existing copper, from `pcbcraft_rules::airwires`.
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

/// True for the layout read commands.
pub(super) fn handles(command: &str) -> bool {
    command == "layout.report"
}

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "layout.report",
        description: "Read-only layout report for a scope: part positions and boxes, nearest-neighbour gaps, alignment rows and columns, per-net half-perimeter and ratsnest length with totals, parts off the board and overlapping parts. Deterministic JSON; changes nothing",
        input_schema: schema(json!({"scope": scope::schema()}), &["scope"]),
    }]
}

/// Run a layout read command.
pub(super) fn run(p: &Project, command: &str, params: &Value) -> Result<Value, Error> {
    match command {
        "layout.report" => {
            let scope = Scope::from_value(p, params.get("scope").unwrap_or(&Value::Null))?;
            report(p, &scope)
        }
        _ => Err(Error::Invalid(format!("unknown layout command: {command}"))),
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

    let unrouted: BTreeMap<Id, f64> =
        pcbcraft_rules::airwires(p)
            .iter()
            .fold(BTreeMap::new(), |mut m, w| {
                *m.entry(w.net).or_insert(0.0) += w.from.distance(w.to);
                m
            });
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
    let (mut total_hpwl, mut total_rats, mut total_open) = (0.0, 0.0, 0.0);
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
        let open = unrouted.get(&net).copied().unwrap_or(0.0);
        total_hpwl += h;
        total_rats += rats;
        total_open += open;
        nets.push((
            p.net_name(net).to_string(),
            net,
            json!({"net": net, "name": p.net_name(net), "pads": points.len(),
                   "hpwl_mm": r(h), "ratsnest_mm": r(rats), "unrouted_mm": r(open)}),
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
        "totals": {"hpwl_mm": r(total_hpwl), "ratsnest_mm": r(total_rats), "unrouted_mm": r(total_open)},
        "off_board": off_board,
        "overlaps": overlapping,
    }))
}
