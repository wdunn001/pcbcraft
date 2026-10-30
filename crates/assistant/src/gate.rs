//! The gate between the model and the design: a verdict for every tool call.
//!
//! Context: adapted from SolidCraft (MIT OR Apache-2.0), commit 32558d0a,
//! `crates/ui-egui/src/agent_chat/gate.rs`, mapped to PCBCraft's tool names from
//! `pcbcraft_mcp::tool_list()` (`../../mcp/src/lib.rs`).
//! Rules: reads run in every mode; design edits ask in mode Ask; file and session actions ask
//! outside mode All; a name not in the tool list, or arguments the engine's schema rejects, are
//! refused with the engine's reason. Every tool in the list gets a verdict in every mode.
//! Do not: let a tool that writes a file or replaces the design into the read list.

use serde_json::Value;

/// How much the assistant may do without asking.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Ord,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum Mode {
    /// Ask before every edit.
    #[default]
    Ask,
    /// Edits to the design run; files and sessions still ask.
    Edits,
    /// Everything runs.
    All,
}

impl Mode {
    /// Every mode, in the order Settings lists them.
    pub const ALL: [Mode; 3] = [Mode::Ask, Mode::Edits, Mode::All];

    /// The short id kept in files.
    pub fn id(self) -> &'static str {
        match self {
            Mode::Ask => "ask",
            Mode::Edits => "edits",
            Mode::All => "all",
        }
    }

    /// The mode for an id from [`Mode::id`].
    pub fn parse(id: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.id() == id)
    }

    /// The words Settings shows.
    pub fn label(self) -> &'static str {
        match self {
            Mode::Ask => "Ask every time",
            Mode::Edits => "Auto-allow design edits",
            Mode::All => "Allow everything",
        }
    }
}

/// What to do with one tool call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Run it now; `change` is true when it edits the design (counts toward the turn limit).
    Run {
        /// True when the call edits the design.
        change: bool,
    },
    /// Wait for the user; the text says why.
    Ask(String),
    /// Do not run it; the text is the reason given to the model.
    Refuse(String),
}

/// Tools that only read.
const READS: &[&str] = &[
    "list_commands",
    "inspect_design",
    "checks",
    "screenshot",
    "board_mesh",
    "design.summary",
    "library.catalog",
    "library.search",
    "library.list",
    "library.inspect_symbol",
    "library.inspect_footprint",
    "library.sources",
    "library.read_part_sidecar",
    "library.compare",
    "design.recents",
];
/// Tools that touch files or replace the whole design.
const SESSION_ACTIONS: &[&str] = &[
    "open",
    "save",
    "export",
    "new_design",
    "sample_design",
    "library.add_folder",
    "library.remove_folder",
    "library.set_source_role",
    "library.save",
];
/// Parameter names that always hold a file or folder.
pub const FILE_KEYS: &[&str] = &[
    "path",
    "directory",
    "dir",
    "file",
    "filename",
    "folder",
    "output_path",
];
/// Parameter names that sometimes hold a file name.
pub const AMBIGUOUS_KEYS: &[&str] = &["name", "source", "target"];
/// Endings that make an ambiguous value read like a file name.
pub const FILE_EXTENSIONS: &[&str] = &[
    ".pcbcraft",
    ".zip",
    ".csv",
    ".txt",
    ".svg",
    ".png",
    ".pdf",
    ".dxf",
    ".json",
    ".kicad_pcb",
    ".kicad_mod",
    ".kicad_sym",
    ".pcbcraft-lib",
];

/// The most commands one batch may carry.
const BATCH_LIMIT: usize = 128;

fn is_file_tool(tool: &str) -> bool {
    SESSION_ACTIONS.contains(&tool)
        || tool.ends_with(".write")
        || tool.ends_with(".export")
        || tool.ends_with("_svg")
        || tool.starts_with("library.export_")
}

fn reads_like_a_file(value: &str) -> bool {
    value.contains('/')
        || value.contains('\\')
        || FILE_EXTENSIONS
            .iter()
            .any(|e| value.to_ascii_lowercase().ends_with(e))
}

/// True when a command's parameters name a file or folder.
fn params_touch_files(params: &Value) -> bool {
    let Some(map) = params.as_object() else {
        return false;
    };
    map.iter().any(|(k, v)| {
        (FILE_KEYS.contains(&k.as_str()) && v.is_string())
            || (AMBIGUOUS_KEYS.contains(&k.as_str()) && v.as_str().is_some_and(reads_like_a_file))
    })
}

