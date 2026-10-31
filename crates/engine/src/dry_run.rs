//! Preview of a batch (`Session::dry_run`) and one-step merging of undo history.
//!
//! Context: the assistant gate shows what a batch would change before the user allows it, and
//! squashes everything one message changed into a single undo step. Both live beside `batch()`
//! in `lib.rs` and use the same `run()`.
//! Rules: a dry run never touches the session (revision, project, undo, redo and `next_id` stay);
//! lists are bounded at `LIST_LIMIT` entries with the overflow counted in `more`.
//! The summary also carries what the review card shows: counts per kind, new net names, new
//! track length, untouched objects and the saved size before and after.
//! Do not: turn `squash_undo_since` into a tool; outside agents must not rewrite history.

use super::{Command, Error, Session, run};
use pcbcraft_doc::Project;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// Most entries kept in each list of a [`ChangeSummary`].
pub const LIST_LIMIT: usize = 64;

/// What a batch would change, found by comparing a scratch copy with the project on stable ids.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ChangeSummary {
    /// New objects as (kind, reference or id).
    pub added: Vec<(String, String)>,
    /// Objects the batch would delete, as (kind, reference or id).
    pub removed: Vec<(String, String)>,
    /// Objects whose only difference is their position.
    pub moved: Vec<(String, String)>,
    /// Objects changed in more than their position.
    pub changed: Vec<(String, String)>,
    /// Nets added, removed or changed.
    pub nets_changed: usize,
    /// True when the board itself (size, outline, rules) would change.
    pub board_changed: bool,
    /// Entries left out of the lists above because of `LIST_LIMIT`.
    pub more: usize,
    /// How many objects of each kind are new, counted past `LIST_LIMIT` (for the review card).
    pub added_counts: BTreeMap<String, usize>,
    /// Names of the nets the batch would add, at most `LIST_LIMIT`.
    pub nets_added: Vec<String>,
    /// Total length in millimetres of the new tracks, straight start to end (an arc counts as its chord).
    pub track_mm_added: f64,
    /// Objects the batch leaves exactly as they are.
    pub untouched: usize,
    /// Objects in the project before the batch (the base of `untouched`).
    pub total: usize,
    /// Size of the saved project before the batch, in bytes.
    pub bytes_before: usize,
    /// Size of the saved project after the batch, in bytes.
    pub bytes_after: usize,
}

/// The results of the commands plus what they would change.
#[derive(Clone, Debug, Serialize)]
pub struct DryRun {
    /// One result per command, as the commands would return them.
    pub results: Vec<Value>,
    /// What the batch would change.
    pub summary: ChangeSummary,
}

impl Session {
    /// Run `commands` on a copy and report what would change. Same checks as `batch()`.
    ///
    /// # Rules
    /// A stale `expected` revision or 0 or more than 128 commands is refused as in `batch()`;
    /// the session is untouched either way.
    ///
    /// # Dependencies
    /// `run()` in `lib.rs`; `Project::validate` in `../../doc/src/lib.rs`.
    ///
    /// # Do not
    /// Commit the candidate: that is `batch()`.
    pub fn dry_run(&self, commands: &[Command], expected: Option<u64>) -> Result<DryRun, Error> {
        self.check_revision(expected)?;
        if commands.is_empty() || commands.len() > 128 {
            return Err(Error::Invalid("batch must contain 1–128 commands".into()));
        }
        let mut candidate = self.project.clone();
        let mut next = self.next_id;
        let mut results = vec![];
        for command in commands {
            results.push(run(
                &mut candidate,
                &mut next,
                &command.command,
                &command.params,
                self.revision.saturating_add(1),
                true,
            )?);
            candidate.validate()?;
        }
        Ok(DryRun {
            results,
            summary: summarize(&self.project, &candidate),
        })
    }

    /// How many undo steps exist now.
    pub fn history_len(&self) -> usize {
        self.undo.len()
    }

