//! Host-side compliance list snapshots: bundled lists, user imports, and
//! read-only matching helpers exposed as engine queries.
//!
//! # Rules
//! - List content lives on disk under the user config folder, not in the project file.
//! - Commands here do not change the design revision or undo history.
use pcbcraft_io::{
    EntityMatchConfidence, ListUpdateSummary, builtin_compliance_list, builtin_compliance_list_ids,
    compliance_list_from_toml, compliance_list_to_toml, diff_compliance_lists, match_entity_name,
    snapshot_staleness_note,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub static LISTS_DIR_TEST: Mutex<Option<PathBuf>> = Mutex::new(None);
pub static LISTS_TEST_LOCK: Mutex<()> = Mutex::new(());

const MAX_PATH: usize = 4096;

pub fn command_is_host_only(id: &str) -> bool {
    matches!(
        id,
        "compliance.lists"
            | "compliance.lists_update"
            | "compliance.entity_match"
            | "compliance.list_staleness"
    )
}

fn lists_dir() -> Result<PathBuf, String> {
    if let Some(path) = LISTS_DIR_TEST
        .lock()
        .ok()
        .and_then(|locked| locked.as_ref().cloned())
    {
        return Ok(path);
    }
    let directory = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    directory("XDG_CONFIG_HOME")
        .or_else(|| directory("APPDATA"))
        .or_else(|| directory("HOME").map(|home| home.join(".config")))
        .map(|base| base.join("pcbcraft").join("compliance-lists"))
        .ok_or_else(|| "no user configuration folder was found".into())
}

fn list_path(id: &str) -> Result<PathBuf, String> {
    Ok(lists_dir()?.join(format!("{id}.toml")))
}

fn load_installed(id: &str) -> Result<pcbcraft_doc::ComplianceListSnapshot, String> {
    let path = list_path(id)?;
    if path.is_file() {
        let text = pcbcraft_io::read_text(&path)?;
        return compliance_list_from_toml(&text);
    }
    builtin_compliance_list(id)
}

fn load_all_installed() -> Result<BTreeMap<String, pcbcraft_doc::ComplianceListSnapshot>, String> {
    let mut out = BTreeMap::new();
    for id in builtin_compliance_list_ids() {
        out.insert((*id).to_owned(), load_installed(id)?);
    }
    if let Ok(dir) = lists_dir() {
        if dir.is_dir() {
            for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "toml")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    let text = pcbcraft_io::read_text(&path)?;
                    let snapshot = compliance_list_from_toml(&text)?;
                    out.insert(stem.to_owned(), snapshot);
                }
            }
        }
    }
    Ok(out)
}

