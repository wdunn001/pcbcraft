//! The machine library commands: list, install, duplicate, update, save, hide and remove.
//!
//! Context: issue #1167. Built-in machines, the person's own machines and the project's
//! copies are one library; these tests run the commands the window and the MCP tools run.
//!
//! # Dependencies
//! - crates/engine/src/machines.rs
//! - crates/engine/src/user_config.rs (a private folder per test)
use pcbcraft_engine::{Session, user_config::UserConfig};
use serde_json::{Value, json};

fn session(tag: &str) -> (Session, std::path::PathBuf) {
    let folder = std::env::temp_dir().join(format!("pcbcraft-machines-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut s = Session::default();
    s.config = UserConfig::in_folder(&folder);
    (s, folder)
}

fn run(s: &mut Session, command: &str, params: Value) -> Result<Value, String> {
    s.execute(command, params, None)
        .map(|v| v.get("result").cloned().unwrap_or(Value::Null))
        .map_err(|e| e.to_string())
}

fn cards(s: &mut Session, params: Value) -> Result<Vec<Value>, String> {
    let list = run(s, "machine.library_list", params)?;
    Ok(list["machines"].as_array().cloned().unwrap_or_default())
}

#[test]
fn built_in_machines_are_listed_as_ordinary_cards() -> Result<(), String> {
    let (mut s, folder) = session("list");
    let all = cards(&mut s, json!({}))?;
    for built in pcbcraft_io::builtin_machine_profile_ids() {
        assert!(
            all.iter()
                .any(|c| c["id"] == *built && c["source"] == "built_in"),
            "{built} is a built-in card"
        );
    }
    let revision = s.revision;
    run(&mut s, "machine.library_list", json!({"kind": "pick_place"}))?;
    assert_eq!(s.revision, revision, "listing is not an edit");
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

#[test]
fn a_hidden_built_in_leaves_the_list_but_never_the_files() -> Result<(), String> {
    let (mut s, folder) = session("hide");
    run(&mut s, "machine.library_hide", json!({"id": "neoden-yy1"}))?;
    assert!(!cards(&mut s, json!({}))?.iter().any(|c| c["id"] == "neoden-yy1"));
    let shown = cards(&mut s, json!({"include_hidden": true}))?;
    assert!(
        shown
            .iter()
            .any(|c| c["id"] == "neoden-yy1" && c["hidden"] == true)
    );
    assert!(
        run(&mut s, "machine.library_remove", json!({"id": "neoden-yy1"})).is_err(),
        "a built-in machine cannot be deleted"
    );
    assert!(run(&mut s, "machine.library_hide", json!({"id": "nothing-here"})).is_err());
    run(
        &mut s,
        "machine.library_hide",
        json!({"id": "neoden-yy1", "hidden": false}),
    )?;
    assert!(cards(&mut s, json!({}))?.iter().any(|c| c["id"] == "neoden-yy1"));
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

#[test]
fn a_session_with_no_folder_refuses_a_save_in_one_plain_sentence() -> Result<(), String> {
    let mut s = Session::default();
    let refusal = run(
        &mut s,
        "machine.add",
        json!({"kind": "cnc_mill", "name": "Bench", "keep_in_my_machines": true}),
    )
    .err()
    .ok_or("a detached session cannot keep a machine")?;
    assert!(refusal.contains("no folder"), "{refusal}");
    assert!(s.project.machine_profiles.is_empty(), "nothing half-added");
    Ok(())
}

#[test]
fn duplicate_remembers_its_source_and_keeps_the_built_in_unchanged() -> Result<(), String> {
    let (mut s, folder) = session("dup");
    let made = run(
        &mut s,
        "machine.duplicate",
        json!({"id": "neoden-yy1", "name": "My placer", "keep_in_my_machines": true}),
    )?;
    assert_eq!(made["id"], "my-placer");
    let copy = s
        .project
        .machine_profiles
        .iter()
        .find(|m| m.id == "my-placer")
        .ok_or("the copy is on the project")?;
    assert_eq!(copy.based_on.as_ref().map(|b| b.id.as_str()), Some("neoden-yy1"));
    assert!(folder.join("machines").join("my-placer.toml").is_file());
    let mine = cards(&mut s, json!({"scope": "mine"}))?;
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0]["in_project"], true);
    assert_eq!(mine[0]["copy_state"], "same");
    // A second duplicate under the same name gets a free id.
    let again = run(
        &mut s,
        "machine.duplicate",
        json!({"id": "neoden-yy1", "name": "My placer"}),
    )?;
    assert_eq!(again["id"], "my-placer-2");
    run(&mut s, "machine.library_remove", json!({"id": "my-placer"}))?;
    assert!(cards(&mut s, json!({"scope": "mine"}))?.is_empty());
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

#[test]
fn a_project_copy_stays_put_until_it_is_updated_from_the_library() -> Result<(), String> {
    let (mut s, folder) = session("update");
    run(
        &mut s,
        "machine.add",
        json!({"kind": "cnc_mill", "name": "Shop mill", "keep_in_my_machines": true}),
    )?;
    let file = folder.join("machines").join("shop-mill.toml");
    let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    let mut newer = pcbcraft_io::machine_profile_from_toml(&text)?;
    newer.version += 1;
    newer.notes = "Re-measured.".into();
    std::fs::write(&file, pcbcraft_io::machine_profile_to_toml(&newer)?).map_err(|e| e.to_string())?;
    let card = cards(&mut s, json!({"scope": "mine"}))?
        .into_iter()
        .next()
        .ok_or("the machine is listed")?;
    assert_eq!(card["copy_state"], "library_newer");
    let before = s
        .project
        .machine_profiles
        .iter()
        .find(|m| m.id == "shop-mill")
        .map(|m| m.notes.clone());
    assert_ne!(before.as_deref(), Some("Re-measured."), "the copy did not move");
    let answer = run(&mut s, "machine.update_from_library", json!({"id": "shop-mill"}))?;
    assert!(
        answer["changed"]
            .as_array()
            .is_some_and(|c| c.iter().any(|k| k == "notes")),
        "{answer}"
    );
    let after = s.project.machine_profiles.iter().find(|m| m.id == "shop-mill");
    assert_eq!(after.map(|m| m.notes.as_str()), Some("Re-measured."));
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

#[test]
fn install_copies_once_and_a_machine_with_an_unknown_dialect_is_refused() -> Result<(), String> {
    let (mut s, folder) = session("install");
    run(&mut s, "machine.install", json!({"id": "neoden-yy1"}))?;
    assert!(run(&mut s, "machine.install", json!({"id": "neoden-yy1"})).is_err());
    assert!(run(&mut s, "machine.install", json!({"id": "no-such"})).is_err());
    run(&mut s, "machine.add", json!({"kind": "pick_place", "name": "Odd placer"}))?;
    let dir = folder.join("out");
    let refusal = run(
        &mut s,
        "placement_job.export",
        json!({"machine": "odd-placer", "directory": dir.to_string_lossy(), "side": "both", "layout": "native"}),
    )
    .err()
    .ok_or("a machine with no writer for its dialect is refused")?;
    assert!(!refusal.is_empty());
    run(
        &mut s,
        "placement_yy1.export",
        json!({"machine": "neoden-yy1", "directory": dir.to_string_lossy(), "side": "both", "layout": "native"}),
    )?;
    assert!(dir.join("Top.csv").is_file(), "the old command name still works");
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

#[test]
fn old_project_files_load_with_machine_defaults() -> Result<(), String> {
    // A machine stored by an older program has none of the library fields.
    let text = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../machine-profiles/neoden-yy1.toml"),
    )
    .map_err(|e| e.to_string())?;
    let old: String = text
        .lines()
        .filter(|l| {
            !["schema_version", "version", "source", "retrieved"]
                .iter()
                .any(|k| l.starts_with(&format!("{k} ")) || l.starts_with(&format!("{k}=")))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let machine = pcbcraft_io::machine_profile_from_toml(&old)?;
    assert_eq!(machine.version, 0, "a missing version reads as 0");
    assert!(machine.based_on.is_none());
    Ok(())
}
