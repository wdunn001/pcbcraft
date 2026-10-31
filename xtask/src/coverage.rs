//! The coverage gate: run the workspace tests under `cargo-llvm-cov`, then
//! read the lcov file this module writes into per-crate and workspace line
//! coverage and check it against `ci/coverage.json` and the baseline.
//!
//! The parsing, ratchet and patch-coverage rules take plain text, so they are
//! unit tested with fixtures and need no instrumented build.
//!
//! # Rules
//! - The only exclusions are the ones listed in `ci/coverage.json`, each with a
//!   reason; `#[cfg(test)]` modules are removed by reading the source.
//! - `ci/coverage-baseline.json` is rewritten only by `--bless`.
//! - The gate mode is `workspace` (a minimum for the whole workspace) or
//!   `patch` (a minimum for the lines a pull request adds). Both also apply the
//!   per-crate ratchet.
//!
//! # Dependencies
//! - ci/coverage.json
//! - ci/coverage-baseline.json
//! - docs/ci.md
//! - xtask/src/main.rs
//!
//! # Do not
//! - Exclude a file because it is poorly covered; add a test instead.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Pinned tool version; CI installs exactly this one.
pub const LLVM_COV_VERSION: &str = "0.9.1";

/// Instrumented lines of each source file: line number to execution count.
pub type Lines = BTreeMap<u32, u64>;

/// Parses an lcov tracefile into `file -> line -> hits`. Paths under `root`
/// become relative; the same line seen twice (generic code) adds its hits.
pub fn parse_lcov(text: &str, root: &str) -> BTreeMap<String, Lines> {
    let mut files: BTreeMap<String, Lines> = BTreeMap::new();
    let mut current: Option<String> = None;
    let prefix = format!("{}/", root.trim_end_matches('/'));
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("SF:") {
            let rel = path.strip_prefix(&prefix).unwrap_or(path).to_string();
            files.entry(rel.clone()).or_default();
            current = Some(rel);
        } else if let Some(data) = line.strip_prefix("DA:") {
            let mut parts = data.split(',');
            let number = parts.next().and_then(|n| n.parse::<u32>().ok());
            let hits = parts.next().and_then(|n| n.parse::<u64>().ok());
            if let (Some(file), Some(number), Some(hits)) = (&current, number, hits) {
                *files
                    .entry(file.clone())
                    .or_default()
                    .entry(number)
                    .or_insert(0) += hits;
            }
        } else if line == "end_of_record" {
            current = None;
        }
    }
    files
}

