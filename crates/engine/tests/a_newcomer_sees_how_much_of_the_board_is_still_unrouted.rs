//! A newcomer sees how much of the board is still unrouted.
use pcbcraft_engine::{Error, Session};
use serde_json::json;

fn id(v: &serde_json::Value) -> Result<u64, Error> {
    v["result"]["id"]
        .as_u64()
        .ok_or_else(|| Error::Invalid("no id".into()))
}

#[test]
fn a_newcomer_sees_how_much_of_the_board_is_still_unrouted() -> Result<(), Error> {
    let mut s = Session::default();
    s.project.auto_sync = false;
    // Place two capacitors on VCC and GND, as a power stage would have.
    let c1 = id(&s.execute(
        "component.add",
        json!({"kind":"capacitor","reference":"C1","value":"100n","position":{"x":10,"y":10}}),
        None,
    )?)?;
    let c2 = id(&s.execute(
        "component.add",
        json!({"kind":"capacitor","reference":"C2","value":"10u","position":{"x":30,"y":10}}),
        None,
    )?)?;
    let vcc = id(&s.execute("net.create", json!({"name":"VCC"}), None)?)?;
    let gnd = id(&s.execute("net.create", json!({"name":"GND"}), None)?)?;
    s.execute(
        "wire.connect",
        json!({"net":vcc,"from":{"component":c1,"pin":"1"},"to":{"component":c2,"pin":"1"}}),
        None,
    )?;
    s.execute(
        "wire.connect",
        json!({"net":gnd,"from":{"component":c1,"pin":"2"},"to":{"component":c2,"pin":"2"}}),
        None,
    )?;
    s.execute("board.sync", json!({}), None)?;
    // The newcomer asks the report how many connections are still open.
    let r =
        s.execute("layout.report", json!({"scope":{"layers":["F.Cu"]}}), None)?["result"].clone();
    let open = r["totals"]["unrouted"]
        .as_u64()
        .ok_or(Error::Invalid("no totals.unrouted".into()))?;
    assert_eq!(open, 2, "both nets are open before routing");
    // Route one net and ask again.
    let c1_fp = s
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.component == c1)
        .map(|f| f.id)
        .ok_or(Error::Invalid("no C1 footprint".into()))?;
    let c2_fp = s
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.component == c2)
        .map(|f| f.id)
        .ok_or(Error::Invalid("no C2 footprint".into()))?;
    s.execute(
        "track.route",
        json!({"from":{"footprint":c1_fp,"pad":"1"},"to":{"footprint":c2_fp,"pad":"1"},"layer":"F.Cu"}),
        None,
    )?;
    let r2 =
        s.execute("layout.report", json!({"scope":{"layers":["F.Cu"]}}), None)?["result"].clone();
    let open2 = r2["totals"]["unrouted"]
        .as_u64()
        .ok_or(Error::Invalid("no totals.unrouted".into()))?;
    assert_eq!(open2, 1, "one net is now closed");
    Ok(())
}
