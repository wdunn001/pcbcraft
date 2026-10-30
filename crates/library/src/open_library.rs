//! The first online CAD source: the official open symbol, footprint and 3D
//! model libraries, read over their anonymous repository endpoints (a folder
//! listing and a raw file download). It implements `CadSource` for import and
//! placement and `PartSource` so it fits the Library tab's Online slot.
//!
//! # Rules
//! - A [`Fetcher`] supplies the bytes, so tests replay recorded answers and the web build can bring its own.
//! - One request per folder page or file, only when the cache cannot answer; reads stay pinned to one repository revision.
//! - Offline, saved listings and files still answer, however old.
//! - The upstream project names live in [`OpenLibraryConfig::default`] only.
//! - A footprint search reads folder listings, never files: names are matched with case, `-`, `_`, `.` and spaces ignored,
//!   so `lqfp64` finds `LQFP-64_10x10mm_P0.5mm`. Each folder listing is saved and reused until it is a day old.
//! - A footprint search that finds nothing in the folders its words point at reads at most [`MAX_EXTRA_FOLDERS`] more folders.
//!
//! # Dependencies
//! - crates/library/src/cad.rs
//! - crates/library/src/cad_cache.rs
//! - crates/library/src/sources.rs
use crate::cad::{CadFile, CadKind, CadRef, CadSource, LicenseInfo, MAX_FILE_BYTES, Provenance};
use crate::cad_cache::CadCache;
use crate::sources::{
    Attribution, MAX_RESULTS, PartResult, PartSource, SetupCard, SourceError, check_query,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Id used in cache keys and notices.
pub const SOURCE_ID: &str = "open-library";
/// Name shown on cards.
pub const SOURCE_NAME: &str = "Open library";
/// Most libraries one search lists.
pub const MAX_LIBRARIES_PER_SEARCH: usize = 8;
/// Folder listings are asked again after this many seconds (24 hours).
pub const LISTING_TTL_SECS: u64 = 24 * 60 * 60;
/// Folders a footprint search may read beyond the ones its words point at.
pub const MAX_EXTRA_FOLDERS: usize = 24;
/// Start of a footprint card's `sku`; the rest is `Library/Name`.
pub const FOOTPRINT_SKU_PREFIX: &str = "footprint:";
const PAGE_SIZE: usize = 100;
const MAX_PAGES: u32 = 50;
const LICENSE_NAME: &str = "CC-BY-SA 4.0";
const LICENSE_URL: &str = "https://creativecommons.org/licenses/by-sa/4.0/";

/// Why a transport could not answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// No connection, or the person switched online sources off.
    Offline(String),
    /// The host has no such file or folder.
    NotFound,
    /// The host asked for a pause.
    RateLimited {
        /// Seconds the host asked us to wait.
        retry_after_secs: u64,
    },
    /// Anything else, in plain words.
    Failed(String),
}

/// Gets the bytes behind a URL. Desktop and web builds each bring one.
///
/// # Rules
/// - Plain GET, no credentials, no cookies.
/// - Refuse an answer over [`MAX_FILE_BYTES`].
/// - Map HTTP 404 to [`FetchError::NotFound`] and 429 to [`FetchError::RateLimited`].
///
/// # Do not
/// - Retry in a loop; the source decides what to do with a failure.
///
/// # Implementations
/// - `apps/pcbcraft/src/online_http.rs`
/// - `crates/ui-egui/src/e2e/library_workspace.rs`
/// - `crates/ui-egui/src/e2e/support.rs`
/// - `crates/ui-egui/src/sources.rs`
pub trait Fetcher: Send + Sync {
    /// Read the whole answer for a URL.
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError>;
}

/// Where the three repositories live. The default is the official open
/// libraries; tests and mirrors can point elsewhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenLibraryConfig {
    /// Base of the repository API, without a trailing slash.
    pub api_base: String,
    /// Web base used for the page link on each card.
    pub web_base: String,
    /// Project path of the symbol repository.
    pub symbols_project: String,
    /// Project path of the footprint repository.
    pub footprints_project: String,
    /// Project path of the 3D model repository.
    pub models_project: String,
    /// Branch every request is first resolved against.
    pub branch: String,
}

