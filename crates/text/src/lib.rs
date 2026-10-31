//! One text catalog for every user-visible string.
//!
//! English is the base: `strings/en.txt` is embedded in the program and is
//! the authority on which keys exist. Another language is a plain
//! `key = value` file that overrides some or all of those keys; any key it
//! leaves out shows the English text and is reported once.
//!
//! Screens read text through [`t!`], which asks the catalog that the running
//! frame activated with [`activate`].

pub mod numbers;
pub mod pseudo;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

/// The English base catalog, embedded so the desktop and browser builds both
/// carry it.
pub const ENGLISH: &str = include_str!("../strings/en.txt");
/// Functions whose text already goes through the catalog; see the file.
pub const CONVERTED_FNS: &str = include_str!("../strings/converted-fns.txt");
/// The Spanish catalog, the first real translation, shipped in every build.
const SPANISH_ES: &str = include_str!("../strings/es.txt");
/// The German catalog, shipped in every build.
const GERMAN_DE: &str = include_str!("../strings/de.txt");
/// The French catalog, shipped in every build.
const FRENCH_FR: &str = include_str!("../strings/fr.txt");
#[cfg(feature = "fixture")]
const FIXTURE_FR: &str = include_str!("../strings/fixture-fr.txt");
#[cfg(feature = "fixture")]
const FIXTURE_RTL: &str = include_str!("../strings/fixture-rtl.txt");
#[cfg(feature = "fixture")]
const FIXTURE_DE: &str = include_str!("../strings/fixture-de.txt");

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
/// # Rules
/// - English comes first; every tag listed also resolves in `language_text`.
/// - The `name` is the language's own name, read from its `language.name`.
/// - A real translation owns its plain tag; fixtures use `qps-` pseudo tags
///   so they never collide with a shipped language.
/// # Dependencies
/// - `crates/text/strings/en.txt`
/// - `crates/text/strings/es.txt`
/// - `crates/text/strings/de.txt`
/// - `crates/text/strings/fr.txt`
/// # Do not
/// - Do not gate a real translation behind a feature; only the test
///   fixtures are optional.
pub fn shipped_languages() -> Vec<Language> {
    let mut list = vec![
        Language {
            tag: "en".into(),
            name: language_name(ENGLISH),
        },
        Language {
            tag: "es".into(),
            name: language_name(SPANISH_ES),
        },
        Language {
            tag: "de".into(),
            name: language_name(GERMAN_DE),
        },
        Language {
            tag: "fr".into(),
            name: language_name(FRENCH_FR),
        },
    ];
    list.extend(fixture_languages());
    list
}

#[cfg(feature = "fixture")]
fn fixture_languages() -> Vec<Language> {
    vec![
        Language {
            tag: FIXTURE_FR_TAG.into(),
            name: language_name(FIXTURE_FR),
        },
        Language {
            tag: FIXTURE_RTL_TAG.into(),
            name: language_name(FIXTURE_RTL),
        },
        Language {
            tag: FIXTURE_DE_TAG.into(),
            name: language_name(FIXTURE_DE),
        },
    ]
}

#[cfg(not(feature = "fixture"))]
fn fixture_languages() -> Vec<Language> {
    Vec::new()
}

/// Tag of the pseudo right-to-left fixture language.
#[cfg(feature = "fixture")]
pub const FIXTURE_RTL_TAG: &str = "qps-rtl";

/// Tag of the partial French fixture, kept clear of the real `fr` catalog.
#[cfg(feature = "fixture")]
pub const FIXTURE_FR_TAG: &str = "qps-fr";

/// Tag of the partial German fixture, kept clear of the real `de` catalog.
#[cfg(feature = "fixture")]
pub const FIXTURE_DE_TAG: &str = "qps-de";

/// The way a language's text runs.
///
/// # Rules
/// Read it with [`Catalog::direction`]; this crate only reports it and
/// mirrors nothing.
///
/// # Dependencies
/// `strings/en.txt` defines `language.direction`.
///
/// # Do not
/// Add a mixed or vertical value here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Left to right (the default).
    Ltr,
    /// Right to left.
    Rtl,
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
        "es" => Some(SPANISH_ES),
        "de" => Some(GERMAN_DE),
        "fr" => Some(FRENCH_FR),
        #[cfg(feature = "fixture")]
        FIXTURE_FR_TAG => Some(FIXTURE_FR),
        #[cfg(feature = "fixture")]
        FIXTURE_RTL_TAG => Some(FIXTURE_RTL),
        #[cfg(feature = "fixture")]
        FIXTURE_DE_TAG => Some(FIXTURE_DE),
        _ => None,
    }
}

