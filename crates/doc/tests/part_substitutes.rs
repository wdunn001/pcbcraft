//! Project library validation for part substitute records (#719).
use pcbcraft_doc::{Project, ProjectLibrary};
use pcbcraft_library::{AlternatePart, PartSubstitutes, SecondSource};

fn sample_symbol(name: &str) -> pcbcraft_library::SymbolDefinition {
    pcbcraft_library::SymbolDefinition {
        name: name.into(),
        reference: "R".into(),
        value: "R".into(),
        description: String::new(),
        keywords: String::new(),
        footprint: String::new(),
        set: String::new(),
        pins: vec![pcbcraft_library::SymbolPin {
            number: "1".into(),
            name: "~".into(),
            position: pcbcraft_geom::Point::new(0.0, 0.0),
            angle: 0.0,
            length: 2.54,
            electrical_type: "passive".into(),
            unit: 1,
            body_style: 1,
            graphical_style: "line".into(),
            visible: true,
        }],
        graphics: vec![],
        extends: None,
        raw_source: String::new(),
        unsupported: vec![],
    }
}

#[test]
fn substitute_record_must_point_at_real_symbols() -> Result<(), String> {
    let mut library = ProjectLibrary {
        symbols: vec![sample_symbol("Primary"), sample_symbol("Alt")],
        ..ProjectLibrary::default()
    };
    library.part_substitutes.insert(
        "Primary".into(),
        PartSubstitutes {
            second_sources: vec![SecondSource {
                manufacturer: "Acme".into(),
                mpn: "1".into(),
                note: String::new(),
            }],
            alternates: vec![AlternatePart {
                symbol: "Alt".into(),
                footprint: String::new(),
                note: String::new(),
            }],
        },
    );
    library.validate()?;
    library.part_substitutes.get_mut("Primary").ok_or("record")?.alternates[0].symbol =
        "Missing".into();
    assert!(library.validate().is_err());
    Ok(())
}

#[test]
fn default_project_has_no_substitute_records() -> Result<(), String> {
    let project = Project::default();
    assert!(project.library.part_substitutes.is_empty());
    project.validate()?;
    Ok(())
}
