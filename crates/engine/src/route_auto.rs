//! Typed `route.auto` engine command: auto-route nets in a bounded scope.
//!
//! Takes a scope (net, layer, or board), optional net list, routing mode, and
//! a bounded rip-up count. Loops the existing router over unrouted connections
//! in scope, in net-class priority order. Returns completion count, failures
//! with the blocking rule, and clearance-check delta. One undo reverses the
//! whole run.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs
//! - crates/engine/src/route/mod.rs
//! - crates/doc/src/lib.rs
//! - crates/geom/src/lib.rs

use super::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

use crate::route::Mode;
use pcbcraft_rules::{airwires, layer_anchor};

/// One open (unrouted) connection that needs a track.
///
/// # Rules
/// - `net` is the net that must be connected.
/// - `from` and `to` are the copper endpoints (pad centers or existing track ends).
/// - Built by `open_connections`, which filters airwires by scope and sorts them.
///
/// # Do not
/// - Mutate after creation; treat as immutable routing input.
#[derive(Clone, Debug)]
struct Connection {
    net: Id,
    from: Point,
    to: Point,
}

/// List the open connections in scope in a fixed order.
///
/// # Rules
/// - Takes the board airwires and filters by scope: `net` keeps only wires whose
///   net is in the `nets` list; `layer` and `board` keep all wires (layer filtering
///   is done by the router refusing pads not on the layer).
/// - Sorts by (net class id or 0, net id, from.x, from.y, to.x, to.y) using
///   total_cmp so the order is deterministic for identical input.
/// - Caps the result at 4096 connections; when more exist, the first 4096 are
///   returned and the caller must report that more remain.
/// - Pure function: reads the project, returns a new vec, never mutates.
///
/// # Dependencies
/// - crates/rules/src/lib.rs (`airwires`)
/// - crates/doc/src/lib.rs (`Project::net_class_id`)
/// - crates/geom/src/lib.rs (`Point`)
///
/// # Do not
/// - Change the sort order; routing must be deterministic for the same input.
/// - Grow the cap without a ticket; 4096 is the bounded limit.
fn open_connections(p: &Project, scope: &str, nets: &[Id]) -> Vec<Connection> {
    let wires = airwires(p);
    let mut result: Vec<Connection> = wires
        .into_iter()
        .filter(|w| {
            if scope == "net" {
                nets.contains(&w.net)
            } else {
                true
            }
        })
        .map(|w| Connection {
            net: w.net,
            from: w.from,
            to: w.to,
        })
        .collect();

    result.sort_by(|a, b| {
        let class_a = p.net_class_id(a.net).unwrap_or(0);
        let class_b = p.net_class_id(b.net).unwrap_or(0);
        if class_a != class_b {
            return class_a.cmp(&class_b);
        }
        if a.net != b.net {
            return a.net.cmp(&b.net);
        }
        let cmp = a.from.x.total_cmp(&b.from.x);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }
        let cmp = a.from.y.total_cmp(&b.from.y);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }
        let cmp = a.to.x.total_cmp(&b.to.x);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }
        a.to.y.total_cmp(&b.to.y)
    });

    if result.len() > 4096 {
        result.truncate(4096);
    }

    result
}

/// Maximum rip-up attempts per net in one auto-route pass.
pub const MAX_RIPUPS: u32 = 1000;

fn default_max_ripups() -> u32 {
    50
}

fn default_keep_existing() -> bool {
    true
}

fn default_seed() -> u64 {
    0
}

