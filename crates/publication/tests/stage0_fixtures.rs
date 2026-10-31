mod support;

use pcbcraft_publication::{stage0_from_manifest_and_files, stage0_from_zip, CheckVerdict};
use support::{good_part_bundle_zip, manifest_for_files, sample_symbol_bytes};
use std::collections::BTreeMap;

#[test]
fn good_part_bundle_passes_stage0() -> Result<(), String> {
    let zip = good_part_bundle_zip()?;
    let report = stage0_from_zip(&zip);
    assert!(
        report.passes(),
        "expected pass, got {:?}",
        report.failure_reasons()
    );
    Ok(())
}

#[test]
fn bad_hash_fails_files_rule() -> Result<(), String> {
    let mut files = BTreeMap::new();
    files.insert("device.kicad_sym".into(), sample_symbol_bytes());
    let mut manifest = manifest_for_files(&files);
    manifest.files[0].sha256 = "0".repeat(64);
    let json = manifest.to_json_pretty()?;
    files.insert("publication.json".into(), json.into_bytes());
    let report = stage0_from_manifest_and_files(
        std::str::from_utf8(files.get("publication.json").ok_or("missing")?).map_err(|_| "utf8")?,
        &files,
    );
    assert!(!report.passes());
    assert!(report
        .failure_reasons()
        .iter()
        .any(|line| line.contains("files")));
    Ok(())
}

#[test]
fn traversal_path_fails_paths_rule() -> Result<(), String> {
    let mut files = BTreeMap::new();
    files.insert("../evil.kicad_sym".into(), sample_symbol_bytes());
    let mut manifest = manifest_for_files(&files);
    manifest.files[0].path = "../evil.kicad_sym".into();
    let json = manifest.to_json_pretty()?;
    files.insert("publication.json".into(), json.into_bytes());
    let report = stage0_from_manifest_and_files(
        std::str::from_utf8(files.get("publication.json").ok_or("missing")?).map_err(|_| "utf8")?,
        &files,
    );
    assert!(!report.passes());
    assert!(report
        .failure_reasons()
        .iter()
        .any(|line| line.contains("paths")));
    Ok(())
}

#[test]
fn disallowed_licence_fails_licence_rule() -> Result<(), String> {
    let mut files = BTreeMap::new();
    files.insert("device.kicad_sym".into(), sample_symbol_bytes());
    let mut manifest = manifest_for_files(&files);
    manifest.licences.hardware = "Proprietary".into();
    let json = manifest.to_json_pretty()?;
    files.insert("publication.json".into(), json.into_bytes());
    let report = stage0_from_manifest_and_files(
        std::str::from_utf8(files.get("publication.json").ok_or("missing")?).map_err(|_| "utf8")?,
        &files,
    );
    assert!(!report.passes());
    assert!(report
        .failure_reasons()
        .iter()
        .any(|line| line.contains("licence")));
    Ok(())
}

#[test]
fn incompatible_lineage_fails_lineage_rule() -> Result<(), String> {
    let mut files = BTreeMap::new();
    files.insert("device.kicad_sym".into(), sample_symbol_bytes());
    let mut manifest = manifest_for_files(&files);
    manifest.derived_from.push(pcbcraft_publication::LineageEntry {
        parent_id: "upstream".into(),
        relation: pcbcraft_publication::LineageRelation::Fork,
        parent_licence: "GPL-3.0-only".into(),
    });
    manifest.licences.software = "MIT".into();
    let json = manifest.to_json_pretty()?;
    files.insert("publication.json".into(), json.into_bytes());
    let report = stage0_from_manifest_and_files(
        std::str::from_utf8(files.get("publication.json").ok_or("missing")?).map_err(|_| "utf8")?,
        &files,
    );
    assert!(!report.passes());
    assert!(report
        .failure_reasons()
        .iter()
        .any(|line| line.contains("lineage")));
    Ok(())
}

