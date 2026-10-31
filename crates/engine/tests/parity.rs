//! Reference-project parity suite (#293). M6 artifact goldens live in
//! `m6_artifact_goldens.rs` (`reference_board_generates_all_m6_artifacts`, #301).
//!
//! Each case builds or loads a project, then snapshots stable outputs and
//! compares them with goldens under `tests/parity/<case>/`. Text outputs are
//! stored verbatim; binary outputs (PNG) are stored as a length plus FNV-1a
//! checksum summary.
//!
//! Bless goldens on purpose with `PCBCRAFT_BLESS=1 cargo test -p pcbcraft-engine
//! --test parity` or `cargo xtask bless-parity`. Never bless in CI.
//!
//! Not covered yet: Gerber and drill output. The Gerber writer is being fixed
//! (#351), so it is not frozen as golden truth. Only structural properties of
//! the stable KiCad export are asserted until the new writers land.

use pcbcraft_doc::Project;
use pcbcraft_engine::{Command, Session};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn golden_dir(case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/parity")
        .join(case)
}
fn blessing() -> bool {
    pcbcraft_testkit::blessing()
}

/// 64-bit FNV-1a, enough to detect any change to a binary output.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn binary_summary(bytes: &[u8]) -> String {
    format!("bytes: {}\nfnv1a64: {:016x}\n", bytes.len(), fnv1a(bytes))
}

fn round(value: &Value) -> Value {
    match value {
        Value::Number(n) if n.is_f64() => {
            let r = (n.as_f64().unwrap_or(0.0) * 10000.0).round() / 10000.0;
            json!(r)
        }
        Value::Array(a) => Value::Array(a.iter().map(round).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), round(v))).collect()),
        other => other.clone(),
    }
}
fn sorted(value: &Value) -> Value {
    match value {
        Value::Array(a) => {
            let mut items: Vec<Value> = a.iter().map(sorted).collect();
            items.sort_by_key(|v| v.to_string());
            Value::Array(items)
        }
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), sorted(v))).collect()),
        other => other.clone(),
    }
}
/// Normalised checks: sorted findings, rounded numbers, no prose scope text.
fn normalised_checks(session: &Session) -> String {
    let mut checks = session.checks();
    if let Some(o) = checks.as_object_mut() {
        o.remove("scope");
    }
    let mut text = serde_json::to_string_pretty(&sorted(&round(&checks))).unwrap_or_default();
    text.push('\n');
    text
}

/// Lines longer than this are shown as a window around the first difference.
const LONG_LINE: usize = 200;
const WINDOW: usize = 120;