/// Matches a path against a glob where `*` stands for any run of characters.
pub fn glob_match(glob: &str, path: &str) -> bool {
    let parts: Vec<&str> = glob.split('*').collect();
    if parts.len() == 1 {
        return glob == path;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !path.starts_with(first) || path.len() < first.len() + last.len() {
        return false;
    }
    if !path.ends_with(last) {
        return false;
    }
    let mut at = first.len();
    let end = path.len() - last.len();
    for middle in &parts[1..parts.len() - 1] {
        match path[at..end].find(middle) {
            Some(i) => at += i + middle.len(),
            None => return false,
        }
    }
    true
}

/// Turns a glob into the regular expression `cargo llvm-cov` expects.
pub fn glob_to_regex(glob: &str) -> String {
    let mut out = String::new();
    for ch in glob.chars() {
        match ch {
            '*' => out.push_str(".*"),
            '.' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// The gate settings read from `ci/coverage.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub mode: Mode,
    pub minimum: f64,
    pub tolerance: f64,
    /// `(glob, reason)` pairs.
    pub exclude: Vec<(String, String)>,
}

/// What the minimum applies to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Workspace,
    Patch,
}

impl Config {
    /// Parses the configuration; every exclusion must carry a reason.
    pub fn parse(text: &str) -> Result<Config, String> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let gate = value.get("gate").ok_or("coverage.json: missing `gate`")?;
        let mode = match gate.get("mode").and_then(|m| m.as_str()) {
            Some("workspace") => Mode::Workspace,
            Some("patch") => Mode::Patch,
            other => return Err(format!("coverage.json: gate.mode is {other:?}")),
        };
        let minimum = gate
            .get("minimum_lines")
            .and_then(|m| m.as_f64())
            .ok_or("coverage.json: gate.minimum_lines is missing")?;
        let tolerance = gate
            .get("ratchet_tolerance")
            .and_then(|m| m.as_f64())
            .unwrap_or(0.5);
        let mut exclude = Vec::new();
        for entry in value
            .get("exclude")
            .and_then(|e| e.as_array())
            .ok_or("coverage.json: missing `exclude`")?
        {
            let glob = entry.get("glob").and_then(|g| g.as_str()).unwrap_or("");
            let reason = entry.get("reason").and_then(|g| g.as_str()).unwrap_or("");
            if glob.is_empty() || reason.trim().is_empty() {
                return Err(format!(
                    "coverage.json: exclusion {entry} needs a glob and a reason"
                ));
            }
            exclude.push((glob.to_string(), reason.to_string()));
        }
        Ok(Config {
            mode,
            minimum,
            tolerance,
            exclude,
        })
    }

    /// True when the file is excluded by a listed glob.
    pub fn excludes(&self, path: &str) -> bool {
        self.exclude.iter().any(|(glob, _)| glob_match(glob, path))
    }

    /// The `--ignore-filename-regex` value for `cargo llvm-cov`.
    pub fn ignore_regex(&self) -> String {
        self.exclude
            .iter()
            .map(|(glob, _)| glob_to_regex(glob))
            .collect::<Vec<_>>()
            .join("|")
    }
}

/// Line ranges (1-based, inclusive) of `#[cfg(test)] mod name { ... }` blocks.
pub fn test_module_ranges(source: &str) -> Vec<(u32, u32)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != "#[cfg(test)]" {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < lines.len() && lines[j].trim_start().starts_with("#[") {
            j += 1;
        }
        let is_mod = lines
            .get(j)
            .map(|l| {
                let l = l.trim_start();
                l.starts_with("mod ")
                    || l.starts_with("pub mod ")
                    || l.starts_with("pub(crate) mod ")
            })
            .unwrap_or(false);
        if !is_mod {
            i += 1;
            continue;
        }
        // Count braces from the module line to its closing brace.
        let mut depth: i64 = 0;
        let mut opened = false;
        let mut end = None;
        'scan: for (k, line) in lines.iter().enumerate().skip(j) {
            let chars: Vec<char> = line.chars().collect();
            let mut c = 0;
            while c < chars.len() {
                match chars[c] {
                    '/' if chars.get(c + 1) == Some(&'/') => break,
                    '"' => {
                        c += 1;
                        while c < chars.len() && chars[c] != '"' {
                            if chars[c] == '\\' {
                                c += 1;
                            }
                            c += 1;
                        }
                    }
                    '\'' => {
                        // A character literal like '{' or '\n'; lifetimes have no closing quote.
                        if chars.get(c + 2) == Some(&'\'') {
                            c += 2;
                        } else if chars.get(c + 1) == Some(&'\\') && chars.get(c + 3) == Some(&'\'')
                        {
                            c += 3;
                        }
                    }
                    '{' => {
                        depth += 1;
                        opened = true;
                    }
                    '}' => depth -= 1,
                    ';' if !opened => break 'scan,
                    _ => {}
                }
                c += 1;
            }
            if opened && depth <= 0 {
                end = Some(k);
                break;
            }
        }
        if let Some(end) = end {
            ranges.push((j as u32 + 1, end as u32 + 1));
            i = end + 1;
        } else {
            i = j + 1;
        }
    }
    ranges
}

/// Hit and found line counts.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Counts {
    pub found: u64,
    pub hit: u64,
}

impl Counts {
    /// Percentage of found lines that ran, or 100 when nothing is instrumented.
    pub fn percent(&self) -> f64 {
        if self.found == 0 {
            100.0
        } else {
            self.hit as f64 * 100.0 / self.found as f64
        }
    }
}

/// The crate a path belongs to: the folder under `crates/` or `apps/`.
pub fn crate_of(path: &str) -> String {
    let mut parts = path.split('/');
    match (parts.next(), parts.next()) {
        (Some("crates" | "apps"), Some(name)) => name.to_string(),
        _ => "other".to_string(),
    }
}

