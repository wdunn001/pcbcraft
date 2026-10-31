//! `cargo xtask test-standards`: the mechanical part of docs/testing-standard.md.
//!
//! Reads every Rust file under `crates/` and `apps/` and reports, per rule:
//! a `#[test]` that asserts nothing, an `#[ignore]` without a reason and an
//! issue number, a clock, sleep, network or real user-folder use in test code,
//! an unseeded random source in test code, and any golden-file variable other
//! than `PCBCRAFT_BLESS`. The first four rules are ratcheted against
//! `ci/test-standards-baseline.txt` (a count per rule and file may only fall);
//! the bless rule has no baseline.
//!
//! # Rules
//! - A rule is a plain token or shape check over text; nothing here compiles code.
//! - `--bless` rewrites the baseline; it never adds a bless-variable exemption.
//!
//! # Dependencies
//! - docs/testing-standard.md
//! - ci/test-standards-baseline.txt
//! - xtask/src/coverage.rs
//!
//! # Do not
//! - Add a token to the deny list without a line in docs/testing-standard.md.
use crate::coverage::{block_end, test_module_ranges};
use std::collections::BTreeMap;
use std::path::Path;

/// Tokens that make a test depend on the wall clock, the network or the
/// person's real folders.
const DENY: [(&str, &str); 14] = [
    ("Instant::now", "clock"),
    ("SystemTime::now", "clock"),
    ("thread::sleep", "sleep"),
    ("TcpStream::connect", "network"),
    ("reqwest::", "network"),
    ("ureq::", "network"),
    ("dirs::config_dir", "user folder"),
    ("dirs::home_dir", "user folder"),
    ("config_dir()", "user folder"),
    ("env::var(\"HOME\")", "user folder"),
    ("env::var(\"APPDATA\")", "user folder"),
    ("thread_rng", "unseeded random"),
    ("from_entropy", "unseeded random"),
    ("rand::random", "unseeded random"),
];

/// One reported problem.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub rule: &'static str,
    pub path: String,
    pub line: u32,
    pub text: String,
}

/// True for files that hold only test code.
pub fn is_test_file(path: &str) -> bool {
    path.contains("/tests/")
        || path.contains("/src/e2e/")
        || path.ends_with("/tests.rs")
        || path.ends_with("_tests.rs")
        || path.starts_with("crates/testkit/")
}

/// Checks one file's text. `path` is relative to the repository root.
pub fn scan(path: &str, source: &str) -> Vec<Finding> {
    let lines: Vec<&str> = source.lines().collect();
    let ranges = test_module_ranges(source);
    let in_test = |n: u32| is_test_file(path) || ranges.iter().any(|(a, b)| n >= *a && n <= *b);
    let mut out = Vec::new();
    let mut push = |rule: &'static str, n: usize, text: &str| {
        out.push(Finding {
            rule,
            path: path.to_string(),
            line: n as u32 + 1,
            text: text.trim().to_string(),
        });
    };
    for (n, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.contains("env::var") {
            let other = ["BLESS", "GOLDEN", "UPDATE_"]
                .iter()
                .any(|k| trimmed.contains(k));
            if other && !trimmed.contains("\"PCBCRAFT_BLESS\"") {
                push("bless", n, line);
            }
        }
        if trimmed.starts_with("#[ignore") {
            let reason_ok = trimmed.contains("= \"")
                && trimmed
                    .split('#')
                    .skip(2)
                    .any(|rest| rest.starts_with(|c: char| c.is_ascii_digit()));
            if !reason_ok {
                push("ignore", n, line);
            }
        }
        if in_test(n as u32 + 1) {
            for (token, _) in DENY {
                if trimmed.contains(token) && !trimmed.contains("\"") {
                    push("environment", n, line);
                    break;
                }
            }
        }
        if trimmed.starts_with("#[test]") {
            let mut j = n + 1;
            let mut should_panic = false;
            while j < lines.len() && lines[j].trim_start().starts_with("#[") {
                should_panic |= lines[j].contains("should_panic");
                j += 1;
            }
            let Some(end) = block_end(&lines, j) else {
                continue;
            };
            let body = lines[j..=end].join("\n");
            let asserts = body.contains("assert")
                || body.contains(".expect(")
                || body.contains(".unwrap(")
                || body.contains('?')
                || should_panic;
            if !asserts {
                push("no-assert", j, lines[j]);
            }
        }
    }
    out
}

/// Counts findings per `(rule, path)`.
pub fn counts(findings: &[Finding]) -> BTreeMap<(String, String), u32> {
    let mut map = BTreeMap::new();
    for f in findings {
        *map.entry((f.rule.to_string(), f.path.clone())).or_insert(0) += 1;
    }
    map
}

/// Parses the baseline: `rule | path | count` per line, `#` comments allowed.
pub fn parse_baseline(text: &str) -> Result<BTreeMap<(String, String), u32>, String> {
    let mut map = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        let count = parts.get(2).and_then(|c| c.parse::<u32>().ok());
        match (parts.first(), parts.get(1), count) {
            (Some(rule), Some(path), Some(count)) if parts.len() == 3 => {
                map.insert((rule.to_string(), path.to_string()), count);
            }
            _ => return Err(format!("test-standards baseline line {}: `{line}`", n + 1)),
        }
    }
    Ok(map)
}

/// Writes the baseline text for the ratcheted rules.
pub fn baseline_text(findings: &[Finding]) -> String {
    let mut out = String::from(
        "# Existing breaches of docs/testing-standard.md. A count may fall, never rise.\n# rule | file | count\n",
    );
    for ((rule, path), count) in counts(findings) {
        if rule != "bless" {
            out.push_str(&format!("{rule} | {path} | {count}\n"));
        }
    }
    out
}

