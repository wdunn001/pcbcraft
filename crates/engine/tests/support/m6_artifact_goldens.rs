//! Golden outputs for every M6 artifact kind (#301, extends #293).
//!
//! Bless with `PCBCRAFT_BLESS=1 cargo test -p pcbcraft-engine --test m6_artifact_goldens`
//! or `cargo xtask bless-parity`. Never bless in CI.

use pcbcraft_doc::{Artifact, ARTIFACT_KINDS, Id, Project};
use pcbcraft_engine::{Command, Session};
use pcbcraft_io::{generate, GeneratedOutput};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden_dir() -> PathBuf {
    repo_root().join("tests/fixtures/artifacts")
}

fn blessing() -> bool {
    std::env::var_os("PCBCRAFT_BLESS").is_some() && std::env::var_os("CI").is_none()
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}

fn binary_summary(bytes: &[u8]) -> String {
    format!("bytes: {}\nfnv1a64: {:016x}\n", bytes.len(), fnv1a(bytes))
}

fn generated_to_golden(output: &GeneratedOutput) -> String {
    match output {
        GeneratedOutput::Text(text) => {
            let mut out = text.clone();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out
        }
        GeneratedOutput::Bytes(bytes) => binary_summary(bytes),
        GeneratedOutput::Files(map) => {
            let mut names: Vec<&String> = map.keys().collect();
            names.sort();
            let mut out = String::new();
            for name in names {
                let body = match map.get(name).map(String::as_str) {
                    Some(text) => text,
                    None => "",
                };
                out.push_str(&format!("=== {name} ===\n{body}"));
                if !body.ends_with('\n') {
                    out.push('\n');
                }
            }
            out
        }
    }
}

pub fn line_diff(expected: &str, actual: &str) -> String {
    let e: Vec<&str> = expected.lines().collect();
    let a: Vec<&str> = actual.lines().collect();
    let mut out = String::new();
    let mut shown = 0;
    for i in 0..e.len().max(a.len()) {
        let (x, y) = (e.get(i), a.get(i));
        if x != y {
            let golden_line = match x {
                Some(line) => line,
                None => "<missing>",
            };
            let actual_line = match y {
                Some(line) => line,
                None => "<missing>",
            };
            out.push_str(&format!(
                "  line {}:\n    golden: {golden_line}\n    actual: {actual_line}\n",
                i + 1,
            ));
            shown += 1;
            if shown == 20 {
                out.push_str("  ... more differences omitted\n");
                break;
            }
        }
    }
    out
}

pub fn compare_golden_in(
    dir: &Path,
    bless: bool,
    kind: &str,
    actual: &str,
    failures: &mut Vec<String>,
    skip_notes: &mut Vec<String>,
) -> Result<(), String> {
    let path = dir.join(format!("{kind}.golden"));
    if bless {
        let _ = std::fs::create_dir_all(dir);
        std::fs::write(&path, actual).map_err(|e| format!("write golden: {e}"))?;
        return Ok(());
    }
    match std::fs::read_to_string(&path) {
        Ok(expected) if expected == actual => {}
        Ok(expected) => failures.push(format!(
            "tests/fixtures/artifacts/{kind}.golden differs:\n{}",
            line_diff(&expected, actual)
        )),
        Err(_) => skip_notes.push(format!(
            "tests/fixtures/artifacts/{kind}.golden not blessed yet (run PCBCRAFT_BLESS=1 on a build host)"
        )),
    }
    Ok(())
}

fn compare_golden(
    kind: &str,
    actual: &str,
    failures: &mut Vec<String>,
    skip_notes: &mut Vec<String>,
) -> Result<(), String> {
    compare_golden_in(
        &golden_dir(),
        blessing(),
        kind,
        actual,
        failures,
        skip_notes,
    )
}

fn kind_owners() -> Result<HashMap<String, String>, String> {
    let path = golden_dir().join("kind-owners.txt");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((kind, issue)) = line.split_once(' ') else {
            return Err(format!("bad kind-owners line: {line}"));
        };
        map.insert(kind.into(), issue.into());
    }
    Ok(map)
}

fn owner_issue(owners: &HashMap<String, String>, kind: &str) -> String {
    match owners.get(kind) {
        Some(issue) => issue.clone(),
        None => "#322".into(),
    }
}

fn seed_spec(kind: &str) -> (String, Vec<String>, Value) {
    let (path, layers, settings): (&str, Vec<&str>, Value) = match kind {
        "copper_plot" => ("pcb-F.Cu.gbr", vec!["F.Cu"], json!({})),
        "mask_plot" => ("pcb-F.Mask.gbr", vec!["F.Mask"], json!({})),
        "silk_plot" => ("pcb-F.SilkS.gbr", vec!["F.SilkS"], json!({})),
        "paste_plot" => ("pcb-F.Paste.gbr", vec!["F.Paste"], json!({})),
        "profile_plot" => ("pcb-Edge.Cuts.gbr", vec!["Edge.Cuts"], json!({})),
        "drill_plated" => ("drill-plated.drl", vec![], json!({})),
        "drill_unplated" => ("drill-unplated.drl", vec![], json!({})),
        "drill_map" => ("board-drill-tools.csv", vec![], json!({})),
        "placement" => ("placement.txt", vec![], json!({})),
        "bom" => ("bom.csv", vec![], json!({})),
        "assembly_drawing" => ("assembly.pdf", vec![], json!({})),
        "fab_drawing" => ("board.pdf", vec![], json!({"view":"pcb"})),
        "raster_image" => ("board.svg", vec![], json!({"view":"pcb"})),
        "isolation_gcode" => ("isolation.nc", vec![], json!({})),
        "drill_gcode" => ("drill.nc", vec![], json!({})),
        "outline_gcode" => ("outline.nc", vec![], json!({})),
        "laser_path" => ("laser.job", vec![], json!({})),
        "stencil" => ("stencil.svg", vec![], json!({})),
        "package" => ("gerber/", vec![], json!({})),
        other => (other, vec![], json!({})),
    };
    (
        path.into(),
        layers.into_iter().map(str::to_string).collect(),
        settings,
    )
}

