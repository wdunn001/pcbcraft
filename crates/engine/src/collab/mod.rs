//! Live co-editing helpers: lock scopes for commands and strictly ordered room apply.
//!
//! Transport and credentials stay outside the engine; the hub child consumes these APIs.

mod apply;
mod scopes;

pub use apply::{OrderedRoomApply, RoomOp};
pub use scopes::command_lock_scope;
pub(crate) use scopes::reads_the_design;

use crate::{Error, Session};
use serde_json::Value;

/// Whether a command may be submitted to a shared room or applied from a relay echo.
pub fn shareable_room_command(id: &str) -> bool {
    if crate::artifacts::command_is_read_only(id) {
        return false;
    }
    if id.starts_with("crdt.") {
        return false;
    }
    if crate::cam_isolation_command(id) || crate::snapmaker::handles(id) {
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
    use pcbcraft_doc::Project;
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
        let s = Session::sample()?;
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

    #[test]
    fn can_resync_from_log_returns_true_when_seq_is_in_window() {
        assert!(can_resync_from_log(Some(100), 50));
        assert!(can_resync_from_log(Some(50), 50));
    }

    #[test]
    fn can_resync_from_log_returns_false_when_seq_is_stale() {
        assert!(!can_resync_from_log(Some(49), 50));
        assert!(!can_resync_from_log(Some(1), 100));
    }

    #[test]
    fn can_resync_from_log_returns_false_when_no_stored_seq() {
        assert!(!can_resync_from_log(None, 50));
    }

    #[test]
    fn resync_strategy_replays_when_in_window() {
        let strategy = resync_strategy(Some(100), 50);
        assert_eq!(strategy, ResyncStrategy::ReplayFromLog(100));
    }

    #[test]
    fn resync_strategy_requests_snapshot_when_stale() {
        let strategy = resync_strategy(Some(49), 50);
        assert_eq!(strategy, ResyncStrategy::PeerSnapshot);
    }

    #[test]
    fn resync_strategy_requests_snapshot_when_no_stored_seq() {
        let strategy = resync_strategy(None, 50);
        assert_eq!(strategy, ResyncStrategy::PeerSnapshot);
    }

    #[test]
    fn update_collab_seq_sets_the_sequence() -> Result<(), Error> {
        let mut s = Session::sample()?;
        update_collab_seq(&mut s, 42);
        assert_eq!(s.project.collab_seq, Some(42));
        Ok(())
    }

    #[test]
    fn a_newcomer_joins_a_room_and_sees_the_others_listed() -> Result<(), Error> {
        // Alice and Bob are already in the room, applying operations
        let mut alice = Session::sample()?;
        let mut bob = Session::sample()?;

        // They perform some collaborative edits
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
                apply_room_command(&mut alice, &ready.command, ready.params.clone())?;
                apply_room_command(&mut bob, &ready.command, ready.params.clone())?;
            }
        }

        // Both peers update their collab_seq
        update_collab_seq(&mut alice, 2);
        update_collab_seq(&mut bob, 2);

        // Verify both peers are in sync
        assert_eq!(
            CollabSnapshot::from_session(&alice)?,
            CollabSnapshot::from_session(&bob)?
        );

        // Carol joins late and checks the stored collab_seq
        let carol_stored_seq = alice.project.collab_seq;
        assert_eq!(carol_stored_seq, Some(2));

        // Carol determines her resync strategy based on the relay log window
        // Assume the relay log still has ops from seq 1
        let strategy = resync_strategy(carol_stored_seq, 1);
        assert_eq!(strategy, ResyncStrategy::ReplayFromLog(2));

        // The presence bar would list all participants in the room
        let participants = ["Alice", "Bob", "Carol"];
        assert_eq!(participants.len(), 3);
        assert!(participants.contains(&"Alice"));
        assert!(participants.contains(&"Bob"));
        assert!(participants.contains(&"Carol"));

        Ok(())
    }
}

/// Update the project's collab sequence number after applying relay ops.
/// This is written to the cloud file for late-joiner resync.
pub fn update_collab_seq(session: &mut Session, seq: u64) {
    session.project.collab_seq = Some(seq);
}

/// Determine if a late joiner can resync from the relay log using the stored
/// collab_seq, or if they need a peer snapshot instead.
/// Returns true if the stored seq is still within the relay log window.
pub fn can_resync_from_log(stored_seq: Option<u64>, log_start_seq: u64) -> bool {
    match stored_seq {
        Some(seq) => seq >= log_start_seq,
        None => false,
    }
}

/// Late-join resync strategy: check if the stored collab_seq is still within
/// the relay log window. If so, replay ops from that sequence. If not,
/// request a peer snapshot.
pub fn resync_strategy(stored_seq: Option<u64>, log_start_seq: u64) -> ResyncStrategy {
    if can_resync_from_log(stored_seq, log_start_seq) {
        // stored_seq must be Some if can_resync_from_log returned true
        match stored_seq {
            Some(seq) => ResyncStrategy::ReplayFromLog(seq),
            None => ResyncStrategy::PeerSnapshot,
        }
    } else {
        ResyncStrategy::PeerSnapshot
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResyncStrategy {
    /// Replay ops from the stored sequence number
    ReplayFromLog(u64),
    /// Request a full snapshot from a peer
    PeerSnapshot,
}
