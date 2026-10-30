use pcbcraft_library::cad::{
    CadFile, CadKind, CadRef, CadSource, LicenseInfo, MAX_NAME_BYTES, Provenance,
};
use pcbcraft_library::sources::{
    Attribution, FixtureSource, PartResult, PartSource, SetupCard, SourceError,
};

fn provenance() -> Provenance {
    Provenance {
        source_id: "open-library".into(),
        source_name: "Open library".into(),
        kind: CadKind::Footprint,
        upstream_path: "Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod".into(),
        blob_id: "0123456789abcdef0123456789abcdef01234567".into(),
        commit_id: "a337146e090931ad1176e20ed0b7100990fac5fa".into(),
        license_name: "CC-BY-SA 4.0".into(),
        license_url: "https://creativecommons.org/licenses/by-sa/4.0/".into(),
        attribution: "Community library contributors".into(),
    }
}

#[test]
fn cad_ref_accepts_plain_names() -> Result<(), SourceError> {
    let r = CadRef::new(CadKind::Symbol, "Device", "R")?.with_revision("7518db51e6f8")?;
    assert_eq!(r.library, "Device");
    assert_eq!(r.name, "R");
    assert_eq!(r.revision, "7518db51e6f8");
    // Part names with spaces, dots and plus signs are real library names.
    CadRef::new(
        CadKind::Footprint,
        "Package_SO.pretty",
        "SOIC-8_3.9x4.9mm_P1.27mm",
    )?;
    CadRef::new(CadKind::Symbol, "Device", "C Polarized+")?;
    Ok(())
}

#[test]
fn cad_ref_refuses_traversal_and_control_characters() {
    let bad = [
        ("Device", ".."),
        ("..", "R"),
        ("Device/../etc", "R"),
        ("Device", "a/b"),
        ("Device", "a\\b"),
        ("Device", "a\0b"),
        ("Device", "line\nbreak"),
        ("Device", ""),
        ("", "R"),
        ("Device", "x..y"),
    ];
    for (library, name) in bad {
        assert!(
            CadRef::new(CadKind::Symbol, library, name).is_err(),
            "{library:?} / {name:?} must be refused"
        );
    }
    let long = "a".repeat(MAX_NAME_BYTES + 1);
    assert!(CadRef::new(CadKind::Symbol, "Device", &long).is_err());
    assert!(
        CadRef::new(CadKind::Symbol, "Device", "R")
            .and_then(|r| r.with_revision("not hex/ok"))
            .is_err(),
        "a revision with a slash or a space is refused"
    );
}

#[test]
fn a_reference_built_by_hand_is_checked_again() {
    let r = CadRef {
        kind: CadKind::Symbol,
        library: "Device".into(),
        name: "../../secret".into(),
        revision: String::new(),
    };
    assert!(r.check().is_err());
}

#[test]
fn provenance_notice_names_source_licence_and_upstream_path() {
    let n = provenance().notice();
    assert!(n.contains("Open library"), "names the source: {n}");
    assert!(n.contains("Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod"));
    assert!(n.contains("CC-BY-SA 4.0"));
    assert!(n.contains("https://creativecommons.org/licenses/by-sa/4.0/"));
    assert!(n.contains("0123456789abcdef"), "keeps the file revision");
    assert!(n.contains("a337146e"), "keeps the repository revision");
    assert!(n.contains("Community library contributors"));
}

#[test]
fn provenance_notice_is_stable_and_under_one_kib() {
    assert_eq!(provenance().notice(), provenance().notice());
    let mut huge = provenance();
    let long = "x".repeat(5000);
    huge.source_name = long.clone();
    huge.upstream_path = long.clone();
    huge.blob_id = long.clone();
    huge.commit_id = long.clone();
    huge.license_name = long.clone();
    huge.license_url = long.clone();
    huge.attribution = format!("{long}\n\u{0}control");
    let n = huge.notice();
    assert!(n.len() < 1024, "notice is {} bytes", n.len());
    assert!(!n.chars().any(char::is_control));
}

#[test]
fn a_notice_with_unknown_revisions_says_nothing_about_them() {
    let mut p = provenance();
    p.blob_id.clear();
    p.commit_id.clear();
    let n = p.notice();
    assert!(!n.contains("revision"), "{n}");
}

struct Stub;
impl CadSource for Stub {
    fn id(&self) -> &str {
        "stub"
    }
    fn name(&self) -> &str {
        "Stub"
    }
    fn license(&self) -> LicenseInfo {
        LicenseInfo {
            name: "CC0".into(),
            url: String::new(),
            terms_note: String::new(),
        }
    }
    fn attribution(&self) -> Attribution {
        Attribution {
            text: "Stub".into(),
            url: String::new(),
        }
    }
    fn list_libraries(&self, _kind: CadKind) -> Result<Vec<String>, SourceError> {
        Ok(vec!["Device".into()])
    }
    fn list_entries(&self, kind: CadKind, library: &str) -> Result<Vec<CadRef>, SourceError> {
        Ok(vec![CadRef::new(kind, library, "R")?])
    }
    fn search(&self, kind: CadKind, _query: &str) -> Result<Vec<CadRef>, SourceError> {
        Ok(vec![CadRef::new(kind, "Device", "R")?])
    }
    fn fetch(&self, cad_ref: &CadRef) -> Result<CadFile, SourceError> {
        Ok(CadFile {
            cad_ref: cad_ref.clone(),
            bytes: b"x".to_vec(),
            provenance: provenance(),
        })
    }
}

#[test]
fn the_typed_fetch_methods_refuse_the_wrong_kind() -> Result<(), SourceError> {
    let s = Stub;
    let symbol = CadRef::new(CadKind::Symbol, "Device", "R")?;
    assert!(s.fetch_symbol(&symbol).is_ok());
    assert!(s.fetch_footprint(&symbol).is_err());
    assert!(s.fetch_model(&symbol).is_err());
    Ok(())
}

struct Both {
    cad: Box<dyn CadSource>,
    parts: Box<dyn PartSource>,
}

#[test]
fn a_cad_source_object_can_sit_beside_a_part_source() -> Result<(), SourceError> {
    let parts = FixtureSource::from_json(r#"{"name":"Sample","parts":[]}"#)?;
    let both = Both {
        cad: Box::new(Stub),
        parts: Box::new(parts),
    };
    assert_eq!(both.cad.id(), "stub");
    assert_eq!(both.parts.name(), "Sample");
    let found: Vec<PartResult> = both.parts.search("anything")?;
    assert!(found.is_empty());
    let card: SetupCard = both.parts.setup();
    assert!(!card.steps.is_empty());
    Ok(())
}
