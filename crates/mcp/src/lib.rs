//! MCP stdio server plus a loopback bridge into the desktop's actual engine session.
mod document_hub;
#[cfg(not(target_arch = "wasm32"))]
pub mod http;
mod prompts;
mod resources;
use base64::Engine as _;
pub use document_hub::DocumentHub;
use pcbcraft_engine::{Command, Session};
use serde_json::{Value, json};
#[cfg(any(not(target_arch = "wasm32"), test))]
use std::io::BufReader;
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub type SharedSession = Arc<Mutex<Session>>;
pub const MAX_MESSAGE: u64 = 1024 * 1024;
/// Protocol versions `initialize` accepts, newest first; the HTTP transport checks its header against it.
pub(crate) const SUPPORTED_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2024-11-05"];
pub fn shared(session: Session) -> SharedSession {
    Arc::new(Mutex::new(session))
}
pub fn tool_list() -> Value {
    let object = |properties: Value, required: &[&str]| json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
    let command = json!({"type":"object","properties":{"command":{"type":"string"},"params":{"type":"object"}},"required":["command","params"],"additionalProperties":false});
    let mut tools = vec![
        json!({"name":"library.save","description":"Atomically save the editable project library to a .pcbcraft-lib collection","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"gerber.export","description":"Write fabrication outputs (Gerber RS-274X front and back copper, board outline, Excellon drill map) into a directory without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"netlist.write","description":"Write a plain-text netlist, one line per net with its component pins, into a directory without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"library.export_footprint","description":"Write one project-library footprint to a .kicad_mod footprint library text file without editing the project","inputSchema":object(json!({"path":{"type":"string"},"name":{"type":"string"}}),&["path","name"])}),
        json!({"name":"library.export_symbol","description":"Write one project-library symbol to a .kicad_sym symbol library text file without editing the project","inputSchema":object(json!({"path":{"type":"string"},"name":{"type":"string"}}),&["path","name"])}),
        json!({"name":"library.export_footprint_svg","description":"Write one project-library footprint to an SVG vector preview file without editing the project","inputSchema":object(json!({"path":{"type":"string"},"name":{"type":"string"}}),&["path","name"])}),
        json!({"name":"library.export_symbol_svg","description":"Write one project-library symbol to an SVG vector preview file without editing the project","inputSchema":object(json!({"path":{"type":"string"},"name":{"type":"string"}}),&["path","name"])}),
        json!({"name":"ipc356.write","description":"Write an IPC-D-356A style in-circuit test netlist (net, component, pin and coordinates in fixed columns) into a directory without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"ps.write","description":"Write a one-page PostScript drawing of either view with copper, pads, zones and text as real stroke and fill operators, without editing the project","inputSchema":object(json!({"path":{"type":"string"},"view":{"type":"string","enum":["pcb","schematic","assembly-top","assembly-bottom","fabrication"]}}),&["path"])}),
        json!({"name":"stats.write","description":"Write a CSV board statistics sheet counting parts, nets, pins, routing, vias and zones, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"drill_chart.write","description":"Write a CSV drill tool chart listing every distinct drill size with its plating and hole count, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"stackup.write","description":"Write a plain-text board stackup sheet with recorded layer notes, substrate data, finish and machining tolerances, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"hpgl.write","description":"Write an HP-GL pen plotter drawing of either view as pen-up and pen-down moves with text label boxes, without editing the project","inputSchema":object(json!({"path":{"type":"string"},"view":{"type":"string","enum":["pcb","schematic","assembly-top","assembly-bottom","fabrication"]}}),&["path"])}),
        json!({"name":"odb.write","description":"Write an ODB++ style board package ZIP holding the layer matrix, polygon pad symbols, per-layer copper features and placed components, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"pdf.write","description":"Write a one-page vector PDF drawing of the board or schematic (outline, copper, pads, zones, wires, labels and text as real PDF paths) into a directory without editing the project","inputSchema":object(json!({"path":{"type":"string"},"view":{"type":"string","enum":["pcb","schematic","assembly-top","assembly-bottom","fabrication"],"default":"pcb"}}),&["path"])}),
        json!({"name":"step.write","description":"Write an AP214 STEP file of the board (outline and every pad as solid prisms in millimetres) into a directory without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"dsn.write","description":"Write a Specctra-style board file (nets, classes, placements, tracks, vias and keepouts as s-expressions) into a directory for external autorouters, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"cmp.write","description":"Write a legacy board trace file listing every placed pad position in mils with a front or back layer code and every net's pad membership, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"report.write","description":"Write a plain-text design report summarizing the board, every component with its package and placement, and every net with its pins, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"ipc2581.write","description":"Write a single-file IPC-2581 style assembly exchange XML with the board outline, nets and their pins, placed components, drill holes and copper by layer, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"idf.write","description":"Write a board-level mechanical drawing (board size plus every placed part's position, side and rotation, with default component heights) into a directory for enclosure tools, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"wrl.write","description":"Write a VRML 2.0 scene extruding the board outline and every placed pad into solid prisms into a directory for mechanical viewers, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"hyperlynx.write","description":"Write a simulation netlist CSV, one line per net naming the net and every reference.pin on it, into a directory for circuit simulators, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"gencad.write","description":"Write a legacy board interchange text listing components, pins in whole mils with shape and layer, and nets by component and pad index, into a directory for EDA bridges, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"bom.write","description":"Write a bill-of-materials CSV, identical parts collapsed into one row with a sorted reference list, into a directory without editing the project; with no fields given, the columns the project itself defines are printed, and a variant name prints that build with its unpopulated parts skipped and footprint swaps applied","inputSchema":object(json!({"path":{"type":"string"},"fields":{"type":"array","items":{"type":"string"},"maxItems":16,"description":"extra component property columns"},"variant":{"type":"string","minLength":1,"maxLength":64,"description":"assembly variant to print the bill for"}}),&["path"])}),
        json!({"name":"dxf.export","description":"Write the board drawing as an ASCII DXF file (copper tracks, vias, pads, zones, board graphics, outline and labels) into a directory without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"placement.write","description":"Write a pick-and-place centroid file (reference, position, rotation, side, package per placed part) into a directory without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"library.export_footprints","description":"Write every project-library footprint to a directory as one library text file each, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"library.export_symbols","description":"Write every project-library symbol to a directory as one library symbol text file each, without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"plot_csv.write","description":"Write a fabrication plot report CSV (per copper layer: element counts and bounding box in millimetres, plus a totals row) into a directory without editing the project","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"library.catalog","description":"Discover installed symbol and footprint libraries on the host","inputSchema":object(json!({}),&[])}),
        json!({"name":"library.search","description":"Search a symbol file or footprint directory without altering the project","inputSchema":object(json!({"path":{"type":"string"},"kind":{"type":"string","enum":["symbol","footprint"]},"query":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100}}),&["path","kind"])}),
        json!({"name":"library.inspect_symbol","description":"Read resolved symbol pins, graphics, units; include_source preserves up to 512KiB of original nodes for editing","inputSchema":object(json!({"path":{"type":"string"},"name":{"type":"string"},"include_source":{"type":"boolean"}}),&["path","name"])}),
        json!({"name":"library.inspect_footprint","description":"Read footprint graphics/pads/drills; include_source preserves up to 512KiB of original nodes","inputSchema":object(json!({"path":{"type":"string"},"include_source":{"type":"boolean"}}),&["path"])}),
        json!({"name":"design.recents","description":"List the design files this computer opened or saved most recently, newest first, for one-click reopening","inputSchema":object(json!({}),&[])}),
        json!({"name":"design.list_open","description":"List every design session open in the desktop app, with path, name, revision and dirty state","inputSchema":object(json!({}),&[])}),
        json!({"name":"design.activate","description":"Switch the active design session by zero-based index or by filesystem path","inputSchema":object(json!({"index":{"type":"integer","minimum":0},"path":{"type":"string"}}),&[])}),
        json!({"name":"library.sources","description":"List every library location the unified browser reads: folders the user added and installed locations, each with collection counts","inputSchema":object(json!({}),&[])}),
        json!({"name":"library.add_folder","description":"Register any folder on this computer as a library source so its symbol and footprint collections join the unified browser; returns discovered counts","inputSchema":object(json!({"path":{"type":"string"},"name":{"type":"string","description":"optional friendly name, defaults to the folder name"}}),&["path"])}),
        json!({"name":"library.remove_folder","description":"Forget an added library folder; installed locations are never removed","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"library.compare","description":"Compare two symbols or two footprints and report pin/pad differences without altering the project","inputSchema":object(json!({"path_a":{"type":"string"},"path_b":{"type":"string"},"kind":{"type":"string","enum":["symbol","footprint"]},"name_a":{"type":"string"},"name_b":{"type":"string"}}),&["path_a","path_b","kind"])}),
        json!({"name":"list_commands","description":"Discover every engine command, input schema and unit convention","inputSchema":object(json!({}),&[])}),
        json!({"name":"inspect_design","description":"Inspect the canonical project, stable IDs, revision, history and truthful capability limits","inputSchema":object(json!({}),&[])}),
        json!({"name":"board_mesh","description":"Read the board as a deterministic 3D triangle mesh in millimetres (z up, bottom face at 0): slab with outline, cutouts and holes, thin copper, mask and silk layers, and parametric default component bodies with the placement matrix of any referenced model; model files are not loaded. Does not edit the project","inputSchema":object(json!({"layers":{"type":"array","items":{"type":"string","enum":["board","copper","mask","silk","bodies"]},"maxItems":5,"description":"parts to build, default all"},"geometry":{"type":"boolean","description":"false returns counts and bounds only, default true"}}),&[])}),
        json!({"name":"checks","description":"Run initial ERC/DRC and inspect unrouted airwires; returns scope and affected object IDs. copper_only narrows the findings to the copper subset (clearance, width, stub length, unrouted, differential-pair checks) for a quick mid-layout pass","inputSchema":object(json!({"copper_only":{"type":"boolean","description":"report only copper-related findings, default false"}}),&[])}),
        json!({"name":"execute","description":"Execute one engine command atomically, optionally rejecting a stale revision","inputSchema":object(json!({"command":{"type":"string"},"params":{"type":"object"},"expected_revision":{"type":"integer","minimum":0}}),&["command","params"])}),
        json!({"name":"batch","description":"Execute 1–128 commands as one transaction and undo step; any failure rolls back everything","inputSchema":object(json!({"commands":{"type":"array","items":command,"minItems":1,"maxItems":128},"expected_revision":{"type":"integer","minimum":0}}),&["commands"])}),
        json!({"name":"dry_run","description":"Preview 1–128 commands on a copy: returns each result and a summary of what would be added, removed, moved or changed. Nothing is applied and the revision does not change","inputSchema":object(json!({"commands":{"type":"array","items":command,"minItems":1,"maxItems":128},"expected_revision":{"type":"integer","minimum":0}}),&["commands"])}),
        json!({"name":"undo","description":"Undo one transaction","inputSchema":object(json!({}),&[])}),
        json!({"name":"redo","description":"Redo one transaction","inputSchema":object(json!({}),&[])}),
        json!({"name":"new_design","description":"Replace the project with a new empty one; replacement is undoable","inputSchema":object(json!({}),&[])}),
        json!({"name":"sample_design","description":"Load the original LED indicator example; replacement is undoable","inputSchema":object(json!({}),&[])}),
        json!({"name":"open","description":"Open and validate a .pcbcraft project from the server host; replacement is undoable","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"save","description":"Atomically save the current project on the server host","inputSchema":object(json!({"path":{"type":"string"}}),&["path"])}),
        json!({"name":"export","description":"Export the supported PCB subset to a board interchange text file, or export a schematic/PCB preview as an SVG vector file or a PNG raster file","inputSchema":object(json!({"path":{"type":"string"},"format":{"type":"string","enum":["kicad_pcb","svg","png"]},"view":{"type":"string","enum":["schematic","pcb"]}}),&["path","format"])}),
        json!({"name":"screenshot","description":"Return a PNG design preview from current session state (not a desktop UI screenshot)","inputSchema":object(json!({"view":{"type":"string","enum":["schematic","pcb"]}}),&["view"])}),
    ];
    for spec in pcbcraft_engine::commands() {
        tools.push(
            json!({"name":spec.id,"description":spec.description,"inputSchema":spec.input_schema}),
        );
    }
    for tool in &mut tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        tool["title"] = json!(title_for(&name));
        tool["annotations"] = annotations_for(&name);
    }
    json!({"tools":tools})
}

/// Which family a tool belongs to, for its client-facing hints.
#[derive(PartialEq)]
enum ToolKind {
    ReadOnly,
    WritesFiles,
    DiscardsState,
    EditsDesign,
}

fn tool_kind(name: &str) -> ToolKind {
    const READ_ONLY: [&str; 14] = [
        "inspect_design",
        "list_commands",
        "checks",
        "board_mesh",
        "screenshot",
        "library.catalog",
        "library.search",
        "library.inspect_symbol",
        "library.inspect_footprint",
        "library.compare",
        "library.sources",
        "design.recents",
        "design.list_open",
        "design.activate",
    ];
    if READ_ONLY.contains(&name) {
        ToolKind::ReadOnly
    } else if ["new_design", "sample_design", "open"].contains(&name) {
        ToolKind::DiscardsState
    } else if name.ends_with(".write")
        || name.ends_with(".export")
        || name.starts_with("export_")
        || name.starts_with("library.export_")
        || ["export", "save", "library.save", "output_job.package"].contains(&name)
    {
        ToolKind::WritesFiles
    } else {
        ToolKind::EditsDesign
    }
}

/// The four MCP behaviour hints for one tool, so a client can decide when to ask first.
///
/// Read-only tools never change the design. File writers change nothing in the
/// project but replace files on the host. Tools that replace the whole design are
/// marked destructive. Every other edit is undoable, so it is not destructive.
///
/// # Rules
/// The table lives here only; `docs/mcp.md` section "Tool hints" describes it.
///
/// # Dependencies
/// `crates/mcp/src/lib.rs` (`tool_list` applies it to every listed tool).
///
/// # Do not
/// Do not add a name to the read-only list unless the tool leaves the revision unchanged.
fn annotations_for(name: &str) -> Value {
    let (read_only, destructive, idempotent) = match tool_kind(name) {
        ToolKind::ReadOnly => (true, false, true),
        ToolKind::WritesFiles => (false, false, true),
        ToolKind::DiscardsState => (false, true, false),
        ToolKind::EditsDesign => (false, false, false),
    };
    json!({"readOnlyHint":read_only,"destructiveHint":destructive,"idempotentHint":idempotent,"openWorldHint":false})
}

/// A short display name: dots and underscores become spaces, first letter capitalized.
///
/// # Do not
/// Do not put a rule here; a title is display text only.
fn title_for(name: &str) -> String {
    let spaced = name.replace(['.', '_'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
pub(crate) fn text_result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
}
pub(crate) fn error_result(error: String) -> Value {
    json!({"content":[{"type":"text","text":error}],"isError":true})
}
fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string parameter: {key}"))
}
fn strings(args: &Value, key: &str) -> Result<Vec<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("{key} entries must be strings"))
            })
            .collect(),
        _ => Err(format!("{key} must be an array of strings")),
    }
}
fn revision(args: &Value) -> Result<Option<u64>, String> {
    args.get("expected_revision")
        .map(|v| {
            v.as_u64()
                .ok_or_else(|| "expected_revision must be a nonnegative integer".into())
        })
        .transpose()
}

pub fn call_tool(session: &SharedSession, name: &str, args: &Value) -> Value {
    match invoke_session(session, name, args) {
        Ok(v) => v,
        Err(e) => error_result(e),
    }
}
/// Check a tool call's arguments against the tool's schema, and for `execute` and `batch` each
/// command's parameters against that command's schema, without running anything. The assistant
/// gate uses it to refuse a malformed call with the engine's own reason.
pub fn check_args(name: &str, args: &Value) -> Result<(), String> {
    if !args.is_object() {
        return Err("arguments must be an object".into());
    }
    validate_tool_schema(name, args)?;
    let one = |command: &Value| -> Result<(), String> {
        let id = command.get("command").and_then(Value::as_str).unwrap_or("");
        let spec = pcbcraft_engine::commands()
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| format!("unknown command: {id}"))?;
        let params = command.get("params").cloned().unwrap_or(json!({}));
        validate_schema(&params, &spec.input_schema, &format!("{id} params"))
    };
    match name {
        "execute" => one(args)?,
        "batch" | "dry_run" => {
            for command in args
                .get("commands")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                one(command)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn validate_tool_schema(name: &str, args: &Value) -> Result<(), String> {
    let tools = tool_list();
    let schema = tools
        .get("tools")
        .and_then(Value::as_array)
        .and_then(|list| {
            list.iter()
                .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
        })
        .and_then(|tool| tool.get("inputSchema"))
        .ok_or("unknown tool")?;
    validate_schema(args, schema, "arguments")
}
pub(crate) fn invoke_session(
    session: &SharedSession,
    name: &str,
    args: &Value,
) -> Result<Value, String> {
    if !args.is_object() {
        return Err("arguments must be an object".into());
    }
    validate_tool_schema(name, args)?;
    if matches!(name, "design.list_open" | "design.activate") {
        return Err("multi-document tools require the desktop control bridge".into());
    }
    if [
        "design.recents",
        "library.catalog",
        "library.search",
        "library.inspect_symbol",
        "library.inspect_footprint",
        "library.compare",
        "library.sources",
        "library.add_folder",
        "library.remove_folder",
    ]
    .contains(&name)
    {
        return pcbcraft_engine::library::read_tool(name, args).map(text_result);
    }
    let mut s = session
        .lock()
        .map_err(|_| "session unavailable after internal failure".to_string())?;
    let result = match name {
        "library.save" => {
            s.project.library.validate()?;
            let path = string(args, "path")?;
            let bytes = serde_json::to_vec_pretty(
                &json!({"format":"pcbcraft-library","version":1,"library":s.project.library}),
            )
            .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > pcbcraft_io::MAX_FILE_BYTES {
                return Err("library collection exceeds 8 MiB".into());
            }
            pcbcraft_io::atomic_write(Path::new(path), &bytes)?;
            json!({"path":path})
        }
        "library.export_footprint"
        | "library.export_symbol"
        | "library.export_footprint_svg"
        | "library.export_symbol_svg" => {
            s.project.library.validate()?;
            let path = string(args, "path")?;
            let definition = string(args, "name")?;
            let footprint =
                name == "library.export_footprint" || name == "library.export_footprint_svg";
            let suffix = match name {
                "library.export_footprint" => "kicad_mod",
                "library.export_symbol" => "kicad_sym",
                _ => "svg",
            };
            if Path::new(path).extension().and_then(|e| e.to_str()) != Some(suffix) {
                return Err(format!("path must end in .{suffix}"));
            }
            let text = if footprint {
                let item = s
                    .project
                    .library
                    .footprints
                    .iter()
                    .find(|f| f.name == definition)
                    .ok_or("unknown project-library footprint")?;
                if suffix == "svg" {
                    pcbcraft_io::footprint_svg(item)?
                } else {
                    pcbcraft_io::footprint_library_text(item)?
                }
            } else {
                let item = s
                    .project
                    .library
                    .symbols
                    .iter()
                    .find(|f| f.name == definition)
                    .ok_or("unknown project-library symbol")?;
                if suffix == "svg" {
                    pcbcraft_io::symbol_svg(item)?
                } else {
                    pcbcraft_io::symbol_library_text(item)?
                }
            };
            pcbcraft_io::atomic_write(Path::new(path), text.as_bytes())?;
            json!({"path":path,"name":definition,"bytes":text.len()})
        }
        "library.export_footprints" => {
            let directory = string(args, "path")?;
            let mut files = Vec::new();
            for item in &s.project.library.footprints {
                let text = pcbcraft_io::footprint_library_text(item)?;
                let name = format!("{}.kicad_mod", item.name);
                pcbcraft_io::atomic_write(&Path::new(&directory).join(&name), text.as_bytes())?;
                files.push(name);
            }
            json!({"directory":directory,"files":files})
        }
        "library.export_symbols" => {
            let directory = string(args, "path")?;
            let mut files = Vec::new();
            for item in &s.project.library.symbols {
                let text = pcbcraft_io::symbol_library_text(item)?;
                let name = format!("{}.kicad_sym", item.name);
                pcbcraft_io::atomic_write(&Path::new(&directory).join(&name), text.as_bytes())?;
                files.push(name);
            }
            json!({"directory":directory,"files":files})
        }
        "gerber.export" => {
            let directory = string(args, "path")?;
            let files = pcbcraft_io::gerber_files(&s.project)?;
            let mut written = Vec::new();
            for (name, text) in &files {
                pcbcraft_io::atomic_write(&Path::new(directory).join(name), text.as_bytes())?;
                written.push(name.clone());
            }
            json!({"directory":directory,"files":written})
        }
        "dsn.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::specctra_dsn(&s.project)?;
            pcbcraft_io::atomic_write(&Path::new(directory).join("pcb.dsn"), text.as_bytes())?;
            json!({"directory":directory,"file":"pcb.dsn"})
        }
        "cmp.write" => {
            let path = string(args, "path")?;
            if Path::new(&path).extension().and_then(|e| e.to_str()) != Some("cmp") {
                return Err("path must end in .cmp".into());
            }
            let text = pcbcraft_io::cmp_text(&s.project)?;
            pcbcraft_io::atomic_write(Path::new(&path), text.as_bytes())?;
            json!({"path":path,"bytes":text.len()})
        }
        "report.write" => {
            let path = string(args, "path")?;
            if Path::new(&path).extension().and_then(|e| e.to_str()) != Some("txt") {
                return Err("path must end in .txt".into());
            }
            let text = pcbcraft_io::report_text(&s.project)?;
            pcbcraft_io::atomic_write(Path::new(&path), text.as_bytes())?;
            json!({"path":path,"bytes":text.len()})
        }
        "ipc2581.write" => {
            let path = string(args, "path")?;
            if Path::new(&path).extension().and_then(|e| e.to_str()) != Some("xml") {
                return Err("path must end in .xml".into());
            }
            let text = pcbcraft_io::ipc2581_text(&s.project)?;
            pcbcraft_io::atomic_write(Path::new(&path), text.as_bytes())?;
            json!({"path":path,"bytes":text.len()})
        }
        "idf.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::idf_board(&s.project)?;
            pcbcraft_io::atomic_write(&Path::new(directory).join("board.idf"), text.as_bytes())?;
            json!({"directory":directory,"file":"board.idf","bytes":text.len()})
        }
        "step.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::step_board(&s.project)?;
            pcbcraft_io::atomic_write(&Path::new(directory).join("board.step"), text.as_bytes())?;
            json!({"directory":directory,"file":"board.step","bytes":text.len()})
        }
        "wrl.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::vrml_board(&s.project)?;
            pcbcraft_io::atomic_write(&Path::new(directory).join("board.wrl"), text.as_bytes())?;
            json!({"directory":directory,"file":"board.wrl","bytes":text.len()})
        }
        "hyperlynx.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::hyperlynx_csv(&s.project)?;
            pcbcraft_io::atomic_write(&Path::new(directory).join("netlist.csv"), text.as_bytes())?;
            json!({"directory":directory,"file":"netlist.csv","bytes":text.len()})
        }
        "gencad.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::gencad_text(&s.project)?;
            pcbcraft_io::atomic_write(&Path::new(directory).join("board.gencad"), text.as_bytes())?;
            json!({"directory":directory,"file":"board.gencad","bytes":text.len()})
        }
        "ps.write" => {
            let directory = string(args, "path")?;
            let view = args.get("view").and_then(Value::as_str).unwrap_or("pcb");
            let text = pcbcraft_io::postscript(&s.project, view)?;
            pcbcraft_io::atomic_write(
                &Path::new(directory).join(format!("pcb-{view}.ps")),
                text.as_bytes(),
            )?;
            json!({"directory":directory,"file":format!("pcb-{view}.ps"),"bytes":text.len()})
        }
        "stats.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::stats_text(&s.project)?;
            pcbcraft_io::atomic_write(
                &Path::new(directory).join("board-stats.csv"),
                text.as_bytes(),
            )?;
            json!({"directory":directory,"file":"board-stats.csv","bytes":text.len()})
        }
        "drill_chart.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::drill_chart_csv(&s.project)?;
            pcbcraft_io::atomic_write(
                &Path::new(directory).join("board-drill-tools.csv"),
                text.as_bytes(),
            )?;
            json!({"directory":directory,"file":"board-drill-tools.csv","bytes":text.len()})
        }
        "stackup.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::stackup_text(&s.project)?;
            pcbcraft_io::atomic_write(
                &Path::new(directory).join("board-stackup.txt"),
                text.as_bytes(),
            )?;
            json!({"directory":directory,"file":"board-stackup.txt","bytes":text.len()})
        }
        "hpgl.write" => {
            let directory = string(args, "path")?;
            let view = args.get("view").and_then(Value::as_str).unwrap_or("pcb");
            let text = pcbcraft_io::hpgl_plot(&s.project, view)?;
            pcbcraft_io::atomic_write(
                &Path::new(directory).join(format!("pcb-{view}.hpgl")),
                text.as_bytes(),
            )?;
            json!({"directory":directory,"file":format!("pcb-{view}.hpgl"),"bytes":text.len()})
        }
        "odb.write" => {
            let directory = string(args, "path")?;
            let bytes = pcbcraft_io::odb_package(&s.project)?;
            pcbcraft_io::atomic_write(&Path::new(directory).join("board.odb.zip"), &bytes)?;
            json!({"directory":directory,"file":"board.odb.zip","bytes":bytes.len()})
        }
        "pdf.write" => {
            let directory = string(args, "path")?;
            let view = args.get("view").and_then(Value::as_str).unwrap_or("pcb");
            let bytes = pcbcraft_io::pdf_document(&s.project, view)?;
            pcbcraft_io::atomic_write(
                &Path::new(directory).join(format!("pcb-{view}.pdf")),
                &bytes,
            )?;
            json!({"directory":directory,"file":format!("pcb-{view}.pdf"),"bytes":bytes.len()})
        }
        "ipc356.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::ipc356_netlist(&s.project)?;
            pcbcraft_io::atomic_write(&Path::new(directory).join("test.brd"), text.as_bytes())?;
            json!({"directory":directory,"file":"test.brd"})
        }
        "netlist.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::netlist_text(&s.project)?;
            pcbcraft_io::atomic_write(
                &Path::new(directory).join("pcb-netlist.net"),
                text.as_bytes(),
            )?;
            json!({"directory":directory,"files":["pcb-netlist.net"]})
        }
        "bom.write" => {
            let directory = string(args, "path")?;
            let fields = strings(args, "fields")?;
            let variant = args
                .get("variant")
                .map(|v| v.as_str().ok_or("variant must be a string"))
                .transpose()?;
            let text = pcbcraft_io::bom_csv(&s.project, &fields, variant)?;
            pcbcraft_io::atomic_write(&Path::new(directory).join("bom.csv"), text.as_bytes())?;
            json!({"directory":directory,"files":["bom.csv"]})
        }
        "dxf.export" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::dxf_board(&s.project)?;
            pcbcraft_io::atomic_write(&Path::new(directory).join("pcb.dxf"), text.as_bytes())?;
            json!({"directory":directory,"files":["pcb.dxf"]})
        }
        "placement.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::placement_text(&s.project)?;
            pcbcraft_io::atomic_write(
                &Path::new(directory).join("placement.txt"),
                text.as_bytes(),
            )?;
            json!({"directory":directory,"files":["placement.txt"]})
        }
        "plot_csv.write" => {
            let directory = string(args, "path")?;
            let text = pcbcraft_io::plot_csv(&s.project)?;
            pcbcraft_io::atomic_write(
                &Path::new(directory).join("plot-report.csv"),
                text.as_bytes(),
            )?;
            json!({"directory":directory,"file":"plot-report.csv"})
        }
        "list_commands" => {
            json!({"commands":pcbcraft_engine::commands(),"units":"millimetres and degrees"})
        }
        "inspect_design" => s.inspect(),
        "board_mesh" => {
            let layers: Option<Vec<String>> = match args.get("layers") {
                Some(v) => Some(serde_json::from_value(v.clone()).map_err(|e| e.to_string())?),
                None => None,
            };
            let geometry = args
                .get("geometry")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            s.board_mesh(layers.as_deref(), geometry)
                .map_err(|e| e.to_string())?
        }
        "checks" => {
            let report = s.checks();
            if args
                .get("copper_only")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                const COPPER: [&str; 7] = [
                    "clearance",
                    "copper_short",
                    "track_width",
                    "stub_length",
                    "unrouted",
                    "diffpair_gap",
                    "diffpair_skew",
                ];
                let keep = |key: &str| {
                    report[key]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|f| f["code"].as_str().is_some_and(|c| COPPER.contains(&c)))
                        .collect::<Vec<_>>()
                };
                return Ok(text_result(json!({
                    "erc": keep("erc"),
                    "drc": keep("drc"),
                    "airwires": report["airwires"],
                    "scope": "Quick copper-only pass: clearance, track width, stub length, unrouted copper and differential-pair checks; ERC findings are omitted. See checks.scope for the full rule set.",
                })));
            }
            report
        }
        "execute" => s
            .execute(
                string(args, "command")?,
                args.get("params").cloned().ok_or("missing params")?,
                revision(args)?,
            )
            .map_err(|e| e.to_string())?,
        "batch" => {
            let commands: Vec<Command> =
                serde_json::from_value(args.get("commands").cloned().ok_or("missing commands")?)
                    .map_err(|e| e.to_string())?;
            s.batch(&commands, revision(args)?)
                .map_err(|e| e.to_string())?
        }
        "dry_run" => {
            let commands: Vec<Command> =
                serde_json::from_value(args.get("commands").cloned().ok_or("missing commands")?)
                    .map_err(|e| e.to_string())?;
            let preview = s
                .dry_run(&commands, revision(args)?)
                .map_err(|e| e.to_string())?;
            serde_json::to_value(preview).map_err(|e| e.to_string())?
        }
        "undo" => s.undo().map_err(|e| e.to_string())?,
        "redo" => s.redo().map_err(|e| e.to_string())?,
        "new_design" => s.replace(Default::default()).map_err(|e| e.to_string())?,
        "sample_design" => s
            .replace(Session::sample().map_err(|e| e.to_string())?.project)
            .map_err(|e| e.to_string())?,
        "open" => {
            let p = pcbcraft_io::load(Path::new(string(args, "path")?))?;
            s.replace(p).map_err(|e| e.to_string())?
        }
        "save" => {
            let path = string(args, "path")?;
            pcbcraft_io::save(&s.project, Path::new(path))?;
            json!({"path":path,"revision":s.revision})
        }
        "export" => {
            let path = string(args, "path")?;
            let format = string(args, "format")?;
            if format == "png" {
                let bytes = pcbcraft_io::preview_png(
                    &s.project,
                    args.get("view").and_then(Value::as_str).unwrap_or("pcb"),
                )?;
                pcbcraft_io::atomic_write(Path::new(path), &bytes)?;
                return Ok(text_result(json!({"path":path,"revision":s.revision})));
            }
            let text = match format {
                "kicad_pcb" => pcbcraft_io::kicad_board(&s.project)?,
                "gerber_f_cu" => pcbcraft_io::gerber_copper(&s.project, pcbcraft_io::Layer::Front)?,
                "gerber_b_cu" => pcbcraft_io::gerber_copper(&s.project, pcbcraft_io::Layer::Back)?,
                "svg" => pcbcraft_io::svg(
                    &s.project,
                    args.get("view").and_then(Value::as_str).unwrap_or("pcb"),
                )?,
                _ => return Err("unsupported export format".into()),
            };
            pcbcraft_io::atomic_write(Path::new(path), text.as_bytes())?;
            json!({"path":path,"revision":s.revision})
        }
        "screenshot" => {
            let view = string(args, "view")?;
            let bytes = pcbcraft_io::preview_png(&s.project, view)?;
            return Ok(
                json!({"content":[{"type":"image","mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(bytes)},{"type":"text","text":format!("{view} design preview; revision {}",s.revision)}],"isError":false}),
            );
        }
        command => s
            .execute(command, args.clone(), None)
            .map_err(|e| e.to_string())?,
    };
    Ok(text_result(result))
}

fn validate_schema(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    let kind = schema.get("type").and_then(Value::as_str);
    let valid = match kind {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("boolean") => value.is_boolean(),
        Some("number") => value.is_number(),
        Some("integer") => value.as_i64().is_some() || value.as_u64().is_some(),
        _ => true,
    };
    if !valid {
        return Err(format!("{path}: expected {}", kind.unwrap_or("value")));
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(value)
    {
        return Err(format!("{path}: unsupported value"));
    }
    if let Some(number) = value.as_f64()
        && (schema
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|min| number < min)
            || schema
                .get("exclusiveMinimum")
                .and_then(Value::as_f64)
                .is_some_and(|min| number <= min)
            || schema
                .get("maximum")
                .and_then(Value::as_f64)
                .is_some_and(|max| number > max))
    {
        return Err(format!("{path}: below minimum"));
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("{path}: missing {key}"));
                }
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        for (key, value) in object {
            if let Some(child) = properties.and_then(|p| p.get(key)) {
                validate_schema(value, child, &format!("{path}.{key}"))?;
            } else if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
                return Err(format!("{path}: unexpected field {key}"));
            }
        }
    }
    if let Some(items) = value.as_array() {
        if schema
            .get("minItems")
            .and_then(Value::as_u64)
            .is_some_and(|min| (items.len() as u64) < min)
            || schema
                .get("maxItems")
                .and_then(Value::as_u64)
                .is_some_and(|max| (items.len() as u64) > max)
        {
            return Err(format!("{path}: invalid item count"));
        }
        if let Some(child) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                validate_schema(item, child, &format!("{path}[{index}]"))?;
            }
        }
    }
    Ok(())
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
pub fn handle_rpc(hub: &DocumentHub, request: Value, connect: Option<&str>) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || method.is_none()
        || id
            .as_ref()
            .is_some_and(|v| !v.is_string() && !v.is_number() && !v.is_null())
    {
        return Some(rpc_error(
            id.unwrap_or(Value::Null),
            -32600,
            "invalid JSON-RPC request",
        ));
    }
    let method = method?;
    let id = id?; // Notifications have no response.
    let params = request.get("params").cloned().unwrap_or(json!({}));
    let result = match method {
        "initialize" => {
            if params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .is_none()
                || !params.get("capabilities").is_some_and(Value::is_object)
                || params
                    .pointer("/clientInfo/name")
                    .and_then(Value::as_str)
                    .is_none()
                || params
                    .pointer("/clientInfo/version")
                    .and_then(Value::as_str)
                    .is_none()
            {
                return Some(rpc_error(
                    id,
                    -32602,
                    "initialize requires protocolVersion, capabilities and clientInfo",
                ));
            }
            let requested = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2025-11-25");
            let version = if SUPPORTED_VERSIONS.contains(&requested) {
                requested
            } else {
                "2025-11-25"
            };
            json!({"protocolVersion":version,"capabilities":{"tools":{"listChanged":false},"resources":{"subscribe":false,"listChanged":false},"prompts":{"listChanged":false}},"serverInfo":{"name":"pcbcraft","version":env!("CARGO_PKG_VERSION")},"instructions":"100% Rust EDA engine. Use inspect_design and list_commands before edits; batches are atomic. Check capability limits and checks.scope."})
        }
        "ping" => json!({}),
        "tools/list" => tool_list(),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "missing tool name"));
            };
            if !tool_list()
                .get("tools")
                .and_then(Value::as_array)
                .is_some_and(|t| {
                    t.iter()
                        .any(|t| t.get("name").and_then(Value::as_str) == Some(name))
                })
            {
                return Some(rpc_error(id, -32602, "unknown tool"));
            }
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            if let Some(address) = connect {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    match forward(address, &request) {
                        Ok(response) => return Some(response),
                        Err(e) => error_result(e),
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = address;
                    error_result("live bridge is unavailable in browser builds".into())
                }
            } else {
                hub.call_tool(name, &args)
            }
        }
        "resources/list" => resources::list(),
        "resources/templates/list" => resources::templates(),
        "resources/read" => {
            let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "missing resource uri"));
            };
            match resources::read(hub, uri) {
                Ok(value) => value,
                Err(e) => {
                    let mut reply = rpc_error(id, e.code, &e.message);
                    if let Some(data) = e.data {
                        reply["error"]["data"] = data;
                    }
                    return Some(reply);
                }
            }
        }
        "prompts/list" => prompts::list(),
        "prompts/get" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "missing prompt name"));
            };
            match prompts::get(name, params.get("arguments").unwrap_or(&Value::Null)) {
                Ok(value) => value,
                Err(prompts::PromptError(message)) => {
                    return Some(rpc_error(id, -32602, &message));
                }
            }
        }
        _ => return Some(rpc_error(id, -32601, "method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<String>, String> {
    let mut bytes = vec![];
    let count = reader
        .take(MAX_MESSAGE + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|e| e.to_string())?;
    if count == 0 {
        return Ok(None);
    }
    if count as u64 > MAX_MESSAGE {
        return Err("message exceeds 1 MiB".into());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|e| e.to_string())
}
/// Run the stdio server loop: one JSON-RPC message per line in, one reply per request out.
///
/// # Rules
/// Before a successful `initialize`, every request except `initialize` and `ping` gets
/// -32002 "initialize the MCP session first" (lifecycle page of the 2025-11-25 spec).
/// Notifications never get a reply. A failed `initialize` does not open the session.
///
/// # Dependencies
/// `crates/mcp/src/lib.rs` (`handle_rpc`, `read_message`).
///
/// # Do not
/// Do not gate `ping`. The HTTP transport in `http.rs` is stateless and does not use this gate.
pub fn serve(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    hub: &DocumentHub,
    connect: Option<&str>,
) -> Result<(), String> {
    let mut initialized = false;
    while let Some(line) = read_message(reader)? {
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => {
                let method = request.get("method").and_then(Value::as_str);
                if !initialized && method.is_some_and(|m| m != "initialize" && m != "ping") {
                    request
                        .get("id")
                        .map(|id| rpc_error(id.clone(), -32002, "initialize the MCP session first"))
                } else {
                    let initialize = method == Some("initialize");
                    let response = handle_rpc(hub, request, connect);
                    if initialize && response.as_ref().is_some_and(|r| r.get("result").is_some()) {
                        initialized = true;
                    }
                    response
                }
            }
            Err(_) => Some(rpc_error(Value::Null, -32700, "parse error")),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut *writer, &response).map_err(|e| e.to_string())?;
            writer.write_all(b"\n").map_err(|e| e.to_string())?;
            writer.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
#[cfg(not(target_arch = "wasm32"))]
fn loopback(address: &str) -> Result<std::net::SocketAddr, String> {
    let address: std::net::SocketAddr = address
        .parse()
        .map_err(|_| "use a literal loopback IP and port, e.g. 127.0.0.1:7991".to_string())?;
    if !address.ip().is_loopback() {
        return Err("control bridge must use a loopback address".into());
    }
    Ok(address)
}
#[cfg(not(target_arch = "wasm32"))]
pub fn forward(address: &str, request: &Value) -> Result<Value, String> {
    let mut stream = std::net::TcpStream::connect_timeout(
        &loopback(address)?,
        std::time::Duration::from_secs(3),
    )
    .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(15)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut stream, request).map_err(|e| e.to_string())?;
    stream.write_all(b"\n").map_err(|e| e.to_string())?;
    let text =
        read_message(&mut BufReader::new(stream))?.ok_or("bridge closed without a response")?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}
#[cfg(not(target_arch = "wasm32"))]
pub fn start_bridge(hub: DocumentHub, address: &str) -> Result<(), String> {
    let listener = std::net::TcpListener::bind(loopback(address)?).map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("pcbcraft-control".into())
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                if stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .is_err()
                {
                    continue;
                }
                if let Ok(reader) = stream.try_clone() {
                    let mut reader = BufReader::new(reader);
                    if let Ok(Some(line)) = read_message(&mut reader) {
                        let response = match serde_json::from_str(&line) {
                            Ok(request) => handle_rpc(&hub, request, None),
                            Err(_) => Some(rpc_error(Value::Null, -32700, "parse error")),
                        };
                        if let Some(response) = response {
                            let _ = serde_json::to_writer(&mut stream, &response);
                            let _ = stream.write_all(b"\n");
                        }
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod http_tests;
#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod tests {
    use super::*;
    fn run_session(lines: &[Value]) -> Result<Vec<Value>, String> {
        let hub = DocumentHub::new(Session::default(), None)?;
        let input: String = lines.iter().map(|line| format!("{line}\n")).collect();
        let mut output = vec![];
        serve(&mut input.as_bytes(), &mut output, &hub, None)?;
        String::from_utf8(output)
            .map_err(|e| e.to_string())?
            .lines()
            .map(|line| serde_json::from_str(line).map_err(|e| e.to_string()))
            .collect()
    }
    fn initialize_request(id: u64) -> Value {
        json!({"jsonrpc":"2.0","id":id,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"1"}}})
    }
    #[test]
    fn methods_other_than_initialize_and_ping_are_refused_before_initialize() -> Result<(), String>
    {
        let replies = run_session(&[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":2,"method":"resources/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"prompts/list"}),
            json!({"jsonrpc":"2.0","id":4,"method":"foo"}),
        ])?;
        assert_eq!(replies.len(), 4);
        for reply in &replies {
            assert_eq!(reply["error"]["code"], -32002, "{reply}");
        }
        Ok(())
    }
    #[test]
    fn ping_works_before_initialize() -> Result<(), String> {
        let replies = run_session(&[json!({"jsonrpc":"2.0","id":1,"method":"ping"})])?;
        assert_eq!(replies[0]["result"], json!({}));
        Ok(())
    }
    #[test]
    fn a_failed_initialize_does_not_open_the_session() -> Result<(), String> {
        let replies = run_session(&[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            initialize_request(3),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/list"}),
        ])?;
        assert_eq!(replies[0]["error"]["code"], -32602);
        assert_eq!(replies[1]["error"]["code"], -32002);
        assert!(replies[2]["result"].is_object());
        assert!(replies[3]["result"]["tools"].is_array());
        Ok(())
    }
    fn listed_tools() -> Result<Vec<Value>, String> {
        tool_list()["tools"]
            .as_array()
            .cloned()
            .ok_or_else(|| "tools is an array".to_string())
    }
    #[test]
    fn every_listed_tool_has_a_title_and_the_four_annotation_hints() -> Result<(), String> {
        for tool in listed_tools()? {
            let name = tool["name"].as_str().unwrap_or("?");
            assert!(
                tool["title"].as_str().is_some_and(|t| !t.is_empty()),
                "{name} has no title"
            );
            for hint in [
                "readOnlyHint",
                "destructiveHint",
                "idempotentHint",
                "openWorldHint",
            ] {
                assert!(
                    tool["annotations"][hint].is_boolean(),
                    "{name} lacks {hint}"
                );
            }
        }
        Ok(())
    }
    #[test]
    fn tools_whose_description_says_without_editing_the_project_are_not_destructive()
    -> Result<(), String> {
        let mut seen = 0;
        for tool in listed_tools()? {
            if tool["description"]
                .as_str()
                .is_some_and(|d| d.contains("without editing the project"))
            {
                seen += 1;
                assert_eq!(
                    tool["annotations"]["destructiveHint"], false,
                    "{}",
                    tool["name"]
                );
            }
        }
        assert!(seen > 10, "the phrase is found on many tools");
        Ok(())
    }
    #[test]
    fn read_only_tools_never_change_the_revision() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let revision = |s: &SharedSession| {
            call_tool(s, "inspect_design", &json!({}))["structuredContent"]["revision"].clone()
        };
        let before = revision(&session);
        assert!(before.is_number(), "inspect_design reports a revision");
        let mut called = 0;
        for tool in listed_tools()? {
            let name = tool["name"].as_str().unwrap_or("?");
            let needs_args = tool["inputSchema"]["required"]
                .as_array()
                .is_some_and(|r| !r.is_empty());
            if tool["annotations"]["readOnlyHint"] == true
                && !needs_args
                && !name.starts_with("design.")
            {
                call_tool(&session, name, &json!({}));
                called += 1;
            }
        }
        assert!(called >= 4, "several read-only tools were called");
        assert_eq!(revision(&session), before);
        Ok(())
    }
    #[test]
    fn new_design_and_open_are_marked_destructive() -> Result<(), String> {
        for tool in listed_tools()? {
            let name = tool["name"].as_str().unwrap_or("?");
            let destructive = tool["annotations"]["destructiveHint"] == true;
            assert_eq!(
                destructive,
                ["new_design", "sample_design", "open"].contains(&name),
                "{name}"
            );
        }
        Ok(())
    }
    #[test]
    fn dry_run_is_listed_and_previews_without_changing_the_revision() -> Result<(), String> {
        let listed = tool_list()["tools"]
            .as_array()
            .is_some_and(|l| l.iter().any(|t| t["name"] == "dry_run"));
        assert!(listed, "dry_run is in tools/list");
        let session = shared(Session::default());
        let reply = call_tool(
            &session,
            "dry_run",
            &json!({"commands":[{"command":"component.add","params":{"kind":"resistor","reference":"R1","value":"10k","position":{"x":10.0,"y":10.0}}}]}),
        );
        assert_eq!(reply["isError"], false, "{reply}");
        let summary = &reply["structuredContent"]["summary"];
        assert_eq!(summary["added"][0][1], "R1", "{reply}");
        let revision = session.lock().map_err(|e| e.to_string())?.revision;
        assert_eq!(revision, 0, "nothing was applied");
        Ok(())
    }
    #[test]
    fn package_tool_is_listed_with_its_schema_and_writes_a_zip() -> Result<(), String> {
        let tools = tool_list();
        let spec = tools["tools"]
            .as_array()
            .and_then(|list| {
                list.iter()
                    .find(|tool| tool["name"] == "output_job.package")
            })
            .ok_or("the package command is in the tool list")?;
        assert_eq!(
            spec["inputSchema"]["required"],
            json!(["path"]),
            "the path is the only required argument"
        );
        assert_eq!(
            spec["inputSchema"]["properties"]["kinds"]["maxItems"], 24,
            "the kind list is bounded"
        );
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let path = std::env::temp_dir().join(format!("pcbcraft-mcp-{}.zip", std::process::id()));
        let reply = call_tool(
            &session,
            "output_job.package",
            &json!({"path": path.to_string_lossy()}),
        );
        assert_eq!(reply["isError"], false, "{reply}");
        assert_eq!(
            reply["structuredContent"]["result"]["manifest"], "manifest.json",
            "the package reports its manifest"
        );
        assert!(path.exists(), "the zip is on disk");
        let missing = call_tool(&session, "output_job.package", &json!({}));
        assert_eq!(missing["isError"], true, "the path is required");
        let _ = std::fs::remove_file(&path);
        Ok(())
    }
    #[test]
    fn board_mesh_tool_returns_triangles_without_editing() -> Result<(), String> {
        let session = shared(Session::default());
        let reply = call_tool(&session, "board_mesh", &json!({"layers":["board"]}));
        assert_eq!(reply["isError"], false, "{reply}");
        let mesh = &reply["structuredContent"];
        assert_eq!(mesh["vertices"], 24);
        assert_eq!(mesh["triangles"], 12);
        assert_eq!(mesh["bounds"]["max"][2], 1.6);
        let bad = call_tool(&session, "board_mesh", &json!({"layers":["wings"]}));
        assert_eq!(bad["isError"], true);
        let summary = call_tool(&session, "board_mesh", &json!({"geometry":false}));
        assert!(
            summary["structuredContent"]["parts"][0]
                .get("indices")
                .is_none()
        );
        Ok(())
    }
    #[test]
    fn checks_copper_only_reports_the_quick_copper_subset() -> Result<(), String> {
        let session = shared(Session::default());
        let a = call_tool(
            &session,
            "execute",
            &json!({"command":"net.create","params":{"name":"A"}}),
        )
        .pointer("/structuredContent/result/id")
        .cloned()
        .ok_or("missing net id")?;
        let b = call_tool(
            &session,
            "execute",
            &json!({"command":"net.create","params":{"name":"B"}}),
        )
        .pointer("/structuredContent/result/id")
        .cloned()
        .ok_or("missing net id")?;
        for (net, y) in [(&a, 10.0), (&b, 10.45)] {
            call_tool(
                &session,
                "execute",
                &json!({"command":"track.add","params":{"net":net,"start":[10.0,y],"end":[30.0,y],"width":0.3}}),
            );
        }
        let full = call_tool(&session, "checks", &json!({}))["structuredContent"].clone();
        let codes: Vec<String> = full["drc"]
            .as_array()
            .ok_or("missing drc")?
            .iter()
            .filter_map(|f| f["code"].as_str().map(str::to_string))
            .collect();
        let erc_codes: Vec<String> = full["erc"]
            .as_array()
            .ok_or("missing erc")?
            .iter()
            .filter_map(|f| f["code"].as_str().map(str::to_string))
            .collect();
        assert!(
            codes.contains(&"clearance".to_string())
                && erc_codes.contains(&"single_pin_net".to_string()),
            "the full pass reports copper and non-copper findings, got {codes:?} {erc_codes:?}"
        );
        let quick =
            call_tool(&session, "checks", &json!({"copper_only": true}))["structuredContent"]
                .clone();
        let quick_codes: Vec<String> = quick["drc"]
            .as_array()
            .ok_or("missing drc")?
            .iter()
            .filter_map(|f| f["code"].as_str().map(str::to_string))
            .collect();
        assert!(
            quick_codes.contains(&"clearance".to_string()),
            "the quick pass keeps the copper finding"
        );
        assert!(
            !quick_codes.contains(&"single_pin_net".to_string())
                && !quick_codes.contains(&"unrouted".to_string()),
            "the quick pass drops non-copper findings, got {quick_codes:?}"
        );
        assert!(
            quick["erc"]
                .as_array()
                .ok_or("missing erc")?
                .iter()
                .all(|f| f["code"] == "unassigned_pin" || f["code"] == "missing_footprint"),
            "the quick pass keeps only copper-subset ERC findings"
        );
        assert!(
            quick["airwires"].is_array(),
            "airwires still travel with the quick pass"
        );
        assert!(
            quick["scope"]
                .as_str()
                .unwrap_or("")
                .contains("copper-only"),
            "the quick pass says what it ran"
        );
        Ok(())
    }

    #[test]
    fn native_library_save_load_preserves_original_data_and_rejects_unknown_fields()
    -> Result<(), String> {
        let session = shared(Session::default());
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/device.kicad_sym");
        let inspected = call_tool(
            &session,
            "library.inspect_symbol",
            &json!({"path":path,"name":"R","include_source":true}),
        );
        let definition = inspected
            .pointer("/structuredContent/definition")
            .cloned()
            .ok_or("missing definition")?;
        assert!(
            !definition
                .get("raw_source")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty()
        );
        assert_eq!(
            call_tool(
                &session,
                "library.symbol.upsert",
                &json!({"definition":definition,"notice":"Original fixture data license retained"})
            )
            .get("isError"),
            Some(&json!(false))
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let file = directory.path().join("parts.pcbcraft-lib");
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        assert_eq!(
            call_tool(&session, "library.save", &json!({"path":file})).get("isError"),
            Some(&json!(false))
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "saving a library does not edit history"
        );
        assert_eq!(
            call_tool(&session, "library.symbol.remove", &json!({"name":"R"})).get("isError"),
            Some(&json!(false))
        );
        assert_eq!(
            call_tool(&session, "library.load", &json!({"path":file})).get("isError"),
            Some(&json!(false))
        );
        assert_eq!(
            session
                .lock()
                .map_err(|_| "session unavailable")?
                .project
                .library
                .symbols
                .first()
                .map(|s| s.name.as_str()),
            Some("R")
        );
        let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
        let mut invalid: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        invalid["library"]["future_provenance"] = json!({"unknown":true});
        std::fs::write(
            &file,
            serde_json::to_vec(&invalid).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        assert_eq!(
            call_tool(&session, "library.load", &json!({"path":file})).get("isError"),
            Some(&json!(true))
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before
        );
        Ok(())
    }
    #[test]
    fn export_tool_writes_png_raster_previews_without_editing() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        for view in ["pcb", "schematic"] {
            let path = directory.path().join(format!("preview-{view}.png"));
            let out = call_tool(
                &session,
                "export",
                &json!({"path": path, "format": "png", "view": view}),
            );
            assert_eq!(
                out.get("isError"),
                Some(&json!(false)),
                "{view} export failed"
            );
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            assert!(
                bytes.starts_with(&[0x89, b'P', b'N', b'G']),
                "the {view} file is a real PNG"
            );
        }
        let bad = call_tool(
            &session,
            "export",
            &json!({"path": directory.path().join("nope.bmp"), "format": "bmp"}),
        );
        assert_eq!(bad.get("isError"), Some(&json!(true)));
        assert!(
            !directory.path().join("nope.bmp").exists(),
            "a refused export writes nothing"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "exporting never edits the project"
        );
        Ok(())
    }

    #[test]
    fn library_export_tools_write_files_the_reader_reparses() -> Result<(), String> {
        let session = shared(Session::default());
        let upsert = call_tool(
            &session,
            "execute",
            &json!({"command":"library.footprint.upsert","params":{"definition":{
                "name":"EXPORT-R","description":"test resistor","keywords":"r",
                "pads":[{"number":"1","pad_type":"smd","shape":"roundrect",
                    "position":{"x":-1.0,"y":0.0},"rotation":0.0,
                    "size":{"x":1.0,"y":1.2},
                    "layers":["F.Cu","F.Mask","F.Paste"],
                    "drill":null,"roundrect_ratio":0.25,"raw_source":""}],
                "graphics":[],"raw_source":"","unsupported":[]}}}),
        );
        assert_eq!(upsert.get("isError"), Some(&json!(false)));
        let upsert = call_tool(
            &session,
            "execute",
            &json!({"command":"library.symbol.upsert","params":{"definition":{
                "name":"EXPORT-IC","reference":"U?","value":"EXPORT-IC",
                "description":"two-pin test symbol","keywords":"test",
                "footprint":"EXPORT-R",
                "pins":[{"number":"1","name":"IN","position":{"x":-5.08,"y":0.0},
                    "angle":0.0,"length":2.54,"electrical_type":"input",
                    "unit":1,"body_style":1}],
                "graphics":[{"unit":1,"body_style":1,"primitive":
                    {"Rectangle":{"start":{"x":-2.54,"y":2.54},
                    "end":{"x":2.54,"y":-2.54},"width":0.254,"fill":"none",
                    "layer":"0"}}}],
                "raw_source":"","unsupported":[]}}}),
        );
        assert_eq!(upsert.get("isError"), Some(&json!(false)));
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let mod_path = directory.path().join("EXPORT-R.kicad_mod");
        let export = call_tool(
            &session,
            "library.export_footprint",
            &json!({"path":mod_path,"name":"EXPORT-R"}),
        );
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(&mod_path).map_err(|e| e.to_string())?;
        let again = pcbcraft_library::read_footprint(&text).map_err(|e| e.to_string())?;
        assert_eq!(again.name, "EXPORT-R");
        assert_eq!(again.pads.len(), 1);
        assert_eq!(again.pads[0].size.x, 1.0);
        assert_eq!(again.pads[0].roundrect_ratio, Some(0.25));
        let sym_path = directory.path().join("EXPORT-IC.kicad_sym");
        let export = call_tool(
            &session,
            "library.export_symbol",
            &json!({"path":sym_path,"name":"EXPORT-IC"}),
        );
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(&sym_path).map_err(|e| e.to_string())?;
        let again = pcbcraft_library::read_symbols(&text).map_err(|e| e.to_string())?;
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].pins.len(), 1);
        assert_eq!(again[0].pins[0].position.x, -5.08);
        assert_eq!(again[0].graphics.len(), 1);
        let fp_svg_path = directory.path().join("EXPORT-R.svg");
        let export = call_tool(
            &session,
            "library.export_footprint_svg",
            &json!({"path":fp_svg_path,"name":"EXPORT-R"}),
        );
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let preview = std::fs::read_to_string(&fp_svg_path).map_err(|e| e.to_string())?;
        assert!(preview.starts_with("<?xml"), "a real SVG document");
        assert!(preview.contains("<title>EXPORT-R</title>"));
        assert!(
            preview.contains("<circle")
                || preview.contains("<rect")
                || preview.contains("<polygon"),
            "the pad is drawn"
        );
        let sym_svg_path = directory.path().join("EXPORT-IC.svg");
        let export = call_tool(
            &session,
            "library.export_symbol_svg",
            &json!({"path":sym_svg_path,"name":"EXPORT-IC"}),
        );
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let preview = std::fs::read_to_string(&sym_svg_path).map_err(|e| e.to_string())?;
        assert!(preview.starts_with("<?xml"), "a real SVG document");
        assert!(preview.contains("<line"), "the pin is drawn");
        assert_eq!(
            call_tool(
                &session,
                "library.export_footprint_svg",
                &json!({"path":directory.path().join("wrong.txt"),"name":"EXPORT-R"})
            )
            .get("isError"),
            Some(&json!(true))
        );
        assert_eq!(
            call_tool(
                &session,
                "library.export_footprint",
                &json!({"path":directory.path().join("wrong.txt"),"name":"EXPORT-R"})
            )
            .get("isError"),
            Some(&json!(true))
        );
        assert_eq!(
            call_tool(
                &session,
                "library.export_symbol",
                &json!({"path":sym_path,"name":"NOPE"})
            )
            .get("isError"),
            Some(&json!(true))
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before
        );
        Ok(())
    }
    #[test]
    fn gerber_export_writes_fabrication_files_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        let net = call_tool(
            &session,
            "execute",
            &json!({"command":"net.create","params":{"name":"SIG"}}),
        );
        let net_id = net
            .pointer("/structuredContent/result/id")
            .cloned()
            .ok_or("missing net id")?;
        assert_eq!(
            call_tool(
                &session,
                "execute",
                &json!({"command":"track.add","params":{"net":net_id,"start":[10.0,10.0],"end":[30.0,10.0],"width":0.3}})
            )
            .get("isError"),
            Some(&json!(false))
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "gerber.export", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        for name in [
            "pcb-F.Cu.gbr",
            "pcb-B.Cu.gbr",
            "pcb-F.Mask.gbr",
            "pcb-B.Mask.gbr",
            "pcb-F.Paste.gbr",
            "pcb-B.Paste.gbr",
            "pcb-F.SilkS.gbr",
            "pcb-B.SilkS.gbr",
            "pcb-Edge.gbr",
            "pcb-Plated.drl",
            "pcb-DrillMap.gbr",
            "README-fabrication.txt",
        ] {
            let text =
                std::fs::read_to_string(directory.path().join(name)).map_err(|e| e.to_string())?;
            assert!(!text.is_empty(), "{name} should carry content");
        }
        let front = std::fs::read_to_string(directory.path().join("pcb-F.Cu.gbr"))
            .map_err(|e| e.to_string())?;
        assert!(
            front.contains("%TF.FileFunction,Copper,L1,Top*%"),
            "front copper carries its file function attribute"
        );
        assert!(
            front.contains("X10.000000Y10.000000D02*\n"),
            "the exported copper should contain the drawn track"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "exporting fabrication files does not edit history"
        );
        Ok(())
    }
    #[test]
    fn dsn_write_exports_a_board_file_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        for reference in ["R1", "R2"] {
            call_tool(
                &session,
                "execute",
                &json!({"command":"component.add","params":{"kind":"resistor","reference":reference,"position":[10.0,10.0]}}),
            );
        }
        call_tool(
            &session,
            "execute",
            &json!({"command":"netlist.import","params":{"nets":[{"name":"SIG","pins":[{"reference":"R1","pin":"1"},{"reference":"R2","pin":"1"}]}]}}),
        );
        call_tool(
            &session,
            "execute",
            &json!({"command":"board.sync","params":{}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "dsn.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text =
            std::fs::read_to_string(directory.path().join("pcb.dsn")).map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("(PCB \""),
            "the board file is s-expression PCB"
        );
        assert!(
            text.contains("(net 1 \"SIG\" (pins")
                && text.contains("(polygon (layer BOARD_BOUNDARY)"),
            "nets and the board boundary are carried"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing a board file must not change the project"
        );
        Ok(())
    }
    #[test]
    fn cmp_write_lists_pads_and_nets_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        for reference in ["R1", "R2"] {
            call_tool(
                &session,
                "execute",
                &json!({"command":"component.add","params":{"kind":"resistor","reference":reference,"position":[10.0,10.0]}}),
            );
        }
        call_tool(
            &session,
            "execute",
            &json!({"command":"netlist.import","params":{"nets":[{"name":"SIG","pins":[{"reference":"R1","pin":"1"},{"reference":"R2","pin":"1"}]}]}}),
        );
        call_tool(
            &session,
            "execute",
            &json!({"command":"board.sync","params":{}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let path = directory.path().join("pcb.cmp");
        let export = call_tool(&session, "cmp.write", &json!({"path":path}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("$FILE\nTYPE:BOARD\nCODE:ASCII\n"),
            "the trace file starts with its header"
        );
        assert!(
            text.contains("$COMPONENTS") && text.matches("$CD ").count() >= 4,
            "every placed pad is listed"
        );
        assert!(text.contains("\nSIG\t"), "the named net lists its pads");
        assert_eq!(
            call_tool(
                &session,
                "cmp.write",
                &json!({"path":directory.path().join("wrong.txt")})
            )
            .get("isError"),
            Some(&json!(true)),
            "the path must end in .cmp"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing a trace file must not change the project"
        );
        Ok(())
    }

    #[test]
    fn report_write_summarizes_parts_and_nets_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        for reference in ["R1", "R2"] {
            call_tool(
                &session,
                "execute",
                &json!({"command":"component.add","params":{"kind":"resistor","reference":reference,"position":[10.0,10.0]}}),
            );
        }
        call_tool(
            &session,
            "execute",
            &json!({"command":"netlist.import","params":{"nets":[{"name":"SIG","pins":[{"reference":"R1","pin":"1"},{"reference":"R2","pin":"1"}]}]}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let path = directory.path().join("report.txt");
        let export = call_tool(&session, "report.write", &json!({"path":path}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("Design report for "),
            "the report opens with the design summary"
        );
        assert!(
            text.contains("COMPONENTS")
                && text.contains("R1")
                && text.contains("SIG,\"R1.1 R2.1\""),
            "the report lists parts and the net with its pins: {text}"
        );
        assert_eq!(
            call_tool(
                &session,
                "report.write",
                &json!({"path":directory.path().join("wrong.md")})
            )
            .get("isError"),
            Some(&json!(true)),
            "the path must end in .txt"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing the report must not change the project"
        );
        Ok(())
    }

    #[test]
    fn ipc2581_write_carries_nets_and_copper_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        for reference in ["R1", "R2"] {
            call_tool(
                &session,
                "execute",
                &json!({"command":"component.add","params":{"kind":"resistor","reference":reference,"position":[10.0,10.0]}}),
            );
        }
        call_tool(
            &session,
            "execute",
            &json!({"command":"netlist.import","params":{"nets":[{"name":"SIG","pins":[{"reference":"R1","pin":"1"},{"reference":"R2","pin":"1"}]}]}}),
        );
        call_tool(
            &session,
            "execute",
            &json!({"command":"board.sync","params":{}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let path = directory.path().join("assembly.xml");
        let export = call_tool(&session, "ipc2581.write", &json!({"path":path}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("<?xml version=\"1.0\""),
            "the exchange document opens as XML"
        );
        assert!(
            text.contains("<NET name=\"SIG\">") && text.contains("<PIN id=\"R1.1\"/>"),
            "the net lists the pins that share it"
        );
        assert_eq!(
            call_tool(
                &session,
                "ipc2581.write",
                &json!({"path":directory.path().join("wrong.txt")})
            )
            .get("isError"),
            Some(&json!(true)),
            "the path must end in .xml"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing the exchange file must not change the project"
        );
        Ok(())
    }

    #[test]
    fn gencad_write_exports_an_interchange_text_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        for reference in ["R1", "R2"] {
            call_tool(
                &session,
                "execute",
                &json!({"command":"component.add","params":{"kind":"resistor","reference":reference,"position":[10.0,10.0]}}),
            );
        }
        call_tool(
            &session,
            "execute",
            &json!({"command":"netlist.import","params":{"nets":[{"name":"SIG","pins":[{"reference":"R1","pin":"1"},{"reference":"R2","pin":"1"}]}]}}),
        );
        call_tool(
            &session,
            "execute",
            &json!({"command":"board.sync","params":{}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "gencad.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(directory.path().join("board.gencad"))
            .map_err(|e| e.to_string())?;
        assert!(
            text.lines().any(|l| l.starts_with("%C\t"))
                && text.lines().any(|l| l.starts_with("%P\t")),
            "components and pins are recorded"
        );
        assert!(
            text.lines()
                .any(|l| l.starts_with("%N\tSIG\t") && l.contains("\t1\t")),
            "the named net lists its members by index"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing an interchange text must not change the project"
        );
        Ok(())
    }

    #[test]
    fn library_batch_exports_write_every_entry_as_a_text_file() -> Result<(), String> {
        let session = shared(Session::default());
        let root = tempfile::tempdir().map_err(|e| e.to_string())?;
        let fixtures =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../library/tests/fixtures");
        for (command, file) in [
            (
                "library.import_footprint_file",
                "R_0805_2012Metric.kicad_mod",
            ),
            ("library.import_footprint_file", "DIP-8_W7.62mm.kicad_mod"),
            ("library.import_symbol_file", "device.kicad_sym"),
        ] {
            let path = fixtures.join(file).to_string_lossy().into_owned();
            let import = call_tool(
                &session,
                "execute",
                &json!({"command":command,"params":{"path":path}}),
            );
            assert_eq!(import.get("isError"), Some(&json!(false)));
        }
        let directory = root.path().join("out");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let footprints = call_tool(
            &session,
            "library.export_footprints",
            &json!({"path":directory}),
        );
        assert_eq!(footprints.get("isError"), Some(&json!(false)));
        assert_eq!(
            footprints
                .pointer("/structuredContent/files")
                .cloned()
                .unwrap_or_default(),
            json!(["DIP-8_W7.62mm.kicad_mod", "R_0805_2012Metric.kicad_mod"]),
            "one file per footprint, sorted by name"
        );
        let symbols = call_tool(
            &session,
            "library.export_symbols",
            &json!({"path":directory}),
        );
        assert_eq!(symbols.get("isError"), Some(&json!(false)));
        assert_eq!(
            symbols
                .pointer("/structuredContent/files")
                .cloned()
                .unwrap_or_default(),
            json!(["LED.kicad_sym", "R.kicad_sym"]),
            "one file per symbol, sorted by name"
        );
        let round_trip = call_tool(
            &session,
            "library.import_footprint_file",
            &json!({"path":directory.join("DIP-8_W7.62mm.kicad_mod")}),
        );
        assert_eq!(
            round_trip.get("isError"),
            Some(&json!(false)),
            "an exported file reads straight back"
        );
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let footprints = call_tool(
            &session,
            "library.export_footprints",
            &json!({"path":directory}),
        );
        assert_eq!(footprints.get("isError"), Some(&json!(false)));
        let symbols = call_tool(
            &session,
            "library.export_symbols",
            &json!({"path":directory}),
        );
        assert_eq!(symbols.get("isError"), Some(&json!(false)));
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "batch export must not change the project"
        );
        Ok(())
    }

    #[test]
    fn plot_csv_write_reports_each_copper_layer_without_editing() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(
            &session,
            "plot_csv.write",
            &json!({"path":directory.path()}),
        );
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(directory.path().join("plot-report.csv"))
            .map_err(|e| e.to_string())?;
        let rows: Vec<Vec<&str>> = text
            .trim_end()
            .lines()
            .map(|line| line.split(',').collect())
            .collect();
        assert_eq!(
            rows.len(),
            4,
            "header, both copper layers, and a totals row"
        );
        assert_eq!(rows[1][0], "B.Cu");
        assert_eq!(rows[2][0], "F.Cu");
        assert_eq!(rows[3][0], "total");
        let plotted: usize = rows[3][1..8]
            .iter()
            .map(|cell| cell.parse::<usize>().map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .sum();
        assert_eq!(plotted, 0, "a schematic-only sample plots no copper");
        assert_eq!(
            call_tool(&session, "plot_csv.write", &json!({})).get("isError"),
            Some(&json!(true)),
            "a missing directory is refused"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing the plot report must not change the project"
        );
        call_tool(
            &session,
            "execute",
            &json!({"command":"track.add","params":{"net":4,"start":[1.0,2.0],"end":[8.0,2.0],"width":0.25,"layer":"F.Cu"}}),
        );
        let export = call_tool(
            &session,
            "plot_csv.write",
            &json!({"path":directory.path()}),
        );
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(directory.path().join("plot-report.csv"))
            .map_err(|e| e.to_string())?;
        assert!(
            text.lines()
                .nth(2)
                .ok_or_else(|| "missing front layer row".to_string())?
                .contains("F.Cu,0,1,0,0,0,0,0,1.000,2.000,8.000,2.000"),
            "a front track appears on the front row with its extent"
        );
        Ok(())
    }

    #[test]
    fn hyperlynx_write_exports_a_simulation_netlist_without_editing_history() -> Result<(), String>
    {
        let session = shared(Session::default());
        for reference in ["R1", "R2"] {
            call_tool(
                &session,
                "execute",
                &json!({"command":"component.add","params":{"kind":"resistor","reference":reference,"position":[10.0,10.0]}}),
            );
        }
        call_tool(
            &session,
            "execute",
            &json!({"command":"netlist.import","params":{"nets":[{"name":"SIG","pins":[{"reference":"R1","pin":"1"},{"reference":"R2","pin":"1"}]}]}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(
            &session,
            "hyperlynx.write",
            &json!({"path":directory.path()}),
        );
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(directory.path().join("netlist.csv"))
            .map_err(|e| e.to_string())?;
        assert_eq!(
            text.trim_end(),
            "SIG,R1.1,R2.1",
            "the named net lists both pins comma separated"
        );
        assert_eq!(
            call_tool(
                &session,
                "hyperlynx.write",
                &json!({"path":directory.path()})
            )
            .get("isError"),
            Some(&json!(false)),
            "a connected design exports again without complaint"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing a simulation netlist must not change the project"
        );
        Ok(())
    }

    #[test]
    fn wrl_write_exports_a_vrml_scene_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        for reference in ["R1", "R2"] {
            call_tool(
                &session,
                "execute",
                &json!({"command":"component.add","params":{"kind":"resistor","reference":reference,"position":[10.0,10.0]}}),
            );
        }
        call_tool(
            &session,
            "execute",
            &json!({"command":"board.sync","params":{}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "wrl.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(directory.path().join("board.wrl"))
            .map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("#VRML V2.0 utf8\n"),
            "the scene declares VRML 2.0"
        );
        assert!(
            text.matches("IndexedFaceSet").count() >= 3,
            "the outline and the placed pads each become a solid"
        );
        assert!(
            text.matches("Shape {").count() == text.matches("IndexedFaceSet").count(),
            "every shape carries exactly one solid"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing a VRML scene must not change the project"
        );
        Ok(())
    }

    #[test]
    fn idf_write_exports_a_mechanical_drawing_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        for reference in ["R1", "R2"] {
            call_tool(
                &session,
                "execute",
                &json!({"command":"component.add","params":{"kind":"resistor","reference":reference,"position":[10.0,10.0]}}),
            );
        }
        call_tool(
            &session,
            "execute",
            &json!({"command":"board.sync","params":{}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "idf.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(directory.path().join("board.idf"))
            .map_err(|e| e.to_string())?;
        assert!(
            text.starts_with(".FILE_IDF\n.3DIDF VERSION: 2.0\n"),
            "the mechanical drawing declares itself"
        );
        assert!(
            text.contains(".END_BODY R1\t") && text.contains(".END_BODY R2\t"),
            "every placed part is listed with its position"
        );
        assert!(
            text.contains(".PIN 2\n"),
            "the embedded pin file counts every part"
        );
        assert!(text.ends_with(".END_FILE_IDF\n"), "the file closes");
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing a mechanical drawing must not change the project"
        );
        Ok(())
    }

    #[test]
    fn ipc356_write_exports_a_test_netlist_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        for reference in ["R1", "R2"] {
            call_tool(
                &session,
                "execute",
                &json!({"command":"component.add","params":{"kind":"resistor","reference":reference,"position":[10.0,10.0]}}),
            );
        }
        call_tool(
            &session,
            "execute",
            &json!({"command":"netlist.import","params":{"nets":[{"name":"SIG","pins":[{"reference":"R1","pin":"1"},{"reference":"R2","pin":"1"}]}]}}),
        );
        call_tool(
            &session,
            "execute",
            &json!({"command":"board.sync","params":{}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "ipc356.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(directory.path().join("test.brd"))
            .map_err(|e| e.to_string())?;
        let rows: Vec<&str> = text.lines().filter(|l| !l.starts_with('*')).collect();
        assert_eq!(rows.len(), 4, "one fixed record per placed pad");
        assert!(
            rows.iter().all(|r| r.chars().count() == 49),
            "records keep the fixed column layout"
        );
        assert!(
            rows.iter().any(|r| r.starts_with("00001"))
                && rows.iter().any(|r| r.starts_with("00000")),
            "connected pads carry their net number and the rest read zero"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing a test netlist must not change the project"
        );
        Ok(())
    }

    #[test]
    fn stats_write_exports_counters_without_editing_history() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "stats.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        assert_eq!(
            export["structuredContent"]["file"],
            json!("board-stats.csv")
        );
        let text = std::fs::read_to_string(directory.path().join("board-stats.csv"))
            .map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("# Board statistics for "),
            "the sheet names the design"
        );
        assert!(
            text.contains("KEY,VALUE") && text.contains("components,"),
            "the counter table is always present"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing the sheet must not change the project"
        );
        Ok(())
    }

    #[test]
    fn drill_chart_write_lists_tools_without_editing_history() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(
            &session,
            "drill_chart.write",
            &json!({"path":directory.path()}),
        );
        assert_eq!(export.get("isError"), Some(&json!(false)));
        assert_eq!(
            export["structuredContent"]["file"],
            json!("board-drill-tools.csv")
        );
        let text = std::fs::read_to_string(directory.path().join("board-drill-tools.csv"))
            .map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("# Drill tool chart for "),
            "the chart names the design"
        );
        assert!(
            text.contains("TOOL,DIAMETER_MM,SLOT_MM,PLATED,HOLES"),
            "the tool table header is always present"
        );
        assert!(
            text.contains("TOTAL,,,,"),
            "the chart ends with a total row"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing the chart must not change the project"
        );
        Ok(())
    }

    #[test]
    fn stackup_write_exports_a_fabrication_sheet_without_editing_history() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "stackup.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        assert_eq!(
            export["structuredContent"]["file"],
            json!("board-stackup.txt")
        );
        let text = std::fs::read_to_string(directory.path().join("board-stackup.txt"))
            .map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("Stackup report for "),
            "the sheet names the design"
        );
        assert!(
            text.contains("NAME,KIND,THICKNESS_MM"),
            "the layer note table is always present"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing the sheet must not change the project"
        );
        Ok(())
    }

    #[test]
    fn hpgl_write_exports_a_plotter_drawing_without_editing_history() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "hpgl.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        assert_eq!(export["structuredContent"]["file"], json!("pcb-pcb.hpgl"));
        let text = std::fs::read_to_string(directory.path().join("pcb-pcb.hpgl"))
            .map_err(|e| e.to_string())?;
        assert!(text.starts_with("IN;SP1;"), "the plotter is initialised");
        assert!(text.ends_with("PU;\n"), "the pen is lifted at the end");
        let schematic = call_tool(
            &session,
            "hpgl.write",
            &json!({"path":directory.path(),"view":"schematic"}),
        );
        assert_eq!(schematic.get("isError"), Some(&json!(false)));
        assert_eq!(
            schematic["structuredContent"]["file"],
            json!("pcb-schematic.hpgl")
        );
        assert_eq!(
            call_tool(
                &session,
                "hpgl.write",
                &json!({"path":directory.path(),"view":"isometric"})
            )
            .get("isError"),
            Some(&json!(true)),
            "only the two known views can be plotted"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing the plot must not change the project"
        );
        Ok(())
    }

    #[test]
    fn odb_write_exports_a_board_package_without_editing_history() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "odb.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        assert_eq!(export["structuredContent"]["file"], json!("board.odb.zip"));
        let bytes =
            std::fs::read(directory.path().join("board.odb.zip")).map_err(|e| e.to_string())?;
        assert!(
            bytes.starts_with(b"PK\x03\x04"),
            "the package is a ZIP file"
        );
        let text = String::from_utf8_lossy(&bytes).into_owned();
        assert!(
            text.contains("/matrix/matrix") && text.contains("/features/top"),
            "the tree holds the job matrix and copper feature files"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing the package must not change the project"
        );
        Ok(())
    }

    #[test]
    fn ps_write_exports_a_drawing_without_editing_history() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "ps.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        assert_eq!(export["structuredContent"]["file"], json!("pcb-pcb.ps"));
        let text = std::fs::read_to_string(directory.path().join("pcb-pcb.ps"))
            .map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("%!PS-Adobe-3.0"),
            "the drawing opens as PostScript"
        );
        assert!(
            text.ends_with("showpage\n%%EOF\n"),
            "the page ends with showpage"
        );
        let schematic = call_tool(
            &session,
            "ps.write",
            &json!({"path":directory.path(),"view":"schematic"}),
        );
        assert_eq!(schematic.get("isError"), Some(&json!(false)));
        assert_eq!(
            schematic["structuredContent"]["file"],
            json!("pcb-schematic.ps")
        );
        assert_eq!(
            call_tool(
                &session,
                "ps.write",
                &json!({"path":directory.path(),"view":"isometric"})
            )
            .get("isError"),
            Some(&json!(true)),
            "only the two known views can be drawn"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing the drawing must not change the project"
        );
        Ok(())
    }

    #[test]
    fn pdf_write_exports_a_drawing_without_editing_history() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "pdf.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let bytes =
            std::fs::read(directory.path().join("pcb-pcb.pdf")).map_err(|e| e.to_string())?;
        assert!(bytes.starts_with(b"%PDF-1.4"), "the file is a PDF");
        assert!(bytes.windows(6).any(|w| w == b"%%EOF\n"));
        let schematic = call_tool(
            &session,
            "pdf.write",
            &json!({"path":directory.path(),"view":"schematic"}),
        );
        assert_eq!(schematic.get("isError"), Some(&json!(false)));
        assert!(
            std::fs::read(directory.path().join("pcb-schematic.pdf"))
                .map_err(|e| e.to_string())?
                .starts_with(b"%PDF-1.4")
        );
        let bad = call_tool(
            &session,
            "pdf.write",
            &json!({"path":directory.path(),"view":"layout"}),
        );
        assert_eq!(bad.get("isError"), Some(&json!(true)));
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing a PDF must not change the project"
        );
        Ok(())
    }

    #[test]
    fn pdf_write_accepts_drawing_views() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        for view in ["assembly-top", "assembly-bottom", "fabrication"] {
            let export = call_tool(
                &session,
                "pdf.write",
                &json!({"path":directory.path(),"view":view}),
            );
            assert_eq!(export.get("isError"), Some(&json!(false)), "{view} PDF");
            let bytes = std::fs::read(directory.path().join(format!("pcb-{view}.pdf")))
                .map_err(|e| e.to_string())?;
            assert!(bytes.starts_with(b"%PDF-1.4"), "{view} file is a PDF");
        }
        Ok(())
    }

    #[test]
    fn step_write_exports_a_board_model_without_editing_history() -> Result<(), String> {
        let session = shared(Session::sample().map_err(|e| e.to_string())?);
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "step.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(directory.path().join("board.step"))
            .map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("ISO-10303-21;"),
            "the file is a STEP model"
        );
        assert!(text.ends_with("END-ISO-10303-21;\n"));
        assert!(text.contains("ADVANCED_FACE("), "faces describe the solids");
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "writing a STEP model must not change the project"
        );
        Ok(())
    }

    #[test]
    fn netlist_write_exports_pins_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        call_tool(
            &session,
            "execute",
            &json!({"command":"component.add","params":{"kind":"resistor","reference":"R1","position":[10.0,10.0]}}),
        );
        call_tool(
            &session,
            "execute",
            &json!({"command":"component.add","params":{"kind":"resistor","reference":"R2","position":[30.0,10.0]}}),
        );
        call_tool(
            &session,
            "execute",
            &json!({"command":"netlist.import","params":{"nets":[{"name":"SIG","pins":[{"reference":"R1","pin":"1"},{"reference":"R2","pin":"1"}]}]}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "netlist.write", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(directory.path().join("pcb-netlist.net"))
            .map_err(|e| e.to_string())?;
        assert!(
            text.contains("SIG R1.1 R2.1\n"),
            "the exported netlist should carry the connected pins"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "the export must not edit the design"
        );
        Ok(())
    }
    #[test]
    fn bom_write_collapses_identical_parts_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        for reference in ["R1", "R2", "C1"] {
            call_tool(
                &session,
                "execute",
                &json!({"command":"component.add","params":{"kind":"resistor","reference":reference,"value":if reference == "C1" {"100n"} else {"10k"},"position":[10.0,10.0]}}),
            );
        }
        let id = {
            let guard = session.lock().map_err(|_| "session unavailable")?;
            guard.inspect()["project"]["components"][0]["id"]
                .as_u64()
                .ok_or("missing component id")?
        };
        call_tool(
            &session,
            "execute",
            &json!({"command":"component.field","params":{"id":id,"key":"Tolerance","value":"1%"}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(
            &session,
            "bom.write",
            &json!({"path":directory.path(),"fields":["Tolerance"]}),
        );
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text =
            std::fs::read_to_string(directory.path().join("bom.csv")).map_err(|e| e.to_string())?;
        assert_eq!(
            text.lines().next(),
            Some("quantity,references,value,kind,footprint,Tolerance"),
            "the requested property becomes a column"
        );
        assert!(
            text.contains("1,R2,10k,resistor,starter pads,"),
            "a part with no property value gets an empty column"
        );
        assert!(
            text.contains("1,R1,10k,resistor,starter pads,1%"),
            "the property column carries the set value"
        );
        let bare = call_tool(&session, "bom.write", &json!({"path":directory.path()}));
        assert_eq!(bare.get("isError"), Some(&json!(false)));
        let plain =
            std::fs::read_to_string(directory.path().join("bom.csv")).map_err(|e| e.to_string())?;
        assert_eq!(
            plain.lines().next(),
            Some("quantity,references,value,kind,footprint"),
            "fields are optional"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "the export must not edit the design"
        );
        Ok(())
    }
    #[test]
    fn dxf_export_writes_the_board_drawing_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        call_tool(
            &session,
            "execute",
            &json!({"command":"net.create","params":{"name":"SIG"}}),
        );
        let net_id = call_tool(
            &session,
            "execute",
            &json!({"command":"net.create","params":{"name":"GND"}}),
        )
        .pointer("/structuredContent/result/id")
        .cloned()
        .ok_or("missing net id")?;
        let sig = session.lock().map_err(|_| "session unavailable")?.inspect()["project"]["nets"]
            [0]["id"]
            .clone();
        call_tool(
            &session,
            "execute",
            &json!({"command":"track.add","params":{"net":sig,"start":[10.0,10.0],"end":[30.0,10.0],"width":0.3}}),
        );
        call_tool(
            &session,
            "execute",
            &json!({"command":"via.place","params":{"net":net_id,"position":[30.0,20.0],"diameter":0.7}}),
        );
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(&session, "dxf.export", &json!({"path":directory.path()}));
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text =
            std::fs::read_to_string(directory.path().join("pcb.dxf")).map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("0\nSECTION\n2\nHEADER\n"),
            "a DXF header first"
        );
        assert!(
            text.contains("0\nLINE\n8\nF_Cu\n10\n10.000\n20\n10.000\n11\n30.000\n21\n10.000\n"),
            "the routed track should be a line entity"
        );
        assert!(
            text.contains("0\nLWPOLYLINE\n8\nEdge\n90\n4\n70\n1\n"),
            "the board outline should be a closed polyline"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "the export must not edit the design"
        );
        Ok(())
    }
    #[test]
    fn placement_write_lists_placed_parts_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        call_tool(&session, "sample_design", &json!({}));
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let before = session.lock().map_err(|_| "session unavailable")?.inspect();
        let export = call_tool(
            &session,
            "placement.write",
            &json!({"path":directory.path()}),
        );
        assert_eq!(export.get("isError"), Some(&json!(false)));
        let text = std::fs::read_to_string(directory.path().join("placement.txt"))
            .map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("File Format,ASCII,\n"),
            "a centroid file starts with its format line"
        );
        assert!(
            text.contains("SMT,THT,X,Y,Rotation,Side,Package\n"),
            "the column header names the fields"
        );
        let rows: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("SMT,\"") || l.starts_with("THT,\""))
            .collect();
        assert_eq!(rows.len(), 3, "one row per placed part");
        let mut sorted = rows.clone();
        sorted.sort_by_key(|row| row.split('"').nth(1).unwrap_or("").to_string());
        assert_eq!(rows, sorted, "rows are listed in reference order");
        assert!(
            rows.iter()
                .any(|row| row.contains(",\"D1\",") && row.contains(",44.000,20.000,")),
            "placed positions appear in the file"
        );
        assert_eq!(
            session.lock().map_err(|_| "session unavailable")?.inspect(),
            before,
            "the export must not edit the design"
        );
        Ok(())
    }
    #[test]
    fn library_reads_do_not_lock_or_mutate_the_shared_design() -> Result<(), String> {
        let session = shared(Session::default());
        let locked = session.lock().map_err(|_| "session unavailable")?;
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/device.kicad_sym");
        let result = call_tool(
            &session,
            "library.inspect_symbol",
            &json!({"path":path,"name":"R"}),
        );
        assert_eq!(result.get("isError"), Some(&json!(false)));
        assert_eq!(
            result.pointer("/structuredContent/definition/name"),
            Some(&json!("R"))
        );
        assert_eq!(locked.revision, 0);
        assert!(locked.project.components.is_empty());
        assert_eq!(
            call_tool(
                &session,
                "library.search",
                &json!({"path":path,"kind":"symbol","limit":101})
            )
            .get("isError"),
            Some(&json!(true))
        );
        Ok(())
    }
    #[test]
    fn library_parts_can_be_compared_side_by_side_without_editing_history() -> Result<(), String> {
        let session = shared(Session::default());
        let device = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/device.kicad_sym");
        let result = call_tool(
            &session,
            "library.compare",
            &json!({"path_a":device,"path_b":device,"kind":"symbol","name_a":"R","name_b":"R"}),
        );
        assert_eq!(result.get("isError"), Some(&json!(false)));
        assert_eq!(
            result.pointer("/structuredContent/shared_pins"),
            Some(&json!(2))
        );
        assert_eq!(
            result.pointer("/structuredContent/pins_only_in_a"),
            Some(&json!([]))
        );
        let resistor = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/R_0805_2012Metric.kicad_mod");
        let chip = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/DIP-8_W7.62mm.kicad_mod");
        let result = call_tool(
            &session,
            "library.compare",
            &json!({"path_a":resistor,"path_b":chip,"kind":"footprint"}),
        );
        assert_eq!(result.get("isError"), Some(&json!(false)));
        assert_eq!(
            result.pointer("/structuredContent/shared_pads"),
            Some(&json!(2))
        );
        assert_eq!(
            result
                .pointer("/structuredContent/pads_only_in_b")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(6)
        );
        assert_eq!(
            call_tool(
                &session,
                "library.compare",
                &json!({"path_a":device,"path_b":chip,"kind":"module"})
            )
            .get("isError"),
            Some(&json!(true))
        );
        let locked = session.lock().map_err(|_| "session unavailable")?;
        assert_eq!(locked.revision, 0);
        assert!(locked.project.components.is_empty());
        Ok(())
    }
    #[test]
    fn schemas_reject_bad_arguments_before_mutation() {
        let session = shared(Session::default());
        for args in [
            json!({"command":"net.create","params":{"name":"A"},"expected_revision":-1}),
            json!({"command":"net.create","params":{"name":"A"},"extra":true}),
        ] {
            assert_eq!(
                call_tool(&session, "execute", &args).get("isError"),
                Some(&json!(true))
            );
        }
        assert_eq!(
            call_tool(&session, "inspect_design", &json!({}))
                .pointer("/structuredContent/revision"),
            Some(&json!(0))
        );
    }
    #[test]
    fn protocol_negotiates_and_notifications_do_not_reply() -> Result<(), String> {
        let hub = DocumentHub::new(Session::default(), None)?;
        let response = handle_rpc(
            &hub,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
            None,
        );
        assert_eq!(
            response
                .and_then(|r| r.get("result").cloned())
                .and_then(|r| r.get("protocolVersion").cloned()),
            Some(json!("2025-06-18"))
        );
        assert!(
            handle_rpc(
                &hub,
                json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                None
            )
            .is_none()
        );
        Ok(())
    }
    #[test]
    fn tool_errors_are_recoverable_and_stdout_is_json_lines() -> Result<(), String> {
        let hub = DocumentHub::new(Session::default(), None)?;
        let mut output = vec![];
        let initialize = json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}});
        let edit = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"component.move","arguments":{"id":999,"position":{"x":0,"y":0}}}});
        let inspect = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"inspect_design"}});
        let messages = format!("{initialize}\nbroken\n{edit}\n{inspect}\n");
        assert!(
            serve(
                &mut BufReader::new(messages.as_bytes()),
                &mut output,
                &hub,
                None
            )
            .is_ok()
        );
        let text = String::from_utf8(output).unwrap_or_default();
        let values: Vec<Value> = text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        assert_eq!(values.len(), 4);
        assert_eq!(
            values.get(2).and_then(|v| v.pointer("/result/isError")),
            Some(&json!(true))
        );
        assert_eq!(
            values.get(3).and_then(|v| v.pointer("/result/isError")),
            Some(&json!(false))
        );
        Ok(())
    }
    #[test]
    fn design_list_open_reports_two_slots_after_second_open() -> Result<(), String> {
        let hub = DocumentHub::new(Session::default(), None)?;
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let first = directory.path().join("first.pcbcraft");
        let second = directory.path().join("second.pcbcraft");
        pcbcraft_io::save(&Session::default().project, &first)?;
        let mut other = Session::default();
        other
            .execute("net.create", json!({"name":"SECOND"}), None)
            .map_err(|e| e.to_string())?;
        pcbcraft_io::save(&other.project, &second)?;
        assert_eq!(
            hub.call_tool("open", &json!({"path":first.to_string_lossy()}))
                .get("isError"),
            Some(&json!(false))
        );
        assert_eq!(
            hub.call_tool("open", &json!({"path":second.to_string_lossy()}))
                .get("isError"),
            Some(&json!(false))
        );
        let listed = hub.call_tool("design.list_open", &json!({}));
        assert_eq!(listed.get("isError"), Some(&json!(false)), "{listed}");
        assert_eq!(
            listed
                .pointer("/structuredContent/documents")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        Ok(())
    }
    #[test]
    fn design_activate_by_path_routes_execute_to_that_session() -> Result<(), String> {
        let hub = DocumentHub::new(Session::default(), None)?;
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let first = directory.path().join("a.pcbcraft");
        let second = directory.path().join("b.pcbcraft");
        pcbcraft_io::save(&Session::default().project, &first)?;
        let mut touched = Session::default();
        touched
            .execute("net.create", json!({"name":"B"}), None)
            .map_err(|e| e.to_string())?;
        pcbcraft_io::save(&touched.project, &second)?;
        assert_eq!(
            hub.call_tool("open", &json!({"path":first.to_string_lossy()}))
                .get("isError"),
            Some(&json!(false))
        );
        assert_eq!(
            hub.call_tool("open", &json!({"path":second.to_string_lossy()}))
                .get("isError"),
            Some(&json!(false))
        );
        let rev0_before = hub.session_revision(0)?;
        assert_eq!(
            hub.call_tool("design.activate", &json!({"path":second.to_string_lossy()}))
                .get("isError"),
            Some(&json!(false))
        );
        assert_eq!(hub.active_index()?, 1);
        let edit = hub.call_tool(
            "execute",
            &json!({"command":"net.create","params":{"name":"ACTIVE"}}),
        );
        assert_eq!(edit.get("isError"), Some(&json!(false)), "{edit}");
        assert_eq!(hub.session_revision(0)?, rev0_before);
        assert!(hub.session_revision(1)? > rev0_before);
        Ok(())
    }
    #[test]
    fn design_activate_rejects_out_of_range_index() -> Result<(), String> {
        let hub = DocumentHub::new(Session::default(), None)?;
        let before = hub.active_index()?;
        let bad = hub.call_tool("design.activate", &json!({"index":99}));
        assert_eq!(bad.get("isError"), Some(&json!(true)));
        assert_eq!(hub.active_index()?, before);
        Ok(())
    }
    #[test]
    fn inspect_design_includes_active_path_and_name_after_activation() -> Result<(), String> {
        let hub = DocumentHub::new(Session::default(), None)?;
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = directory.path().join("named.pcbcraft");
        let mut session = Session::default();
        session.project.name = "Bench supply".into();
        pcbcraft_io::save(&session.project, &path)?;
        assert_eq!(
            hub.call_tool("open", &json!({"path":path.to_string_lossy()}))
                .get("isError"),
            Some(&json!(false))
        );
        let inspect = hub.call_tool("inspect_design", &json!({}));
        assert_eq!(inspect.get("isError"), Some(&json!(false)), "{inspect}");
        assert_eq!(
            inspect.pointer("/structuredContent/name"),
            Some(&json!("Bench supply"))
        );
        assert_eq!(
            inspect.pointer("/structuredContent/active_index"),
            Some(&json!(0))
        );
        assert!(
            inspect
                .pointer("/structuredContent/path")
                .and_then(Value::as_str)
                .is_some_and(|p| !p.is_empty())
        );
        Ok(())
    }
}