/// Problems that are not covered by the baseline.
pub fn judge(findings: &[Finding], baseline: &BTreeMap<(String, String), u32>) -> Vec<String> {
    let mut problems = Vec::new();
    let now = counts(findings);
    for f in findings.iter().filter(|f| f.rule == "bless") {
        problems.push(format!(
            "{}:{}: reads a golden-file variable other than PCBCRAFT_BLESS: {}",
            f.path, f.line, f.text
        ));
    }
    for ((rule, path), count) in &now {
        if rule == "bless" {
            continue;
        }
        let allowed = baseline
            .get(&(rule.clone(), path.clone()))
            .copied()
            .unwrap_or(0);
        if *count > allowed {
            for f in findings
                .iter()
                .filter(|f| f.rule == rule && &f.path == path)
            {
                problems.push(format!("{}:{}: {rule}: {}", f.path, f.line, f.text));
            }
            problems.push(format!(
                "{path}: {count} `{rule}` findings, the baseline allows {allowed}"
            ));
        }
    }
    problems
}

/// Collects `.rs` files below `dir` as paths relative to `root`.
fn files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            files(root, &path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs")
            && let Ok(rel) = path.strip_prefix(root)
        {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// `cargo xtask test-standards [--bless]`.
pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let mut paths = Vec::new();
    for top in ["crates", "apps"] {
        files(root, &root.join(top), &mut paths)?;
    }
    let mut findings = Vec::new();
    for path in &paths {
        let source =
            std::fs::read_to_string(root.join(path)).map_err(|e| format!("{path}: {e}"))?;
        findings.extend(scan(path, &source));
    }
    let baseline_path = root.join("ci/test-standards-baseline.txt");
    if args.iter().any(|a| a == "--bless") {
        std::fs::write(&baseline_path, baseline_text(&findings)).map_err(|e| e.to_string())?;
        println!("Baseline rewritten: ci/test-standards-baseline.txt");
        return Ok(());
    }
    let baseline = match std::fs::read_to_string(&baseline_path) {
        Ok(text) => parse_baseline(&text)?,
        Err(_) => BTreeMap::new(),
    };
    let problems = judge(&findings, &baseline);
    println!(
        "test-standards: {} files, {} findings, {} over the baseline",
        paths.len(),
        findings.len(),
        problems.len()
    );
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(path: &str, source: &str) -> Vec<&'static str> {
        scan(path, source).iter().map(|f| f.rule).collect()
    }

    #[test]
    fn a_test_with_no_assertion_is_reported() {
        let src = "#[test]\nfn runs_only() {\n    let _ = 1 + 1;\n}\n#[test]\nfn checks() {\n    assert_eq!(1, 1);\n}\n#[test]\nfn returns() -> Result<(), String> {\n    Err::<(), String>(String::new())?;\n    Ok(())\n}\n#[test]\n#[should_panic]\nfn panics() {\n    boom();\n}\n";
        let found = scan("crates/a/src/lib.rs", src);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, "no-assert");
        assert_eq!(found[0].line, 2);
    }

    #[test]
    fn ignore_needs_a_reason_and_an_issue_number() {
        let bad = "#[ignore]\n#[ignore = \"needs a display\"]\n";
        assert_eq!(rules("crates/a/src/lib.rs", bad), vec!["ignore", "ignore"]);
        let good = "#[ignore = \"flaky: #517\"]\n";
        assert!(rules("crates/a/src/lib.rs", good).is_empty());
    }

    #[test]
    fn clocks_sleeps_and_real_folders_are_denied_in_test_code_only() {
        let src = "fn real() { let t = Instant::now(); }\n#[cfg(test)]\nmod tests {\n    fn t() { std::thread::sleep(d); }\n}\n";
        let found = scan("crates/a/src/lib.rs", src);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].line, 4);
        assert_eq!(
            rules(
                "crates/a/tests/x.rs",
                "fn t() { let h = dirs::home_dir(); }\n"
            ),
            vec!["environment"]
        );
    }

    #[test]
    fn any_other_golden_variable_is_reported() {
        let src = "let a = std::env::var(\"UPDATE_GOLDEN\");\nlet b = std::env::var_os(\"PCBCRAFT_BLESS\");\nlet c = std::env::var(\"BLESS_GOLDEN\");\n";
        let found = scan("crates/a/src/lib.rs", src);
        assert_eq!(found.iter().map(|f| f.line).collect::<Vec<_>>(), vec![1, 3]);
        assert!(judge(&found, &BTreeMap::new()).len() == 2);
    }

    #[test]
    fn the_baseline_lets_old_findings_stay_but_not_grow() -> Result<(), String> {
        let src = "#[ignore]\n#[ignore]\n";
        let found = scan("crates/a/src/lib.rs", src);
        let text = baseline_text(&found);
        let baseline = parse_baseline(&text)?;
        assert!(judge(&found, &baseline).is_empty());
        let more = scan("crates/a/src/lib.rs", "#[ignore]\n#[ignore]\n#[ignore]\n");
        assert!(!judge(&more, &baseline).is_empty());
        assert!(judge(&scan("crates/a/src/lib.rs", "#[ignore]\n"), &baseline).is_empty());
        Ok(())
    }

    #[test]
    fn a_bad_baseline_line_is_an_error() {
        assert!(parse_baseline("ignore | a.rs").is_err());
        assert!(parse_baseline("# note\n\nignore | a.rs | 2\n").is_ok());
    }
}
