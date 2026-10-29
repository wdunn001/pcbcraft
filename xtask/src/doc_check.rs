//! The documentation standard check behind `cargo xtask docs check`.
//! Four checks: paths named in doc blocks exist, trait implementation lists
//! match the code, AGENTS.md stays small, and missing-docs warnings only fall.
//!
//! # Rules
//! - Every check is a pure function over text and an existence closure; only `run` touches the disk.
//! - Failures print `docs-check: <file>:<line>: <message>` on stderr.
//!
//! # Dependencies
//! - docs/doc-standard.md
//! - xtask/doc-ratchet.toml
//! - xtask/src/guide_check.rs
//! - xtask/src/main.rs
//!
//! # Do not
//! - Add a dependency; the ratchet file is read by hand.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

const AGENTS_MAX_LINES: usize = 60;
const AGENTS_MIN_PRE_EDIT: usize = 3;
const LONG_BLOCK_LINES: usize = 15;
const RATCHET_FILE: &str = "xtask/doc-ratchet.toml";
const ENGINE_LIB: &str = "crates/engine/src/lib.rs";

/// A run of consecutive `///` or `//!` lines, prefix removed.
struct Block {
    start: usize,
    lines: Vec<String>,
    after_line: usize,
}

/// A named section of a block and its list items as (line number, text).
struct Section {
    name: String,
    items: Vec<(usize, String)>,
}

/// A public trait, or the engine command registry, and the doc block above it.
struct TraitDef {
    file: String,
    line: usize,
    name: String,
    block: Option<Block>,
}

/// One `impl Trait for X` line and the doc block above it.
struct ImplSite {
    file: String,
    line: usize,
    trait_name: String,
    block: Option<Block>,
}

/// The parsed ratchet file; `lines` is kept so it can be written back unchanged.
pub struct Ratchet {
    lines: Vec<String>,
    missing_docs: BTreeMap<String, u64>,
    pending: BTreeSet<String>,
}

/// Collects the doc blocks of one file in order.
fn parse_blocks(text: &str) -> Vec<Block> {
    let all: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < all.len() {
        let is_doc = |s: &str| {
            let t = s.trim_start();
            t.starts_with("///") || t.starts_with("//!")
        };
        if !all.get(i).is_some_and(|s| is_doc(s)) {
            i += 1;
            continue;
        }
        let start = i;
        let mut lines = Vec::new();
        while let Some(s) = all.get(i).filter(|s| is_doc(s)) {
            let body = s.trim_start().get(3..).unwrap_or("");
            lines.push(body.strip_prefix(' ').unwrap_or(body).to_string());
            i += 1;
        }
        let mut j = i;
        while all.get(j).is_some_and(|s| s.trim_start().starts_with("#[")) {
            j += 1;
        }
        blocks.push(Block {
            start: start + 1,
            lines,
            after_line: j + 1,
        });
    }
    blocks
}

/// Splits a block into `# Name` sections with their `- ` items.
fn sections(block: &Block) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for (offset, line) in block.lines.iter().enumerate() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix("# ") {
            out.push(Section {
                name: name.trim().to_string(),
                items: Vec::new(),
            });
        } else if let (Some(item), Some(section)) = (t.strip_prefix("- "), out.last_mut()) {
            section
                .items
                .push((block.start + offset, item.trim().to_string()));
        }
    }
    out
}

/// The path token of a list item: up to the first space or `(`, without backticks or anchor.
fn item_path(item: &str) -> String {
    let token: String = item
        .trim_start_matches('`')
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '(')
        .collect();
    let token = token.trim_matches('`').trim_end_matches(',');
    token.split('#').next().unwrap_or("").to_string()
}

/// Check (a): `Contract:` paths and `# Dependencies` and `# Implementations` items exist.
fn check_paths(file: &str, blocks: &[Block], exists: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut errors = Vec::new();
    let mut test = |line: usize, path: &str| {
        if !path.contains('/') && Path::new(path).extension().is_none() {
            errors.push(format!(
                "{file}:{line}: `{path}` is not a repo-root relative path"
            ));
        } else if !exists(path) {
            errors.push(format!("{file}:{line}: path does not exist: {path}"));
        }
    };
    for block in blocks {
        for (offset, text) in block.lines.iter().enumerate() {
            if let Some(rest) = text.trim().strip_prefix("Contract:") {
                let token = rest.split_whitespace().next().unwrap_or("");
                test(
                    block.start + offset,
                    token.trim_matches('`').trim_end_matches(','),
                );
            }
        }
        for section in sections(block) {
            if section.name == "Dependencies" || section.name == "Implementations" {
                for (line, item) in section.items {
                    if item != "none yet" {
                        test(line, &item_path(&item));
                    }
                }
            }
        }
    }
    errors
}

