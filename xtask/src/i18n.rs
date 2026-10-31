//! `cargo xtask i18n`: the localization gate.
//!
//! Context: the text catalog is `crates/text/strings/*.txt` (English is the base). The
//! undefined-key and converted-function checks stay in `cargo xtask strings check`; this
//! file adds what that leaves out. Docs: `docs/localization.md`.
//! Local rules: counts live in `crates/text/strings/i18n-ratchet.txt` and only go down.
//! The pure functions take text so tests use fixtures.
//!
//! # Dependencies
//! - crates/text/strings/i18n-ratchet.txt
//! - crates/text/src/lib.rs (`parse`)
//! - crates/ui-egui/src/e2e/i18n.rs runs the pseudo-locale and font checks
//!
//! # Do not
//! - Raise a count; translate the key, convert the call or remove the dead key.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Calls whose first argument is text a person reads.
const TEXT_CALLS: [&str; 11] = [
    ".button(",
    ".small_button(",
    ".label(",
    ".heading(",
    ".on_hover_text(",
    ".on_disabled_hover_text(",
    ".strong(",
    ".weak(",
    "RichText::new(",
    "Label::new(",
    "Button::new(",
];

/// Plural and tooltip suffixes that belong to a base key.
const SUFFIXES: [&str; 8] = [
    ".tip", ".zero", ".one", ".two", ".few", ".many", ".other", ".aria",
];

/// Keys English defines that `locale` lacks.
pub fn missing_keys(english: &[(String, String)], locale: &[(String, String)]) -> Vec<String> {
    let have: BTreeSet<&str> = locale.iter().map(|(k, _)| k.as_str()).collect();
    english
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| !have.contains(k))
        .map(str::to_owned)
        .collect()
}

/// The `{name}` placeholders of a value with how often each occurs.
pub fn placeholder_counts(value: &str) -> BTreeMap<String, usize> {
    let mut found = BTreeMap::new();
    let mut rest = value;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let name = &after[..close];
        if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            *found.entry(name.to_owned()).or_default() += 1;
        }
        rest = &after[close + 1..];
    }
    found
}

/// Keys whose placeholder names or counts differ from English.
pub fn placeholder_mismatches(
    english: &[(String, String)],
    locale: &[(String, String)],
) -> Vec<String> {
    let base: BTreeMap<&str, &str> = english
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    locale
        .iter()
        .filter_map(|(k, v)| {
            let en = base.get(k.as_str())?;
            (placeholder_counts(en) != placeholder_counts(v)).then(|| k.clone())
        })
        .collect()
}

/// Every string literal in `source`, without its quotes.
fn literals(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\'' if chars.get(i + 2) == Some(&'\'') => i += 2,
            '\'' if chars.get(i + 1) == Some(&'\\') && chars.get(i + 3) == Some(&'\'') => i += 3,
            '"' => {
                let mut text = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    if let Some(c) = chars.get(i) {
                        text.push(*c);
                    }
                    i += 1;
                }
                out.push(text);
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// Keys no source refers to. A key counts as used when its text, or its base without a
/// `.tip` or plural suffix, appears as a string literal, or when a literal that ends in `.`
/// or holds `{` is the start of the key (a key built at run time).
pub fn unused_keys(keys: &[String], sources: &[String]) -> Vec<String> {
    let mut lits: BTreeSet<String> = BTreeSet::new();
    for s in sources {
        lits.extend(literals(s));
    }
    let prefixes: Vec<String> = lits
        .iter()
        .filter(|l| l.len() >= 3 && (l.ends_with('.') || l.contains('{')))
        .map(|l| l.split('{').next().unwrap_or_default().to_owned())
        .filter(|p| p.len() >= 3)
        .collect();
    keys.iter()
        .filter(|k| {
            let mut bases = vec![k.as_str()];
            for suffix in SUFFIXES {
                if let Some(b) = k.strip_suffix(suffix) {
                    bases.push(b);
                    for again in SUFFIXES {
                        if let Some(bb) = b.strip_suffix(again) {
                            bases.push(bb);
                        }
                    }
                }
            }
            !bases.iter().any(|b| lits.contains(*b)) && !prefixes.iter().any(|p| k.starts_with(p))
        })
        .cloned()
        .collect()
}

/// User-visible string literals handed straight to a text call, outside test code.
/// A literal needs two letters in a row to count; a lone glyph such as "x" is not text.
pub fn hardcoded_literals(source: &str) -> usize {
    let code = source.split("#[cfg(test)]").next().unwrap_or(source);
    let mut count = 0;
    for call in TEXT_CALLS {
        let mut rest = code;
        while let Some(at) = rest.find(call) {
            let after = rest[at + call.len()..].trim_start();
            if let Some(body) = after.strip_prefix('"') {
                let lit = literals(&format!("\"{body}"))
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                let letters = lit
                    .chars()
                    .collect::<Vec<_>>()
                    .windows(2)
                    .any(|w| w[0].is_alphabetic() && w[1].is_alphabetic());
                if letters {
                    count += 1;
                }
            }
            rest = &rest[at + call.len()..];
        }
    }
    count
}

/// Catalog lookups in `text`: `t!(` calls and `catalog.get(` calls.
fn lookups(text: &str) -> usize {
    let bytes = text.as_bytes();
    let macros = text
        .match_indices("t!(")
        .filter(|(at, _)| {
            *at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_')
        })
        .count();
    macros + text.matches("catalog.get(").count()
}

/// The text between the parentheses that open at `from` (the byte after `(`).
fn balanced(text: &str, from: usize) -> &str {
    let mut depth = 1usize;
    for (i, c) in text[from..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &text[from..from + i];
                }
            }
            _ => {}
        }
    }
    &text[from..]
}

