//! Live co-editing helpers: lock scopes for commands and strictly ordered room apply.
//!
//! Transport and credentials stay outside the engine; the hub child consumes these APIs.

mod apply;
mod scopes;

pub use apply::{OrderedRoomApply, RoomOp};
pub use scopes::command_lock_scope;

use crate::{Error, Session};
use pcbcraft_doc::Project;
use serde_json::Value;

/// Whether a command may be submitted to a shared room or applied from a relay echo.
pub fn shareable_room_command(id: &str) -> bool {
    if crate::artifacts::command_is_read_only(id) {
        return false;
    }
    if id.starts_with("crdt.") {
        return false;
    }
    if id == "cam.isolation" || crate::snapmaker::handles(id) {
        return false;
    }
    !scopes::room_read_only(id)
}

/// Snapshot two peers can compare for byte-identical projects, ids and revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollabSnapshot {
    pub revision: u64,
    pub next_id: u64,
    pub project: Vec<u8>,
}

impl CollabSnapshot {
    pub fn from_session(session: &Session) -> Result<Self, Error> {
        Ok(CollabSnapshot {
            revision: session.revision,
            next_id: session.collab_next_id(),
            project: serde_json::to_vec(&session.project)
                .map_err(|e| Error::Invalid(e.to_string()))?,
        })
    }
}

/// Apply one relay-ordered room edit. Uses [`None`] for the revision guard; ordering is external.
pub fn apply_room_command(session: &mut Session, id: &str, params: Value) -> Result<Value, Error> {
    if !shareable_room_command(id) {
        return Err(Error::Invalid(format!("{id} is not a room edit")));
    }
    session.execute(id, params, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fresh_pair() -> Result<(Session, Session), Error> {
        let a = Session::sample()?;
        let b = Session::sample()?;
        Ok((a, b))
    }

    #[test]
    fn two_peers_applying_the_same_ordered_ops_reach_identical_projects() -> Result<(), Error> {
        let (mut a, mut b) = fresh_pair()?;
        let start_a = CollabSnapshot::from_session(&a)?;
        let start_b = CollabSnapshot::from_session(&b)?;
        assert_eq!(start_a, start_b);

        let ops = vec![
            RoomOp {
                seq: 1,
                local: None,
                command: "net.create".into(),
                params: json!({"name":"ROOM"}),
            },
            RoomOp {
                seq: 2,
                local: None,
                command: "component.add".into(),
                params: json!({"kind":"resistor","reference":"R9","position":{"x":80,"y":40}}),
            },
        ];

        let mut order = OrderedRoomApply::new(0);
        for op in ops {
            for ready in order.ingest(op) {
                apply_room_command(&mut a, &ready.command, ready.params.clone())?;
                apply_room_command(&mut b, &ready.command, ready.params.clone())?;
            }
        }
        assert_eq!(
            CollabSnapshot::from_session(&a)?,
            CollabSnapshot::from_session(&b)?
        );
        assert!(CollabSnapshot::from_session(&a)?.revision > start_a.revision);
        Ok(())
    }

    #[test]
    fn a_rejected_command_advances_nothing_on_every_peer() -> Result<(), Error> {
        let (mut a, mut b) = fresh_pair()?;
        let before_a = CollabSnapshot::from_session(&a)?;
        let before_b = CollabSnapshot::from_session(&b)?;

        let bad = RoomOp {
            seq: 1,
            local: None,
            command: "track.add".into(),
            params: json!({"net":99999,"start":{"x":0,"y":0},"end":{"x":1,"y":1},"width":0.2}),
        };
        let mut order = OrderedRoomApply::new(0);
        for ready in order.ingest(bad) {
            let err_a = apply_room_command(&mut a, &ready.command, ready.params.clone());
            let err_b = apply_room_command(&mut b, &ready.command, ready.params.clone());
            assert!(err_a.is_err());
            assert!(err_b.is_err());
        }
        assert_eq!(CollabSnapshot::from_session(&a)?, before_a);
        assert_eq!(CollabSnapshot::from_session(&b)?, before_b);
        Ok(())
    }

    #[test]
    fn every_command_has_a_scope_or_takes_board() {
        for spec in crate::commands() {
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

    #[test]
    fn ordered_apply_holds_out_of_order_ops_until_the_gap_fills() -> Result<(), Error> {
        let mut order = OrderedRoomApply::new(0);
        assert!(
            order
                .ingest(RoomOp {
                    seq: 2,
                    local: None,
                    command: "net.create".into(),
                    params: json!({"name":"LATE"}),
                })
                .is_empty()
        );
        let ready = order.ingest(RoomOp {
            seq: 1,
            local: None,
            command: "net.create".into(),
            params: json!({"name":"FIRST"}),
        });
        assert_eq!(ready.len(), 2);
        assert_eq!(ready[0].seq, 1);
        assert_eq!(ready[1].seq, 2);
        Ok(())
    }

    #[test]
    fn wire_connect_locks_the_net() -> Result<(), Error> {
        let mut s = Session::sample()?;
        let net = s
            .project
            .nets
            .first()
            .ok_or_else(|| Error::Invalid("sample net".into()))?
            .id;
        let scope = command_lock_scope(
            &s.project,
            "wire.connect",
            &json!({"net":net,"from":{"component":1,"pin":"1"},"to":{"component":2,"pin":"1"}}),
        );
        assert_eq!(scope, format!("net:{net}"));
        Ok(())
    }

    #[test]
    fn footprint_move_locks_its_component() -> Result<(), Error> {
        let s = Session::sample()?;
        let fp = s.project.board.footprints[0].id;
        let component = s.project.board.footprints[0].component;
        let scope = command_lock_scope(
            &s.project,
            "footprint.move",
            &json!({"id":fp,"position":{"x":1,"y":2}}),
        );
        assert_eq!(scope, format!("component:{component}"));
        Ok(())
    }
}
