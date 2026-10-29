use pcbcraft_engine::{Command, Session};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn option<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn flag(args: &[String], key: &str) -> bool {
    args.iter().any(|a| a == key)
}

fn empty_session(name: Option<&str>) -> Result<Session, String> {
    let mut session = Session::default();
    if let Some(title) = name {
        session
            .execute("project.rename", json!({"name": title}), None)
            .map_err(|e| e.to_string())?;
    }
    Ok(session)
}

fn write_new_project(path: &Path, name: Option<&str>) -> Result<(), String> {
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    let session = empty_session(name)?;
    pcbcraft_io::save(&session.project, path)?;
    Ok(())
}

fn scaffold_build_json(name: Option<&str>) -> Value {
    let mut commands: Vec<Command> = Vec::new();
    if let Some(title) = name {
        commands.push(Command {
            command: "project.rename".into(),
            params: json!({"name": title}),
        });
    }
    json!({"commands": commands})
}

fn init_project_directory(dir: &Path, name: Option<&str>, force: bool) -> Result<PathBuf, String> {
    if dir.exists() {
        if !dir.is_dir() {
            return Err(format!("{} is not a directory", dir.display()));
        }
        if !force {
            let mut entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
            if entries.next().is_some() {
                return Err(format!(
                    "{} is not empty; pass --force to scaffold anyway",
                    dir.display()
                ));
            }
        }
    } else {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let design = dir.join("design.pcbcraft");
    if design.exists() && !force {
        return Err(format!("{} already exists", design.display()));
    }
    write_new_project(&design, name)?;
    let build_path = dir.join("build.json");
    if build_path.exists() && !force {
        return Err(format!("{} already exists", build_path.display()));
    }
    let script = scaffold_build_json(name);
    pcbcraft_io::atomic_write(
        &build_path,
        serde_json::to_string_pretty(&script)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )?;
    Ok(design)
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let action = args.first().map(String::as_str).unwrap_or("help");
    let mut session = if args.iter().any(|a| a == "--sample") {
        Session::sample().map_err(|e| e.to_string())?
    } else {
        Session::default()
    };
    if let Some(path) = option(&args, "--project") {
        session
            .replace(pcbcraft_io::load(Path::new(path))?)
            .map_err(|e| e.to_string())?;
    }
    match action {
        "commands" => println!("{}", json!({"commands":pcbcraft_engine::commands()})),
        "inspect" => println!("{}", session.inspect()),
        "checks" => println!("{}", session.checks()),
        "tool" => {
            let name = args
                .get(1)
                .ok_or("usage: pcbcraft-cli tool NAME --args JSON")?;
            let input: Value = serde_json::from_str(option(&args, "--args").unwrap_or("{}"))
                .map_err(|e| e.to_string())?;
            let shared = pcbcraft_mcp::shared(session);
            let result = pcbcraft_mcp::call_tool(&shared, name, &input);
            if result.get("isError").and_then(Value::as_bool) == Some(true) {
                return Err(result
                    .pointer("/content/0/text")
                    .and_then(Value::as_str)
                    .unwrap_or("tool failed")
                    .into());
            }
            if let Some(path) = option(&args, "--save") {
                let current = shared.lock().map_err(|_| "session unavailable")?;
                pcbcraft_io::save(&current.project, Path::new(path))?;
            }
            println!("{result}");
        }
        "mcp" => {
            let hub = pcbcraft_mcp::DocumentHub::new(
                session,
                option(&args, "--project").map(str::to_string),
            )?;
            if let Some(address) = option(&args, "--http") {
                // The endpoint and token go to stderr (or a 0600 file); stdout stays unused.
                let token_file = option(&args, "--token-file").map(Path::new);
                let (listener, token) =
                    pcbcraft_mcp::http::start_http(address, token_file, &mut std::io::stderr())?;
                pcbcraft_mcp::http::serve_listener(&hub, &listener, &token);
                return Ok(());
            }
            pcbcraft_mcp::serve(
                &mut std::io::stdin().lock(),
                &mut std::io::stdout().lock(),
                &hub,
                option(&args, "--connect"),
            )?
        }
        "run" => {
            let path = args
                .get(1)
                .ok_or("usage: pcbcraft-cli run script.json --save design.pcbcraft")?;
            let text = pcbcraft_io::read_text(Path::new(path))?;
            let script: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            let commands: Vec<Command> = serde_json::from_value(
                script
                    .get("commands")
                    .cloned()
                    .ok_or("script needs commands array")?,
            )
            .map_err(|e| e.to_string())?;
            let result = session.batch(&commands, None).map_err(|e| e.to_string())?;
            if let Some(path) = option(&args, "--save") {
                pcbcraft_io::save(&session.project, Path::new(path))?;
            }
            if let Some(path) = option(&args, "--export") {
                pcbcraft_io::atomic_write(
                    Path::new(path),
                    pcbcraft_io::kicad_board(&session.project)?.as_bytes(),
                )?;
            }
            println!("{result}");
        }
        "new" => {
            let path = option(&args, "--out").ok_or("new requires --out design.pcbcraft")?;
            let title = option(&args, "--name");
            write_new_project(Path::new(path), title)?;
            println!("{}", json!({"path":path,"name":title}));
        }
        "init" => {
            let dir = option(&args, "--dir")
                .or_else(|| args.get(1).map(String::as_str))
                .ok_or("init requires DIRECTORY or --dir PATH")?;
            let title = option(&args, "--name");
            let force = flag(&args, "--force");
            let design = init_project_directory(Path::new(dir), title, force)?;
            println!(
                "{}",
                json!({
                    "directory": dir,
                    "design": design.to_string_lossy(),
                    "build": Path::new(dir).join("build.json").to_string_lossy(),
                    "name": title,
                })
            );
        }
        "sample" => {
            let path = option(&args, "--out").ok_or("sample requires --out design.pcbcraft")?;
            let sample = Session::sample().map_err(|e| e.to_string())?;
            pcbcraft_io::save(&sample.project, Path::new(path))?;
            println!("{}", json!({"path":path}));
        }
        "export" => {
            let path = option(&args, "--out").ok_or("export requires --out board.kicad_pcb")?;
            pcbcraft_io::atomic_write(
                Path::new(path),
                pcbcraft_io::kicad_board(&session.project)?.as_bytes(),
            )?;
            println!("{}", json!({"path":path}));
        }
        "snapshot" => {
            let path = option(&args, "--out").ok_or("snapshot requires --out preview.png")?;
            let view = option(&args, "--view").unwrap_or("pcb");
            let bytes = pcbcraft_io::preview_png(&session.project, view)?;
            pcbcraft_io::atomic_write(Path::new(path), &bytes)?;
            println!("{}", json!({"path":path,"view":view}));
        }
        "help" | "--help" | "-h" => println!(
            "PCBCraft — Rust EDA, MCP-first\n\n\
commands | inspect | checks | new | init | sample | run | export | snapshot | tool | mcp\n\n\
Common: --sample or --project FILE\n\
new: pcbcraft-cli new --out design.pcbcraft [--name TITLE]\n\
init: pcbcraft-cli init DIRECTORY [--name TITLE] [--force]\n\
Tool: pcbcraft-cli tool NAME --args JSON [--save FILE]\n\
MCP: pcbcraft-cli mcp [--connect 127.0.0.1:7991 | --http 127.0.0.1:PORT [--token-file FILE]]\n\
Desktop: pcbcraft --sample --control 127.0.0.1:7991\n\
See README.md and docs/mcp.md for schemas and examples."
        ),
        _ => return Err(format!("unknown action: {action}")),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("pcbcraft-cli: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_io::load;

    #[test]
    fn new_writes_an_empty_native_project_file() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("blank.pcbcraft");
        write_new_project(&path, None)?;
        let project = load(&path)?;
        assert_eq!(project.name, "Untitled circuit");
        assert!(project.components.is_empty());
        Ok(())
    }

    #[test]
    fn new_applies_the_optional_title() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("named.pcbcraft");
        write_new_project(&path, Some("Bench power"))?;
        let project = load(&path)?;
        assert_eq!(project.name, "Bench power");
        Ok(())
    }

    #[test]
    fn init_scaffolds_a_directory_with_design_and_build_json() -> Result<(), String> {
        let root = tempfile::tempdir().map_err(|e| e.to_string())?;
        let folder = root.path().join("my-board");
        let design = init_project_directory(&folder, Some("My board"), false)?;
        assert_eq!(design, folder.join("design.pcbcraft"));
        let loaded = load(&design)?;
        assert_eq!(loaded.name, "My board");
        let build_text = pcbcraft_io::read_text(&folder.join("build.json"))?;
        let build: Value = serde_json::from_str(&build_text).map_err(|e| e.to_string())?;
        let commands = build
            .get("commands")
            .and_then(Value::as_array)
            .map(|v| v.len())
            .unwrap_or(0);
        assert_eq!(commands, 1);
        Ok(())
    }

    #[test]
    fn init_refuses_a_nonempty_directory_without_force() -> Result<(), String> {
        let root = tempfile::tempdir().map_err(|e| e.to_string())?;
        let folder = root.path().join("busy");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        std::fs::write(folder.join("notes.txt"), b"keep").map_err(|e| e.to_string())?;
        let err = init_project_directory(&folder, None, false)
            .err()
            .ok_or("expected init to fail on a non-empty directory")?;
        assert!(err.contains("not empty"));
        Ok(())
    }
}
