//! Changeset gate and changelog compiler behind `cargo xtask changeset` and
//! `cargo xtask changelog`.
//!
//! Context: each pull request carries one `.changes/<branch-slug>.md` file
//! (format: `.changes/README.md`). `check` reads changed files from
//! `git diff origin/main...HEAD`; `changelog` compiles and archives the files;
//! `list` prints them as JSON for generators such as `status-data`.
//!
//! # Rules
//! - The frontmatter is one `key: value` per line between `---` lines, split at
//!   the first colon, so line tools and the JSON listing read it the same way.
//! - `check` is pure over (changed files, file reader); only `run` touches git.
//!
//! # Dependencies
//! - .changes/README.md
//! - .changes/TEMPLATE.md
//! - xtask/src/main.rs
//!
//! # Do not
//! - Read the network, or accept a changeset under `.changes/released/` as new.
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// Allowed `type` values, in the order the changelog lists them.
pub const TYPES: [&str; 7] = ["feature", "fix", "refactor", "docs", "test", "ci", "chore"];
/// Longest `summary` accepted, in characters.
const MAX_SUMMARY: usize = 200;
/// Shortest reason accepted after `docs: n/a:`.
const MIN_REASON: usize = 20;
/// Issue links in the changelog point here.
const ISSUE_URL: &str = "https://github.com/wdunn001/pcbcraft/issues";

/// The paste-ready template printed with every failure.
pub const TEMPLATE: &str = "---
type: fix
area: layout
issues: [123]
user_visible: true
docs: [docs/getting-started.md]
summary: One plain sentence a user could read.
---
";

/// One parsed `.changes/<slug>.md` file.
///
/// # Rules
/// - `docs` is `None` when the author wrote `n/a: <reason>`; the reason is in `docs_reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changeset {
    /// Slug, the file name without `.md`.
    pub slug: String,
    /// One of [`TYPES`].
    pub kind: String,
    /// Tab or crate name.
    pub area: String,
    /// Fork issue numbers.
    pub issues: Vec<u32>,
    /// Whether a user can see the change.
    pub user_visible: bool,
    /// Documentation files the change updated.
    pub docs: Vec<String>,
    /// The reason when `docs` is `n/a`.
    pub docs_reason: Option<String>,
    /// One plain sentence.
    pub summary: String,
}

