use pcbcraft_library::starter_models;

#[test]
fn starter_catalog_lists_the_chip_resistor() {
    let all = starter_models::all();
    assert!(!all.is_empty());
    assert!(starter_models::by_id("chip_resistor_0603").is_some());
}

#[test]
fn starter_models_match_footprint_and_path_names() {
    assert!(starter_models::for_footprint("R_0603_1608Metric").is_some());
    assert!(starter_models::for_footprint("chip_0603").is_some());
    assert!(starter_models::for_model_path(
        "Resistor_SMD.3dshapes/R_0603_1608Metric.step"
    )
    .is_some());
    assert!(starter_models::for_footprint("DIP-8_W7.62mm").is_none());
}
