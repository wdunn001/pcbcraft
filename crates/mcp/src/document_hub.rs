//! Bounded multi-document routing for the control bridge and MCP tools.
use pcbcraft_engine::Session;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::{SharedSession, invoke_session, shared, text_result, validate_tool_schema};

pub const MAX_OPEN_DOCUMENTS: usize = 8;
const MAX_PATH_CHARS: usize = 1024;
const UNTITLED: &str = "Untitled design";
const DEFAULT_PROJECT_NAME: &str = "Untitled circuit";

struct DocumentSlot {
    session: SharedSession,
    path: String,
    saved_revision: Option<u64>,
}

struct HubState {
    slots: Vec<DocumentSlot>,
    active: usize,
}

/// Up to eight engine sessions with one active index for MCP and the desktop bridge.
#[derive(Clone)]
pub struct DocumentHub {
    inner: Arc<Mutex<HubState>>,
}

impl DocumentHub {
    pub fn new(session: Session, initial_path: Option<String>) -> Result<Self, String> {
        let shared = shared(session);
        let (revision, path) = {
            let locked = shared
                .lock()
                .map_err(|_| "session unavailable after internal failure".to_string())?;
            let path = initial_path.unwrap_or_default();
            if path.len() > MAX_PATH_CHARS {
                return Err("path must be at most 1024 characters".into());
            }
            (locked.revision, path)
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(HubState {
                slots: vec![DocumentSlot {
                    session: shared,
                    path,
                    saved_revision: Some(revision),
                }],
                active: 0,
            })),
        })
    }

    pub fn active_session(&self) -> Result<SharedSession, String> {
        let state = self
            .inner
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?;
        state
            .slots
            .get(state.active)
            .map(|slot| slot.session.clone())
            .ok_or_else(|| "no open documents".into())
    }

    pub fn call_tool(&self, name: &str, args: &Value) -> Value {
        match self.invoke(name, args) {
            Ok(v) => v,
            Err(e) => crate::error_result(e),
        }
    }

    fn invoke(&self, name: &str, args: &Value) -> Result<Value, String> {
        if !args.is_object() {
            return Err("arguments must be an object".into());
        }
        validate_tool_schema(name, args)?;
        match name {
            "design.list_open" => self.list_open(),
            "design.activate" => self.activate(args),
            "open" => self.open_path(string_param(args, "path")?),
            "inspect_design" => self.inspect_active(),
            _ => {
                let session = self.active_session()?;
                let result = invoke_session(&session, name, args)?;
                if name == "save" {
                    self.sync_active_saved()?;
                } else if name == "new_design" || name == "sample_design" {
                    self.set_active_path(String::new())?;
                    self.sync_active_saved()?;
                }
                Ok(result)
            }
        }
    }

    fn list_open(&self) -> Result<Value, String> {
        let state = self
            .inner
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?;
        if state.slots.is_empty() {
            return Err("no open documents".into());
        }
        let mut documents = Vec::with_capacity(state.slots.len());
        for (index, slot) in state.slots.iter().enumerate() {
            let locked = slot
                .session
                .lock()
                .map_err(|_| "session unavailable after internal failure".to_string())?;
            documents.push(json!({
                "index": index,
                "path": slot.path,
                "name": slot_display_name(&locked.project.name, &slot.path),
                "revision": locked.revision,
                "dirty": slot.saved_revision != Some(locked.revision),
            }));
        }
        Ok(text_result(json!({"documents": documents})))
    }

    fn activate(&self, args: &Value) -> Result<Value, String> {
        let has_index = args.get("index").is_some();
        let has_path = args
            .get("path")
            .is_some_and(|v| v.is_string() && !v.as_str().is_some_and(str::is_empty));
        if has_index == has_path {
            return Err("provide exactly one of index or path".into());
        }
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?;
        if state.slots.is_empty() {
            return Err("no open documents".into());
        }
        let target = if has_index {
            let index = args
                .get("index")
                .and_then(Value::as_u64)
                .ok_or("index must be a nonnegative integer")? as usize;
            if index >= state.slots.len() {
                return Err("document index out of range".into());
            }
            index
        } else {
            let raw = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or("path must be a string")?;
            let normalized = normalize_path(raw)?;
            state
                .slots
                .iter()
                .position(|slot| paths_match(&slot.path, &normalized))
                .ok_or("no open document with that path")?
        };
        state.active = target;
        Ok(text_result(json!({"active_index": target})))
    }

    fn open_path(&self, raw: &str) -> Result<Value, String> {
        let normalized = normalize_path(raw)?;
        let project = pcbcraft_io::load(Path::new(raw))?;
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?;
        if let Some(index) = state
            .slots
            .iter()
            .position(|slot| paths_match(&slot.path, &normalized))
        {
            state.active = index;
            let revision = state.slots[index]
                .session
                .lock()
                .map_err(|_| "session unavailable after internal failure".to_string())?
                .revision;
            return Ok(text_result(json!({"revision": revision})));
        }
        let (session, index) = if state.slots.len() == 1 && state.slots[0].path.is_empty() {
            (state.slots[0].session.clone(), 0)
        } else if state.slots.len() >= MAX_OPEN_DOCUMENTS {
            return Err(format!(
                "at most {MAX_OPEN_DOCUMENTS} designs can be open at once"
            ));
        } else {
            let session = shared(Session::default());
            let index = state.slots.len();
            state.slots.push(DocumentSlot {
                session: session.clone(),
                path: String::new(),
                saved_revision: None,
            });
            (session, index)
        };
        let (payload, revision) = {
            let mut locked = session
                .lock()
                .map_err(|_| "session unavailable after internal failure".to_string())?;
            let payload = locked.replace(project).map_err(|e| e.to_string())?;
            (payload, locked.revision)
        };
        state.slots[index].path = normalized;
        state.slots[index].saved_revision = Some(revision);
        state.active = index;
        Ok(text_result(payload))
    }

    fn inspect_active(&self) -> Result<Value, String> {
        let (session, path, name, active_index) = {
            let state = self
                .inner
                .lock()
                .map_err(|_| "session unavailable after internal failure".to_string())?;
            let index = state.active;
            let slot = state.slots.get(index).ok_or("no open documents")?;
            let locked = slot
                .session
                .lock()
                .map_err(|_| "session unavailable after internal failure".to_string())?;
            (
                slot.session.clone(),
                slot.path.clone(),
                slot_display_name(&locked.project.name, &slot.path),
                index,
            )
        };
        let locked = session
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?;
        let mut payload = locked.inspect();
        if let Some(map) = payload.as_object_mut() {
            map.insert("active_index".into(), json!(active_index));
            map.insert("path".into(), json!(path));
            map.insert("name".into(), json!(name));
        }
        Ok(text_result(payload))
    }

    fn sync_active_saved(&self) -> Result<(), String> {
        let index = self
            .inner
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?
            .active;
        let revision = self
            .active_session()?
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?
            .revision;
        self.set_slot_saved(index, revision)
    }

    fn set_slot_saved(&self, index: usize, revision: u64) -> Result<(), String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?;
        let slot = state
            .slots
            .get_mut(index)
            .ok_or("document index out of range")?;
        slot.saved_revision = Some(revision);
        Ok(())
    }

    fn set_active_path(&self, path: String) -> Result<(), String> {
        if path.len() > MAX_PATH_CHARS {
            return Err("path must be at most 1024 characters".into());
        }
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?;
        let active = state.active;
        let slot = state.slots.get_mut(active).ok_or("no open documents")?;
        slot.path = path;
        Ok(())
    }
}