/// The trait name of an `impl ... Trait for X` line, if the line is one.
fn impl_trait_name(line: &str) -> Option<String> {
    let t = line.trim();
    let t = t.strip_prefix("unsafe ").unwrap_or(t);
    let rest = t.strip_prefix("impl")?;
    let mut rest = rest;
    if rest.starts_with('<') {
        let mut depth = 0usize;
        let mut end = None;
        for (i, c) in rest.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        end = Some(i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = rest.get(end?..)?;
    } else if !rest.starts_with(' ') {
        return None;
    }
    let (left, _) = rest.split_once(" for ")?;
    let left = left.trim().trim_start_matches('!');
    let last = left.rsplit("::").next().unwrap_or(left);
    let name: String = last
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Finds public traits, plus `pub fn commands` in the engine lib, in one file.
fn find_traits(file: &str, text: &str) -> Vec<TraitDef> {
    let mut blocks = parse_blocks(text);
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        let name = t
            .strip_prefix("pub trait ")
            .or_else(|| t.strip_prefix("pub unsafe trait "))
            .map(|rest| {
                rest.chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>()
            })
            .or_else(|| {
                (file == ENGINE_LIB && t.starts_with("pub fn commands(")).then(|| "commands".into())
            });
        if let Some(name) = name {
            let pos = blocks.iter().position(|b| b.after_line == i + 1);
            out.push(TraitDef {
                file: file.to_string(),
                line: i + 1,
                name,
                block: pos.map(|p| blocks.swap_remove(p)),
            });
        }
    }
    out
}

/// Finds every `impl Trait for X` in one file.
fn find_impls(file: &str, text: &str) -> Vec<ImplSite> {
    let mut blocks = parse_blocks(text);
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if let Some(trait_name) = impl_trait_name(line) {
            let pos = blocks.iter().position(|b| b.after_line == i + 1);
            out.push(ImplSite {
                file: file.to_string(),
                line: i + 1,
                trait_name,
                block: pos.map(|p| blocks.swap_remove(p)),
            });
        }
    }
    out
}

/// Whether a path sits under a `tests/` directory.
fn is_test_path(path: &str) -> bool {
    path.starts_with("tests/") || path.contains("/tests/")
}

