use crate::part_types::{
    FootprintSource, Model3dRef, MpnSource, Part, PinSpec,
};
use crate::{FootprintDefinition, ParamBody, SymbolDefinition, pin_kind_from_library_string};
use crate::{PackageKey, read_footprint, read_symbols};

pub fn part_from_library_bundle(
    id: u64,
    symbol: SymbolDefinition,
    footprint: FootprintDefinition,
    manufacturer: &str,
    mpn: &str,
    datasheet_url: &str,
    attribution: Vec<String>,
) -> Result<(Part, Vec<String>), String> {
    let package_key = PackageKey::from_footprint(&footprint)?;
    let mut warnings = Vec::new();
    let mut pins = Vec::new();
    for sp in &symbol.pins {
        let (kind, unknown) = pin_kind_from_library_string(&sp.electrical_type);
        if unknown {
            warnings.push(format!(
                "pin {} type {:?} mapped to passive",
                sp.number, sp.electrical_type
            ));
        }
        pins.push(PinSpec {
            number: sp.number.clone(),
            name: sp.name.clone(),
            pads: vec![sp.number.clone()],
            kind,
            library_type: sp.electrical_type.clone(),
            unit: None,
        });
    }
    let model3d = footprint.model.as_ref().map(|m| Model3dRef::Wrl {
        path: m.path.clone(),
        offset: m.offset,
        scale: m.scale,
        rotate: m.rotate,
    }).or_else(|| {
        Some(Model3dRef::Parametric {
            body: ParamBody::from_key(&package_key, &symbol.description),
        })
    });
    let part = Part {
        id,
        manufacturer: manufacturer.to_owned(),
        mpn: mpn.to_owned(),
        base_mpn: mpn.to_owned(),
        orderables: vec![],
        family: None,
        category: symbol.keywords.clone(),
        lifecycle: String::new(),
        msl: None,
        datasheet: if datasheet_url.is_empty() {
            None
        } else {
            Some(crate::part_types::DatasheetRef {
                url: datasheet_url.to_owned(),
                sha256: String::new(),
                retrieved_at: String::new(),
                checked_at: String::new(),
                local_path: None,
            })
        },
        symbol: symbol.name.clone(),
        footprint: footprint.name.clone(),
        package_key,
        footprint_source: FootprintSource::Library,
        model3d,
        pins,
        limits: Default::default(),
        requirements: vec![],
        layout_notes: vec![],
        verification: Default::default(),
        mpn_source: MpnSource::Library,
        attribution,
        import_warnings: warnings.clone(),
    };
    part.validate().map_err(|e| e.to_string())?;
    Ok((part, warnings))
}

/// Bounded JSON reader for distributor-style part records.
pub fn read_part_json(text: &str) -> Result<PartJson, String> {
    if text.len() > 512 * 1024 {
        return Err("part JSON exceeds 512 KiB".into());
    }
    serde_json::from_str(text).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartJson {
    pub manufacturer: String,
    pub mpn: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub datasheet: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub footprint: String,
}

pub fn load_symbol_footprint_files(
    symbol_path: &str,
    symbol_name: &str,
    footprint_path: &str,
) -> Result<(SymbolDefinition, FootprintDefinition), String> {
    let sym_text = std::fs::read_to_string(symbol_path)
        .map_err(|e| format!("{symbol_path}: {e}"))?;
    let symbols = read_symbols(&sym_text).map_err(|e| e.to_string())?;
    let symbol = symbols
        .into_iter()
        .find(|s| s.name == symbol_name)
        .ok_or_else(|| format!("symbol {symbol_name} not found"))?;
    let fp_text = std::fs::read_to_string(footprint_path)
        .map_err(|e| format!("{footprint_path}: {e}"))?;
    let footprint = read_footprint(&fp_text).map_err(|e| e.to_string())?;
    Ok((symbol, footprint))
}
