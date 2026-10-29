//! Copper layer names in the project file, aware of the board's stackup.
//!
//! A copper layer is an index in memory. In the file it is a name: `F.Cu` for
//! index 0, `B.Cu` for the last copper layer of the stackup, `In<i>.Cu` for
//! the inner layer at index `i`. A 4-layer board therefore saves its bottom as
//! `B.Cu`, not `In3.Cu`. Files written before this rule carry no
//! `layer_naming` marker and keep the old index-derived reading, where `B.Cu`
//! is always index 1, so every item of such a file lands on the same layer.
//!
//! Rules: the names only change inside `Project`'s own serde impls, through a
//! thread-local context that [`Layer`]'s serde reads; a `Layer` serialized on
//! its own (a command payload, one track) has no context and keeps the
//! index-derived name. The names written are positional, so they always read
//! back, whatever the rows of the stackup table are called.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs#Layer
//! - crates/doc/src/lib.rs#Project

use serde::{Deserialize, Serialize, Serializer};
use std::cell::Cell;

/// Marker value written to the `layer_naming` key of a project file.
const STACKUP_NAMING: &str = "stackup";

thread_local! {
    /// Copper layer count of the project being read or written, when its file
    /// uses stackup names.
    static COPPER_COUNT: Cell<Option<u8>> = const { Cell::new(None) };
}

/// Restores the previous context when a project read or write ends, also on
/// an early return or a panic.
pub(crate) struct Scope(Option<u8>);

impl Scope {
    /// Start a context where layer names follow a stackup of `count` copper layers.
    pub(crate) fn enter(count: u8) -> Scope {
        Scope(COPPER_COUNT.with(|c| c.replace(Some(count))))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        COPPER_COUNT.with(|c| c.set(self.0));
    }
}

/// Copper layer count of the stackup in force, if any.
pub(crate) fn current_count() -> Option<u8> {
    COPPER_COUNT.with(Cell::get)
}

/// File name of copper layer `index` on a board of `count` copper layers, or
/// none when the index lies outside the stackup.
pub(crate) fn stackup_name(index: u8, count: u8) -> Option<String> {
    if index >= count {
        None
    } else if index == 0 {
        Some("F.Cu".into())
    } else if index + 1 == count {
        Some("B.Cu".into())
    } else {
        Some(format!("In{index}.Cu"))
    }
}

/// Copper layer index for a file name on a board of `count` copper layers,
/// or none when the name is not one of the stackup names.
pub(crate) fn stackup_index(name: &str, count: u8) -> Option<u8> {
    match name {
        "F.Cu" => Some(0),
        "B.Cu" => count.checked_sub(1),
        _ => None,
    }
}

/// Number of copper rows in a project's `board.layer_records`, read from the
/// parsed file; two when the table has none, as in `Project::copper_layer_count`.
pub(crate) fn copper_count_of(file: &serde_json::Value) -> u8 {
    let rows = file
        .pointer("/board/layer_records")
        .and_then(serde_json::Value::as_array);
    let n = rows.map_or(0, |rows| {
        rows.iter()
            .filter(|r| r.get("kind").and_then(serde_json::Value::as_str) == Some("copper"))
            .count()
    });
    match n {
        0 => 2,
        n => n.min(usize::from(crate::MAX_COPPER_LAYERS)) as u8,
    }
}

/// True when the parsed file says its layers are named by stackup.
pub(crate) fn file_uses_stackup_names(file: &serde_json::Value) -> bool {
    file.get("layer_naming").and_then(serde_json::Value::as_str) == Some(STACKUP_NAMING)
}

/// Marker field of [`crate::Project`]: always written as `"stackup"`, never
/// read back as data (the loader checks the key before it parses layers).
///
/// # Rules
/// - Carries no state; every save writes it, every load ignores it.
///
/// # Dependencies
/// - crates/doc/src/lib.rs#Project
///
/// # Do not
/// - Remove it from saved files: without it a reader takes `B.Cu` as index 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayerNaming;

impl Serialize for LayerNaming {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(STACKUP_NAMING)
    }
}

impl<'de> Deserialize<'de> for LayerNaming {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(deserializer).map(|_| LayerNaming)
    }
}
