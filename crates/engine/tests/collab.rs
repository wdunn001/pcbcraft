//! Collab command scopes and ordered room apply (issue #369).

use pcbcraft_doc::Project;
use pcbcraft_engine::{
    CollabSnapshot, OrderedRoomApply, RoomOp, Session, apply_room_command, command_lock_scope,
};
use serde_json::json;

#[test]
fn two_peers_applying_the_same_ordered_ops_reach_identical_projects() {
    let mut a = Session::sample().expect("sample a");
    let mut b = Session::sample().expect("sample b");
    assert_eq!(
        CollabSnapshot::from_session(&a).expect("snap a"),
        CollabSnapshot::from_session(&b).expect("snap b")
    );

    let ops = [
        RoomOp {
            seq: 1,
            local: None,
            command: "project.set_meta".into(),
            params: json!({"key":"room","value":"369"}),
        },
        RoomOp {
            seq: 2,
            local: None,
            command: "net.create".into(),
            params: json!({"name":"COLLAB"}),
        },
    ];
    let mut order = OrderedRoomApply::new(0);
    for op in ops {
        for ready in order.ingest(op) {
            apply_room_command(&mut a, &ready.command, ready.params.clone()).expect("peer a");
            apply_room_command(&mut b, &ready.command, ready.params.clone()).expect("peer b");
        }
    }
    assert_eq!(
        CollabSnapshot::from_session(&a).expect("final a"),
        CollabSnapshot::from_session(&b).expect("final b")
    );
}

#[test]
fn a_rejected_command_advances_nothing_on_every_peer() {
    let mut a = Session::sample().expect("sample a");
    let mut b = Session::sample().expect("sample b");
    let before_a = CollabSnapshot::from_session(&a).expect("before a");
    let before_b = CollabSnapshot::from_session(&b).expect("before b");

    let mut order = OrderedRoomApply::new(0);
    for ready in order.ingest(RoomOp {
        seq: 1,
        local: None,
        command: "component.move".into(),
        params: json!({"id":99999,"position":{"x":0,"y":0}}),
    }) {
        assert!(apply_room_command(&mut a, &ready.command, ready.params.clone()).is_err());
        assert!(apply_room_command(&mut b, &ready.command, ready.params.clone()).is_err());
    }
    assert_eq!(CollabSnapshot::from_session(&a).expect("after a"), before_a);
    assert_eq!(CollabSnapshot::from_session(&b).expect("after b"), before_b);
}

#[test]
fn every_command_has_a_scope_or_takes_board() {
    for spec in pcbcraft_engine::commands() {
        let scope = command_lock_scope(&Project::default(), spec.id, &json!({}));
        assert!(
            scope == "board"
                || scope.starts_with("net:")
                || scope.starts_with("component:")
                || scope.starts_with("sheet:"),
            "{} -> {scope}",
            spec.id
        );
    }
}