fn write_installed(snapshot: &pcbcraft_doc::ComplianceListSnapshot) -> Result<(), String> {
    snapshot.validate()?;
    let dir = lists_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let text = compliance_list_to_toml(snapshot)?;
    pcbcraft_io::atomic_write(&list_path(&snapshot.id)?, text.as_bytes())
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListsUpdateParams {
    #[serde(default)]
    path: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    allow_network: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntityMatchParams {
    list_id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StalenessParams {
    list_id: String,
    today: String,
}

fn read_update_body(params: &ListsUpdateParams) -> Result<String, String> {
    if !params.text.is_empty() {
        return Ok(params.text.clone());
    }
    if !params.path.is_empty() {
        if params.path.len() > MAX_PATH {
            return Err("import path must be 1 to 4096 bytes".into());
        }
        return pcbcraft_io::read_text(Path::new(&params.path));
    }
    if !params.url.is_empty() {
        if !params.allow_network {
            return Err("network use was not allowed for this list update".into());
        }
        #[cfg(target_arch = "wasm32")]
        {
            return Err("list updates from a URL are not available in the browser build".into());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            return fetch_list_url(&params.url);
        }
    }
    Err("compliance.lists_update needs path, text or url".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_list_url(url: &str) -> Result<String, String> {
    if url.len() > 2048 {
        return Err("list url must be at most 2048 bytes".into());
    }
    if !url.starts_with("https://") {
        return Err("list url must use https".into());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .redirects(2)
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|e| format!("list download failed: {e}"))?;
    let mut body = String::new();
    response
        .into_reader()
        .take(pcbcraft_doc::MAX_COMPLIANCE_LIST_BYTES as u64)
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    Ok(body)
}

#[cfg(not(target_arch = "wasm32"))]
use std::io::Read;

pub fn run_host_command(command: &str, params: &Value) -> Result<Value, String> {
    match command {
        "compliance.lists" => lists_summary(),
        "compliance.lists_update" => lists_update(params),
        "compliance.entity_match" => entity_match(params),
        "compliance.list_staleness" => list_staleness(params),
        _ => Err(format!("unknown compliance command: {command}")),
    }
}

fn lists_summary() -> Result<Value, String> {
    let lists = load_all_installed()?;
    let rows: Vec<Value> = lists
        .values()
        .map(|snapshot| {
            json!({
                "id": snapshot.id,
                "title": snapshot.title,
                "retrieved": snapshot.retrieved_date,
                "cadence_days": snapshot.cadence_days,
                "entity_count": snapshot.entities.len(),
                "substance_count": snapshot.substances.len(),
            })
        })
        .collect();
    Ok(json!({"lists": rows, "count": rows.len()}))
}

fn lists_update(params: &Value) -> Result<Value, String> {
    let a: ListsUpdateParams = serde_json::from_value(params.clone())
        .map_err(|e| format!("compliance.lists_update params: {e}"))?;
    let text = read_update_body(&a)?;
    let incoming = compliance_list_from_toml(&text)?;
    let before = load_installed(&incoming.id).ok();
    write_installed(&incoming)?;
    let summary = if let Some(ref old) = before {
        diff_compliance_lists(old, &incoming)?
    } else {
        ListUpdateSummary {
            list_id: incoming.id.clone(),
            entities_added: incoming.entities.iter().map(|e| e.name.clone()).collect(),
            substances_added: incoming
                .substances
                .iter()
                .filter_map(|s| {
                    if s.name.is_empty() {
                        if s.cas.is_empty() {
                            Some(s.ec_number.clone())
                        } else {
                            Some(s.cas.clone())
                        }
                    } else {
                        Some(s.name.clone())
                    }
                })
                .collect(),
            ..Default::default()
        }
    };
    Ok(json!({
        "id": incoming.id,
        "retrieved": incoming.retrieved_date,
        "entities_added": summary.entities_added,
        "entities_removed": summary.entities_removed,
        "substances_added": summary.substances_added,
        "substances_removed": summary.substances_removed,
    }))
}

fn entity_match(params: &Value) -> Result<Value, String> {
    let a: EntityMatchParams = serde_json::from_value(params.clone())
        .map_err(|e| format!("compliance.entity_match params: {e}"))?;
    if a.list_id.is_empty() || a.list_id.len() > 64 {
        return Err("list_id must be 1 to 64 bytes".into());
    }
    if a.name.is_empty() || a.name.len() > 256 {
        return Err("name must be 1 to 256 bytes".into());
    }
    let snapshot = load_installed(&a.list_id)?;
    let matched = match_entity_name(&snapshot, &a.name);
    Ok(match matched {
        Some(m) => json!({
            "matched": true,
            "entry_name": m.entry_name,
            "confidence": match m.confidence {
                EntityMatchConfidence::Exact => "exact",
                EntityMatchConfidence::Possible => "possible",
            },
        }),
        None => json!({"matched": false}),
    })
}

fn list_staleness(params: &Value) -> Result<Value, String> {
    let a: StalenessParams = serde_json::from_value(params.clone())
        .map_err(|e| format!("compliance.list_staleness params: {e}"))?;
    if a.list_id.is_empty() || a.list_id.len() > 64 {
        return Err("list_id must be 1 to 64 bytes".into());
    }
    if a.today.is_empty() || a.today.len() > 32 {
        return Err("today must be 1 to 32 bytes".into());
    }
    let snapshot = load_installed(&a.list_id)?;
    let note = snapshot_staleness_note(&snapshot, &a.today);
    Ok(json!({
        "id": snapshot.id,
        "retrieved": snapshot.retrieved_date,
        "cadence_days": snapshot.cadence_days,
        "stale": note.is_some(),
        "note": note,
    }))
}
