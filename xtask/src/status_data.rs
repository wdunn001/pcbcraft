//! `cargo xtask status-data`: the implementation status the website renders.
//!
//! Context: the status page must be computed, never typed. This module reads a cached
//! snapshot of the tracker (issues, merged pull requests, CI runs, releases) from the
//! fork, the feature list in `site/src/data/features.yaml`, `ROADMAP.md` and the Rust
//! sources, and writes `site/src/data/status.json`. The snapshot is the only network
//! product: `--refresh` rewrites it with `curl`, every other run works offline.
//!
//! # Rules
//! - The tracker is `wdunn001/pcbcraft` (`DEFAULT_REPO`). The old tracker is never read.
//! - A state is Built only with a merged pull request and a green run (the first
//!   completed green run on its merge commit or a later one); Released only with a
//!   release published after the merge. Everything else comes from the Tags line.
//! - A feature is `tested` only when a `test:` evidence name matches a `#[test]`
//!   function in the sources; a name that matches nothing is listed in `missing_tests`.
//! - Output is byte stable: no wall clock, sorted keys, sorted lists, one trailing newline.
//! - No internal host, address or path reaches the output: every string is scrubbed and
//!   a final guard fails the run if one survives.
//! - Unknown tags, a missing Tags line and unmatched references are warnings, never errors.
//!
//! # Dependencies
//! - site/src/data/status.schema.json
//! - site/src/data/features.yaml
//! - ROADMAP.md
//! - xtask/src/status_schema.rs
//! - xtask/tests/fixtures/status/snapshot.json
//!
//! # Do not
//! - Call the network from a test or from a run without `--refresh`.
//! - Count closed issues or promise dates; the page shows evidence only.
use crate::status_schema;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The tracker and code repository the status reads.
pub const DEFAULT_REPO: &str = "wdunn001/pcbcraft";
/// Cached API snapshot, not committed (CI caches it).
const SNAPSHOT_PATH: &str = "site/src/data/status-snapshot.json";
/// ETag cache for conditional requests, not committed.
const CACHE_PATH: &str = "site/src/data/status-cache.json";
/// Generated output, not committed.
const OUT_PATH: &str = "site/src/data/status.json";
/// The committed schema.
const SCHEMA_PATH: &str = "site/src/data/status.schema.json";
/// Feature list shared with the marketing pages.
const FEATURES_PATH: &str = "site/src/data/features.yaml";
/// Owner decision (coordinator default): merged items still tagged needs-owner-test show
/// as Built with `owner_tested: false`. Set to false to show them as awaiting-test.
const UNTESTED_SHOWS_AS_BUILT: bool = true;
/// Tag namespaces from TAGGING.md.
const NAMESPACES: [&str; 13] = [
    "track",
    "kind",
    "area",
    "milestone",
    "crate",
    "conflict",
    "size",
    "machine",
    "priority",
    "status",
    "stage",
    "research",
    "enrich",
];
/// Status values in use (TAGGING.md plus `review`, `split`, `built`, `ready`).
const STATUSES: [&str; 10] = [
    "open",
    "claimed",
    "in-progress",
    "pr-open",
    "needs-owner-test",
    "blocked",
    "review",
    "split",
    "built",
    "ready",
];

/// Cache of ETags and timestamps for conditional requests.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct EtagCache {
    /// ETag for the issues list (page 1).
    issues_etag: Option<String>,
    /// Per-issue updated_at timestamps for incremental updates.
    #[serde(default)]
    issue_timestamps: BTreeMap<u64, String>,
    /// Timestamp of the last successful full refresh.
    last_full: Option<String>,
    /// Timestamp of the last successful incremental update.
    last_incremental: Option<String>,
}

impl EtagCache {
    /// Load the cache from disk, or return a fresh empty cache.
    fn load(root: &Path) -> Result<Self, String> {
        let path = root.join(CACHE_PATH);
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Save the cache to disk.
    fn save(&self, root: &Path) -> Result<(), String> {
        let path = root.join(CACHE_PATH);
        let text = render(&self.to_value())?;
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn to_value(&self) -> Value {
        json!({
            "issues_etag": self.issues_etag,
            "issue_timestamps": self.issue_timestamps,
            "last_full": self.last_full,
            "last_incremental": self.last_incremental,
        })
    }

    /// Return the since parameter for incremental updates, if any.
    fn since(&self) -> Option<&str> {
        if let Some(ts) = self.last_incremental.as_deref() {
            return Some(ts);
        }
        self.last_full.as_deref()
    }
}
/// Delivery state of an issue, least advanced first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Planned,
    Ready,
    InProgress,
    AwaitingTest,
    Built,
    Released,
}

impl State {
    /// The word written to the JSON file.
    pub fn name(self) -> &'static str {
        match self {
            State::Planned => "planned",
            State::Ready => "ready",
            State::InProgress => "in-progress",
            State::AwaitingTest => "awaiting-test",
            State::Built => "built",
            State::Released => "released",
        }
    }
}

/// What the Tags line of an issue says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tags {
    /// Every tag in line order.
    pub all: Vec<String>,
    /// Tags outside the vocabulary.
    pub unknown: Vec<String>,
    /// The `status:` value, if any.
    pub status: Option<String>,
    /// The bare `ready` tag is present.
    pub ready: bool,
    /// The first `milestone:` value.
    pub milestone: Option<String>,
}

impl Tags {
    /// `status:blocked` is an overlay flag, never a state.
    pub fn blocked(&self) -> bool {
        self.status.as_deref() == Some("blocked") || self.all.iter().any(|t| t == "status:blocked")
    }
    /// The issue still waits for the owner to run it.
    pub fn owner_untested(&self) -> bool {
        self.all.iter().any(|t| t == "status:needs-owner-test")
    }
}

