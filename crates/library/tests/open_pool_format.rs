//! Reads recorded second open pool JSON and CC0 base component fixtures.
use pcbcraft_library::cc0_component::read_cc0_component;
use pcbcraft_library::online_import::open_pool_step_path;
use pcbcraft_library::open_pool::{read_open_pool_package, read_open_pool_part};
use std::path::PathBuf;

type Boxed = Box<dyn std::error::Error>;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/open_pool")
}

#[test]
fn a_part_record_names_its_entity_package_and_mpn() -> Result<(), Boxed> {
    let text = std::fs::read_to_string(fixtures().join("base-0603.json"))?;
    let part = read_open_pool_part(&text)?;
    assert_eq!(part.mpn, "base 0603 resistor");
    assert_eq!(part.entity_id, "3ce770af-7be0-4f72-b2af-2c8010911aaf");
    assert_eq!(part.package_id, "fe22ac1d-25b9-4a47-8414-51c990d7f94b");
    assert!(part.tags.contains(&"resistor".to_owned()));
    Ok(())
}

#[test]
fn a_package_record_names_its_default_step_path() -> Result<(), Boxed> {
    let text = std::fs::read_to_string(fixtures().join("R0603-package.json"))?;
    let package = read_open_pool_package(&text)?;
    assert_eq!(package.name, "R0603");
    assert_eq!(
        package.default_step_path(),
        Some("3d_models/passive/resistor/R_0603_1608Metric.step")
    );
    Ok(())
}

#[test]
fn a_matching_part_and_package_yield_the_cached_step_path() -> Result<(), Boxed> {
    let part = read_open_pool_part(&std::fs::read_to_string(fixtures().join("base-0603.json"))?)?;
    let package =
        read_open_pool_package(&std::fs::read_to_string(fixtures().join("R0603-package.json"))?)?;
    let path = open_pool_step_path(&part, &package).ok_or("expected a STEP path")?;
    assert!(path.ends_with("R_0603_1608Metric.step"));
    Ok(())
}

#[test]
fn a_cc0_component_record_lists_its_signals() -> Result<(), Boxed> {
    let text = std::fs::read_to_string(fixtures().join("diode-component.lp"))?;
    let cmp = read_cc0_component(&text)?;
    assert_eq!(cmp.name, "Diode");
    assert_eq!(cmp.prefix, "D");
    assert_eq!(cmp.signals.len(), 2);
    assert!(cmp.signals.iter().any(|s| s.name == "A"));
    assert!(cmp.signals.iter().any(|s| s.name == "C"));
    Ok(())
}

#[test]
fn a_part_file_with_the_wrong_type_is_refused() {
    assert!(read_open_pool_part(r#"{"type":"package","uuid":"00000000-0000-0000-0000-000000000001","entity":"00000000-0000-0000-0000-000000000002","package":"00000000-0000-0000-0000-000000000003","MPN":[false,"x"],"manufacturer":[false,""],"value":[false,""]}"#).is_err());
}
