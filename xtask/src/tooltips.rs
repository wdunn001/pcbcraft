//! Tooltip content lint behind `cargo xtask tooltips`.
//!
//! Context: docs/tooltips.md is the standard and its checklist is what this file
//! enforces. The scan is a plain source read, so it needs no build and runs in
//! a second. It reads `.on_hover_text(` and `.on_disabled_hover_text(` string
//! literals under crates/ui-egui/src, and `key.tip = text` lines in
//! crates/text/strings/en.txt once the text catalog (#349) is on main.
//!
//! Rules: a finding is `file:line`, a rule name and the tip text. Exemptions
//! live in crates/ui-egui/tooltip-exemptions.txt and each needs a reason. The
//! default run lists findings and exits zero; `--strict` fails on any finding
//! or any exemption that no longer matches. Tips built at run time (a
//! `command_tip(..)` call, a variable) have no literal and are skipped.
//! The e2e gate (#393) calls `run` with `--strict` once the audit is clean.
//!
//! # Dependencies
//! - docs/tooltips.md
//! - crates/ui-egui/tooltip-exemptions.txt
//! - crates/ui-egui/src
//!
//! # Do not
//! - Add an exemption without a reason, or make the default run fatal before #631.
use std::path::{Path, PathBuf};

/// Tips with fewer words than this say too little to be a sentence.
const MIN_WORDS: usize = 4;
/// Two short lines at the width of a tooltip.
const MAX_CHARS: usize = 220;
/// Where the exemption list lives, relative to the repo root.
const EXEMPTIONS: &str = "crates/ui-egui/tooltip-exemptions.txt";
/// Rust sources scanned for tooltip literals.
const SOURCE_DIR: &str = "crates/ui-egui/src";
/// The text catalog, read when present.
const CATALOG: &str = "crates/text/strings/en.txt";

/// Words that say a tooltip names a limit.
const LIMIT_WORDS: [&str; 16] = [
    "between",
    "at least",
    "at most",
    "up to",
    "no more",
    "minimum",
    "maximum",
    "largest",
    "smallest",
    "larger than",
    "smaller than",
    "must",
    "only",
    "within",
    "range",
    "refuses",
];
/// Words that say a disabled tooltip tells the user how to enable the control.
const WAY_FORWARD: [&str; 12] = [
    "first", "select", "pick", "choose", "add ", "open", "need", "make", "after", "once", "until",
    "start",
];
/// Words ignored when comparing a tooltip with its label.
const FILLER: [&str; 12] = [
    "the", "a", "an", "this", "your", "to", "of", "on", "in", "it", "for", "and",
];

/// One tooltip found in the source or the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tip {
    /// Repo-relative file the text came from.
    pub file: String,
    /// One-based line of the call or catalog entry.
    pub line: usize,
    /// The literal text, escapes resolved for the common cases.
    pub text: String,
    /// Label of the control, when one sits just before the call.
    pub label: Option<String>,
    /// True for `on_disabled_hover_text`.
    pub disabled: bool,
    /// True when the control just before the call has bounds (a range or a slider).
    pub bounded: bool,
}

/// One rule failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Rule name, also the second column of an exemption line.
    pub rule: &'static str,
    /// The tip that failed.
    pub tip: Tip,
}

