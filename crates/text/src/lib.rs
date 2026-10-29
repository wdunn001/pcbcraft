//! One text catalog for every user-visible string.
//!
//! English is the base: `strings/en.txt` is embedded in the program and is
//! the authority on which keys exist. Another language is a plain
//! `key = value` file that overrides some or all of those keys; any key it
//! leaves out shows the English text and is reported once.
//!
//! Screens read text through [`t!`], which asks the catalog that the running
//! frame activated with [`activate`].

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

/// The English base catalog, embedded so the desktop and browser builds both
/// carry it.
pub const ENGLISH: &str = include_str!("../strings/en.txt");
/// Functions whose text already goes through the catalog; see the file.
pub const CONVERTED_FNS: &str = include_str!("../strings/converted-fns.txt");
#[cfg(feature = "fixture")]
const FIXTURE_FR: &str = include_str!("../strings/fixture-fr.txt");

/// Language tag meaning "follow the computer's language".
pub const SYSTEM: &str = "system";

/// A catalog line that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// One-based line number.
    pub line: usize,
    /// What is wrong with it.
    pub reason: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.reason)
    }
}

/// True for a well-formed key: lowercase letters, digits, `.`, `-`, `_`.
pub fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
        })
}

/// Read `key = value` lines. Blank lines and `#` comments are ignored. The
/// result keeps file order and keeps duplicates so a checker can report
/// them; every malformed line is returned in the error list.
pub fn parse(text: &str) -> (Vec<(String, String)>, Vec<ParseError>) {
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            errors.push(ParseError {
                line: index + 1,
                reason: "expected `key = value`".into(),
            });
            continue;
        };
        let key = key.trim();
        if !valid_key(key) {
            errors.push(ParseError {
                line: index + 1,
                reason: format!("bad key `{key}`"),
            });
            continue;
        }
        entries.push((key.to_string(), value.trim().to_string()));
    }
    (entries, errors)
}

/// The `{name}` placeholders a value uses, sorted and unique.
pub fn placeholders(value: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = value;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            break;
        };
        let name = &after[..close];
        if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            found.insert(name.to_string());
        }
        rest = &after[close + 1..];
    }
    found
}

/// A language the app can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    /// Tag such as `en` or `de`.
    pub tag: String,
    /// The language's name in itself, from its `language.name` key.
    pub name: String,
}

/// The language files this build carries, English first. A test-only
/// fixture language is listed only with the `fixture` feature.
pub fn shipped_languages() -> Vec<Language> {
    let mut list = vec![Language {
        tag: "en".into(),
        name: language_name(ENGLISH),
    }];
    list.extend(fixture_language());
    list
}

#[cfg(feature = "fixture")]
fn fixture_language() -> Option<Language> {
    Some(Language {
        tag: "fr".into(),
        name: language_name(FIXTURE_FR),
    })
}

#[cfg(not(feature = "fixture"))]
fn fixture_language() -> Option<Language> {
    None
}

fn language_name(text: &str) -> String {
    parse(text)
        .0
        .into_iter()
        .find(|(key, _)| key == "language.name")
        .map(|(_, value)| value)
        .unwrap_or_default()
}

fn language_text(tag: &str) -> Option<&'static str> {
    match tag {
        "en" => Some(ENGLISH),
        #[cfg(feature = "fixture")]
        "fr" => Some(FIXTURE_FR),
        _ => None,
    }
}

/// Map a system locale such as `de_DE.UTF-8` to a shipped tag, or `en`.
pub fn match_locale(locale: &str) -> String {
    let primary: String = locale
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase();
    if shipped_languages().iter().any(|l| l.tag == primary) {
        primary
    } else {
        "en".into()
    }
}

/// The language of this computer, from the usual environment variables.
/// The browser build and unknown values give `en`.
pub fn resolve_system_language() -> String {
    for name in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(name)
            && !value.is_empty()
        {
            return match_locale(&value);
        }
    }
    "en".into()
}

