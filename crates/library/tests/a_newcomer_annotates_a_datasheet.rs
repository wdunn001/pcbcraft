//! Story from issue #722: a newcomer can annotate datasheets with highlights, notes and bookmarks.
use pcbcraft_library::datasheet_annotations::{
    DatasheetEntry, EntryKind, ImportMode, PageRect, export_file, import_file, list_entries,
    remove_entry, set_store_path_for_test, upsert_entry,
};

fn with_store(path: std::path::PathBuf) -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    set_store_path_for_test(Some(path));
    guard
}

#[test]
fn a_newcomer_annotates_a_datasheet() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("pcbcraft-anno-story-{}", std::process::id()));
    let _guard = with_store(dir.join("store.json"));

    let hash = "abc12345deadbeef";

    // A newcomer opens a datasheet and bookmarks the pinout table on page 2.
    let bookmark = DatasheetEntry {
        id: "bookmark-pinout".into(),
        kind: EntryKind::Bookmark,
        page: 2,
        rect: PageRect {
            x0: 0.1,
            y0: 0.2,
            x1: 0.3,
            y1: 0.4,
        },
        note: String::new(),
        label: "Pinout table".into(),
    };
    upsert_entry(hash, bookmark)?;

    // They also highlight the absolute maximum ratings on page 3.
    let highlight = DatasheetEntry {
        id: "highlight-max-ratings".into(),
        kind: EntryKind::Highlight,
        page: 3,
        rect: PageRect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 0.5,
        },
        note: "Max current 200 mA, max voltage 3.6 V".into(),
        label: String::new(),
    };
    upsert_entry(hash, highlight)?;

    // They can list all their marks on this document.
    let entries = list_entries(hash)?;
    assert_eq!(entries.len(), 2);

    // They can remove a mark.
    remove_entry(hash, "highlight-max-ratings")?;
    let entries = list_entries(hash)?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "bookmark-pinout");

    // They can export their annotations.
    let exported = export_file(None)?;
    assert_eq!(exported.documents.len(), 1);

    // They can import them on a different machine.
    // The lock from the first call is still held, so only the store path changes here.
    set_store_path_for_test(Some(dir.join("store2.json")));
    let imported = import_file(&exported, ImportMode::ReplaceAll)?;
    assert_eq!(imported, 1);
    let entries = list_entries(hash)?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].label, "Pinout table");

    Ok(())
}
