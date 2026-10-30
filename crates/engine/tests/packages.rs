//! Export package tests on the sample design (issues #214 and #224).
//!
//! Golden files live in `tests/golden/`; set `UPDATE_GOLDEN=1` to rewrite them after a
//! deliberate change and review the diff.
use pcbcraft_engine::Session;
use serde_json::json;
use std::path::{Path, PathBuf};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pcbcraft-pkg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Names of the entries of a stored ZIP, in archive order.
fn zip_names(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut at = 0;
    while at + 30 <= bytes.len() && bytes[at..at + 4] == [0x50, 0x4b, 0x03, 0x04] {
        let size = u32::from_le_bytes([
            bytes[at + 18],
            bytes[at + 19],
            bytes[at + 20],
            bytes[at + 21],
        ]) as usize;
        let name_len = u16::from_le_bytes([bytes[at + 26], bytes[at + 27]]) as usize;
        let extra = u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]) as usize;
        names.push(String::from_utf8_lossy(&bytes[at + 30..at + 30 + name_len]).into_owned());
        at += 30 + name_len + extra + size;
    }
    names
}

fn golden(name: &str, actual: &str) -> Result<(), String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, actual).map_err(|e| e.to_string())?;
    }
    let want = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    assert_eq!(want, actual, "golden {name} differs");
    Ok(())
}

fn export(s: &mut Session, id: &str, dir: &Path) -> Result<serde_json::Value, String> {
    s.execute(
        "package.export",
        json!({"id": id, "directory": dir.to_string_lossy()}),
        None,
    )
    .map(|v| v["result"].clone())
    .map_err(|e| e.to_string())
}

#[test]
fn the_prototype_fab_package_matches_its_golden_file_list() -> Result<(), String> {
    let mut s = Session::sample().map_err(|e| e.to_string())?;
    let dir = scratch("fab");
    let r = export(&mut s, "fab_prototype", &dir)?;
    let zip = r["written"][0]
        .as_str()
        .ok_or("no archive written")?
        .to_string();
    assert_eq!(zip, "LED_indicator-fab-A.zip");
    let bytes = std::fs::read(dir.join(&zip)).map_err(|e| e.to_string())?;
    let names = zip_names(&bytes);
    assert_eq!(names.first().map(String::as_str), Some("manifest.json"));
    golden("fab_prototype.txt", &format!("{}\n", names.join("\n")))?;
    // The manifest lists every file and nothing else.
    let listed: Vec<String> = r["files"]
        .as_array()
        .ok_or("no files")?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    assert_eq!(&names[1..], listed.as_slice());
    // Two exports of one design are byte-identical.
    let dir2 = scratch("fab2");
    export(&mut s, "fab_prototype", &dir2)?;
    assert_eq!(
        bytes,
        std::fs::read(dir2.join(&zip)).map_err(|e| e.to_string())?
    );
    // The run is recorded with revision and hash.
    assert_eq!(s.project.package_runs.len(), 1);
    assert_eq!(
        s.project.package_runs[0].hash,
        r["hash"].as_str().ok_or("hash")?
    );
    Ok(())
}

#[test]
fn the_prototype_assembly_package_matches_its_golden_files() -> Result<(), String> {
    let mut s = Session::sample().map_err(|e| e.to_string())?;
    let dir = scratch("asm");
    let r = export(&mut s, "assembly_prototype", &dir)?;
    let mut written: Vec<String> = r["written"]
        .as_array()
        .ok_or("written")?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    written.sort();
    assert_eq!(
        written,
        [
            "LED_indicator-bom-A.csv",
            "LED_indicator-cpl-A.csv",
            "manifest.json"
        ]
    );
    let bom =
        std::fs::read_to_string(dir.join("LED_indicator-bom-A.csv")).map_err(|e| e.to_string())?;
    let cpl =
        std::fs::read_to_string(dir.join("LED_indicator-cpl-A.csv")).map_err(|e| e.to_string())?;
    assert!(cpl.starts_with("Designator,Mid X,Mid Y,Rotation,Layer\n"));
    golden("assembly_prototype_bom.csv", &bom)?;
    golden("assembly_prototype_cpl.csv", &cpl)?;
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).map_err(|e| e.to_string())?;
    assert!(
        manifest.contains("LED_indicator-bom-A.csv")
            && manifest.contains("LED_indicator-cpl-A.csv")
    );
    Ok(())
}