/// Find the first new error that appeared after routing.
///
/// # Rules
/// - Compares two DRC error sets by code and the objects involved, and returns the code of
///   the first error in `after` that is not in `before`, in sorted order for determinism.
/// - A second error of a code that was already present counts as new when it involves
///   other objects.
/// - Returns `None` if no new errors appeared.
///
/// # Dependencies
/// - crates/engine/src/route/mod.rs (`errors`)
///
/// # Do not
/// - Use for anything other than detecting new DRC errors from routing.
fn first_new_error(
    before: &BTreeSet<(&'static str, Vec<Id>)>,
    after: &BTreeSet<(&'static str, Vec<Id>)>,
) -> Option<&'static str> {
    after.difference(before).next().map(|(code, _)| *code)
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScopeKind {
    Net,
    Layer,
    Board,
}

/// Parsed and validated `route.auto` command parameters.
///
/// # Rules
/// - `scope` is required and must be `net`, `layer`, or `board`.
/// - `nets` is required and non-empty when `scope` is `net`, at most 1024 ids.
/// - `layer` resolves to an existing copper layer; defaults to the top copper.
/// - `max_ripups` must be in `(0, MAX_RIPUPS]`.
/// - Identical input and seed produce identical copper.
///
/// # Dependencies
/// - crates/engine/src/lib.rs (`resolve_layer_param`)
///
/// # Do not
/// - Change the `deny_unknown_fields` attribute.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteAutoParams {
    pub scope: ScopeKind,
    #[serde(default)]
    pub nets: Vec<Id>,
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(default)]
    pub mode: Option<Mode>,
    #[serde(default = "default_max_ripups")]
    pub max_ripups: u32,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default = "default_keep_existing")]
    pub keep_existing: bool,
}

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "route.auto",
        description: "Auto-route nets in scope, rip up and retry failed nets up to the given count, then report which nets are still unrouted and which rule blocked them; identical input and seed produce identical copper",
        input_schema: schema(
            json!({
                "scope":{"type":"string","enum":["net","layer","board"],"description":"what to route: specific nets, one layer, or the whole board"},
                "nets":{"type":"array","items":{"type":"integer","minimum":1},"minItems":1,"maxItems":1024,"description":"net ids when scope is net; required and non-empty then"},
                "layer":{"type":"string","maxLength":64,"description":"copper layer name for scope layer or for net scope to constrain routing to a single layer; defaults to the top copper layer"},
                "mode":{"type":"string","enum":["orthogonal","octilinear","octilinear_shove"],"description":"how to route; octilinear walkaround with shove is the default"},
                "max_ripups":{"type":"integer","minimum":1,"maximum":1000,"description":"how many times to rip up and retry a failing net; 50 by default"},
                "seed":{"type":"integer","minimum":0,"description":"random seed for tie-breaking; 0 by default, same seed gives same result"},
                "keep_existing":{"type":"boolean","description":"do not rip up tracks the user already placed; true by default"}
            }),
            &["scope"],
        ),
    }]
}

