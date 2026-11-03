//! Bounded multi-document routing for the control bridge and MCP tools.
use pcbcraft_engine::Session;
use pcbcraft_engine::design_document::{
    DocumentShellState, display_title, is_dirty, project_has_content,
};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::{SharedSession, invoke_session, shared, text_result, validate_tool_schema};

pub use pcbcraft_engine::design_document::MAX_OPEN_DOCUMENTS;
const MAX_PATH_CHARS: usize = 1024;

struct DocumentSlot {
    session: SharedSession,
    path: String,
    saved_revision: Option<u64>,
    shell: DocumentShellState,
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
    /// Create a hub with one document slot holding the given session.
    ///
    /// The initial path is used for display and save tracking; empty means
    /// a new unnamed design. The path must be at most 1024 characters.
    /// Returns an error if the session lock is poisoned or the path is too long.
    ///
    /// # Rules
    /// - The caller must keep the session alive as long as the hub is used.
    /// - Only one slot exists until another design is opened via the open tool.
    ///
    /// # Dependencies
    /// - crates/mcp/src/lib.rs (`SharedSession`)
    ///
    /// # Do not
    /// - Pass a path longer than 1024 characters.
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
                    shell: DocumentShellState::default(),
                }],
                active: 0,
            })),
        })
    }

    /// Return the shared session of the currently active document.
    ///
    /// Returns an error if there are no open documents or the lock is poisoned.
    ///
    /// # Rules
    /// - The caller must not hold the lock while calling this.
    /// - The returned session is the one that receives execute and tool calls.
    ///
    /// # Dependencies
    /// - crates/mcp/src/lib.rs (`SharedSession`)
    ///
    /// # Do not
    /// - Assume this returns the same session across calls if documents are opened.
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

    /// Route a tool call through the hub, dispatching by tool name.
    ///
    /// Document management tools (design.list_open, design.activate, open)
    /// are handled by the hub; all others are forwarded to the active session.
    /// Returns a JSON-RPC result object with isError set on failure.
    ///
    /// # Rules
    /// - Args must be a JSON object.
    /// - Tool names are validated against the registered schema.
    ///
    /// # Dependencies
    /// - crates/mcp/src/lib.rs (`SharedSession`, `validate_tool_schema`)
    ///
    /// # Do not
    /// - Call this for non-tool methods like initialize or ping.
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
                "name": display_title(&locked.project.name, &slot.path),
                "revision": locked.revision,
                "dirty": is_dirty(
                    slot.saved_revision,
                    locked.revision,
                    &slot.path,
                    project_has_content(&locked.project),
                ),
                "workbench_tab": slot.shell.workbench_tab.as_str(),
                "selected": slot.shell.selected,
                "selected_unit": slot.shell.selected_unit,
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
        let project = pcbcraft_io::load_for_open(Path::new(raw))?;
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
                shell: DocumentShellState::default(),
            });
            (session, index)
        };
        let (payload, revision) = {
            let mut locked = session
                .lock()
                .map_err(|_| "session unavailable after internal failure".to_string())?;
            let payload = locked.open_project(project).map_err(|e| e.to_string())?;
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
                display_title(&locked.project.name, &slot.path),
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

/// One open design as the document tab strip shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSummary {
    /// Position of the design in the hub, starting at 0.
    pub index: usize,
    /// File path, empty for a design that was never saved.
    pub path: String,
    /// Tab label: project name, else file name, else a plain untitled label.
    pub name: String,
    /// Engine revision of the design now.
    pub revision: u64,
    /// Revision last written to disk or loaded, `None` before either.
    pub saved_revision: Option<u64>,
    /// True when the design has edits that are not saved.
    pub dirty: bool,
}

impl DocumentHub {
    /// Index of the design that receives tool calls.
    ///
    /// The desktop window reads it after each call to see whether a call
    /// moved the active design, and then swaps its own per-design view.
    ///
    /// # Rules
    /// - Returns an error when the lock is poisoned.
    /// - The index is always below the number of open designs.
    ///
    /// # Dependencies
    /// - crates/mcp/src/document_hub.rs (`HubState` holds the index)
    ///
    /// # Do not
    /// - Cache the value across tool calls; `open` and `design.activate` change it.
    pub fn active_index(&self) -> Result<usize, String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?
            .active)
    }

    /// List every open design with its title and unsaved-edits flag.
    ///
    /// The document tab strip draws one tab per entry and a dot on the
    /// entries that are dirty, from the same rule the `design.list_open` tool uses.
    ///
    /// # Rules
    /// - At most eight entries, in tab order.
    /// - Returns an error when a lock is poisoned.
    ///
    /// # Dependencies
    /// - crates/engine/src/design_document.rs (`display_title`, `is_dirty`)
    ///
    /// # Do not
    /// - Hold the result across edits; ask again each frame.
    pub fn documents(&self) -> Result<Vec<DocumentSummary>, String> {
        let state = self
            .inner
            .lock()
            .map_err(|_| "session unavailable after internal failure".to_string())?;
        let mut out = Vec::with_capacity(state.slots.len());
        for (index, slot) in state.slots.iter().enumerate() {
            let locked = slot
                .session
                .lock()
                .map_err(|_| "session unavailable after internal failure".to_string())?;
            out.push(DocumentSummary {
                index,
                path: slot.path.clone(),
                name: display_title(&locked.project.name, &slot.path),
                revision: locked.revision,
                saved_revision: slot.saved_revision,
                dirty: is_dirty(
                    slot.saved_revision,
                    locked.revision,
                    &slot.path,
                    project_has_content(&locked.project),
                ),
            });
        }
        Ok(out)
    }
}

fn string_param<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string parameter: {key}"))
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
}
