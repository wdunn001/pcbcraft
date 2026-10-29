//! Reference data for the website, behind `cargo xtask site-data`.
//! Writes four JSON files into site/src/data/ from the code: the MCP tool
//! list, the engine commands, the machine profiles and the design check names.
//! `--check` regenerates in memory and fails when a committed file differs.
//!
//! # Rules
//! - Output is byte identical across runs: object keys sort, lists sort by name or id, one trailing newline.
//! - Counts come from the lists beside them. `generate` is pure; only `run` writes.
//!
//! # Dependencies
//! - crates/mcp/src/lib.rs
//! - crates/engine/src/lib.rs
//! - crates/io/src/machine_profile.rs
//! - crates/text/strings/en.txt
//!
//! # Do not
//! - Hand-edit the JSON files; regenerate them.
//! - Write anywhere but site/src/data/.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// Where the generated files live, relative to the repository root.
const OUT_DIR: &str = "site/src/data";
/// Where the machine profile TOML files live, relative to the repository root.
const PROFILE_DIR: &str = "machine-profiles";
/// Source folder scanned for finding codes.
const RULES_DIR: &str = "crates/rules/src";
/// The English text catalog, which carries a title for each check.
const CATALOG: &str = "crates/text/strings/en.txt";

/// Generated file name and its text, in a stable order.
type Files = BTreeMap<String, String>;

/// Entry point for `cargo xtask site-data [--check]`.
///
/// # Rules
/// - Plain run writes the four files; `--check` writes nothing and fails on any difference.
///
/// # Dependencies
/// - xtask/src/main.rs
///
/// # Do not
/// - Accept other flags silently; an unknown argument is an error.
pub fn run(args: &[String], root: &Path) -> Result<(), String> {
    let check = match args {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => return Err("usage: cargo xtask site-data [--check]".into()),
    };
    let out = root.join(OUT_DIR);
    let files = generate(root)?;
    if check {
        let stale = stale_files(&files, &out);
        if stale.is_empty() {
            println!("site-data: {} files are current", files.len());
            return Ok(());
        }
        return Err(format!(
            "site-data: stale or missing: {}. Run `cargo xtask site-data` and commit the result.",
            stale.join(", ")
        ));
    }
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (name, text) in &files {
        let path = out.join(name);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("site-data: wrote {}", path.display());
    }
    Ok(())
}