/// Merge and CI proof for one issue, as far as the snapshot shows.
#[derive(Clone, Copy, Debug, Default)]
pub struct Evidence {
    /// A pull request naming the issue is merged on main.
    pub merged: bool,
    /// The first completed run on that merge or a later main commit succeeded.
    pub green: bool,
    /// A release was published after the merge.
    pub released: bool,
}

/// Reads line 1 of an issue body. `None` means there is no Tags line.
///
/// # Rules
/// - Only the first line counts (TAGGING.md); tags are the backticked words after `**Tags:**`.
pub fn parse_tags(body: &str) -> Option<Tags> {
    let first = body.lines().next()?.trim();
    let rest = first.strip_prefix("**Tags:**")?;
    let mut tags = Tags::default();
    for part in rest.split('`').skip(1).step_by(2) {
        let tag = part.trim();
        if tag.is_empty() {
            continue;
        }
        tags.all.push(tag.to_string());
        match tag.split_once(':') {
            None if tag == "ready" => tags.ready = true,
            Some((ns, value)) if NAMESPACES.contains(&ns) => {
                if ns == "status" {
                    if STATUSES.contains(&value) {
                        tags.status = Some(value.to_string());
                    } else {
                        tags.unknown.push(tag.to_string());
                    }
                }
                if ns == "milestone" && tags.milestone.is_none() {
                    tags.milestone = Some(value.to_string());
                }
            }
            Some(("owner-decision", _)) => {}
            _ if tag == "owner-decision" => {}
            _ => tags.unknown.push(tag.to_string()),
        }
    }
    Some(tags)
}

/// Delivery state of one issue: the tags alone, lifted by evidence.
///
/// # Rules
/// - `built` and `released` come only from evidence; a `status:built` tag without it
///   reads as awaiting-test (and the run warns).
/// - `blocked` never changes the state.
pub fn state_of(tags: &Tags, evidence: &Evidence) -> State {
    if evidence.merged && evidence.green {
        if evidence.released {
            return State::Released;
        }
        if tags.owner_untested() && !UNTESTED_SHOWS_AS_BUILT {
            return State::AwaitingTest;
        }
        return State::Built;
    }
    match tags.status.as_deref() {
        Some("needs-owner-test") | Some("built") => State::AwaitingTest,
        Some("claimed") | Some("in-progress") | Some("pr-open") | Some("review") => {
            State::InProgress
        }
        Some("ready") => State::Ready,
        _ if tags.ready => State::Ready,
        _ => State::Planned,
    }
}

/// All `#N` numbers in a text, without parentheticals that point at epics or parents.
pub fn refs(text: &str) -> Vec<u64> {
    let mut kept = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        kept.push_str(&rest[..open]);
        let tail = &rest[open + 1..];
        let (inner, after) = match tail.find(')') {
            Some(close) => (&tail[..close], &tail[close + 1..]),
            None => (tail, ""),
        };
        let lower = inner.trim_start().to_lowercase();
        if !(lower.starts_with("epic")
            || lower.starts_with("child of")
            || lower.starts_with("parent"))
        {
            kept.push_str(inner);
        }
        kept.push(' ');
        rest = after;
    }
    kept.push_str(rest);
    all_refs(&kept)
}

