//! Reconnect one end of a drawn wire onto a pin, joining the wire to that pin's
//! net. Used when the user drags a wire end onto a different pin.
//!
//! # Rules
//! - The moved vertex snaps to the pin position; junction validation applies
//!   exactly as in `wire.set_point` (a junction sitting on the old vertex would
//!   be left off the wire).
//! - If the pin already has a net and the wire has a different net, the wire's
//!   net is merged into the pin's net (the pin's net is kept).
//! - If the pin has no net, it is assigned the wire's net.
//! - One command is one undo step; a refused edit leaves the project untouched.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs (Session, Project, Id, Error, PinRef, parse, merge_net_into)
//! - crates/doc/src/lib.rs (Project::pin, Project::pin_position)
//! - crates/geom/src/lib.rs (Point)
//!
//! # Do not
//! - Call this on a `wire.connect` pin-to-pin record; it is only for drawn wires.

use super::*;
use pcbcraft_doc::PinRef;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconnectParams {
    id: Id,
    end: String,
    pin: PinRef,
}

/// The id and params of this module's one command.
pub(super) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "wire.reconnect_end",
        description: "Drag one end of a drawn wire onto a pin: move the end vertex to the pin and join the wire to that pin's net, merging nets if they differ; one undo step",
        input_schema: schema(
            json!({"id":id_schema(),"end":{"type":"string","enum":["start","end"]},"pin":pin_schema()}),
            &["id", "end", "pin"],
        ),
    }]
}

/// Dispatch for the wire reconnect command; `None` when `command` is not one.
pub(super) fn run(
    p: &mut Project,
    _next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    if !commands().iter().any(|c| c.id == command) {
        return None;
    }
    Some(reconnect_end(p, params))
}

