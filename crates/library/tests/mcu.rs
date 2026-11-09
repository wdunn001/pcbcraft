use pcbcraft_library::mcu::{
    MCU_FILE_SUFFIX, import_mcu_bundle_json, load_builtin, parse_and_validate_mcu, validate_mcu,
};

#[test]
fn a_newcomer_loads_builtin_stm32f405_and_stm32g431_tables() -> Result<(), String> {
    for id in ["stm32f405rg", "stm32g431cb"] {
        let doc = load_builtin(id).map_err(|e| format!("{id}: {e}"))?;
        assert_eq!(doc.mcu.id, id);
        assert!(!doc.provenance.licence.is_empty());
        assert!(!doc.package.is_empty());
        assert!(!doc.pin.is_empty());
    }
    Ok(())
}

#[test]
fn a_newcomer_is_refused_when_a_pin_is_missing_from_the_package_table() -> Result<(), String> {
    let mut doc = load_builtin("stm32g431cb").map_err(|e| format!("{}: {e:?}", "built-in G431"))?;
    doc.pin.push(pcbcraft_library::mcu::McuPin {
        name: "PZ99".into(),
        five_volt_tolerance: false,
        analog_only: false,
        boot_strap: None,
        power_domain: None,
        alternate: vec![pcbcraft_library::mcu::PinAlternate {
            af: 0,
            signal: "GPIOZ.99".into(),
        }],
    });
    let err = validate_mcu(&doc).err().ok_or("orphan pin must fail")?;
    assert!(err.contains("PZ99"), "error must name the pin, got {err}");
    Ok(())
}

#[test]
fn a_newcomer_is_refused_when_provenance_is_incomplete() -> Result<(), String> {
    let text = include_str!("../data/mcu/stm32g431cb.toml").replace(
        "licence = \"ST document licence (pin names and mux facts transcribed for PCBCraft)\"",
        "licence = \"\"",
    );
    let err = parse_and_validate_mcu(&text).err().ok_or("empty licence")?;
    assert!(err.contains("licence"), "got {err}");
    Ok(())
}

#[test]
fn a_newcomer_imports_a_json_bundle_for_the_first_family() -> Result<(), String> {
    let json = include_str!("../data/mcu/bundles/stm32f405rg.json");
    let doc = import_mcu_bundle_json(json).map_err(|e| format!("{}: {e:?}", "import bundle"))?;
    assert_eq!(doc.mcu.family, "STM32F4");
    assert_eq!(doc.schema, 1);
    Ok(())
}

#[test]
fn mcu_file_suffix_is_documented() {
    assert!(MCU_FILE_SUFFIX.ends_with(".toml"));
}
