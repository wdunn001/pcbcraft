//! Native library discovery and cached read queries, separate from document mutation.
use pcbcraft_library::{FootprintDefinition, SymbolDefinition};
use serde_json::{Value, json};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

const MAX_LIBRARY_BYTES: u64 = 32 * 1024 * 1024;
fn regular_metadata(path: &Path) -> Result<std::fs::Metadata, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "{}: library source must be a regular file",
            path.display()
        ));
    }
    if metadata.len() > MAX_LIBRARY_BYTES {
        return Err("library exceeds 32 MiB".into());
    }
    Ok(metadata)
}
fn read_library(path: &Path) -> Result<String, String> {
    regular_metadata(path)?;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("library source must be a regular file".into());
    }
    let mut text = String::new();
    file.take(MAX_LIBRARY_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if text.len() as u64 > MAX_LIBRARY_BYTES {
        return Err("library exceeds 32 MiB".into());
    }
    Ok(text)
}
fn scan_directory(path: &Path, limit: usize) -> Result<(Vec<PathBuf>, bool), String> {
    let mut directory = std::fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut files = Vec::new();
    for entry in directory.by_ref().take(limit) {
        files.push(
            entry
                .map_err(|e| format!("{}: {e}", path.display()))?
                .path(),
        );
    }
    let truncated = directory
        .next()
        .transpose()
        .map_err(|e| format!("{}: {e}", path.display()))?
        .is_some();
    Ok((files, truncated))
}