/// Applies the exclusions and the `#[cfg(test)]` module removal. `source_of`
/// returns a file's text, or `None` when it cannot be read.
pub fn filter(
    files: &BTreeMap<String, Lines>,
    config: &Config,
    source_of: &dyn Fn(&str) -> Option<String>,
) -> BTreeMap<String, Lines> {
    let mut out = BTreeMap::new();
    for (path, lines) in files {
        if config.excludes(path) || !path.ends_with(".rs") || path.starts_with('/') {
            continue;
        }
        let ranges = source_of(path)
            .map(|s| test_module_ranges(&s))
            .unwrap_or_default();
        let kept: Lines = lines
            .iter()
            .filter(|(n, _)| !ranges.iter().any(|(a, b)| **n >= *a && **n <= *b))
            .map(|(n, h)| (*n, *h))
            .collect();
        if !kept.is_empty() {
            out.insert(path.clone(), kept);
        }
    }
    out
}

/// Line counts per crate; the key `workspace` holds the total.
pub fn summarize(files: &BTreeMap<String, Lines>) -> BTreeMap<String, Counts> {
    let mut out: BTreeMap<String, Counts> = BTreeMap::new();
    for (path, lines) in files {
        let found = lines.len() as u64;
        let hit = lines.values().filter(|h| **h > 0).count() as u64;
        for key in [crate_of(path), "workspace".to_string()] {
            let counts = out.entry(key).or_default();
            counts.found += found;
            counts.hit += hit;
        }
    }
    out
}

/// Rounds to two decimals so the baseline file stays stable.
fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// The baseline as JSON text: `{"workspace": pct, "crates": {name: pct}}`.
pub fn baseline_json(summary: &BTreeMap<String, Counts>) -> String {
    let mut crates = serde_json::Map::new();
    for (name, counts) in summary {
        if name != "workspace" {
            crates.insert(name.clone(), serde_json::json!(round2(counts.percent())));
        }
    }
    let workspace = summary.get("workspace").map(Counts::percent).unwrap_or(0.0);
    let value = serde_json::json!({ "workspace": round2(workspace), "crates": crates });
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_default();
    text.push('\n');
    text
}

/// Reads per-crate percentages from baseline JSON text.
pub fn parse_baseline(text: &str) -> Result<BTreeMap<String, f64>, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let crates = value
        .get("crates")
        .and_then(|c| c.as_object())
        .ok_or("baseline: missing `crates`")?;
    Ok(crates
        .iter()
        .filter_map(|(k, v)| v.as_f64().map(|p| (k.clone(), p)))
        .collect())
}

/// Crates that fell more than `tolerance` points below their baseline.
pub fn ratchet(
    summary: &BTreeMap<String, Counts>,
    baseline: &BTreeMap<String, f64>,
    tolerance: f64,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, base) in baseline {
        match summary.get(name) {
            Some(counts) => {
                let now = counts.percent();
                if now < base - tolerance {
                    problems.push(format!(
                        "{name}: line coverage {now:.2}% fell below its baseline {base:.2}% (allowed drop {tolerance})"
                    ));
                }
            }
            None => problems.push(format!("{name}: in the baseline but not measured")),
        }
    }
    problems
}

/// Added line numbers per file from `git diff -U0` output.
pub fn added_lines(diff: &str) -> BTreeMap<String, BTreeSet<u32>> {
    let mut out: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    let mut file: Option<String> = None;
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ ") {
            file = path.strip_prefix("b/").map(str::to_string);
        } else if line.starts_with("@@") {
            let Some(plus) = line.split_whitespace().find(|p| p.starts_with('+')) else {
                continue;
            };
            let spec = plus.trim_start_matches('+');
            let mut parts = spec.split(',');
            let start = parts.next().and_then(|n| n.parse::<u32>().ok());
            let count = parts
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(1);
            if let (Some(file), Some(start)) = (&file, start) {
                out.entry(file.clone())
                    .or_default()
                    .extend(start..start + count);
            }
        }
    }
    out
}