/// Places that join translated fragments: a `format!` call holding two catalog lookups, or a
/// lookup added to another with `+`.
pub fn concatenations(source: &str) -> usize {
    let code = source.split("#[cfg(test)]").next().unwrap_or(source);
    let mut count = 0;
    for (at, _) in code.match_indices("format!(") {
        if lookups(balanced(code, at + "format!(".len())) >= 2 {
            count += 1;
        }
    }
    for statement in code.split(';') {
        let plus = statement.contains("+ &t!(") || statement.contains("+ t!(");
        if plus && lookups(statement) >= 2 {
            count += 1;
        }
    }
    count
}

/// The ratchet file: `kind | scope | count` lines.
pub fn parse_ratchet(text: &str) -> Result<BTreeMap<(String, String), usize>, String> {
    let mut out = BTreeMap::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let p: Vec<&str> = line.split('|').map(str::trim).collect();
        let [kind, scope, count] = p[..] else {
            return Err(format!(
                "i18n-ratchet.txt: expected `kind | scope | count`: {line}"
            ));
        };
        let n = count
            .parse()
            .map_err(|_| format!("i18n-ratchet.txt: bad count in `{line}`"))?;
        out.insert((kind.to_owned(), scope.to_owned()), n);
    }
    Ok(out)
}

/// Compare measured counts with the ratchet; one message per count that moved.
pub fn compare(
    now: &BTreeMap<(String, String), usize>,
    ratchet: &BTreeMap<(String, String), usize>,
) -> Vec<String> {
    let mut problems = Vec::new();
    let keys: BTreeSet<_> = now.keys().chain(ratchet.keys()).collect();
    for key in keys {
        let have = now.get(key).copied().unwrap_or(0);
        let max = ratchet.get(key).copied().unwrap_or(0);
        if have > max {
            problems.push(format!(
                "{} {}: {have}, the ratchet allows {max}",
                key.0, key.1
            ));
        } else if have < max {
            problems.push(format!(
                "{} {}: {have} now, the ratchet says {max}; lower it in crates/text/strings/i18n-ratchet.txt",
                key.0, key.1
            ));
        }
    }
    problems
}

/// Ratchet lines for the current counts, for pasting into the file.
pub fn ratchet_lines(now: &BTreeMap<(String, String), usize>) -> String {
    now.iter()
        .filter(|(_, n)| **n > 0)
        .map(|((k, s), n)| format!("{k} | {s} | {n}\n"))
        .collect()
}

fn rust_sources(dir: &Path, out: &mut Vec<(String, String)>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            rust_sources(&path, out)?;
        } else if path.extension().is_some_and(|x| x == "rs") {
            let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            out.push((path.to_string_lossy().into_owned(), text));
        }
    }
    Ok(())
}

