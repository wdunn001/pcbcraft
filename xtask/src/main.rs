//! Repository automation. Every CI job calls one of these subcommands, so a
//! failing job can be reproduced locally with the same command.
//! See docs/ci.md.
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
  docs         rustdoc with warnings denied
  deny         cargo-deny licenses, advisories, bans and sources
  msrv         check the workspace with the declared rust-version toolchain
  bless-parity regenerate the reference-project parity goldens
  screenshots  [--out DIR] [--bin PATH] capture the desktop UI views as PNG files";

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
const VIEWS: [(&str, &[&str]); 8] = [
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
        "08-dual-amplifier-pcb",
        &[
            "--project",
            "examples/dual-amplifier.pcbcraft",
            "--tab",
            "pcb",
        ],
    ),
];

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

fn run_env(program: &str, args: &[&str], envs: &[(&str, &str)]) -> Result<(), String> {
    if status(program, args, envs)?.success() {
        Ok(())
    } else {
        Err(format!("{program} {} failed", args.join(" ")))
    }
}

fn cargo(args: &[&str]) -> Result<(), String> {
    run_env("cargo", args, &[])
}

fn has_cargo_subcommand(name: &str) -> bool {
    Command::new("cargo")
        .args([name, "--version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn fmt() -> Result<(), String> {
    cargo(&["fmt", "--all", "--", "--check"])
}

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

fn wasm() -> Result<(), String> {
    let mut args = vec!["check", "--locked", "--target", "wasm32-unknown-unknown"];
    for package in WASM_PACKAGES {
        args.extend(["-p", package]);
    }
    cargo(&args)
}

fn docs() -> Result<(), String> {
    run_env(
        "cargo",
        &["doc", "--workspace", "--no-deps", "--locked"],
        &[("RUSTDOCFLAGS", "-D warnings")],
    )
}

fn deny() -> Result<(), String> {
    if !has_cargo_subcommand("deny") {
        return Err("cargo-deny is not installed: cargo install --locked cargo-deny".into());
    }
    cargo(&["deny", "--workspace", "--all-features", "check"])
}

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

fn layers() -> Result<(), String> {
    let metadata = workspace_metadata()?;
    let layer = |name: &str| -> Option<u8> {
        match name {
            "pcbcraft-geom" | "pcbcraft-calc" => Some(0),
            "pcbcraft-doc" | "pcbcraft-library" => Some(1),
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

fn root() -> Result<PathBuf, String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?;
    Path::new(&manifest_dir)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask has no parent directory".into())
}

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
        }
        "policy" => {
            layers()?;
            lint_policy()?;
        }
        "layers" => layers()?,
        "fmt" => fmt()?,
        "clippy" => clippy()?,
        "test" => test()?,
        "wasm" => wasm()?,
        "docs" => docs()?,
        "deny" => deny()?,
        "msrv" => msrv()?,
        "screenshots" => screenshots(args.get(1..).unwrap_or(&[]))?,
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

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