#[test]
fn http_homepage_fails_urls_rule() -> Result<(), String> {
    let mut files = BTreeMap::new();
    files.insert("device.kicad_sym".into(), sample_symbol_bytes());
    let mut manifest = manifest_for_files(&files);
    manifest.identity.homepage = Some("http://example.com/not-secure".into());
    let json = manifest.to_json_pretty()?;
    files.insert("publication.json".into(), json.into_bytes());
    let report = stage0_from_manifest_and_files(
        std::str::from_utf8(files.get("publication.json").ok_or("missing")?).map_err(|_| "utf8")?,
        &files,
    );
    assert!(!report.passes());
    assert!(report
        .failure_reasons()
        .iter()
        .any(|line| line.contains("urls")));
    Ok(())
}

#[test]
fn zip_bomb_entry_cap_flags_archive_limits() -> Result<(), String> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..600usize {
        entries.push((format!("f{i}.txt"), vec![b'x'; 16]));
    }
    entries.push(("publication.json".into(), br#"{"schema_version":1}"#.to_vec()));
    let zip = pcbcraft_io::zip_entries(&entries)?;
    let report = stage0_from_zip(&zip);
    assert!(!report.passes());
    Ok(())
}

#[test]
fn huge_preview_media_mismatch_fails_media_type() -> Result<(), String> {
    let mut files = BTreeMap::new();
    let png_header = vec![0u8; 4096];
    files.insert("preview.png".into(), png_header);
    let mut manifest = manifest_for_files(&files);
    manifest.files[0].path = "preview.png".into();
    manifest.files[0].media_type = "application/json".into();
    manifest.files[0].role = "preview".into();
    manifest.files[0].format = "png".into();
    manifest.files[0].sha256 = pcbcraft_publication::sha256_hex(files.get("preview.png").ok_or("missing")?);
    manifest.files[0].size = files["preview.png"].len() as u64;
    let pairs: Vec<(String, String)> = manifest
        .files
        .iter()
        .map(|f| (f.path.clone(), f.sha256.clone()))
        .collect();
    manifest.bundle_root_hash = pcbcraft_publication::bundle_root_hash(&pairs);
    let json = manifest.to_json_pretty()?;
    files.insert("publication.json".into(), json.into_bytes());
    let report = stage0_from_manifest_and_files(
        std::str::from_utf8(files.get("publication.json").ok_or("missing")?).map_err(|_| "utf8")?,
        &files,
    );
    assert!(!report.passes());
    assert!(report
        .failure_reasons()
        .iter()
        .any(|line| line.contains("media_type")));
    Ok(())
}

#[test]
fn near_duplicate_size_flags_human_queue() -> Result<(), String> {
    let mut files = BTreeMap::new();
    files.insert("a.bin".into(), vec![1u8; 2048]);
    files.insert("b.bin".into(), vec![2u8; 2048]);
    let mut manifest = manifest_for_files(&files);
    manifest.files.clear();
    for path in ["a.bin", "b.bin"] {
        let data = files.get(path).ok_or("missing")?;
        manifest.files.push(pcbcraft_publication::FileEntry {
            path: path.into(),
            sha256: pcbcraft_publication::sha256_hex(data),
            size: data.len() as u64,
            media_type: "text/plain".into(),
            role: "attachment".into(),
            format: "binary".into(),
            format_version: None,
        });
    }
    let pairs: Vec<(String, String)> = manifest
        .files
        .iter()
        .map(|f| (f.path.clone(), f.sha256.clone()))
        .collect();
    manifest.bundle_root_hash = pcbcraft_publication::bundle_root_hash(&pairs);
    let json = manifest.to_json_pretty()?;
    files.insert("publication.json".into(), json.into_bytes());
    let report = stage0_from_manifest_and_files(
        std::str::from_utf8(files.get("publication.json").ok_or("missing")?).map_err(|_| "utf8")?,
        &files,
    );
    let flagged = report.results.iter().any(|r| {
        r.rule == "near_duplicate" && matches!(r.verdict, CheckVerdict::Flag(_))
    });
    assert!(flagged);
    Ok(())
}