/// A CLDR plural category. A language uses only some of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Used by Arabic for 0.
    Zero,
    /// The singular-like form.
    One,
    /// Used by Arabic and Hebrew for 2.
    Two,
    /// Small counts in Russian, Polish and Arabic.
    Few,
    /// Large counts in Russian, Polish and Arabic.
    Many,
    /// Everything else; mandatory in every language file.
    Other,
}

impl Category {
    /// The key suffix: `one` for `cmp.count.one`.
    pub fn suffix(self) -> &'static str {
        match self {
            Category::Zero => "zero",
            Category::One => "one",
            Category::Two => "two",
            Category::Few => "few",
            Category::Many => "many",
            Category::Other => "other",
        }
    }

    const ALL: [Category; 6] = [
        Category::Zero,
        Category::One,
        Category::Two,
        Category::Few,
        Category::Many,
        Category::Other,
    ];
}

fn primary_tag(tag: &str) -> &str {
    tag.split(['-', '_']).next().unwrap_or(tag)
}

/// The plural category of a whole-number `count` in the language `tag`.
///
/// Rules are the Unicode CLDR cardinal rules (`plurals.xml`), read for whole
/// numbers only, so `v = 0` and `i = n`. Research:
/// `research-602-plural-forms.md`. A tag with no rule here follows the
/// English rule.
///
/// # Rules
/// French and Portuguese use `many` only for exact millions and larger; this
/// app has no such counts, so they map to `other`.
///
/// # Dependencies
/// None.
///
/// # Do not
/// Add fractional counts or an ICU dependency for this short list.
pub fn plural_category(tag: &str, count: u64) -> Category {
    let n = count;
    let last_two = n % 100;
    let few_digit = (2..=4).contains(&(n % 10)) && !(12..=14).contains(&last_two);
    match primary_tag(tag) {
        // other only.
        "ja" | "zh" => Category::Other,
        // one: i = 0,1.
        "fr" | "pt" => {
            if n <= 1 {
                Category::One
            } else {
                Category::Other
            }
        }
        // one: n % 10 = 1 and n % 100 != 11; few: n % 10 = 2..4 and
        // n % 100 != 12..14; many: the rest of whole numbers.
        "ru" => {
            if n % 10 == 1 && last_two != 11 {
                Category::One
            } else if few_digit {
                Category::Few
            } else {
                Category::Many
            }
        }
        // one: n = 1; few: n % 10 = 2..4 and n % 100 != 12..14; many: rest.
        "pl" => {
            if n == 1 {
                Category::One
            } else if few_digit {
                Category::Few
            } else {
                Category::Many
            }
        }
        // zero: 0; one: 1; two: 2; few: n % 100 = 3..10; many: n % 100 =
        // 11..99; other: 100, 101, 102 and so on.
        "ar" => match n {
            0 => Category::Zero,
            1 => Category::One,
            2 => Category::Two,
            _ => match last_two {
                3..=10 => Category::Few,
                11..=99 => Category::Many,
                _ => Category::Other,
            },
        },
        // one: n = 1; two: n = 2.
        "he" => match n {
            1 => Category::One,
            2 => Category::Two,
            _ => Category::Other,
        },
        // en, de, es and any language without a rule here: one: n = 1.
        _ => {
            if n == 1 {
                Category::One
            } else {
                Category::Other
            }
        }
    }
}

/// The categories a language uses, so a file can be checked against them.
/// Derived from [`plural_category`] over a spread of counts that reaches
/// every branch. `other` is always included: CLDR keeps it for fractions.
pub fn plural_categories(tag: &str) -> Vec<Category> {
    let samples = [0u64, 1, 2, 3, 5, 11, 21, 100, 101, 111];
    Category::ALL
        .into_iter()
        .filter(|c| *c == Category::Other || samples.iter().any(|n| plural_category(tag, *n) == *c))
        .collect()
}

