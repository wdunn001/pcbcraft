//! Engine tests for inner copper plane roles (issue 666).
use pcbcraft_engine::{Error, Session};
use serde_json::json;

type T = Result<(), Error>;

fn four_layer_stack(s: &mut Session) -> T {
    for row in [
        json!({"name":"F.Cu","kind":"copper","thickness":0.035}),
        json!({"name":"Core","kind":"core","thickness":1.0}),
        json!({"name":"In1.Cu","kind":"copper","thickness":0.035}),
        json!({"name":"Prepreg","kind":"prepreg","thickness":0.2}),
        json!({"name":"In2.Cu","kind":"copper","thickness":0.035}),
        json!({"name":"B.Cu","kind":"copper","thickness":0.035}),
    ] {
        s.execute("board.set_layer_record", row, None)?;
    }
    Ok(())
}

#[test]
fn inner_copper_roles_record_list_undo_and_refuse_outer_layers() -> T {
    let mut s = Session::default();
    four_layer_stack(&mut s)?;
    s.execute(
        "board.set_copper_layer_role",
        json!({"layer":"In1.Cu","role":"ground"}),
        None,
    )?;
    s.execute(
        "board.set_copper_layer_role",
        json!({"layer":"In2.Cu","role":"power"}),
        None,
    )?;
    let list = s.execute("board.copper_layer_roles", json!({}), None)?["result"]["layers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        list.iter().any(|r| {
            r["layer"] == "In1.Cu" && r["role"] == "ground"
        }),
        "In1 is ground: {list:?}"
    );
    assert!(
        list.iter().any(|r| {
            r["layer"] == "In2.Cu" && r["role"] == "power"
        }),
        "In2 is power: {list:?}"
    );
    assert!(
        s.execute(
            "board.set_copper_layer_role",
            json!({"layer":"F.Cu","role":"ground"}),
            None
        )
        .is_err(),
        "outer layers refuse a plane role"
    );
    s.execute(
        "board.set_copper_layer_role",
        json!({"layer":"In1.Cu","role":"signal"}),
        None,
    )?;
    let roles = &s.project.board.copper_layer_roles;
    assert!(
        roles.get(1).is_none_or(|r| r.is_none()),
        "signal clears the stored role: {roles:?}"
    );
    s.undo()?;
    assert_eq!(
        s.project.board.copper_layer_roles.get(1).and_then(|r| r.as_deref()),
        Some("ground")
    );
    Ok(())
}

#[test]
fn plane_roles_move_with_stackup_reorder() -> T {
    let mut s = Session::default();
    four_layer_stack(&mut s)?;
    s.execute(
        "board.set_copper_layer_role",
        json!({"layer":"In1.Cu","role":"ground"}),
        None,
    )?;
    s.execute(
        "board.set_copper_layer_role",
        json!({"layer":"In2.Cu","role":"power"}),
        None,
    )?;
    s.execute(
        "board.stackup_move_layer",
        json!({"name":"In1.Cu","direction":"down"}),
        None,
    )?;
    let list = s.execute("board.copper_layer_roles", json!({}), None)?["result"]["layers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        list.iter().any(|r| r["layer"] == "In1.Cu" && r["role"] == "power"),
        "roles swap with the copper rows: {list:?}"
    );
    assert!(
        list.iter().any(|r| r["layer"] == "In2.Cu" && r["role"] == "ground"),
        "roles swap with the copper rows: {list:?}"
    );
    Ok(())
}