/// Every `#N` (or `{{PCBREF:N}}`) in a text, in order, without duplicates.
fn all_refs(text: &str) -> Vec<u64> {
    let mut out = Vec::new();
    // Migrated issue bodies write references as {{PCBREF:N}} with the old number.
    let text = text.replace("{{PCBREF:", "#");
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '#' {
            let mut j = i + 1;
            let mut n = String::new();
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                n.push(bytes[j]);
                j += 1;
            }
            if let Ok(v) = n.parse::<u64>()
                && v > 0
                && !out.contains(&v)
            {
                out.push(v);
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// Turns a reference in old-tracker or fork numbering into a fork issue number.
struct Resolver {
    /// Old tracker number to fork number (from the "Migrated from" line).
    migrated: HashMap<u64, u64>,
    /// Fork numbers present in the snapshot.
    known: BTreeSet<u64>,
    /// References that matched an old number and a different fork number.
    ambiguous: BTreeSet<u64>,
}

impl Resolver {
    /// Old numbers win: merge messages written before the move use them.
    fn resolve(&mut self, n: u64) -> Option<u64> {
        match (self.migrated.get(&n).copied(), self.known.contains(&n)) {
            (Some(fork), true) if fork != n => {
                self.ambiguous.insert(n);
                Some(fork)
            }
            (Some(fork), _) => Some(fork),
            (None, true) => Some(n),
            (None, false) => None,
        }
    }
    /// Resolves a list, dropping what matches nothing and returning the dropped count.
    fn resolve_all(&mut self, list: &[u64]) -> (Vec<u64>, usize) {
        let mut out = Vec::new();
        let mut missed = 0;
        for n in list {
            match self.resolve(*n) {
                Some(v) => {
                    if !out.contains(&v) {
                        out.push(v);
                    }
                }
                None => missed += 1,
            }
        }
        out.sort_unstable();
        (out, missed)
    }
}

/// One merged pull request with its CI proof.
#[derive(Clone, Debug)]
struct Merge {
    pr: u64,
    title: String,
    url: String,
    sha: String,
    merged_at: String,
    ci: Option<Value>,
}

impl Merge {
    /// JSON form used inside issues and features.
    fn to_json(&self) -> Value {
        json!({"pr": self.pr, "title": self.title, "url": self.url, "merge_sha": self.sha,
               "merged_at": self.merged_at, "ci": self.ci.clone().unwrap_or(Value::Null)})
    }
}

/// The first green run on `sha` or, failing that, the earliest green run created at or
/// after `merged_at` (red and cancelled runs are common, so a later green run proves it).
fn first_green(runs: &[Value], sha: &str, merged_at: &str) -> Option<Value> {
    let green: Vec<&Value> = runs
        .iter()
        .filter(|r| r["conclusion"].as_str() == Some("success"))
        .collect();
    let exact = green
        .iter()
        .filter(|r| r["head_sha"].as_str() == Some(sha))
        .min_by(|a, b| a["created_at"].as_str().cmp(&b["created_at"].as_str()));
    let later = green
        .iter()
        .filter(|r| r["created_at"].as_str().unwrap_or("") >= merged_at)
        .min_by(|a, b| a["created_at"].as_str().cmp(&b["created_at"].as_str()));
    exact
        .or(later)
        .map(|r| json!({"url": r["url"], "head_sha": r["head_sha"], "created_at": r["created_at"]}))
}

/// Text field of a JSON object, empty when absent.
fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Entry of the feature list.
#[derive(Debug, Default, Clone)]
pub struct FeatureEntry {
    pub area: String,
    pub name: String,
    pub status: String,
    pub evidence: Vec<String>,
    pub issues: Vec<u64>,
}

/// Parses the flat `features.yaml` layout: `- area: x` starts an entry, two-space
/// `key: value` lines follow, lists are written `[a, b]`.
pub fn parse_features(text: &str) -> Vec<FeatureEntry> {
    let mut out: Vec<FeatureEntry> = Vec::new();
    let unquote = |v: &str| v.trim().trim_matches('"').trim_matches('\'').to_string();
    let list = |v: &str| -> Vec<String> {
        v.trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(unquote)
            .filter(|x| !x.is_empty())
            .collect()
    };
    for raw in text.lines() {
        if raw.trim_start().starts_with('#') || raw.trim().is_empty() {
            continue;
        }
        let (starts, body) = match raw.strip_prefix("- ") {
            Some(b) => (true, b),
            None => (false, raw.trim_start()),
        };
        if starts {
            out.push(FeatureEntry::default());
        }
        let Some((key, value)) = body.split_once(':') else {
            continue;
        };
        let Some(entry) = out.last_mut() else {
            continue;
        };
        match key.trim() {
            "area" => entry.area = unquote(value),
            "name" => entry.name = unquote(value),
            "status" => entry.status = unquote(value),
            "evidence" => entry.evidence = list(value),
            "issues" => {
                entry.issues = list(value)
                    .iter()
                    .filter_map(|v| v.trim_start_matches('#').parse().ok())
                    .collect()
            }
            _ => {}
        }
    }
    out
}

/// Lower-case ASCII slug used as a feature id.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// A checklist line of ROADMAP.md.
#[derive(Debug, Clone)]
pub struct RoadmapLine {
    pub milestone: String,
    pub text: String,
    pub checked: bool,
    pub subset: bool,
    pub refs: Vec<u64>,
}

/// Parses `## Mx: title` headings and `- [x]` / `- [ ]` lines under them.
pub fn parse_roadmap(text: &str) -> Vec<RoadmapLine> {
    let mut milestone = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            milestone = h.split(':').next().unwrap_or("").trim().to_string();
        } else if let Some(item) = line.strip_prefix("- [") {
            let checked = item.starts_with('x');
            let Some(body) = item.get(3..) else { continue };
            out.push(RoadmapLine {
                milestone: milestone.clone(),
                text: body.trim().to_string(),
                checked,
                subset: body.to_lowercase().contains("subset"),
                refs: all_refs(body),
            });
        }
    }
    out
}

/// Maps test function names to the file that defines them, scanning `.rs` files under
/// `crates`, `apps` and `xtask`. A name counts only with a test attribute just above it.
pub fn scan_tests(root: &Path) -> HashMap<String, String> {
    fn walk(dir: &Path, root: &Path, out: &mut HashMap<String, String>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for path in paths {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if name != "target" && name != "node_modules" && name != "fixtures" {
                    walk(&path, root, out);
                }
            } else if name.ends_with(".rs") {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let lines: Vec<&str> = text.lines().collect();
                for (i, line) in lines.iter().enumerate() {
                    let Some(after) = line.trim_start().strip_prefix("fn ") else {
                        continue;
                    };
                    let fname: String = after
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    let from = i.saturating_sub(3);
                    let is_test = lines
                        .get(from..i)
                        .map(|above| {
                            above
                                .iter()
                                .any(|l| l.contains("#[test]") || l.contains("::test]"))
                        })
                        .unwrap_or(false);
                    if is_test && !fname.is_empty() {
                        out.entry(fname).or_insert_with(|| rel.clone());
                    }
                }
            }
        }
    }
    let mut out = HashMap::new();
    for dir in ["crates", "apps", "xtask"] {
        walk(&root.join(dir), root, &mut out);
    }
    out
}

/// True when a whitespace-delimited token names an internal host, address or path.
fn is_internal(token: &str) -> bool {
    let t = token.to_lowercase();
    if [
        "/home/", "/srv/", "/users/", "/mnt/", "/tmp/", "@local", "quasarke", "c:\\", "~/",
    ]
    .iter()
    .any(|p| t.contains(p))
    {
        return true;
    }
    if [".lan", ".home", ".internal", ".local"]
        .iter()
        .any(|suffix| {
            t.trim_end_matches(|c: char| !c.is_ascii_alphanumeric())
                .ends_with(suffix)
        })
    {
        return true;
    }
    t.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .any(|piece| {
            let groups: Vec<&str> = piece.split('.').collect();
            groups.len() == 4 && groups.iter().all(|g| !g.is_empty() && g.len() <= 3)
        })
}

/// Replaces internal tokens in every string of the tree and returns how many it changed.
fn scrub(value: &mut Value) -> usize {
    match value {
        Value::String(text) => {
            if text.split_whitespace().any(is_internal) {
                let cleaned: Vec<&str> = text
                    .split_whitespace()
                    .map(|w| if is_internal(w) { "[removed]" } else { w })
                    .collect();
                *text = cleaned.join(" ");
                1
            } else {
                0
            }
        }
        Value::Array(list) => list.iter_mut().map(scrub).sum(),
        Value::Object(map) => map.values_mut().map(scrub).sum(),
        _ => 0,
    }
}