/// Check (b): trait implementation lists, `Contract:` lines and the registry list.
fn check_traits(defs: &[TraitDef], impls: &[ImplSite], pending: &BTreeSet<String>) -> Vec<String> {
    let mut errors = Vec::new();
    for def in defs.iter().filter(|d| !pending.contains(&d.name)) {
        let here = format!("{}:{}", def.file, def.line);
        let section = def
            .block
            .as_ref()
            .map(sections)
            .and_then(|s| s.into_iter().find(|s| s.name == "Implementations"));
        let Some(section) = section else {
            errors.push(format!(
                "{here}: `{}` has no # Implementations section",
                def.name
            ));
            continue;
        };
        if def.name == "commands" {
            continue;
        }
        let none_yet = section.items.iter().any(|(_, t)| t == "none yet");
        let listed: BTreeSet<String> = section
            .items
            .iter()
            .filter(|(_, t)| t != "none yet")
            .map(|(_, t)| item_path(t))
            .filter(|p| !is_test_path(p))
            .collect();
        let actual: BTreeSet<String> = impls
            .iter()
            .filter(|i| i.trait_name == def.name && !is_test_path(&i.file))
            .map(|i| i.file.clone())
            .collect();
        if none_yet && !actual.is_empty() {
            errors.push(format!(
                "{here}: `{}` says none yet but is implemented in {}",
                def.name,
                actual.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if !none_yet && actual.is_empty() && listed.is_empty() {
            errors.push(format!(
                "{here}: `{}` has no implementations; the list must be `none yet`",
                def.name
            ));
        }
        for path in listed.difference(&actual) {
            errors.push(format!(
                "{here}: `{}` lists {path} but no `impl {} for` is there",
                def.name, def.name
            ));
        }
        if !none_yet {
            for path in actual.difference(&listed) {
                errors.push(format!(
                    "{here}: `{}` is implemented in {path} but not listed",
                    def.name
                ));
            }
        }
        for site in impls
            .iter()
            .filter(|i| i.trait_name == def.name && !is_test_path(&i.file))
        {
            let wanted = format!("Contract: {}", def.file);
            let ok = site
                .block
                .as_ref()
                .is_some_and(|b| b.lines.iter().any(|l| l.trim().trim_matches('`') == wanted));
            if !ok {
                errors.push(format!(
                    "{}:{}: impl of `{}` needs a doc line `{wanted}`",
                    site.file, site.line, def.name
                ));
            }
        }
    }
    errors
}

/// Check (c): AGENTS.md size, banned headings and the pre-edit lines.
fn check_agents(text: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let count = text.lines().count();
    if count > AGENTS_MAX_LINES {
        errors.push(format!(
            "AGENTS.md:1: {count} lines, the limit is {AGENTS_MAX_LINES}"
        ));
    }
    for (i, line) in text.lines().enumerate() {
        if !line.starts_with('#') {
            continue;
        }
        let head = line.trim_start_matches('#').trim().to_lowercase();
        let banned = ["business rules", "rules"].iter().any(|b| {
            head.strip_prefix(b)
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        });
        if banned {
            errors.push(format!(
                "AGENTS.md:{}: business rules belong in the file or contract docs, not here",
                i + 1
            ));
        }
    }
    let pre = text.lines().filter(|l| l.contains("Pre-edit:")).count();
    if pre < AGENTS_MIN_PRE_EDIT {
        errors.push(format!(
            "AGENTS.md:1: needs {AGENTS_MIN_PRE_EDIT} `Pre-edit:` lines, found {pre}"
        ));
    }
    errors
}

/// Warnings for doc blocks longer than the soft limit; these never fail the check.
fn long_block_warnings(file: &str, blocks: &[Block]) -> Vec<String> {
    blocks
        .iter()
        .filter(|b| b.lines.len() > LONG_BLOCK_LINES)
        .map(|b| {
            format!(
                "docs-check: warning: {file}:{} block has {} lines",
                b.start,
                b.lines.len()
            )
        })
        .collect()
}

impl Ratchet {
    /// Parses the ratchet file line by line: `[section]`, `name = value` and `#` comments.
    pub fn parse(text: &str) -> Result<Ratchet, String> {
        let mut section = String::new();
        let mut missing_docs = BTreeMap::new();
        let mut pending = BTreeSet::new();
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            if let Some(name) = t.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                section = name.trim().to_string();
                continue;
            }
            let (key, value) = t
                .split_once('=')
                .ok_or_else(|| format!("{RATCHET_FILE}:{}: expected name = value", i + 1))?;
            let (key, value) = (key.trim().to_string(), value.trim());
            match section.as_str() {
                "missing_docs" => {
                    let n = value
                        .parse::<u64>()
                        .map_err(|_| format!("{RATCHET_FILE}:{}: not a number: {value}", i + 1))?;
                    missing_docs.insert(key, n);
                }
                "pending_traits" => {
                    if value == "true" {
                        pending.insert(key);
                    }
                }
                // Read by guide_check.rs.
                "guide_coverage" => {}
                _ => return Err(format!("{RATCHET_FILE}:{}: unknown section", i + 1)),
            }
        }
        Ok(Ratchet {
            lines: text.lines().map(str::to_string).collect(),
            missing_docs,
            pending,
        })
    }

    /// The file text, identical to what was parsed unless counts were set.
    pub fn render(&self) -> String {
        let mut out = self.lines.join("\n");
        out.push('\n');
        out
    }

    /// Rewrites the `[missing_docs]` numbers from a fresh count, adding crates that are new.
    pub fn set_counts(&mut self, counts: &BTreeMap<String, u64>) {
        let mut in_section = false;
        let mut last_entry = None;
        for (i, line) in self.lines.iter_mut().enumerate() {
            let t = line.trim().to_string();
            if t.starts_with('[') {
                in_section = t == "[missing_docs]";
            } else if let (true, Some((key, _))) = (in_section, t.split_once('=')) {
                let key = key.trim().to_string();
                *line = format!("{key} = {}", counts.get(&key).copied().unwrap_or(0));
                last_entry = Some(i);
            }
        }
        let known: BTreeSet<String> = self.missing_docs.keys().cloned().collect();
        let at = last_entry.map_or(self.lines.len(), |i| i + 1);
        let added: Vec<String> = counts
            .iter()
            .filter(|(k, _)| !known.contains(*k))
            .map(|(name, n)| format!("{name} = {n}"))
            .collect();
        self.lines.splice(at..at, added);
        for (name, n) in counts {
            self.missing_docs.insert(name.clone(), *n);
        }
        for (name, n) in self.missing_docs.iter_mut() {
            if !counts.contains_key(name) {
                *n = 0;
            }
        }
    }
}

/// Check (d), pure part: compares fresh counts with the ratchet. Returns (errors, notices).
fn check_ratchet(
    counts: &BTreeMap<String, u64>,
    ratchet: &BTreeMap<String, u64>,
) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut notices = Vec::new();
    for (name, n) in counts {
        match ratchet.get(name) {
            None => errors.push(format!(
                "{RATCHET_FILE}: {name} has {n} missing-docs warnings and no ratchet entry"
            )),
            Some(max) if n > max => errors.push(format!(
                "{RATCHET_FILE}: {name} has {n} missing-docs warnings, the ratchet allows {max}"
            )),
            _ => {}
        }
    }
    for (name, max) in ratchet {
        let n = counts.get(name).copied().unwrap_or(0);
        if n < *max {
            notices.push(format!(
                "docs-check: {name} is down to {n}, lower the ratchet"
            ));
        }
    }
    (errors, notices)
}