    /// Merge every undo step made since the history had `len` entries into one, so a whole
    /// message undoes together. Keeps the project from before the message; returns how many steps
    /// were merged. If the history was trimmed below `len`, nothing is merged and 0 is returned.
    /// `next_id` is untouched, so ids are never reused.
    pub fn squash_undo_since(&mut self, len: usize) -> usize {
        if self.undo.len() <= len.saturating_add(1) || len >= self.undo.len() {
            return 0;
        }
        let merged = self.undo.len() - len - 1;
        self.undo.truncate(len + 1);
        merged
    }
}

/// Rows of the collections that are arrays of objects with an `id`, keyed by id.
fn rows<'a>(project: &'a Value) -> BTreeMap<(String, u64), &'a Value> {
    let mut out = BTreeMap::new();
    let mut take = |map: &'a serde_json::Map<String, Value>| {
        for (name, value) in map {
            if let Some(list) = value.as_array() {
                for row in list {
                    if let Some(id) = row.get("id").and_then(Value::as_u64) {
                        out.insert((name.clone(), id), row);
                    }
                }
            }
        }
    };
    if let Some(map) = project.as_object() {
        take(map);
        // Tracks, vias and zones live inside the board object.
        if let Some(board) = map.get("board").and_then(Value::as_object) {
            take(board);
        }
    }
    out
}

fn label(row: &Value, id: u64) -> String {
    ["reference", "name", "text"]
        .iter()
        .find_map(|k| {
            row.get(*k)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        })
        .map_or_else(|| id.to_string(), str::to_string)
}

fn kind(collection: &str) -> String {
    collection.trim_end_matches('s').replace('_', " ")
}

/// True when the two rows differ only in their `position` field.
fn only_moved(a: &Value, b: &Value) -> bool {
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else {
        return false;
    };
    a.get("position") != b.get("position")
        && a.keys()
            .chain(b.keys())
            .filter(|k| *k != "position")
            .all(|k| a.get(k) == b.get(k))
}

/// Straight-line length of a track row, 0 when its ends are missing.
fn track_mm(row: &Value) -> f64 {
    let at = |end: &str, axis: &str| {
        row.get(end)
            .and_then(|p| p.get(axis))
            .and_then(Value::as_f64)
    };
    match (
        at("start", "x"),
        at("start", "y"),
        at("end", "x"),
        at("end", "y"),
    ) {
        (Some(a), Some(b), Some(c), Some(d)) => (c - a).hypot(d - b),
        _ => 0.0,
    }
}

fn push(list: &mut Vec<(String, String)>, more: &mut usize, entry: (String, String)) {
    if list.len() < LIST_LIMIT {
        list.push(entry);
    } else {
        *more += 1;
    }
}

