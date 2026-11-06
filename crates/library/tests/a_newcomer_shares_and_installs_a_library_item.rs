//! Story from issue #987: publish to a folder, install from it, offline,
//! with hash verification. No server required.
use pcbcraft_library::static_registry::{
    PublishInput, install_publication, publish_to_folder, verify_publication_dir,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn registry_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "pcbcraft-registry-acceptance-{}",
        std::process::id()
    ))
}

#[test]
fn a_newcomer_shares_and_installs_a_library_item() -> Result<(), String> {
    let root = registry_dir();
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).map_err(|e| format!("create registry dir: {e}"))?;

    // Step 1: Publish a component to the local registry folder
    let mut files = BTreeMap::new();
    files.insert(
        "TestLED.kicad_sym".to_string(),
        "symbol content".to_string().as_bytes().to_vec(),
    );
    let input = PublishInput {
        id: "test-led".to_string(),
        title: "Test LED".to_string(),
        version: "1.0.0".to_string(),
        kind: "component".to_string(),
        tags: vec!["led".to_string()],
        files,
    };
    let pub_dir =
        publish_to_folder(&root, input).map_err(|e| format!("publish_to_folder failed: {e}"))?;

    // Step 2: Verify the publication was written and its manifest is valid
    verify_publication_dir(&pub_dir).map_err(|e| format!("verify_publication_dir failed: {e}"))?;

    // Step 3: Install from the registry folder (hash-verified copy)
    let dest = root.join("installed");
    let files = install_publication(&root, "test-led", "1.0.0", &dest)
        .map_err(|e| format!("install_publication failed: {e}"))?;
    assert_eq!(files, vec!["TestLED.kicad_sym"]);

    // Step 4: Tamper with a file in the registry and confirm installation is refused
    let sym_path = pub_dir.join("TestLED.kicad_sym");
    std::fs::write(&sym_path, "tampered content").map_err(|e| format!("write tampered: {e}"))?;

    let dest2 = root.join("installed2");
    let result = install_publication(&root, "test-led", "1.0.0", &dest2);
    assert!(
        result.is_err(),
        "install should fail after tampering the publication file"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}
