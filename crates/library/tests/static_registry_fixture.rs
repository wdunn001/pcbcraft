use pcbcraft_library::static_registry::{
    build_catalogue, catalogue_with_origin, install_publication, merge_catalogues,
    read_instance_feed, verify_publication_dir, verify_registry_manifest,
};
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/static-registry")
}

#[test]
fn fixture_registry_catalogue_and_manifest_match_disk() -> Result<(), String> {
    let root = fixture_root();
    verify_registry_manifest(&root)?;
    verify_publication_dir(&root.join("publications/fixture-r/1.0.0"))?;
    let built = build_catalogue(&root)?;
    assert_eq!(built.items.len(), 1);
    assert_eq!(built.items[0].id, "fixture-r");
    Ok(())
}

#[test]
fn fixture_install_copies_verified_files() -> Result<(), String> {
    let root = fixture_root();
    let dest = std::env::temp_dir().join(format!(
        "pcbcraft-registry-install-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dest);
    let files = install_publication(&root, "fixture-r", "1.0.0", &dest)?;
    assert_eq!(files, vec!["FixtureR.kicad_sym".to_string()]);
    let _ = std::fs::remove_dir_all(&dest);
    Ok(())
}

#[test]
fn fixture_instance_feed_items_carry_origin() -> Result<(), String> {
    let root = fixture_root();
    let local = build_catalogue(&root)?.items;
    let feed_path = root.join("instances/sample-instances.json");
    let feed = read_instance_feed(&feed_path)?;
    let catalogue_path = feed_path
        .parent()
        .ok_or("feed parent")?
        .join(&feed.catalogue_path);
    let remote = catalogue_with_origin(&catalogue_path, &feed.instance_id, &feed.title)?;
    assert_eq!(remote.len(), 1);
    assert_eq!(
        remote[0].origin.as_ref().map(|o| o.instance_id.as_str()),
        Some("fixture-peer")
    );
    let merged = merge_catalogues(local, remote);
    assert_eq!(merged.len(), 2);
    assert!(merged.iter().any(|i| i.id == "peer-led" && i.origin.is_some()));
    Ok(())
}

