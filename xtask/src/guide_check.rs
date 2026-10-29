//! The user guide check behind `cargo xtask guide-check`, run inside the
//! `docs-check` CI job. A guide step names a control by directive, so a
//! renamed label breaks the build instead of the manual.
//!
//! # Rules
//! - Every check is a pure function over text and lookup closures; only `run` touches the disk.
//! - Failures print `guide-check: <page>:<line>: <message>` on stderr.
//!
//! # Dependencies
//! - docs/guide/en/index.toml
//! - docs/doc-standard.md
//! - crates/text/strings/en.txt
//! - xtask/doc-ratchet.toml
//! - xtask/src/main.rs
//!
//! # Do not
//! - Add a dependency; front matter, the index and the ratchet are read by hand.
//! - Check stories or replay steps here; that is a separate check.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const GUIDE_DIR: &str = "docs/guide/en";
const INDEX_FILE: &str = "docs/guide/en/index.toml";
const CATALOG_FILE: &str = "docs/shots/catalog.toml";
const RATCHET_FILE: &str = "xtask/doc-ratchet.toml";
const SECTIONS: [&str; 4] = ["start", "howto", "reference", "concepts"];
const KINDS: [&str; 4] = ["tutorial", "howto", "reference", "concept"];
const REQUIRED_FRONT: [&str; 6] = ["title", "area", "kind", "story", "shots", "since"];
const MAX_WORDS: usize = 20;
/// Key prefixes whose labels a guide should eventually mention.
const COVERED_PREFIXES: [&str; 3] = ["menu.", "board.tool_strip.", "docks.workbench_tabs."];

/// Names of other design tools. Built from pieces so this file never spells them whole.
fn banned_names() -> Vec<String> {
    [
        ["ki", "cad"],
        ["alt", "ium"],
        ["ea", "gle"],
        ["libre", "pcb"],
        ["easy", "eda"],
        ["or", "cad"],
        ["hori", "zon eda"],
    ]
    .iter()
    .map(|p| p.concat())
    .collect()
}

/// What a page is checked against. Closures keep the checks free of disk access.
pub struct Context<'a> {
    /// Every catalog key with its English value.
    pub keys: &'a BTreeMap<String, String>,
    /// Shot ids in the catalog; `None` while the catalog file does not exist.
    pub shots: Option<&'a BTreeSet<String>>,
    /// Whether a repo-relative path exists.
    pub exists: &'a dyn Fn(&str) -> bool,
    /// Heading anchors of a repo-relative markdown file, or `None` when unreadable.
    pub anchors: &'a dyn Fn(&str) -> Option<BTreeSet<String>>,
}

/// A parsed page: front matter pairs and the body with its first line number.
struct Page {
    front: BTreeMap<String, String>,
    body: Vec<(usize, String)>,
}

/// Splits `---` front matter from the body. Returns messages when it is malformed.
fn split_page(text: &str) -> Result<Page, Vec<String>> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().map(|l| l.trim()) != Some("---") {
        return Err(vec![
            "1: page must start with a --- front matter block".into(),
        ]);
    }
    let Some(end) = lines.iter().skip(1).position(|l| l.trim() == "---") else {
        return Err(vec!["1: front matter is not closed with ---".into()]);
    };
    let end = end + 1;
    let mut front = BTreeMap::new();
    let mut problems = Vec::new();
    for (i, line) in lines.iter().enumerate().take(end).skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        match line.split_once(':') {
            Some((k, v)) => {
                front.insert(k.trim().to_string(), v.trim().to_string());
            }
            None => problems.push(format!("{}: front matter line needs `key: value`", i + 1)),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    let body = lines
        .iter()
        .enumerate()
        .skip(end + 1)
        .map(|(i, l)| (i + 1, (*l).to_string()))
        .collect();
    Ok(Page { front, body })
}

/// Body lines outside fenced code blocks.
fn prose_lines(page: &Page) -> Vec<(usize, &str)> {
    let mut in_fence = false;
    let mut out = Vec::new();
    for (n, line) in &page.body {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            out.push((*n, line.as_str()));
        }
    }
    out
}