pub(super) fn run(
    p: &mut Project,
    next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    if command != "route.auto" {
        return None;
    }
    let a: RouteAutoParams = match serde_json::from_value(params.clone()) {
        Ok(a) => a,
        Err(e) => return Some(Err(Error::Invalid(format!("route.auto params: {e}")))),
    };

    let mode = a.mode.unwrap_or(Mode::Octilinear);

    let layer = match a.scope {
        ScopeKind::Net => match &a.layer {
            Some(s) => match resolve_layer_param(p, s) {
                Ok(layer) => Some(layer),
                Err(e) => return Some(Err(e)),
            },
            None => None,
        },
        ScopeKind::Layer => {
            let resolved = match &a.layer {
                Some(s) => match resolve_layer_param(p, s) {
                    Ok(layer) => layer,
                    Err(e) => return Some(Err(e)),
                },
                None => {
                    let top = p.copper_layers().into_iter().next();
                    match top {
                        Some(top) => top,
                        None => {
                            return Some(Err(Error::Invalid("board has no copper layers".into())));
                        }
                    }
                }
            };
            Some(resolved)
        }
        ScopeKind::Board => None,
    };

    if a.scope == ScopeKind::Net && a.nets.is_empty() {
        return Some(Err(Error::Invalid(
            "route.auto: net scope requires a non-empty nets list".into(),
        )));
    }

    for net in &a.nets {
        if !p.nets.iter().any(|n| n.id == *net) {
            return Some(Err(Error::Invalid(format!(
                "route.auto: unknown net id {net}"
            ))));
        }
    }

    if a.max_ripups == 0 || a.max_ripups > MAX_RIPUPS {
        return Some(Err(Error::Invalid(format!(
            "route.auto: max_ripups must be between 1 and {MAX_RIPUPS}, got {}",
            a.max_ripups
        ))));
    }

    // Check for new DRC errors after routing; if any, refuse the whole run.
    let before = crate::route::errors(p);

    // Get the list of open connections to route.
    let scope_name = match a.scope {
        ScopeKind::Net => "net",
        ScopeKind::Layer => "layer",
        ScopeKind::Board => "board",
    };
    let connections = open_connections(p, scope_name, &a.nets);
    let open_count = connections.len();

    // Refuse if there is nothing to route after parsing the scope.
    if connections.is_empty() {
        return Some(Ok(json!({
            "scope": scope_name,
            "open_connections": 0,
            "completed": 0,
            "failed": [],
            "attempts": 0,
            "seed": a.seed,
            "keep_existing": a.keep_existing,
            "findings_before": before.len(),
            "new_findings": 0,
        })));
    }
    // Route each connection, saving and restoring state on failure.
    let mut completed = 0;
    let mut failed: Vec<Value> = vec![];
    let mut attempts = 0;

    for conn in &connections {
        attempts += 1;

        // Save current board state before attempting to route.
        let saved_tracks = p.board.tracks.clone();
        let saved_vias = p.board.vias.clone();
        let saved_next = *next;

        // Determine the width for this net.
        let width = crate::class_width(p, conn.net);

        let on =
            layer.unwrap_or_else(|| p.copper_layers().into_iter().next().unwrap_or(Layer::Front));
        // A surface-mount pad has copper on its own side alone, so a route on
        // the other layer cannot finish on it. Where the island already
        // carries a via, a through-hole pad or a track on this layer, aim at
        // that instead: it is the same copper, and it is reachable. Without
        // this the router laid a track that connected nothing, noticed the
        // airwire had not closed, and threw the work away.
        let from = layer_anchor(p, conn.from, conn.net, on).unwrap_or(conn.from);
        let to = layer_anchor(p, conn.to, conn.net, on).unwrap_or(conn.to);

        // Try to route this connection.
        let result = crate::route::run(p, next, from, to, conn.net, width, on, mode);

        match result {
            Ok(_) => {
                // Check if the connection is actually completed (airwire is gone).
                let still_open = airwires(p)
                    .iter()
                    .any(|w| w.net == conn.net && w.from == conn.from && w.to == conn.to);
                if still_open {
                    // The track was placed but the connection is not actually completed.
                    // This can happen when a track ends on a pad that's on a different layer.
                    // Undo the routing and mark as failed.
                    p.board.tracks = saved_tracks;
                    p.board.vias = saved_vias;
                    *next = saved_next;
                    failed.push(json!({
                        "net": conn.net,
                        "net_name": p.net_name(conn.net),
                        "from": json!({"x": conn.from.x, "y": conn.from.y}),
                        "to": json!({"x": conn.to.x, "y": conn.to.y}),
                        "reason": "track ended on pad not on this layer",
                    }));
                } else {
                    completed += 1;
                }
            }
            Err(e) => {
                // Routing failed; restore the board state and record the failure.
                p.board.tracks = saved_tracks;
                p.board.vias = saved_vias;
                *next = saved_next;

                failed.push(json!({
                    "net": conn.net,
                    "net_name": p.net_name(conn.net),
                    "from": json!({"x": conn.from.x, "y": conn.from.y}),
                    "to": json!({"x": conn.to.x, "y": conn.to.y}),
                    "reason": e.to_string(),
                }));
            }
        }
    }

    // Check for new DRC errors after routing.
    let after = crate::route::errors(p);
    if let Some(code) = first_new_error(&before, &after) {
        return Some(Err(Error::Invalid(format!(
            "route.auto would break the {} check; nothing was changed",
            code,
        ))));
    }

    Some(Ok(json!({
        "scope": scope_name,
        "open_connections": open_count,
        "completed": completed,
        "failed": failed,
        "attempts": attempts,
        "seed": a.seed,
        "keep_existing": a.keep_existing,
        "findings_before": before.len(),
        "new_findings": 0,
    })))
}