/// True when an engine command writes symbol, footprint and sidecar files on disk.
fn command_writes_folder_library(command: &str, params: &Value) -> bool {
    command == "part.save"
        && params
            .get("target")
            .and_then(Value::as_str)
            .is_some_and(|t| t == "personal" || t == "shared")
}

fn commands_of(tool: &str, args: &Value) -> Vec<Value> {
    match tool {
        "execute" => vec![args.clone()],
        "batch" | "dry_run" => args
            .get("commands")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => vec![],
    }
}

/// True when the call edits the design (the turn's change count and undo step).
pub fn edits_design(tool: &str, _args: &Value) -> bool {
    match tool {
        "open" | "new_design" | "sample_design" | "undo" | "redo" => true,
        t if READS.contains(&t) || is_file_tool(t) || t == "dry_run" => false,
        _ => true,
    }
}

/// Decide what happens to `tool` called with `args`.
///
/// `undo_left` is how many of this message's own changes `undo` could still take back; asking
/// for more undoes work from before the message. `always` holds keys the user chose to always
/// allow (see [`tool_key`]).
pub fn request(
    tool: &str,
    args: &Value,
    mode: Mode,
    undo_left: usize,
    always: &[String],
) -> Verdict {
    if let Err(reason) = pcbcraft_mcp::check_args(tool, args) {
        return Verdict::Refuse(reason);
    }
    if tool == "batch" && commands_of(tool, args).len() > BATCH_LIMIT {
        return Verdict::Refuse(format!("a batch holds at most {BATCH_LIMIT} commands"));
    }
    if READS.contains(&tool) {
        return Verdict::Run { change: false };
    }
    if tool == "dry_run"
        || (tool == "execute"
            && args.get("command").and_then(Value::as_str) == Some("design.summary"))
    {
        return Verdict::Run { change: false };
    }
    if tool == "undo" || tool == "redo" {
        return if undo_left == 0 && mode != Mode::All {
            Verdict::Ask("undoes work done before this message".into())
        } else {
            Verdict::Run { change: true }
        };
    }
    let files = is_file_tool(tool)
        || commands_of(tool, args).iter().any(|c| {
            let name = c.get("command").and_then(Value::as_str).unwrap_or("");
            is_file_tool(name)
                || c.get("params").is_some_and(|p| {
                    params_touch_files(p) || command_writes_folder_library(name, p)
                })
        })
        || (!matches!(tool, "execute" | "batch") && params_touch_files(args));
    if files {
        return if mode == Mode::All {
            Verdict::Run {
                change: edits_design(tool, args),
            }
        } else {
            Verdict::Ask("works with files or replaces the design".into())
        };
    }
    if mode != Mode::Ask || always.contains(&tool_key(tool, args)) {
        Verdict::Run { change: true }
    } else {
        Verdict::Ask("changes the design".into())
    }
}

/// The name "Always allow" remembers: the command for `execute`, `batch` for a batch.
pub fn tool_key(tool: &str, args: &Value) -> String {
    if tool == "execute"
        && let Some(command) = args.get("command").and_then(Value::as_str)
    {
        return command.to_string();
    }
    tool.to_string()
}

/// A plain title for the approval card.
pub fn describe(tool: &str, args: &Value) -> String {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    match tool {
        "batch" => format!("Batch of {} commands", commands_of(tool, args).len()),
        "execute" => describe_command(
            &text(args, "command").unwrap_or_default(),
            args.get("params").unwrap_or(&Value::Null),
        ),
        "undo" => "Undo the last change".into(),
        "redo" => "Redo the last change".into(),
        "new_design" => "Start a new empty design".into(),
        "sample_design" => "Load the example design".into(),
        "open" | "save" => {
            let path = text(args, "path").unwrap_or_default();
            let name = path.rsplit(['/', '\\']).next().unwrap_or("");
            let verb = if tool == "open" {
                "Open"
            } else {
                "Save the design to"
            };
            format!("{verb} {name}")
        }
        other => describe_command(other, args),
    }
}