/// One side of a differing line. Short lines print whole; long lines (the SVG
/// goldens are a single 60 KB line) print about `WINDOW` characters around the
/// first character where the two sides differ.
fn excerpt(line: &str, other: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= LONG_LINE && other.chars().count() <= LONG_LINE {
        return line.to_string();
    }
    let first = chars
        .iter()
        .zip(other.chars())
        .position(|(a, b)| *a != b)
        .unwrap_or_else(|| chars.len().min(other.chars().count()));
    let start = first.saturating_sub(WINDOW / 2).min(chars.len());
    let end = (start + WINDOW).min(chars.len());
    let body: String = chars[start..end].iter().collect();
    format!(
        "{}{body}{} (column {})",
        if start > 0 { "..." } else { "" },
        if end < chars.len() { "..." } else { "" },
        first + 1
    )
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
                x.map_or("<missing>".to_string(), |x| excerpt(x, y.unwrap_or(&""))),
                y.map_or("<missing>".to_string(), |y| excerpt(y, x.unwrap_or(&"")))
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

fn compare(case: &str, name: &str, actual: &str, failures: &mut Vec<String>) -> Result<(), String> {
    compare_in(&golden_dir(case), blessing(), case, name, actual, failures)
}

/// The comparison itself, with the golden folder and bless switch passed in so
/// the harness can be tested against a temporary folder.
fn compare_in(
    dir: &Path,
    bless: bool,
    case: &str,
    name: &str,
    actual: &str,
    failures: &mut Vec<String>,
) -> Result<(), String> {
    let path = dir.join(name);
    if bless {
        let _ = std::fs::create_dir_all(dir);
        std::fs::write(&path, actual).map_err(|e| format!("write golden: {e}"))?;
        return Ok(());
    }
    match std::fs::read_to_string(&path) {
        Ok(expected) if expected == actual => {}
        Ok(expected) => failures.push(format!(
            "{case}/{name} differs:\n{}",
            line_diff(&expected, actual)
        )),
        Err(_) => failures.push(format!(
            "{case}/{name} has no golden (run with PCBCRAFT_BLESS=1)"
        )),
    }
    Ok(())
}

fn run_script(session: &mut Session, file: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(root().join("examples").join(file))
        .map_err(|e| format!("script: {e}"))?;
    let script: Value = serde_json::from_str(&text).map_err(|e| format!("script json: {e}"))?;
    let commands: Vec<Command> =
        serde_json::from_value(script["commands"].clone()).map_err(|e| format!("commands: {e}"))?;
    session
        .batch(&commands, None)
        .map_err(|e| format!("batch: {e}"))
}

fn load_example(file: &str) -> Result<Session, String> {
    let project = pcbcraft_io::load(&root().join("examples").join(file))
        .map_err(|e| format!("load example: {e}"))?;
    let mut session = Session::default();
    session
        .replace(project)
        .map_err(|e| format!("replace: {e}"))?;
    Ok(session)
}

/// Every stable text output of a session, by file name.
fn outputs(session: &Session) -> Result<Vec<(String, String)>, String> {
    let p = &session.project;
    let project_json =
        serde_json::to_string_pretty(p).map_err(|e| format!("project json: {e}"))? + "\n";
    let png = |view: &str| -> Result<String, String> {
        let bytes = pcbcraft_io::preview_png(p, view).map_err(|e| format!("png: {e}"))?;
        Ok(binary_summary(&bytes))
    };
    Ok(vec![
        ("project.json".into(), project_json),
        ("checks.json".into(), normalised_checks(session)),
        (
            "board.kicad_pcb".into(),
            pcbcraft_io::kicad_board(p).map_err(|e| format!("kicad: {e}"))?,
        ),
        (
            "pcb.svg".into(),
            pcbcraft_io::svg(p, "pcb").map_err(|e| format!("pcb svg: {e}"))?,
        ),
        (
            "schematic.svg".into(),
            pcbcraft_io::svg(p, "schematic").map_err(|e| format!("schematic svg: {e}"))?,
        ),
        ("pcb.png.summary".into(), png("pcb")?),
        ("schematic.png.summary".into(), png("schematic")?),
    ])
}

fn check_case(case: &str, session: &Session, extra: Vec<(String, String)>) -> Result<(), String> {
    let mut failures = vec![];
    for (name, text) in outputs(session)?.into_iter().chain(extra) {
        compare(case, &name, &text, &mut failures)?;
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    Ok(())
}

fn led_routed() -> Result<(Session, Value, Value), String> {
    let mut session = Session::default();
    let build = run_script(&mut session, "led-indicator.json")?;
    let route = run_script(&mut session, "route-led.json")?;
    Ok((session, build, route))
}

#[test]
fn parity_led_indicator_matches_goldens() -> Result<(), String> {
    let (session, build, route) = led_routed()?;
    let findings = session.checks();
    // docs/validation.md records zero findings and zero airwires once routed.
    assert_eq!(findings["drc"], json!([]));
    assert_eq!(findings["airwires"], json!([]));
    let results = format!(
        "{}\n{}\n",
        serde_json::to_string_pretty(&build).unwrap_or_default(),
        serde_json::to_string_pretty(&route).unwrap_or_default()
    );
    check_case(
        "led-indicator",
        &session,
        vec![("command-results.json".into(), results)],
    )
}

#[test]
fn parity_led_indicator_unrouted_reports_airwires() -> Result<(), String> {
    let mut session = Session::default();
    run_script(&mut session, "led-indicator.json")?;
    let airwires = session.checks()["airwires"].as_array().map_or(0, Vec::len);
    assert_eq!(airwires, 3, "docs/validation.md: three disconnected links");
    let mut failures = vec![];
    compare(
        "led-indicator-unrouted",
        "checks.json",
        &normalised_checks(&session),
        &mut failures,
    )?;
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    Ok(())
}

mod support;

#[test]
fn parity_dual_amplifier_matches_goldens() -> Result<(), String> {
    check_case(
        "dual-amplifier",
        &load_example("dual-amplifier.pcbcraft")?,
        vec![],
    )?;
    support::m6_artifact_goldens::run_reference_board_m6_artifact_goldens()
}

#[test]
fn parity_library_rc_matches_goldens() -> Result<(), String> {
    check_case(
        "kicad-library-rc",
        &load_example("kicad-library-rc.pcbcraft")?,
        vec![],
    )
}

#[test]
fn failed_batch_leaves_session_untouched() -> Result<(), String> {
    let (mut session, _, _) = led_routed()?;
    let before = (session.project.clone(), session.revision);
    let bad = [
        Command {
            command: "project.rename".into(),
            params: json!({"name":"half applied"}),
        },
        Command {
            command: "track.add".into(),
            params: json!({"net":4,"start":{"x":0,"y":0},"end":{"x":1e12,"y":0},"width":0.2}),
        },
    ];
    assert!(session.batch(&bad, None).is_err());
    assert_eq!(before, (session.project.clone(), session.revision));
    Ok(())
}

fn all_cases() -> Result<Vec<(&'static str, Session)>, String> {
    Ok(vec![
        ("led-indicator", led_routed()?.0),
        ("dual-amplifier", load_example("dual-amplifier.pcbcraft")?),
        (
            "kicad-library-rc",
            load_example("kicad-library-rc.pcbcraft")?,
        ),
    ])
}

#[test]
fn every_writer_is_deterministic() -> Result<(), String> {
    for (case, session) in all_cases()? {
        let first = outputs(&session)?;
        let again = outputs(&session)?;
        assert_eq!(first, again, "{case}: writer output changed between runs");
    }
    // Two independent builds from a fresh load must match, which catches hash
    // order or counter drift that a clone of one session never could.
    let run_a = all_cases()?;
    let run_b = all_cases()?;
    for ((case, a), (_, b)) in run_a.iter().zip(run_b.iter()) {
        assert_eq!(outputs(a)?, outputs(b)?, "{case}: independent runs differ");
    }
    Ok(())
}

fn diff_failure(golden: &str, actual: &str) -> Result<Vec<String>, String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    std::fs::write(dir.path().join("out.txt"), golden).map_err(|e| format!("write: {e}"))?;
    let mut failures = Vec::new();
    compare_in(dir.path(), false, "case", "out.txt", actual, &mut failures)?;
    Ok(failures)
}

#[test]
fn harness_fails_on_a_wrong_golden_with_a_readable_diff() -> Result<(), String> {
    let failures = diff_failure("one\ntwo\nthree\n", "one\nTWO\nthree\n")?;
    assert_eq!(failures.len(), 1, "{failures:?}");
    let text = &failures[0];
    assert!(text.contains("case/out.txt"), "{text}");
    assert!(text.contains("line 2"), "{text}");
    assert!(text.contains("golden: two"), "{text}");
    assert!(text.contains("actual: TWO"), "{text}");
    assert!(!text.contains("line 1"), "{text}");
    Ok(())
}

#[test]
fn harness_passes_an_identical_golden_and_flags_a_missing_one() -> Result<(), String> {
    assert!(diff_failure("same\n", "same\n")?.is_empty());
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let mut failures = Vec::new();
    compare_in(dir.path(), false, "case", "none.txt", "x", &mut failures)?;
    assert!(failures[0].contains("has no golden"), "{failures:?}");
    Ok(())
}

#[test]
fn harness_shows_a_window_for_a_one_line_golden() -> Result<(), String> {
    let golden = format!("<svg>{}A{}</svg>", "<g/>".repeat(5000), "<g/>".repeat(5000));
    let actual = golden.replacen('A', "B", 1);
    let failures = diff_failure(&golden, &actual)?;
    let text = &failures[0];
    assert!(text.contains("line 1"), "{text}");
    assert!(text.len() < 1000, "diff is {} bytes", text.len());
    assert!(text.contains("A<g/>") && text.contains("B<g/>"), "{text}");
    assert!(text.contains("column 20006"), "{text}");
    Ok(())
}

#[test]
fn harness_blesses_into_the_given_folder() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let mut failures = Vec::new();
    compare_in(
        dir.path(),
        true,
        "case",
        "new.txt",
        "blessed\n",
        &mut failures,
    )?;
    assert!(failures.is_empty());
    let written = std::fs::read_to_string(dir.path().join("new.txt")).map_err(|e| e.to_string())?;
    assert_eq!(written, "blessed\n");
    Ok(())
}