/// Every `{{ui:key}}` in a line, with the key text.
fn directives(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("{{ui:") {
        let after = &rest[start + 5..];
        match after.find("}}") {
            Some(end) => {
                out.push(after[..end].trim().to_string());
                rest = &after[end + 2..];
            }
            None => break,
        }
    }
    out
}

/// Replaces each `{{ui:key}}` with the bold English label, as the site renders it.
/// Only tests call it until the site renderer reuses it.
#[cfg(test)]
pub fn render_directives(text: &str, keys: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{ui:") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 5..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let key = after[..end].trim();
        match keys.get(key) {
            Some(label) => out.push_str(&format!("**{label}**")),
            None => out.push_str(&rest[start..start + 5 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// Shot ids named by `shot:<id>` in a line.
fn shot_refs(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("shot:") {
        let after = &rest[start + 5..];
        let id: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !id.is_empty() {
            out.push(id.clone());
        }
        rest = &after[id.len()..];
    }
    out
}

/// Text of every `**bold**` span on a line.
fn bold_spans(line: &str) -> Vec<String> {
    let parts: Vec<&str> = line.split("**").collect();
    parts
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1 && *i + 1 < parts.len())
        .map(|(_, s)| (*s).to_string())
        .collect()
}

/// Whether a line is a numbered step such as `3. Click ...`.
fn is_step(line: &str) -> bool {
    let t = line.trim_start();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    digits > 0 && t[digits..].starts_with(". ")
}

/// GitHub-style heading anchor: lowercase, spaces to hyphens, punctuation dropped.
pub fn slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            ' ' => Some('-'),
            _ => None,
        })
        .collect()
}

/// Heading anchors of a markdown text, ignoring fenced code.
pub fn heading_anchors(text: &str) -> BTreeSet<String> {
    let mut in_fence = false;
    let mut out = BTreeSet::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence && line.starts_with('#') {
            out.insert(slug(line.trim_start_matches('#')));
        }
    }
    out
}