fn reference_board_session() -> Result<Session, String> {
    let project = pcbcraft_io::load(&repo_root().join("examples/dual-amplifier.pcbcraft"))
        .map_err(|e| format!("load reference board: {e}"))?;
    let mut session = Session::default();
    session
        .replace(project)
        .map_err(|e| format!("replace: {e}"))?;
    Ok(session)
}

fn seed_artifacts(session: &mut Session) -> Result<Vec<(String, Id)>, String> {
    let mut seeded = Vec::new();
    for kind in ARTIFACT_KINDS {
        let (path, layers, settings) = seed_spec(kind);
        let name = format!("m6 golden {kind}");
        let batch = session
            .batch(
                &[Command {
                    command: "artifact.add".into(),
                    params: json!({
                        "name": name,
                        "kind": kind,
                        "layers": layers,
                        "settings": settings,
                        "path": path,
                    }),
                }],
                None,
            )
            .map_err(|e| format!("artifact.add {kind}: {e}"))?;
        let id = batch
            .get("results")
            .and_then(Value::as_array)
            .and_then(|results| results.first())
            .and_then(|entry| entry.get("id"))
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("artifact.add {kind} missing id in response"))?;
        seeded.push(((*kind).into(), id));
    }
    Ok(seeded)
}

fn payload_to_golden(body: &Value) -> Result<String, String> {
    if body.get("files").is_some() {
        return files_payload_to_golden(body);
    }
    let encoding = body
        .get("encoding")
        .and_then(Value::as_str)
        .ok_or_else(|| "generate payload missing encoding".to_string())?;
    match encoding {
        "text" => {
            let text = body
                .get("payload")
                .and_then(Value::as_str)
                .ok_or_else(|| "text payload missing".to_string())?;
            let mut out = text.to_string();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            Ok(out)
        }
        "base64" => {
            let payload = body
                .get("payload")
                .and_then(Value::as_str)
                .ok_or_else(|| "base64 payload missing".to_string())?;
            let bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                payload.as_bytes(),
            )
            .map_err(|e| format!("base64 decode: {e}"))?;
            Ok(binary_summary(&bytes))
        }
        other => Err(format!("unknown generate encoding {other}")),
    }
}

fn files_payload_to_golden(body: &Value) -> Result<String, String> {
    let files = body
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "files payload missing".to_string())?;
    let mut out = String::new();
    let mut entries: Vec<&Value> = files.iter().collect();
    entries.sort_by_key(|v| match v.get("name").and_then(Value::as_str) {
        Some(name) => name.to_string(),
        None => String::new(),
    });
    for entry in entries {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "file entry missing name".to_string())?;
        let text = entry
            .get("payload")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("file {name} missing payload"))?;
        out.push_str(&format!("=== {name} ===\n{text}"));
        if !text.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

pub fn run_reference_board_m6_artifact_goldens() -> Result<(), String> {
    let owners = kind_owners()?;
    let mut session = reference_board_session()?;
    let seeded = seed_artifacts(&mut session)?;
    let mut failures = Vec::new();
    let mut skip_notes = Vec::new();

    for (kind, id) in seeded {
        let issue = owner_issue(&owners, &kind);
        match generate(&session.project, artifact_by_id(&session.project, id)?) {
            Ok(output) => {
                let via_io = generated_to_golden(&output);
                let query = session
                    .query(
                        "artifact.generate",
                        json!({"id": id}),
                        Some(session.revision),
                    )
                    .map_err(|e| format!("artifact.generate {kind}: {e}"))?;
                let body = query
                    .get("result")
                    .ok_or_else(|| format!("artifact.generate {kind} missing result"))?;
                let via_cmd = payload_to_golden(body)?;
                if via_io != via_cmd {
                    failures.push(format!(
                        "{kind}: io generate and artifact.generate disagree"
                    ));
                }
                compare_golden(&kind, &via_io, &mut failures, &mut skip_notes)?;
            }
            Err(message) if message.contains("not available yet") => {
                skip_notes.push(format!(
                    "skip {kind} (owned by issue {issue}): {message}"
                ));
            }
            Err(message) => {
                failures.push(format!("{kind} failed unexpectedly: {message}"));
            }
        }
    }

    for note in &skip_notes {
        eprintln!("{note}");
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

fn artifact_by_id(project: &Project, id: Id) -> Result<&Artifact, String> {
    project
        .artifacts
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| format!("artifact id {id} missing"))
}
