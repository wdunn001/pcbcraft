//! Bus entry stub commands: tap a single member net off a drawn bus with a
//! short 45-degree stub that joins a pin to the bus.
//!
//! Each command edits the project, checks that the tap is on the bus, binds
//! the stub to the member net, and connects the pin. The whole tap is one
//! undo step. The stub is a two-vertex drawn wire; the member index is
//! carried in the command and not stored on the wire, so the bus name alone
//! determines which net the stub joins.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs#bus_notation
//! - crates/engine/src/lib.rs#bus_members
//! - crates/engine/src/lib.rs#assign
//! - crates/doc/src/lib.rs#DrawnWire
//! - crates/geom/src/lib.rs#Point
use super::*;
use pcbcraft_doc::PinRef;

/// Commands offered by the bus entry module.
pub(super) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "wire.bus_entry",
        description: "Tap one member net off a drawn bus with a short 45-degree stub that joins a pin to the bus; the member index selects which net (0 is DB0) and the tap point must be within 0.5 mm of the bus; the stub is 2.54 mm long and the whole tap is one undo step",
        input_schema: schema(
            json!({
                "bus": id_schema(),
                "member": {"type":"integer","minimum":0,"maximum":63,"description":"member index, 0 for DB0"},
                "at": point_schema(),
                "pin": pin_schema()
            }),
            &["bus", "member", "at", "pin"],
        ),
    }]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BusEntryParams {
    bus: Id,
    member: usize,
    at: Point,
    pin: PinRef,
}

/// Dispatch for the bus entry commands; `None` when `command` is not one.
///
/// # Rules
/// - The tap point must be within 0.5 mm of the bus wire.
/// - The member index must be in range for the bus width.
///
/// # Do not
/// - Do not create a new bus wire; only tap an existing one.
pub(super) fn run(
    p: &mut Project,
    next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    if !commands().iter().any(|c| c.id == command) {
        return None;
    }
    Some(match command {
        "wire.bus_entry" => bus_entry(p, next, params),
        _ => Err(Error::Invalid(format!(
            "unknown bus_entry command: {}",
            command
        ))),
    })
}