fn string_param<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string parameter: {key}"))
}

fn slot_display_name(project_name: &str, path: &str) -> String {
    let trimmed = project_name.trim();
    if !trimmed.is_empty() && trimmed != DEFAULT_PROJECT_NAME {
        return trimmed.to_string();
    }
    if !path.is_empty()
        && let Some(name) = Path::new(path).file_name().and_then(|s| s.to_str())
    {
        return name.to_string();
    }
    UNTITLED.to_string()
}

fn normalize_path(raw: &str) -> Result<String, String> {
    if raw.is_empty() || raw.len() > MAX_PATH_CHARS {
        return Err("path must be 1 to 1024 characters".into());
    }
    let path = Path::new(raw);
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical.to_string_lossy().into_owned());
    }
    Ok(raw.to_string())
}

fn paths_match(stored: &str, candidate: &str) -> bool {
    if stored == candidate {
        return true;
    }
    if stored.is_empty() || candidate.is_empty() {
        return false;
    }
    match (normalize_path(stored), normalize_path(candidate)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
impl DocumentHub {
    pub fn session_revision(&self, index: usize) -> Result<u64, String> {
        let state = self
            .inner
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?;
        let slot = state
            .slots
            .get(index)
            .ok_or("document index out of range")?;
        Ok(slot
            .session
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?
            .revision)
    }

    pub fn active_index(&self) -> Result<usize, String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?
            .active)
    }
}
