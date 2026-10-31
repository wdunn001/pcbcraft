//! The shared `Scope` that layout commands act on.
//!
//! Context: placement and report commands take one `scope` object instead of
//! each inventing its own filter. Objects outside the scope are obstacles:
//! they are read but never moved. Issue: layout work, fork issue 97.
//!
//! Local rules:
//! - A scope names layers, nets, components or any mix. Each list that is
//!   given narrows the scope (a part must match every list that is given).
//! - An empty scope (all three lists empty) is refused. There is no implicit
//!   whole board; to mean the whole board, list its layers.
//! - `respect_locked` defaults to true: locked footprints are in scope for
//!   reading but `Scope::may_move` says no.
//! - Every list is capped at `MAX_ENTRIES`; named ids and layers must exist.
//!
//! Dependencies: `crates/doc/src/lib.rs` (`Project`, `Footprint`).
use super::Error;
use pcbcraft_doc::{Footprint, Id, Project};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Most entries allowed in each scope list.
pub const MAX_ENTRIES: usize = 1024;

fn yes() -> bool {
    true
}

/// Which footprints a layout command may read or move.
///
/// # Rules
/// - Build it with serde from command params, then call `validate` before use.
/// - Membership is by footprint: layer is the side it is mounted on, net is any
///   pad on that net, component is the part id.
///
/// # Dependencies
/// - crates/engine/src/layout.rs is the first user; later placers reuse it.
///
/// # Do not
/// - Treat an empty scope as the whole board.
/// - Move a footprint without asking `may_move`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    /// Layer names such as "F.Cu"; a part matches by the side it is mounted on.
    #[serde(default)]
    pub layers: Vec<String>,
    /// Net ids; a part matches when one of its pads is on one of them.
    #[serde(default)]
    pub nets: Vec<Id>,
    /// Component ids.
    #[serde(default)]
    pub components: Vec<Id>,
    /// When true (the default) locked footprints are never moved.
    #[serde(default = "yes")]
    pub respect_locked: bool,
}

/// The JSON schema of a scope, for command input schemas.
pub fn schema() -> Value {
    json!({
        "type":"object",
        "description":"What the command may touch: at least one of layers, nets, components. Empty is refused. Outside objects are obstacles and never move",
        "properties":{
            "layers":{"type":"array","items":{"type":"string"},"maxItems":MAX_ENTRIES,"description":"copper layer names; matches parts mounted on that side"},
            "nets":{"type":"array","items":{"type":"integer","minimum":1},"maxItems":MAX_ENTRIES,"description":"net ids; matches parts with a pad on the net"},
            "components":{"type":"array","items":{"type":"integer","minimum":1},"maxItems":MAX_ENTRIES,"description":"component ids"},
            "respect_locked":{"type":"boolean","default":true,"description":"locked parts are never moved"}
        },
        "additionalProperties":false
    })
}

impl Scope {
    /// Parse and validate a scope from command params.
    pub fn from_value(project: &Project, v: &Value) -> Result<Self, Error> {
        let scope: Scope =
            serde_json::from_value(v.clone()).map_err(|e| Error::Invalid(format!("scope: {e}")))?;
        scope.validate(project)?;
        Ok(scope)
    }

    /// Refuse an empty scope, an oversized list, or a name that does not exist.
    pub fn validate(&self, project: &Project) -> Result<(), Error> {
        if self.layers.is_empty() && self.nets.is_empty() && self.components.is_empty() {
            return Err(Error::Invalid(
                "scope is empty: name at least one layer, net or component".into(),
            ));
        }
        if self.layers.len() > MAX_ENTRIES
            || self.nets.len() > MAX_ENTRIES
            || self.components.len() > MAX_ENTRIES
        {
            return Err(Error::Invalid(format!(
                "scope lists hold at most {MAX_ENTRIES} entries each"
            )));
        }
        let layer_names: Vec<String> = project
            .copper_layers()
            .into_iter()
            .map(|l| project.layer_name(l))
            .collect();
        for l in &self.layers {
            if !layer_names.contains(l) {
                return Err(Error::Invalid(format!("scope: unknown layer {l}")));
            }
        }
        for n in &self.nets {
            if !project.nets.iter().any(|x| x.id == *n) {
                return Err(Error::Invalid(format!("scope: unknown net {n}")));
            }
        }
        for c in &self.components {
            if project.component(*c).is_none() {
                return Err(Error::Invalid(format!("scope: unknown component {c}")));
            }
        }
        Ok(())
    }

    /// True when the footprint is inside the scope.
    pub fn contains(&self, project: &Project, f: &Footprint) -> bool {
        (self.layers.is_empty() || self.layers.contains(&project.layer_name(f.board_side())))
            && (self.components.is_empty() || self.components.contains(&f.component))
            && (self.nets.is_empty()
                || f.pads
                    .iter()
                    .any(|p| p.net.is_some_and(|n| self.nets.contains(&n))))
    }

    /// True when a command may move the footprint: in scope, and not locked
    /// while `respect_locked` is on.
    pub fn may_move(&self, project: &Project, f: &Footprint) -> bool {
        self.contains(project, f) && !(self.respect_locked && f.locked == Some(true))
    }
}
