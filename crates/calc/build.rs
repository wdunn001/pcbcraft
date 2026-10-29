use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;

fn run() -> Result<(), String> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?;
    let data_path = Path::new(&manifest_dir).join("data/calculators.toml");
    println!("cargo:rerun-if-changed={}", data_path.display());
    let raw =
        fs::read_to_string(&data_path).map_err(|e| format!("read {}: {e}", data_path.display()))?;
    let doc: toml::Value = raw.parse().map_err(|e| format!("parse calculators.toml: {e}"))?;
    let calcs = doc
        .get("calculator")
        .and_then(toml::Value::as_array)
        .ok_or("missing calculator array")?;
    let mut ids = HashSet::new();
    let mut vector_count = 0usize;
    for entry in calcs {
        let id = entry
            .get("id")
            .and_then(toml::Value::as_str)
            .ok_or("calculator id")?;
        if !ids.insert(id.to_string()) {
            return Err(format!("duplicate calculator id {id}"));
        }
        let inputs = entry
            .get("input")
            .and_then(toml::Value::as_array)
            .ok_or("input array")?;
        for input in inputs {
            let min = input
                .get("min")
                .and_then(toml::Value::as_float)
                .ok_or("min")?;
            let max = input
                .get("max")
                .and_then(toml::Value::as_float)
                .ok_or("max")?;
            if min > max {
                return Err(format!("{id}: input min > max"));
            }
        }
        let vectors = entry
            .get("vector")
            .and_then(toml::Value::as_array)
            .ok_or("vector array")?;
        vector_count += vectors.len();
    }
    if ids.len() != 32 {
        return Err(format!("expected 32 calculators, got {}", ids.len()));
    }
    if vector_count != 119 {
        return Err(format!("expected 119 golden vectors, got {vector_count}"));
    }

    let out_dir = env::var("OUT_DIR").map_err(|e| e.to_string())?;
    let mut out = String::from(
        "/// Calculator metadata compiled from `data/calculators.toml`.\n\
         #[derive(Clone, Copy, Debug, PartialEq, Eq)]\n\
         pub struct CalculatorSpec {\n\
             pub id: &'static str,\n\
             pub title: &'static str,\n\
             pub group: &'static str,\n\
             pub accuracy: crate::Accuracy,\n\
             pub formula: &'static str,\n\
             pub source: &'static str,\n\
         }\n\n\
         pub static CALCULATORS: &[CalculatorSpec] = &[\n",
    );
    for entry in calcs {
        let id = entry["id"].as_str().ok_or("id")?;
        let title = entry["title"].as_str().ok_or("title")?;
        let group = entry["group"].as_str().ok_or("group")?;
        let accuracy = entry["accuracy"].as_str().ok_or("accuracy")?;
        let formula = entry["formula"].as_str().ok_or("formula")?;
        let source = entry["source"].as_str().ok_or("source")?;
        let acc = match accuracy {
            "exact" => "Accuracy::Exact",
            "model" => "Accuracy::Model",
            "estimate" => "Accuracy::Estimate",
            other => return Err(format!("unknown accuracy {other} for {id}")),
        };
        let esc = |s: &str| s.replace('\\', "\\\\").replace('\"', "\\\"");
        out.push_str(&format!(
            "    CalculatorSpec {{ id: \"{}\", title: \"{}\", group: \"{}\", accuracy: {acc}, formula: \"{}\", source: \"{}\" }},\n",
            esc(id),
            esc(title),
            esc(group),
            esc(formula),
            esc(source)
        ));
    }
    out.push_str("];\n");
    fs::write(Path::new(&out_dir).join("catalog.rs"), out).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("pcbcraft-calc build.rs: {e}");
        std::process::exit(1);
    }
}
