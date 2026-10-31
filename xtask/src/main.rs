//! Repository automation. Every CI job calls one of these subcommands, so a
//! failing job can be reproduced locally with the same command.
//!
//! # Rules
//! - Every CI job calls one subcommand; add the subcommand here before the job.
//! - Subcommands print what they run and fail with a non-zero exit code.
//!
//! # Dependencies
//! - .github/workflows/ci.yml
//! - docs/ci.md
//! - xtask/src/coverage.rs
//! - xtask/src/doc_check.rs
//! - xtask/src/tooltips.rs
//! - xtask/src/status_data.rs
//! - xtask/src/i18n.rs
//! - xtask/src/a11y.rs
//!
//! # Do not
//! - Add a CI step that has no `cargo xtask` command behind it.
mod a11y;
mod coverage;
mod doc_check;
mod e2e;
mod i18n;
mod showcase;
mod status_data;
mod status_schema;
mod tooltips;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

const USAGE: &str = "usage: cargo xtask <command>

  ci           policy, fmt, clippy, test, wasm and docs (run before committing)
  policy       dependency layers and lint configuration
  layers       dependency layers only
  fmt          rustfmt check
  clippy       clippy on all targets with warnings denied
  test         workspace tests (cargo-nextest when installed) and doctests
  wasm         WebAssembly check of the shared UI and browser entry point
  tooltips     [--strict] check tooltip text against docs/tooltips.md
  docs         rustdoc with warnings denied
  docs check   documentation standard: paths, trait implementation lists, AGENTS.md, missing-docs ratchet
  deny         cargo-deny licenses, advisories, bans and sources
  msrv         check the workspace with the declared rust-version toolchain
  bless-parity regenerate the reference-project parity goldens
  strings      extract | check: list text catalog keys, or fail on a bad catalog
  a11y         accessibility gate: names, duplicates, target size, keyboard reach, contrast, motion
  i18n         [--write] localization gate: key parity, placeholders, literals, pseudo-locale, fonts
  coverage     [--bless] [--base REF] [--no-run] tests under cargo-llvm-cov, per-crate table, gate and ratchet
  e2e          full a_newcomer suite plus the tooltip and story coverage gate
  showcase     regenerate the sample projects in examples/ from engine commands
tooltips     [--strict] check tooltip text against docs/tooltips.md
  docs         rustdoc with warnings denied
  docs check   documentation standard: paths, trait implementation lists, AGENTS.md, missing-docs ratchet
  deny         cargo-deny licenses, advisories, bans and sources
  msrv         check the workspace with the declared rust-version toolchain
  bless-parity regenerate the reference-project parity goldens
  strings      extract | check: list text catalog keys, or fail on a bad catalog
  a11y         accessibility gate: names, duplicates, target size, keyboard reach, contrast, motion
  i18n         [--write] localization gate: key parity, placeholders, literals, pseudo-locale, fonts
  coverage     [--bless] [--base REF] [--no-run] tests under cargo-llvm-cov, per-crate table, gate and ratchet
  e2e          full a_newcomer suite plus the tooltip and story coverage gate
  showcase     regenerate the sample projects in examples/ from engine commands
  screenshots  [--out DIR] [--bin PATH] capture the desktop UI views as PNG files
  board3d-bench  run the six-hundred-part 3D render regression test
  status-data  [--refresh] write site/src/data/status.json from the cached tracker snapshot

/// Packages built for wasm32. Kept in one place so `ci` and `wasm` agree.
const WASM_PACKAGES: [&str; 2] = ["pcbcraft-ui-egui", "pcbcraft-web"];

/// Lints in the workspace table that AGENTS.md requires. A crate that opts
/// out of `[lints] workspace = true`, or source that allows one of these, fails.
const REQUIRED_DENY: [&str; 6] = [
    "unwrap_used",
    "expect_used",
    "panic",
    "unimplemented",
    "todo",
    "unreachable",
];

/// Desktop views captured for review: (file stem, extra app arguments).
const VIEWS: [(&str, &[&str]); 11] = [
    ("01-schematic-dark", &["--sample", "--tab", "schematic"]),
    (
        "02-schematic-light",
        &["--sample", "--tab", "schematic", "--light"],
    ),
    ("03-pcb", &["--sample", "--tab", "pcb"]),
    ("04-checks", &["--sample", "--tab", "checks"]),
    ("05-libraries", &["--sample", "--tab", "libraries"]),
    ("06-symbol-editor", &["--sample", "--tab", "symbol-editor"]),
    (
        "07-footprint-editor",
        &["--sample", "--tab", "footprint-editor"],
    ),
    (
        "09-showcase-layout",
        &["--project", "examples/showcase.pcbcraft", "--tab", "pcb"],
    ),
    (
        "10-showcase-3d",
        &["--project", "examples/showcase.pcbcraft", "--tab", "3d"],
    ),
    (
        "11-showcase-schematic",
        &[
            "--project",
            "examples/showcase.pcbcraft",
            "--tab",
            "schematic",
        ],
    ),
    (
        "08-dual-amplifier-pcb",
        &[
            "--project",
            "examples/dual-amplifier.pcbcraft",
            "--tab",
            "pcb",
        ],
    ),
];

