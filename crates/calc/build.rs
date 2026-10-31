use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\"', "\\\"")
}

fn str_array(values: &[String], esc_fn: &dyn Fn(&str) -> String) -> String {
    if values.is_empty() {
        return "&[]".to_string();
    }
    let inner: Vec<String> = values
        .iter()
        .map(|v| format!("\"{}\"", esc_fn(v)))
        .collect();
    format!("&[{}]", inner.join(", "))
}

fn load_glossary_parts(
    manifest_dir: &Path,
    calc_ids: &HashSet<String>,
    out_dir: &Path,
) -> Result<(), String> {
    let parts_dir = manifest_dir.join("data/parts");
    println!("cargo:rerun-if-changed={}", parts_dir.display());
    let mut paths: Vec<PathBuf> = fs::read_dir(&parts_dir)
        .map_err(|e| format!("read {}: {e}", parts_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err("no glossary parts in data/parts".into());
    }
    let mut merged: Vec<toml::Value> = Vec::new();
    for path in &paths {
        println!("cargo:rerun-if-changed={}", path.display());
        let raw =
            fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let doc: toml::Value =
            raw.parse().map_err(|e| format!("parse {}: {e}", path.display()))?;
        let terms = doc
            .get("term")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| format!("missing [[term]] in {}", path.display()))?;
        for t in terms {
            merged.push(t.clone());
        }
    }
    let mut ids = HashSet::new();
    for entry in &merged {
        let id = entry
            .get("id")
            .and_then(toml::Value::as_str)
            .ok_or("term id")?;
        if !ids.insert(id.to_string()) {
            return Err(format!("duplicate glossary id {id}"));
        }
        for key in [
            "term",
            "category",
            "short",
            "long",
            "search",
            "source",
        ] {
            if entry
                .get(key)
                .and_then(toml::Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(format!("{id}: missing {key}"));
            }
        }
        if let Some(calc) = entry.get("calculator").and_then(toml::Value::as_str) {
            if !calc.is_empty() && !calc_ids.contains(calc) {
                return Err(format!("{id}: unknown calculator {calc}"));
            }
        }
    }
    for entry in &merged {
        let id = entry["id"].as_str().ok_or("id")?;
        if let Some(related) = entry.get("related").and_then(toml::Value::as_array) {
            for r in related {
                let rid = r.as_str().ok_or("related id")?;
                if !ids.contains(rid) {
                    return Err(format!("{id}: unknown related id {rid}"));
                }
            }
        }
    }

    let mut out = String::from(
        "/// Glossary entry compiled from `data/parts/*.toml`.\n\
         #[derive(Clone, Copy, Debug, PartialEq, Eq)]\n\
         pub struct Term {\n\
             pub id: &'static str,\n\
             pub term: &'static str,\n\
             pub category: &'static str,\n\
             pub short: &'static str,\n\
             pub long: &'static str,\n\
             pub synonyms: &'static [&'static str],\n\
             pub related: &'static [&'static str],\n\
             pub units: &'static str,\n\
             pub in_pcbcraft: &'static [&'static str],\n\
             pub search: &'static str,\n\
             pub calculator: &'static str,\n\
             pub source: &'static str,\n\
         }\n\n\
         pub static TERMS: &[Term] = &[\n",
    );
    for entry in &merged {
        let id = entry["id"].as_str().ok_or("id")?;
        let term = entry["term"].as_str().ok_or("term")?;
        let category = entry["category"].as_str().ok_or("category")?;
        let short = entry["short"].as_str().ok_or("short")?;
        let long = entry["long"].as_str().ok_or("long")?;
        let search = entry["search"].as_str().ok_or("search")?;
        let source = entry["source"].as_str().ok_or("source")?;
        let units = entry
            .get("units")
            .and_then(toml::Value::as_str)
            .unwrap_or("");
        let calculator = entry
            .get("calculator")
            .and_then(toml::Value::as_str)
            .unwrap_or("");
        let synonyms: Vec<String> = entry
            .get("synonyms")
            .and_then(toml::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(toml::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let related: Vec<String> = entry
            .get("related")
            .and_then(toml::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(toml::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let in_pcbcraft: Vec<String> = entry
            .get("in_pcbcraft")
            .and_then(toml::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(toml::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        out.push_str(&format!(
            "    Term {{ id: \"{}\", term: \"{}\", category: \"{}\", short: \"{}\", long: \"{}\", synonyms: {}, related: {}, units: \"{}\", in_pcbcraft: {}, search: \"{}\", calculator: \"{}\", source: \"{}\" }},\n",
            esc(id),
            esc(term),
            esc(category),
            esc(short),
            esc(long),
            str_array(&synonyms, &esc),
            str_array(&related, &esc),
            esc(units),
            str_array(&in_pcbcraft, &esc),
            esc(search),
            esc(calculator),
            esc(source),
        ));
    }
    out.push_str("];\n");
    fs::write(out_dir.join("terms.rs"), out).map_err(|e| e.to_string())?;
    Ok(())
}

fn calculator_ids(manifest_dir: &Path) -> Result<HashSet<String>, String> {
    let data_path = manifest_dir.join("data/calculators.toml");
    println!("cargo:rerun-if-changed={}", data_path.display());
    let raw =
        fs::read_to_string(&data_path).map_err(|e| format!("read {}: {e}", data_path.display()))?;
    let doc: toml::Value = raw.parse().map_err(|e| format!("parse calculators.toml: {e}"))?;
    let calcs = doc
        .get("calculator")
        .and_then(toml::Value::as_array)
        .ok_or("missing calculator array")?;
    let mut ids = HashSet::new();
    for entry in calcs {
        let id = entry
            .get("id")
            .and_then(toml::Value::as_str)
            .ok_or("calculator id")?;
        if !ids.insert(id.to_string()) {
            return Err(format!("duplicate calculator id {id}"));
        }
    }
    Ok(ids)
}

fn run() -> Result<(), String> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?;
    let manifest_path = Path::new(&manifest_dir);
    let calc_ids = calculator_ids(manifest_path)?;
    let out_dir = env::var("OUT_DIR").map_err(|e| e.to_string())?;
    load_glossary_parts(manifest_path, &calc_ids, Path::new(&out_dir))?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("pcbcraft-calc build.rs: {e}");
        std::process::exit(1);
    }
}