/// Builds the status document from a snapshot and the repository files. Pure: no I/O.
///
/// # Rules
/// - `generated_at` is the snapshot time, so two runs on one snapshot are identical.
/// - Warnings are summarised, one line per kind, never one line per merge.
///
/// # Dependencies
/// - site/src/data/status.schema.json (the shape of the result)
pub fn generate(
    snapshot: &Value,
    features_yaml: &str,
    roadmap_md: &str,
    tests: &HashMap<String, String>,
    stale: bool,
) -> Result<Value, String> {
    let empty = Vec::new();
    let list = |key: &str| {
        snapshot
            .get(key)
            .and_then(Value::as_array)
            .unwrap_or(&empty)
            .clone()
    };
    let runs = list("runs");
    let releases = list("releases");
    let first_release = releases
        .iter()
        .map(|r| s(r, "published_at").to_string())
        .filter(|t| !t.is_empty())
        .min();
    let latest_release = releases
        .iter()
        .max_by(|a, b| s(a, "published_at").cmp(s(b, "published_at")))
        .map(|r| s(r, "tag").to_string());
    let mut warnings: Vec<String> = Vec::new();

    // Issues from the snapshot; pull requests never reach it, but drop any that did.
    let raw_issues: Vec<Value> = list("issues")
        .into_iter()
        .filter(|i| i.get("pull_request").is_none())
        .collect();
    let mut resolver = Resolver {
        migrated: HashMap::new(),
        known: BTreeSet::new(),
        ambiguous: BTreeSet::new(),
    };
    for issue in &raw_issues {
        let n = issue["number"].as_u64().unwrap_or(0);
        resolver.known.insert(n);
        if let Some(old) = issue["migrated_from"].as_u64() {
            resolver.migrated.insert(old, n);
        }
    }

    // Merged pull requests and their issue references.
    let mut pulls: Vec<Value> = list("pulls")
        .into_iter()
        .filter(|p| !s(p, "merged_at").is_empty())
        .collect();
    pulls.sort_by(|a, b| {
        (s(a, "merged_at"), a["number"].as_u64()).cmp(&(s(b, "merged_at"), b["number"].as_u64()))
    });
    let mut merges_by_issue: BTreeMap<u64, Vec<Merge>> = BTreeMap::new();
    let (mut unnamed, mut unmatched) = (0usize, 0usize);
    for pull in &pulls {
        let mut numbers = refs(s(pull, "issues_line").trim_start_matches("Issues:"));
        if numbers.is_empty() {
            numbers = all_refs(s(pull, "title"));
        }
        let (resolved, missed) = resolver.resolve_all(&numbers);
        unmatched += missed;
        if resolved.is_empty() {
            unnamed += 1;
            continue;
        }
        let merge = Merge {
            pr: pull["number"].as_u64().unwrap_or(0),
            title: s(pull, "title").to_string(),
            url: s(pull, "url").to_string(),
            sha: s(pull, "merge_sha").to_string(),
            merged_at: s(pull, "merged_at").to_string(),
            ci: first_green(&runs, s(pull, "merge_sha"), s(pull, "merged_at")),
        };
        for n in resolved {
            merges_by_issue.entry(n).or_default().push(merge.clone());
        }
    }
    if unnamed > 0 {
        warnings.push(format!(
            "{unnamed} merged pull requests name no tracked issue"
        ));
    }
    if unmatched > 0 {
        warnings.push(format!(
            "{unmatched} issue references in merged pull requests match no issue"
        ));
    }

    // Per-issue state.
    let mut issues_out: Vec<Value> = Vec::new();
    let mut state_of_issue: BTreeMap<u64, State> = BTreeMap::new();
    let (mut no_tags, mut built_unproven) = (Vec::new(), 0usize);
    let mut unknown_tags: BTreeMap<String, usize> = BTreeMap::new();
    let mut counts: BTreeMap<&'static str, u64> = BTreeMap::new();
    for state in [
        State::Planned,
        State::Ready,
        State::InProgress,
        State::AwaitingTest,
        State::Built,
        State::Released,
    ] {
        counts.insert(state.name(), 0);
    }
    let mut sorted_issues = raw_issues.clone();
    sorted_issues.sort_by_key(|i| i["number"].as_u64().unwrap_or(0));
    for issue in &sorted_issues {
        let number = issue["number"].as_u64().unwrap_or(0);
        let tags = parse_tags(s(issue, "tags_line")).unwrap_or_else(|| {
            no_tags.push(number);
            Tags::default()
        });
        for tag in &tags.unknown {
            *unknown_tags.entry(tag.clone()).or_default() += 1;
        }
        let merges = merges_by_issue.get(&number).cloned().unwrap_or_default();
        let proven = merges.iter().any(|m| m.ci.is_some());
        let released = proven
            && first_release
                .as_ref()
                .map(|t| merges.iter().any(|m| m.ci.is_some() && &m.merged_at <= t))
                .unwrap_or(false);
        let state = state_of(
            &tags,
            &Evidence {
                merged: !merges.is_empty(),
                green: proven,
                released,
            },
        );
        if tags.status.as_deref() == Some("built") && state < State::Built {
            built_unproven += 1;
        }
        let (blocked_by, _) = resolver.resolve_all(&refs(s(issue, "depends_line")));
        let blocked_by: Vec<u64> = blocked_by.into_iter().filter(|b| *b != number).collect();
        *counts.entry(state.name()).or_default() += 1;
        state_of_issue.insert(number, state);
        issues_out.push(json!({
            "number": number, "title": s(issue, "title"), "url": s(issue, "url"),
            "state": state.name(), "blocked": tags.blocked(), "blocked_by": blocked_by,
            "owner_tested": !tags.owner_untested(),
            "milestone": tags.milestone.clone().map(Value::from).unwrap_or(Value::Null),
            "tags": tags.all, "merges": merges.iter().map(Merge::to_json).collect::<Vec<_>>(),
        }));
    }
    if !no_tags.is_empty() {
        let shown: Vec<String> = no_tags.iter().take(10).map(|n| format!("#{n}")).collect();
        warnings.push(format!(
            "{} issues have no Tags line ({})",
            no_tags.len(),
            shown.join(", ")
        ));
    }
    for (tag, count) in &unknown_tags {
        warnings.push(format!("unknown tag `{tag}` on {count} issues"));
    }
    if built_unproven > 0 {
        warnings.push(format!("{built_unproven} issues are tagged status:built without a merge on main and a green run"));
    }
    for n in &resolver.ambiguous {
        warnings.push(format!("reference #{n} matches an old-tracker number and a different issue; the old number was used"));
    }

    // Features.
    let least = |ns: &[u64]| -> Option<State> {
        ns.iter()
            .filter_map(|n| state_of_issue.get(n))
            .min()
            .copied()
    };
    let mut features_out: Vec<Value> = Vec::new();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();
    let (mut tested, mut untested) = (0u64, 0u64);
    for entry in parse_features(features_yaml) {
        let mut ids = entry.issues.clone();
        let (mut commands, mut mcp, mut wanted) = (Vec::new(), Vec::new(), Vec::new());
        for e in &entry.evidence {
            match e.split_once(':') {
                Some(("command", v)) => commands.push(v.to_string()),
                Some(("mcp", v)) => mcp.push(v.to_string()),
                Some(("test", v)) => wanted.push(v.to_string()),
                Some(("issue", v)) => ids.extend(all_refs(v)),
                _ => {}
            }
        }
        let (issue_numbers, missed) = resolver.resolve_all(&ids);
        if missed > 0 {
            warnings.push(format!(
                "feature `{}` lists {missed} issues that match no issue",
                entry.name
            ));
        }
        let mut merges: BTreeMap<u64, Merge> = BTreeMap::new();
        for n in &issue_numbers {
            for m in merges_by_issue.get(n).into_iter().flatten() {
                merges.insert(m.pr, m.clone());
            }
        }
        let ci = merges
            .values()
            .max_by(|a, b| a.merged_at.cmp(&b.merged_at))
            .and_then(|m| m.ci.clone());
        let (mut found, mut missing) = (Vec::new(), Vec::new());
        wanted.sort();
        wanted.dedup();
        for name in wanted {
            match tests.get(&name) {
                Some(file) => found.push(json!({"name": name, "file": file})),
                None => missing.push(name),
            }
        }
        let proof = if found.is_empty() {
            "no-test"
        } else {
            "tested"
        };
        if found.is_empty() {
            untested += 1
        } else {
            tested += 1
        }
        let mut id = slug(&format!("{} {}", entry.area, entry.name));
        if !seen_ids.insert(id.clone()) {
            warnings.push(format!(
                "feature id `{id}` is listed twice; the later entry got a suffix"
            ));
            id = format!("{id}-2");
            seen_ids.insert(id.clone());
        }
        features_out.push(json!({
            "id": id, "area": entry.area, "title": entry.name, "listed_status": entry.status,
            "state": least(&issue_numbers).map(State::name).unwrap_or("unlinked"),
            "issues": issue_numbers, "merges": merges.values().map(Merge::to_json).collect::<Vec<_>>(),
            "ci": ci.unwrap_or(Value::Null), "tests": found, "missing_tests": missing,
            "proof": proof, "commands": commands, "mcp_tools": mcp,
        }));
    }
    features_out.sort_by(|a, b| s(a, "id").cmp(s(b, "id")));

    // Roadmap lines and drift against the checkbox.
    let mut roadmap_out: Vec<Value> = Vec::new();
    for line in parse_roadmap(roadmap_md) {
        let (numbers, _) = resolver.resolve_all(&line.refs);
        let state = least(&numbers);
        let all_built = !numbers.is_empty()
            && numbers.iter().all(|n| {
                state_of_issue
                    .get(n)
                    .map(|s| *s >= State::Built)
                    .unwrap_or(false)
            });
        if !line.subset {
            if line.checked && state.map(|s| s < State::Built).unwrap_or(false) {
                warnings.push(format!(
                    "roadmap drift: checked but not built: {}",
                    short(&line.text)
                ));
            } else if !line.checked && all_built {
                warnings.push(format!(
                    "roadmap drift: every issue built but unchecked: {}",
                    short(&line.text)
                ));
            }
        }
        roadmap_out.push(json!({
            "milestone": line.milestone, "text": line.text, "checked": line.checked,
            "scope": if line.subset { "subset" } else { "full" }, "issues": numbers,
            "state": state.map(State::name).unwrap_or("unlinked"),
        }));
    }

    // Source block.
    let mut last_green: Option<&Value> = None;
    for run in &runs {
        if run["conclusion"].as_str() == Some("success")
            && last_green
                .map(|g| s(run, "created_at") > s(g, "created_at"))
                .unwrap_or(true)
        {
            last_green = Some(run);
        }
    }
    let last_green_ci = last_green
        .map(|r| json!({"url": r["url"], "head_sha": r["head_sha"], "created_at": r["created_at"]}))
        .unwrap_or(Value::Null);
    let mut doc = json!({
        "schema_version": 1,
        "generated_at": s(snapshot, "fetched_at"),
        "stale": stale,
        "source": {
            "repo": s(snapshot, "repo"),
            "main_sha": snapshot.get("main_sha").cloned().unwrap_or(Value::Null),
            "last_green_ci": last_green_ci,
            "latest_release": latest_release.map(Value::from).unwrap_or(Value::Null),
            "api_requests": snapshot.get("api_requests").and_then(Value::as_u64).unwrap_or(0),
        },
        "counts": {"issues_by_state": counts, "features_by_proof": {"tested": tested, "no-test": untested}},
        "features": features_out, "roadmap": roadmap_out, "issues": issues_out, "warnings": [],
    });
    let removed = scrub(&mut doc);
    if removed > 0 {
        warnings.push(format!(
            "{removed} text values had internal hosts, addresses or paths removed"
        ));
    }
    warnings.sort();
    if let Some(w) = doc.get_mut("warnings") {
        *w = json!(warnings);
    }
    let rendered = render(&doc)?;
    if let Some(bad) = ["192.168.", "/home/", "/srv/", "@local", "quasarke"]
        .iter()
        .find(|p| rendered.contains(**p))
    {
        return Err(format!(
            "output still contains `{bad}`; refusing to publish it"
        ));
    }
    Ok(doc)
}