fn bus_entry(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, Error> {
    let a: BusEntryParams = parse(params)?;
    let bus_wire = p
        .drawn_wires
        .iter()
        .find(|w| w.id == a.bus)
        .ok_or_else(missing)?;
    let head = *bus_wire.points.first().ok_or_else(missing)?;
    let bus_label = p
        .labels
        .iter()
        .find(|l| l.kind == "bus" && l.position.distance(head) < 0.05)
        .ok_or_else(|| Error::Invalid("no bus label near this bus wire".into()))?;
    let (stem, count) = bus_notation(&bus_label.text)?;
    if a.member >= count {
        return Err(Error::Invalid("bus member out of range".into()));
    }
    let on_bus = bus_wire
        .points
        .windows(2)
        .any(|pair| pcbcraft_geom::point_segment(a.at, pair[0], pair[1]) < 0.5);
    if !on_bus {
        return Err(Error::Invalid("point is not on the bus".into()));
    }
    let member_net = p
        .nets
        .iter()
        .find(|n| n.name == format!("{}{}", stem, a.member))
        .ok_or_else(|| Error::Invalid("member net not found".into()))?
        .id;
    let pin_pos = p.pin_position(&a.pin).ok_or_else(missing)?;
    // Compute 2.54 mm stub from tap point toward pin position.
    let dx = pin_pos.x - a.at.x;
    let dy = pin_pos.y - a.at.y;
    let length = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if length < 1e-6 {
        (1.0, 0.0)
    } else {
        (dx / length, dy / length)
    };
    let stub_end = Point::new(a.at.x + ux * 2.54, a.at.y + uy * 2.54);
    let stub_id = allocate(next)?;
    p.drawn_wires.push(DrawnWire {
        id: stub_id,
        net: Some(member_net),
        points: vec![a.at, stub_end],
    });
    assign(p, member_net, std::slice::from_ref(&a.pin))?;
    Ok(json!({"stub": stub_id, "member": a.member, "net": member_net}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn missing() -> Error {
        Error::Invalid("object not found".into())
    }

    fn create_bus_and_part(s: &mut Session) -> Result<(Id, Id, String), Error> {
        s.execute(
            "wire.draw_bus",
            json!({"name": "DB*8", "points": [{"x": 0.0, "y": 0.0}, {"x": 10.0, "y": 0.0}, {"x": 10.0, "y": 10.0}]}),
            None,
        )?;
        let bus_id = s.project.drawn_wires[0].id;

        s.execute(
            "component.add",
            json!({"kind": "resistor", "position": {"x": 15.0, "y": 0.0}}),
            None,
        )?;
        let comp_id = s.project.components[0].id;

        Ok((bus_id, comp_id, "1".to_string()))
    }

    #[test]
    fn bus_entry_taps_one_member_net_onto_a_pin() -> Result<(), Error> {
        let mut s = Session::default();
        let (bus_id, comp_id, pin_num) = create_bus_and_part(&mut s)?;

        let r = s.execute(
            "wire.bus_entry",
            json!({
                "bus": bus_id,
                "member": 0,
                "at": {"x": 5.0, "y": 0.0},
                "pin": {"component": comp_id, "pin": pin_num}
            }),
            None,
        )?["result"]
            .clone();

        let stub_id = r["stub"].as_u64().ok_or_else(missing)? as Id;
        let stub = s
            .project
            .drawn_wires
            .iter()
            .find(|w| w.id == stub_id)
            .ok_or_else(missing)?;
        assert_eq!(stub.net, Some(r["net"].as_u64().ok_or_else(missing)? as Id));
        assert_eq!(stub.points.len(), 2);

        let pin = s
            .project
            .pin(&PinRef {
                component: comp_id,
                pin: pin_num,
                unit: None,
            })
            .ok_or_else(missing)?;
        assert_eq!(pin.net, Some(r["net"].as_u64().ok_or_else(missing)? as Id));

        Ok(())
    }

    #[test]
    fn bus_entry_refuses_a_member_beyond_the_bus_width() -> Result<(), Error> {
        let mut s = Session::default();
        let (bus_id, comp_id, pin_num) = create_bus_and_part(&mut s)?;

        let err = s.execute(
            "wire.bus_entry",
            json!({
                "bus": bus_id,
                "member": 99,
                "at": {"x": 5.0, "y": 0.0},
                "pin": {"component": comp_id, "pin": pin_num}
            }),
            None,
        );
        assert!(err.is_err());
        Ok(())
    }

    #[test]
    fn bus_entry_refuses_a_point_off_the_bus() -> Result<(), Error> {
        let mut s = Session::default();
        let (bus_id, comp_id, pin_num) = create_bus_and_part(&mut s)?;

        let err = s.execute(
            "wire.bus_entry",
            json!({
                "bus": bus_id,
                "member": 0,
                "at": {"x": 50.0, "y": 50.0},
                "pin": {"component": comp_id, "pin": pin_num}
            }),
            None,
        );
        assert!(err.is_err());
        Ok(())
    }

    #[test]
    fn bus_entry_is_one_undo_step() -> Result<(), Error> {
        let mut s = Session::default();
        let (bus_id, comp_id, pin_num) = create_bus_and_part(&mut s)?;

        s.execute(
            "wire.bus_entry",
            json!({
                "bus": bus_id,
                "member": 1,
                "at": {"x": 5.0, "y": 0.0},
                "pin": {"component": comp_id, "pin": pin_num}
            }),
            None,
        )?;

        let stub_count = s.project.drawn_wires.len();
        s.undo()?;
        assert_eq!(s.project.drawn_wires.len(), stub_count - 1);

        Ok(())
    }
}
