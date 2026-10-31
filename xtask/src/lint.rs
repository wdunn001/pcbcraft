//! The cheap first stage of CI behind `cargo xtask lint`, and the pure checks
//! it needs: committed conflict markers and the structure of workflow files.
//! The other steps (`fmt`, `policy`, `layers`, `docs check`, `strings check`)
//! are the existing subcommands; this module only runs them in order and
//! reports one result per step.
//!
//! # Rules
//! - The workflow check is a small structural reader, not a YAML library: it
//!   rejects tabs, duplicate keys in one mapping, unknown top-level keys, jobs
//!   without `runs-on` or `uses`, and `needs:` entries that name no job.
//! - `--json` prints, as the last line of output, an array of
//!   `{"step", "ok", "seconds"}` objects, one per step, in run order.
//! - Every step runs even after a failure, so one run names all of them.
//!
//! # Dependencies
//! - xtask/src/main.rs
//! - .github/workflows/ci.yml
//! - docs/ci.md
//!
//! # Do not
//! - Add a YAML dependency; the structural reader is enough for workflow files.
//! - Reimplement a check that already has a subcommand.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// Keys a workflow file may have at its top level.
const TOP_KEYS: [&str; 8] = [
    "name",
    "run-name",
    "on",
    "permissions",
    "env",
    "defaults",
    "concurrency",
    "jobs",
];

/// Keys a job may have.
const JOB_KEYS: [&str; 16] = [
    "name",
    "needs",
    "if",
    "runs-on",
    "permissions",
    "outputs",
    "env",
    "steps",
    "strategy",
    "timeout-minutes",
    "container",
    "services",
    "uses",
    "with",
    "secrets",
    "concurrency",
];

/// Also allowed on a job; kept apart so the arrays above stay short.
const JOB_KEYS_MORE: [&str; 2] = ["continue-on-error", "defaults"];

/// The result of one lint step.
pub struct StepResult {
    pub step: String,
    pub ok: bool,
    pub seconds: f64,
}

/// What the structural reader found in one workflow file.
#[derive(Debug, Default)]
pub struct Workflow {
    /// Job id to the ids it lists under `needs:`.
    pub jobs: BTreeMap<String, Vec<String>>,
}