/// First 80 characters of a text, for warnings.
fn short(text: &str) -> String {
    text.chars().take(80).collect()
}

/// Pretty JSON with sorted keys and one trailing newline.
pub fn render(doc: &Value) -> Result<String, String> {
    let mut text = serde_json::to_string_pretty(doc).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(text)
}

/// Keeps the fields the status needs from one raw API issue; `None` for pull requests.
pub fn trim_issue(item: &Value) -> Option<Value> {
    if item.get("pull_request").is_some() {
        return None;
    }
    let body = s(item, "body");
    let tags_line = body.lines().next().unwrap_or("");
    let migrated_from = body.lines().find_map(|l| {
        let rest = l.strip_prefix("Migrated from `")?;
        let n = rest.split('#').nth(1)?;
        n.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse::<u64>()
            .ok()
    });
    // The "Depends on" / "Blocked by" lines, plus the lines under a "## Depends on" heading.
    let mut depends: Vec<&str> = Vec::new();
    let mut under_heading = false;
    for line in body.lines() {
        let plain = line.trim_start_matches(['#', ' ']).to_lowercase();
        if line.starts_with("## ") {
            under_heading = plain.starts_with("depends on") || plain.starts_with("blocked by");
        } else if under_heading
            || plain.starts_with("depends on")
            || plain.starts_with("blocked by")
        {
            depends.push(line);
        }
    }
    let depends_text = depends.join(" ");
    Some(json!({
        "number": item["number"], "title": s(item, "title"), "url": s(item, "html_url"),
        "state": s(item, "state"), "tags_line": tags_line, "migrated_from": migrated_from,
        "depends_line": depends_text,
    }))
}