/// Runs the checks. `--write` prints the current ratchet lines instead of comparing.
pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    use pcbcraft_text::{ENGLISH, parse};
    let strings = root.join("crates/text/strings");
    let english = parse(ENGLISH).0;
    let mut now: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut problems = Vec::new();
    let mut table = String::from("| Check | Scope | Count |\n| --- | --- | --- |\n");
    for language in pcbcraft_text::shipped_languages() {
        let tag = language.tag.as_str();
        if tag == "en" || tag.starts_with("qps-") {
            continue;
        }
        let text = std::fs::read_to_string(strings.join(format!("{tag}.txt")))
            .map_err(|e| e.to_string())?;
        let locale = parse(&text).0;
        let missing = missing_keys(&english, &locale).len();
        now.insert(("missing".into(), tag.into()), missing);
        for key in placeholder_mismatches(&english, &locale) {
            problems.push(format!("{tag}: `{key}` has different placeholders than en"));
        }
    }
    let mut files = Vec::new();
    rust_sources(&root.join("crates/ui-egui/src"), &mut files)?;
    let ui: Vec<&(String, String)> = files.iter().filter(|(p, _)| !p.contains("/e2e/")).collect();
    for (path, source) in &ui {
        let short = path
            .rsplit("ui-egui/src/")
            .next()
            .unwrap_or(path)
            .to_owned();
        let literal = hardcoded_literals(source);
        if literal > 0 {
            now.insert(("literals".into(), short.clone()), literal);
        }
        let joined = concatenations(source);
        if joined > 0 {
            now.insert(("concatenation".into(), short), joined);
        }
    }
    let mut all = Vec::new();
    for dir in ["crates", "apps"] {
        rust_sources(&root.join(dir), &mut all)?;
    }
    let sources: Vec<String> = all.into_iter().map(|(_, s)| s).collect();
    let keys: Vec<String> = english.iter().map(|(k, _)| k.clone()).collect();
    now.insert(
        ("unused".into(), "keys".into()),
        unused_keys(&keys, &sources).len(),
    );
    for ((kind, scope), n) in &now {
        if *n > 0 {
            table.push_str(&format!("| {kind} | {scope} | {n} |\n"));
        }
    }
    if args.iter().any(|a| a == "--write") {
        print!("{}", ratchet_lines(&now));
        return Ok(());
    }
    let ratchet = parse_ratchet(
        &std::fs::read_to_string(strings.join("i18n-ratchet.txt")).map_err(|e| e.to_string())?,
    )?;
    problems.extend(compare(&now, &ratchet));
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") {
        let _ = std::fs::write(&path, format!("### Localization counts\n\n{table}\n"));
    }
    println!("{table}");
    if problems.is_empty() {
        println!("i18n: catalog checks clean (counts match the ratchet)");
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(text: &str) -> Vec<(String, String)> {
        pcbcraft_text::parse(text).0
    }

    #[test]
    fn a_missing_key_is_found() {
        let en = entries("a = A\nb = B\nc = C");
        let de = entries("a = x\nc = z");
        assert_eq!(missing_keys(&en, &de), vec!["b".to_owned()]);
    }

    #[test]
    fn placeholder_counts_and_names_must_match() {
        let en = entries("one = {n} of {n}\ntwo = {a}\nthree = plain");
        let ok = entries("one = {n} von {n}\ntwo = {a}!");
        assert!(placeholder_mismatches(&en, &ok).is_empty());
        let bad = entries("one = {n} von\ntwo = {b}");
        assert_eq!(placeholder_mismatches(&en, &bad), vec!["one", "two"]);
    }

    #[test]
    fn unused_keys_honour_suffixes_and_built_keys() {
        let keys: Vec<String> = [
            "a.b",
            "a.b.tip",
            "n.one",
            "n.other",
            "dead.key",
            "err.code_7",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let src = vec![r#"t!("a.b"); plural("n", 2); format!("err.{}", c);"#.to_owned()];
        assert_eq!(unused_keys(&keys, &src), vec!["dead.key".to_owned()]);
    }

    #[test]
    fn literals_in_text_calls_are_counted_outside_tests() {
        let src = r#"
            ui.label("Save the board");
            ui.button( "Go" );
            ui.label(t!("ok"));
            ui.button("x");
            ui.label(format!("{}", 1));
            #[cfg(test)] mod t { fn f() { ui.label("In a test"); } }
        "#;
        assert_eq!(hardcoded_literals(src), 2);
    }

    #[test]
    fn two_lookups_in_one_format_are_a_concatenation() {
        let bad = r#"let s = format!("{} {}", t!("a"), t!("b"));"#;
        let ok = r#"let s = format!("{}", t!("a")); let u = t!("b");"#;
        assert_eq!(concatenations(bad), 1);
        assert_eq!(concatenations(ok), 0);
    }

    #[test]
    fn the_ratchet_fails_on_a_rise_and_on_a_fall() -> Result<(), String> {
        let file = parse_ratchet("missing | de | 5\n# c\nunused | keys | 2\n")?;
        let mut now = BTreeMap::new();
        now.insert(("missing".to_owned(), "de".to_owned()), 6);
        now.insert(("unused".to_owned(), "keys".to_owned()), 1);
        let problems = compare(&now, &file);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].contains("allows 5"));
        assert!(problems[1].contains("lower it"));
        assert!(parse_ratchet("bad line").is_err());
        Ok(())
    }
}
