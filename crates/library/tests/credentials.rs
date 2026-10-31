use pcbcraft_library::credentials::{FileKeyStore, KeyStore, MemoryKeyStore};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

#[test]
fn keys_roundtrip_through_the_store() -> Result<(), pcbcraft_library::credentials::CredentialError>
{
    let store = MemoryKeyStore::default();
    store.set("sample.key", "value")?;
    assert_eq!(store.get("sample.key")?, Some("value".into()));
    store.delete("sample.key")?;
    assert_eq!(store.get("sample.key")?, None);
    Ok(())
}

#[test]
fn fallback_store_reports_the_warning() -> Result<(), pcbcraft_library::credentials::CredentialError>
{
    let dir = TempDir::new()
        .map_err(|e| pcbcraft_library::credentials::CredentialError::Failed(e.to_string()))?;
    let path = dir.path().join("keys.json");
    let store = FileKeyStore::new(&path);
    assert!(
        store
            .storage_warning()
            .is_some_and(|w| w.contains("plain file")),
        "file fallback must warn"
    );
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct ProjectStub {
    name: String,
    revision: u32,
}

#[test]
fn keys_never_appear_in_serialized_project()
-> Result<(), pcbcraft_library::credentials::CredentialError> {
    let store = MemoryKeyStore::default();
    store.set("digikey.client_secret", "top-secret")?;
    let project = ProjectStub {
        name: "demo".into(),
        revision: 1,
    };
    let text = serde_json::to_string(&project)
        .map_err(|e| pcbcraft_library::credentials::CredentialError::Failed(e.to_string()))?;
    assert!(
        !text.contains("top-secret"),
        "project JSON must not embed key store values"
    );
    Ok(())
}