/// Renders step results as the JSON array printed by `--json`.
pub fn to_json(results: &[StepResult]) -> String {
    let items: Vec<String> = results
        .iter()
        .map(|r| {
            format!(
                "{{\"step\":\"{}\",\"ok\":{},\"seconds\":{:.2}}}",
                r.step, r.ok, r.seconds
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// Runs one named step, timing it and printing its failure message.
pub fn timed(name: &str, step: impl FnOnce() -> Result<(), String>) -> StepResult {
    println!("== lint: {name}");
    let started = Instant::now();
    let outcome = step();
    if let Err(message) = &outcome {
        eprintln!("lint: {name} failed:\n{message}");
    }
    StepResult {
        step: name.to_string(),
        ok: outcome.is_ok(),
        seconds: started.elapsed().as_secs_f64(),
    }
}

/// Fails when a tracked file holds a line that starts with a conflict marker.
pub fn conflict_markers(root: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["grep", "-nE", "^(<<<<<<<|>>>>>>>) "])
        .output()
        .map_err(|e| format!("git grep: {e}"))?;
    match output.status.code() {
        Some(1) => Ok(()),
        Some(0) => Err(format!(
            "committed conflict markers:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )),
        _ => Err(format!(
            "git grep failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )),
    }
}

/// Checks every workflow file under `.github/workflows`.
pub fn workflows(root: &Path) -> Result<(), String> {
    let dir = root.join(".github/workflows");
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut problems = Vec::new();
    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.ends_with(".yml") || name.ends_with(".yaml")) {
            continue;
        }
        count += 1;
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                if let Err(e) = check_workflow(&text) {
                    problems.push(format!("{name}: {e}"));
                }
            }
            Err(e) => problems.push(format!("{name}: {e}")),
        }
    }
    if problems.is_empty() {
        println!("workflows: {count} files parse and every needs names a job");
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// One mapping on the indentation stack.
struct Frame {
    indent: usize,
    keys: BTreeSet<String>,
    last_key: String,
}

/// A key found on a line: its text and the value after the colon.
fn split_key(text: &str) -> Option<(String, String)> {
    let text = text.trim_end();
    if let Some(rest) = text.strip_prefix('"').or_else(|| text.strip_prefix('\'')) {
        let quote = text.chars().next()?;
        let end = rest.find(quote)?;
        let after = rest[end + 1..].strip_prefix(':')?;
        if after.is_empty() || after.starts_with(' ') {
            return Some((rest[..end].to_string(), after.trim().to_string()));
        }
        return None;
    }
    if text.starts_with('#') {
        return None;
    }
    let at = if let Some(i) = text.find(": ") {
        i
    } else if text.ends_with(':') {
        text.len() - 1
    } else {
        return None;
    };
    let key = &text[..at];
    if key.contains(" #") || key.starts_with(['[', '{', '$']) {
        return None;
    }
    Some((key.to_string(), text[at + 1..].trim().to_string()))
}

/// Value of `needs:` written on one line: `a` or `[a, b]`.
fn inline_list(value: &str) -> Vec<String> {
    value
        .trim_matches(['[', ']'])
        .split(',')
        .map(|s| s.trim().trim_matches(['"', '\'']).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Reads a workflow file's structure and checks it. See the module rules.
pub fn check_workflow(text: &str) -> Result<Workflow, String> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut workflow = Workflow::default();
    let mut job_keys: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut block_indent: Option<usize> = None;
    let mut needs_job: Option<String> = None;
    let mut problems = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim_end();
        let content = line.trim_start();
        if content.is_empty() {
            continue;
        }
        let lead = &line[..line.len() - content.len()];
        if lead.contains('\t') {
            problems.push(format!("line {number}: tab in indentation"));
            continue;
        }
        let mut indent = lead.len();
        if let Some(block) = block_indent {
            if indent > block {
                continue;
            }
            block_indent = None;
        }
        if content.starts_with('#') {
            continue;
        }
        let mut rest = content;
        let mut item = false;
        while let Some(after) = rest.strip_prefix("- ") {
            item = true;
            indent += 2;
            rest = after.trim_start();
        }
        if rest == "-" {
            continue;
        }
        let Some((key, value)) = split_key(rest) else {
            if !item && stack.last().is_some_and(|f| indent <= f.indent) {
                problems.push(format!("line {number}: unexpected text, not indented under its key"));
            }
            if item && stack.len() == 3 {
                if let Some(job) = &needs_job {
                    let name = rest.trim_matches(['"', '\'']).to_string();
                    workflow.jobs.entry(job.clone()).or_default().push(name);
                }
            }
            continue;
        };
        if item {
            while stack.last().is_some_and(|f| f.indent >= indent) {
                stack.pop();
            }
            stack.push(Frame {
                indent,
                keys: BTreeSet::new(),
                last_key: String::new(),
            });
        } else {
            while stack.last().is_some_and(|f| f.indent > indent) {
                stack.pop();
            }
            if stack.last().is_none_or(|f| f.indent < indent) {
                stack.push(Frame {
                    indent,
                    keys: BTreeSet::new(),
                    last_key: String::new(),
                });
            }
        }
        let depth = stack.len();
        let in_jobs = depth >= 2 && stack[0].last_key == "jobs";
        let Some(frame) = stack.last_mut() else {
            continue;
        };
        if !frame.keys.insert(key.clone()) {
            problems.push(format!("line {number}: duplicate key `{key}`"));
        }
        frame.last_key = key.clone();
        if depth == 1 && !TOP_KEYS.contains(&key.as_str()) {
            problems.push(format!(
                "line {number}: `{key}` is not a workflow key (bad indentation?)"
            ));
        }
        if in_jobs && depth == 2 {
            workflow.jobs.entry(key.clone()).or_default();
            job_keys.entry(key.clone()).or_default();
            needs_job = None;
        } else if in_jobs && depth == 3 {
            let job = stack[1].last_key.clone();
            if !JOB_KEYS.contains(&key.as_str()) && !JOB_KEYS_MORE.contains(&key.as_str()) {
                problems.push(format!(
                    "line {number}: job `{job}` has unknown key `{key}`"
                ));
            }
            job_keys.entry(job.clone()).or_default().insert(key.clone());
            needs_job = None;
            if key == "needs" {
                if value.is_empty() {
                    needs_job = Some(job);
                } else {
                    workflow
                        .jobs
                        .entry(job)
                        .or_default()
                        .extend(inline_list(&value));
                }
            }
        }
        let value_start = value.chars().next();
        if matches!(value_start, Some('|') | Some('>')) {
            block_indent = Some(indent);
        }
    }
    for (job, keys) in &job_keys {
        if !keys.contains("runs-on") && !keys.contains("uses") {
            problems.push(format!("job `{job}` has neither runs-on nor uses"));
        }
    }
    for (job, needs) in &workflow.jobs {
        for need in needs {
            if !workflow.jobs.contains_key(need) {
                problems.push(format!("job `{job}` needs `{need}`, which is not a job"));
            }
        }
    }
    if workflow.jobs.is_empty() {
        problems.push("no jobs found".to_string());
    }
    if problems.is_empty() {
        Ok(workflow)
    } else {
        Err(problems.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "name: CI\non:\n  push:\n    branches: [main]\n  merge_group:\njobs:\n  a:\n    runs-on: x\n    steps:\n      - run: |\n          echo hi\n          x: y\n  b:\n    needs: [a]\n    runs-on: x\n  c:\n    needs:\n      - a\n      - b\n    runs-on: x\n";

    #[test]
    fn a_good_workflow_lists_its_jobs_and_needs() {
        let flow = check_workflow(GOOD).unwrap_or_default();
        assert_eq!(flow.jobs.len(), 3);
        assert_eq!(flow.jobs["c"], vec!["a".to_string(), "b".to_string()]);
        assert_eq!(flow.jobs["b"], vec!["a".to_string()]);
    }

    #[test]
    fn a_duplicate_trigger_key_is_rejected() {
        let bad = GOOD.replace("  merge_group:\n", "  merge_group:\n  merge_group:\n");
        let error = check_workflow(&bad).err().unwrap_or_default();
        assert!(error.contains("duplicate key `merge_group`"), "{error}");
    }

    #[test]
    fn an_unindented_trigger_is_rejected() {
        let bad = GOOD.replace("  push:\n", "push:\n");
        let error = check_workflow(&bad).err().unwrap_or_default();
        assert!(error.contains("`push` is not a workflow key"), "{error}");
    }

    #[test]
    fn a_needs_naming_no_job_is_rejected() {
        let bad = GOOD.replace("needs: [a]", "needs: [zzz]");
        let error = check_workflow(&bad).err().unwrap_or_default();
        assert!(error.contains("needs `zzz`"), "{error}");
    }

    #[test]
    fn a_job_without_runs_on_is_rejected() {
        let bad = GOOD.replace(
            "  b:\n    needs: [a]\n    runs-on: x\n",
            "  b:\n    needs: [a]\n",
        );
        let error = check_workflow(&bad).err().unwrap_or_default();
        assert!(error.contains("job `b` has neither"), "{error}");
    }

    #[test]
    fn a_block_scalar_line_left_of_its_block_is_rejected() {
        let bad = GOOD.replace("          echo hi\n", "          echo hi\necho left\n");
        let error = check_workflow(&bad).err().unwrap_or_default();
        assert!(error.contains("unexpected text"), "{error}");
    }

    #[test]
    fn a_tab_is_rejected() {
        let error = check_workflow("name: x\njobs:\n\ta: 1\n")
            .err()
            .unwrap_or_default();
        assert!(error.contains("tab in indentation"), "{error}");
    }

    #[test]
    fn json_lists_each_step_with_its_result() {
        let results = vec![
            StepResult {
                step: "fmt".into(),
                ok: true,
                seconds: 1.5,
            },
            StepResult {
                step: "layers".into(),
                ok: false,
                seconds: 0.25,
            },
        ];
        assert_eq!(
            to_json(&results),
            "[{\"step\":\"fmt\",\"ok\":true,\"seconds\":1.50},{\"step\":\"layers\",\"ok\":false,\"seconds\":0.25}]"
        );
    }
}