/// Keeps the fields the status needs from one raw pull request.
pub fn trim_pull(item: &Value) -> Value {
    let body = s(item, "body");
    let issues_line = body
        .lines()
        .find(|l| l.starts_with("Issues:"))
        .unwrap_or("");
    json!({
        "number": item["number"], "title": s(item, "title"), "url": s(item, "html_url"),
        "merged_at": s(item, "merged_at"), "merge_sha": s(item, "merge_commit_sha"),
        "issues_line": issues_line,
    })
}

/// Keeps the fields the status needs from one raw workflow run.
pub fn trim_run(item: &Value) -> Value {
    json!({"url": s(item, "html_url"), "head_sha": s(item, "head_sha"),
           "conclusion": s(item, "conclusion"), "created_at": s(item, "created_at")})
}

/// One GET against the GitHub API through `curl`; counts into `requests`.
///
/// When `etag` is provided, sends `If-None-Match`; on a 304 response
/// returns `Ok(None)` (body unchanged) and does not count against the
/// caller's request budget. On a 200 response returns `Ok(Some((body, etag)))`
/// where `etag` is the new ETag for the response.
fn api_get(
    path: &str,
    requests: &mut u64,
    etag: Option<&str>,
) -> Result<Option<(Value, String)>, String> {
    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS",
        "-L",
        "-w",
        "\\nHTTP_CODE:%{http_code}\\nETAG:%{etag}",
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        "User-Agent: pcbcraft-xtask",
    ]);
    if let Some(tag) = etag {
        cmd.args(["-H", &format!("If-None-Match: {tag}")]);
    }
    if let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        cmd.args(["-H", &format!("Authorization: Bearer {token}")]);
    }
    cmd.arg(format!("https://api.github.com/{path}"));
    *requests += 1;
    let out = cmd.output().map_err(|e| format!("curl: {e}"))?;
    let text = String::from_utf8(out.stdout).map_err(|e| format!("curl output: {e}"))?;

    // Parse curl's custom write-out appended at the end.
    let body_end = text.rfind("\\nHTTP_CODE:").unwrap_or(text.len());
    let body = &text[..body_end];
    let meta = &text[body_end..];
    let code = extract_meta(meta, "HTTP_CODE:");
    let new_etag = extract_meta(meta, "ETAG:");

    if code == "304" {
        return Ok(None); // Not modified; caller keeps cached value.
    }
    if !out.status.success() && code != "200" {
        return Err(format!("GitHub API request for {path} failed (HTTP {code})"));
    }
    if body.is_empty() {
        return Ok(Some((json!(null), new_etag)));
    }
    serde_json::from_str(body)
        .map(|v| Some((v, new_etag)))
        .map_err(|e| format!("GitHub API reply for {path}: {e}"))
}
fn extract_meta(meta: &str, key: &str) -> String {
    for line in meta.lines() {
        if let Some(val) = line.strip_prefix(key) {
            return val.to_string();
        }
    }
    String::new()
}