#[test]
fn every_reader_round_trips_its_own_writer() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    for (case, session) in all_cases()? {
        let path = dir.path().join(format!("{case}.pcbcraft"));
        pcbcraft_io::save(&session.project, &path).map_err(|e| format!("save: {e}"))?;
        let loaded: Project = pcbcraft_io::load(&path).map_err(|e| format!("load: {e}"))?;
        assert_eq!(loaded, session.project, "{case}: native round trip");
        // Saving the loaded project is byte-identical.
        let again = dir.path().join(format!("{case}-again.pcbcraft"));
        pcbcraft_io::save(&loaded, &again).map_err(|e| format!("save again: {e}"))?;
        assert_eq!(
            std::fs::read(&path).map_err(|e| format!("read: {e}"))?,
            std::fs::read(&again).map_err(|e| format!("read again: {e}"))?,
            "{case}: save is not idempotent"
        );
    }
    Ok(())
}

/// Structural properties only. Gerber and drill output are pending #351 and
/// must not be frozen as goldens; the limited KiCad export is stable.
#[test]
fn kicad_export_is_structurally_sound() -> Result<(), String> {
    for (case, session) in all_cases()? {
        let text = pcbcraft_io::kicad_board(&session.project).map_err(|e| format!("kicad: {e}"))?;
        assert!(text.starts_with("(kicad_pcb"), "{case}");
        let (mut open, mut close, mut in_string, mut escaped) = (0i64, 0i64, false, false);
        for c in text.chars() {
            if in_string {
                match (escaped, c) {
                    (true, _) => escaped = false,
                    (false, '\\') => escaped = true,
                    (false, '"') => in_string = false,
                    _ => {}
                }
            } else {
                match c {
                    '"' => in_string = true,
                    '(' => open += 1,
                    ')' => close += 1,
                    _ => {}
                }
            }
        }
        assert_eq!(open, close, "{case}: unbalanced s-expression");
        let footprints = text.matches("(footprint ").count();
        assert_eq!(
            footprints,
            session.project.board.footprints.len(),
            "{case}: footprint count"
        );
    }
    Ok(())
}