#[test]
fn a_package_with_a_missing_writer_refuses_and_writes_nothing() -> Result<(), String> {
    let mut s = Session::sample().map_err(|e| e.to_string())?;
    let dir = scratch("stencil");
    let err = s
        .execute(
            "package.export",
            json!({"id": "stencil", "directory": dir.to_string_lossy()}),
            None,
        )
        .err()
        .ok_or("a stencil package must refuse")?
        .to_string();
    assert!(err.contains("#420"), "the refusal names the issue: {err}");
    assert!(!dir.exists());
    let list = s
        .query("package.list", json!({}), None)
        .map_err(|e| e.to_string())?;
    let all = list["result"]["packages"].as_array().ok_or("packages")?;
    assert_eq!(all.len(), 10);
    let flags: Vec<(&str, bool)> = all
        .iter()
        .filter_map(|p| Some((p["id"].as_str()?, p["available"].as_bool()?)))
        .collect();
    assert!(flags.contains(&("fab_prototype", true)) && flags.contains(&("stencil", false)));
    Ok(())
}

#[test]
fn a_project_package_is_saved_validated_and_exported() -> Result<(), String> {
    let mut s = Session::sample().map_err(|e| e.to_string())?;
    let mine = json!({"id":"mine","name":"Mine","audience":"me","container":"file",
        "files":[{"kind":"bom","format":"bom_csv","name":"{project}-parts.csv"}]});
    s.execute("package.save", json!({"package": mine}), None)
        .map_err(|e| e.to_string())?;
    let bad = json!({"id":"fab_prototype","name":"x","audience":"","container":"file",
        "files":[{"kind":"bom","format":"bom_csv","name":"a.csv"}]});
    assert!(
        s.execute("package.save", json!({"package": bad}), None)
            .is_err(),
        "bundled id"
    );
    for name in ["../a.csv", "/a.csv", "a/b.csv", "{nope}.csv"] {
        let p = json!({"id":"x","name":"x","audience":"","container":"file",
            "files":[{"kind":"bom","format":"bom_csv","name":name}]});
        assert!(
            s.execute("package.save", json!({"package": p}), None)
                .is_err(),
            "{name}"
        );
    }
    let unknown = json!({"id":"y","name":"y","audience":"","container":"file",
        "files":[{"kind":"bom","format":"made_up","name":"a.csv"}]});
    assert!(
        s.execute("package.save", json!({"package": unknown}), None)
            .is_err()
    );
    let dir = scratch("mine");
    let r = export(&mut s, "mine", &dir)?;
    assert_eq!(r["written"][0], "LED_indicator-parts.csv");
    // It round-trips through the project file.
    let text = serde_json::to_string(&s.project).map_err(|e| e.to_string())?;
    let back: pcbcraft_doc::Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    back.validate()?;
    assert_eq!(back.packages, s.project.packages);
    Ok(())
}

#[test]
fn a_bundled_package_can_be_duplicated_edited_and_exported() -> Result<(), String> {
    let mut s = Session::sample().map_err(|e| e.to_string())?;
    s.execute(
        "package.duplicate",
        json!({"from": "fab_prototype", "id": "my_fab", "name": "My fab"}),
        None,
    )
    .map_err(|e| e.to_string())?;
    assert!(
        s.execute(
            "package.duplicate",
            json!({"from": "fab_prototype", "id": "fab_prototype"}),
            None,
        )
        .is_err(),
        "bundled id"
    );
    let list = s
        .query("package.list", json!({}), None)
        .map_err(|e| e.to_string())?;
    let mine = list["result"]["packages"]
        .as_array()
        .ok_or("packages")?
        .iter()
        .find(|p| p["id"] == "my_fab")
        .ok_or("my_fab")?;
    assert_eq!(mine["source"], "project");
    let dir = scratch("dup");
    export(&mut s, "my_fab", &dir)?;
    Ok(())
}

#[test]
fn the_ipc2581_package_writes_one_xml_file() -> Result<(), String> {
    let mut s = Session::sample().map_err(|e| e.to_string())?;
    let dir = scratch("ipc");
    let r = export(&mut s, "ipc2581", &dir)?;
    let written: Vec<String> = r["written"]
        .as_array()
        .ok_or("written")?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    assert_eq!(written, ["LED_indicator-A.xml"]);
    let xml =
        std::fs::read_to_string(dir.join("LED_indicator-A.xml")).map_err(|e| e.to_string())?;
    assert!(
        xml.contains("IPC-2581") || xml.contains("ipc2581"),
        "looks like IPC-2581"
    );
    assert_eq!(r["files"].as_array().ok_or("files")?.len(), 1);
    let dir2 = scratch("ipc2");
    export(&mut s, "ipc2581", &dir2)?;
    assert_eq!(
        std::fs::read(dir.join("LED_indicator-A.xml")).map_err(|e| e.to_string())?,
        std::fs::read(dir2.join("LED_indicator-A.xml")).map_err(|e| e.to_string())?
    );
    Ok(())
}
