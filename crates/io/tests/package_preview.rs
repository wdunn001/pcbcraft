//! Preview file names for export packages (issue #214).

use pcbcraft_doc::Project;
use pcbcraft_io::{find_package, preview_package_files};

fn sample_project() -> Result<Project, String> {
    let mut project = Project {
        name: "sample_board".into(),
        ..Project::default()
    };
    project.meta.insert("revision".into(), "A".into());
    project.validate()?;
    Ok(project)
}

#[test]
fn the_fab_package_previews_its_layer_and_drill_names() -> Result<(), String> {
    let p = sample_project()?;
    let def = find_package(&p, "fab_prototype").ok_or("fab_prototype package not found")?;
    let names = preview_package_files(&p, &def, None)?;
    assert!(
        names.iter().any(|n| n.ends_with(".GTL")),
        "top layer Gerber is listed: {names:?}"
    );
    assert!(
        names.iter().any(|n| n.ends_with(".GBL")),
        "bottom layer Gerber is listed: {names:?}"
    );
    assert!(
        names.iter().any(|n| n.ends_with(".TXT")),
        "drill file or readme is listed: {names:?}"
    );
    Ok(())
}

#[test]
fn the_stencil_package_previews_template_names_even_though_it_is_disabled() -> Result<(), String> {
    let p = sample_project()?;
    let def = find_package(&p, "stencil").ok_or("stencil package not found")?;
    let names = preview_package_files(&p, &def, None)?;
    assert!(
        !names.is_empty(),
        "disabled packages still show their intended files: {names:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_variant_is_an_error_for_the_fab_package() -> Result<(), String> {
    let p = sample_project()?;
    let def = find_package(&p, "fab_prototype").ok_or("fab_prototype package not found")?;
    let err = preview_package_files(&p, &def, Some("zzz"))
        .err()
        .ok_or("unknown variant must fail")?;
    assert!(
        err.contains("variant") || err.contains("zzz"),
        "the error names the variant: {err}"
    );
    Ok(())
}