struct CachedSymbols {
    path: PathBuf,
    len: u64,
    modified: Option<std::time::SystemTime>,
    definitions: Arc<Vec<SymbolDefinition>>,
}
// Metadata-based cache: rewriting a file while preserving both length and mtime
// can retain a stale entry. This is a performance cache, not content identity.
// Files without usable modification times are always reparsed. Restart the
// session to refresh deliberately timestamp-preserving external replacements.
static CACHE: OnceLock<Mutex<Vec<CachedSymbols>>> = OnceLock::new();
pub fn symbols(path: &Path) -> Result<Arc<Vec<SymbolDefinition>>, String> {
    if path.extension().and_then(|s| s.to_str()) != Some("kicad_sym") {
        return Err("select a .kicad_sym symbol library file".into());
    }
    let metadata = regular_metadata(path)?;
    let modified = metadata.modified().ok();
    let cache = CACHE.get_or_init(|| Mutex::new(vec![]));
    if let Some(definitions) = cache
        .lock()
        .map_err(|_| "library cache unavailable")?
        .iter()
        .find(|c| {
            modified.is_some()
                && c.path == path
                && c.len == metadata.len()
                && c.modified == modified
        })
        .map(|c| c.definitions.clone())
    {
        return Ok(definitions);
    }
    let text = read_library(path)?;
    let definitions = Arc::new(pcbcraft_library::read_symbols(&text).map_err(|e| e.to_string())?);
    let mut cache = cache.lock().map_err(|_| "library cache unavailable")?;
    cache.retain(|c| c.path != path);
    while cache.len() >= 2 {
        cache.remove(0);
    }
    cache.push(CachedSymbols {
        path: path.into(),
        len: metadata.len(),
        modified,
        definitions: definitions.clone(),
    });
    Ok(definitions)
}
pub fn symbol(path: &Path, name: &str) -> Result<SymbolDefinition, String> {
    symbols(path)?
        .iter()
        .find(|s| s.name == name)
        .cloned()
        .ok_or_else(|| format!("symbol {name} not found in {}", path.display()))
}
pub fn footprint(path: &Path) -> Result<FootprintDefinition, String> {
    if path.extension().and_then(|s| s.to_str()) != Some("kicad_mod") {
        return Err("select a .kicad_mod footprint file".into());
    }
    pcbcraft_library::read_footprint(&read_library(path)?).map_err(|e| e.to_string())
}
/// A named folder the user registered so its symbol and footprint collections
/// join the one library the browser shows, no matter where they live.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct LibrarySource {
    pub name: String,
    pub path: String,
}
const MAX_LIBRARY_SOURCES: usize = 64;
pub static SOURCES_PATH_TEST: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Held by any test that redirects the sources file, so two never
/// race over the shared slot.
pub static SOURCES_TEST_LOCK: Mutex<()> = Mutex::new(());
fn sources_path() -> Result<PathBuf, String> {
    if let Some(path) = SOURCES_PATH_TEST
        .lock()
        .ok()
        .and_then(|locked| locked.as_ref().cloned())
    {
        return Ok(path);
    }
    let directory = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    directory("XDG_CONFIG_HOME")
        .or_else(|| directory("APPDATA"))
        .or_else(|| directory("HOME").map(|home| home.join(".config")))
        .map(|base| base.join("pcbcraft").join("library-sources.json"))
        .ok_or_else(|| "no user configuration folder was found".into())
}
fn usable_source(source: &LibrarySource) -> bool {
    !source.name.trim().is_empty()
        && source.name.chars().count() <= 64
        && !source.path.trim().is_empty()
        && source.path.chars().count() <= 1024
        && std::fs::metadata(&source.path).is_ok_and(|metadata| metadata.is_dir())
}
fn load_sources() -> Vec<LibrarySource> {
    let Ok(path) = sources_path() else {
        return vec![];
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return vec![];
    };
    serde_json::from_str::<Vec<LibrarySource>>(&text)
        .unwrap_or_default()
        .into_iter()
        .filter(usable_source)
        .take(MAX_LIBRARY_SOURCES)
        .collect()
}
fn same_folder(left: &Path, right: &Path) -> bool {
    left.canonicalize().unwrap_or_else(|_| left.to_path_buf())
        == right.canonicalize().unwrap_or_else(|_| right.to_path_buf())
}
/// Which named source a scanned root belongs to, so the unified list can say
/// where every collection came from.
fn source_label(root: &Path) -> String {
    if let Some(source) = load_sources()
        .into_iter()
        .find(|source| same_folder(Path::new(&source.path), root))
    {
        return source.name;
    }
    if built_in_roots()
        .iter()
        .any(|installed| same_folder(installed, root))
    {
        return "Installed collections".into();
    }
    "Other libraries".into()
}
fn write_sources(sources: &[LibrarySource]) -> Result<(), String> {
    let file = sources_path()?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    pcbcraft_io::atomic_write(
        &file,
        serde_json::to_string_pretty(sources)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
}
fn counts(root: &Path) -> Result<(usize, usize), String> {
    let scanned = catalog_paths(vec![root.to_path_buf()], 200)?;
    Ok((
        scanned["symbol_libraries"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0),
        scanned["footprint_libraries"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0),
    ))
}
/// Register any folder on this computer as a library source; its collections
/// join every other collection in the unified browser on the next scan.
pub fn add_folder(path: &str, name: Option<&str>) -> Result<Value, String> {
    let path = path.trim();
    if path.is_empty() || path.chars().count() > 1024 {
        return Err("provide the full path to a library folder".into());
    }
    let folder = PathBuf::from(path);
    if !std::fs::metadata(&folder).is_ok_and(|metadata| metadata.is_dir()) {
        return Err("choose a folder that exists on this computer".into());
    }
    let canonical = folder
        .canonicalize()
        .map_err(|e| format!("{}: {e}", folder.display()))?;
    let name = name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            canonical
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Library folder")
                .replace(['_', '-'], " ")
        });
    if name.chars().count() > 64 {
        return Err("source names must be 64 characters or fewer".into());
    }
    let mut sources = load_sources();
    if sources.len() >= MAX_LIBRARY_SOURCES {
        return Err("library sources are limited to 64 folders".into());
    }
    if sources
        .iter()
        .any(|source| same_folder(Path::new(&source.path), &canonical))
    {
        return Err("that folder is already one of the library sources".into());
    }
    let (symbols, footprints) = counts(&canonical)?;
    sources.push(LibrarySource {
        name: name.clone(),
        path: canonical.to_string_lossy().into_owned(),
    });
    write_sources(&sources)?;
    Ok(json!({
        "added": name,
        "path": canonical.to_string_lossy(),
        "symbol_libraries": symbols,
        "footprint_libraries": footprints,
        "sources": self::sources()?["sources"].clone(),
    }))
}
/// Forget an added folder; the installed locations are never removed.
pub fn remove_folder(path: &str) -> Result<Value, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("provide the folder path to remove".into());
    }
    let target = PathBuf::from(path);
    let mut removed = None;
    let mut sources = Vec::new();
    for source in load_sources() {
        if removed.is_none() && same_folder(Path::new(&source.path), &target) {
            removed = Some(source.name);
        } else {
            sources.push(source);
        }
    }
    let Some(name) = removed else {
        return Err("that folder is not one of the added library sources".into());
    };
    write_sources(&sources)?;
    Ok(json!({"removed": name, "sources": self::sources()?["sources"].clone()}))
}
/// Every place collections are read from: added folders first, then installed
/// locations, each with how many collections it contributes.
pub fn sources() -> Result<Value, String> {
    let mut list: Vec<Value> = Vec::new();
    for source in load_sources() {
        let (symbols, footprints) = counts(&PathBuf::from(&source.path)).unwrap_or((0, 0));
        list.push(json!({
            "name": source.name,
            "path": source.path,
            "kind": "added",
            "symbol_libraries": symbols,
            "footprint_libraries": footprints,
        }));
    }
    for installed in built_in_roots() {
        if !std::fs::metadata(&installed).is_ok_and(|metadata| metadata.is_dir()) {
            continue;
        }
        let (symbols, footprints) = counts(&installed).unwrap_or((0, 0));
        list.push(json!({
            "name": "Installed collections",
            "path": installed.to_string_lossy(),
            "kind": "built-in",
            "symbol_libraries": symbols,
            "footprint_libraries": footprints,
        }));
    }
    Ok(json!({"sources": list}))
}
fn paths() -> Vec<PathBuf> {
    let mut roots = built_in_roots();
    roots.extend(
        load_sources()
            .into_iter()
            .map(|source| PathBuf::from(source.path)),
    );
    roots
}
fn built_in_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/usr/share/kicad"),
        PathBuf::from("/usr/local/share/kicad"),
        PathBuf::from("C:/Program Files/KiCad/10.0/share/kicad"),
        PathBuf::from("C:/Program Files/KiCad/9.0/share/kicad"),
        PathBuf::from(r"\\wsl.localhost\Ubuntu\usr\share\kicad"),
    ];
    for version in ["10", "9", "8"] {
        for suffix in ["SYMBOL_DIR", "FOOTPRINT_DIR"] {
            if let Ok(path) = std::env::var(format!("KICAD{version}_{suffix}")) {
                roots.push(PathBuf::from(path));
            }
        }
    }
    roots
}
pub fn catalog() -> Result<Value, String> {
    catalog_paths(paths(), 4096)
}
fn catalog_paths(roots: Vec<PathBuf>, scan_limit: usize) -> Result<Value, String> {
    let mut symbol_files = vec![];
    let mut footprint_libraries = vec![];
    let mut truncated_directories = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for root in roots {
        let source = source_label(&root);
        for (directory, symbols) in [
            (root.join("symbols"), true),
            (root.join("footprints"), false),
            (root.clone(), true),
            (root, false),
        ] {
            if !seen.insert((directory.clone(), symbols)) {
                continue;
            }
            match std::fs::metadata(&directory) {
                Ok(metadata) if metadata.is_dir() => {}
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("{}: {e}", directory.display())),
            }
            let (entries, truncated) = scan_directory(&directory, scan_limit)?;
            if truncated {
                truncated_directories.push(directory.to_string_lossy().into_owned());
            }
            for path in entries {
                let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                let item = json!({"name":path.file_stem().and_then(|s|s.to_str()).unwrap_or(""),"path":path.to_string_lossy(),"source":source});
                if symbols
                    && extension == "kicad_sym"
                    && std::fs::metadata(&path)
                        .map_err(|e| format!("{}: {e}", path.display()))?
                        .is_file()
                {
                    symbol_files.push(item);
                } else if !symbols
                    && extension == "pretty"
                    && std::fs::metadata(&path)
                        .map_err(|e| format!("{}: {e}", path.display()))?
                        .is_dir()
                {
                    footprint_libraries.push(item);
                }
            }
        }
    }
    symbol_files.sort_by(|a, b| {
        a.get("name")
            .and_then(Value::as_str)
            .cmp(&b.get("name").and_then(Value::as_str))
    });
    footprint_libraries.sort_by(|a, b| {
        a.get("name")
            .and_then(Value::as_str)
            .cmp(&b.get("name").and_then(Value::as_str))
    });
    symbol_files.dedup();
    footprint_libraries.dedup();
    truncated_directories.sort();
    truncated_directories.dedup();
    Ok(
        json!({"symbol_libraries":symbol_files,"footprint_libraries":footprint_libraries,"scan_truncated":!truncated_directories.is_empty(),"truncated_directories":truncated_directories,"license":"KiCad official libraries: CC-BY-SA-4.0 with design exception; third-party libraries retain their own licenses","license_url":"https://www.kicad.org/libraries/license/"}),
    )
}
pub fn search(path: &Path, kind: &str, query: &str, limit: usize) -> Result<Value, String> {
    if query.len() > 256 || limit == 0 || limit > 100 {
        return Err("query must be <=256 bytes and limit 1–100".into());
    }
    let query = query.to_lowercase();
    let mut entries = vec![];
    let mut total = 0;
    let mut scan_truncated = false;
    match kind {
        "symbol" => {
            let definitions = symbols(path)?;
            for s in definitions.iter().filter(|s| {
                format!("{} {} {}", s.name, s.description, s.keywords)
                    .to_lowercase()
                    .contains(&query)
            }) {
                total += 1;
                if entries.len() < limit {
                    entries.push(json!({"name":s.name,"description":s.description,"keywords":s.keywords,"footprint":s.footprint,"pins":s.pins.len(),"units":s.pins.iter().map(|p|p.unit).max().unwrap_or(1),"path":path.to_string_lossy(),"kind":"symbol","unsupported":s.unsupported}));
                }
            }
        }
        "footprint" => {
            let (directory, truncated) = scan_directory(path, 10_000)?;
            scan_truncated = truncated;
            let mut files = vec![];
            for path in directory {
                if path.extension().and_then(|s| s.to_str()) == Some("kicad_mod")
                    && std::fs::metadata(&path)
                        .map_err(|e| format!("{}: {e}", path.display()))?
                        .is_file()
                {
                    files.push(path);
                }
            }
            files.sort();
            for file in files {
                let name = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if !name.to_lowercase().contains(&query) {
                    continue;
                }
                total += 1;
                if entries.len() < limit {
                    entries.push(
                        json!({"name":name,"path":file.to_string_lossy(),"kind":"footprint"}),
                    );
                }
            }
        }
        _ => return Err("kind must be symbol or footprint".into()),
    }
    Ok(
        json!({"entries":entries,"total":total,"limit":limit,"truncated":total>limit||scan_truncated,"scan_truncated":scan_truncated}),
    )
}
/// Compare two library parts of the same kind and report what differs.
pub fn compare(
    path_a: &Path,
    path_b: &Path,
    kind: &str,
    name_a: &str,
    name_b: &str,
) -> Result<Value, String> {
    match kind {
        "symbol" => {
            let a = symbol(path_a, name_a)?;
            let b = symbol(path_b, name_b)?;
            let pins = |s: &SymbolDefinition| -> Vec<String> {
                s.pins.iter().map(|p| p.number.clone()).collect()
            };
            let (pa, pb) = (pins(&a), pins(&b));
            let only = |x: &[String], y: &[String]| -> Vec<String> {
                x.iter().filter(|n| !y.contains(n)).cloned().collect()
            };
            let units =
                |s: &SymbolDefinition| -> u32 { s.pins.iter().map(|p| p.unit).max().unwrap_or(1) };
            Ok(json!({
                "kind": "symbol",
                "a": {
                    "name": a.name,
                    "path": path_a.to_string_lossy(),
                    "pins": pa.len(),
                    "units": units(&a),
                    "reference_prefix": a.reference,
                    "default_value": a.value,
                    "footprint": a.footprint,
                },
                "b": {
                    "name": b.name,
                    "path": path_b.to_string_lossy(),
                    "pins": pb.len(),
                    "units": units(&b),
                    "reference_prefix": b.reference,
                    "default_value": b.value,
                    "footprint": b.footprint,
                },
                "shared_pins": pa.iter().filter(|n| pb.contains(n)).count(),
                "pins_only_in_a": only(&pa, &pb),
                "pins_only_in_b": only(&pb, &pa),
                "same_footprint_recommendation": a.footprint == b.footprint,
                "same_reference_prefix": a.reference == b.reference,
            }))
        }
        "footprint" => {
            let a = footprint(path_a)?;
            let b = footprint(path_b)?;
            let pads = |f: &FootprintDefinition| -> Vec<String> {
                f.pads.iter().map(|p| p.number.clone()).collect()
            };
            let (pa, pb) = (pads(&a), pads(&b));
            let only = |x: &[String], y: &[String]| -> Vec<String> {
                x.iter().filter(|n| !y.contains(n)).cloned().collect()
            };
            Ok(json!({
                "kind": "footprint",
                "a": {
                    "name": a.name,
                    "path": path_a.to_string_lossy(),
                    "pads": pa.len(),
                    "graphics": a.graphics.len(),
                },
                "b": {
                    "name": b.name,
                    "path": path_b.to_string_lossy(),
                    "pads": pb.len(),
                    "graphics": b.graphics.len(),
                },
                "shared_pads": pa.iter().filter(|n| pb.contains(n)).count(),
                "pads_only_in_a": only(&pa, &pb),
                "pads_only_in_b": only(&pb, &pa),
                "same_pad_count": pa.len() == pb.len(),
            }))
        }
        _ => Err("kind must be symbol or footprint".into()),
    }
}
pub fn read_tool(name: &str, args: &Value) -> Result<Value, String> {
    let text = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("missing {key}"))
    };
    match name {
        "library.catalog" => catalog(),
        "library.sources" => sources(),
        "design.recents" => Ok(json!({"designs": crate::recents::load_recent_paths()})),
        "library.add_folder" => {
            let name = match args.get("name") {
                None => None,
                Some(v) => Some(v.as_str().ok_or("name must be a string")?),
            };
            add_folder(text("path")?, name)
        }
        "library.remove_folder" => remove_folder(text("path")?),
        "library.search" => {
            let limit = match args.get("limit") {
                None => 30,
                Some(v) => v
                    .as_u64()
                    .filter(|v| (1..=100).contains(v))
                    .ok_or("limit must be an integer from 1 to 100")?,
            };
            let limit = usize::try_from(limit).map_err(|_| "limit exceeds platform bounds")?;
            let query = match args.get("query") {
                None => "",
                Some(v) => v.as_str().ok_or("query must be a string")?,
            };
            search(Path::new(text("path")?), text("kind")?, query, limit)
        }
        "library.inspect_symbol" => {
            let mut definition = symbol(Path::new(text("path")?), text("name")?)?;
            let include_source = args
                .get("include_source")
                .map(|v| v.as_bool().ok_or("include_source must be boolean"))
                .transpose()?
                .unwrap_or(false);
            let truncated = include_source && definition.raw_source.len() > 512 * 1024;
            if !include_source || truncated {
                definition.raw_source.clear();
            }
            Ok(
                json!({"definition":definition,"kind":"symbol","path":text("path")?,"source_truncated":truncated}),
            )
        }
        "library.inspect_footprint" => {
            let mut definition = footprint(Path::new(text("path")?))?;
            let include_source = args
                .get("include_source")
                .map(|v| v.as_bool().ok_or("include_source must be boolean"))
                .transpose()?
                .unwrap_or(false);
            let truncated = include_source
                && definition.raw_source.len()
                    + definition
                        .pads
                        .iter()
                        .map(|p| p.raw_source.len())
                        .sum::<usize>()
                    > 512 * 1024;
            if !include_source || truncated {
                definition.raw_source.clear();
                for pad in &mut definition.pads {
                    pad.raw_source.clear();
                }
            }
            Ok(
                json!({"definition":definition,"kind":"footprint","path":text("path")?,"source_truncated":truncated}),
            )
        }
        "library.compare" => {
            let optional = |key: &str| -> Result<String, String> {
                match args.get(key) {
                    None => Ok(String::new()),
                    Some(v) => v
                        .as_str()
                        .map(str::to_string)
                        .ok_or(format!("{key} must be a string")),
                }
            };
            compare(
                Path::new(text("path_a")?),
                Path::new(text("path_b")?),
                text("kind")?,
                &optional("name_a")?,
                &optional("name_b")?,
            )
        }
        _ => Err("unknown library query".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Result<Self, String> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "pcbcraft-library-test-{}-{timestamp}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).map_err(|e| e.to_string())?;
            Ok(Self(path))
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn library_search_rejects_bad_json_types_before_filesystem_access() {
        for limit in [
            json!(0),
            json!(101),
            json!(-1),
            json!(1.5),
            json!("10"),
            json!(null),
            json!(4_294_967_297u64),
        ] {
            let args = json!({"path":"missing.kicad_sym","kind":"symbol","limit":limit});
            assert!(read_tool("library.search", &args).is_err_and(|e| e.contains("limit")));
        }
        assert!(
            read_tool(
                "library.search",
                &json!({"path":"missing.kicad_sym","kind":"symbol","query":true})
            )
            .is_err_and(|e| e.contains("query"))
        );
    }
    #[test]
    fn library_reads_are_regular_bounded_and_independent_of_project_cap() -> Result<(), String> {
        let directory = TestDirectory::new()?;
        let path = directory.0.join("large.kicad_sym");
        let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        file.write_all(&vec![b' '; 8 * 1024 * 1024 + 128])
            .map_err(|e| e.to_string())?;
        file.write_all(b"(kicad_symbol_lib (symbol X))")
            .map_err(|e| e.to_string())?;
        drop(file);
        assert_eq!(symbols(&path)?.len(), 1);
        assert!(read_library(&directory.0).is_err_and(|e| e.contains("regular file")));
        let too_large = directory.0.join("oversized.kicad_sym");
        std::fs::File::create(&too_large)
            .and_then(|f| f.set_len(MAX_LIBRARY_BYTES + 1))
            .map_err(|e| e.to_string())?;
        assert!(symbols(&too_large).is_err_and(|e| e.contains("32 MiB")));
        let non_file = directory.0.join("directory.kicad_mod");
        std::fs::create_dir(&non_file).map_err(|e| e.to_string())?;
        assert!(footprint(&non_file).is_err_and(|e| e.contains("regular file")));
        Ok(())
    }
    #[test]
    fn directory_scans_report_truncation_and_errors() -> Result<(), String> {
        let directory = TestDirectory::new()?;
        for name in ["A.kicad_sym", "B.kicad_sym"] {
            std::fs::write(directory.0.join(name), "(kicad_symbol_lib)")
                .map_err(|e| e.to_string())?;
        }
        let (files, truncated) = scan_directory(&directory.0, 1)?;
        assert_eq!(files.len(), 1);
        assert!(truncated);
        let (_, truncated) = scan_directory(&directory.0, 2)?;
        assert!(!truncated);
        assert!(scan_directory(&directory.0.join("absent"), 10).is_err());
        let catalog = catalog_paths(vec![directory.0.clone()], 1)?;
        assert_eq!(catalog["scan_truncated"], true);
        assert_eq!(
            catalog["truncated_directories"].as_array().map(Vec::len),
            Some(1)
        );
        Ok(())
    }
    #[test]
    fn footprint_search_only_returns_files_and_reports_result_limit() -> Result<(), String> {
        let directory = TestDirectory::new()?;
        for name in ["A.kicad_mod", "B.kicad_mod"] {
            std::fs::write(directory.0.join(name), "(footprint X)").map_err(|e| e.to_string())?;
        }
        std::fs::create_dir(directory.0.join("C.kicad_mod")).map_err(|e| e.to_string())?;
        let results = search(&directory.0, "footprint", "", 1)?;
        assert_eq!(results["total"], 2);
        assert_eq!(results["truncated"], true);
        assert_eq!(results["scan_truncated"], false);
        assert_eq!(results["entries"].as_array().map(Vec::len), Some(1));
        Ok(())
    }
    #[test]
    fn added_folders_join_the_one_library_and_can_be_removed_again() -> Result<(), String> {
        let _guard = SOURCES_TEST_LOCK
            .lock()
            .map_err(|_| "test lock unavailable")?;
        let root = TestDirectory::new()?;
        let vendor = root.0.join("digikey_footprints");
        std::fs::create_dir_all(vendor.join("footprints")).map_err(|e| e.to_string())?;
        std::fs::write(
            vendor.join("MyParts.kicad_sym"),
            "(kicad_symbol_lib (version 20250114))",
        )
        .map_err(|e| e.to_string())?;
        std::fs::create_dir_all(vendor.join("footprints/Resistors.pretty"))
            .map_err(|e| e.to_string())?;
        SOURCES_PATH_TEST
            .lock()
            .map_err(|_| "test lock unavailable")?
            .replace(root.0.join("sources.json"));
        let added = add_folder(vendor.to_str().ok_or("folder path is not text")?, None)?;
        assert_eq!(added["added"], "digikey footprints");
        assert_eq!(added["symbol_libraries"], 1);
        assert_eq!(added["footprint_libraries"], 1);
        let listing = catalog_paths(paths(), 64)?;
        let entry = listing["symbol_libraries"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["name"] == "MyParts"))
            .ok_or("the added folder is missing from the unified listing")?;
        assert_eq!(
            entry["source"], "digikey footprints",
            "every collection says where it came from"
        );
        let listed = sources()?;
        assert!(
            listed["sources"].as_array().is_some_and(|items| items
                .iter()
                .any(|item| item["kind"] == "added"
                    && item["name"] == "digikey footprints"
                    && item["symbol_libraries"] == 1)),
            "the source list names the added folder with its counts"
        );
        assert!(
            add_folder(vendor.to_str().ok_or("folder path is not text")?, None)
                .is_err_and(|error| error.contains("already")),
            "the same folder is not registered twice"
        );
        assert!(
            add_folder("/definitely/missing/library-folder", None)
                .is_err_and(|error| error.contains("folder that exists")),
            "a missing folder is refused"
        );
        let long = "n".repeat(65);
        assert!(
            add_folder(
                vendor.to_str().ok_or("folder path is not text")?,
                Some(&long)
            )
            .is_err_and(|error| error.contains("64")),
            "source names are bounded"
        );
        let removed = remove_folder(vendor.to_str().ok_or("folder path is not text")?)?;
        assert_eq!(removed["removed"], "digikey footprints");
        assert!(
            remove_folder(vendor.to_str().ok_or("folder path is not text")?)
                .is_err_and(|error| error.contains("not one of")),
            "removing twice says so plainly"
        );
        SOURCES_PATH_TEST
            .lock()
            .map_err(|_| "test lock unavailable")?
            .take();
        Ok(())
    }
    #[test]
    fn library_source_tools_answer_through_the_read_tool_router() -> Result<(), String> {
        let _guard = SOURCES_TEST_LOCK
            .lock()
            .map_err(|_| "test lock unavailable")?;
        let listing = read_tool("library.sources", &json!({}))?;
        assert!(
            listing["sources"].is_array(),
            "sources always answers with a list"
        );
        assert!(
            read_tool("design.recents", &json!({}))?["designs"].is_array(),
            "the recent designs tool answers through the same router"
        );
        assert!(
            read_tool("library.add_folder", &json!({"path": 4}))
                .is_err_and(|error| error.contains("path")),
            "a non-text path is refused"
        );
        assert!(
            read_tool("library.remove_folder", &json!({}))
                .is_err_and(|error| error.contains("path")),
            "a missing path is refused"
        );
        Ok(())
    }
}