/// Markdown link targets `[text](target)` in a line, images included.
fn link_targets(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(i) = rest.find("](") {
        let after = &rest[i + 2..];
        match after.find(')') {
            Some(end) => {
                out.push(after[..end].trim().to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    out
}

/// Joins a repo-relative page path with a relative link and folds `..` segments.
fn resolve(page: &str, target: &str) -> String {
    let mut parts: Vec<&str> = page.split('/').collect();
    parts.pop();
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Words in a sentence, counting a directive as one word.
fn word_count(sentence: &str) -> usize {
    let mut s = sentence.to_string();
    while let Some(start) = s.find("{{ui:") {
        match s[start..].find("}}") {
            Some(end) => s.replace_range(start..start + end + 2, "label"),
            None => break,
        }
    }
    s.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}

/// Sentences longer than the limit on one prose line, as `(words, start)` pairs.
fn long_sentences(line: &str) -> Vec<(usize, String)> {
    let t = line.trim_start();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    let t = if digits > 0 && t[digits..].starts_with(". ") {
        &t[digits + 2..]
    } else {
        t
    };
    let t = t.trim_start_matches(['-', '*']).trim();
    let mut out = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = t.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        current.push(*c);
        let ends =
            matches!(c, '.' | '?' | '!') && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if ends {
            let words = word_count(&current);
            if words > MAX_WORDS {
                out.push((words, current.trim().to_string()));
            }
            current.clear();
        }
    }
    let words = word_count(&current);
    if words > MAX_WORDS {
        out.push((words, current.trim().to_string()));
    }
    out
}

/// Runs every per-page check. Messages start with the line number.
pub fn check_page(page_path: &str, text: &str, ctx: &Context) -> Vec<String> {
    let page = match split_page(text) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let mut errors = Vec::new();
    for key in REQUIRED_FRONT {
        if !page.front.contains_key(key) {
            errors.push(format!("1: front matter is missing `{key}`"));
        }
    }
    if let Some(kind) = page.front.get("kind") {
        if !KINDS.contains(&kind.as_str()) {
            errors.push(format!(
                "1: kind `{kind}` must be one of {}",
                KINDS.join(", ")
            ));
        }
    }
    let mut shots_used = 0usize;
    let front_shots: Vec<String> = page
        .front
        .get("shots")
        .map(|s| {
            s.trim_matches(['[', ']'])
                .split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        })
        .unwrap_or_default();
    shots_used += front_shots.len();
    let banned = banned_names();
    for (n, line) in prose_lines(&page) {
        for key in directives(line) {
            if !ctx.keys.contains_key(&key) {
                errors.push(format!(
                    "{n}: {{{{ui:{key}}}}} is not a key in crates/text/strings/en.txt"
                ));
            }
        }
        if is_step(line) {
            let without = strip_directives(line);
            for span in bold_spans(&without) {
                errors.push(format!(
                    "{n}: bold `{span}` in a step; name controls with a {{{{ui:key}}}} directive"
                ));
            }
        }
        for id in shot_refs(line) {
            shots_used += 1;
            if let Some(known) = ctx.shots {
                if !known.contains(&id) {
                    errors.push(format!("{n}: shot `{id}` is not in {CATALOG_FILE}"));
                }
            }
        }
        if line.contains('\u{2014}') || line.contains('\u{2013}') {
            errors.push(format!(
                "{n}: no em-dashes or en-dashes; use a plain sentence"
            ));
        }
        let lower = line.to_lowercase();
        for name in &banned {
            if lower.contains(name.as_str()) {
                errors.push(format!("{n}: do not name other design tools"));
            }
        }
        if !line.trim_start().starts_with('#') {
            for (words, sentence) in long_sentences(line) {
                errors.push(format!(
                    "{n}: sentence has {words} words, the limit is {MAX_WORDS}: {sentence}"
                ));
            }
        }
        for target in link_targets(line) {
            errors.extend(check_link(n, page_path, &target, &page, ctx));
        }
    }
    if let Some(known) = ctx.shots {
        for id in &front_shots {
            if !known.contains(id) {
                errors.push(format!(
                    "1: front matter shot `{id}` is not in {CATALOG_FILE}"
                ));
            }
        }
    }
    if shots_used == 0 {
        errors.push("1: page needs at least one shot".into());
    }
    errors
}

/// Removes directives so their text is not mistaken for hand-written bold.
fn strip_directives(line: &str) -> String {
    let mut s = line.to_string();
    while let Some(start) = s.find("{{ui:") {
        match s[start..].find("}}") {
            Some(end) => s.replace_range(start..start + end + 2, ""),
            None => break,
        }
    }
    s
}

/// Checks one link: the file exists and the anchor, when given, is a heading there.
fn check_link(n: usize, page_path: &str, target: &str, page: &Page, ctx: &Context) -> Vec<String> {
    if target.is_empty()
        || target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with("shot:")
    {
        return Vec::new();
    }
    let (file, anchor) = match target.split_once('#') {
        Some((f, a)) => (f, Some(a)),
        None => (target, None),
    };
    if file.is_empty() {
        let own: BTreeSet<String> = page
            .body
            .iter()
            .filter(|(_, l)| l.starts_with('#'))
            .map(|(_, l)| slug(l.trim_start_matches('#')))
            .collect();
        return match anchor {
            Some(a) if !own.contains(a) => {
                vec![format!("{n}: anchor #{a} is not a heading on this page")]
            }
            _ => Vec::new(),
        };
    }
    let path = resolve(page_path, file);
    if !(ctx.exists)(&path) {
        return vec![format!("{n}: link target {path} does not exist")];
    }
    if let (Some(a), true) = (anchor, path.ends_with(".md")) {
        match (ctx.anchors)(&path) {
            Some(set) if !set.contains(a) => {
                return vec![format!("{n}: anchor #{a} is not a heading in {path}")];
            }
            _ => {}
        }
    }
    Vec::new()
}

/// Section directories and page names from index.toml.
fn parse_index(text: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line == "[[section]]" {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("{INDEX_FILE}:{}: expected key = value", i + 1));
        };
        let value = value.trim();
        match key.trim() {
            "dir" => out.push((value.trim_matches('"').to_string(), Vec::new())),
            "title" => {}
            "pages" => {
                let names = value
                    .trim_matches(['[', ']'])
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty());
                match out.last_mut() {
                    Some(section) => section.1.extend(names),
                    None => return Err(format!("{INDEX_FILE}:{}: pages before dir", i + 1)),
                }
            }
            other => return Err(format!("{INDEX_FILE}:{}: unknown key {other}", i + 1)),
        }
    }
    Ok(out)
}

/// Compares the index with the page files found on disk, as `section/name` strings.
fn check_index(index: &[(String, Vec<String>)], on_disk: &BTreeSet<String>) -> Vec<String> {
    let mut errors = Vec::new();
    let dirs: Vec<&str> = index.iter().map(|(d, _)| d.as_str()).collect();
    if dirs != SECTIONS {
        errors.push(format!(
            "{INDEX_FILE}: sections must be {} in that order",
            SECTIONS.join(", ")
        ));
    }
    let mut listed = BTreeSet::new();
    for (dir, pages) in index {
        for p in pages {
            let id = format!("{dir}/{p}");
            if !on_disk.contains(&id) {
                errors.push(format!(
                    "{INDEX_FILE}: {id} is listed but {GUIDE_DIR}/{id}.md does not exist"
                ));
            }
            listed.insert(id);
        }
    }
    for id in on_disk.difference(&listed) {
        errors.push(format!("{INDEX_FILE}: {GUIDE_DIR}/{id}.md is not listed"));
    }
    errors
}

/// Labels a guide could mention: `menu.*`, `board.tool_strip.*` and `docks.workbench_tabs.*`
/// keys that are not tooltips and not full sentences.
fn coverable_keys(keys: &BTreeMap<String, String>) -> BTreeSet<String> {
    keys.iter()
        .filter(|(k, v)| {
            COVERED_PREFIXES.iter().any(|p| k.starts_with(p))
                && !k.ends_with(".tip")
                && !v.trim_end().ends_with('.')
        })
        .map(|(k, _)| k.clone())
        .collect()
}

/// Count of coverable keys that no page mentions by directive.
pub fn coverage_gap(keys: &BTreeMap<String, String>, mentioned: &BTreeSet<String>) -> u64 {
    coverable_keys(keys)
        .iter()
        .filter(|k| !mentioned.contains(*k))
        .count() as u64
}

/// Ratchet comparison: a rise is an error, a fall is a notice to lower the number.
pub fn check_coverage(gap: u64, allowed: Option<u64>) -> (Vec<String>, Vec<String>) {
    match allowed {
        None => (
            vec![format!(
                "{RATCHET_FILE}: [guide_coverage] unmentioned = {gap} is missing"
            )],
            Vec::new(),
        ),
        Some(max) if gap > max => (
            vec![format!(
                "{RATCHET_FILE}: {gap} menu, tool and tab labels have no guide page, the ratchet allows {max}"
            )],
            Vec::new(),
        ),
        Some(max) if gap < max => (
            Vec::new(),
            vec![format!(
                "guide-check: coverage gap is down to {gap}, lower [guide_coverage] unmentioned to {gap}"
            )],
        ),
        Some(_) => (Vec::new(), Vec::new()),
    }
}

/// Reads `unmentioned = N` from the `[guide_coverage]` section of the ratchet file.
fn read_allowed(text: &str) -> Option<u64> {
    let mut in_section = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t == "[guide_coverage]";
        } else if let (true, Some((k, v))) = (in_section, t.split_once('=')) {
            if k.trim() == "unmentioned" {
                return v.trim().parse().ok();
            }
        }
    }
    None
}

