//! Engine tests for `layout.place_polish`: the Arrange command.
use pcbcraft_engine::{Error, Session};
use serde_json::{Value, json};

fn id(v: &Value) -> Result<u64, Error> {
    v["result"]["id"]
        .as_u64()
        .ok_or_else(|| Error::Invalid("no id".into()))
}

type Positions = Vec<(u64, f64, f64, f64)>;

fn positions(s: &Session) -> Positions {
    s.project
        .board
        .footprints
        .iter()
        .map(|f| (f.id, f.position.x, f.position.y, f.rotation))
        .collect()
}

fn fp_of(s: &Session, component: u64) -> Result<u64, Error> {
    s.project
        .board
        .footprints
        .iter()
        .find(|f| f.component == component)
        .map(|f| f.id)
        .ok_or_else(|| Error::Invalid("no footprint".into()))
}

fn undo_depth(s: &Session) -> Result<u64, Error> {
    s.inspect()["history"]["undo"]
        .as_u64()
        .ok_or_else(|| Error::Invalid("no undo depth".into()))
}

/// R1 and R2 far left of a locked connector J1 they are wired to. Returns the
/// session and the footprint ids (r1, r2, j1).
fn board(lock_resistors: bool) -> Result<(Session, [u64; 3]), Error> {
    let mut s = Session::default();
    s.project.auto_sync = false;
    let mut add = |kind: &str, reference: &str| -> Result<u64, Error> {
        id(&s.execute(
            "component.add",
            json!({"kind":kind,"reference":reference,"value":"v","position":{"x":10,"y":10}}),
            None,
        )?)
    };
    let (r1, r2, j1) = (
        add("resistor", "R1")?,
        add("resistor", "R2")?,
        add("connector", "J1")?,
    );
    let mut net = |name: &str| -> Result<u64, Error> {
        id(&s.execute("net.create", json!({"name":name}), None)?)
    };
    let (n1, n2, n3) = (net("N1")?, net("N2")?, net("N3")?);
    for (n, a, ap, b, bp) in [
        (n1, r1, "1", j1, "1"),
        (n2, r2, "1", j1, "2"),
        (n3, r1, "2", r2, "2"),
    ] {
        s.execute(
            "wire.connect",
            json!({"net":n,"from":{"component":a,"pin":ap},"to":{"component":b,"pin":bp}}),
            None,
        )?;
    }
    s.execute("board.sync", json!({}), None)?;
    let ids = [fp_of(&s, r1)?, fp_of(&s, r2)?, fp_of(&s, j1)?];
    for (fp, x, y) in [
        (ids[0], 14.0, 14.0),
        (ids[1], 14.0, 28.0),
        (ids[2], 44.0, 21.0),
    ] {
        s.execute(
            "footprint.move",
            json!({"id":fp,"position":{"x":x,"y":y}}),
            None,
        )?;
    }
    s.execute(
        "footprint.set_locked",
        json!({"id":ids[2],"locked":true}),
        None,
    )?;
    if lock_resistors {
        for fp in &ids[..2] {
            s.execute("footprint.set_locked", json!({"id":fp,"locked":true}), None)?;
        }
    }
    Ok((s, ids))
}

fn find(p: &Positions, fp: u64) -> Result<(u64, f64, f64, f64), Error> {
    p.iter()
        .copied()
        .find(|q| q.0 == fp)
        .ok_or_else(|| Error::Invalid("part missing".into()))
}

#[test]
fn arrange_moves_unlocked_parts_and_never_the_locked_one() -> Result<(), Error> {
    let (mut s, ids) = board(false)?;
    let before = positions(&s);
    let out = s.execute("layout.place_polish", json!({}), None)?;
    let r = &out["result"];
    let count = r["count"]
        .as_u64()
        .ok_or_else(|| Error::Invalid("no count".into()))?;
    assert!(count >= 1, "the resistors are far from their connector");
    assert_eq!(r["unlocked"], json!(2));
    let moved = r["moved"]
        .as_array()
        .ok_or_else(|| Error::Invalid("no moved list".into()))?;
    assert_eq!(moved.len() as u64, count);
    for m in moved {
        let fp = m["footprint"].as_u64().unwrap_or(0);
        assert!(
            fp == ids[0] || fp == ids[1],
            "only unlocked parts are listed"
        );
    }
    let after = positions(&s);
    assert_eq!(
        find(&before, ids[2])?,
        find(&after, ids[2])?,
        "the locked connector keeps its exact position and rotation"
    );
    assert_ne!(before, after, "something moved");
    let (a, b) = (
        r["wire_before_mm"].as_f64().unwrap_or(f64::NAN),
        r["wire_after_mm"].as_f64().unwrap_or(f64::NAN),
    );
    assert!(b <= a + 1e-9, "the wire got no longer: {a} -> {b}");
    assert!(
        r["message"]
            .as_str()
            .is_some_and(|m| m.contains("unlocked parts moved")),
        "a plain sentence says what happened"
    );
    Ok(())
}

#[test]
fn arrange_is_one_undo_step() -> Result<(), Error> {
    let (mut s, _) = board(false)?;
    let before = positions(&s);
    let depth = undo_depth(&s)?;
    s.execute("layout.place_polish", json!({}), None)?;
    assert_eq!(undo_depth(&s)?, depth + 1, "one command, one undo entry");
    assert_ne!(positions(&s), before);
    s.undo()?;
    assert_eq!(positions(&s), before, "one undo restores every position");
    Ok(())
}

#[test]
fn arrange_gives_the_same_result_twice_from_the_same_board() -> Result<(), Error> {
    let (mut a, _) = board(false)?;
    let (mut b, _) = board(false)?;
    a.execute("layout.place_polish", json!({"seed":3,"moves":800}), None)?;
    b.execute("layout.place_polish", json!({"seed":3,"moves":800}), None)?;
    assert_eq!(
        positions(&a),
        positions(&b),
        "same board and seed, same parts"
    );
    Ok(())
}

#[test]
fn arrange_with_nothing_unlocked_says_so_and_writes_nothing() -> Result<(), Error> {
    let (mut s, _) = board(true)?;
    let before = positions(&s);
    let depth = undo_depth(&s)?;
    let err = s
        .execute("layout.place_polish", json!({}), None)
        .err()
        .ok_or_else(|| Error::Invalid("expected a refusal".into()))?;
    assert!(
        format!("{err}").contains("no unlocked parts"),
        "a plain sentence: {err}"
    );
    assert_eq!(positions(&s), before);
    assert_eq!(undo_depth(&s)?, depth, "a refusal adds no undo step");
    Ok(())
}

#[test]
fn arrange_refuses_values_outside_its_bounds() -> Result<(), Error> {
    let (mut s, _) = board(false)?;
    let before = positions(&s);
    for params in [
        json!({"moves":0}),
        json!({"moves":20_001}),
        json!({"seed":-1}),
        json!({"turbo":true}),
        json!({"scope":{}}),
    ] {
        assert!(
            s.execute("layout.place_polish", params.clone(), None)
                .is_err(),
            "{params} is refused"
        );
    }
    assert_eq!(positions(&s), before, "refusals change nothing");
    Ok(())
}

#[test]
fn arrange_is_listed_as_a_command() {
    assert!(
        pcbcraft_engine::commands()
            .iter()
            .any(|c| c.id == "layout.place_polish")
    );
}
