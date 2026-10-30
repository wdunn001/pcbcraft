use super::*;
use pcbcraft_library::{
    load_symbol_footprint_files, part_from_library_bundle, verify_part, FootprintSource,
    Model3dRef, MpnSource, PackageKey, Part, VerifyContext,
};
use serde::Deserialize;

pub(super) fn commands() -> Vec<CommandSpec> {
    [
        (
            "part.add",
            "Add a verified part record to the project library",
            schema(
                json!({"part":{"type":"object","description":"full part record without id; id is assigned"}}),
                &["part"],
            ),
        ),
        (
            "part.set",
            "Replace one project-library part by id",
            schema(json!({"part":{"type":"object"}}), &["part"]),
        ),
        (
            "part.remove",
            "Remove a project-library part; placed components keep their embedded snapshots",
            schema(json!({"id":id_schema()}), &["id"]),
        ),
        (
            "part.attach",
            "Link a placed component to a project-library part",
            schema(
                json!({"component":id_schema(),"part":id_schema()}),
                &["component", "part"],
            ),
        ),
        (
            "part.detach",
            "Clear the part link on a placed component",
            schema(json!({"component":id_schema()}), &["component"]),
        ),
        (
            "part.verify",
            "Run verification checks on a part and store the result on the record",
            schema(json!({"id":id_schema()}), &["id"]),
        ),
        (
            "part.import",
            "Import a part from symbol and footprint library files plus metadata",
            schema(
                json!({
                    "symbol_path":{"type":"string","maxLength":4096},
                    "symbol_name":{"type":"string","maxLength":128},
                    "footprint_path":{"type":"string","maxLength":4096},
                    "manufacturer":{"type":"string","maxLength":128},
                    "mpn":{"type":"string","maxLength":128},
                    "datasheet_url":{"type":"string","maxLength":4096},
                    "notice":{"type":"string","maxLength":4096}
                }),
                &["symbol_path", "symbol_name", "footprint_path", "mpn"],
            ),
        ),
        (
            "part.import_user_file",
            "Record a user-provided vendor model or metadata file without network access",
            schema(
                json!({
                    "path":{"type":"string","maxLength":4096},
                    "manufacturer":{"type":"string","maxLength":128},
                    "mpn":{"type":"string","maxLength":128},
                    "symbol":{"type":"string","maxLength":128},
                    "footprint":{"type":"string","maxLength":128}
                }),
                &["path", "mpn"],
            ),
        ),
        (
            "part.fetch_datasheet",
            "Download a datasheet to the user cache and record its hash on the part (native builds only)",
            schema(
                json!({
                    "id":id_schema(),
                    "url":{"type":"string","maxLength":4096},
                    "distributor":{"type":"string","maxLength":48}
                }),
                &["id", "url"],
            ),
        ),
        (
            "part.set_distributor_credential",
            "Store a distributor API token in the native keychain (not in the project file; native builds only)",
            schema(
                json!({
                    "distributor":{"type":"string","maxLength":48},
                    "secret":{"type":"string","maxLength":4096}
                }),
                &["distributor", "secret"],
            ),
        ),
        (
            "part.clear_distributor_credential",
            "Remove a stored distributor API token from the native keychain (native builds only)",
            schema(
                json!({"distributor":{"type":"string","maxLength":48}}),
                &["distributor"],
            ),
        ),
    ]
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
struct PartPayload {
    part: Part,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PartId {
    id: Id,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachParams {
    component: Id,
    part: Id,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentOnly {
    component: Id,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportParams {
    symbol_path: String,
    symbol_name: String,
    footprint_path: String,
    #[serde(default)]
    manufacturer: String,
    mpn: String,
    #[serde(default)]
    datasheet_url: String,
    #[serde(default)]
    notice: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UserImportParams {
    path: String,
    #[serde(default)]
    manufacturer: String,
    mpn: String,
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    footprint: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FetchDatasheetParams {
    id: Id,
    url: String,
    #[serde(default)]
    distributor: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DistributorCredentialParams {
    distributor: String,
    secret: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DistributorOnlyParams {
    distributor: String,
}

pub(super) fn run(
    p: &mut Project,
    next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    if !commands().iter().any(|c| c.id == command) {
        return None;
    }
    Some((|| match command {
        "part.add" => part_add(p, next, params),
        "part.set" => part_set(p, params),
        "part.remove" => part_remove(p, params),
        "part.attach" => part_attach(p, params),
        "part.detach" => part_detach(p, params),
        "part.verify" => part_verify_cmd(p, params),
        "part.import" => part_import(p, next, params),
        "part.import_user_file" => part_import_user(p, next, params),
        "part.fetch_datasheet" => part_fetch_datasheet(p, params),
        "part.set_distributor_credential" => part_set_distributor_credential(params),
        "part.clear_distributor_credential" => part_clear_distributor_credential(params),
        _ => Err(Error::Invalid("unknown part command".into())),
    })())
}

fn part_add(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, Error> {
    let mut a: PartPayload = parse(params)?;
    if a.part.id != 0 {
        return Err(Error::Invalid("part.add assigns the id; leave id at zero".into()));
    }
    if p.library.parts.len() >= pcbcraft_library::MAX_PARTS {
        return Err(Error::Invalid("project library part limit reached".into()));
    }
    a.part.id = allocate(next)?;
    a.part.validate().map_err(|e| Error::Invalid(e.to_string()))?;
    if p.library.parts.iter().any(|x| x.mpn == a.part.mpn) {
        return Err(Error::Invalid("duplicate part MPN".into()));
    }
    let id = a.part.id;
    p.library.parts.push(a.part);
    p.library.parts.sort_by(|a, b| a.mpn.cmp(&b.mpn));
    Ok(json!({"id": id}))
}

fn part_set(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: PartPayload = parse(params)?;
    if a.part.id == 0 {
        return Err(Error::Invalid("part.set requires a non-zero id".into()));
    }
    a.part.validate().map_err(|e| Error::Invalid(e.to_string()))?;
    let slot = p
        .library
        .parts
        .iter_mut()
        .find(|x| x.id == a.part.id)
        .ok_or_else(|| Error::Invalid("part not found".into()))?;
    *slot = a.part;
    Ok(json!({"id": slot.id}))
}

fn part_remove(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: PartId = parse(params)?;
    let before = p.library.parts.len();
    p.library.parts.retain(|x| x.id != a.id);
    if before == p.library.parts.len() {
        return Err(Error::Invalid("part not found".into()));
    }
    for c in &mut p.components {
        if c.part == Some(a.id) {
            c.part = None;
        }
    }
    Ok(json!({"removed": a.id}))
}

fn part_attach(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: AttachParams = parse(params)?;
    if !p.library.parts.iter().any(|x| x.id == a.part) {
        return Err(Error::Invalid("part not found".into()));
    }
    let c = p
        .components
        .iter_mut()
        .find(|c| c.id == a.component)
        .ok_or_else(|| Error::Invalid("component not found".into()))?;
    c.part = Some(a.part);
    Ok(json!({"component": a.component, "part": a.part}))
}

fn part_detach(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: ComponentOnly = parse(params)?;
    let c = p
        .components
        .iter_mut()
        .find(|c| c.id == a.component)
        .ok_or_else(|| Error::Invalid("component not found".into()))?;
    c.part = None;
    Ok(json!({"component": a.component}))
}

fn part_verify_cmd(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: PartId = parse(params)?;
    let symbol_name;
    let footprint_name;
    {
        let part = p
            .library
            .parts
            .iter()
            .find(|x| x.id == a.id)
            .ok_or_else(|| Error::Invalid("part not found".into()))?;
        symbol_name = part.symbol.clone();
        footprint_name = part.footprint.clone();
    }
    let symbol = p.library.symbols.iter().find(|s| s.name == symbol_name);
    let footprint = p
        .library
        .footprints
        .iter()
        .find(|f| f.name == footprint_name);
    let ctx = VerifyContext { symbol, footprint };
    let (verification, messages) = {
        let part = p
            .library
            .parts
            .iter()
            .find(|x| x.id == a.id)
            .ok_or_else(|| Error::Invalid("part not found".into()))?;
        verify_part(part, ctx)
    };
    if let Some(part) = p.library.parts.iter_mut().find(|x| x.id == a.id) {
        part.verification = verification.clone();
    }
    Ok(json!({"verification": verification, "messages": messages}))
}

fn part_import(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, Error> {
    let a: ImportParams = parse(params)?;
    if a.symbol_path.len() > 4096 || a.footprint_path.len() > 4096 {
        return Err(Error::Invalid("path too long".into()));
    }
    let (symbol, footprint) = load_symbol_footprint_files(
        &a.symbol_path,
        &a.symbol_name,
        &a.footprint_path,
    )
    .map_err(|e| Error::Invalid(e))?;
    symbol.validate().map_err(|e| Error::Invalid(e.to_string()))?;
    footprint.validate().map_err(|e| Error::Invalid(e.to_string()))?;
    upsert_symbol_footprint(p, &symbol, &footprint, a.notice.as_deref());
    let id = allocate(next)?;
    let mut attribution = vec![];
    if let Some(n) = &a.notice {
        attribution.push(n.clone());
    }
    let (mut part, warnings) = part_from_library_bundle(
        id,
        symbol,
        footprint,
        &a.manufacturer,
        &a.mpn,
        &a.datasheet_url,
        attribution,
    )
    .map_err(|e| Error::Invalid(e))?;
    if p.library.parts.iter().any(|x| x.mpn == part.mpn) {
        return Err(Error::Invalid("duplicate part MPN".into()));
    }
    part.import_warnings.extend(warnings);
    let part_id = part.id;
    p.library.parts.push(part);
    p.library.parts.sort_by(|a, b| a.mpn.cmp(&b.mpn));
    Ok(json!({"id": part_id}))
}

fn part_import_user(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, Error> {
    let a: UserImportParams = parse(params)?;
    let file_name = std::path::Path::new(&a.path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("import")
        .to_owned();
    let id = allocate(next)?;
    let key = PackageKey {
        family: pcbcraft_library::PackageFamily::Unknown,
        pin_count: 1,
        pitch_mm: 1.0,
        body_length_mm: 1.0,
        body_width_mm: 1.0,
        lead_span_mm: 1.0,
        exposed_pad_mm: 0.0,
        height_mm: 0.0,
        courtyard_half_x: None,
        courtyard_half_y: None,
    };
    let part = Part {
        id,
        manufacturer: a.manufacturer,
        mpn: a.mpn.clone(),
        base_mpn: a.mpn,
        orderables: vec![],
        family: None,
        category: String::new(),
        lifecycle: String::new(),
        msl: None,
        datasheet: None,
        symbol: if a.symbol.is_empty() {
            "USER".into()
        } else {
            a.symbol
        },
        footprint: if a.footprint.is_empty() {
            "USER".into()
        } else {
            a.footprint
        },
        package_key: key,
        footprint_source: FootprintSource::UserImported,
        model3d: Some(Model3dRef::Wrl {
            path: a.path,
            offset: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            rotate: [0.0, 0.0, 0.0],
        }),
        pins: vec![],
        limits: Default::default(),
        requirements: vec![],
        layout_notes: vec![format!("imported from {file_name}")],
        verification: Default::default(),
        mpn_source: MpnSource::UserImported,
        attribution: vec![],
        import_warnings: vec![],
    };
    part.validate().map_err(|e| Error::Invalid(e.to_string()))?;
    if p.library.parts.iter().any(|x| x.mpn == part.mpn) {
        return Err(Error::Invalid("duplicate part MPN".into()));
    }
    let part_id = part.id;
    p.library.parts.push(part);
    Ok(json!({"id": part_id}))
}

fn part_fetch_datasheet(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: FetchDatasheetParams = parse(params)?;
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (p, a);
        return Err(Error::Invalid(
            "datasheet download is not available on wasm builds".into(),
        ));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        bounded("distributor", &a.distributor, 48)?;
        let headers = pcbcraft_io::auth_headers_for_distributor(&a.distributor)
            .map_err(Error::Invalid)?;
        let hash = pcbcraft_io::fetch_datasheet_cache(&a.url, &headers)
            .map_err(Error::Invalid)?;
        let part = p
            .library
            .parts
            .iter_mut()
            .find(|x| x.id == a.id)
            .ok_or_else(|| Error::Invalid("part not found".into()))?;
        let ds = part
            .datasheet
            .get_or_insert(pcbcraft_library::DatasheetRef {
                url: a.url.clone(),
                sha256: String::new(),
                retrieved_at: String::new(),
                checked_at: String::new(),
                local_path: None,
            });
        ds.url = a.url;
        ds.sha256 = hash.sha256;
        ds.local_path = Some(hash.path);
        ds.retrieved_at = hash.retrieved_at;
        ds.checked_at = hash.retrieved_at;
        Ok(json!({"sha256": hash.sha256, "path": hash.path}))
    }
}

fn part_set_distributor_credential(params: &Value) -> Result<Value, Error> {
    let a: DistributorCredentialParams = parse(params)?;
    bounded("distributor", &a.distributor, 48)?;
    bounded("secret", &a.secret, 4096)?;
    #[cfg(target_arch = "wasm32")]
    {
        let _ = a;
        return Err(Error::Invalid(
            "distributor credentials are not available on wasm builds".into(),
        ));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        pcbcraft_io::set_distributor_credential(&a.distributor, &a.secret)
            .map_err(Error::Invalid)?;
        Ok(json!({}))
    }
}

fn part_clear_distributor_credential(params: &Value) -> Result<Value, Error> {
    let a: DistributorOnlyParams = parse(params)?;
    bounded("distributor", &a.distributor, 48)?;
    #[cfg(target_arch = "wasm32")]
    {
        let _ = a;
        return Err(Error::Invalid(
            "distributor credentials are not available on wasm builds".into(),
        ));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        pcbcraft_io::clear_distributor_credential(&a.distributor)
            .map_err(Error::Invalid)?;
        Ok(json!({}))
    }
}

fn upsert_symbol_footprint(
    p: &mut Project,
    symbol: &pcbcraft_library::SymbolDefinition,
    footprint: &pcbcraft_library::FootprintDefinition,
    notice: Option<&str>,
) {
    if let Some(n) = notice {
        if !p.library.notices.contains(&n.to_string()) {
            p.library.notices.push(n.to_string());
        }
    }
    if let Some(def) = p.library.symbols.iter_mut().find(|s| s.name == symbol.name) {
        *def = symbol.clone();
    } else {
        p.library.symbols.push(symbol.clone());
    }
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
    p.library.symbols.sort_by(|a, b| a.name.cmp(&b.name));
    p.library.footprints.sort_by(|a, b| a.name.cmp(&b.name));
}