impl Default for OpenLibraryConfig {
    fn default() -> Self {
        Self {
            api_base: "https://gitlab.com/api/v4/projects".into(),
            web_base: "https://gitlab.com".into(),
            symbols_project: "kicad/libraries/kicad-symbols".into(),
            footprints_project: "kicad/libraries/kicad-footprints".into(),
            models_project: "kicad/libraries/kicad-packages3D".into(),
            branch: "master".into(),
        }
    }
}

/// Libraries searched when the words name none.
fn default_libraries(kind: CadKind) -> &'static [&'static str] {
    match kind {
        CadKind::Symbol => &[
            "Device",
            "Connector_Generic",
            "Diode",
            "Transistor_BJT",
            "Regulator_Linear",
            "Amplifier_Operational",
        ],
        CadKind::Footprint | CadKind::Model => &[
            "Resistor_SMD",
            "Capacitor_SMD",
            "Inductor_SMD",
            "LED_SMD",
            "Diode_SMD",
            "Package_SO",
            "Package_TO_SOT_SMD",
            "Connector_PinHeader_2.54mm",
        ],
    }
}

/// Everyday words to the symbol they mean: word, library, symbol name.
const SYMBOL_ALIASES: &[(&str, &str, &str)] = &[
    ("resistor", "Device", "R"),
    ("capacitor", "Device", "C"),
    ("inductor", "Device", "L"),
    ("diode", "Device", "D"),
    ("led", "Device", "LED"),
    ("crystal", "Device", "Crystal"),
    ("fuse", "Device", "Fuse"),
];

/// Package family words to the footprint libraries that hold them.
const FOOTPRINT_FAMILIES: &[(&[&str], &[&str])] = &[
    (
        &["lqfp", "tqfp", "pqfp", "hqfp", "vqfp", "mqfp", "qfp"],
        &["Package_QFP"],
    ),
    (
        &[
            "qfn", "vqfn", "wqfn", "uqfn", "tqfn", "dfn", "wdfn", "udfn", "tdfn",
        ],
        &["Package_DFN_QFN"],
    ),
    (
        &[
            "soic", "sop", "ssop", "tssop", "msop", "tsop", "vssop", "so",
        ],
        &["Package_SO"],
    ),
    (
        &["sot", "to"],
        &["Package_TO_SOT_SMD", "Package_TO_SOT_THT"],
    ),
    (
        &["bga", "fbga", "lfbga", "tfbga", "ucsp", "csp", "wlcsp"],
        &["Package_BGA", "Package_CSP"],
    ),
    (&["dip", "pdip", "cdip"], &["Package_DIP"]),
    (&["sip"], &["Package_SIP"]),
    (&["lga"], &["Package_LGA"]),
    (&["plcc", "lcc"], &["Package_LCC"]),
    (&["sod", "sma", "smb", "smc"], &["Diode_SMD"]),
];

/// Which kind of card a search returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardScope {
    /// Symbols only.
    Symbols,
    /// Footprints only.
    Footprints,
    /// Symbols, then footprints.
    Both,
}

/// Letters and digits only, lower case: `LQFP-64_10x10` and `lqfp 64 10x10` read the same.
fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The leading letters of a squashed word: `lqfp` for `lqfp64`.
fn letters_of(squashed: &str) -> &str {
    let end = squashed
        .find(|c: char| !c.is_alphabetic())
        .unwrap_or(squashed.len());
    &squashed[..end]
}

struct Layout {
    suffix: &'static str,
    extensions: &'static [&'static str],
    folder: &'static str,
}

fn layout(kind: CadKind) -> Layout {
    match kind {
        CadKind::Symbol => Layout {
            suffix: ".kicad_symdir",
            extensions: &[".kicad_sym"],
            folder: "symbols",
        },
        CadKind::Footprint => Layout {
            suffix: ".pretty",
            extensions: &[".kicad_mod"],
            folder: "footprints",
        },
        CadKind::Model => Layout {
            suffix: ".3dshapes",
            extensions: &[".step", ".stp"],
            folder: "models",
        },
    }
}