/// Run the `tooltips` subcommand from the repo root. `--strict` makes findings fatal.
pub fn run(args: &[String]) -> Result<(), String> {
    let strict = args.iter().any(|a| a == "--strict");
    let root = Path::new(".");
    let mut tips = Vec::new();
    let mut files = Vec::new();
    collect_rs(&root.join(SOURCE_DIR), &mut files)?;
    files.sort();
    for path in files {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path
            .to_string_lossy()
            .trim_start_matches("./")
            .replace('\\', "/");
        tips.extend(scan_source(&name, &text));
    }
    if let Ok(text) = std::fs::read_to_string(root.join(CATALOG)) {
        tips.extend(scan_catalog(CATALOG, &text));
    }
    let exemption_text = std::fs::read_to_string(root.join(EXEMPTIONS)).unwrap_or_default();
    let exemptions = parse_exemptions(&exemption_text)?;
    let (findings, stale) = apply_exemptions(check(&tips), &exemptions);
    for f in &findings {
        eprintln!("{}:{}: {}: {}", f.tip.file, f.tip.line, f.rule, f.tip.text);
    }
    for e in &stale {
        eprintln!(
            "{EXEMPTIONS}: exemption matches nothing: {} | {} | {}",
            e.file, e.rule, e.text
        );
    }
    eprintln!(
        "tooltips: {} checked, {} failing, {} exempt, {} stale exemptions",
        tips.len(),
        findings.len(),
        exemptions.len() - stale.len(),
        stale.len()
    );
    if strict && (!findings.is_empty() || !stale.is_empty()) {
        return Err("tooltip lint failed (see docs/tooltips.md)".into());
    }
    Ok(())
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            // Story files under e2e/ hold test text, not shipped tooltips.
            if path.file_name().is_some_and(|n| n == "e2e") {
                continue;
            }
            collect_rs(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Find every tooltip literal in one Rust source file.
pub fn scan_source(file: &str, text: &str) -> Vec<Tip> {
    let mut tips = Vec::new();
    for (marker, disabled) in [
        (".on_hover_text(", false),
        (".on_disabled_hover_text(", true),
    ] {
        let mut from = 0;
        while let Some(pos) = text[from..].find(marker) {
            let at = from + pos;
            from = at + marker.len();
            let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
            if text[line_start..at].trim_start().starts_with("//") {
                continue;
            }
            let arg = &text[from..];
            let Some(literal) = leading_literals(arg) else {
                continue;
            };
            let before = &text[at.saturating_sub(700)..at];
            let window = last_lines(before, 8);
            // Only the statement that ends in this call counts, not the buttons before it.
            let window = window.rfind(";\n").map_or(window, |i| &window[i..]);
            tips.push(Tip {
                file: file.to_string(),
                line: text[..at].matches('\n').count() + 1,
                text: literal,
                label: last_label(window),
                disabled,
                bounded: ["DragValue", "Slider", ".range(", ".clamp_range("]
                    .iter()
                    .any(|m| window.contains(m)),
            });
        }
    }
    tips.sort_by_key(|t| t.line);
    tips
}

/// Read `key.tip = text` lines from the catalog; the label is the sibling `key` entry.
pub fn scan_catalog(file: &str, text: &str) -> Vec<Tip> {
    let mut entries = std::collections::BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            entries.insert(k.trim().to_string(), (n + 1, v.trim().to_string()));
        }
    }
    entries
        .iter()
        .filter_map(|(key, (line, value))| {
            let base = key.strip_suffix(".tip")?;
            Some(Tip {
                file: file.to_string(),
                line: *line,
                text: value.clone(),
                label: entries.get(base).map(|(_, v)| v.clone()),
                disabled: base.ends_with(".disabled"),
                bounded: false,
            })
        })
        .collect()
}

/// The string literal (or adjacent literals joined) at the start of a call argument.
fn leading_literals(arg: &str) -> Option<String> {
    let mut rest = arg.trim_start();
    if rest.starts_with("format!") || rest.starts_with("if ") || rest.starts_with("match ") {
        return None;
    }
    let mut out = String::new();
    while rest.starts_with('"') {
        let (piece, after) = take_string(rest)?;
        out.push_str(&piece);
        rest = after.trim_start();
        if !rest.starts_with('"') {
            break;
        }
    }
    // A `)` must close the call; anything else (`.to_string()`, `+`) means it is built.
    if out.is_empty() || !rest.starts_with(')') {
        return None;
    }
    Some(out)
}