#[cfg(test)]
mod tests {
    /// Moves every footprint well inside the board edge so a route is not refused for clearance.
    fn move_footprints_inward(s: &mut Session) -> Result<(), Error> {
        s.execute("board.resize", json!({"width":100.0,"height":60.0}), None)?;
        let ids: Vec<u64> = s.project.board.footprints.iter().map(|f| f.id).collect();
        for (i, id) in ids.into_iter().enumerate() {
            let x = 20.0 + 20.0 * i as f64;
            s.execute(
                "footprint.move",
                json!({"id":id,"position":{"x":x,"y":30.0}}),
                None,
            )?;
        }
        Ok(())
    }

    use super::*;
    use crate::{Error, Session};

    #[test]
    fn route_auto_validates_parameters_and_returns_shell_result() -> Result<(), Error> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        let before = s.inspect();

        // net scope with no nets is refused
        let r = s.execute("route.auto", json!({"scope":"net"}), None);
        assert!(r.is_err(), "net scope requires nets");
        assert_eq!(s.inspect()["project"], before["project"]);

        // unknown net id is refused
        let r = s.execute("route.auto", json!({"scope":"net","nets":[999999]}), None);
        assert!(r.is_err(), "unknown net id refused");
        assert_eq!(s.inspect()["project"], before["project"]);

        // layer name that is not copper is refused
        let r = s.execute(
            "route.auto",
            json!({"scope":"layer","layer":"Nope.Cu"}),
            None,
        );
        assert!(r.is_err(), "non-copper layer refused");
        assert_eq!(s.inspect()["project"], before["project"]);

        // max_ripups 0 is refused
        let r = s.execute("route.auto", json!({"scope":"board","max_ripups":0}), None);
        assert!(r.is_err(), "max_ripups 0 refused");
        assert_eq!(s.inspect()["project"], before["project"]);

        // max_ripups above cap is refused
        let r = s.execute(
            "route.auto",
            json!({"scope":"board","max_ripups":1001}),
            None,
        );
        assert!(r.is_err(), "max_ripups above cap refused");
        assert_eq!(s.inspect()["project"], before["project"]);

        // unknown field is refused
        let r = s.execute("route.auto", json!({"scope":"board","typo":true}), None);
        assert!(r.is_err(), "unknown field refused");
        assert_eq!(s.inspect()["project"], before["project"]);

        // valid board scope on an empty board returns the shell result
        let r = s.execute("route.auto", json!({"scope":"board"}), None)?;
        assert_eq!(r["result"]["scope"], json!("board"));
        assert_eq!(r["result"]["completed"], json!(0));
        assert_eq!(r["result"]["open_connections"], json!(0));
        assert!(r["result"]["failed"].is_array());
        assert_eq!(r["result"]["failed"].as_array().map(Vec::len), Some(0));
        assert_eq!(r["result"]["seed"], json!(0));
        assert_eq!(r["result"]["keep_existing"], json!(true));
        assert_eq!(r["result"]["attempts"], json!(0));