/// Coverage of the changed lines that are instrumented.
pub fn patch_counts(
    files: &BTreeMap<String, Lines>,
    added: &BTreeMap<String, BTreeSet<u32>>,
) -> Counts {
    let mut counts = Counts::default();
    for (path, numbers) in added {
        let Some(lines) = files.get(path) else {
            continue;
        };
        for number in numbers {
            if let Some(hits) = lines.get(number) {
                counts.found += 1;
                if *hits > 0 {
                    counts.hit += 1;
                }
            }
        }
    }
    counts
}

/// The per-crate table as Markdown for the job summary.
pub fn markdown(
    summary: &BTreeMap<String, Counts>,
    baseline: &BTreeMap<String, f64>,
    patch: Option<Counts>,
) -> String {
    let mut out = String::from(
        "## Coverage\n\n| Crate | Lines | Covered | Line coverage | Baseline |\n| --- | ---: | ---: | ---: | ---: |\n",
    );
    for (name, counts) in summary.iter().filter(|(n, _)| *n != "workspace") {
        let base = baseline
            .get(name)
            .map(|b| format!("{b:.2}%"))
            .unwrap_or_else(|| "new".into());
        out.push_str(&format!(
            "| {name} | {} | {} | {:.2}% | {base} |\n",
            counts.found,
            counts.hit,
            counts.percent()
        ));
    }
    if let Some(total) = summary.get("workspace") {
        out.push_str(&format!(
            "| **workspace** | {} | {} | **{:.2}%** | |\n",
            total.found,
            total.hit,
            total.percent()
        ));
    }
    if let Some(p) = patch {
        out.push_str(&format!(
            "\nChanged lines: {} of {} instrumented lines covered ({:.2}%).\n",
            p.hit,
            p.found,
            p.percent()
        ));
    }
    out
}

/// Applies the configured minimum. `patch` is `None` when no base was given.
pub fn gate(
    config: &Config,
    summary: &BTreeMap<String, Counts>,
    patch: Option<Counts>,
) -> Vec<String> {
    let mut problems = Vec::new();
    match config.mode {
        Mode::Workspace => {
            let total = summary.get("workspace").map(Counts::percent).unwrap_or(0.0);
            if total < config.minimum {
                problems.push(format!(
                    "workspace line coverage {total:.2}% is below the minimum {}%",
                    config.minimum
                ));
            }
        }
        Mode::Patch => {
            if let Some(p) = patch
                && p.percent() < config.minimum
            {
                problems.push(format!(
                    "changed lines are {:.2}% covered ({} of {}), below the minimum {}%",
                    p.percent(),
                    p.hit,
                    p.found,
                    config.minimum
                ));
            }
        }
    }
    problems
}