/// Prints and runs a program, returning its exit status.
fn status(program: &str, args: &[&str], envs: &[(&str, &str)]) -> Result<ExitStatus, String> {
    println!("+ {program} {}", args.join(" "));
    let mut command = Command::new(program);
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .status()
        .map_err(|e| format!("{program} {}: {e}", args.join(" ")))
}

/// Runs a program with extra environment variables and fails on a non-zero exit.
fn run_env(program: &str, args: &[&str], envs: &[(&str, &str)]) -> Result<(), String> {
    if status(program, args, envs)?.success() {
        Ok(())
    } else {
        Err(format!("{program} {} failed", args.join(" ")))
    }
}

/// Runs cargo with the given arguments.
fn cargo(args: &[&str]) -> Result<(), String> {
    run_env("cargo", args, &[])
}

/// True when `cargo <name> --version` succeeds.
fn has_cargo_subcommand(name: &str) -> bool {
    Command::new("cargo")
        .args([name, "--version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Checks formatting of the whole workspace.
fn fmt() -> Result<(), String> {
    cargo(&["fmt", "--all", "--", "--check"])
}

/// Six-hundred-part 3D software render gate (issue 721).
fn board3d_bench() -> Result<(), String> {
    run_env(
        "cargo",
        &[
            "test",
            "-p",
            "pcbcraft-ui-egui",
            "--test",
            "board3d_perf",
            "--",
            "six_hundred_parts_render_stays_within_the_baseline",
            "--nocapture",
        ],
        &[("CARGO_BUILD_JOBS", "4")],
    )
}

/// Runs clippy on all targets with warnings denied.
fn clippy() -> Result<(), String> {
    cargo(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ])
}

/// Runs workspace tests with nextest when installed, plus doctests.
fn test() -> Result<(), String> {
    if has_cargo_subcommand("nextest") {
        cargo(&[
            "nextest",
            "run",
            "--workspace",
            "--locked",
            "--no-fail-fast",
        ])?;
        // nextest does not run doctests.
        cargo(&["test", "--workspace", "--locked", "--doc"])
    } else {
        cargo(&["test", "--workspace", "--locked"])
    }
}

/// Run every `a_newcomer_*` story and walk the whole UI, then fail on any
/// control without a tooltip, a story or an id that is not exempted.
fn e2e() -> Result<(), String> {
    let root = root()?;
    let dir = root.join("target").join("e2e");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let touch = dir.join("touched.log");
    let walk = dir.join("walk.log");
    let _ = std::fs::remove_file(&touch);
    let _ = std::fs::remove_file(&walk);
    // A failing story still leaves its touches in the log, so the gap report
    // below is printed either way and the run fails on both.
    let stories = run_env(
        "cargo",
        &[
            "test",
            "-p",
            "pcbcraft-ui-egui",
            "--locked",
            "--",
            "a_newcomer_",
            "e2e_audit_walk",
        ],
        &[
            ("PCBCRAFT_E2E_TOUCH_LOG", &touch.to_string_lossy()),
            ("PCBCRAFT_E2E_WALK_LOG", &walk.to_string_lossy()),
        ],
    );
    let walk_text = std::fs::read_to_string(&walk)
        .map_err(|e| format!("the UI walk wrote no log ({}): {e}", walk.display()))?;
    let touch_text = std::fs::read_to_string(&touch).unwrap_or_default();
    let exempt_path = root.join("e2e-exemptions.txt");
    let exemptions = match std::fs::read_to_string(&exempt_path) {
        Ok(text) => e2e::Exemptions::parse(&text)?,
        Err(_) => e2e::Exemptions::default(),
    };
    let report = e2e::diff(&walk_text, &touch_text, &exemptions);
    let lines = report.lines();
    let summary = dir.join("report.txt");
    let _ = std::fs::write(&summary, lines.join("\n"));
    println!(
        "e2e gate: {} controls painted, {} gaps (report: {})",
        report.painted,
        lines.len(),
        summary.display()
    );
    for line in &lines {
        eprintln!("{line}");
    }
    stories?;
    if report.is_clean() {
        return Ok(());
    }
    Err(format!(
        "{} controls lack a tooltip, a story or an id; fix them or add a reviewed entry to e2e-exemptions.txt",
        lines.len()
    ))
}

/// Checks the wasm packages for the wasm32 target.
fn wasm() -> Result<(), String> {
    let mut args = vec!["check", "--locked", "--target", "wasm32-unknown-unknown"];
    for package in WASM_PACKAGES {
        args.extend(["-p", package]);
    }
    cargo(&args)
}

/// Builds rustdoc with warnings denied. The documentation standard check is `doc_check::run`.
fn docs() -> Result<(), String> {
    run_env(
        "cargo",
        &["doc", "--workspace", "--no-deps", "--locked"],
        &[("RUSTDOCFLAGS", "-D warnings")],
    )
}

/// Runs cargo-deny for licenses, advisories, bans and sources.
fn deny() -> Result<(), String> {
    if !has_cargo_subcommand("deny") {
        return Err("cargo-deny is not installed: cargo install --locked cargo-deny".into());
    }
    cargo(&["deny", "--workspace", "--all-features", "check"])
}

/// Reads `cargo metadata` for the workspace without dependencies.
fn workspace_metadata() -> Result<serde_json::Value, String> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("cargo metadata failed".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}

fn str_field<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("missing metadata field {key}"))
}