/// What a search will look at: libraries to list, words every name must hold,
/// and (library, symbol) pairs that everyday words stand for.
struct SearchPlan {
    libraries: Vec<String>,
    words: Vec<String>,
    targets: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TreeItem {
    id: String,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    path: String,
}

/// A folder listing as stored in the cache: the items and the repository
/// revision they were read at.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Listing {
    commit: String,
    items: Vec<TreeItem>,
}

#[derive(Deserialize)]
struct CommitAnswer {
    id: String,
}

fn pct(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn from_fetch(err: FetchError, what: &str) -> SourceError {
    match err {
        FetchError::Offline(m) => SourceError::Unavailable(format!(
            "no connection to the open library, and {what} is not saved on this computer yet ({m})"
        )),
        FetchError::NotFound => SourceError::NotFound(what.to_owned()),
        FetchError::RateLimited { retry_after_secs } => {
            SourceError::RateLimited { retry_after_secs }
        }
        FetchError::Failed(m) => {
            SourceError::Unavailable(format!("the open library could not be read: {m}"))
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The official open libraries as a CAD source and a part source.
///
/// # Rules
/// - Contract: crates/library/src/cad.rs (`CadSource`) and crates/library/src/sources.rs (`PartSource`).
/// - Cards carry the source name and the licence line in their description.
///
/// # Dependencies
/// - crates/library/src/cad_cache.rs
///
/// # Do not
/// - Fetch a file the person has not asked for; a card costs one listing, not one file.
pub struct OpenLibrarySource {
    fetcher: Box<dyn Fetcher>,
    cache: CadCache,
    config: OpenLibraryConfig,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
    commits: Mutex<BTreeMap<CadKind, String>>,
}

impl OpenLibrarySource {
    /// A source over a transport and a cache, pointed at the official libraries.
    pub fn new(fetcher: Box<dyn Fetcher>, cache: CadCache) -> Self {
        Self::with_config(fetcher, cache, OpenLibraryConfig::default())
    }

    /// The same, with other repository locations (a mirror, or a test double).
    pub fn with_config(
        fetcher: Box<dyn Fetcher>,
        cache: CadCache,
        config: OpenLibraryConfig,
    ) -> Self {
        Self {
            fetcher,
            cache,
            config,
            clock: Box::new(now_secs),
            commits: Mutex::new(BTreeMap::new()),
        }
    }

    /// Replace the clock (seconds since the Unix epoch), for tests.
    pub fn with_clock(mut self, clock: Box<dyn Fn() -> u64 + Send + Sync>) -> Self {
        self.clock = clock;
        self
    }

    /// The cache, so a settings page can show its size or purge it.
    pub fn cache(&self) -> &CadCache {
        &self.cache
    }

    /// Forget the saved listings so the next lookup asks the host again.
    /// Saved files stay; they are keyed by their own revision.
    pub fn refresh(&self) -> Result<(), SourceError> {
        if let Ok(mut c) = self.commits.lock() {
            c.clear();
        }
        for kind in [CadKind::Symbol, CadKind::Footprint, CadKind::Model] {
            self.cache
                .forget_listing(SOURCE_ID, &format!("{}/commit.json", layout(kind).folder))?;
            for lib in self.cached_listing_keys(kind) {
                self.cache.forget_listing(SOURCE_ID, &lib)?;
            }
        }
        Ok(())
    }

    fn cached_listing_keys(&self, kind: CadKind) -> Vec<String> {
        let Some(root) = self.cache.root() else {
            return Vec::new();
        };
        let dir = root
            .join("listings")
            .join(SOURCE_ID)
            .join(layout(kind).folder);
        let mut keys = Vec::new();
        if let Ok(read) = std::fs::read_dir(dir) {
            for e in read.flatten() {
                if let Some(n) = e.file_name().to_str() {
                    keys.push(format!("{}/{n}", layout(kind).folder));
                }
            }
        }
        keys
    }

    fn project(&self, kind: CadKind) -> &str {
        match kind {
            CadKind::Symbol => &self.config.symbols_project,
            CadKind::Footprint => &self.config.footprints_project,
            CadKind::Model => &self.config.models_project,
        }
    }

    fn base(&self, kind: CadKind) -> String {
        format!("{}/{}", self.config.api_base, pct(self.project(kind)))
    }

    /// Revision of the repository for this kind: remembered for the session,
    /// else saved, else asked. A failed ask falls back to a saved answer of any age.
    fn commit(&self, kind: CadKind) -> Result<String, SourceError> {
        if let Ok(c) = self.commits.lock()
            && let Some(id) = c.get(&kind)
        {
            return Ok(id.clone());
        }
        let key = format!("{}/commit.json", layout(kind).folder);
        let now = (self.clock)();
        let saved = self.cache.get_listing(SOURCE_ID, &key)?;
        if let Some(s) = &saved
            && now.saturating_sub(s.stored_at) < LISTING_TTL_SECS
            && let Ok(c) = serde_json::from_str::<CommitAnswer>(&s.body)
        {
            self.remember(kind, &c.id);
            return Ok(c.id);
        }
        let url = format!(
            "{}/repository/commits/{}",
            self.base(kind),
            pct(&self.config.branch)
        );
        match self.fetcher.get(&url) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let c: CommitAnswer = serde_json::from_str(&text).map_err(|_| {
                    SourceError::Unavailable("the open library answered in an unknown form".into())
                })?;
                let slim = format!("{{\"id\":\"{}\"}}", c.id);
                let _ = self.cache.put_listing(SOURCE_ID, &key, &slim, now);
                self.remember(kind, &c.id);
                Ok(c.id)
            }
            Err(e) => {
                if let Some(s) = saved
                    && let Ok(c) = serde_json::from_str::<CommitAnswer>(&s.body)
                {
                    self.remember(kind, &c.id);
                    return Ok(c.id);
                }
                Err(from_fetch(e, "the library list"))
            }
        }
    }

    fn remember(&self, kind: CadKind, id: &str) {
        if let Ok(mut c) = self.commits.lock() {
            c.insert(kind, id.to_owned());
        }
    }

    /// Items of one folder (or the repository root) with the revision they were read at.
    fn list_dir(&self, kind: CadKind, folder: Option<&str>) -> Result<Listing, SourceError> {
        let key = format!("{}/{}.json", layout(kind).folder, folder.unwrap_or("_root"));
        let now = (self.clock)();
        let saved = self.cache.get_listing(SOURCE_ID, &key)?;
        let parsed = saved.as_ref().and_then(|s| {
            serde_json::from_str::<Listing>(&s.body)
                .ok()
                .map(|l| (s.stored_at, l))
        });
        if let Some((at, l)) = &parsed
            && now.saturating_sub(*at) < LISTING_TTL_SECS
        {
            return Ok(l.clone());
        }
        let fresh = self.read_dir(kind, folder);
        match fresh {
            Ok(l) => {
                if let Ok(text) = serde_json::to_string(&l) {
                    let _ = self.cache.put_listing(SOURCE_ID, &key, &text, now);
                }
                Ok(l)
            }
            Err(e) => match parsed {
                Some((_, l)) => Ok(l),
                None => Err(e),
            },
        }
    }

    fn read_dir(&self, kind: CadKind, folder: Option<&str>) -> Result<Listing, SourceError> {
        let commit = self.commit(kind)?;
        let mut items: Vec<TreeItem> = Vec::new();
        for page in 1..=MAX_PAGES {
            let path = folder
                .map(|f| format!("path={}&", pct(f)))
                .unwrap_or_default();
            let url = format!(
                "{}/repository/tree?{path}ref={commit}&per_page={PAGE_SIZE}&page={page}",
                self.base(kind)
            );
            let what = folder.unwrap_or("the library list");
            let bytes = self.fetcher.get(&url).map_err(|e| from_fetch(e, what))?;
            let got: Vec<TreeItem> = serde_json::from_slice(&bytes).map_err(|_| {
                SourceError::Unavailable("the open library answered in an unknown form".into())
            })?;
            let n = got.len();
            items.extend(got);
            if n < PAGE_SIZE {
                break;
            }
        }
        Ok(Listing { commit, items })
    }

    /// Names of every library of this kind, sorted.
    fn libraries(&self, kind: CadKind) -> Result<Vec<String>, SourceError> {
        let l = layout(kind);
        let mut names: Vec<String> = self
            .list_dir(kind, None)?
            .items
            .into_iter()
            .filter(|i| i.kind == "tree")
            .filter_map(|i| i.name.strip_suffix(l.suffix).map(str::to_owned))
            .collect();
        names.sort();
        Ok(names)
    }

    fn entries(&self, kind: CadKind, library: &str) -> Result<(Vec<CadRef>, String), SourceError> {
        let l = layout(kind);
        let probe = CadRef::new(kind, library, "x")?;
        let folder = format!("{}{}", probe.library, l.suffix);
        let listing = self.list_dir(kind, Some(&folder))?;
        let mut out = Vec::new();
        for item in listing.items.iter().filter(|i| i.kind == "blob") {
            let Some(stem) = l
                .extensions
                .iter()
                .find_map(|ext| item.name.strip_suffix(ext))
            else {
                continue;
            };
            if let Ok(r) = CadRef::new(kind, library, stem).and_then(|r| r.with_revision(&item.id))
            {
                out.push(r);
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok((out, listing.commit))
    }

    /// The listing item behind a reference and the revision it was read at.
    fn locate(&self, cad_ref: &CadRef) -> Result<(TreeItem, String), SourceError> {
        cad_ref.check()?;
        let l = layout(cad_ref.kind);
        let folder = format!("{}{}", cad_ref.library, l.suffix);
        let listing = self.list_dir(cad_ref.kind, Some(&folder))?;
        let item = listing
            .items
            .iter()
            .find(|i| {
                i.kind == "blob"
                    && l.extensions
                        .iter()
                        .any(|ext| i.name.strip_suffix(ext) == Some(cad_ref.name.as_str()))
            })
            .cloned()
            .ok_or_else(|| {
                SourceError::NotFound(format!("{} {}", cad_ref.library, cad_ref.name))
            })?;
        Ok((item, listing.commit))
    }

    fn provenance(&self, r: &CadRef, path: &str, commit: &str) -> Provenance {
        let lic = CadSource::license(self);
        Provenance {
            source_id: SOURCE_ID.into(),
            source_name: SOURCE_NAME.into(),
            kind: r.kind,
            upstream_path: path.to_owned(),
            blob_id: r.revision.clone(),
            commit_id: commit.to_owned(),
            license_name: lic.name,
            license_url: lic.url,
            attribution: CadSource::attribution(self).text,
        }
    }

    fn page_url(&self, kind: CadKind, commit: &str, path: &str) -> String {
        format!(
            "{}/{}/-/blob/{commit}/{path}",
            self.config.web_base,
            self.project(kind)
        )
    }

    /// Which libraries to list for a search and what a name must match there.
    fn plan(&self, kind: CadKind, query: &str) -> Result<SearchPlan, SourceError> {
        let query = check_query(query)?.to_lowercase();
        let all = self.libraries(kind)?;
        let mut chosen: Vec<String> = Vec::new();
        let mut words: Vec<String> = Vec::new();
        let mut targets: Vec<(String, String)> = Vec::new();
        for token in query.split_whitespace() {
            if kind == CadKind::Symbol
                && let Some((_, lib, name)) = SYMBOL_ALIASES.iter().find(|(w, _, _)| *w == token)
            {
                if all.iter().any(|l| l == lib) && !chosen.iter().any(|c| c == lib) {
                    chosen.push((*lib).to_owned());
                }
                targets.push(((*lib).to_owned(), (*name).to_owned()));
                continue;
            }
            let matches: Vec<&String> = all
                .iter()
                .filter(|l| {
                    let lower = l.to_lowercase();
                    lower == token || (token.len() >= 3 && lower.starts_with(token))
                })
                .collect();
            if matches.is_empty() {
                words.push(token.to_owned());
            } else {
                for m in matches {
                    if !chosen.contains(m) {
                        chosen.push(m.clone());
                    }
                }
            }
        }
        if chosen.is_empty() {
            chosen = default_libraries(kind)
                .iter()
                .filter(|d| all.iter().any(|l| l == *d))
                .map(|d| (*d).to_owned())
                .collect();
        }
        chosen.truncate(MAX_LIBRARIES_PER_SEARCH);
        Ok(SearchPlan {
            libraries: chosen,
            words,
            targets,
        })
    }

    /// Footprint names that hold every word of the query, best match first.
    ///
    /// # Rules
    /// - Reads folder listings only; the files stay on the host until one is placed.
    /// - Words that name a library limit the search to it; otherwise the folders of the
    ///   package family come first and, when they hold nothing, more folders are read.
    /// - One folder that cannot be listed does not hide the others.
    fn search_footprints(&self, query: &str) -> Result<Vec<CadRef>, SourceError> {
        let kind = CadKind::Footprint;
        let query = check_query(query)?;
        let all = self.libraries(kind)?;
        let mut chosen: Vec<String> = Vec::new();
        let mut words: Vec<String> = Vec::new();
        for token in query.split_whitespace() {
            let squashed = squash(token);
            if squashed.is_empty() {
                continue;
            }
            let named: Vec<&String> = all
                .iter()
                .filter(|l| {
                    let lib = squash(l);
                    lib == squashed || (squashed.len() >= 3 && lib.starts_with(&squashed))
                })
                .collect();
            if named.is_empty() {
                words.push(squashed);
            } else {
                for m in named {
                    if !chosen.contains(m) {
                        chosen.push(m.clone());
                    }
                }
            }
        }
        let mut near: Vec<String> = Vec::new();
        let mut rest: Vec<String> = Vec::new();
        if chosen.is_empty() {
            let add = |list: &mut Vec<String>, lib: &str| {
                if all.iter().any(|l| l == lib) && !list.iter().any(|l| l == lib) {
                    list.push(lib.to_owned());
                }
            };
            for w in &words {
                let family = letters_of(w);
                for (names, libs) in FOOTPRINT_FAMILIES {
                    if names.contains(&family) {
                        libs.iter().for_each(|l| add(&mut near, l));
                    }
                }
            }
            for w in &words {
                let family = letters_of(w);
                if family.len() >= 3 {
                    for lib in all.iter().filter(|l| squash(l).contains(family)) {
                        add(&mut near, lib);
                    }
                }
            }
            for lib in default_libraries(kind) {
                add(&mut rest, lib);
            }
            for lib in all.iter().filter(|l| l.starts_with("Package_")) {
                add(&mut rest, lib);
            }
            for lib in &all {
                add(&mut rest, lib);
            }
            rest.retain(|l| !near.contains(l));
            near.truncate(MAX_LIBRARIES_PER_SEARCH);
        } else {
            chosen.truncate(MAX_LIBRARIES_PER_SEARCH);
            near = chosen;
        }
        let joined: String = words.concat();
        let mut hits: Vec<(u8, CadRef)> = Vec::new();
        let mut first_error: Option<SourceError> = None;
        let mut scan = |libs: &[String], hits: &mut Vec<(u8, CadRef)>| {
            for lib in libs {
                match self.entries(kind, lib) {
                    Ok((entries, _)) => {
                        for e in entries {
                            let name = squash(&e.name);
                            if words.iter().all(|w| name.contains(w.as_str())) {
                                let rank = if name == joined {
                                    0
                                } else if name.starts_with(&joined) {
                                    1
                                } else if name.contains(&joined) {
                                    2
                                } else {
                                    3
                                };
                                hits.push((rank, e));
                            }
                        }
                    }
                    Err(e) => {
                        first_error.get_or_insert(e);
                    }
                }
            }
        };
        scan(&near, &mut hits);
        if hits.is_empty() {
            rest.truncate(MAX_EXTRA_FOLDERS);
            scan(&rest, &mut hits);
        }
        // Best match first; among equals the plainer name (fewer `_`/`-` parts, so no
        // exposed-pad or thermal-via variant) and then the name in plain order.
        let parts = |r: &CadRef| r.name.split(['_', '-']).count();
        hits.sort_by(|a, b| {
            (a.0, parts(&a.1), &a.1.name, &a.1.library).cmp(&(
                b.0,
                parts(&b.1),
                &b.1.name,
                &b.1.library,
            ))
        });
        hits.truncate(MAX_RESULTS);
        match (hits.is_empty(), first_error) {
            (true, Some(e)) => Err(e),
            _ => Ok(hits.into_iter().map(|(_, r)| r).collect()),
        }
    }

    /// Cards for a search, symbols and/or footprints.
    ///
    /// # Rules
    /// - Footprint cards carry the sku `footprint:Library/Name`, so a symbol and a footprint of the same name stay apart.
    /// - With both kinds asked for, one kind failing leaves the other's cards.
    pub fn search_cards(
        &self,
        scope: CardScope,
        query: &str,
    ) -> Result<Vec<PartResult>, SourceError> {
        let mut out = Vec::new();
        let mut failure = None;
        for kind in [CadKind::Symbol, CadKind::Footprint] {
            let wanted = match scope {
                CardScope::Symbols => kind == CadKind::Symbol,
                CardScope::Footprints => kind == CadKind::Footprint,
                CardScope::Both => true,
            };
            if !wanted {
                continue;
            }
            let found = self.search_refs(kind, query).and_then(|refs| {
                let commit = self.commit(kind)?;
                Ok(refs
                    .iter()
                    .map(|r| self.card(r, &commit))
                    .collect::<Vec<_>>())
            });
            match found {
                Ok(cards) => out.extend(cards),
                Err(e) => failure = failure.or(Some(e)),
            }
        }
        match (out.is_empty(), failure) {
            (true, Some(e)) => Err(e),
            _ => Ok(out),
        }
    }

    fn search_refs(&self, kind: CadKind, query: &str) -> Result<Vec<CadRef>, SourceError> {
        if kind == CadKind::Footprint {
            return self.search_footprints(query);
        }
        let SearchPlan {
            libraries,
            words,
            targets,
        } = self.plan(kind, query)?;
        let mut out: Vec<CadRef> = Vec::new();
        let mut first_error: Option<SourceError> = None;
        for lib in libraries {
            let entries = match self.entries(kind, &lib) {
                Ok((entries, _)) => entries,
                Err(e) => {
                    // One library that cannot be listed must not hide the others.
                    first_error.get_or_insert(e);
                    continue;
                }
            };
            for e in entries {
                let lower = e.name.to_lowercase();
                let by_word = words.iter().all(|w| lower.contains(w.as_str()));
                let by_alias = targets.iter().any(|(l, n)| {
                    *l == e.library && {
                        let n = n.to_lowercase();
                        lower == n || lower.starts_with(&format!("{n}_"))
                    }
                });
                let wants_all = words.is_empty() && targets.is_empty();
                let hit = if targets.is_empty() {
                    by_word
                } else {
                    by_alias && words.iter().all(|w| lower.contains(w.as_str()))
                };
                if hit || wants_all {
                    out.push(e);
                    if out.len() >= MAX_RESULTS {
                        return Ok(out);
                    }
                }
            }
        }
        match (out.is_empty(), first_error) {
            (true, Some(e)) => Err(e),
            _ => Ok(out),
        }
    }

    fn card(&self, r: &CadRef, commit: &str) -> PartResult {
        let l = layout(r.kind);
        let path = format!("{}{}/{}{}", r.library, l.suffix, r.name, l.extensions[0]);
        PartResult {
            source: SOURCE_NAME.into(),
            mpn: r.name.clone(),
            manufacturer: String::new(),
            description: format!(
                "{} in {}. Licence: {LICENSE_NAME} with a design-output exception.",
                match r.kind {
                    CadKind::Symbol => "Symbol",
                    CadKind::Footprint => "Footprint",
                    CadKind::Model => "3D model",
                },
                r.library
            ),
            kind: String::new(),
            distributor: String::new(),
            sku: match r.kind {
                CadKind::Footprint => format!("{FOOTPRINT_SKU_PREFIX}{}/{}", r.library, r.name),
                _ => format!("{}/{}", r.library, r.name),
            },
            stock: 0,
            price_breaks: Vec::new(),
            lifecycle: String::new(),
            datasheet_url: String::new(),
            product_url: self.page_url(r.kind, commit, &path),
            retrieved_at: (self.clock)(),
        }
    }
}

/// Contract: crates/library/src/cad.rs
impl CadSource for OpenLibrarySource {
    fn id(&self) -> &str {
        SOURCE_ID
    }
    fn name(&self) -> &str {
        SOURCE_NAME
    }
    fn license(&self) -> LicenseInfo {
        LicenseInfo {
            name: LICENSE_NAME.into(),
            url: LICENSE_URL.into(),
            terms_note: "Boards and project files that use these parts carry no licence duty. \
                Sharing the library files themselves needs this credit and the licence text."
                .into(),
        }
    }
    fn attribution(&self) -> Attribution {
        Attribution {
            text: "Library data: the open library community, CC-BY-SA 4.0".into(),
            url: LICENSE_URL.into(),
        }
    }
    fn list_libraries(&self, kind: CadKind) -> Result<Vec<String>, SourceError> {
        self.libraries(kind)
    }
    fn list_entries(&self, kind: CadKind, library: &str) -> Result<Vec<CadRef>, SourceError> {
        Ok(self.entries(kind, library)?.0)
    }
    fn search(&self, kind: CadKind, query: &str) -> Result<Vec<CadRef>, SourceError> {
        self.search_refs(kind, query)
    }
    fn cached_path(&self, cad_ref: &CadRef) -> Option<std::path::PathBuf> {
        let (item, _) = self.locate(cad_ref).ok()?;
        let key_path = format!("{}/{}", layout(cad_ref.kind).folder, item.path);
        self.cache.file_path(SOURCE_ID, &key_path, &item.id)
    }
    fn fetch(&self, cad_ref: &CadRef) -> Result<CadFile, SourceError> {
        cad_ref.check()?;
        let (item, commit) = self.locate(cad_ref)?;
        let l = layout(cad_ref.kind);
        let key_path = format!("{}/{}", l.folder, item.path);
        let resolved = cad_ref.clone().with_revision(&item.id)?;
        let bytes = match self.cache.get(SOURCE_ID, &key_path, Some(&item.id))? {
            Some(b) => b,
            None => {
                let url = format!(
                    "{}/repository/files/{}/raw?ref={commit}",
                    self.base(cad_ref.kind),
                    pct(&item.path)
                );
                let b = self
                    .fetcher
                    .get(&url)
                    .map_err(|e| from_fetch(e, &item.path))?;
                if b.len() > MAX_FILE_BYTES {
                    return Err(SourceError::Invalid("the file is too large".into()));
                }
                // A failed save must not hide a file that was fetched fine.
                let _ = self.cache.put(SOURCE_ID, &key_path, Some(&item.id), &b);
                b
            }
        };
        let provenance = self.provenance(&resolved, &item.path, &commit);
        Ok(CadFile {
            cad_ref: resolved,
            bytes,
            provenance,
        })
    }
}

/// Contract: crates/library/src/sources.rs
/// Cards come from symbol files only; `fetch` also reads the symbol's own description.
impl PartSource for OpenLibrarySource {
    fn id(&self) -> &str {
        SOURCE_ID
    }
    fn name(&self) -> &str {
        SOURCE_NAME
    }
    fn search(&self, query: &str) -> Result<Vec<PartResult>, SourceError> {
        let refs = self.search_refs(CadKind::Symbol, query)?;
        let commit = self.commit(CadKind::Symbol)?;
        Ok(refs.iter().map(|r| self.card(r, &commit)).collect())
    }
    fn fetch(&self, mpn: &str) -> Result<PartResult, SourceError> {
        let q = check_query(mpn)?;
        let found = self.search_refs(CadKind::Symbol, q)?;
        let exact = found
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(q))
            .ok_or_else(|| SourceError::NotFound(q.to_owned()))?;
        let file = CadSource::fetch(self, exact)?;
        let mut card = self.card(exact, &file.provenance.commit_id);
        if let Ok(text) = file.text()
            && let Ok(defs) = crate::read_symbols(text)
            && let Some(def) = defs.iter().find(|d| d.name == exact.name)
            && !def.description.is_empty()
        {
            card.description = format!(
                "{}. Licence: {LICENSE_NAME} with a design-output exception.",
                def.description.trim_end_matches('.')
            );
        }
        Ok(card)
    }
    fn setup(&self) -> SetupCard {
        SetupCard {
            steps: vec![
                "Nothing to set up. Parts are read from the open library when you open them, \
                 and kept on this computer so they work offline."
                    .into(),
            ],
            portal_url: String::new(),
            fields: Vec::new(),
        }
    }
    fn attribution(&self) -> Attribution {
        CadSource::attribution(self)
    }
}