/// Names of generated files whose committed copy is missing or different.
fn stale_files(files: &Files, out: &Path) -> Vec<String> {
    files
        .iter()
        .filter(|(name, text)| {
            std::fs::read_to_string(out.join(name)).ok().as_deref() != Some(text.as_str())
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// Builds the four files from the code and the repository files.
///
/// # Rules
/// - Returns file name to text; the caller decides whether to write or compare.
///
/// # Dependencies
/// - crates/mcp/src/lib.rs
/// - crates/engine/src/lib.rs
pub fn generate(root: &Path) -> Result<Files, String> {
    let mut files = Files::new();
    files.insert("mcp-tools.json".into(), render(&mcp_tools()?)?);
    files.insert("commands.json".into(), render(&commands()?)?);
    files.insert(
        "machine-profiles.json".into(),
        render(&machine_profiles(root)?)?,
    );
    files.insert("design-rules.json".into(), render(&design_rules(root)?)?);
    Ok(files)
}

/// Pretty JSON with a trailing newline. Object keys come out sorted because
/// `serde_json` keeps maps ordered without the `preserve_order` feature.
fn render(value: &Value) -> Result<String, String> {
    let mut text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(text)
}

/// A string field of a JSON object, or an error naming it.
fn text_of<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing text field {key}"))
}

/// The MCP tools as the server lists them, sorted by name.
fn mcp_tools() -> Result<Value, String> {
    let listed = pcbcraft_mcp::tool_list();
    let tools = listed
        .get("tools")
        .and_then(Value::as_array)
        .ok_or("tool_list has no tools array")?;
    let mut rows = Vec::new();
    for tool in tools {
        rows.push(json!({
            "name": text_of(tool, "name")?,
            "description": text_of(tool, "description")?,
            "inputSchema": tool.get("inputSchema").cloned().unwrap_or(Value::Null),
        }));
    }
    sort_by_key(&mut rows, "name", false)?;
    let duplicates = duplicates_of(&rows, "name");
    Ok(json!({"count": rows.len(), "duplicateNames": duplicates, "tools": rows}))
}

/// The engine commands, sorted by id.
fn commands() -> Result<Value, String> {
    let mut rows: Vec<Value> = pcbcraft_engine::commands()
        .into_iter()
        .map(|c| json!({"id": c.id, "description": c.description, "inputSchema": c.input_schema}))
        .collect();
    sort_by_key(&mut rows, "id", false)?;
    let duplicates = duplicates_of(&rows, "id");
    Ok(json!({"count": rows.len(), "duplicateIds": duplicates, "commands": rows}))
}

/// Values of a text field that appear more than once. The server lists a few
/// names twice; counts keep every entry and this list lets a page generator
/// emit one page per name.
fn duplicates_of(rows: &[Value], key: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut again = BTreeSet::new();
    for name in rows
        .iter()
        .filter_map(|r| r.get(key).and_then(Value::as_str))
    {
        if !seen.insert(name) {
            again.insert(name.to_string());
        }
    }
    again
}

/// Sorts rows by a text field (stable) and, when `unique`, refuses a duplicate.
fn sort_by_key(rows: &mut [Value], key: &str, unique: bool) -> Result<(), String> {
    rows.sort_by(|a, b| {
        let left = a.get(key).and_then(Value::as_str).unwrap_or("");
        let right = b.get(key).and_then(Value::as_str).unwrap_or("");
        left.cmp(right)
    });
    let mut seen = BTreeSet::new();
    for row in rows.iter().filter(|_| unique) {
        let name = text_of(row, key)?;
        if !seen.insert(name.to_string()) {
            return Err(format!("duplicate {key}: {name}"));
        }
    }
    Ok(())
}

/// The shop machine profiles read from machine-profiles/*.toml, sorted by id.
fn machine_profiles(root: &Path) -> Result<Value, String> {
    let dir = root.join(PROFILE_DIR);
    let mut rows = Vec::new();
    for path in files_with_extension(&dir, "toml")? {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let p = pcbcraft_io::machine_profile_from_toml(&text)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        rows.push(json!({
            "id": p.id,
            "name": p.name,
            "kind": p.kind,
            "vendor": p.vendor,
            "model": p.model,
            "notes": p.notes,
            "workArea": {"x": p.work_area.x, "y": p.work_area.y, "z": p.work_area.z, "origin": p.work_area.origin},
            "output": {"format": p.output.format, "dialect": p.output.dialect, "units": p.output.units},
        }));
    }
    sort_by_key(&mut rows, "id", true)?;
    Ok(json!({"count": rows.len(), "profiles": rows}))
}

/// Files directly inside a folder with the given extension, sorted by path.
fn files_with_extension(dir: &Path, extension: &str) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// The design check names: every finding code written in the rules crate plus
/// every `check.<code>` key in the English catalog, each with its catalog title
/// when there is one.
fn design_rules(root: &Path) -> Result<Value, String> {
    let catalog =
        std::fs::read_to_string(root.join(CATALOG)).map_err(|e| format!("{CATALOG}: {e}"))?;
    let (entries, _) = pcbcraft_text::parse(&catalog);
    let titles: BTreeMap<String, String> = entries
        .into_iter()
        .filter_map(|(k, v)| k.strip_prefix("check.").map(|c| (c.to_string(), v)))
        .collect();
    let mut codes: BTreeSet<String> = titles.keys().cloned().collect();
    for path in files_with_extension(&root.join(RULES_DIR), "rs")? {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        codes.extend(finding_codes(&text));
    }
    let rows: Vec<Value> = codes
        .iter()
        .map(|code| json!({"code": code, "title": titles.get(code)}))
        .collect();
    Ok(json!({"count": rows.len(), "rules": rows}))
}

/// Codes written as `code: "name"` in Rust source, in lowercase snake case.
fn finding_codes(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for part in text.split("code: \"").skip(1) {
        let name: String = part.chars().take_while(|c| *c != '"').collect();
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit());
        if valid && part.contains('"') {
            found.push(name);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repository root, one level above the xtask manifest directory.
    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    #[test]
    fn site_data_is_deterministic() -> Result<(), String> {
        let first = generate(&repo())?;
        let second = generate(&repo())?;
        assert_eq!(first, second);
        assert_eq!(first.len(), 4);
        assert!(first.values().all(|t| t.ends_with("}\n")));
        Ok(())
    }

    #[test]
    fn site_data_check_fails_when_stale() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let files = generate(&repo())?;
        for (name, text) in &files {
            std::fs::write(dir.path().join(name), text).map_err(|e| e.to_string())?;
        }
        assert!(stale_files(&files, dir.path()).is_empty());
        let tools = dir.path().join("mcp-tools.json");
        let edited = files["mcp-tools.json"].replacen(
            "\"description\": \"",
            "\"description\": \"Edited. ",
            1,
        );
        std::fs::write(&tools, edited).map_err(|e| e.to_string())?;
        assert_eq!(
            stale_files(&files, dir.path()),
            vec!["mcp-tools.json".to_string()]
        );
        std::fs::remove_file(&tools).map_err(|e| e.to_string())?;
        assert_eq!(
            stale_files(&files, dir.path()),
            vec!["mcp-tools.json".to_string()]
        );
        Ok(())
    }

    #[test]
    fn mcp_tool_count_matches_tool_list() -> Result<(), String> {
        let listed = pcbcraft_mcp::tool_list();
        let expected = listed["tools"].as_array().map(Vec::len).ok_or("no tools")?;
        let made = mcp_tools()?;
        assert_eq!(made["count"], json!(expected));
        assert_eq!(made["tools"].as_array().map(Vec::len), Some(expected));
        let commands = commands()?;
        assert_eq!(commands["count"], json!(pcbcraft_engine::commands().len()));
        Ok(())
    }

    #[test]
    fn every_machine_profile_file_is_listed() -> Result<(), String> {
        let made = machine_profiles(&repo())?;
        let listed: BTreeSet<String> = made["profiles"]
            .as_array()
            .ok_or("no profiles")?
            .iter()
            .map(|p| text_of(p, "id").map(str::to_string))
            .collect::<Result<_, _>>()?;
        let files = files_with_extension(&repo().join(PROFILE_DIR), "toml")?;
        assert_eq!(listed.len(), files.len());
        let builtin: BTreeSet<String> = pcbcraft_io::builtin_machine_profile_ids()
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(listed, builtin);
        Ok(())
    }
}