/// Loaded strings for one language over the English base.
#[derive(Debug)]
pub struct Catalog {
    tag: String,
    english: BTreeMap<String, String>,
    overrides: BTreeMap<String, String>,
    reported: Mutex<Vec<String>>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self::english()
    }
}

impl Catalog {
    /// The English catalog.
    pub fn english() -> Self {
        Self {
            tag: "en".into(),
            english: parse(ENGLISH).0.into_iter().collect(),
            overrides: BTreeMap::new(),
            reported: Mutex::new(Vec::new()),
        }
    }

    /// The catalog for a tag. `system` follows the computer; an unknown or
    /// invalid tag gives English.
    pub fn for_language(tag: &str) -> Self {
        let tag = if tag == SYSTEM {
            resolve_system_language()
        } else {
            tag.to_string()
        };
        match language_text(&tag) {
            Some(text) if tag != "en" => Self::english().with_language(&tag, text),
            _ => Self::english(),
        }
    }

    /// English with the lines of `text` laid over it. Lines with a key that
    /// English does not define are ignored, so a stale file cannot add text.
    pub fn with_language(mut self, tag: &str, text: &str) -> Self {
        self.tag = tag.to_string();
        self.overrides = parse(text)
            .0
            .into_iter()
            .filter(|(key, _)| self.english.contains_key(key))
            .collect();
        self
    }