/// Parses the frontmatter of one changeset file.
///
/// # Rules
/// - Unknown keys, duplicate keys and missing keys are errors.
///
/// # Do not
/// - Add YAML features; the format is deliberately line-oriented.
pub fn parse(slug: &str, text: &str) -> Result<Changeset, String> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err("the file must start with a line containing only ---".into());
    }
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| format!("line without a colon: {line}"))?;
        let key = key.trim().to_string();
        if !["type", "area", "issues", "user_visible", "docs", "summary"].contains(&key.as_str()) {
            return Err(format!("unknown key `{key}`"));
        }
        if map.insert(key.clone(), value.trim().to_string()).is_some() {
            return Err(format!("duplicate key `{key}`"));
        }
    }
    if !closed {
        return Err("the frontmatter must end with a line containing only ---".into());
    }
    let get = |k: &str| -> Result<&String, String> {
        map.get(k).ok_or_else(|| format!("missing key `{k}`"))
    };
    let kind = get("type")?.clone();
    if !TYPES.contains(&kind.as_str()) {
        return Err(format!("type `{kind}` must be one of {}", TYPES.join(", ")));
    }
    let area = get("area")?.clone();
    if area.is_empty()
        || !area
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(format!(
            "area `{area}` must be lowercase letters, digits and dashes (a tab or crate)"
        ));
    }
    let issues = parse_list(get("issues")?)?
        .iter()
        .map(|s| {
            s.trim_start_matches('#')
                .parse::<u32>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("issue `{s}` is not a positive number"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let user_visible = match get("user_visible")?.as_str() {
        "true" => true,
        "false" => false,
        other => return Err(format!("user_visible `{other}` must be true or false")),
    };
    let docs_raw = get("docs")?;
    let (docs, docs_reason) = if let Some(reason) = docs_raw.strip_prefix("n/a:") {
        let reason = reason.trim();
        if reason.chars().count() < MIN_REASON {
            return Err(format!(
                "`docs: n/a:` needs a reason of at least {MIN_REASON} characters"
            ));
        }
        (Vec::new(), Some(reason.to_string()))
    } else {
        let list = parse_list(docs_raw)?;
        if list.is_empty() {
            return Err("docs must list the files you updated, or say `n/a: <reason>`".into());
        }
        (list, None)
    };
    let summary = get("summary")?.clone();
    if summary.chars().count() < 10 || summary.chars().count() > MAX_SUMMARY {
        return Err(format!(
            "summary must be one plain sentence of 10 to {MAX_SUMMARY} characters"
        ));
    }
    if summary.contains('\u{2014}') || summary.contains('\u{2013}') {
        return Err("summary must not contain dashes used as punctuation".into());
    }
    Ok(Changeset {
        slug: slug.to_string(),
        kind,
        area,
        issues,
        user_visible,
        docs,
        docs_reason,
        summary,
    })
}

/// Parses `[a, b]` (brackets optional) into trimmed items.
fn parse_list(raw: &str) -> Result<Vec<String>, String> {
    let inner = raw.trim();
    let inner = match (inner.strip_prefix('['), inner.ends_with(']')) {
        (Some(rest), true) => rest.trim_end_matches(']'),
        (None, false) => inner,
        _ => return Err(format!("`{raw}` is not a [list]")),
    };
    Ok(inner
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

/// True for `.changes/<name>.md` that is a real changeset, not a doc or a released one.
fn is_changeset_path(path: &str) -> bool {
    path.strip_prefix(".changes/").is_some_and(|rest| {
        !rest.contains('/') && rest.ends_with(".md") && rest != "README.md" && rest != "TEMPLATE.md"
    })
}

/// True for the files that count as documentation.
fn is_doc_path(path: &str) -> bool {
    path.starts_with("docs/")
        || path.starts_with("site/src/content/docs/")
        || path == "README.md"
        || path == "CHANGELOG.md"
        || path.starts_with(".cursor/rules/")
}

/// True for the paths where a change is visible to users even if the author forgot to say so.
fn is_user_surface(path: &str) -> bool {
    let io_reader = ["read", "load", "trace"].iter().any(|w| path.contains(w));
    path.starts_with("crates/ui-egui/")
        || path.starts_with("crates/engine/src/")
        || (path.starts_with("crates/io/src/") && !io_reader)
        || path.starts_with("site/")
}

/// Checks a pull request. `files` are the changed paths; `read` returns a file's text.
///
/// # Rules
/// - Exactly one changeset; docs needed when user visible or on a user surface.
/// - `doc_comments` are Rust files whose diff adds `//!` or `///` lines; outside
///   crates/ui-egui they count as documentation, so agents need no separate prose.
/// - Quality of the comments is `cargo xtask docs check`'s job, not this one's.
///
/// # Do not
/// - Touch git or the file system; callers pass both in.
pub fn check(
    files: &[String],
    doc_comments: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> Result<(), String> {
    let sets: Vec<&String> = files.iter().filter(|f| is_changeset_path(f)).collect();
    if sets.len() != 1 {
        return Err(if sets.is_empty() {
            "this pull request has no changeset: add one file under .changes/".into()
        } else {
            format!(
                "this pull request changes {} changesets ({}); it must have exactly one",
                sets.len(),
                sets.iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        });
    }
    let path = sets[0];
    let slug = path
        .trim_start_matches(".changes/")
        .trim_end_matches(".md")
        .to_string();
    let text = read(path).ok_or_else(|| format!("{path} cannot be read"))?;
    let set = parse(&slug, &text).map_err(|e| format!("{path}: {e}"))?;
    let changed_docs: Vec<&String> = files
        .iter()
        .filter(|f| {
            is_doc_path(f) || (doc_comments.contains(f) && !f.starts_with("crates/ui-egui/"))
        })
        .collect();
    let surface = files.iter().find(|f| is_user_surface(f));
    let needs_docs = set.user_visible || surface.is_some();
    if needs_docs && set.docs_reason.is_none() && changed_docs.is_empty() {
        let why = if set.user_visible {
            "user_visible is true".to_string()
        } else {
            format!("it touches {}", surface.map(|s| s.as_str()).unwrap_or(""))
        };
        return Err(format!(
            "{path}: {why}, so a documentation file must change in this pull request \
             (docs/, site/src/content/docs/, README.md, CHANGELOG.md, .cursor/rules/, or new `//!` and `///` \
             comments in Rust code outside crates/ui-egui), \
             or the changeset must say `docs: n/a: <reason of at least {MIN_REASON} characters>`"
        ));
    }
    for d in &set.docs {
        if !changed_docs.contains(&d) {
            return Err(format!(
                "{path}: docs lists {d}, which this pull request does not change"
            ));
        }
    }
    Ok(())
}

/// Instructions printed under every failure.
fn help() -> String {
    format!(
        "\nAdd `.changes/<branch-slug>.md` with this content (edit every line):\n\n{TEMPLATE}\n\
         Rules: one file per pull request; docs is the list of doc files you changed, or\n\
         `docs: n/a: <reason of at least {MIN_REASON} characters>`. See .changes/README.md."
    )
}

/// Lists the files changed since the merge base with the base branch.
fn changed_files(base: &str) -> Result<Vec<String>, String> {
    let range = format!("{base}...HEAD");
    let out = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=ACMR", &range])
        .output()
        .map_err(|e| format!("git diff: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git diff {range} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

/// Rust files whose diff adds a `//!` or `///` line.
fn doc_comment_files(base: &str) -> Result<Vec<String>, String> {
    let range = format!("{base}...HEAD");
    let out = Command::new("git")
        .args(["diff", "-U0", "--diff-filter=ACMR", &range, "--", "*.rs"])
        .output()
        .map_err(|e| format!("git diff: {e}"))?;
    let mut found: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some(p) = line.strip_prefix("+++ b/") {
            current = p.to_string();
        } else if line.strip_prefix('+').is_some_and(|l| {
            let l = l.trim_start();
            l.starts_with("//!") || l.starts_with("///")
        }) && !current.is_empty()
            && !found.contains(&current)
        {
            found.push(current.clone());
        }
    }
    Ok(found)
}

/// Loads every unreleased changeset, sorted by slug.
pub fn load_dir(root: &Path) -> Result<Vec<Changeset>, String> {
    let dir = root.join(".changes");
    let mut sets = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(sets);
    };
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if !path.is_file() || !is_changeset_path(&format!(".changes/{name}")) {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
        sets.push(parse(name.trim_end_matches(".md"), &text).map_err(|e| format!("{name}: {e}"))?);
    }
    sets.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(sets)
}

/// Renders the changelog section for `sets`, grouped by area then type.
pub fn render(version: &str, date: &str, sets: &[Changeset]) -> String {
    let mut out = format!("## {version} - {date}\n");
    let mut areas: Vec<&str> = sets.iter().map(|s| s.area.as_str()).collect();
    areas.sort_unstable();
    areas.dedup();
    for area in areas {
        out.push_str(&format!("\n### {area}\n"));
        for kind in TYPES {
            let group: Vec<&Changeset> = sets
                .iter()
                .filter(|s| s.area == area && s.kind == kind)
                .collect();
            if group.is_empty() {
                continue;
            }
            out.push_str(&format!("\n{kind}\n\n"));
            for s in group {
                let links: Vec<String> = s
                    .issues
                    .iter()
                    .map(|n| format!("[#{n}]({ISSUE_URL}/{n})"))
                    .collect();
                let tail = if links.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", links.join(", "))
                };
                out.push_str(&format!("- {}{tail}\n", s.summary));
            }
        }
    }
    out
}

/// Puts `section` above the first existing release in `existing`.
fn insert_section(existing: &str, section: &str) -> String {
    if existing.trim().is_empty() {
        return format!("# Changelog\n\n{section}");
    }
    match existing.find("\n## ") {
        Some(i) => format!("{}\n{section}{}", &existing[..i], &existing[i..]),
        None => format!("{}\n\n{section}", existing.trim_end()),
    }
}

/// Today's date as YYYY-MM-DD, from the system `date` command.
fn today() -> String {
    Command::new("date")
        .arg("+%F")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "undated".into())
}

/// `cargo xtask changeset check | list`.
pub fn run_changeset(root: &Path, args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("check") => {
            let base = std::env::var("CHANGESET_BASE").unwrap_or_else(|_| "origin/main".into());
            let files = changed_files(&base)?;
            let reader = |p: &str| std::fs::read_to_string(root.join(p)).ok();
            let comments = doc_comment_files(&base)?;
            check(&files, &comments, &reader).map_err(|e| format!("{e}\n{}", help()))?;
            println!("changeset ok ({} changed files)", files.len());
            Ok(())
        }
        Some("list") => {
            let sets = load_dir(root)?;
            let json: Vec<serde_json::Value> = sets
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "slug": s.slug, "type": s.kind, "area": s.area, "issues": s.issues,
                        "user_visible": s.user_visible, "docs": s.docs,
                        "docs_reason": s.docs_reason, "summary": s.summary,
                    })
                })
                .collect();
            println!("{}", serde_json::Value::Array(json));
            Ok(())
        }
        _ => Err("usage: cargo xtask changeset check | list".into()),
    }
}

