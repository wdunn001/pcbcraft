//! Attachment rows: the files and notes attached to the project, shown at the
//! bottom of the Layers tree.
//!
//! Context: a project can hold text files (licences, datasheets) in
//! `Project.files` and a free-text note in `Project.meta['notes']`. This file
//! turns those into rows of the tree. The logic is pure and follows the same
//! pattern as artifact_rows.rs.
//! Local rules: the files group has key 'files' and its leaves use
//! 'files/file:<name>'; the note has key 'notes'. No sub-rows for the note.
//!
//! # Dependencies
//! - crates/ui-egui/src/layers_tree.rs (Row, RowKind, Env, row)
//! - crates/doc/src/lib.rs (Project, EmbeddedFile)
//!
//! # Do not
//! - Edit the doc from here; attachment commands live in the engine.

use crate::layers_tree::{Env, Row, RowKind, row};

/// Add the Attached files and Project notes rows to the tree.
///
/// Why: the Layers tree is the one place to find everything about the board,
/// including the files and notes the designer attached to the project.
///
/// # Rules
/// - The files group row appears only when the project has at least one file.
/// - The notes row appears only when meta['notes'] is set and non-empty.
/// - Both rows sit at the bottom, below the board layers, parts and nets.
///
/// # Dependencies
/// - crates/ui-egui/src/layers_tree.rs (children() wraps its result with this)
///
/// # Do not
/// - Add these rows to the Schematic tree; only the Layout tab shows them.
pub(crate) fn with_attachment_rows(env: &Env, key: &str, built: Vec<Row>) -> Vec<Row> {
    let project = env.project;
    let mut rows = built;
    if !key.is_empty() && key != "files" {
        return rows;
    }
    if key == "files" {
        return project.files.iter().map(file_row).collect();
    }
    // key == "": append the two rows at the bottom
    if !project.files.is_empty() {
        rows.push(section("files", "Attached files", project.files.len()));
    }
    if let Some(note) = project.meta.get("notes")
        && !note.trim().is_empty()
    {
        rows.push(notes_row(note));
    }
    rows
}

fn file_row(f: &pcbcraft_doc::EmbeddedFile) -> Row {
    let size = f.data.len() / 1024;
    let sub = if f.is_binary() {
        format!("{} KiB binary", size)
    } else {
        format!("{} KiB", size)
    };
    let mut r = row(
        "files",
        &format!("file:{}", f.name),
        RowKind::Leaf,
        f.name.clone(),
    );
    r.sub = sub;
    r.marks = false;
    r
}

fn notes_row(note: &str) -> Row {
    let mut r = row("", "notes", RowKind::Leaf, "Project notes".to_owned());
    let text = if note.len() > 40 {
        format!("{}…", &note[..40])
    } else {
        note.to_string()
    };
    r.sub = text;
    r.marks = false;
    r
}

fn section(key: &str, label: &str, n: usize) -> Row {
    let mut r = row("", key, RowKind::Group, label.to_owned());
    r.count = n.to_string();
    r.expandable = true;
    r
}
