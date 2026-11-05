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

use crate::route::Mode;

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

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ScopeKind {
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
                "layer":{"type":"string","maxLength":64,"description":"copper layer name when scope is layer; defaults to the top copper layer"},
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
    _next: &mut Id,
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

    let _mode = a.mode.unwrap_or(Mode::Octilinear);

    let _layer = match a.scope {
        ScopeKind::Net => None,
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

    // Shell: return the result shape with completed 0 for now.
    Some(Ok(json!({
        "scope": match a.scope {
            ScopeKind::Net => "net",
            ScopeKind::Layer => "layer",
            ScopeKind::Board => "board",
        },
        "completed": 0,
        "failed": [],
        "attempts": 0,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

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
        assert!(r["result"]["failed"].is_array());
        assert_eq!(r["result"]["failed"].as_array().unwrap().len(), 0);
        assert_eq!(r["result"]["attempts"], json!(0));

        Ok(())
    }
}