fn describe_command(command: &str, params: &Value) -> String {
    let get = |k: &str| params.get(k).and_then(Value::as_str);
    if command == "component.add" {
        let kind = get("kind").unwrap_or("part");
        return match (get("reference"), get("value")) {
            (Some(r), Some(v)) => format!("Add {kind} {r} ({v})"),
            (Some(r), None) => format!("Add {kind} {r}"),
            _ => format!("Add a {kind}"),
        };
    }
    command.replace(['.', '_'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn add(reference: &str) -> Value {
        json!({"command":"component.add","params":{"kind":"resistor","reference":reference,"value":"10k","position":{"x":1.0,"y":2.0}}})
    }

    #[test]
    fn the_gate_refuses_and_limits() {
        assert!(matches!(
            request("no_such_tool", &json!({}), Mode::All, 0, &[]),
            Verdict::Refuse(_)
        ));
        let many = json!({"commands": vec![add("R1"); 129]});
        assert!(matches!(
            request("batch", &many, Mode::All, 0, &[]),
            Verdict::Refuse(_)
        ));
    }

    #[test]
    fn file_actions_wait_for_the_user() {
        for mode in [Mode::Ask, Mode::Edits] {
            assert!(matches!(
                request("save", &json!({"path":"/tmp/a.pcbcraft"}), mode, 0, &[]),
                Verdict::Ask(_)
            ));
            assert!(matches!(
                request("gerber.export", &json!({"path":"/tmp/out"}), mode, 0, &[]),
                Verdict::Ask(_)
            ));
        }
        assert_eq!(
            request(
                "save",
                &json!({"path":"/tmp/a.pcbcraft"}),
                Mode::All,
                0,
                &[]
            ),
            Verdict::Run { change: false }
        );
    }

    #[test]
    fn permission_modes_decide_what_asks() {
        let a = add("R1");
        assert!(matches!(
            request("execute", &a, Mode::Ask, 0, &[]),
            Verdict::Ask(_)
        ));
        assert_eq!(
            request("execute", &a, Mode::Edits, 0, &[]),
            Verdict::Run { change: true }
        );
        assert_eq!(
            request("execute", &a, Mode::Ask, 0, &["component.add".into()]),
            Verdict::Run { change: true }
        );
        assert_eq!(
            request("inspect_design", &json!({}), Mode::Ask, 0, &[]),
            Verdict::Run { change: false }
        );
        assert!(matches!(
            request("undo", &json!({}), Mode::Edits, 0, &[]),
            Verdict::Ask(_)
        ));
        assert_eq!(
            request("undo", &json!({}), Mode::Edits, 2, &[]),
            Verdict::Run { change: true }
        );
    }

    #[test]
    fn every_tool_in_the_list_gets_a_verdict_in_every_mode() {
        let list = pcbcraft_mcp::tool_list();
        let names: Vec<String> = list["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["name"].as_str().map(str::to_string))
            .collect();
        assert!(names.len() > 100);
        for name in &names {
            for mode in [Mode::Ask, Mode::Edits, Mode::All] {
                // Empty arguments may be refused by the schema; the call must still not panic
                // and a file tool must never run silently outside mode All.
                let v = request(name, &json!({}), mode, 0, &[]);
                if is_file_tool(name) && mode != Mode::All {
                    assert!(
                        !matches!(v, Verdict::Run { .. }),
                        "{name} ran without asking"
                    );
                }
            }
        }
        for read in READS.iter().filter(|r| names.iter().any(|n| n == *r)) {
            assert!(!is_file_tool(read), "{read} is both a read and a file tool");
        }
    }

    #[test]
    fn a_malformed_execute_is_refused_with_the_engines_reason_and_runs_nothing() {
        let bad =
            json!({"command":"component.add","params":{"kind":"resistor","position":"nowhere"}});
        match request("execute", &bad, Mode::All, 0, &[]) {
            Verdict::Refuse(reason) => assert!(!reason.is_empty()),
            other => assert_eq!(other, Verdict::Refuse(String::new()), "must refuse"),
        }
        let unknown = json!({"command":"no.such","params":{}});
        assert!(
            matches!(request("execute", &unknown, Mode::All, 0, &[]), Verdict::Refuse(r) if r.contains("unknown"))
        );
    }

    #[test]
    fn the_card_title_names_the_part() {
        let a = add("R1");
        assert_eq!(describe("execute", &a), "Add resistor R1 (10k)");
        assert_eq!(tool_key("execute", &a), "component.add");
        assert_eq!(
            describe("batch", &json!({"commands":[{},{}]})),
            "Batch of 2 commands"
        );
    }
}