        Ok(())
    }

    #[test]
    fn open_connections_are_in_scope_and_ordered() -> Result<(), Error> {
        // Build a 2-net fixture: N1 (id 1) and N2 (id 2)
        let mut s = Session::default();
        s.execute("board.resize", json!({"width":50,"height":50}), None)?;

        let r1 = s.execute("net.create", json!({"name":"N1"}), None)?;
        let n1 = r1["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("n1 id not found".into()))?;

        let r2 = s.execute("net.create", json!({"name":"N2"}), None)?;
        let n2 = r2["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("n2 id not found".into()))?;

        // Part 1: two pads on N1
        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","position":{"x":10,"y":10}}),
            None,
        )?;
        let p1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p1 id not found".into()))?;
        s.execute(
            "net.connect",
            json!({"net":n1,"pins":[{"component":p1,"pin":"1"},{"component":p1,"pin":"2"}]}),
            None,
        )?;

        // Part 2: two pads on N2
        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","position":{"x":20,"y":20}}),
            None,
        )?;
        let p2 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p2 id not found".into()))?;
        s.execute(
            "net.connect",
            json!({"net":n2,"pins":[{"component":p2,"pin":"1"},{"component":p2,"pin":"2"}]}),
            None,
        )?;

        s.execute("board.sync", json!({}), None)?;
        move_footprints_inward(&mut s)?;

        // Board scope: both nets appear, N1 first (smaller id)
        let conns = open_connections(&s.project, "board", &[]);
        assert_eq!(conns.len(), 2, "board scope lists both open connections");
        assert_eq!(conns[0].net, n1, "N1 comes first by net id");
        assert_eq!(conns[1].net, n2, "N2 comes second");

        // Net scope: only N2 when asked
        let conns = open_connections(&s.project, "net", &[n2]);
        assert_eq!(conns.len(), 1, "net scope filters to requested nets");
        assert_eq!(conns[0].net, n2, "only N2 appears");

        Ok(())
    }

    #[test]
    fn route_auto_completes_a_fixture_board() -> Result<(), Error> {
        // Build a simple board with one net that can be routed.
        let mut s = Session::default();
        s.project.auto_sync = false;
        s.execute("board.resize", json!({"width":40,"height":30}), None)?;

        // Create a net and two components with pads on that net.
        let r = s.execute("net.create", json!({"name":"N1"}), None)?;
        let n1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("n1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","position":{"x":10,"y":10}}),
            None,
        )?;
        let p1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","position":{"x":30,"y":10}}),
            None,
        )?;
        let p2 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p2 id not found".into()))?;

        // Connect pads to the net.
        s.execute(
            "net.connect",
            json!({"net":n1,"pins":[{"component":p1,"pin":"1"},{"component":p2,"pin":"1"}]}),
            None,
        )?;

        s.execute("board.sync", json!({}), None)?;
        move_footprints_inward(&mut s)?;

        // Run route.auto on the board.
        let r = s.execute("route.auto", json!({"scope":"board"}), None)?;
        let result = &r["result"];

        // Expect one completed connection.
        assert_eq!(result["completed"], json!(1), "one connection routed");
        assert_eq!(result["attempts"], json!(1), "one attempt made");
        assert_eq!(
            result["open_connections"],
            json!(1),
            "one open connection found"
        );

        // Expect no failures.
        let failed = result["failed"]
            .as_array()
            .ok_or_else(|| Error::Invalid("failed is not an array".into()))?;
        assert_eq!(failed.len(), 0, "no failed connections");

        // Expect tracks to have been created.
        assert!(
            !s.project.board.tracks.is_empty(),
            "tracks were created by routing"
        );

        Ok(())
    }

    /// Two resistors on one net, parked well inside a 100 by 60 board. Their pads are
    /// front-side surface-mount pads, so they have copper on the front layer only.
    fn two_resistors_on_one_net() -> Result<(Session, u64), Error> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        s.execute("board.resize", json!({"width":40,"height":30}), None)?;
        let r = s.execute("net.create", json!({"name":"N1"}), None)?;
        let net = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("net id not found".into()))?;
        let mut parts = Vec::new();
        for (reference, x) in [("R1", 10), ("R2", 30)] {
            let r = s.execute(
                "component.add",
                json!({"kind":"resistor","reference":reference,"position":{"x":x,"y":10}}),
                None,
            )?;
            parts.push(
                r["result"]["id"]
                    .as_u64()
                    .ok_or_else(|| Error::Invalid("part id not found".into()))?,
            );
        }
        s.execute(
            "net.connect",
            json!({"net":net,"pins":[
                {"component":parts[0],"pin":"1"},
                {"component":parts[1],"pin":"1"}
            ]}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        move_footprints_inward(&mut s)?;
        Ok((s, net))
    }

    #[test]
    fn route_auto_net_scope_routes_on_the_requested_layer() -> Result<(), Error> {
        // The pads are on the front, so asking for the front layer completes the connection
        // and every track it laid is on that layer.
        let (mut s, net) = two_resistors_on_one_net()?;
        let r = s.execute(
            "route.auto",
            json!({"scope":"net","nets":[net],"layer":"F.Cu"}),
            None,
        )?;
        let result = &r["result"];
        assert_eq!(
            result["completed"],
            json!(1),
            "one connection routed: {result}"
        );
        assert_eq!(result["failed"], json!([]), "nothing failed");
        let tracks = &s.project.board.tracks;
        assert!(!tracks.is_empty(), "the route laid copper");
        assert!(
            tracks.iter().all(|t| t.layer == Layer::Front),
            "every track is on the requested layer"
        );
        Ok(())
    }

    #[test]
    fn route_auto_net_scope_refuses_a_layer_its_pads_do_not_reach() -> Result<(), Error> {
        // Asking for the back layer cannot join two front-only pads. The run reports the
        // connection as failed, leaves the board as it was, and never calls it completed.
        let (mut s, net) = two_resistors_on_one_net()?;
        let before = s.project.board.tracks.len();
        let r = s.execute(
            "route.auto",
            json!({"scope":"net","nets":[net],"layer":"B.Cu"}),
            None,
        )?;
        let result = &r["result"];
        assert_eq!(result["completed"], json!(0), "not completed: {result}");
        assert_eq!(
            result["failed"].as_array().map(Vec::len),
            Some(1),
            "the connection is reported failed: {result}"
        );
        assert_eq!(s.project.board.tracks.len(), before, "no copper was kept");
        assert!(
            pcbcraft_rules::drc(&s.project)
                .iter()
                .all(|f| f.code != "track_on_different_layer_pad"),
            "no dead copper is left behind for the check to find"
        );
        Ok(())
    }

    #[test]
    fn route_auto_hard_rule_failure_reports_blocked() -> Result<(), Error> {
        // Create a board where routing may fail due to clearance.
        let mut s = Session::default();
        s.project.auto_sync = false;
        s.execute("board.resize", json!({"width":40,"height":30}), None)?;

        let r = s.execute("net.create", json!({"name":"N1"}), None)?;
        let n1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("n1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","position":{"x":10,"y":10}}),
            None,
        )?;
        let p1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","position":{"x":30,"y":10}}),
            None,
        )?;
        let p2 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p2 id not found".into()))?;

        s.execute(
            "net.connect",
            json!({"net":n1,"pins":[{"component":p1,"pin":"1"},{"component":p2,"pin":"1"}]}),
            None,
        )?;

        s.execute("board.sync", json!({}), None)?;
        move_footprints_inward(&mut s)?;

        // Run route.auto - should either route or report failure
        let r = s.execute("route.auto", json!({"scope":"board"}), None)?;
        let result = &r["result"];

        // Either it completes or it fails with a reason
        let completed = result["completed"].as_u64().unwrap_or(0);
        let failed = result["failed"].as_array().map(|a| a.len()).unwrap_or(0);
        assert!(
            completed == 1 || failed == 1,
            "either routed or reported failure"
        );

        // If it failed, it should have a reason
        if failed == 1 {
            let failures = result["failed"]
                .as_array()
                .ok_or_else(|| Error::Invalid("failed is not an array".into()))?;
            let first = failures
                .first()
                .ok_or_else(|| Error::Invalid("one failure expected".into()))?;
            assert!(first["reason"].is_string(), "failure has a reason string");
        }

        Ok(())
    }

    #[test]
    fn route_auto_one_undo_reverses_whole_run() -> Result<(), Error> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        s.execute("board.resize", json!({"width":40,"height":30}), None)?;

        let r = s.execute("net.create", json!({"name":"N1"}), None)?;
        let n1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("n1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","position":{"x":10,"y":10}}),
            None,
        )?;
        let p1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","position":{"x":30,"y":10}}),
            None,
        )?;
        let p2 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p2 id not found".into()))?;

        s.execute(
            "net.connect",
            json!({"net":n1,"pins":[{"component":p1,"pin":"1"},{"component":p2,"pin":"1"}]}),
            None,
        )?;

        s.execute("board.sync", json!({}), None)?;
        move_footprints_inward(&mut s)?;

        // Run route.auto
        let r = s.execute("route.auto", json!({"scope":"board"}), None)?;
        let result = &r["result"];
        assert_eq!(result["completed"], json!(1), "one connection routed");

        // Verify tracks exist
        let tracks_before_undo = s.project.board.tracks.len();
        assert!(tracks_before_undo > 0, "tracks exist after routing");

        // Undo once
        s.undo()?;

        // Verify tracks are gone
        assert!(
            s.project.board.tracks.is_empty(),
            "one undo reverses all routing"
        );

        Ok(())
    }

    #[test]
    fn route_auto_dry_run_does_not_change_board() -> Result<(), Error> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        s.execute("board.resize", json!({"width":40,"height":30}), None)?;

        let r = s.execute("net.create", json!({"name":"N1"}), None)?;
        let n1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("n1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","position":{"x":10,"y":10}}),
            None,
        )?;
        let p1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","position":{"x":30,"y":10}}),
            None,
        )?;
        let p2 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p2 id not found".into()))?;

        s.execute(
            "net.connect",
            json!({"net":n1,"pins":[{"component":p1,"pin":"1"},{"component":p2,"pin":"1"}]}),
            None,
        )?;

        s.execute("board.sync", json!({}), None)?;
        move_footprints_inward(&mut s)?;

        // Dry run
        let dry = s.dry_run(
            &[crate::Command {
                command: "route.auto".into(),
                params: json!({"scope":"board"}),
            }],
            None,
        )?;
        let result = &dry
            .results
            .first()
            .ok_or_else(|| Error::Invalid("no dry run result".into()))?;

        // Dry run should report what would be done
        assert_eq!(result["open_connections"], json!(1));

        // But no tracks should be created
        assert!(
            s.project.board.tracks.is_empty(),
            "dry run does not create tracks"
        );

        Ok(())
    }

    #[test]
    fn route_auto_deterministic_with_same_seed() -> Result<(), Error> {
        // Run route.auto twice with the same seed and verify identical results.
        fn run_with_seed(seed: u64) -> Result<(u64, u64), Error> {
            let mut s = Session::default();
            s.project.auto_sync = false;
            s.execute("board.resize", json!({"width":40,"height":30}), None)?;

            let r = s.execute("net.create", json!({"name":"N1"}), None)?;
            let n1 = r["result"]["id"]
                .as_u64()
                .ok_or_else(|| Error::Invalid("n1 id not found".into()))?;

            let r = s.execute(
                "component.add",
                json!({"kind":"resistor","reference":"R1","position":{"x":10,"y":10}}),
                None,
            )?;
            let p1 = r["result"]["id"]
                .as_u64()
                .ok_or_else(|| Error::Invalid("p1 id not found".into()))?;

            let r = s.execute(
                "component.add",
                json!({"kind":"resistor","reference":"R2","position":{"x":30,"y":10}}),
                None,
            )?;
            let p2 = r["result"]["id"]
                .as_u64()
                .ok_or_else(|| Error::Invalid("p2 id not found".into()))?;

            s.execute(
                "net.connect",
                json!({"net":n1,"pins":[{"component":p1,"pin":"1"},{"component":p2,"pin":"1"}]}),
                None,
            )?;

            s.execute("board.sync", json!({}), None)?;
            move_footprints_inward(&mut s)?;

            let r = s.execute("route.auto", json!({"scope":"board","seed":seed}), None)?;
            let result = &r["result"];

            let completed = result["completed"].as_u64().unwrap_or(0);
            let attempts = result["attempts"].as_u64().unwrap_or(0);
            Ok((completed, attempts))
        }

        let (c1, a1) = run_with_seed(42)?;
        let (c2, a2) = run_with_seed(42)?;

        assert_eq!(c1, c2, "same seed gives same completed count");
        assert_eq!(a1, a2, "same seed gives same attempt count");

        Ok(())
    }

    #[test]
    fn route_auto_result_is_clearance_clean() -> Result<(), Error> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        s.execute("board.resize", json!({"width":40,"height":30}), None)?;

        let r = s.execute("net.create", json!({"name":"N1"}), None)?;
        let n1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("n1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","position":{"x":10,"y":10}}),
            None,
        )?;
        let p1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","position":{"x":30,"y":10}}),
            None,
        )?;
        let p2 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p2 id not found".into()))?;

        s.execute(
            "net.connect",
            json!({"net":n1,"pins":[{"component":p1,"pin":"1"},{"component":p2,"pin":"1"}]}),
            None,
        )?;

        s.execute("board.sync", json!({}), None)?;
        move_footprints_inward(&mut s)?;

        let before = crate::route::errors(&s.project);

        let r = s.execute("route.auto", json!({"scope":"board"}), None)?;
        assert_eq!(r["result"]["completed"], json!(1), "one connection routed");

        let after = crate::route::errors(&s.project);

        assert!(
            first_new_error(&before, &after).is_none(),
            "routing must not introduce new DRC errors"
        );

        Ok(())
    }

    #[test]
    fn route_auto_refuses_a_hard_rule_failure_and_changes_nothing() -> Result<(), Error> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        s.execute("board.resize", json!({"width":40,"height":30}), None)?;

        let r = s.execute("net.create", json!({"name":"N1"}), None)?;
        let n1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("n1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","position":{"x":10,"y":10}}),
            None,
        )?;
        let p1 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p1 id not found".into()))?;

        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","position":{"x":30,"y":10}}),
            None,
        )?;
        let p2 = r["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("p2 id not found".into()))?;

        s.execute(
            "net.connect",
            json!({"net":n1,"pins":[{"component":p1,"pin":"1"},{"component":p2,"pin":"1"}]}),
            None,
        )?;

        s.execute("board.sync", json!({}), None)?;
        move_footprints_inward(&mut s)?;

        let before =
            serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;

        let r = s.execute(
            "route.auto",
            json!({"scope":"layer","layer":"Nope.Cu"}),
            None,
        );
        assert!(r.is_err(), "invalid layer must be refused");

        let r = s.execute("route.auto", json!({"scope":"board","max_ripups":0}), None);
        assert!(r.is_err(), "max_ripups 0 must be refused");

        let after = serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;
        assert_eq!(before, after, "project must not change on refusal");

        Ok(())
    }

    #[test]
    fn first_new_error_detects_new_codes() {
        let before: BTreeSet<(&'static str, Vec<u64>)> = BTreeSet::from([("clearance", vec![1])]);
        let after_same: BTreeSet<(&'static str, Vec<u64>)> =
            BTreeSet::from([("clearance", vec![1])]);
        let after_new: BTreeSet<(&'static str, Vec<u64>)> =
            BTreeSet::from([("clearance", vec![1]), ("short", vec![2])]);

        assert_eq!(
            first_new_error(&before, &after_same),
            None,
            "no new errors detected"
        );
        assert_eq!(
            first_new_error(&before, &after_new),
            Some("short"),
            "new error code detected"
        );
        let after_more: BTreeSet<(&'static str, Vec<u64>)> =
            BTreeSet::from([("clearance", vec![1]), ("clearance", vec![3])]);
        assert_eq!(
            first_new_error(&before, &after_more),
            Some("clearance"),
            "another error of a code that was already present is new"
        );
    }
}
