//! Oracle tests for fabrication presets and class-pair rules.
use pcbcraft_doc::{
    ClassPairRule, FabRules, FAB_PRESET_NAMES, fab_preset, fab_preset_list,
};

type T = Result<(), String>;

#[test]
fn fab_preset_list_matches_the_named_catalog() {
    let names = fab_preset_list();
    assert_eq!(names.len(), FAB_PRESET_NAMES.len());
    for (got, want) in names.iter().zip(FAB_PRESET_NAMES.iter()) {
        assert_eq!(*got, *want);
    }
}

#[test]
fn fab_preset_carries_vendor_minimums_and_passes_valid() -> T {
    let preset = fab_preset("JLCPCB 1-2L").ok_or("missing JLCPCB preset")?;
    assert_eq!(preset.name, "JLCPCB 1-2L");
    let rules = &preset.rules;
    assert!((rules.min_drill.unwrap_or(0.0) - 0.3).abs() < 1e-12);
    assert!((rules.plating_allowance.unwrap_or(0.0) - 0.025).abs() < 1e-12);
    assert!(rules.valid());
    assert!(fab_preset("not a vendor").is_none());
    for name in FAB_PRESET_NAMES {
        let preset = fab_preset(name).ok_or("listed preset missing")?;
        assert!(preset.rules.valid(), "preset {name} must validate");
    }
    Ok(())
}

#[test]
fn fab_rules_valid_accepts_vendor_bounds_and_rejects_just_outside() {
    let mut rules = FabRules::default();
    rules.min_track = Some(0.001);
    assert!(rules.valid());
    rules.min_track = Some(0.0009);
    assert!(!rules.valid());
    rules.min_track = Some(100.0);
    assert!(rules.valid());
    rules.min_track = Some(100.1);
    assert!(!rules.valid());
    rules.min_track = Some(0.127);
    rules.max_aspect_ratio = Some(1.0);
    assert!(rules.valid());
    rules.max_aspect_ratio = Some(50.0);
    assert!(rules.valid());
    rules.max_aspect_ratio = Some(50.1);
    assert!(!rules.valid());
}

#[test]
fn fab_rules_valid_rejects_out_of_range_numbers() {
    let mut ok = FabRules {
        min_track: Some(0.127),
        ..FabRules::default()
    };
    assert!(ok.valid());
    ok.min_track = Some(150.0);
    assert!(!ok.valid());
    ok.min_track = Some(f64::NAN);
    assert!(!ok.valid());
    ok.min_track = Some(0.127);
    ok.max_aspect_ratio = Some(100.0);
    assert!(!ok.valid());
}

#[test]
fn fab_rules_drill_tool_adds_plating_allowance() {
    let rules = FabRules {
        plating_allowance: Some(0.025),
        ..FabRules::default()
    };
    assert!((rules.drill_tool(0.3) - 0.325).abs() < 1e-12);
    assert!((FabRules::default().drill_tool(1.0) - 1.0).abs() < 1e-12);
}

#[test]
fn class_pair_rule_key_sorts_ids_and_valid_checks_bounds() {
    let rule = ClassPairRule {
        class_a: 5,
        class_b: 2,
        clearance: 0.25,
    };
    assert_eq!(rule.key(), (2, 5));
    assert!(rule.valid());
    let same = ClassPairRule {
        class_a: 3,
        class_b: 3,
        clearance: 0.2,
    };
    assert!(!same.valid());
    let zero = ClassPairRule {
        class_a: 0,
        class_b: 2,
        clearance: 0.2,
    };
    assert!(!zero.valid());
}