/// Read one quoted literal, returning its text and what follows it.
fn take_string(s: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut chars = s.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, &s[i + 1..])),
            '\\' => match chars.next()?.1 {
                'n' => out.push(' '),
                'u' => {
                    // \u{2212} style escapes: skip to the closing brace.
                    for (_, d) in chars.by_ref() {
                        if d == '}' {
                            break;
                        }
                    }
                    out.push('?');
                }
                // A trailing backslash continues the literal on the next line.
                '\n' => {
                    while let Some(&(_, w)) = chars.clone().peekable().peek() {
                        if w.is_whitespace() {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

fn last_lines(s: &str, n: usize) -> &str {
    let mut idx = s.len();
    for _ in 0..n {
        match s[..idx].rfind('\n') {
            Some(i) => idx = i,
            None => return s,
        }
    }
    &s[idx..]
}

/// The label literal of the control built just before a tooltip call.
fn last_label(window: &str) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for marker in [
        ".button(",
        ".small_button(",
        "Button::new(",
        ".checkbox(",
        ".selectable_label(",
        ".selectable_value(",
        ".radio_value(",
        ".label(",
        ".text(",
    ] {
        let mut from = 0;
        while let Some(pos) = window[from..].find(marker) {
            let at = from + pos;
            from = at + marker.len();
            let args = &window[from..];
            let end = args
                .find(")\n")
                .or_else(|| args.find(')'))
                .unwrap_or(args.len());
            let call = &args[..end];
            if let Some(lit) = call.split('"').nth(1)
                && best.as_ref().is_none_or(|(p, _)| at >= *p)
            {
                best = Some((at, lit.to_string()));
            }
        }
    }
    best.map(|(_, label)| label)
}

fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Run every rule over the tips.
pub fn check(tips: &[Tip]) -> Vec<Finding> {
    let mut found = Vec::new();
    for tip in tips {
        let mut fail = |rule: &'static str| {
            found.push(Finding {
                rule,
                tip: tip.clone(),
            });
        };
        let tip_words = words(&tip.text);
        if tip_words.len() < MIN_WORDS {
            fail("too-short");
        }
        if tip.text.chars().count() > MAX_CHARS {
            fail("too-long");
        }
        if tip.text.contains('\u{2014}') || tip.text.contains('\u{2013}') {
            fail("dash");
        }
        if let Some(label) = &tip.label {
            let label_words = words(label);
            let meaningful: Vec<&String> = tip_words
                .iter()
                .filter(|w| !FILLER.contains(&w.as_str()))
                .collect();
            if !label_words.is_empty()
                && !meaningful.is_empty()
                && meaningful.iter().all(|w| label_words.contains(w))
            {
                fail("repeats-label");
            }
        }
        let lower = tip.text.to_lowercase();
        if tip.bounded
            && !tip.disabled
            && !LIMIT_WORDS.iter().any(|w| lower.contains(w))
            && !lower.chars().any(|c| c.is_ascii_digit())
        {
            fail("missing-limit");
        }
        if tip.disabled && !WAY_FORWARD.iter().any(|w| lower.contains(w)) {
            fail("disabled-no-way-forward");
        }
    }
    found
}

/// One reviewed exemption: `file | rule | tip text | reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exemption {
    /// Repo-relative file of the tip.
    pub file: String,
    /// Rule name being waived.
    pub rule: String,
    /// Exact tip text.
    pub text: String,
}

/// Parse the exemption file. A line without a reason is an error.
pub fn parse_exemptions(text: &str) -> Result<Vec<Exemption>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(4, '|').map(str::trim).collect();
        match parts.as_slice() {
            [file, rule, text, reason] if !reason.is_empty() && !text.is_empty() => {
                out.push(Exemption {
                    file: (*file).into(),
                    rule: (*rule).into(),
                    text: (*text).into(),
                });
            }
            _ => {
                return Err(format!(
                    "{EXEMPTIONS}:{}: expected `file | rule | tip text | reason` with a reason",
                    n + 1
                ));
            }
        }
    }
    Ok(out)
}

