//! Verify the checked-in static registry fixture and its generated catalogue.
use pcbcraft_library::static_registry::{
    build_catalogue, generate_index_html, rebuild_site, verify_registry_manifest,
};
use std::fs;
use std::path::{Path, PathBuf};

const FIXTURE: &str = "crates/library/tests/fixtures/static-registry";

pub fn run(repo: &Path, bless: bool) -> Result<(), String> {
    let root = repo.join(FIXTURE);
    if bless {
        rebuild_site(&root)?;
        println!("registry bless: rebuilt catalogue and site under {FIXTURE}");
        return Ok(());
    }
    verify_registry_manifest(&root)?;
    let built = build_catalogue(&root)?;
    let catalogue_path = root.join("catalogue.json");
    let on_disk_text = fs::read_to_string(&catalogue_path).map_err(|e| e.to_string())?;
    let on_disk: pcbcraft_library::static_registry::CatalogueDocument =
        serde_json::from_str(&on_disk_text).map_err(|e| e.to_string())?;
    if built != on_disk {
        return Err(
            "catalogue.json does not match publications; run cargo xtask registry bless"
                .into(),
        );
    }
    let html_path = root.join("index.html");
    let html_disk = fs::read_to_string(&html_path).map_err(|e| e.to_string())?;
    let html_built = generate_index_html(&built);
    if html_disk != html_built {
        return Err(
            "index.html does not match the catalogue; run cargo xtask registry bless".into(),
        );
    }
    println!("registry check: fixture catalogue and site are current");
    Ok(())
}

pub fn root() -> Result<PathBuf, String> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask has no parent directory")
        .map(|p| p.to_path_buf())
}