/// Checks that every crate inherits the workspace rust-version and builds with that toolchain.
fn msrv() -> Result<(), String> {
    let metadata = workspace_metadata()?;
    let manifest =
        std::fs::read_to_string(root()?.join("Cargo.toml")).map_err(|e| e.to_string())?;
    let version = manifest
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("rust-version")
                .and_then(|rest| rest.trim().strip_prefix('='))
                .map(|rest| rest.trim().trim_matches('"').to_string())
        })
        .ok_or("workspace Cargo.toml has no rust-version")?;
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or("missing metadata packages")?;
    for package in packages {
        let name = str_field(package, "name")?;
        let declared = package
            .get("rust_version")
            .and_then(serde_json::Value::as_str);
        if name != "xtask" && declared != Some(version.as_str()) {
            return Err(format!(
                "{name} must inherit rust-version {version} from the workspace"
            ));
        }
    }
    let toolchain = format!("+{version}");
    cargo(&[
        &toolchain,
        "check",
        "--workspace",
        "--all-targets",
        "--locked",
    ])
}

/// Fails when a crate depends on a higher layer or a UI crate from below the UI layer.
fn layers() -> Result<(), String> {
    let metadata = workspace_metadata()?;
    let layer = |name: &str| -> Option<u8> {
        match name {
            "pcbcraft-geom" | "pcbcraft-text" | "pcbcraft-calc" => Some(0),
            "pcbcraft-doc" | "pcbcraft-library" | "pcbcraft-sim" => Some(1),
            "pcbcraft-rules" | "pcbcraft-io" => Some(2),
            "pcbcraft-engine" => Some(3),
            "pcbcraft-mcp" | "pcbcraft-ui-egui" => Some(4),
            "pcbcraft" | "pcbcraft-cli" | "pcbcraft-web" => Some(5),
            _ => None,
        }
    };
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or("missing metadata packages")?;
    for package in packages {
        let name = str_field(package, "name")?;
        let Some(level) = layer(name) else {
            continue;
        };
        for dependency in package
            .get("dependencies")
            .and_then(serde_json::Value::as_array)
            .ok_or("missing dependencies")?
        {
            let dep = str_field(dependency, "name")?;
            if layer(dep).is_some_and(|target| target > level)
                || (level < 4
                    && ["egui", "eframe", "wgpu", "winit", "rfd", "craft-ui"].contains(&dep))
            {
                return Err(format!("layer violation: {name} depends on {dep}"));
            }
        }
    }
    println!("dependency layers passed");
    Ok(())
}

/// The repository root, one level above the xtask manifest directory.
fn root() -> Result<PathBuf, String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?;
    Path::new(&manifest_dir)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask has no parent directory".into())
}

