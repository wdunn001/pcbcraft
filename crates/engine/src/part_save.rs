//! Save project parts to the project library or to registered folder libraries.
use super::{Error, Project, parse, schema, CommandSpec};
use pcbcraft_library::{
    FootprintDefinition, PartAlternate, PartProvenance, PartSidecar, PartSidecarField,
    PART_SIDECAR_FORMAT, PART_SIDECAR_VERSION, sidecar_path_in_folder, write_part_sidecar,
    SymbolDefinition,
};
use pcbcraft_doc::ComponentField;
use serde::Deserialize;
use serde_json::{json, Value};

pub(super) fn commands() -> Vec<CommandSpec> {
    [(
        "part.save",
        "Save a placed part or a project-library symbol to the project library or to the registered personal or shared folder, writing symbol and footprint library text plus a sidecar for parameters and alternates",
        schema(
            json!({
                "target":{"type":"string","enum":["project","personal","shared"],"default":"project","description":"project keeps the part in the open design; personal or shared writes files under the folder marked with that role"},
                "id":{"type":"integer","minimum":1,"description":"a placed component to save"},
                "symbol":{"type":"string","description":"a project-library symbol name when no component id is given"},
                "alternates":{"type":"array","maxItems":8,"items":{"type":"object","properties":{"manufacturer":{"type":"string"},"mpn":{"type":"string"},"datasheet":{"type":"string"},"distributor":{"type":"string"},"sku":{"type":"string"}},"required":["manufacturer","mpn"],"additionalProperties":false}},
                "provenance_notice":{"type":"string","description":"optional provenance line kept in the sidecar when copying out of a project"}
            }),
            &[],
        ),
    )]
    .into_iter()
    .map(|(id, description, input_schema)| CommandSpec {
        id,
        description,
        input_schema,
    })
    .collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PartSaveParams {
    #[serde(default = "default_target")]
    target: String,
    id: Option<u64>,
    symbol: Option<String>,
    alternates: Option<Vec<PartAlternate>>,
    provenance_notice: Option<String>,
}

fn default_target() -> String {
    "project".into()
}

struct ResolvedPart {
    symbol: SymbolDefinition,
    footprint: Option<FootprintDefinition>,
    value: String,
    kind: String,
    fields: Vec<ComponentField>,
}

pub(super) fn run(p: &mut Project, command: &str, params: &Value) -> Option<Result<Value, Error>> {
    if command != "part.save" {
        return None;
    }
    Some(part_save(p, params))
}

fn part_save(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: PartSaveParams = parse(params)?;
    if a.target != "project" && a.target != "personal" && a.target != "shared" {
        return Err(Error::Invalid(
            "target must be project, personal or shared".into(),
        ));
    }
    if a.id.is_some() == a.symbol.is_some() {
        return Err(Error::Invalid(
            "give either a component id or a project-library symbol name".into(),
        ));
    }
    let resolved = if let Some(id) = a.id {
        resolve_from_component(p, id)?
    } else {
        resolve_from_library(p, a.symbol.as_deref().unwrap_or(""))?
    };
    let mut alternates = a.alternates.unwrap_or_default();
    for alt in &alternates {
        if alt.manufacturer.trim().is_empty() || alt.mpn.trim().is_empty() {
            return Err(Error::Invalid(
                "every alternate needs a manufacturer and mpn".into(),
            ));
        }
    }
    let provenance = build_provenance(p, &a, &resolved)?;
    let sidecar = PartSidecar {
        format: PART_SIDECAR_FORMAT.into(),
        version: PART_SIDECAR_VERSION,
        symbol: resolved.symbol.name.clone(),
        footprint: resolved
            .footprint
            .as_ref()
            .map(|f| f.name.clone())
            .unwrap_or_else(|| resolved.symbol.footprint.clone()),
        value: resolved.value.clone(),
        kind: resolved.kind.clone(),
        fields: resolved
            .fields
            .iter()
            .map(|f| PartSidecarField {
                key: f.key.clone(),
                value: f.value.clone(),
            })
            .collect(),
        alternates,
        provenance,
    };
    sidecar.validate().map_err(Error::Invalid)?;
    match a.target.as_str() {
        "project" => save_to_project(p, &resolved, sidecar),
        "personal" | "shared" => save_to_folder(p, &resolved, sidecar, &a.target),
        _ => unreachable!(),
    }
}

fn build_provenance(
    p: &Project,
    a: &PartSaveParams,
    resolved: &ResolvedPart,
) -> Result<Option<PartProvenance>, Error> {
    if a.target == "project" && a.id.is_none() {
        return Ok(None);
    }
    let notice = a
        .provenance_notice
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(str::trim)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if a.id.is_some() {
                format!(
                    "Copied from project {} as {} with parameters and alternates in the sidecar.",
                    p.name, resolved.symbol.name
                )
            } else {
                String::new()
            }
        });
    if notice.is_empty() {
        return Ok(None);
    }
    if notice.len() > 8 * 1024 {
        return Err(Error::Invalid("provenance notice is too long".into()));
    }
    Ok(Some(PartProvenance {
        notice,
        source_project: if a.id.is_some() {
            p.name.clone()
        } else {
            String::new()
        },
    }))
}

