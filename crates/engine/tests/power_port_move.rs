//! Engine tests for `power_port.move` (#1754): the port and the wire ends on
//! it move in one undo step, and a drop on another net is refused.
use pcbcraft_engine::{Error, Session};
use pcbcraft_geom::Point;
use serde_json::json;

type T = Result<(), Error>;

fn rig(s: &mut Session, name: &str) -> Result<(u64, u64), Error> {
    s.execute(
        "component.add",
        json!({"kind":"resistor","position":{"x":40.0,"y":40.0}}),
        None,
    )?;
    let part = s.project.components.len().saturating_sub(1);
    let c = &s.project.components[part];
    let (cid, pin) = (c.id, c.pin_position("1").unwrap_or_default());
    let top = Point::new(pin.x, pin.y - 10.0);
    let placed = s.execute(
        "label.place",
        json!({"kind":"power","name":name,"style":"bar","position":top}),
        None,
    )?;
    let port = placed["result"]["id"].as_u64().unwrap_or_default();
    let net = placed["result"]["net"].as_u64().unwrap_or_default();
    let drawn = s.execute(
        "wire.place",
        json!({"points":[pin,top],"pins":[{"component":cid,"pin":"1"}],"net":net}),
        None,
    )?;
    Ok((port, drawn["result"]["id"].as_u64().unwrap_or_default()))
}

#[test]
fn a_power_port_moves_with_its_wire_in_one_undo_step() -> T {
    let mut s = Session::default();
    let (port, wire) = rig(&mut s, "3V3")?;
    let before = s.project.drawn_wires.clone();
    let to = Point::new(50.0, 20.0);
    s.execute("power_port.move", json!({"id":port,"position":to}), None)?;
    let moved = s
        .project
        .drawn_wires
        .iter()
        .find(|w| w.id == wire)
        .map(|w| w.points.clone())
        .unwrap_or_default();
    assert_eq!(moved.last().copied(), Some(to), "the wire end follows");
    assert!(
        moved
            .windows(2)
            .all(|p| p[0].x == p[1].x || p[0].y == p[1].y),
        "no slanted segment: {moved:?}"
    );
    s.undo()?;
    assert_eq!(s.project.drawn_wires, before, "one undo restores the wire");
    Ok(())
}

#[test]
fn a_port_dropped_on_a_pin_of_another_net_is_refused() -> T {
    let mut s = Session::default();
    let (port, _) = rig(&mut s, "3V3")?;
    s.execute(
        "component.add",
        json!({"kind":"resistor","position":{"x":80.0,"y":40.0}}),
        None,
    )?;
    let other = s.project.components.len().saturating_sub(1);
    let c = &s.project.components[other];
    let (id, pin) = (c.id, c.pin_position("1").unwrap_or_default());
    let made = s.execute("net.create", json!({"name":"SIG"}), None)?;
    let net = made["result"]["id"].as_u64().unwrap_or_default();
    s.execute(
        "net.connect",
        json!({"net":net,"pins":[{"component":id,"pin":"1"}]}),
        None,
    )?;
    let refused = s.execute("power_port.move", json!({"id":port,"position":pin}), None);
    assert!(
        refused.is_err_and(|e| e.to_string().contains("another net")),
        "joining two nets by a drop is refused"
    );
    Ok(())
}

#[test]
fn only_a_power_port_moves_this_way() -> T {
    let mut s = Session::default();
    rig(&mut s, "3V3")?;
    let placed = s.execute(
        "label.place",
        json!({"kind":"label","name":"SIG","position":{"x":5.0,"y":5.0}}),
        None,
    )?;
    let id = placed["result"]["id"].as_u64().unwrap_or_default();
    let refused = s.execute(
        "power_port.move",
        json!({"id":id,"position":{"x":9.0,"y":9.0}}),
        None,
    );
    assert!(refused.is_err(), "a plain label keeps label.move");
    Ok(())
}
