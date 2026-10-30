use pcbcraft_publication::{
    bundle_root_hash, sha256_hex, Author, FileEntry, Identity, LayerLicences, ProvenanceBlock,
    PublicationManifest, VersionInfo, SCHEMA_VERSION,
};
use std::collections::BTreeMap;

pub fn sample_symbol_bytes() -> Vec<u8> {
    include_str!("../../library/tests/fixtures/device.kicad_sym").into()
}

pub fn manifest_for_files(files: &BTreeMap<String, Vec<u8>>) -> PublicationManifest {
    let entries: Vec<FileEntry> = files
        .iter()
        .map(|(path, data)| FileEntry {
            path: path.clone(),
            sha256: sha256_hex(data),
            size: data.len() as u64,
            media_type: if path.ends_with(".kicad_sym") {
                "text/x-kicad".into()
            } else {
                "application/json".into()
            },
            role: if path.ends_with(".kicad_sym") {
                "symbol".into()
            } else {
                "manifest".into()
            },
            format: if path.ends_with(".kicad_sym") {
                "kicad-symbol".into()
            } else {
                "json".into()
            },
            format_version: None,
        })
        .collect();
    let pairs: Vec<(String, String)> = entries
        .iter()
        .map(|f| (f.path.clone(), f.sha256.clone()))
        .collect();
    PublicationManifest {
        schema_version: SCHEMA_VERSION,
        identity: Identity {
            id: "demo-resistor".into(),
            name: "Demo resistor symbol".into(),
            authors: vec![Author {
                name: "Test Author".into(),
                contact: None,
            }],
            homepage: Some("https://example.com/demo".into()),
        },
        licences: LayerLicences {
            hardware: "CC-BY-4.0".into(),
            software: "MIT".into(),
            documentation: "CC-BY-4.0".into(),
            models_3d: "CC0-1.0".into(),
        },
        version: VersionInfo {
            version: "1.0.0".into(),
            previous_version_hash: None,
            changelog: String::new(),
        },
        derived_from: Vec::new(),
        tags: vec!["part".into(), "symbol".into()],
        roles: vec!["publisher".into()],
        bom_summary: None,
        compliance_claims: Vec::new(),
        previews: Vec::new(),
        files: entries,
        provenance: ProvenanceBlock {
            publisher: "Test Author".into(),
            tool: "pcbcraft-publication-tests".into(),
            tool_version: "0.0.0".into(),
            source_revision: None,
        },
        publisher_signature: None,
        bundle_root_hash: bundle_root_hash(&pairs),
    }
}

pub fn good_part_bundle_zip() -> Result<Vec<u8>, String> {
    let symbol = sample_symbol_bytes();
    let mut files = BTreeMap::new();
    files.insert("device.kicad_sym".into(), symbol);
    let manifest = manifest_for_files(&files);
    let manifest_json = manifest.to_json_pretty()?;
    files.insert("publication.json".into(), manifest_json.into_bytes());
    let entries: Vec<(String, Vec<u8>)> = files.into_iter().collect();
    pcbcraft_io::zip_entries(&entries)
}
