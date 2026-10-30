//! Read-only and folder-writing tools for static registries (no portal service).
//!
//! # Rules
//! - Every install verifies file hashes before writing.
//! - Registry paths must exist on disk; this module does not fetch URLs.
//!
//! # Dependencies
//! - crates/library/src/static_registry.rs
use pcbcraft_library::static_registry::{
    PublishInput, build_catalogue, install_publication, merge_catalogues, publish_to_folder,
    read_instance_feed, read_sources_index, rebuild_site, verify_publication_dir,
    verify_registry_manifest, catalogue_with_origin,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {key}"))
}

fn catalogue_json(registry_root: &Path) -> Result<Value, String> {
    let catalogue = build_catalogue(registry_root)?;
    serde_json::to_value(catalogue).map_err(|e| e.to_string())
}

pub fn read_tool(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "registry.catalogue" => catalogue_json(Path::new(text(args, "path")?)),
        "registry.sources" => {
            let doc = read_sources_index(Path::new(text(args, "path")?))?;
            serde_json::to_value(doc).map_err(|e| e.to_string())
        }
        "registry.verify" => {
            let root = Path::new(text(args, "path")?);
            verify_registry_manifest(root)?;
            for item in build_catalogue(root)?.items {
                verify_publication_dir(&root.join(&item.path))?;
            }
            Ok(json!({"ok": true}))
        }
        "registry.rebuild" => {
            let root = Path::new(text(args, "path")?);
            let catalogue = rebuild_site(root)?;
            Ok(json!({"items": catalogue.items.len()}))
        }
        "registry.publish" => publish(args),
        "registry.install" => {
            let root = Path::new(text(args, "registry")?);
            let dest = Path::new(text(args, "dest")?);
            let files = install_publication(
                root,
                text(args, "id")?,
                text(args, "version")?,
                dest,
            )?;
            Ok(json!({"files": files}))
        }
        "registry.instance_items" => instance_items(args),
        _ => Err(format!("unknown registry tool: {name}")),
    }
}

fn publish(args: &Value) -> Result<Value, String> {
    let root = Path::new(text(args, "registry")?);
    let files_val = args
        .get("files")
        .and_then(Value::as_object)
        .ok_or("files must be an object of path to base64")?;
    let mut files = BTreeMap::new();
    for (path, value) in files_val {
        let b64 = value
            .as_str()
            .ok_or("each file value must be a base64 string")?;
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            b64.as_bytes(),
        )
        .map_err(|_| format!("{path} is not valid base64"))?;
        files.insert(path.clone(), bytes);
    }
    let tags: Vec<String> = args
        .get("tags")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let dest = publish_to_folder(
        root,
        PublishInput {
            id: text(args, "id")?.to_owned(),
            title: text(args, "title")?.to_owned(),
            version: text(args, "version")?.to_owned(),
            kind: args
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("part")
                .to_owned(),
            tags,
            files,
        },
    )?;
    Ok(json!({"path": dest.to_string_lossy()}))
}

fn instance_items(args: &Value) -> Result<Value, String> {
    let registry_root = Path::new(text(args, "path")?);
    let local = build_catalogue(registry_root)?.items;
    let feed_path = args
        .get("instance_feed")
        .and_then(Value::as_str)
        .map(Path::new)
        .ok_or("missing instance_feed")?;
    let feed = read_instance_feed(feed_path)?;
    let catalogue_path = feed_path
        .parent()
        .ok_or("instance feed has no parent folder")?
        .join(&feed.catalogue_path);
    let remote = catalogue_with_origin(&catalogue_path, &feed.instance_id, &feed.title)?;
    let merged = merge_catalogues(local, remote);
    Ok(json!({"items": merged}))
}
