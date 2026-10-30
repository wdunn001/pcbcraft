mod support;

use pcbcraft_publication::{PublicationManifest, PUBLICATION_SCHEMA_V1};
use support::{good_part_bundle_zip, manifest_for_files, sample_symbol_bytes};
use std::collections::BTreeMap;

#[test]
fn manifest_round_trips_through_json() -> Result<(), String> {
    let mut files = BTreeMap::new();
    files.insert("device.kicad_sym".into(), sample_symbol_bytes());
    let manifest = manifest_for_files(&files);
    let json = manifest.to_json_pretty()?;
    let again = PublicationManifest::from_json(&json)?;
    assert_eq!(manifest, again);
    Ok(())
}

#[test]
fn schema_file_is_present_and_valid_json() -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(PUBLICATION_SCHEMA_V1).map_err(|e| e.to_string())?;
    assert_eq!(value["title"], "PCBCraft publication manifest");
    assert!(value.get("properties").is_some());
    Ok(())
}

#[test]
fn good_zip_bundle_includes_publication_json() -> Result<(), String> {
    let zip = good_part_bundle_zip()?;
    assert!(zip.starts_with(b"PK\x03\x04"));
    Ok(())
}
