//! Move a power or ground port together with the wire ends that touch it.
//!
//! # Rules
//! - Only a power port moves with this command; `label.move` stays for the
//!   other label kinds.
//! - The wire ends on the port follow it, a bend keeps straight wires square,
//!   and a drop on a pin or wire of another net is refused.
//! - One command is one undo step; a refused edit leaves the project alone.
//!
//! # Dependencies
//! - crates/doc/src/power_port.rs (`Project::move_power_port` holds the rule)
//!
//! # Do not
//! - Copy the wire-follow rule here; call the project method.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PowerPortMoveParams {
    id: Id,
    position: Point,
}

/// The id and params of this module's one command.
pub(super) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "power_port.move",
        description: "Move a power or ground port to a new point; the wire ends that touch it follow so the connection is kept, a port dropped on a pin or wire with no net joins that net, and a drop on another net is refused; one undo step",
        input_schema: schema(
            json!({"id":id_schema(),"position":point_schema()}),
            &["id", "position"],
        ),
    }]
}

/// Dispatch for the power port command; `None` when `command` is not it.
pub(super) fn run(p: &mut Project, command: &str, params: &Value) -> Option<Result<Value, Error>> {
    if command != "power_port.move" {
        return None;
    }
    Some(move_port(p, params))
}

fn move_port(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: PowerPortMoveParams = parse(params)?;
    let bare = p
        .move_power_port(a.id, a.position)
        .map_err(Error::Invalid)?;
    let net = p
        .labels
        .iter()
        .find(|l| l.id == a.id)
        .map(|l| l.net)
        .ok_or_else(missing)?;
    if !bare.is_empty() {
        assign(p, net, &bare)?;
    }
    Ok(json!({"id": a.id, "joined": bare.len()}))
}