/// Pages through a list endpoint, 100 items a page.
fn api_pages(path: &str, requests: &mut u64) -> Result<Vec<Value>, String> {
    let mut all = Vec::new();
    for page in 1..=30 {
        let got = api_get(
            &format!(
                "{path}{}per_page=100&page={page}",
                if path.contains('?') { "&" } else { "?" }
            ),
            requests,
            None,
        )?;
        let items = got
            .as_ref()
            .and_then(|(v, _)| v.as_array())
            .cloned()
            .unwrap_or_default();
        let done = items.len() < 100;
        all.extend(items);
        if done {
            break;
        }
    }
    Ok(all)
}
/// Fetches a fresh snapshot of the tracker. Needs the network; never called by tests.
///
/// # Rules
/// - Reads only `repo`. About 10 requests for this repository.
/// - Uses ETag cache for conditional requests on issues list page 1.
/// - Supports incremental updates with `since` parameter.
fn fetch_snapshot(repo: &str, fetched_at: &str, root: &Path) -> Result<Value, String> {
    let mut requests = 0u64;
    let cache = EtagCache::load(root)?;

    // Use cached ETag for issues list page 1
    let issues_result = api_get(
        &format!("repos/{repo}/issues?state=all&per_page=100&page=1"),
        &mut requests,
        cache.issues_etag.as_deref(),
    )?;

    let issues = if let Some((body, new_etag)) = issues_result {
        // Got new data or first request
        let page1_items = body.as_array().cloned().unwrap_or_default();
        let mut all_issues = page1_items.clone();

        // If page 1 was full, fetch remaining pages starting from page 2
        if page1_items.len() == 100 {
            all_issues.extend(api_pages_from(&format!("repos/{repo}/issues?state=all"), 2, &mut requests)?);
        }

        // Update cache with new ETag
        let mut updated_cache = cache;
        updated_cache.issues_etag = Some(new_etag);
        updated_cache.last_full = Some(fetched_at.to_string());
        updated_cache.save(root)?;

        all_issues
    } else {
        // 304 Not Modified - use cached snapshot
        // Try incremental update with since parameter
        if let Some(since_ts) = cache.since() {
            let incremental = api_get(
                &format!("repos/{repo}/issues?state=all&per_page=100&page=1&since={since_ts}"),
                &mut requests,
                None,
            )?;
            if let Some((body, _)) = incremental {
                let items = body.as_array().cloned().unwrap_or_default();
                if !items.is_empty() {
                    // Got incremental updates - merge with cached snapshot
                    let snapshot_path = root.join(SNAPSHOT_PATH);
                    if snapshot_path.is_file() {
                        let text = std::fs::read_to_string(&snapshot_path)
                            .map_err(|e| format!("{}: {e}", snapshot_path.display()))?;
                        let snap: Value = serde_json::from_str(&text)
                            .map_err(|e| format!("snapshot: {e}"))?;
                        let cached_issues = snap["issues"].as_array().cloned().unwrap_or_default();
                        // Merge: replace updated issues, add new ones
                        let merged = merge_issues(&cached_issues, &items)?;
                        // Update cache
                        let mut updated_cache = cache;
                        updated_cache.last_incremental = Some(fetched_at.to_string());
                        updated_cache.save(root)?;
                        return Ok(json!({
                            "repo": repo, "fetched_at": fetched_at, "api_requests": requests,
                            "issues": merged,
                            "pulls": snap["pulls"].clone(),
                            "runs": snap["runs"].clone(),
                            "releases": snap["releases"].clone(),
                            "main_sha": snap["main_sha"].clone(),
                        }));
                    }
                }
            }
        }
        // No incremental updates or no cached snapshot - use full cached snapshot
        let snapshot_path = root.join(SNAPSHOT_PATH);
        let text = std::fs::read_to_string(&snapshot_path)
            .map_err(|e| format!("{}: {e}", snapshot_path.display()))?;
        let snap: Value = serde_json::from_str(&text)
            .map_err(|e| format!("snapshot: {e}"))?;
        snap["issues"].as_array().cloned().unwrap_or_default()
    };

    let pulls = api_pages(&format!("repos/{repo}/pulls?state=closed"), &mut requests)?;
    let runs = api_get(
        &format!(
            "repos/{repo}/actions/workflows/ci.yml/runs?branch=main&status=completed&per_page=100"
        ),
        &mut requests,
        None,
    )?;
    let releases = api_get(&format!("repos/{repo}/releases?per_page=30"), &mut requests, None)?;
    let branch = api_get(&format!("repos/{repo}/branches/main"), &mut requests, None)?;
    Ok(json!({
        "repo": repo, "fetched_at": fetched_at, "api_requests": requests,
        "main_sha": branch.as_ref().and_then(|(v, _)| v["commit"]["sha"].as_str()).unwrap_or(""),
        "issues": issues.iter().filter_map(trim_issue).collect::<Vec<_>>(),
        "pulls": pulls.iter().map(trim_pull).collect::<Vec<_>>(),
        "runs": runs.as_ref().and_then(|(v, _)| v["workflow_runs"].as_array()).cloned().unwrap_or_default().iter().map(trim_run).collect::<Vec<_>>(),
        "releases": releases.as_ref().and_then(|(v, _)| v.as_array()).cloned().unwrap_or_default().iter()
            .filter(|r| r["draft"].as_bool() != Some(true))
            .map(|r| json!({"tag": s(r, "tag_name"), "published_at": s(r, "published_at")})).collect::<Vec<_>>(),
    }))
}

/// Pages through a list endpoint starting from a given page, 100 items a page.
fn api_pages_from(path: &str, start_page: u32, requests: &mut u64) -> Result<Vec<Value>, String> {
    let mut all = Vec::new();
    for page in start_page..=30 {
        let got = api_get(
            &format!(
                "{path}{}per_page=100&page={page}",
                if path.contains('?') { "&" } else { "?" }
            ),
            requests,
            None,
        )?;
        let items = got
            .as_ref()
            .and_then(|(v, _)| v.as_array())
            .cloned()
            .unwrap_or_default();
        let done = items.len() < 100;
        all.extend(items);
        if done {
            break;
        }
    }
    Ok(all)
}

