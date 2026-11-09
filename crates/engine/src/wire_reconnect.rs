//! Reconnect one end of a drawn wire onto a pin or onto another wire, joining
//! the wire to the target's net. Used when the user drags a wire end onto a
//! different pin or onto a different wire to make a tee.
//!
//! # Rules
//! - Exactly one of `pin` or `onto` must be provided.
//! - With `pin`: the moved vertex snaps to the pin position; junction validation
//!   applies exactly as in `wire.set_point`.
//! - With `onto`: the moved vertex snaps to the nearest point on the target wire
//!   (within 0.5 mm of the given point); the target wire is split at that point
//!   and a junction label is placed.
//! - If the target has a different net, the dragged wire's net is merged into
//!   the target's net.
//! - One command is one undo step; a refused edit leaves the project untouched.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs (Session, Project, Id, Error, PinRef, parse, merge_net_into)
//! - crates/doc/src/lib.rs (Project::pin, Project::pin_position)
//! - crates/geom/src/lib.rs (Point, point_segment)
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
    pin: Option<PinRef>,
    onto: Option<OntoTarget>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OntoTarget {
    wire: Id,
    at: Point,
}

/// The id and params of this module's one command.
pub(super) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "wire.reconnect_end",
        description: "Drag one end of a drawn wire onto a pin or onto another wire: move the end vertex to the target and join the wire to that target's net, merging nets if they differ; one undo step",
        input_schema: schema(
            json!({"id":id_schema(),"end":{"type":"string","enum":["start","end"]},"pin":pin_schema(),"onto":{"type":"object","properties":{"wire":id_schema(),"at":point_schema()}}}),
            &["id", "end"],
        ),
    }]
}

/// Dispatch for the wire reconnect command; `None` when `command` is not one.
pub(super) fn run(
    p: &mut Project,
    next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    if !commands().iter().any(|c| c.id == command) {
        return None;
    }
    Some(reconnect_end(p, next, params))
}

fn reconnect_end(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, Error> {
    let a: ReconnectParams = parse(params)?;
    let has_pin = a.pin.is_some();
    let has_onto = a.onto.is_some();
    if has_pin && has_onto {
        return Err(Error::Invalid(
            "give a pin or a wire to reconnect to, not both".into(),
        ));
    }
    if !has_pin && !has_onto {
        return Err(Error::Invalid(
            "give a pin or a wire to reconnect to".into(),
        ));
    }
    let (vertex_index, is_end) = match a.end.as_str() {
        "start" => (Some(0), false),
        "end" => (None, true),
        other => {
            return Err(Error::Invalid(format!(
                "end must be start or end, not {other}"
            )));
        }
    };
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
    if let Some(pin) = a.pin.as_ref() {
        return reconnect_to_pin(p, a.id, at, pin, wire_net);
    }
    let target = a
        .onto
        .as_ref()
        .ok_or_else(|| Error::Invalid("give a pin or a wire to reconnect to".into()))?;
    if target.wire == a.id {
        return Err(Error::Invalid("cannot reconnect a wire onto itself".into()));
    }
    reconnect_to_wire(p, next, a.id, at, target, wire_net)
}

fn reconnect_to_pin(
    p: &mut Project,
    wire_id: Id,
    at: usize,
    pin: &PinRef,
    wire_net: Option<Id>,
) -> Result<Value, Error> {
    let pin_obj = p
        .pin(pin)
        .ok_or_else(|| Error::Invalid("pin not found".into()))?;
    let pin_net = pin_obj.net;
    let pin_pos = p
        .pin_position(pin)
        .ok_or_else(|| Error::Invalid("pin has no position".into()))?;
    let wire = p
        .drawn_wires
        .iter()
        .find(|w| w.id == wire_id)
        .ok_or_else(|| Error::Invalid("wire not found".into()))?;
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
        assign(p, resolved, std::slice::from_ref(pin))?;
    }
    let wire = p
        .drawn_wires
        .iter_mut()
        .find(|w| w.id == wire_id)
        .ok_or_else(|| Error::Invalid("wire not found".into()))?;
    wire.points[at] = pin_pos;
    Ok(json!({"id": wire_id}))
}

fn reconnect_to_wire(
    p: &mut Project,
    next: &mut Id,
    wire_id: Id,
    at: usize,
    target: &OntoTarget,
    wire_net: Option<Id>,
) -> Result<Value, Error> {
    if !target.at.valid() {
        return Err(Error::Invalid("onto point is out of range".into()));
    }
    let target_wire = p
        .drawn_wires
        .iter()
        .find(|w| w.id == target.wire)
        .ok_or_else(|| Error::Invalid("target wire not found".into()))?;
    let target_net = target_wire.net;
    let closest = closest_point_on_wire(target_wire, target.at);
    let dist = target.at.distance(closest);
    if dist > 0.5 {
        return Err(Error::Invalid(format!(
            "onto point is too far from the target wire ({:.3} mm)",
            dist
        )));
    }
    let resolved = target_net
        .or(wire_net)
        .ok_or_else(|| Error::Invalid("neither wire has a net".into()))?;
    if let Some(wire_net) = wire_net
        && wire_net != resolved
    {
        merge_net_into(p, wire_net, resolved)?;
    }
    if target_net.is_none() {
        let target_wire = p
            .drawn_wires
            .iter_mut()
            .find(|w| w.id == target.wire)
            .ok_or_else(|| Error::Invalid("target wire not found".into()))?;
        target_wire.net = Some(resolved);
    }
    let wire = p
        .drawn_wires
        .iter_mut()
        .find(|w| w.id == wire_id)
        .ok_or_else(|| Error::Invalid("wire not found".into()))?;
    wire.points[at] = closest;
    wire.net = Some(resolved);
    let target_wire = p
        .drawn_wires
        .iter()
        .find(|w| w.id == target.wire)
        .ok_or_else(|| Error::Invalid("target wire not found".into()))?;
    if !target_wire.points.contains(&closest) {
        let target = p
            .drawn_wires
            .iter_mut()
            .find(|w| w.id == target.wire)
            .ok_or_else(|| Error::Invalid("target wire not found".into()))?;
        let segment = (0..target.points.len() - 1)
            .find(|i| {
                let from = target.points[*i];
                let to = target.points[*i + 1];
                let d = pcbcraft_geom::point_segment(closest, from, to);
                d <= 1e-4
            })
            .ok_or_else(|| Error::Invalid("closest point not on a segment".into()))?;
        let mut head = target.points[..=segment].to_vec();
        head.push(closest);
        let mut tail = vec![closest];
        tail.extend_from_slice(&target.points[segment + 1..]);
        target.points = head;
        let split_id = allocate(next)?;
        p.drawn_wires.push(DrawnWire {
            id: split_id,
            net: Some(resolved),
            points: tail,
        });
    }
    let label = allocate(next)?;
    p.labels.push(pcbcraft_doc::Label {
        id: label,
        kind: "junction".into(),
        net: resolved,
        position: closest,
        rotation: 0.0,
        text: String::new(),
        style: String::new(),
    });
    Ok(json!({"id": wire_id, "junction": label}))
}