/// Problems with the plural keys of a language file, one line each: a base
/// without `.other`, or a category the language does not use. Keys are the
/// parsed `(key, value)` pairs.
///
/// # Rules
/// A translator adds exactly the categories the language needs.
///
/// # Dependencies
/// [`plural_categories`].
///
/// # Do not
/// Demand `.few` from every language.
pub fn plural_problems(tag: &str, entries: &[(String, String)]) -> Vec<String> {
    let used = plural_categories(tag);
    let mut bases: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (key, _) in entries {
        if let Some((base, suffix)) = key.rsplit_once('.')
            && Category::ALL.iter().any(|c| c.suffix() == suffix)
        {
            bases.entry(base).or_default().push(suffix);
        }
    }
    let mut out = Vec::new();
    for (base, suffixes) in bases {
        if !suffixes.contains(&"other") {
            out.push(format!("{base}: no `.other` form"));
        }
        for suffix in suffixes {
            if !used.iter().any(|c| c.suffix() == suffix) {
                out.push(format!("{base}.{suffix}: `{tag}` does not use `{suffix}`"));
            }
        }
    }
    out
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
            .filter(|(key, _)| self.english.contains_key(key) || self.english_plural_form(key))
            .collect();
        self
    }

    /// True when `key` is `{base}.{category}` and English defines
    /// `{base}.other`, so a language may add forms English lacks.
    fn english_plural_form(&self, key: &str) -> bool {
        key.rsplit_once('.').is_some_and(|(base, suffix)| {
            Category::ALL.iter().any(|c| c.suffix() == suffix)
                && self.english.contains_key(&format!("{base}.other"))
        })
    }

    /// The tag of the loaded language.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// The way the loaded language runs. The `language.direction` key
    /// decides (`ltr` or `rtl`). Without one, Arabic, Hebrew, Persian and
    /// Urdu tags are right to left and the rest left to right. A value that
    /// is neither gives left to right and is reported once as
    /// `language.direction`.
    ///
    /// # Rules
    /// Reports only; no screen is mirrored from here.
    ///
    /// # Dependencies
    /// `strings/en.txt` for the key.
    ///
    /// # Do not
    /// Call it per widget; read it once per frame.
    pub fn direction(&self) -> Direction {
        match self.overrides.get("language.direction").map(|v| v.trim()) {
            Some("ltr") => Direction::Ltr,
            Some("rtl") => Direction::Rtl,
            Some(_) => {
                self.report("language.direction");
                Direction::Ltr
            }
            None => {
                let primary = self.tag.split(['-', '_']).next().unwrap_or("");
                if matches!(primary, "ar" | "he" | "fa" | "ur") {
                    Direction::Rtl
                } else {
                    Direction::Ltr
                }
            }
        }
    }

    /// The loaded language tag, for the number and date functions in
    /// [`numbers`]. `en` when the tag was unknown.
    pub fn number_tag(&self) -> &str {
        &self.tag
    }

    /// Every key English defines, in sorted order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.english.keys().map(String::as_str)
    }

    /// True when English defines `key`.
    pub fn defines(&self, key: &str) -> bool {
        self.english.contains_key(key)
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

    /// `{base}.one` when the language treats the count as one, else
    /// `{base}.other`, with `{count}` filled in. A language may add
    /// `{base}.zero`.
    ///
    /// # Rules
    /// French and Portuguese count 0 as one (see `is_one`).
    ///
    /// # Dependencies
    /// `strings/en.txt` for the `.one` and `.other` keys.
    ///
    /// # Do not
    /// Build a plural with an inline `s` in screen code.
    pub fn plural(&self, base: &str, count: u64) -> String {
        let zero = format!("{base}.zero");
        let form = if count == 0
            && (self.overrides.contains_key(&zero) || self.english.contains_key(&zero))
        {
            "zero"
        } else {
            plural_category(&self.tag, count).suffix()
        };
        let wanted = format!("{base}.{form}");
        let key = if self.overrides.contains_key(&wanted) || self.english.contains_key(&wanted) {
            wanted
        } else {
            format!("{base}.other")
        };
        self.format(&key, &[("count", &count.to_string())])
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

/// The language tag the number and date functions use on this thread: the
/// active catalog's, or `en` when none is active.
///
/// # Rules
/// Read it where a widget formats or parses a number.
/// # Dependencies
/// [`Catalog::number_tag`].
/// # Do not
/// Call it from the io or mcp crates; machine output never localizes.
pub fn active_number_tag() -> String {
    ACTIVE.with(|slot| match slot.borrow().as_ref() {
        Some(catalog) => catalog.number_tag().to_string(),
        None => "en".to_string(),
    })
}

/// Look up `key` in the active catalog (English when none is active).
pub fn lookup(key: &str, args: &[(&str, &str)]) -> String {
    ACTIVE.with(|slot| match slot.borrow().as_ref() {
        Some(catalog) => catalog.format(key, args),
        None => Catalog::english().format(key, args),
    })
}

/// Like [`lookup`], but `None` for a key English does not define, and
/// without reporting it. For keys that are optional by design.
pub fn lookup_known(key: &str) -> Option<String> {
    ACTIVE.with(|slot| {
        let fallback;
        let catalog = match slot.borrow().as_ref() {
            Some(catalog) => Arc::clone(catalog),
            None => {
                fallback = Arc::new(Catalog::english());
                fallback
            }
        };
        catalog.defines(key).then(|| catalog.get(key))
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
    const FIXTURE_RTL: &str = include_str!("../strings/fixture-rtl.txt");

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
    fn loading_spanish_swaps_menu_labels_and_keeps_placeholders() {
        let english = Catalog::english();
        let spanish = Catalog::english().with_language("es", SPANISH_ES);
        assert_eq!(english.get("menu.file"), "File");
        assert_eq!(spanish.get("menu.file"), "Archivo");
        assert_eq!(spanish.get("menu.help"), "Ayuda");
        assert_eq!(
            spanish.format("menu.edit.clear_net", &[("name", "GND")]),
            "Quitar el ruteo de GND",
            "the name placeholder still fills"
        );
        assert_eq!(spanish.plural("cmp.count", 1), "1 componente");
        assert_eq!(spanish.plural("cmp.count", 7), "7 componentes");
        assert_eq!(match_locale("es_ES.UTF-8"), "es");
        for (key, value) in parse(SPANISH_ES).0 {
            let base = english.get(&key);
            assert_eq!(
                placeholders(&base),
                placeholders(&value),
                "es.txt keeps the placeholders of {key}"
            );
        }
        assert!(
            spanish.missing_keys().is_empty(),
            "es.txt defines every key, so nothing falls back"
        );
    }

    #[test]
    fn loading_german_swaps_menu_labels_and_keeps_placeholders() {
        let english = Catalog::english();
        let german = Catalog::english().with_language("de", GERMAN_DE);
        assert_eq!(english.get("menu.file"), "File");
        assert_eq!(german.get("menu.file"), "Datei");
        assert_eq!(german.get("menu.help"), "Hilfe");
        assert_eq!(
            german.format("menu.edit.clear_net", &[("name", "GND")]),
            "Verdrahtung auf GND löschen",
            "the name placeholder still fills"
        );
        assert_eq!(german.plural("cmp.count", 1), "1 Bauteil");
        assert_eq!(german.plural("cmp.count", 7), "7 Bauteile");
        assert_eq!(match_locale("de_DE.UTF-8"), "de");
        for (key, value) in parse(GERMAN_DE).0 {
            let base = english.get(&key);
            assert_eq!(
                placeholders(&base),
                placeholders(&value),
                "de.txt keeps the placeholders of {key}"
            );
        }
        assert!(
            german.missing_keys().is_empty(),
            "de.txt defines every key, so nothing falls back"
        );
    }

    #[test]
    fn loading_french_swaps_menu_labels_and_keeps_placeholders() {
        let english = Catalog::english();
        let french = Catalog::english().with_language("fr", FRENCH_FR);
        assert_eq!(english.get("menu.file"), "File");
        assert_eq!(french.get("menu.file"), "Fichier");
        assert_eq!(french.get("menu.help"), "Aide");
        assert_eq!(
            french.format("menu.edit.clear_net", &[("name", "GND")]),
            "Effacer le câblage sur GND",
            "the name placeholder still fills"
        );
        assert_eq!(french.plural("cmp.count", 1), "1 composant");
        assert_eq!(french.plural("cmp.count", 7), "7 composants");
        assert_eq!(match_locale("fr_FR.UTF-8"), "fr");
        for (key, value) in parse(FRENCH_FR).0 {
            let base = english.get(&key);
            assert_eq!(
                placeholders(&base),
                placeholders(&value),
                "fr.txt keeps the placeholders of {key}"
            );
        }
        assert!(
            french.missing_keys().is_empty(),
            "fr.txt defines every key, so nothing falls back"
        );
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
    fn french_uses_the_one_form_for_zero_and_one() {
        let french = Catalog::english().with_language("fr", FIXTURE);
        let forms: Vec<String> = [0, 1, 2, 5, 11, 21, 101]
            .iter()
            .map(|n| french.plural("cmp.count", *n))
            .collect();
        assert_eq!(
            forms,
            [
                "0 pièce",
                "1 pièce",
                "2 pièces",
                "5 pièces",
                "11 pièces",
                "21 pièces",
                "101 pièces"
            ]
        );
    }

    #[test]
    fn english_is_left_to_right() {
        assert_eq!(Catalog::english().direction(), Direction::Ltr);
        assert_eq!(Catalog::english().get("language.direction"), "ltr");
        let french = Catalog::english().with_language("fr", FIXTURE);
        assert_eq!(french.direction(), Direction::Ltr);
    }

    #[test]
    fn the_pseudo_rtl_fixture_reports_right_to_left() {
        let rtl = Catalog::english().with_language("qps-rtl", FIXTURE_RTL);
        assert_eq!(rtl.direction(), Direction::Rtl);
        assert_ne!(rtl.get("menu.file"), Catalog::english().get("menu.file"));
        #[cfg(feature = "fixture")]
        {
            assert!(
                shipped_languages()
                    .iter()
                    .any(|l| l.tag == FIXTURE_RTL_TAG && l.name == "Pseudo RTL")
            );
            assert_eq!(
                Catalog::for_language(FIXTURE_RTL_TAG).direction(),
                Direction::Rtl
            );
        }
        // Shipped tags with no key still run right to left.
        for tag in ["ar", "he", "fa", "ur", "ar-EG"] {
            assert_eq!(
                Catalog::english().with_language(tag, "").direction(),
                Direction::Rtl,
                "{tag}"
            );
        }
        assert_eq!(
            Catalog::english().with_language("de", "").direction(),
            Direction::Ltr
        );
    }

    #[test]
    fn portuguese_uses_the_one_form_for_zero_and_one() {
        let text = "cmp.count.one = {count} peça\ncmp.count.other = {count} peças\n";
        for tag in ["pt", "pt-BR"] {
            let pt = Catalog::english().with_language(tag, text);
            assert_eq!(pt.plural("cmp.count", 0), "0 peça");
            assert_eq!(pt.plural("cmp.count", 1), "1 peça");
            assert_eq!(pt.plural("cmp.count", 2), "2 peças");
            assert_eq!(pt.plural("cmp.count", 5), "5 peças");
        }
    }

    #[test]
    fn an_invalid_direction_value_falls_back_to_left_to_right_and_is_reported() {
        let bad = Catalog::english().with_language("xx", "language.direction = sideways\n");
        assert_eq!(bad.direction(), Direction::Ltr);
        assert_eq!(bad.direction(), Direction::Ltr);
        assert_eq!(bad.missing_keys(), vec!["language.direction".to_string()]);
        let good = Catalog::english().with_language("xx", "language.direction = rtl\n");
        assert_eq!(good.direction(), Direction::Rtl);
        assert!(good.missing_keys().is_empty());
    }

    #[test]
    fn the_english_file_still_passes_the_well_formed_test_with_the_new_key() {
        let (entries, errors) = parse(ENGLISH);
        assert!(errors.is_empty(), "{errors:?}");
        let count = entries
            .iter()
            .filter(|(k, _)| k == "language.direction")
            .count();
        assert_eq!(count, 1);
        let (rtl, rtl_errors) = parse(FIXTURE_RTL);
        assert!(rtl_errors.is_empty(), "{rtl_errors:?}");
        for (key, value) in rtl {
            let base = entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v);
            assert_eq!(
                base.map(|v| placeholders(v)),
                Some(placeholders(&value)),
                "placeholders of {key} match English"
            );
        }
    }

    #[test]
    fn english_and_spanish_use_the_one_form_only_for_one() {
        let text = "cmp.count.one = {count} pieza\ncmp.count.other = {count} piezas\n";
        let es = Catalog::english().with_language("es", text);
        let en = Catalog::english();
        assert_eq!(es.plural("cmp.count", 0), "0 piezas");
        assert_eq!(es.plural("cmp.count", 1), "1 pieza");
        assert_eq!(es.plural("cmp.count", 2), "2 piezas");
        assert_eq!(es.plural("cmp.count", 5), "5 piezas");
        assert_eq!(en.plural("cmp.count", 0), "0 parts");
        assert_eq!(en.plural("cmp.count", 1), "1 part");
        assert_eq!(en.plural("cmp.count", 2), "2 parts");
        assert_eq!(en.plural("cmp.count", 5), "5 parts");
    }

    /// Counts 0, 1, 2, 5, 11, 21, 101 in order, as category suffixes.
    fn boundary(tag: &str) -> Vec<&'static str> {
        [0, 1, 2, 5, 11, 21, 101]
            .iter()
            .map(|n| plural_category(tag, *n).suffix())
            .collect()
    }

    #[test]
    fn each_shipped_language_maps_the_boundary_counts_to_the_expected_category() {
        let one_other = ["other", "one", "other", "other", "other", "other", "other"];
        assert_eq!(boundary("en"), one_other);
        assert_eq!(boundary("de"), one_other);
        assert_eq!(boundary("es"), one_other);
        let zero_is_one = ["one", "one", "other", "other", "other", "other", "other"];
        assert_eq!(boundary("fr"), zero_is_one);
        assert_eq!(boundary("pt"), zero_is_one);
        assert_eq!(boundary("pt-BR"), zero_is_one);
        assert_eq!(boundary("ja"), ["other"; 7]);
        assert_eq!(boundary("zh"), ["other"; 7]);
        assert_eq!(
            boundary("ru"),
            ["many", "one", "few", "many", "many", "one", "one"]
        );
        assert_eq!(
            boundary("pl"),
            ["many", "one", "few", "many", "many", "many", "many"]
        );
        assert_eq!(
            boundary("ar"),
            ["zero", "one", "two", "few", "many", "many", "other"]
        );
        assert_eq!(
            boundary("he"),
            ["other", "one", "two", "other", "other", "other", "other"]
        );
    }

    #[test]
    fn a_russian_file_may_define_few_and_many_and_they_are_kept() {
        let text = "cmp.count.one = {count} деталь\ncmp.count.few = {count} детали\n\
                    cmp.count.many = {count} деталей\ncmp.count.other = {count} детали\n";
        let ru = Catalog::english().with_language("ru", text);
        assert_eq!(ru.plural("cmp.count", 1), "1 деталь");
        assert_eq!(ru.plural("cmp.count", 3), "3 детали");
        assert_eq!(ru.plural("cmp.count", 5), "5 деталей");
        assert_eq!(ru.plural("cmp.count", 21), "21 деталь");
        assert!(ru.missing_keys().is_empty());
        assert!(plural_problems("ru", &parse(text).0).is_empty());
    }

    #[test]
    fn a_language_file_must_define_other_for_every_plural_base() {
        let entries = parse("cmp.count.one = {count} peça\n").0;
        assert_eq!(
            plural_problems("pt", &entries),
            vec!["cmp.count: no `.other` form".to_string()]
        );
        assert!(plural_problems("fr", &parse(FIXTURE).0).is_empty());
    }

    #[test]
    fn a_language_file_may_not_define_a_category_its_language_does_not_use() {
        let entries = parse("cmp.count.one = {count}\ncmp.count.other = {count}\n").0;
        assert_eq!(plural_problems("ja", &entries).len(), 1);
        assert!(plural_problems("ja", &entries)[0].contains("cmp.count.one"));
        assert!(plural_problems("en", &entries).is_empty());
    }

    #[test]
    fn a_missing_category_falls_back_to_other() {
        let text = "cmp.count.other = {count} Teile\n";
        let de = Catalog::english().with_language("de", text);
        assert_eq!(de.plural("cmp.count", 1), "1 part");
        let ru = Catalog::english().with_language("ru", text);
        assert_eq!(ru.plural("cmp.count", 5), "5 Teile");
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
        assert_eq!(match_locale("xx_XX.UTF-8"), "en");
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

    /// Places in `source` that build a plural by hand: `== 1 {` followed by
    /// `""` or `"s"`, or the text `(s)` inside a string. Each hit is a short
    /// excerpt. Use `Catalog::plural` instead.
    pub fn plural_s_violations(source: &str) -> Vec<String> {
        let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
        let mut found = Vec::new();
        for pattern in ["==1{\"\"", "==1{\"s\""] {
            let mut from = 0;
            while let Some(at) = compact[from..].find(pattern) {
                found.push(format!("`{pattern}`"));
                from += at + pattern.len();
            }
        }
        let (mut in_string, mut escaped) = (false, false);
        let chars: Vec<char> = source.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                } else if c == '('
                    && chars.get(i + 1) == Some(&'s')
                    && chars.get(i + 2) == Some(&')')
                {
                    found.push("`(s)` in a string".to_string());
                }
            } else if c == '\'' && chars.get(i + 2) == Some(&'\'') {
                i += 2;
            } else if c == '"' {
                in_string = true;
            }
            i += 1;
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

#[cfg(test)]
mod area_tests {
    use super::*;

    /// Every `t!` key used inside `fns` (`file function` pairs under
    /// crates/ui-egui/src) exists in en.txt.
    fn keys_exist(fns: &[(&str, &str)]) -> Result<(), String> {
        let english: BTreeSet<String> = parse(ENGLISH).0.into_iter().map(|(k, _)| k).collect();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui-egui/src");
        for (file, function) in fns {
            let text = std::fs::read_to_string(src.join(file)).map_err(|e| e.to_string())?;
            let body = scan::fn_body(&text, function)
                .ok_or_else(|| format!("{file}: no fn {function}"))?;
            for key in scan::referenced_keys(body) {
                assert!(english.contains(&key), "{file} {function}: key {key}");
            }
        }
        Ok(())
    }

    /// Hand-built plurals that already exist, as `file count` lines in
    /// `strings/plural-known.txt`. The list may only shrink.
    #[test]
    fn the_ui_source_has_no_inline_plural_s() -> Result<(), String> {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui-egui/src");
        let known: BTreeMap<String, usize> = include_str!("../strings/plural-known.txt")
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| {
                let (file, n) = l.rsplit_once(' ')?;
                Some((file.to_string(), n.parse().ok()?))
            })
            .collect();
        let mut seen = BTreeMap::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                    let hits = scan::plural_s_violations(&text).len();
                    if hits > 0 {
                        let name = path.strip_prefix(&src).map_err(|e| e.to_string())?;
                        seen.insert(name.to_string_lossy().replace('\\', "/"), hits);
                    }
                }
            }
        }
        let over: Vec<String> = seen
            .iter()
            .filter(|(file, hits)| **hits > known.get(*file).copied().unwrap_or(0))
            .map(|(file, hits)| format!("{file} {hits}"))
            .collect();
        if over.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "hand-built plurals beyond plural-known.txt (use Catalog::plural): {over:?}"
            ))
        }
    }

    #[test]
    fn the_guard_fails_on_a_string_that_builds_a_plural_with_an_inline_s() {
        let bad = r#"format!("{} item{}", n, if n == 1 { "" } else { "s" })"#;
        assert_eq!(scan::plural_s_violations(bad).len(), 1);
        let bad = r#"let m = "Loaded {} file(s)";"#;
        assert_eq!(scan::plural_s_violations(bad).len(), 1);
        let fine = r#"let x = Some(s); if n == 1 { one() } else { other() }"#;
        assert!(scan::plural_s_violations(fine).is_empty());
    }

    #[test]
    fn every_key_used_by_board_layout_exists() -> Result<(), String> {
        keys_exist(&[
            ("layers_tree.rs", "layers_tab"),
            ("lib.rs", "stackup_section"),
            ("lib.rs", "holes_section"),
            ("lib.rs", "fiducials_section"),
            ("lib.rs", "footer"),
            ("lib.rs", "tool_strip"),
            ("lib.rs", "draw_tool_options"),
            ("lib.rs", "tool_hint"),
            ("lib.rs", "inspector"),
            ("canvas.rs", "canvas"),
            ("canvas.rs", "paint_board"),
            ("canvas.rs", "canvas_click"),
            ("board3d.rs", "show"),
        ])
    }

    #[test]
    fn every_key_used_by_schematic_exists() -> Result<(), String> {
        keys_exist(&[
            ("lib.rs", "nets_tab"),
            ("lib.rs", "object_detail_rows"),
            ("lib.rs", "type_note"),
            ("lib.rs", "empty_state"),
            ("canvas.rs", "paint_schematic"),
        ])
    }

    #[test]
    fn every_key_used_by_library_exists() -> Result<(), String> {
        keys_exist(&[
            ("library.rs", "request"),
            ("library.rs", "search"),
            ("library.rs", "poll"),
            ("library.rs", "collection_browser"),
            ("library.rs", "library_preview"),
            ("authoring.rs", "library_editor"),
            ("authoring.rs", "project_library"),
            ("authoring.rs", "library_workspaces"),
            ("authoring.rs", "graphic"),
            ("authoring.rs", "add_graphic"),
            ("lib.rs", "browser"),
            ("lib.rs", "library"),
            ("lib.rs", "files_tab"),
            ("library_quality.rs", "quality_badge"),
            ("library_quality.rs", "library_checks_section"),
            ("library_quality.rs", "refresh_footprint_quality"),
        ])
    }

    #[test]
    fn every_key_used_by_rules_exists() -> Result<(), String> {
        keys_exist(&[
            ("lib.rs", "rules_tab"),
            ("lib.rs", "param_help"),
            ("lib.rs", "field_help"),
        ])
    }

    #[test]
    fn every_key_used_by_checks_exists() -> Result<(), String> {
        keys_exist(&[
            ("lib.rs", "checks_view"),
            ("lib.rs", "human_code"),
            ("lib.rs", "custom_heading"),
            ("lib.rs", "custom_measure_line"),
            ("lib.rs", "empty_state"),
        ])
    }

    #[test]
    fn every_key_used_by_settings_exists() -> Result<(), String> {
        keys_exist(&[("lib.rs", "settings_tab"), ("lib.rs", "shortcuts")])
    }

    #[test]
    fn every_key_used_by_docks_exists() -> Result<(), String> {
        keys_exist(&[
            ("lib.rs", "dock_frame"),
            ("lib.rs", "workbench_tabs"),
            ("lib.rs", "builds_tab"),
            ("lib.rs", "shortcut_rows"),
            ("lib.rs", "command_tip"),
        ])
    }

    #[test]
    fn every_key_used_by_dialogs_exists() -> Result<(), String> {
        keys_exist(&[
            ("lib.rs", "start_home"),
            ("lib.rs", "execute_palette_params"),
            ("lib.rs", "params_template"),
            ("lib.rs", "run"),
            ("lib.rs", "run_and_keep"),
            ("lib.rs", "tool_call"),
            ("lib.rs", "refusal_text"),
            ("editing.rs", "shift_click"),
            ("editing.rs", "move_selection"),
            ("editing.rs", "delete_selection"),
            ("editing.rs", "duplicate_selection"),
            ("editing.rs", "copy_selection"),
            ("editing.rs", "paste_last_copy_at"),
            ("editing.rs", "paste_text_at"),
            ("editing.rs", "paste_last_copy"),
            ("editing.rs", "measure_click"),
            ("editing.rs", "cleanup_tracks"),
            ("lib.rs", "write_make_files"),
            ("lib.rs", "write_uv_print_files"),
            ("lib.rs", "write_job_file"),
            ("lib.rs", "capture"),
            ("lib.rs", "copy_plot_to_clipboard"),
        ])
    }
}