/// Drop exempt findings; return what is left and the exemptions that matched nothing.
pub fn apply_exemptions(
    findings: Vec<Finding>,
    exemptions: &[Exemption],
) -> (Vec<Finding>, Vec<Exemption>) {
    let matches = |e: &Exemption, f: &Finding| {
        e.file == f.tip.file && e.rule == f.rule && e.text == f.tip.text
    };
    let kept = findings
        .iter()
        .filter(|f| !exemptions.iter().any(|e| matches(e, f)))
        .cloned()
        .collect();
    let stale = exemptions
        .iter()
        .filter(|e| !findings.iter().any(|f| matches(e, f)))
        .cloned()
        .collect();
    (kept, stale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tip(text: &str) -> Tip {
        Tip {
            file: "a.rs".into(),
            line: 1,
            text: text.into(),
            label: None,
            disabled: false,
            bounded: false,
        }
    }

    fn rules(tip: Tip) -> Vec<&'static str> {
        check(&[tip]).iter().map(|f| f.rule).collect()
    }

    #[test]
    fn tooltip_lint_rejects_a_tip_that_repeats_its_label() {
        let mut t = tip("Save the design file");
        t.label = Some("Save design file".into());
        assert_eq!(rules(t), ["repeats-label"]);
    }

    #[test]
    fn tooltip_lint_accepts_a_tip_that_adds_to_its_label() {
        let mut t = tip("Write the design to its file on disk.");
        t.label = Some("Save".into());
        assert!(rules(t).is_empty());
    }

    #[test]
    fn tooltip_lint_rejects_a_tip_under_the_word_floor() {
        assert_eq!(rules(tip("Zoom in")), ["too-short"]);
        assert!(rules(tip("Zoom in on the drawing.")).is_empty());
    }

    #[test]
    fn tooltip_lint_rejects_a_long_tip_and_a_dash() {
        assert_eq!(rules(tip(&"word ".repeat(60))), ["too-long"]);
        assert_eq!(rules(tip("Move it \u{2014} then drop it here.")), ["dash"]);
    }

    #[test]
    fn tooltip_lint_rejects_a_bounded_control_with_no_limit() {
        let mut t = tip("Set how wide the track is drawn.");
        t.bounded = true;
        assert_eq!(rules(t.clone()), ["missing-limit"]);
        t.text = "Set the track width, between 0.1 and 5 mm.".into();
        assert!(rules(t).is_empty());
    }

    #[test]
    fn tooltip_lint_rejects_a_disabled_control_with_no_way_forward() {
        let mut t = tip("This button is turned off right now.");
        t.disabled = true;
        assert_eq!(rules(t.clone()), ["disabled-no-way-forward"]);
        t.text = "Nothing to undo yet: make an edit first.".into();
        assert!(rules(t).is_empty());
    }

    #[test]
    fn source_scan_reads_literals_labels_and_bounds() {
        let src = "ui.button(\"Save\")\n    .on_hover_text(\"Write the design \\\n        to disk.\")\n    .clicked();\nui.add(egui::DragValue::new(&mut x))\n    .on_hover_text(command_tip(\"a\"));\nui.add_enabled(c, egui::Button::new(\"Undo\"))\n    .on_disabled_hover_text(\"Nothing to undo yet.\");\n";
        let tips = scan_source("a.rs", src);
        assert_eq!(tips.len(), 2);
        assert_eq!(tips[0].text, "Write the design to disk.");
        assert_eq!(tips[0].label.as_deref(), Some("Save"));
        assert!(tips[1].disabled);
        assert_eq!(tips[1].label.as_deref(), Some("Undo"));
    }

    #[test]
    fn catalog_scan_pairs_a_tip_with_its_label() {
        let tips = scan_catalog("en.txt", "# c\napp.home = Home\napp.home.tip = Home\n");
        assert_eq!(tips.len(), 1);
        assert_eq!(tips[0].label.as_deref(), Some("Home"));
        assert_eq!(rules(tips[0].clone()), ["too-short", "repeats-label"]);
    }

    #[test]
    fn exemptions_need_a_reason_and_go_stale_when_unused() {
        assert!(parse_exemptions("a.rs | too-short | Zoom in |").is_err());
        let ex = parse_exemptions("a.rs | too-short | Zoom in | label is the whole idea")
            .unwrap_or_default();
        assert_eq!(ex.len(), 1);
        let (kept, stale) = apply_exemptions(check(&[tip("Zoom in")]), &ex);
        assert!(kept.is_empty() && stale.is_empty());
        let (kept, stale) = apply_exemptions(check(&[tip("Zoom out on the drawing.")]), &ex);
        assert!(kept.is_empty());
        assert_eq!(stale.len(), 1);
    }
}