/// The crate name from a cargo `package_id`, in the old or the new format.
fn crate_from_package_id(id: &str) -> Option<String> {
    if let Some((_, tail)) = id.split_once('#') {
        if let Some((name, _)) = tail.split_once('@') {
            return Some(name.to_string());
        }
        let dir = id.split('#').next()?.rsplit('/').next()?;
        return Some(dir.to_string());
    }
    id.split_whitespace().next().map(str::to_string)
}

/// Counts `missing_docs` compiler messages per crate in cargo's JSON output.
fn parse_missing_docs(stdout: &str) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let code = value
            .pointer("/message/code/code")
            .and_then(serde_json::Value::as_str);
        if value.get("reason").and_then(serde_json::Value::as_str) != Some("compiler-message")
            || code != Some("missing_docs")
        {
            continue;
        }
        let name = value
            .get("package_id")
            .and_then(serde_json::Value::as_str)
            .and_then(crate_from_package_id);
        if let Some(name) = name {
            *counts.entry(name).or_insert(0) += 1;
        }
    }
    counts
}

/// Runs cargo check with `-W missing_docs` in its own target dir and counts the warnings.
fn count_missing_docs(root: &Path) -> Result<BTreeMap<String, u64>, String> {
    let existing = std::env::var("RUSTFLAGS").unwrap_or_default();
    let flags = format!("{existing} -W missing_docs");
    let args = [
        "check",
        "--workspace",
        "--lib",
        "--locked",
        "--message-format",
        "json",
    ];
    println!("+ cargo {} (RUSTFLAGS={flags})", args.join(" "));
    let output = Command::new("cargo")
        .args(args)
        .env("RUSTFLAGS", flags.trim())
        .env("CARGO_TARGET_DIR", root.join("target").join("doc-check"))
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo check: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo check failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(parse_missing_docs(&String::from_utf8_lossy(&output.stdout)))
}

