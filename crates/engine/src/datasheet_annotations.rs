//! Host-profile commands for datasheet highlights, notes and bookmarks, plus
//! one design command that copies an entry into a component field.
use super::*;
use pcbcraft_library::datasheet_annotations::{
    self, DatasheetAnnotationsFile, DatasheetEntry, EntryKind, ImportMode,
};

pub(super) fn commands() -> Vec<CommandSpec> {
    let entry_schema = json!({
        "type":"object",
        "properties":{
            "id":{"type":"string","minLength":1,"maxLength":32},
            "kind":{"type":"string","enum":["highlight","note","bookmark"]},
            "page":{"type":"integer","minimum":1,"maximum":10000},
            "rect":{"type":"object","properties":{"x0":{"type":"number"},"y0":{"type":"number"},"x1":{"type":"number"},"y1":{"type":"number"}},"required":["x0","y0","x1","y1"],"additionalProperties":false},
            "note":{"type":"string","maxLength":2000},
            "label":{"type":"string","maxLength":128}
        },
        "required":["id","kind","page","rect"],
        "additionalProperties":false
    });
    vec![
        CommandSpec {
            id: "profile.datasheet_list",
            description: "List highlights, notes and bookmarks the user stored for one datasheet, keyed by document hash; read only on the design",
            input_schema: schema(
                json!({"document_hash":{"type":"string","minLength":8,"maxLength":64}}),
                &["document_hash"],
            ),
        },
        CommandSpec {
            id: "profile.datasheet_upsert",
            description: "Add or replace one highlight, note or bookmark on a datasheet in the user profile, keyed by document hash; never leaves this computer",
            input_schema: schema(
                json!({"document_hash":{"type":"string","minLength":8,"maxLength":64},"entry":entry_schema}),
                &["document_hash", "entry"],
            ),
        },
        CommandSpec {
            id: "profile.datasheet_remove",
            description: "Remove one stored datasheet mark by id; keyed by document hash",
            input_schema: schema(
                json!({"document_hash":{"type":"string","minLength":8,"maxLength":64},"entry_id":{"type":"string","minLength":1,"maxLength":32}}),
                &["document_hash", "entry_id"],
            ),
        },
        CommandSpec {
            id: "profile.datasheet_export",
            description: "Export stored datasheet marks as JSON from the user profile; optional document_hash limits the export to one file",
            input_schema: schema(
                json!({"document_hash":{"type":"string","minLength":8,"maxLength":64}}),
                &[],
            ),
        },
        CommandSpec {
            id: "profile.datasheet_import",
            description: "Import stored datasheet marks from an export payload into the user profile; merge keeps other documents, replace_document replaces one hash, replace_all replaces the whole store",
            input_schema: schema(
                json!({
                    "payload":{"type":"object"},
                    "mode":{"type":"string","enum":["merge","replace_document","replace_all"],"default":"merge"}
                }),
                &["payload"],
            ),
        },
        CommandSpec {
            id: "datasheet.copy_annotation_to_field",
            description: "Copy one stored datasheet note or bookmark into a component property as plain text of at most 200 characters; the mark is found by document hash and entry id in the user profile",
            input_schema: schema(
                json!({
                    "id":id_schema(),
                    "document_hash":{"type":"string","minLength":8,"maxLength":64},
                    "entry_id":{"type":"string","minLength":1,"maxLength":32},
                    "key":{"type":"string","maxLength":64,"default":"Notes"}
                }),
                &["id", "document_hash", "entry_id"],
            ),
        },
    ]
}

pub fn is_profile_command(id: &str) -> bool {
    matches!(
        id,
        "profile.datasheet_list"
            | "profile.datasheet_upsert"
            | "profile.datasheet_remove"
            | "profile.datasheet_export"
            | "profile.datasheet_import"
    )
}

pub fn run_profile(id: &str, params: &Value) -> Result<Value, String> {
    match id {
        "profile.datasheet_list" => {
            let a: ListParams = parse_host(params)?;
            let entries = datasheet_annotations::list_entries(&a.document_hash)?;
            Ok(json!({"document_hash": a.document_hash, "entries": entries}))
        }
        "profile.datasheet_upsert" => {
            let a: UpsertParams = parse_host(params)?;
            let saved = datasheet_annotations::upsert_entry(&a.document_hash, a.entry)?;
            Ok(json!({"document_hash": a.document_hash, "entry": saved}))
        }
        "profile.datasheet_remove" => {
            let a: RemoveParams = parse_host(params)?;
            let removed = datasheet_annotations::remove_entry(&a.document_hash, &a.entry_id)?;
            Ok(
                json!({"document_hash": a.document_hash, "entry_id": a.entry_id, "removed": removed}),
            )
        }
        "profile.datasheet_export" => {
            let a: ExportParams = parse_host(params)?;
            let file = datasheet_annotations::export_file(a.document_hash.as_deref())?;
            Ok(json!({"profile": file}))
        }
        "profile.datasheet_import" => {
            let a: ImportParams = parse_host(params)?;
            let mode = match a.mode.as_deref().unwrap_or("merge") {
                "merge" => ImportMode::Merge,
                "replace_document" => ImportMode::ReplaceDocument,
                "replace_all" => ImportMode::ReplaceAll,
                other => return Err(format!("unknown import mode {other}")),
            };
            let count = datasheet_annotations::import_file(&a.payload, mode)?;
            Ok(json!({"imported_entries": count}))
        }
        _ => Err(format!("unknown profile command {id}")),
    }
}

pub(super) fn run_design(
    p: &mut Project,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    if command != "datasheet.copy_annotation_to_field" {
        return None;
    }
    Some(copy_to_field(p, params))
}

fn copy_to_field(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: CopyParams = parse(params)?;
    let entry = datasheet_annotations::entry_by_id(&a.document_hash, &a.entry_id)
        .map_err(Error::Invalid)?
        .ok_or_else(|| Error::Invalid("no such datasheet entry in the profile".into()))?;
    let text = datasheet_annotations::entry_field_text(&entry);
    let key = a.key.unwrap_or_else(|| "Notes".into());
    let component = p
        .components
        .iter_mut()
        .find(|c| c.id == a.id)
        .ok_or_else(missing)?;
    let set = set_component_field(component, &key, Some(&text))?;
    Ok(json!({
        "id": a.id,
        "key": key,
        "set": set,
        "text": text,
        "entry_kind": kind_name(entry.kind),
    }))
}

fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Highlight => "highlight",
        EntryKind::Note => "note",
        EntryKind::Bookmark => "bookmark",
    }
}

fn parse_host<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, String> {
    serde_json::from_value(v.clone()).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    document_hash: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpsertParams {
    document_hash: String,
    entry: DatasheetEntry,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveParams {
    document_hash: String,
    entry_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportParams {
    #[serde(default)]
    document_hash: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportParams {
    payload: DatasheetAnnotationsFile,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CopyParams {
    id: Id,
    document_hash: String,
    entry_id: String,
    #[serde(default)]
    key: Option<String>,
}