fn reconnect_end(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: ReconnectParams = parse(params)?;
    let (vertex_index, is_end) = match a.end.as_str() {
        "start" => (Some(0), false),
        "end" => (None, true),
        other => {
            return Err(Error::Invalid(format!(
                "end must be start or end, not {other}"
            )));
        }
    };
    let pin = p
        .pin(&a.pin)
        .ok_or_else(|| Error::Invalid("pin not found".into()))?;
    let pin_net = pin.net;
    let pin_pos = p
        .pin_position(&a.pin)
        .ok_or_else(|| Error::Invalid("pin has no position".into()))?;
    let wire = p
        .drawn_wires
        .iter()
        .find(|w| w.id == a.id)
        .ok_or_else(|| Error::Invalid("wire not found".into()))?;
    let wire_net = wire.net;
    let at = if is_end {
        wire.points.len().saturating_sub(1)
    } else {
        vertex_index.ok_or_else(|| Error::Invalid("start end must have index 0".into()))?
    };
    if at >= wire.points.len() {
        return Err(Error::Invalid("vertex index out of range".into()));
    }
    if wire.points[at] == pin_pos {
        return Err(Error::Invalid("end vertex is already on the pin".into()));
    }
    for label in p.labels.iter() {
        if label.kind == "junction"
            && label.position == wire.points[at]
            && wire.net == Some(label.net)
        {
            return Err(Error::Invalid(
                "a junction sits on this end vertex; move the junction first".into(),
            ));
        }
    }
    let resolved = pin_net
        .or(wire_net)
        .ok_or_else(|| Error::Invalid("neither the wire nor the pin has a net".into()))?;
    if let Some(wire_net) = wire_net
        && wire_net != resolved
    {
        merge_net_into(p, wire_net, resolved)?;
    }
    if pin_net.is_none() {
        assign(p, resolved, std::slice::from_ref(&a.pin))?;
    }
    let wire = p
        .drawn_wires
        .iter_mut()
        .find(|w| w.id == a.id)
        .ok_or_else(|| Error::Invalid("wire not found".into()))?;
    wire.points[at] = pin_pos;
    Ok(json!({"id": a.id}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::PinRef;

    fn build(s: &mut Session) -> Result<(), Error> {
        s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":0.0,"y":0.0},"reference":"R1"}),
            None,
        )?;
        s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":10.0,"y":0.0},"reference":"R2"}),
            None,
        )?;
        Ok(())
    }

    /// Draw a wire on a fresh net from pin 1 of one part to pin 1 of the other, and tie both pins to it.
    fn draw_between(s: &mut Session, from: Id, to: Id) -> Result<Id, Error> {
        let net = s.execute("net.create", json!({"name":"N"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("no net".into()))?;
        let at = |s: &Session, component: Id| {
            s.project.pin_position(&PinRef {
                component,
                pin: "1".to_string(),
                unit: None,
            })
        };
        let (a, b) = (at(s, from), at(s, to));
        let (Some(a), Some(b)) = (a, b) else {
            return Err(Error::Invalid("pin has no position".into()));
        };
        s.execute(
            "wire.draw",
            json!({"points":[{"x":a.x,"y":a.y},{"x":b.x,"y":b.y}],"net":net}),
            None,
        )?;
        s.execute(
            "net.connect",
            json!({"net":net,"pins":[
                {"component":from,"pin":"1"},
                {"component":to,"pin":"1"}
            ]}),
            None,
        )?;
        s.project
            .drawn_wires
            .last()
            .map(|w| w.id)
            .ok_or_else(|| Error::Invalid("no wire".into()))
    }

    #[test]
    fn reconnecting_a_wire_end_moves_it_onto_the_new_pin_and_joins_its_net() -> Result<(), Error> {
        let mut s = Session::default();
        build(&mut s)?;
        let u1 = s.project.components[0].id;
        let u2 = s.project.components[1].id;
        let wire_id = draw_between(&mut s, u1, u2)?;
        let pin2 = s.project.pin_position(&PinRef {
            component: u2,
            pin: "2".to_string(),
            unit: None,
        });
        s.execute(
            "wire.reconnect_end",
            json!({"id":wire_id,"end":"end","pin":{"component":u2,"pin":"2"}}),
            None,
        )?;
        let wire = s
            .project
            .drawn_wires
            .iter()
            .find(|w| w.id == wire_id)
            .ok_or_else(|| Error::Invalid("no wire".into()))?;
        let last = wire
            .points
            .last()
            .ok_or_else(|| Error::Invalid("no point".into()))?;
        assert_eq!(
            *last,
            pin2.ok_or_else(|| Error::Invalid("no pin pos".into()))?
        );
        let pin = s
            .project
            .pin(&PinRef {
                component: u2,
                pin: "2".to_string(),
                unit: None,
            })
            .ok_or_else(|| Error::Invalid("no pin".into()))?;
        assert!(pin.net.is_some());
        Ok(())
    }

    #[test]
    fn reconnecting_to_a_pin_without_a_net_gives_it_the_wires_net() -> Result<(), Error> {
        let mut s = Session::default();
        build(&mut s)?;
        let u1 = s.project.components[0].id;
        let u2 = s.project.components[1].id;
        let wire_id = draw_between(&mut s, u1, u2)?;
        let wire = s
            .project
            .drawn_wires
            .iter()
            .find(|w| w.id == wire_id)
            .ok_or_else(|| Error::Invalid("no wire".into()))?;
        let wire_net = wire.net.ok_or_else(|| Error::Invalid("no net".into()))?;
        s.execute(
            "wire.reconnect_end",
            json!({"id":wire_id,"end":"end","pin":{"component":u2,"pin":"2"}}),
            None,
        )?;
        let pin = s
            .project
            .pin(&PinRef {
                component: u2,
                pin: "2".to_string(),
                unit: None,
            })
            .ok_or_else(|| Error::Invalid("no pin".into()))?;
        assert_eq!(pin.net, Some(wire_net));
        Ok(())
    }

    #[test]
    fn reconnecting_is_one_undo_step() -> Result<(), Error> {
        let mut s = Session::default();
        build(&mut s)?;
        let u1 = s.project.components[0].id;
        let u2 = s.project.components[1].id;
        let wire_id = draw_between(&mut s, u1, u2)?;
        let before = s.project.clone();
        s.execute(
            "wire.reconnect_end",
            json!({"id":wire_id,"end":"end","pin":{"component":u2,"pin":"2"}}),
            None,
        )?;
        assert_ne!(s.project, before);
        s.undo()?;
        assert_eq!(s.project, before);
        Ok(())
    }

    #[test]
    fn a_refused_reconnect_leaves_the_design_unchanged() -> Result<(), Error> {
        let mut s = Session::default();
        build(&mut s)?;
        let u1 = s.project.components[0].id;
        let u2 = s.project.components[1].id;
        let wire_id = draw_between(&mut s, u1, u2)?;
        let before = s.project.clone();
        assert!(
            s.execute(
                "wire.reconnect_end",
                json!({"id":wire_id,"end":"middle","pin":{"component":u2,"pin":"2"}}),
                None
            )
            .is_err()
        );
        assert_eq!(s.project, before);
        Ok(())
    }
}