/// `cargo xtask coverage [--bless] [--base REF] [--no-run]`.
pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let bless = args.iter().any(|a| a == "--bless");
    let no_run = args.iter().any(|a| a == "--no-run");
    let base = args
        .iter()
        .position(|a| a == "--base")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| {
            std::env::var("COVERAGE_BASE")
                .ok()
                .filter(|b| !b.is_empty())
        });
    let config_text = std::fs::read_to_string(root.join("ci/coverage.json"))
        .map_err(|e| format!("ci/coverage.json: {e}"))?;
    let config = Config::parse(&config_text)?;
    let out_dir = root.join("target/coverage");
    let lcov_path = out_dir.join("lcov.info");
    if !no_run {
        std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
        let version = std::process::Command::new("cargo")
            .args(["llvm-cov", "--version"])
            .output()
            .map_err(|e| e.to_string())?;
        if !String::from_utf8_lossy(&version.stdout).contains(LLVM_COV_VERSION) {
            return Err(format!(
                "cargo-llvm-cov {LLVM_COV_VERSION} is required: cargo install --locked cargo-llvm-cov --version ={LLVM_COV_VERSION}, and rustup component add llvm-tools"
            ));
        }
        let regex = config.ignore_regex();
        crate::cargo(&[
            "llvm-cov",
            "nextest",
            "--workspace",
            "--locked",
            "--no-fail-fast",
            "--no-report",
            "--ignore-filename-regex",
            &regex,
        ])?;
        let lcov = lcov_path.to_string_lossy().to_string();
        crate::cargo(&["llvm-cov", "report", "--lcov", "--output-path", &lcov])?;
        let html = out_dir.join("html").to_string_lossy().to_string();
        crate::cargo(&["llvm-cov", "report", "--html", "--output-dir", &html])?;
    }
    let text =
        std::fs::read_to_string(&lcov_path).map_err(|e| format!("{}: {e}", lcov_path.display()))?;
    let root_str = root.to_string_lossy().to_string();
    let parsed = parse_lcov(&text, &root_str);
    let files = filter(&parsed, &config, &|p| {
        std::fs::read_to_string(root.join(p)).ok()
    });
    let summary = summarize(&files);
    let baseline_path = root.join("ci/coverage-baseline.json");
    let baseline = match std::fs::read_to_string(&baseline_path) {
        Ok(text) => parse_baseline(&text)?,
        Err(_) => BTreeMap::new(),
    };
    let patch = match &base {
        Some(base) => {
            let range = format!("{base}...HEAD");
            let diff = std::process::Command::new("git")
                .args(["diff", "-U0", "--no-color", &range])
                .current_dir(root)
                .output()
                .map_err(|e| e.to_string())?;
            if !diff.status.success() {
                return Err(format!("git diff {range} failed"));
            }
            Some(patch_counts(
                &files,
                &added_lines(&String::from_utf8_lossy(&diff.stdout)),
            ))
        }
        None => None,
    };
    let table = markdown(&summary, &baseline, patch);
    println!("{table}");
    std::fs::write(out_dir.join("summary.md"), &table).map_err(|e| e.to_string())?;
    if bless {
        std::fs::write(&baseline_path, baseline_json(&summary)).map_err(|e| e.to_string())?;
        println!("Baseline rewritten: ci/coverage-baseline.json");
        return Ok(());
    }
    let mut problems = gate(&config, &summary, patch);
    problems.extend(ratchet(&summary, &baseline, config.tolerance));
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LCOV: &str = "TN:\nSF:/w/crates/geom/src/a.rs\nDA:1,3\nDA:2,0\nDA:3,1\nLF:3\nLH:2\nend_of_record\nSF:/w/crates/doc/src/b.rs\nDA:1,0\nDA:1,2\nDA:5,0\nend_of_record\nSF:/w/apps/pcbcraft/src/main.rs\nDA:1,0\nend_of_record\n";

    fn config(mode: &str) -> Result<Config, String> {
        Config::parse(&format!(
            "{{\"gate\":{{\"mode\":\"{mode}\",\"minimum_lines\":70}},\"exclude\":[{{\"glob\":\"apps/*/src/main.rs\",\"reason\":\"glue\"}}]}}"
        ))
    }

    #[test]
    fn lcov_lines_are_read_per_file_and_repeats_add_up() {
        let files = parse_lcov(LCOV, "/w");
        assert_eq!(files["crates/geom/src/a.rs"].len(), 3);
        assert_eq!(files["crates/doc/src/b.rs"][&1], 2);
        assert_eq!(files["crates/doc/src/b.rs"][&5], 0);
    }

    #[test]
    fn exclusions_need_a_reason_and_match_globs() -> Result<(), String> {
        assert!(Config::parse("{\"gate\":{\"mode\":\"patch\",\"minimum_lines\":70},\"exclude\":[{\"glob\":\"x\",\"reason\":\" \"}]}").is_err());
        let cfg = config("workspace")?;
        assert!(cfg.excludes("apps/pcbcraft/src/main.rs"));
        assert!(!cfg.excludes("apps/pcbcraft/src/host.rs"));
        assert_eq!(cfg.ignore_regex(), "apps/.*/src/main\\.rs");
        Ok(())
    }

    #[test]
    fn per_crate_and_workspace_percentages() -> Result<(), String> {
        let files = filter(&parse_lcov(LCOV, "/w"), &config("workspace")?, &|_| None);
        let summary = summarize(&files);
        assert_eq!(summary["geom"], Counts { found: 3, hit: 2 });
        assert_eq!(summary["doc"], Counts { found: 2, hit: 1 });
        assert_eq!(summary["workspace"], Counts { found: 5, hit: 3 });
        assert!(!summary.contains_key("pcbcraft"));
        assert!((summary["workspace"].percent() - 60.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn test_modules_are_removed_by_reading_the_source() -> Result<(), String> {
        let source = "fn a() {}\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = \"}\"; let _ = '{'; }\n}\nfn b() {}\n";
        assert_eq!(test_module_ranges(source), vec![(3, 5)]);
        let mut files = BTreeMap::new();
        files.insert(
            "crates/geom/src/a.rs".to_string(),
            Lines::from([(1, 1), (4, 1), (6, 0)]),
        );
        let kept = filter(&files, &config("workspace")?, &|_| Some(source.to_string()));
        assert_eq!(
            kept["crates/geom/src/a.rs"]
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![1, 6]
        );
        Ok(())
    }

    #[test]
    fn a_file_module_declaration_is_not_a_block() {
        assert!(test_module_ranges("#[cfg(test)]\nmod tests;\nfn a() {}\n").is_empty());
    }

    #[test]
    fn ratchet_allows_the_tolerance_and_flags_bigger_drops() {
        let summary = BTreeMap::from([
            (
                "geom".to_string(),
                Counts {
                    found: 100,
                    hit: 80,
                },
            ),
            (
                "doc".to_string(),
                Counts {
                    found: 100,
                    hit: 50,
                },
            ),
        ]);
        let baseline = BTreeMap::from([
            ("geom".to_string(), 80.4),
            ("doc".to_string(), 51.0),
            ("gone".to_string(), 10.0),
        ]);
        let problems = ratchet(&summary, &baseline, 0.5);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].starts_with("doc:"));
        assert!(problems[1].starts_with("gone:"));
    }

    #[test]
    fn baseline_round_trips() -> Result<(), String> {
        let summary = BTreeMap::from([
            ("geom".to_string(), Counts { found: 3, hit: 2 }),
            ("workspace".to_string(), Counts { found: 3, hit: 2 }),
        ]);
        let parsed = parse_baseline(&baseline_json(&summary))?;
        assert_eq!(parsed, BTreeMap::from([("geom".to_string(), 66.67)]));
        Ok(())
    }

    #[test]
    fn diff_hunks_give_added_line_numbers() {
        let diff = "diff --git a/x b/x\n+++ b/crates/geom/src/a.rs\n@@ -1,0 +2,2 @@\n+a\n+b\n@@ -9 +12 @@\n+c\n+++ b/README.md\n@@ -1 +1,0 @@\n";
        let added = added_lines(diff);
        assert_eq!(
            added["crates/geom/src/a.rs"]
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![2, 3, 12]
        );
        assert!(added["README.md"].is_empty());
    }

    #[test]
    fn patch_coverage_counts_only_instrumented_changed_lines() -> Result<(), String> {
        let files = filter(&parse_lcov(LCOV, "/w"), &config("patch")?, &|_| None);
        let diff = "+++ b/crates/geom/src/a.rs\n@@ -0,0 +1,4 @@\n+++ b/crates/doc/src/b.rs\n@@ -0,0 +5 @@\n";
        let counts = patch_counts(&files, &added_lines(diff));
        // geom lines 1..=3 are instrumented (2 hit); line 4 is not; doc line 5 is not hit.
        assert_eq!(counts, Counts { found: 4, hit: 2 });
        let cfg = config("patch")?;
        assert_eq!(gate(&cfg, &summarize(&files), Some(counts)).len(), 1);
        assert!(gate(&cfg, &summarize(&files), Some(Counts { found: 10, hit: 7 })).is_empty());
        assert!(gate(&cfg, &summarize(&files), None).is_empty());
        Ok(())
    }

    #[test]
    fn workspace_mode_fails_under_the_minimum() -> Result<(), String> {
        let files = filter(&parse_lcov(LCOV, "/w"), &config("workspace")?, &|_| None);
        let problems = gate(&config("workspace")?, &summarize(&files), None);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("60.00%"));
        Ok(())
    }
}