/// Merge incremental issue updates into the cached issues list.
fn merge_issues(cached: &[Value], updates: &[Value]) -> Result<Vec<Value>, String> {
    let mut by_number = BTreeMap::new();
    for issue in cached {
        if let Some(num) = issue["number"].as_u64() {
            by_number.insert(num, issue.clone());
        }
    }
    for update in updates {
        if let Some(num) = update["number"].as_u64() {
            by_number.insert(num, update.clone());
        }
    }
    Ok(by_number.into_values().collect())
}


/// Current UTC time as `YYYY-MM-DDTHH:MM:SSZ`, from `date`.
fn utc_now() -> String {
    Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|t| t.trim().to_string())
        .unwrap_or_default()
}

/// Returns the age of a snapshot in days, or None if unparseable.
fn snapshot_age_days(fetched_at: &str) -> Option<u64> {
    // Use date to parse the ISO timestamp and compute age in seconds
    let now = Command::new("date")
        .args(["-u", "+%s"])
        .output()
        .ok()?
        .stdout;
    let now_secs: i64 = String::from_utf8(now).ok()?.trim().parse().ok()?;

    let then = Command::new("date")
        .args(["-u", "-d", fetched_at, "+%s"])
        .output()
        .ok()?
        .stdout;
    let then_secs: i64 = String::from_utf8(then).ok()?.trim().parse().ok()?;

    let age_secs = now_secs.saturating_sub(then_secs);
    if age_secs < 0 {
        return Some(0);
    }
    Some((age_secs / 86400) as u64)
}

/// Validates a document against the committed schema.
fn check_doc(root: &Path, doc: &Value) -> Result<(), String> {
    let schema_text = std::fs::read_to_string(root.join(SCHEMA_PATH))
        .map_err(|e| format!("{SCHEMA_PATH}: {e}"))?;
    let schema: Value =
        serde_json::from_str(&schema_text).map_err(|e| format!("{SCHEMA_PATH}: {e}"))?;
    let problems = status_schema::validate(&schema, doc);
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "status data does not match the schema:\n  {}",
            problems.join("\n  ")
        ))
    }
}

/// The subcommand: `status-data [--refresh] [--repo OWNER/NAME] [--snapshot FILE] [--out FILE] [--check FILE]`.
///
/// # Rules
/// - Without `--refresh` nothing touches the network.
/// - A failed refresh falls back to the cached snapshot and marks the output `stale`.
/// - `--check FILE` only validates an existing output against the schema.
pub fn run(args: &[String], root: &Path) -> Result<(), String> {
    let (mut refresh, mut repo, mut snapshot, mut out, mut check) =
        (false, DEFAULT_REPO.to_string(), None, None, None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--refresh" => refresh = true,
            "--repo" => repo = value("--repo")?,
            "--snapshot" => snapshot = Some(value("--snapshot")?),
            "--out" => out = Some(value("--out")?),
            "--check" => check = Some(value("--check")?),
            other => {
                return Err(format!(
                    "unknown option {other}\nusage: cargo xtask status-data [--refresh] [--repo OWNER/NAME] [--snapshot FILE] [--out FILE] [--check FILE]"
                ));
            }
        }
    }
    if let Some(file) = check {
        let text = std::fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?;
        let doc: Value = serde_json::from_str(&text).map_err(|e| format!("{file}: {e}"))?;
        check_doc(root, &doc)?;
        println!("{file} matches {SCHEMA_PATH}");
        return Ok(());
    }
    let snapshot_path = root.join(snapshot.unwrap_or_else(|| SNAPSHOT_PATH.to_string()));
    let mut stale = false;
    if refresh {
        match fetch_snapshot(&repo, &utc_now(), root) {
            Ok(fresh) => {
                std::fs::write(&snapshot_path, render(&fresh)?)
                    .map_err(|e| format!("{}: {e}", snapshot_path.display()))?;
            }
            Err(e) if snapshot_path.is_file() => {
                // Check if the cached snapshot is older than 7 days
                let cached = std::fs::read_to_string(&snapshot_path)
                    .map_err(|read_e| format!("{}: {read_e}", snapshot_path.display()))?;
                let cached_snap: Value = serde_json::from_str(&cached)
                    .map_err(|parse_e| format!("snapshot: {parse_e}"))?;
                let fetched_at = cached_snap["fetched_at"]
                    .as_str()
                    .unwrap_or("");
                if let Some(age_days) = snapshot_age_days(fetched_at) {
                    if age_days >= 7 {
                        return Err(format!(
                            "refresh failed ({e}) and cached snapshot is {age_days} days old (>= 7 days)"
                        ));
                    }
                }
                eprintln!("refresh failed ({e}); using the cached snapshot");
                stale = true;
            }
            Err(e) => return Err(e),
        }
    }
    let text = std::fs::read_to_string(&snapshot_path).map_err(|e| {
        format!(
            "{}: {e}; run `cargo xtask status-data --refresh` once",
            snapshot_path.display()
        )
    })?;
    let snap: Value = serde_json::from_str(&text).map_err(|e| format!("snapshot: {e}"))?;
    let read = |p: &str| std::fs::read_to_string(root.join(p)).map_err(|e| format!("{p}: {e}"));
    let doc = generate(
        &snap,
        &read(FEATURES_PATH)?,
        &read("ROADMAP.md")?,
        &scan_tests(root),
        stale,
    )?;
    check_doc(root, &doc)?;
    let out_path = root.join(out.unwrap_or_else(|| OUT_PATH.to_string()));
    std::fs::write(&out_path, render(&doc)?).map_err(|e| format!("{}: {e}", out_path.display()))?;
    println!(
        "wrote {} ({} features, {} issues, {} warnings)",
        out_path.display(),
        doc["features"].as_array().map(Vec::len).unwrap_or(0),
        doc["issues"].as_array().map(Vec::len).unwrap_or(0),
        doc["warnings"].as_array().map(Vec::len).unwrap_or(0)
    );
    Ok(())
}

#[cfg(test)]
mod tests;
