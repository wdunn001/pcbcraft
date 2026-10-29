//! Golden outputs for every M6 artifact kind (#472, extends #293).
//!
//! Bless with `PCBCRAFT_BLESS=1 cargo test -p pcbcraft-engine --test m6_artifact_goldens`
//! or `cargo xtask bless-parity`. Never bless in CI.

use pcbcraft_doc::{Artifact, ARTIFACT_KINDS, Id, Project};
use pcbcraft_engine::{Command, Session};
use pcbcraft_io::{generate, GeneratedOutput};
use serde_json::{json, Value};
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
                let body = map.get(name).map(String::as_str).unwrap_or("");
                out.push_str(&format!("=== {name} ===\n{body}"));
                if !body.ends_with('\n') {
                    out.push('\n');
                }
            }
            out
        }
    }
}

fn line_diff(expected: &str, actual: &str) -> String {
    let e: Vec<&str> = expected.lines().collect();
    let a: Vec<&str> = actual.lines().collect();
    let mut out = String::new();
    let mut shown = 0;
    for i in 0..e.len().max(a.len()) {
        let (x, y) = (e.get(i), a.get(i));
        if x != y {
            out.push_str(&format!(
                "  line {}:\n    golden: {}\n    actual: {}\n",
                i + 1,
                x.unwrap_or(&"<missing>"),
                y.unwrap_or(&"<missing>")
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

fn compare_golden(
    kind: &str,
    actual: &str,
    failures: &mut Vec<String>,
    skip_notes: &mut Vec<String>,
) -> Result<(), String> {
    let path = golden_dir().join(format!("{kind}.golden"));
    if blessing() {
        let _ = std::fs::create_dir_all(golden_dir());
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
            "skip golden diff for {kind}: tests/fixtures/artifacts/{kind}.golden not blessed yet (run PCBCRAFT_BLESS=1 on a build host)"
        )),
    }
    Ok(())
}

fn owner_issue(kind: &str) -> &'static str {
    match kind {
        "copper_plot" | "mask_plot" | "silk_plot" | "paste_plot" | "profile_plot" | "drill_map"
        | "stencil" => "#325",
        "drill_plated" | "drill_unplated" => "#276",
        "placement" | "bom" => "#326",
        "assembly_drawing" | "fab_drawing" => "#327",
        "raster_image" => "#328",
        "isolation_gcode" => "#329",
        "drill_gcode" => "#330",
        "outline_gcode" => "#331",
        "laser_path" => "#358",
        "package" => "#286",
        _ => "#322",
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
        "stencil" => ("stencil.svg", vec![], json!({})),
        "laser_path" => ("laser.job", vec![], json!({})),
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
    entries.sort_by_key(|v| {
        v.get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
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
    let mut session = reference_board_session()?;
    let seeded = seed_artifacts(&mut session)?;
    let mut failures = Vec::new();
    let mut skip_notes = Vec::new();

    for (kind, id) in seeded {
        let issue = owner_issue(&kind);
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

    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    Ok(())
}

fn artifact_by_id(project: &Project, id: Id) -> Result<&Artifact, String> {
    project
        .artifacts
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| format!("artifact id {id} missing"))
}

#[test]
fn reference_board_generates_all_m6_artifacts() -> Result<(), String> {
    run_reference_board_m6_artifact_goldens()
}