fn closest_point_on_wire(wire: &DrawnWire, p: Point) -> Point {
    let mut best = wire.points[0];
    let mut best_dist = p.distance(best);
    for i in 0..wire.points.len() - 1 {
        let from = wire.points[i];
        let to = wire.points[i + 1];
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let length_sq = dx * dx + dy * dy;
        let t = if length_sq < 1e-20 {
            0.0
        } else {
            let raw = ((p.x - from.x) * dx + (p.y - from.y) * dy) / length_sq;
            raw.clamp(0.0, 1.0)
        };
        let proj = Point::new(from.x + t * dx, from.y + t * dy);
        let dist = p.distance(proj);
        if dist < best_dist {
            best_dist = dist;
            best = proj;
        }
    }
    best
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

    #[test]
    fn reconnecting_a_wire_end_onto_another_wire_makes_a_junction_and_one_net() -> Result<(), Error>
    {
        let mut s = Session::default();
        s.execute("net.create", json!({"name":"A"}), None)?;
        s.execute("net.create", json!({"name":"B"}), None)?;
        let net_a = s
            .project
            .nets
            .iter()
            .find(|n| n.name == "A")
            .ok_or_else(|| Error::Invalid("not found".into()))?
            .id;
        let net_b = s
            .project
            .nets
            .iter()
            .find(|n| n.name == "B")
            .ok_or_else(|| Error::Invalid("not found".into()))?
            .id;
        s.execute(
            "wire.draw",
            json!({"points":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0}],"net":net_a}),
            None,
        )?;
        let wire_a = s
            .project
            .drawn_wires
            .last()
            .ok_or_else(|| Error::Invalid("not found".into()))?
            .id;
        s.execute(
            "wire.draw",
            json!({"points":[{"x":5.0,"y":-5.0},{"x":5.0,"y":5.0}],"net":net_b}),
            None,
        )?;
        let wire_b = s
            .project
            .drawn_wires
            .last()
            .ok_or_else(|| Error::Invalid("not found".into()))?
            .id;
        s.execute(
            "wire.reconnect_end",
            json!({"id":wire_b,"end":"end","onto":{"wire":wire_a,"at":{"x":5.0,"y":0.0}}}),
            None,
        )?;
        let wire_a = s
            .project
            .drawn_wires
            .iter()
            .find(|w| w.id == wire_a)
            .ok_or_else(|| Error::Invalid("no wire a".into()))?;
        let wire_b = s
            .project
            .drawn_wires
            .iter()
            .find(|w| w.id == wire_b)
            .ok_or_else(|| Error::Invalid("no wire b".into()))?;
        assert_eq!(wire_a.net, Some(net_a));
        assert_eq!(wire_b.net, Some(net_a));
        assert!(
            s.project
                .labels
                .iter()
                .any(|l| l.kind == "junction" && l.position == Point::new(5.0, 0.0)),
            "a junction should exist at the tee point"
        );
        Ok(())
    }

    #[test]
    fn reconnect_refuses_both_pin_and_wire() -> Result<(), Error> {
        let mut s = Session::default();
        build(&mut s)?;
        let u1 = s.project.components[0].id;
        let u2 = s.project.components[1].id;
        let wire_id = draw_between(&mut s, u1, u2)?;
        let result = s.execute(
            "wire.reconnect_end",
            json!({"id":wire_id,"end":"end","pin":{"component":u2,"pin":"2"},"onto":{"wire":wire_id,"at":{"x":5.0,"y":0.0}}}),
            None,
        );
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn reconnect_refuses_the_same_wire() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute("net.create", json!({"name": "N"}), None)?;
        let net = s
            .project
            .nets
            .last()
            .ok_or_else(|| Error::Invalid("not found".into()))?
            .id;
        s.execute(
            "wire.draw",
            json!({"points":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0}],"net":net}),
            None,
        )?;
        let wire_id = s
            .project
            .drawn_wires
            .last()
            .ok_or_else(|| Error::Invalid("not found".into()))?
            .id;
        let result = s.execute(
            "wire.reconnect_end",
            json!({"id":wire_id,"end":"end","onto":{"wire":wire_id,"at":{"x":5.0,"y":0.0}}}),
            None,
        );
        assert!(result.is_err());
        Ok(())
    }
}
