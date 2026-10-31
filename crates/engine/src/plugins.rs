//! Local plugin registry commands (data-only manifests and export templates).
use pcbcraft_doc::Project;
use pcbcraft_plugin::{
    export_output_name, export_template_path, load_export_template, load_registry, render_pin_map,
};
use serde_json::{Value, json};
use std::path::Path;

const MAX_ROOT: usize = 4096;
const MAX_PLUGIN_ID: usize = 64;
const MAX_TEMPLATE_ID: usize = 64;
const MAX_DIRECTORY: usize = 4096;

fn bounded(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.is_empty() || value.len() > max {
        return Err(format!("{label} must be 1 to {max} bytes"));
    }
    Ok(())
}

fn find_plugin(root: &Path, plugin_id: &str) -> Result<pcbcraft_plugin::PluginManifest, String> {
    let manifests = load_registry(root)?;
    manifests
        .into_iter()
        .find(|m| m.id == plugin_id)
        .ok_or_else(|| format!("plugin {plugin_id} not found"))
}

/// List plugins discovered under a local root directory.
pub fn plugins_list(params: &Value) -> Result<Value, String> {
    let root = params
        .get("root")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing root".to_string())?;
    bounded("root", root, MAX_ROOT)?;
    let manifests = load_registry(Path::new(root))?;
    let plugins: Vec<Value> = manifests
        .into_iter()
        .map(|m| {
            json!({
                "id": m.id,
                "name": m.name,
                "version": m.version,
                "license": m.license,
                "export_templates": m.export_templates.iter().map(|t| json!({
                    "id": t.id,
                    "description": t.description,
                    "output_name": if t.output_name.is_empty() { Value::Null } else { json!(t.output_name) },
                })).collect::<Vec<_>>(),
                "rule_set_bundles": m.rule_set_bundles,
                "importers": m.importers.iter().map(|i| json!({"id": i.id})).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({"root": root, "plugins": plugins}))
}

/// Render a plugin export template and optionally write it under `directory`.
pub fn plugins_export(p: &Project, params: &Value) -> Result<Value, String> {
    let root = params
        .get("root")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing root".to_string())?;
    bounded("root", root, MAX_ROOT)?;
    let plugin_id = params
        .get("plugin_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing plugin_id".to_string())?;
    bounded("plugin_id", plugin_id, MAX_PLUGIN_ID)?;
    let template_id = params
        .get("template_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing template_id".to_string())?;
    bounded("template_id", template_id, MAX_TEMPLATE_ID)?;
    let manifest = find_plugin(Path::new(root), plugin_id)?;
    let template_path = export_template_path(&manifest, template_id)?;
    let template = load_export_template(&template_path)?;
    let text = render_pin_map(p, &template)?;
    let file_name = export_output_name(&manifest, template_id)?;
    if let Some(directory) = params.get("directory").and_then(Value::as_str) {
        bounded("directory", directory, MAX_DIRECTORY)?;
        let path = Path::new(directory).join(&file_name);
        pcbcraft_io::atomic_write(&path, text.as_bytes())
            .map_err(|e| format!("could not write export: {e}"))?;
        return Ok(json!({
            "plugin_id": plugin_id,
            "template_id": template_id,
            "directory": directory,
            "file": file_name,
            "bytes": text.len(),
        }));
    }
    Ok(json!({
        "plugin_id": plugin_id,
        "template_id": template_id,
        "file": file_name,
        "text": text,
    }))
}