/// Entry point: runs the four checks; `--static` skips (d), `--write-ratchet` rewrites the counts.
pub fn run(args: &[String]) -> Result<(), String> {
    let root = crate::root()?;
    let mut paths = Vec::new();
    for dir in ["crates", "apps", "xtask"] {
        crate::rust_files(&root.join(dir), &mut paths)?;
    }
    paths.sort();
    let mut files = Vec::new();
    for path in paths {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        files.push((rel, text));
    }
    let ratchet_path = root.join(RATCHET_FILE);
    let mut ratchet = Ratchet::parse(
        &std::fs::read_to_string(&ratchet_path)
            .map_err(|e| format!("{}: {e}", ratchet_path.display()))?,
    )?;
    let exists = |p: &str| root.join(p).exists();
    let mut errors = Vec::new();
    let (mut defs, mut impls) = (Vec::new(), Vec::new());
    for (rel, text) in &files {
        let blocks = parse_blocks(text);
        errors.extend(check_paths(rel, &blocks, &exists));
        for warning in long_block_warnings(rel, &blocks) {
            eprintln!("{warning}");
        }
        defs.extend(find_traits(rel, text));
        impls.extend(find_impls(rel, text));
    }
    errors.extend(check_traits(&defs, &impls, &ratchet.pending));
    let agents = std::fs::read_to_string(root.join("AGENTS.md")).map_err(|e| e.to_string())?;
    errors.extend(check_agents(&agents));
    if !args.iter().any(|a| a == "--static") || args.iter().any(|a| a == "--write-ratchet") {
        let counts = count_missing_docs(&root)?;
        if args.iter().any(|a| a == "--write-ratchet") {
            ratchet.set_counts(&counts);
            std::fs::write(&ratchet_path, ratchet.render())
                .map_err(|e| format!("{}: {e}", ratchet_path.display()))?;
            println!("wrote {RATCHET_FILE}: {counts:?}");
        } else {
            let (bad, notices) = check_ratchet(&counts, &ratchet.missing_docs);
            errors.extend(bad);
            for notice in notices {
                println!("{notice}");
            }
        }
    }
    if errors.is_empty() {
        println!("documentation standard passed");
        Ok(())
    } else {
        for e in &errors {
            eprintln!("docs-check: {e}");
        }
        Err(format!("docs-check: {} problem(s)", errors.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strips the `|` margin that keeps fixture lines from looking like real doc comments.
    fn fx(text: &str) -> String {
        text.lines()
            .map(|l| l.trim_start().strip_prefix('|').unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn exists_in(known: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |p| known.contains(&p)
    }

    fn traits_of(file: &str, text: &str) -> Vec<TraitDef> {
        find_traits(file, &fx(text))
    }

    const TRAIT_OK: &str = "
        |/// One catalogue.
        |///
        |/// # Implementations
        |/// - crates/a/src/lib.rs (`Fixture`)
        |/// - crates/a/tests/t.rs (`Counting`, test only)
        |pub trait Source {}";

    const IMPL_OK: &str = "
        |/// Contract: crates/a/src/lib.rs
        |impl Source for Fixture {}";

    #[test]
    fn path_check_flags_missing_contract_path() {
        let text = fx("|/// Contract: crates/a/src/gone.rs\n|impl X for Y {}");
        let bad = check_paths("f.rs", &parse_blocks(&text), &exists_in(&[]));
        assert_eq!(bad.len(), 1);
        assert!(
            bad.iter()
                .all(|m| m.contains("f.rs:1") && m.contains("does not exist"))
        );
        let ok = check_paths(
            "f.rs",
            &parse_blocks(&text),
            &exists_in(&["crates/a/src/gone.rs"]),
        );
        assert!(ok.is_empty());
    }

    #[test]
    fn path_check_accepts_existing_dependency_and_anchor() {
        let text =
            fx("|//! Head.\n|//!\n|//! # Dependencies\n|//! - crates/a/src/lib.rs#item and more\n");
        let ok = check_paths(
            "f.rs",
            &parse_blocks(&text),
            &exists_in(&["crates/a/src/lib.rs"]),
        );
        assert!(ok.is_empty());
        let bad = check_paths("f.rs", &parse_blocks(&text), &exists_in(&[]));
        assert_eq!(bad.len(), 1);
    }

    #[test]
    fn path_check_flags_dependency_without_slash() {
        let text = fx("|//! # Dependencies\n|//! - lib\n");
        let bad = check_paths("f.rs", &parse_blocks(&text), &exists_in(&["lib"]));
        assert_eq!(bad.len(), 1);
        assert!(
            bad.iter()
                .all(|m| m.contains("not a repo-root relative path"))
        );
        let text = fx("|//! # Dependencies\n|//! - Cargo.toml\n");
        assert!(check_paths("f.rs", &parse_blocks(&text), &exists_in(&["Cargo.toml"])).is_empty());
    }

    #[test]
    fn trait_without_implementations_section_fails() {
        let bare = traits_of(
            "crates/a/src/lib.rs",
            "|/// One catalogue.\n|pub trait Source {}",
        );
        let bad = check_traits(&bare, &[], &BTreeSet::new());
        assert_eq!(bad.len(), 1);
        assert!(bad.iter().all(|m| m.contains("no # Implementations")));
        let undocumented = traits_of("crates/a/src/lib.rs", "|pub trait Source {}");
        assert_eq!(check_traits(&undocumented, &[], &BTreeSet::new()).len(), 1);
    }

    #[test]
    fn trait_list_missing_a_real_impl_fails() {
        let defs = traits_of("crates/a/src/lib.rs", TRAIT_OK);
        let mut impls = find_impls("crates/a/src/lib.rs", &fx(IMPL_OK));
        assert!(check_traits(&defs, &impls, &BTreeSet::new()).is_empty());
        impls.extend(find_impls("crates/b/src/lib.rs", &fx(IMPL_OK)));
        let bad = check_traits(&defs, &impls, &BTreeSet::new());
        assert!(
            bad.iter()
                .any(|m| m.contains("crates/b/src/lib.rs but not listed"))
        );
    }

    #[test]
    fn trait_list_naming_a_file_without_impl_fails() {
        let defs = traits_of("crates/a/src/lib.rs", TRAIT_OK);
        let bad = check_traits(&defs, &[], &BTreeSet::new());
        assert!(bad.iter().any(|m| m.contains("lists crates/a/src/lib.rs")));
        let impls = find_impls("crates/a/src/lib.rs", &fx(IMPL_OK));
        assert!(check_traits(&defs, &impls, &BTreeSet::new()).is_empty());
    }

    #[test]
    fn trait_with_no_impls_requires_none_yet() {
        let none = traits_of(
            "crates/a/src/lib.rs",
            "|/// Keys.\n|///\n|/// # Implementations\n|/// - none yet\n|pub trait Secrets {}",
        );
        assert!(check_traits(&none, &[], &BTreeSet::new()).is_empty());
        let impls = find_impls("crates/a/src/lib.rs", "impl Secrets for Vault {}");
        assert!(!check_traits(&none, &impls, &BTreeSet::new()).is_empty());
    }

    #[test]
    fn pending_trait_is_skipped() {
        let defs = traits_of("crates/a/src/lib.rs", "|pub trait Source {}");
        assert_eq!(check_traits(&defs, &[], &BTreeSet::new()).len(), 1);
        let pending = BTreeSet::from(["Source".to_string()]);
        assert!(check_traits(&defs, &[], &pending).is_empty());
    }

    #[test]
    fn impl_without_contract_line_fails() {
        let defs = traits_of("crates/a/src/lib.rs", TRAIT_OK);
        let bare = find_impls(
            "crates/a/src/lib.rs",
            "impl<T: Send> Source for Fixture<T> {}",
        );
        assert_eq!(bare.len(), 1);
        let bad = check_traits(&defs, &bare, &BTreeSet::new());
        assert!(
            bad.iter()
                .any(|m| m.contains("needs a doc line `Contract: crates/a/src/lib.rs`"))
        );
        let good = find_impls("crates/a/src/lib.rs", &fx(IMPL_OK));
        assert!(check_traits(&defs, &good, &BTreeSet::new()).is_empty());
    }

    /// An AGENTS.md of the right shape, with `extra` appended.
    fn agents_text(extra: &str) -> String {
        format!(
            "# Agents\n\n## Before editing\n\n- **Pre-edit:** one\n- **Pre-edit:** two\n- **Pre-edit:** three\n{extra}"
        )
    }

    #[test]
    fn agents_over_60_lines_fails() {
        let long = agents_text(&"filler\n".repeat(60));
        assert!(
            check_agents(&long)
                .iter()
                .any(|m| m.contains("the limit is 60"))
        );
        assert!(check_agents(&agents_text(&"filler\n".repeat(50))).is_empty());
    }

    #[test]
    fn agents_with_rules_heading_fails() {
        let bad = check_agents(&agents_text("\n## Business rules\n"));
        assert!(bad.iter().any(|m| m.contains("business rules")));
        assert!(!check_agents(&agents_text("\n### Rules\n")).is_empty());
        assert!(check_agents(&agents_text("\n## Rulebook of crates\n")).is_empty());
    }

    #[test]
    fn agents_needs_three_pre_edit_lines() {
        let two = "# Agents\n- Pre-edit: a\n- Pre-edit: b\n";
        assert!(check_agents(two).iter().any(|m| m.contains("found 2")));
        assert!(check_agents(&agents_text("")).is_empty());
    }

    #[test]
    fn agents_passes_the_real_shape() {
        let text = "# PCBCraft agent instructions\n\n## Before coding\n\nRead.\n\n## Before editing a file (docs/doc-standard.md)\n\n- **Pre-edit:** a\n- **Pre-edit:** b\n- **Pre-edit:** c\n- Never shorten a `# Do not` section.\n\n## Parallel agents\n\n- One clone.\n\n## Architecture invariants\n\n- Layering.\n";
        assert!(check_agents(text).is_empty());
    }

    fn map(entries: &[(&str, u64)]) -> BTreeMap<String, u64> {
        entries.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn ratchet_rise_fails() {
        let (bad, _) = check_ratchet(&map(&[("a", 6)]), &map(&[("a", 5)]));
        assert_eq!(bad.len(), 1);
        assert!(
            bad.iter()
                .all(|m| m.contains("a has 6") && m.contains("allows 5"))
        );
        let (ok, _) = check_ratchet(&map(&[("a", 5)]), &map(&[("a", 5)]));
        assert!(ok.is_empty());
    }

    #[test]
    fn ratchet_equal_passes() {
        let (bad, notices) =
            check_ratchet(&map(&[("a", 5), ("b", 0)]), &map(&[("a", 5), ("b", 0)]));
        assert!(bad.is_empty() && notices.is_empty());
    }

    #[test]
    fn ratchet_drop_prints_notice() {
        let (bad, notices) = check_ratchet(&map(&[("a", 3)]), &map(&[("a", 5), ("b", 2)]));
        assert!(bad.is_empty());
        assert_eq!(notices.len(), 2);
        assert!(
            notices
                .iter()
                .any(|m| m == "docs-check: a is down to 3, lower the ratchet")
        );
        assert!(notices.iter().any(|m| m.contains("b is down to 0")));
    }

    #[test]
    fn ratchet_missing_crate_fails() {
        let (bad, _) = check_ratchet(&map(&[("new", 1)]), &map(&[("a", 5)]));
        assert_eq!(bad.len(), 1);
        assert!(
            bad.iter()
                .all(|m| m.contains("new has 1") && m.contains("no ratchet entry"))
        );
        let (ok, _) = check_ratchet(&map(&[("new", 1)]), &map(&[("new", 1)]));
        assert!(ok.is_empty());
    }

    #[test]
    fn long_block_warns_but_passes() {
        let long = format!("{}|pub fn f() {{}}", "|/// line\n".repeat(16));
        let text = fx(&long);
        let warnings = long_block_warnings("f.rs", &parse_blocks(&text));
        assert_eq!(warnings.len(), 1);
        assert!(warnings.iter().all(|w| w.contains("block has 16 lines")));
        let short = fx("|/// line\n|pub fn f() {}");
        assert!(long_block_warnings("f.rs", &parse_blocks(&short)).is_empty());
        assert!(check_paths("f.rs", &parse_blocks(&text), &exists_in(&[])).is_empty());
    }

    #[test]
    fn ratchet_file_round_trips() -> Result<(), String> {
        let text = "# Missing-docs warnings per crate.\n[missing_docs]\npcbcraft-geom = 29\npcbcraft-doc = 169\n\n# Pending.\n[pending_traits]\nSecrets = true\n";
        let mut ratchet = Ratchet::parse(text)?;
        assert_eq!(ratchet.render(), text);
        assert_eq!(ratchet.missing_docs.get("pcbcraft-doc"), Some(&169));
        assert!(ratchet.pending.contains("Secrets"));
        ratchet.set_counts(&map(&[("pcbcraft-geom", 3), ("pcbcraft-new", 7)]));
        let again = Ratchet::parse(&ratchet.render())?;
        assert_eq!(again.missing_docs.get("pcbcraft-geom"), Some(&3));
        assert_eq!(again.missing_docs.get("pcbcraft-doc"), Some(&0));
        assert_eq!(again.missing_docs.get("pcbcraft-new"), Some(&7));
        assert!(Ratchet::parse("[missing_docs]\nx = many\n").is_err());
        Ok(())
    }
}