/// Collects every `.rs` file under a directory, skipping `target`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// AGENTS.md forbids unsafe and production unwrap/expect/panic/todo/
/// unimplemented/unreachable. The workspace lint table enforces that through
/// clippy; this check makes sure nothing quietly opts out of it.
fn lint_policy() -> Result<(), String> {
    let root = root()?;
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| e.to_string())?;
    if !manifest.contains("unsafe_code = \"forbid\"") {
        return Err("workspace lints must keep unsafe_code = \"forbid\"".into());
    }
    for lint in REQUIRED_DENY {
        if !manifest.contains(&format!("{lint} = \"deny\"")) {
            return Err(format!(
                "workspace clippy lints must keep {lint} = \"deny\""
            ));
        }
    }
    let metadata = workspace_metadata()?;
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or("missing metadata packages")?;
    let mut problems = Vec::new();
    for package in packages {
        let name = str_field(package, "name")?;
        let path = str_field(package, "manifest_path")?;
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let inherits = text
            .split("[lints]")
            .nth(1)
            .is_some_and(|rest| rest.trim_start().starts_with("workspace = true"));
        if !inherits {
            problems.push(format!("{name}: Cargo.toml needs [lints] workspace = true"));
        }
    }
    let mut files = Vec::new();
    for dir in ["crates", "apps", "xtask"] {
        rust_files(&root.join(dir), &mut files)?;
    }
    for file in files {
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        for (number, line) in text.lines().enumerate() {
            let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            let attribute = compact.starts_with("#[") || compact.starts_with("#![");
            let opts_out = attribute
                && (compact.contains("allow(") || compact.contains("expect("))
                && (compact.contains("unsafe_code")
                    || REQUIRED_DENY
                        .iter()
                        .any(|lint| compact.contains(&format!("clippy::{lint}"))));
            if opts_out {
                problems.push(format!(
                    "{}:{}: lint opt-out is not allowed: {}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    if problems.is_empty() {
        println!("lint policy passed");
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// The value after a `--key` argument, if present.
fn option_value(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Run the desktop app once per view with `--capture`. Needs a display; in CI
/// this runs under Xvfb with Mesa's software Vulkan driver.
fn screenshots(args: &[String]) -> Result<(), String> {
    let root = root()?;
    let out = option_value(args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target").join("screenshots"));
    let out = if out.is_absolute() {
        out
    } else {
        root.join(out)
    };
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let bin = match option_value(args, "--bin") {
        Some(bin) => PathBuf::from(bin),
        None => {
            cargo(&["build", "--locked", "-p", "pcbcraft"])?;
            let target = std::env::var("CARGO_TARGET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| root.join("target"));
            target.join("debug").join("pcbcraft")
        }
    };
    let mut failed = Vec::new();
    for (stem, view_args) in VIEWS {
        let png = out.join(format!("{stem}.png"));
        let _ = std::fs::remove_file(&png);
        let png_arg = png.to_string_lossy().into_owned();
        let mut full: Vec<&str> = view_args.to_vec();
        full.extend(["--capture", &png_arg]);
        println!("+ {} {}", bin.display(), full.join(" "));
        let mut child = Command::new(&bin)
            .args(&full)
            .current_dir(&root)
            .spawn()
            .map_err(|e| format!("{}: {e}", bin.display()))?;
        let started = Instant::now();
        let finished = loop {
            match child.try_wait().map_err(|e| e.to_string())? {
                Some(status) => break Some(status),
                None if started.elapsed() > Duration::from_secs(90) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                None => std::thread::sleep(Duration::from_millis(200)),
            }
        };
        match finished {
            Some(status) if status.success() && png.is_file() => {
                println!("captured {}", png.display());
            }
            Some(status) => failed.push(format!("{stem}: exited with {status} and no image")),
            None => failed.push(format!("{stem}: timed out after 90 s")),
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("\n"))
    }
}

/// Dispatches the subcommand named on the command line; `ci` is the default.
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let action = args.first().map(String::as_str).unwrap_or("ci");
    match action {
        "ci" => {
            layers()?;
            lint_policy()?;
            fmt()?;
            clippy()?;
            test()?;
            wasm()?;
            docs()?;
            doc_check::run(&[])?;
        }
        "policy" => {
            layers()?;
            lint_policy()?;
        }
        "layers" => layers()?,
        "fmt" => fmt()?,
        "clippy" => clippy()?,
        "test" => test()?,
        "coverage" => coverage::run(&root()?, args.get(1..).unwrap_or(&[]))?,
        "e2e" => e2e()?,
        "a11y" => a11y_gate()?,
        "i18n" => i18n_gate(args.get(1..).unwrap_or(&[]))?,
        "wasm" => wasm()?,
        "docs" if args.get(1).map(String::as_str) == Some("check") => {
            doc_check::run(args.get(2..).unwrap_or(&[]))?;
        }
        "docs" => docs()?,
        "tooltips" => tooltips::run(args.get(1..).unwrap_or(&[]))?,
        "deny" => deny()?,
        "msrv" => msrv()?,
        "strings" => strings(args.get(1).map(String::as_str))?,
        "showcase" => showcase::run(&root()?)?,
        "status-data" => status_data::run(args.get(1..).unwrap_or(&[]), &root()?)?,
        "screenshots" => screenshots(args.get(1..).unwrap_or(&[]))?,
        "board3d-bench" => board3d_bench()?,
        "bless-parity" => run_env(
            "cargo",
            &["test", "-p", "pcbcraft-engine", "--test", "parity"],
            &[("PCBCRAFT_BLESS", "1")],
        )?,
        "help" | "--help" | "-h" => println!("{USAGE}"),
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

/// `a11y` runs the accessibility tests of ui-egui and prints the per-screen table.
fn a11y_gate() -> Result<(), String> {
    let dir = root()?.join("target").join("a11y");
    let _ = std::fs::remove_file(dir.join("report.json"));
    let mut args = vec!["test", "-p", "pcbcraft-ui-egui", "--lib", "--"];
    args.extend(a11y::FILTERS);
    let result = run_env(
        "cargo",
        &args,
        &[("PCBCRAFT_A11Y_REPORT", &dir.to_string_lossy())],
    );
    if let Ok(report) = std::fs::read_to_string(dir.join("report.json")) {
        let table = a11y::table(&report)?;
        println!("{table}");
        if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") {
            let _ = std::fs::write(&path, format!("### Accessibility by screen\n\n{table}\n"));
        }
    }
    result
}

/// `i18n` runs the text checks of `i18n.rs`, then the pseudo-locale and font tests in ui-egui.
fn i18n_gate(args: &[String]) -> Result<(), String> {
    i18n::run(&root()?, args)?;
    if args.iter().any(|a| a == "--write") {
        return Ok(());
    }
    let report = root()?.join("target").join("i18n");
    run_env(
        "cargo",
        &[
            "test",
            "-p",
            "pcbcraft-ui-egui",
            "--lib",
            "--",
            "e2e::i18n::",
        ],
        &[("PCBCRAFT_I18N_REPORT", &report.to_string_lossy())],
    )
}

/// `strings extract` prints every catalog key with its English text.
/// `strings check` fails on a malformed or duplicated key in en.txt, a
/// translation file with unknown keys or changed placeholders, a `t!("key")`
/// in ui-egui that en.txt lacks, or a string literal inside a function that
/// strings/converted-fns.txt lists as converted.
fn strings(mode: Option<&str>) -> Result<(), String> {
    use pcbcraft_text::{ENGLISH, parse, placeholders, scan};
    let (english, errors) = parse(ENGLISH);
    match mode {
        Some("extract") => {
            for (key, value) in &english {
                println!("{key} = {value}");
            }
            Ok(())
        }
        Some("check") => {
            let mut problems: Vec<String> = errors.iter().map(|e| format!("en.txt {e}")).collect();
            let mut seen = std::collections::BTreeMap::new();
            for (key, value) in &english {
                if seen.insert(key.clone(), value.clone()).is_some() {
                    problems.push(format!("en.txt: duplicate key `{key}`"));
                }
            }
            let strings_dir = root()?.join("crates/text/strings");
            let entries = std::fs::read_dir(&strings_dir).map_err(|e| e.to_string())?;
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == "en.txt"
                    || !name.ends_with(".txt")
                    || name == "converted-fns.txt"
                    || name == "plural-known.txt"
                    || name == "i18n-ratchet.txt"
                {
                    continue;
                }
                let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                let (lines, errors) = parse(&text);
                problems.extend(errors.iter().map(|e| format!("{name} {e}")));
                for (key, value) in lines {
                    match seen.get(&key) {
                        None => problems.push(format!("{name}: key `{key}` is not in en.txt")),
                        Some(base) if placeholders(base) != placeholders(&value) => {
                            problems.push(format!("{name}: `{key}` changes the placeholders"));
                        }
                        Some(_) => {}
                    }
                }
            }
            let mut files = Vec::new();
            let src = root()?.join("crates/ui-egui/src");
            collect_rs(&src, &src, &mut files)?;
            problems.extend(scan::problems(&files));
            if problems.is_empty() {
                println!("strings check: {} keys, catalog clean", english.len());
                Ok(())
            } else {
                Err(problems.join("\n"))
            }
        }
        _ => Err("usage: cargo xtask strings extract|check".into()),
    }
}

fn collect_rs(base: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(base, &path, out)?;
        } else if path.extension().is_some_and(|x| x == "rs") {
            let name = path
                .strip_prefix(base)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into_owned();
            out.push((
                name,
                std::fs::read_to_string(&path).map_err(|e| e.to_string())?,
            ));
        }
    }
    Ok(())
}

/// Runs the chosen subcommand and exits non-zero with the message on failure.
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
