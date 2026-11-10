//! Lossless library export acceptance tests for issue 97.
//!
//! Every fixture under crates/library/tests/fixtures is read, written back with
//! the pcbcraft_io writers, and read again. The definitions must match once the
//! raw_source text cache is stripped, since the writer regenerates that cache, and
//! unknown nodes must survive the round trip.
//!
//! Local rules: no unwrap, expect or panic; every fallible step returns
//! Result<(), String> with the fixture path in the message. The writers are
//! pcbcraft_io writers operating on pcbcraft_library definitions.

use std::path::{Path, PathBuf};

/// Resolve the fixtures directory relative to this crate's manifest.
fn fixtures_dir() -> Result<PathBuf, String> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR")).join("../library/tests/fixtures"))
}

/// List fixture files with the given extension, sorted by path.
fn fixture_paths(extension: &str) -> Result<Vec<PathBuf>, String> {
    let dir = fixtures_dir()?;
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        if entry
            .file_type()
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .is_file()
            && entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e == extension)
                .unwrap_or(false)
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err(format!("no {extension} fixtures found"));
    }
    Ok(paths)
}

/// Read a text file, attaching the path to any error.
fn read_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Recursively remove every raw_source key from a JSON value so definitions
/// compare equal even though the writer regenerates that text cache.
fn strip_raw_sources(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let cleaned: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .filter(|(key, _)| key != "raw_source")
                .map(|(key, val)| (key, strip_raw_sources(val)))
                .collect();
            serde_json::Value::Object(cleaned)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(strip_raw_sources).collect())
        }
        other => other,
    }
}

/// Serialize one footprint definition to JSON with raw_source removed, for
/// lossless round trip comparison.
fn comparable_footprint(
    def: &pcbcraft_library::FootprintDefinition,
) -> Result<serde_json::Value, String> {
    Ok(strip_raw_sources(
        serde_json::to_value(def).map_err(|e| e.to_string())?,
    ))
}

/// Serialize symbol definitions to JSON with raw_source removed, for lossless
/// round trip comparison.
fn comparable_symbols(
    defs: &[pcbcraft_library::SymbolDefinition],
) -> Result<serde_json::Value, String> {
    Ok(strip_raw_sources(
        serde_json::to_value(defs).map_err(|e| e.to_string())?,
    ))
}

#[test]
fn footprint_fixtures_round_trip_keep_the_definition() -> Result<(), String> {
    for path in fixture_paths("kicad_mod")? {
        let text = read_text(&path)?;
        let def = pcbcraft_library::read_footprint(&text)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let written = pcbcraft_io::footprint_library_text(&def)?;
        let again = pcbcraft_library::read_footprint(&written)
            .map_err(|e| format!("{} rewrite: {e}", path.display()))?;
        assert_eq!(comparable_footprint(&def)?, comparable_footprint(&again)?);
    }
    Ok(())
}

#[test]
fn symbol_fixtures_round_trip_keep_the_definitions() -> Result<(), String> {
    for path in fixture_paths("kicad_sym")? {
        let text = read_text(&path)?;
        let mut defs = pcbcraft_library::read_symbols(&text)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let written = pcbcraft_io::symbols_library_file_text(&defs)?;
        let mut again = pcbcraft_library::read_symbols(&written)
            .map_err(|e| format!("{} rewrite: {e}", path.display()))?;
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        again.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(comparable_symbols(&defs)?, comparable_symbols(&again)?);
    }
    Ok(())
}

#[test]
fn unknown_footprint_nodes_survive_the_round_trip() -> Result<(), String> {
    let path = fixtures_dir()?.join("unknown-nodes.kicad_mod");
    let text = read_text(&path)?;
    let def =
        pcbcraft_library::read_footprint(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let written = pcbcraft_io::footprint_library_text(&def)?;
    let again = pcbcraft_library::read_footprint(&written)
        .map_err(|e| format!("{} rewrite: {e}", path.display()))?;
    if again.retained.is_empty() && again.pads.iter().all(|p| p.raw_source.is_empty()) {
        return Err(format!("{} lost unknown nodes", path.display()));
    }
    Ok(())
}

#[test]
fn unknown_symbol_nodes_survive_the_round_trip() -> Result<(), String> {
    let path = fixtures_dir()?.join("unknown-nodes.kicad_sym");
    let text = read_text(&path)?;
    let defs =
        pcbcraft_library::read_symbols(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let written = pcbcraft_io::symbols_library_file_text(&defs)?;
    let again = pcbcraft_library::read_symbols(&written)
        .map_err(|e| format!("{} rewrite: {e}", path.display()))?;
    if again.iter().all(|s| s.retained.is_empty()) {
        return Err(format!("{} lost unknown nodes", path.display()));
    }
    Ok(())
}