fn summarize(before: &Project, after: &Project) -> ChangeSummary {
    let (Ok(a), Ok(b)) = (serde_json::to_value(before), serde_json::to_value(after)) else {
        return ChangeSummary::default();
    };
    let (old, new) = (rows(&a), rows(&b));
    let mut s = ChangeSummary {
        board_changed: a.get("board") != b.get("board"),
        total: old.len(),
        bytes_before: a.to_string().len(),
        bytes_after: b.to_string().len(),
        ..ChangeSummary::default()
    };
    for ((name, id), row) in &new {
        let entry = (kind(name), label(row, *id));
        match old.get(&(name.clone(), *id)) {
            None if name == "nets" => {
                s.nets_changed += 1;
                if s.nets_added.len() < LIST_LIMIT {
                    s.nets_added.push(entry.1);
                }
            }
            None => {
                *s.added_counts.entry(entry.0.clone()).or_default() += 1;
                if name == "tracks" {
                    s.track_mm_added += track_mm(row);
                }
                push(&mut s.added, &mut s.more, entry);
            }
            Some(was) if was == row => s.untouched += 1,
            Some(_) if name == "nets" => s.nets_changed += 1,
            Some(was) if only_moved(was, row) => push(&mut s.moved, &mut s.more, entry),
            Some(_) => push(&mut s.changed, &mut s.more, entry),
        }
    }
    for ((name, id), row) in &old {
        if new.contains_key(&(name.clone(), *id)) {
            continue;
        }
        if name == "nets" {
            s.nets_changed += 1;
        } else {
            push(&mut s.removed, &mut s.more, (kind(name), label(row, *id)));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn add(reference: &str, x: f64) -> Command {
        Command {
            command: "component.add".into(),
            params: json!({"kind":"resistor","reference":reference,"value":"10k","position":{"x":x,"y":20.0}}),
        }
    }

    #[test]
    fn dry_run_reports_what_a_batch_would_change_and_changes_nothing() -> Result<(), Error> {
        let s = Session::default();
        let dry = s.dry_run(&[add("R1", 10.0)], Some(0))?;
        assert_eq!(s.revision, 0);
        assert!(s.project.components.is_empty());
        assert!(
            dry.summary
                .added
                .iter()
                .any(|(k, r)| k == "component" && r == "R1")
        );
        assert_eq!(s.history_len(), 0);
        Ok(())
    }

    #[test]
    fn dry_run_counts_board_tracks_vias_and_unchanged_objects() -> Result<(), Error> {
        let mut s = Session::default();
        assert!(s.batch(&[add("R1", 10.0)], None).is_ok());
        let net = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("net id".into()))?;
        let dry = s.dry_run(
            &[Command {
                command: "track.add".into(),
                params: json!({"net":net,"start":{"x":0.0,"y":0.0},"end":{"x":30.0,"y":40.0},"width":0.2}),
            }],
            Some(s.revision),
        )?;
        assert_eq!(dry.summary.added_counts.get("track"), Some(&1));
        assert!((dry.summary.track_mm_added - 50.0).abs() < 1e-6);
        assert_eq!(dry.summary.untouched, 3, "R1 and its pads are unchanged");
        assert_eq!(dry.summary.total, 4);
        assert!(dry.summary.bytes_after > dry.summary.bytes_before);
        let via = s.dry_run(
            &[Command {
                command: "via.place".into(),
                params: json!({"net":net,"position":{"x":5.0,"y":5.0},"diameter":0.8,"drill":0.4}),
            }],
            Some(s.revision),
        )?;
        assert_eq!(via.summary.added_counts.get("via"), Some(&1));
        Ok(())
    }

    #[test]
    fn dry_run_lists_new_net_names() -> Result<(), Error> {
        let s = Session::default();
        let dry = s.dry_run(
            &[Command {
                command: "net.create".into(),
                params: json!({"name":"CLK"}),
            }],
            None,
        )?;
        assert_eq!(dry.summary.nets_added, vec!["CLK".to_string()]);
        assert_eq!(dry.summary.nets_changed, 1);
        Ok(())
    }

    #[test]
    fn dry_run_refuses_a_stale_revision_and_an_empty_batch() {
        let s = Session::default();
        assert!(matches!(
            s.dry_run(&[add("R1", 1.0)], Some(5)),
            Err(Error::Stale { .. })
        ));
        assert!(s.dry_run(&[], None).is_err());
    }

    #[test]
    fn squash_undo_since_merges_three_steps_into_one_and_keeps_ids() {
        let mut s = Session::default();
        let start = s.history_len();
        for (i, r) in ["R1", "R2", "R3"].iter().enumerate() {
            assert!(s.batch(&[add(r, 10.0 + i as f64)], None).is_ok());
        }
        let ids = s
            .project
            .components
            .iter()
            .map(|c| c.id)
            .collect::<Vec<_>>();
        assert_eq!(s.squash_undo_since(start), 2);
        assert!(s.undo().is_ok());
        assert!(
            s.project.components.is_empty(),
            "one undo takes back all three"
        );
        assert!(s.redo().is_ok());
        assert_eq!(
            s.project
                .components
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            ids
        );
    }

    #[test]
    fn squash_undo_since_merges_nothing_when_the_history_was_trimmed() {
        let mut s = Session::default();
        assert!(s.batch(&[add("R1", 1.0)], None).is_ok());
        let n = s.history_len();
        assert_eq!(s.squash_undo_since(n + 40), 0);
        assert_eq!(s.squash_undo_since(n), 0);
        assert_eq!(s.history_len(), n);
    }
}