/// Shot ids from the catalog: `id = "x"` lines and `[shot.x]` headers.
pub fn catalog_ids(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("id") {
            if let Some((_, v)) = rest.split_once('=') {
                out.insert(v.trim().trim_matches('"').to_string());
            }
        } else if let Some(h) = t.strip_prefix("[shot.").and_then(|r| r.strip_suffix(']')) {
            out.insert(h.trim_matches('"').to_string());
        }
    }
    out
}

/// Collects every `.md` file under a directory.
fn md_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            md_files(&path, out)?;
        } else if path.extension().is_some_and(|x| x == "md") {
            out.push(path);
        }
    }
    Ok(())
}

/// Entry point: checks the index, every page and the coverage ratchet.
/// `--write-ratchet` rewrites `[guide_coverage]` to the current count.
pub fn run(args: &[String]) -> Result<(), String> {
    let root = crate::root()?;
    let (entries, _) = pcbcraft_text::parse(pcbcraft_text::ENGLISH);
    let keys: BTreeMap<String, String> = entries.into_iter().collect();
    let catalog = std::fs::read_to_string(root.join(CATALOG_FILE))
        .ok()
        .map(|t| catalog_ids(&t));
    if catalog.is_none() {
        println!("guide-check: {CATALOG_FILE} does not exist yet, shot ids are not checked");
    }
    let exists = |p: &str| root.join(p).exists();
    let anchors = |p: &str| {
        std::fs::read_to_string(root.join(p))
            .ok()
            .map(|t| heading_anchors(&t))
    };
    let ctx = Context {
        keys: &keys,
        shots: catalog.as_ref(),
        exists: &exists,
        anchors: &anchors,
    };
    let mut files = Vec::new();
    md_files(&root.join(GUIDE_DIR), &mut files)?;
    files.sort();
    let mut errors = Vec::new();
    let mut on_disk = BTreeSet::new();
    let mut mentioned = BTreeSet::new();
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(path).map_err(|e| format!("{rel}: {e}"))?;
        if let Some(id) = rel
            .strip_prefix(&format!("{GUIDE_DIR}/"))
            .and_then(|r| r.strip_suffix(".md"))
        {
            on_disk.insert(id.to_string());
        }
        for line in text.lines() {
            mentioned.extend(directives(line));
        }
        for e in check_page(&rel, &text, &ctx) {
            errors.push(format!("{rel}:{e}"));
        }
    }
    let index_text =
        std::fs::read_to_string(root.join(INDEX_FILE)).map_err(|e| format!("{INDEX_FILE}: {e}"))?;
    errors.extend(check_index(&parse_index(&index_text)?, &on_disk));
    let gap = coverage_gap(&keys, &mentioned);
    let ratchet_path = root.join(RATCHET_FILE);
    let ratchet_text =
        std::fs::read_to_string(&ratchet_path).map_err(|e| format!("{RATCHET_FILE}: {e}"))?;
    if args.iter().any(|a| a == "--write-ratchet") {
        let mut out = String::new();
        let mut in_section = false;
        let mut wrote = false;
        for line in ratchet_text.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                in_section = t == "[guide_coverage]";
            }
            if in_section && t.starts_with("unmentioned") {
                out.push_str(&format!("unmentioned = {gap}\n"));
                wrote = true;
            } else {
                out.push_str(line);
                out.push('\n');
            }
        }
        if !wrote {
            out.push_str(&format!("\n[guide_coverage]\nunmentioned = {gap}\n"));
        }
        std::fs::write(&ratchet_path, out).map_err(|e| format!("{RATCHET_FILE}: {e}"))?;
        println!("wrote {RATCHET_FILE}: unmentioned = {gap}");
    } else {
        let (bad, notices) = check_coverage(gap, read_allowed(&ratchet_text));
        errors.extend(bad);
        for n in notices {
            println!("{n}");
        }
    }
    if errors.is_empty() {
        println!("guide check passed ({} page(s))", files.len());
        Ok(())
    } else {
        for e in &errors {
            eprintln!("guide-check: {e}");
        }
        Err(format!("guide-check: {} problem(s)", errors.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> BTreeMap<String, String> {
        [
            ("board.tool_strip.route", "Route"),
            (
                "board.tool_strip.route_tool_click_two_pads",
                "Route tool: click two pads.",
            ),
            ("menu.file", "File"),
            ("menu.file.tip", "Start, open, save."),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    fn page(body: &str) -> String {
        format!(
            "---\ntitle: T\narea: layout\nkind: howto\nstory: \nshots: []\nsince: 0.1\n---\n{body}\n"
        )
    }

    fn run_page(text: &str, shots: Option<&BTreeSet<String>>) -> Vec<String> {
        let k = keys();
        let exists = |p: &str| p == "docs/mcp.md" || p == "docs/guide/en/start/other.md";
        let anchors = |_: &str| Some(["real-heading".to_string()].into_iter().collect());
        let ctx = Context {
            keys: &k,
            shots,
            exists: &exists,
            anchors: &anchors,
        };
        check_page("docs/guide/en/start/p.md", text, &ctx)
    }

    #[test]
    fn a_clean_page_passes() {
        let text = page("Goal.\n\n1. Pick {{ui:board.tool_strip.route}}.\n\n![](shot:a-shot)");
        assert_eq!(run_page(&text, None), Vec::<String>::new());
    }

    #[test]
    fn an_unknown_ui_key_fails() {
        let text = page("1. Pick {{ui:board.tool_strip.nope}}.\n\n![](shot:a)");
        let errors = run_page(&text, None);
        assert!(
            errors.iter().any(|e| e.contains("board.tool_strip.nope")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_bold_label_without_a_directive_fails() {
        let text = page("1. Pick **Route**.\n\n![](shot:a)");
        let errors = run_page(&text, None);
        assert!(
            errors.iter().any(|e| e.contains("bold `Route`")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_missing_shot_id_fails_once_the_catalog_exists() {
        let text = page("Goal.\n\n![](shot:gone)");
        assert!(run_page(&text, None).is_empty());
        let known: BTreeSet<String> = ["other".to_string()].into_iter().collect();
        let errors = run_page(&text, Some(&known));
        assert!(
            errors.iter().any(|e| e.contains("shot `gone`")),
            "{errors:?}"
        );
        assert!(
            run_page(&page("Goal."), None)
                .iter()
                .any(|e| e.contains("at least one shot"))
        );
    }

    #[test]
    fn a_long_sentence_fails() {
        let long = "word ".repeat(21);
        let errors = run_page(&page(&format!("{long}end.\n\n![](shot:a)")), None);
        assert!(
            errors.iter().any(|e| e.contains("limit is 20")),
            "{errors:?}"
        );
        let dash = run_page(&page("Short \u{2014} text.\n\n![](shot:a)"), None);
        assert!(dash.iter().any(|e| e.contains("em-dashes")), "{dash:?}");
    }

    #[test]
    fn a_broken_internal_link_fails() {
        let ok = page("See [mcp](../../../mcp.md) and [x](other.md#real-heading).\n\n![](shot:a)");
        assert!(run_page(&ok, None).is_empty(), "{:?}", run_page(&ok, None));
        let bad =
            page("See [gone](missing.md) and [x](other.md#nope) and [y](#nowhere).\n\n![](shot:a)");
        assert_eq!(run_page(&bad, None).len(), 3);
    }

    #[test]
    fn the_coverage_ratchet_rejects_a_rise() {
        let k = keys();
        let none = BTreeSet::new();
        assert_eq!(coverage_gap(&k, &none), 2);
        let one: BTreeSet<String> = ["menu.file".to_string()].into_iter().collect();
        assert_eq!(coverage_gap(&k, &one), 1);
        assert!(!check_coverage(3, Some(2)).0.is_empty());
        assert!(check_coverage(2, Some(2)).0.is_empty());
        let (errors, notices) = check_coverage(1, Some(2));
        assert!(errors.is_empty() && !notices.is_empty());
        assert!(!check_coverage(1, None).0.is_empty());
    }

    #[test]
    fn the_index_must_match_the_pages_on_disk() {
        let index = parse_index(
            "[[section]]\ndir = \"start\"\ntitle = \"Start here\"\npages = [\"a\"]\n\
             [[section]]\ndir = \"howto\"\npages = []\n[[section]]\ndir = \"reference\"\npages = []\n\
             [[section]]\ndir = \"concepts\"\npages = []\n",
        )
        .unwrap_or_default();
        let disk: BTreeSet<String> = ["start/a".to_string()].into_iter().collect();
        assert!(check_index(&index, &disk).is_empty());
        let extra: BTreeSet<String> = ["start/a".to_string(), "start/b".to_string()]
            .into_iter()
            .collect();
        assert_eq!(check_index(&index, &extra).len(), 1);
    }

    #[test]
    fn a_directive_renders_as_the_bold_label() {
        let out = render_directives("Pick {{ui:board.tool_strip.route}}.", &keys());
        assert_eq!(out, "Pick **Route**.");
    }
}