fn resolve_from_component(p: &Project, id: u64) -> Result<ResolvedPart, Error> {
    let component = p
        .components
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| Error::Invalid("component not found".into()))?;
    let symbol = component
        .library_symbol
        .as_ref()
        .map(|s| s.definition.clone())
        .filter(|s| !s.name.is_empty())
        .ok_or_else(|| {
            Error::Invalid(
                "save a library part or adopt its symbol into the project library first".into(),
            )
        })?;
    let footprint = component
        .library_footprint
        .as_ref()
        .map(|f| f.definition.clone())
        .or_else(|| footprint_from_project(p, &symbol));
    Ok(ResolvedPart {
        symbol,
        footprint,
        value: component.value.clone(),
        kind: component.kind.clone(),
        fields: component.fields.clone(),
    })
}

fn resolve_from_library(p: &Project, name: &str) -> Result<ResolvedPart, Error> {
    if name.is_empty() || name.len() > 128 {
        return Err(Error::Invalid("symbol name is out of range".into()));
    }
    let symbol = p
        .library
        .symbols
        .iter()
        .find(|s| s.name == name)
        .cloned()
        .ok_or_else(|| Error::Invalid("symbol is not in the project library".into()))?;
    let footprint = footprint_from_project(p, &symbol);
    let existing = p.library.parts.iter().find(|part| part.symbol == name);
    Ok(ResolvedPart {
        symbol,
        footprint,
        value: existing
            .and_then(|p| (!p.value.is_empty()).then_some(p.value.clone()))
            .unwrap_or_default(),
        kind: existing
            .and_then(|p| (!p.kind.is_empty()).then_some(p.kind.clone()))
            .unwrap_or_else(|| "library".into()),
        fields: existing
            .map(|p| {
                p.fields
                    .iter()
                    .map(|f| ComponentField {
                        key: f.key.clone(),
                        value: f.value.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn footprint_from_project(p: &Project, symbol: &SymbolDefinition) -> Option<FootprintDefinition> {
    if symbol.footprint.is_empty() {
        return None;
    }
    p.library
        .footprints
        .iter()
        .find(|f| f.name == symbol.footprint)
        .cloned()
}

fn save_to_project(
    p: &mut Project,
    resolved: &ResolvedPart,
    sidecar: PartSidecar,
) -> Result<Value, Error> {
    p.library.validate().map_err(Error::Invalid)?;
    upsert_symbol(p, &resolved.symbol);
    if let Some(footprint) = &resolved.footprint {
        upsert_footprint(p, footprint);
    }
    if let Some(existing) = p
        .library
        .parts
        .iter_mut()
        .find(|part| part.symbol == sidecar.symbol)
    {
        *existing = sidecar;
    } else {
        p.library.parts.push(sidecar.clone());
    }
    p.library.parts.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    p.library.validate().map_err(Error::Invalid)?;
    Ok(json!({
        "target": "project",
        "symbol": sidecar.symbol,
        "footprint": sidecar.footprint,
        "fields": sidecar.fields.len(),
        "alternates": sidecar.alternates.len(),
    }))
}

fn upsert_symbol(p: &mut Project, symbol: &SymbolDefinition) {
    if let Some(def) = p
        .library
        .symbols
        .iter_mut()
        .find(|s| s.name == symbol.name)
    {
        *def = symbol.clone();
    } else {
        p.library.symbols.push(symbol.clone());
    }
    p.library.symbols.sort_by(|a, b| a.name.cmp(&b.name));
}

fn upsert_footprint(p: &mut Project, footprint: &FootprintDefinition) {
    if let Some(def) = p
        .library
        .footprints
        .iter_mut()
        .find(|f| f.name == footprint.name)
    {
        *def = footprint.clone();
    } else {
        p.library.footprints.push(footprint.clone());
    }
    p.library.footprints.sort_by(|a, b| a.name.cmp(&b.name));
}

fn save_to_folder(
    p: &Project,
    resolved: &ResolvedPart,
    sidecar: PartSidecar,
    target: &str,
) -> Result<Value, Error> {
    let folder = crate::library::folder_for_role(target).map_err(Error::Invalid)?;
    let symbol_path = folder.join(format!("{}.kicad_sym", resolved.symbol.name));
    let symbol_text =
        pcbcraft_io::symbol_library_text(&resolved.symbol).map_err(Error::Invalid)?;
    pcbcraft_io::atomic_write(&symbol_path, symbol_text.as_bytes()).map_err(Error::Invalid)?;
    let mut footprint_path = None;
    if let Some(footprint) = &resolved.footprint {
        let path = folder.join(format!("{}.kicad_mod", footprint.name));
        let text = pcbcraft_io::footprint_library_text(footprint).map_err(Error::Invalid)?;
        pcbcraft_io::atomic_write(&path, text.as_bytes()).map_err(Error::Invalid)?;
        footprint_path = Some(path);
    }
    let sidecar_path = sidecar_path_in_folder(&folder, &resolved.symbol.name);
    write_part_sidecar(&sidecar_path, &sidecar).map_err(Error::Invalid)?;
    Ok(json!({
        "target": target,
        "folder": folder.to_string_lossy(),
        "symbol_file": symbol_path.to_string_lossy(),
        "footprint_file": footprint_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        "sidecar_file": sidecar_path.to_string_lossy(),
        "provenance": sidecar.provenance.as_ref().map(|p| p.notice.clone()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{add_folder, set_source_role, SOURCES_PATH_TEST, SOURCES_TEST_LOCK};
    use crate::Session;
    use pcbcraft_library::{SymbolDefinition, SymbolPin};
    use serde_json::json;
    use std::sync::Mutex;

    struct TestDirectory {
        _guard: std::sync::MutexGuard<'static, ()>,
        path: std::path::PathBuf,
    }

    impl TestDirectory {
        fn new() -> Result<Self, String> {
            let guard = SOURCES_TEST_LOCK
                .lock()
                .map_err(|_| "test lock unavailable")?;
            let path = std::env::temp_dir().join(format!(
                "pcbcraft-part-save-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            SOURCES_PATH_TEST
                .lock()
                .map_err(|_| "test lock unavailable")?
                .replace(path.join("sources.json"));
            Ok(Self {
                _guard: guard,
                path,
            })
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            if let Ok(mut slot) = SOURCES_PATH_TEST.lock() {
                slot.take();
            }
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn sample_symbol() -> SymbolDefinition {
        SymbolDefinition {
            name: "SaveMe".into(),
            reference: "U".into(),
            value: String::new(),
            footprint: "SaveMeFP".into(),
            description: "test".into(),
            keywords: String::new(),
            set: String::new(),
            pins: vec![SymbolPin {
                number: "1".into(),
                name: "~".into(),
                position: pcbcraft_geom::Point::new(0.0, 0.0),
                angle: 0.0,
                length: 2.5,
                electrical_type: "passive".into(),
                unit: 1,
                body_style: 1,
                graphical_style: "line".into(),
                visible: true,
            }],
            graphics: vec![],
            extends: None,
            raw_source: String::new(),
            unsupported: vec![],
        }
    }

    #[test]
    fn part_saved_to_personal_folder_reloads_with_fields_and_alternates() -> Result<(), Error> {
        let _dir = TestDirectory::new().map_err(Error::Invalid)?;
        let personal = _dir.path.join("my-parts");
        std::fs::create_dir_all(&personal).map_err(Error::Invalid)?;
        add_folder(
            personal
                .to_str()
                .ok_or_else(|| Error::Invalid("path".into()))?,
            Some("My parts"),
        )
        .map_err(Error::Invalid)?;
        set_source_role(
            personal
                .to_str()
                .ok_or_else(|| Error::Invalid("path".into()))?,
            Some("personal"),
        )
        .map_err(Error::Invalid)?;

        let mut s = Session::default();
        let symbol = sample_symbol();
        s.execute(
            "library.symbol.upsert",
            json!({"definition": symbol}),
            None,
        )?;
        let footprint = pcbcraft_library::read_footprint(
            "(footprint SaveMeFP (layer F.Cu) (pad 1 smd rect (at 0 0) (size 1 1) (layers F.Cu)))",
        )
        .map_err(Error::Invalid)?;
        s.execute(
            "library.footprint.upsert",
            json!({"definition": footprint}),
            None,
        )?;
        s.execute(
            "component.place_custom",
            json!({"name":"SaveMe","position":{"x":10,"y":10},"reference":"U1","value":"TEST"}),
            None,
        )?;
        let id = s.project.components[0].id;
        s.execute(
            "footprint.assign_custom",
            json!({"id": id, "name":"SaveMeFP"}),
            None,
        )?;
        s.execute(
            "component.field",
            json!({"id": id, "key":"Manufacturer", "value":"Acme"}),
            None,
        )?;
        s.execute(
            "part.save",
            json!({
                "target":"personal",
                "id": id,
                "alternates":[{"manufacturer":"Other","mpn":"ALT-1","datasheet":"https://example.test/ds"}]
            }),
            None,
        )?;

        let sidecar_path = sidecar_path_in_folder(&personal, "SaveMe");
        let loaded = pcbcraft_library::read_part_sidecar(&sidecar_path).map_err(Error::Invalid)?;
        assert_eq!(loaded.fields.len(), 1);
        assert_eq!(loaded.fields[0].key, "Manufacturer");
        assert_eq!(loaded.alternates.len(), 1);
        assert!(
            loaded.provenance.as_ref().is_some_and(|p| !p.notice.is_empty()),
            "copy out keeps a provenance notice"
        );
        Ok(())
    }

    #[test]
    fn part_save_to_project_library_keeps_sidecar_in_the_design() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "library.symbol.upsert",
            json!({"definition": sample_symbol()}),
            None,
        )?;
        s.execute(
            "part.save",
            json!({"target":"project","symbol":"SaveMe","alternates":[{"manufacturer":"AltCo","mpn":"1"}]}),
            None,
        )?;
        assert_eq!(s.project.library.parts.len(), 1);
        assert_eq!(s.project.library.parts[0].alternates.len(), 1);
        Ok(())
    }
}