    /// The tag of the loaded language.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Every key English defines, in sorted order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.english.keys().map(String::as_str)
    }

    /// The text for a key. A key the language lacks shows the English text;
    /// a key nobody defines shows the key itself. Either is reported once.
    pub fn get(&self, key: &str) -> String {
        if let Some(value) = self.overrides.get(key) {
            return value.clone();
        }
        if self.tag != "en" || !self.english.contains_key(key) {
            self.report(key);
        }
        self.english
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// The text for a key with `{name}` placeholders filled in.
    pub fn format(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut text = self.get(key);
        for (name, value) in args {
            text = text.replace(&format!("{{{name}}}"), value);
        }
        text
    }

    /// `{base}.one` for a count of one, else `{base}.other`, with `{count}`
    /// filled in. A language may add `{base}.zero`.
    pub fn plural(&self, base: &str, count: u64) -> String {
        let zero = format!("{base}.zero");
        let form = if self.is_one(count) {
            "one"
        } else if count == 0 && (self.overrides.contains_key(&zero) || self.english.contains_key(&zero)) {
            "zero"
        } else {
            "other"
        };
        self.format(&format!("{base}.{form}"), &[("count", &count.to_string())])
    }

    fn is_one(&self, count: u64) -> bool {
        match self.tag() {
            "fr" | "pt-" => count == 0 || count == 1,
            _ => count == 1,
        }
    }

    /// A length in the project's unit. `value_mm` is millimetres; `unit` is
    /// the project setting `mm`, `mil` or `inch`. Anything else reads as mm.
    pub fn format_length(&self, value_mm: f64, unit: &str) -> String {
        let (value, key, places) = match unit {
            "mil" => (value_mm / 0.0254, "unit.mil", 1),
            "inch" => (value_mm / 25.4, "unit.inch", 3),
            _ => (value_mm, "unit.mm", 2),
        };
        format!("{value:.places$} {}", self.get(key))
    }

    /// Keys asked for that the active language did not define, once each,
    /// in the order they were first asked.
    pub fn missing_keys(&self) -> Vec<String> {
        self.reported.lock().map(|r| r.clone()).unwrap_or_default()
    }

    fn report(&self, key: &str) {
        if let Ok(mut list) = self.reported.lock()
            && !list.iter().any(|k| k == key)
        {
            list.push(key.to_string());
        }
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Arc<Catalog>>> = const { RefCell::new(None) };
}

/// Make `catalog` the one [`t!`] reads on this thread. The window calls this
/// at the start of every frame, so a language change shows on the next one.
pub fn activate(catalog: &Arc<Catalog>) {
    ACTIVE.with(|slot| *slot.borrow_mut() = Some(Arc::clone(catalog)));
}

/// Look up `key` in the active catalog (English when none is active).
pub fn lookup(key: &str, args: &[(&str, &str)]) -> String {
    ACTIVE.with(|slot| match slot.borrow().as_ref() {
        Some(catalog) => catalog.format(key, args),
        None => Catalog::english().format(key, args),
    })
}

/// The text for a key: `t!("menu.file")`, or with placeholders
/// `t!("menu.edit.clear_net", name = net_name)`.
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::lookup($key, &[])
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::lookup($key, &[$((stringify!($name), &*($value).to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../strings/fixture-fr.txt");

    #[test]
    fn loading_a_fixture_language_swaps_catalog_strings() {
        let english = Catalog::english();
        let french = Catalog::english().with_language("fr", FIXTURE);
        assert_eq!(english.get("menu.settings"), "Settings");
        assert_eq!(french.get("menu.settings"), "Paramètres");
        assert_eq!(french.get("menu.file"), "Fichier");
        for (key, _) in parse(FIXTURE).0 {
            assert!(
                english.keys().any(|k| k == key),
                "fixture key {key} exists in English"
            );
            assert_ne!(
                french.get(&key),
                english.get(&key),
                "the fixture really translates {key}"
            );
        }
    }

    #[test]
    fn missing_key_falls_back_to_english_and_is_reported_once() {
        let french = Catalog::english().with_language("fr", FIXTURE);
        assert_eq!(french.get("menu.file.new"), "New");
        assert_eq!(french.get("menu.file.new"), "New");
        assert_eq!(french.get("no.such.key"), "no.such.key");
        assert_eq!(
            french.missing_keys(),
            vec!["menu.file.new".to_string(), "no.such.key".to_string()]
        );
        assert!(Catalog::english().missing_keys().is_empty());
    }

    #[test]
    fn plural_picks_the_right_template_for_count() {
        let english = Catalog::english();
        assert_eq!(english.plural("cmp.count", 1), "1 part");
        assert_eq!(english.plural("cmp.count", 0), "0 parts");
        assert_eq!(english.plural("cmp.count", 7), "7 parts");
        let french = Catalog::english().with_language("fr", FIXTURE);
        assert_eq!(french.plural("cmp.count", 2), "2 pièces");
    }

    #[test]
    fn format_length_honours_project_unit() {
        let c = Catalog::english();
        assert_eq!(c.format_length(2.54, "mm"), "2.54 mm");
        assert_eq!(c.format_length(2.54, "mil"), "100.0 mil");
        assert_eq!(c.format_length(25.4, "inch"), "1.000 in");
        assert_eq!(c.format_length(1.0, "furlong"), "1.00 mm");
    }

    #[test]
    fn the_english_file_is_well_formed() {
        let (entries, errors) = parse(ENGLISH);
        assert!(errors.is_empty(), "{errors:?}");
        let mut seen = BTreeSet::new();
        for (key, _) in &entries {
            assert!(seen.insert(key.clone()), "duplicate key {key}");
        }
        for (key, value) in parse(FIXTURE).0 {
            let base = entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v);
            assert_eq!(
                base.map(|v| placeholders(v)),
                Some(placeholders(&value)),
                "placeholders of {key} match English"
            );
        }
    }

    #[test]
    fn the_scanner_finds_literals_and_keys() {
        let source =
            "fn demo(ui: u8) {\n ui.button(\n \"Hi\")\n .on_hover_text(t!(\"app.home\")); }";
        let body = scan::fn_body(source, "demo");
        assert!(body.is_some_and(|b| scan::literal_violations(b).len() == 1));
        assert_eq!(scan::referenced_keys(source), vec!["app.home".to_string()]);
    }

    #[test]
    fn a_bad_tag_or_the_system_tag_gives_a_working_catalog() {
        assert_eq!(Catalog::for_language("zz").tag(), "en");
        assert_eq!(match_locale("de_DE.UTF-8"), "en");
        assert_eq!(Catalog::for_language("").get("menu.file"), "File");
    }

    #[test]
    fn the_macro_fills_placeholders_from_the_active_catalog() {
        activate(&Arc::new(Catalog::english()));
        assert_eq!(t!("menu.file"), "File");
        assert_eq!(
            t!("menu.edit.clear_net", name = "GND"),
            "Clear wiring on GND"
        );
    }
}

/// Source scanning shared by `cargo xtask strings check` and the ui-egui
/// guard test.
pub mod scan {
    /// Calls whose first argument is text a person reads.
    const TEXT_CALLS: [&str; 7] = [
        "button(",
        "on_hover_text(",
        "on_disabled_hover_text(",
        "RichText::new(",
        "Label::new(",
        "label(",
        "heading(",
    ];

    /// The `file function` pairs listed in `strings/converted-fns.txt`.
    pub fn converted_fns() -> Vec<(String, String)> {
        super::CONVERTED_FNS
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| {
                let mut words = l.split_whitespace();
                Some((words.next()?.to_string(), words.next()?.to_string()))
            })
            .collect()
    }

    /// The text of `fn name(...) { ... }` in `source`, braces matched while
    /// skipping string literals.
    pub fn fn_body<'a>(source: &'a str, name: &str) -> Option<&'a str> {
        let needle = format!("fn {name}(");
        let start = source.find(&needle)?;
        let rest = &source[start..];
        let bytes = rest.as_bytes();
        let mut depth = 0usize;
        let mut opened = false;
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'"' => {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'"' {
                        if bytes[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                }
                b'{' => {
                    depth += 1;
                    opened = true;
                }
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if opened && depth == 0 {
                        return Some(&rest[..=i]);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Each place in `body` where a text call is given a string literal.
    pub fn literal_violations(body: &str) -> Vec<String> {
        let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
        let mut found = Vec::new();
        for call in TEXT_CALLS {
            let pattern = format!("{call}\"");
            let mut from = 0;
            while let Some(at) = compact[from..].find(&pattern) {
                let begin = from + at;
                let end = (begin + pattern.len() + 24).min(compact.len());
                let end = (end..=compact.len())
                    .find(|e| compact.is_char_boundary(*e))
                    .unwrap_or(compact.len());
                found.push(compact[begin..end].to_string());
                from = begin + pattern.len();
            }
        }
        found
    }

    /// Every key named in a `t!("key"...)` call in `source`.
    pub fn referenced_keys(source: &str) -> Vec<String> {
        let mut keys = Vec::new();
        let mut rest = source;
        while let Some(at) = rest.find("t!(\"") {
            let after = &rest[at + 4..];
            let preceded_by_ident = rest[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
            if let Some(end) = after.find('"')
                && !preceded_by_ident
            {
                keys.push(after[..end].to_string());
            }
            rest = after;
        }
        keys
    }

    /// Problems in the ui-egui sources, one line each: unknown keys, and
    /// string literals inside converted functions. `files` is
    /// `(file name, source)`.
    pub fn problems(files: &[(String, String)]) -> Vec<String> {
        let english: std::collections::BTreeSet<String> = super::parse(super::ENGLISH)
            .0
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        let mut out = Vec::new();
        for (file, source) in files {
            for key in referenced_keys(source) {
                if !english.contains(&key) {
                    out.push(format!("{file}: key `{key}` is not in en.txt"));
                }
            }
        }
        for (file, function) in converted_fns() {
            let Some((_, source)) = files.iter().find(|(name, _)| *name == file) else {
                out.push(format!("converted-fns.txt names missing file {file}"));
                continue;
            };
            let Some(body) = fn_body(source, &function) else {
                out.push(format!(
                    "converted-fns.txt names missing fn {file} {function}"
                ));
                continue;
            };
            for hit in literal_violations(body) {
                out.push(format!(
                    "{file} fn {function}: literal text outside the catalog near `{hit}`"
                ));
            }
        }
        out
    }
}
