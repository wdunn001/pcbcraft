use serde_json::Value;
use std::collections::BTreeMap;

/// One relay-ordered edit waiting to land on every peer.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomOp {
    pub seq: u64,
    pub local: Option<u64>,
    pub command: String,
    pub params: Value,
}

/// Holds relay ops until their sequence predecessors have been applied.
///
/// PCBCraft does not apply local edits optimistically: the hub applies whatever this drains, in
/// order, after the relay assigns `seq`.
#[derive(Clone, Debug, Default)]
pub struct OrderedRoomApply {
    applied: u64,
    pending: BTreeMap<u64, RoomOp>,
}

impl OrderedRoomApply {
    #[must_use]
    pub fn new(applied: u64) -> Self {
        Self {
            applied,
            pending: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn applied(&self) -> u64 {
        self.applied
    }

    /// Accept one relay op and return every op that is now safe to apply, in order.
    pub fn ingest(&mut self, op: RoomOp) -> Vec<RoomOp> {
        if op.seq <= self.applied {
            return Vec::new();
        }
        if op.seq > self.applied + 1 {
            self.pending.insert(op.seq, op);
            return Vec::new();
        }
        let mut ready = vec![op];
        self.applied = ready[0].seq;
        while let Some(next) = self.pending.remove(&(self.applied + 1)) {
            self.applied = next.seq;
            ready.push(next);
        }
        ready
    }
}