/// `cargo xtask changelog [--version X]`.
pub fn run_changelog(root: &Path, args: &[String]) -> Result<(), String> {
    let version = args
        .iter()
        .position(|a| a == "--version")
        .map(|i| {
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| "--version needs a value".to_string())
        })
        .transpose()?;
    let sets = load_dir(root)?;
    if sets.is_empty() {
        println!("no unreleased changesets");
        return Ok(());
    }
    let label = version.clone().unwrap_or_else(|| "Unreleased".into());
    let section = render(&label, &today(), &sets);
    let Some(version) = version else {
        println!("{section}");
        return Ok(());
    };
    if version.is_empty() || version.contains('/') || version.contains("..") {
        return Err(format!("`{version}` is not a usable version label"));
    }
    let target = root.join(".changes/released").join(&version);
    if target.exists() {
        return Err(format!("{} already exists", target.display()));
    }
    let log = root.join("CHANGELOG.md");
    let existing = std::fs::read_to_string(&log).unwrap_or_default();
    std::fs::write(&log, insert_section(&existing, &section)).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    for s in &sets {
        let name = format!("{}.md", s.slug);
        std::fs::rename(root.join(".changes").join(&name), target.join(&name))
            .map_err(|e| format!("{name}: {e}"))?;
    }
    println!(
        "CHANGELOG.md updated; {} files moved to {}",
        sets.len(),
        target.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "---\ntype: fix\narea: layout\nissues: [12, 13]\nuser_visible: true\ndocs: [docs/ci.md]\nsummary: Tracks now snap to the grid when you drag them.\n---\n";

    fn files(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn reader(text: &'static str) -> impl Fn(&str) -> Option<String> {
        move |_| Some(text.to_string())
    }

    #[test]
    fn parses_a_good_file() {
        let c = parse("x", GOOD).expect("parses");
        assert_eq!((c.kind.as_str(), c.issues.clone()), ("fix", vec![12, 13]));
        assert!(c.user_visible && c.docs == ["docs/ci.md"]);
    }

    #[test]
    fn rejects_bad_fields() {
        for (from, to) in [
            ("type: fix", "type: bug"),
            ("area: layout", "area: Layout"),
            ("issues: [12, 13]", "issues: [abc]"),
            ("user_visible: true", "user_visible: yes"),
            ("docs: [docs/ci.md]", "docs: n/a: short"),
            ("summary: Tracks", "colour: red\nsummary: Tracks"),
        ] {
            assert!(parse("x", &GOOD.replace(from, to)).is_err(), "{to}");
        }
        assert!(parse("x", "no frontmatter").is_err());
        assert!(parse("x", &GOOD.replace("area: layout\n", "")).is_err());
        assert!(parse("x", &GOOD.replace("type: fix\n", "type: fix\ntype: fix\n")).is_err());
    }

    #[test]
    fn accepts_na_with_reason() {
        let t = GOOD.replace(
            "docs: [docs/ci.md]",
            "docs: n/a: internal refactor with no behaviour change",
        );
        let c = parse("x", &t).expect("parses");
        assert!(c.docs.is_empty() && c.docs_reason.is_some());
    }

    #[test]
    fn needs_exactly_one_changeset() {
        let r = reader(GOOD);
        assert!(check(&files(&["docs/ci.md"]), &[], &r).is_err());
        let two = files(&[".changes/a.md", ".changes/b.md", "docs/ci.md"]);
        assert!(check(&two, &[], &r).is_err());
        let released = files(&[".changes/released/1.0/a.md", "docs/ci.md"]);
        assert!(check(&released, &[], &r).is_err());
        assert!(check(&files(&[".changes/a.md", "docs/ci.md"]), &[], &r).is_ok());
        let readme = files(&[".changes/README.md", ".changes/a.md", "docs/ci.md"]);
        assert!(check(&readme, &[], &r).is_ok());
    }

    #[test]
    fn docs_required_when_visible_or_on_a_surface() {
        let r = reader(GOOD);
        assert!(check(&files(&[".changes/a.md", "crates/geom/src/x.rs"]), &[], &r).is_err());
        let hidden = reader(
            "---\ntype: chore\narea: ci\nissues: []\nuser_visible: false\ndocs: [docs/ci.md]\nsummary: Tidy the workflow file layout.\n---\n",
        );
        assert!(
            check(
                &files(&[".changes/a.md", "crates/geom/src/x.rs", "docs/ci.md"]),
                &[],
                &hidden
            )
            .is_ok()
        );
        let na = reader(
            "---\ntype: chore\narea: ci\nissues: []\nuser_visible: false\ndocs: n/a: workflow tidy with no user effect\nsummary: Tidy the workflow file layout.\n---\n",
        );
        for surface in [
            "crates/ui-egui/src/lib.rs",
            "crates/engine/src/lib.rs",
            "crates/io/src/gerber.rs",
            "site/src/x.astro",
        ] {
            assert!(
                check(&files(&[".changes/a.md", surface]), &[], &na).is_ok(),
                "{surface}"
            );
        }
        let no_n_a = reader(
            "---\ntype: chore\narea: ci\nissues: []\nuser_visible: false\ndocs: [docs/ci.md]\nsummary: Tidy the workflow file layout.\n---\n",
        );
        assert!(
            check(
                &files(&[".changes/a.md", "crates/ui-egui/src/lib.rs"]),
                &[],
                &no_n_a
            )
            .is_err()
        );
        assert!(
            check(
                &files(&[".changes/a.md", "crates/io/src/plot_read.rs"]),
                &[],
                &na
            )
            .is_ok()
        );
    }

    #[test]
    fn doc_comments_count_outside_the_ui() {
        let r = reader(
            GOOD.replace("docs: [docs/ci.md]", "docs: [crates/io/src/gerber.rs]")
                .leak(),
        );
        let f = files(&[".changes/a.md", "crates/io/src/gerber.rs"]);
        assert!(check(&f, &[], &r).is_err());
        assert!(check(&f, &files(&["crates/io/src/gerber.rs"]), &r).is_ok());
        let ui = files(&[".changes/a.md", "crates/ui-egui/src/lib.rs"]);
        assert!(check(&ui, &files(&["crates/ui-egui/src/lib.rs"]), &reader(GOOD)).is_err());
    }

    #[test]
    fn listed_docs_must_change() {
        let r = reader(GOOD);
        assert!(check(&files(&[".changes/a.md", "README.md"]), &[], &r).is_err());
    }

    #[test]
    fn changelog_groups_by_area_and_type() {
        let a = parse("a", GOOD).expect("a");
        let mut b = a.clone();
        b.slug = "b".into();
        b.kind = "feature".into();
        b.area = "io".into();
        b.issues = vec![];
        let text = render("1.0.0", "2026-10-30", &[a, b]);
        assert!(text.starts_with("## 1.0.0 - 2026-10-30\n"));
        assert!(text.find("### io").expect("io") < text.find("### layout").expect("layout"));
        assert!(text.contains("[#12](https://github.com/wdunn001/pcbcraft/issues/12)"));
        let merged = insert_section("# Changelog\n\nintro\n\n## 0.1 - old\n- x\n", &text);
        assert!(merged.find("1.0.0").expect("new") < merged.find("0.1 - old").expect("old"));
    }
}
