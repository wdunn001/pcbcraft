//! Pick and place machine profiles with nozzle rules: job files and plans for the
//! sample and the showcase, and the rule behaviour in plain cases.
//!
//! Context: the YY1 job file has no nozzle column, so the nozzle shows as the head and
//! as nozzle-change rows; the plan behind it is `plan_placement`.
//!
//! # Dependencies
//! - crates/io/src/neoden_yy1.rs, crates/doc/src/placement_machine.rs
//! - machine-profiles/neoden-yy1.toml, machine-profiles/neoden-4.toml
//! - examples/showcase.pcbcraft, examples/led-indicator.json
//! - crates/io/tests/fixtures/neoden/

use pcbcraft_doc::{
    ComponentField, FIELD_NOZZLE, Layer, MachineProfile, PartFacts, PartPlacement,
    PickPlaceSection, Project, matching_rule, plan_placement,
};
use pcbcraft_io::{
    NeodenYy1Options, YyLayout, YySide, builtin_machine_profile, builtin_machine_profile_ids,
    placement_neoden_yy1,
};
use std::path::PathBuf;

const BOTTOM: [&str; 3] = ["R1", "U7", "SW3"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn showcase() -> Result<Project, String> {
    pcbcraft_io::load(&root().join("../../examples/showcase.pcbcraft"))
}

/// The showcase with three parts on the bottom face.
fn two_sided() -> Result<Project, String> {
    let mut p = showcase()?;
    let ids: Vec<u64> = p
        .components
        .iter()
        .filter(|c| BOTTOM.contains(&c.reference.as_str()))
        .map(|c| c.id)
        .collect();
    for f in &mut p.board.footprints {
        if ids.contains(&f.component) {
            f.side = Some(Layer::Back);
        }
    }
    if let Some(f) = p.board.fiducials.last_mut() {
        f.layer = Layer::Back;
    }
    Ok(p)
}

fn yy1() -> Result<MachineProfile, String> {
    builtin_machine_profile("neoden-yy1")
}

fn native() -> NeodenYy1Options {
    NeodenYy1Options {
        layout: YyLayout::Native,
        variant: None,
        include_dnp_as_skip: false,
    }
}

fn golden(name: &str, bytes: &[u8]) -> Result<(), String> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?;
    let path = root().join("tests/fixtures/neoden").join(name);
    if std::env::var("PCBCRAFT_BLESS").is_ok() {
        std::fs::write(&path, &text).map_err(|e| e.to_string())?;
    }
    let want = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
    if want == text {
        Ok(())
    } else {
        Err(format!("{name} differs from its golden file"))
    }
}

fn write_all(p: &Project, side: YySide, prefix: &str) -> Result<(), String> {
    let out = placement_neoden_yy1(p, side, &yy1()?, &native())?;
    for (name, bytes) in &out.files {
        golden(&format!("{prefix}.{name}"), bytes)?;
    }
    Ok(())
}

#[test]
fn golden_showcase_job_both_sides() -> Result<(), String> {
    write_all(&two_sided()?, YySide::Both, "showcase")
}

#[test]
fn golden_showcase_job_with_a_nozzle_override() -> Result<(), String> {
    let mut p = two_sided()?;
    for c in &mut p.components {
        if c.reference == "U4" {
            c.fields.push(ComponentField {
                key: FIELD_NOZZLE.into(),
                value: "CN400".into(),
            });
        }
    }
    write_all(&p, YySide::Top, "showcase-override")
}

/// Pad spans measured from the generated land patterns, per package family.
const PACKAGES: [(&str, f64); 30] = [
    ("0201", 1.0),
    ("0402", 1.6),
    ("0603", 2.5),
    ("0805", 3.0),
    ("1206", 4.2),
    ("1210", 4.2),
    ("1812", 5.6),
    ("2010", 6.2),
    ("2512", 7.5),
    ("SOD-123", 4.3),
    ("SOD-323", 2.9),
    ("SMA", 6.0),
    ("SMB", 6.0),
    ("SMC", 8.5),
    ("SOT-23", 3.2),
    ("SOT-23-5", 3.2),
    ("SOT-89", 4.8),
    ("SOT-223", 7.0),
    ("TO-252", 9.5),
    ("SOIC-8", 6.6),
    ("TSSOP-20", 6.9),
    ("DFN-8", 3.0),
    ("QFN-32", 5.9),
    ("QFN-48", 7.4),
    ("QFP-48", 9.0),
    ("LQFP-100", 15.5),
    ("Crystal_3225", 4.0),
    ("Tact_4p5", 6.5),
    ("USB_C_Receptacle", 9.2),
    ("BGA-256", 17.0),
];

#[test]
fn every_bundled_package_size_maps_to_a_nozzle_on_each_bundled_profile() -> Result<(), String> {
    let mut seen = 0;
    for id in builtin_machine_profile_ids() {
        let profile = builtin_machine_profile(id)?;
        if profile.kind != "pick_place" {
            continue;
        }
        seen += 1;
        let pp = &profile.pick_place;
        for (package, span) in PACKAGES {
            let facts = PartFacts {
                package: package.into(),
                size_mm: span,
                height_mm: None,
            };
            let rule = matching_rule(pp, &facts)
                .and_then(|i| pp.placement_rule.get(i))
                .ok_or_else(|| format!("{id}: no rule for {package}"))?;
            assert!(
                pp.nozzle.iter().any(|n| n.id == rule.nozzle),
                "{id}: {package} names nozzle {} which is not listed",
                rule.nozzle
            );
        }
        let plan = plan_placement(&showcase()?, pp, None);
        assert!(
            plan.problems.iter().all(|q| q.code != "placement_nozzle"),
            "{id}: {:?}",
            plan.problems
        );
    }
    assert!(seen >= 2, "expected two pick and place profiles");
    Ok(())
}

#[test]
fn the_showcase_gets_the_vendor_nozzle_for_each_package() -> Result<(), String> {
    let p = showcase()?;
    let plan = plan_placement(&p, &yy1()?.pick_place, None);
    let nozzle = |reference: &str| {
        plan.parts
            .iter()
            .find(|q| q.reference == reference)
            .and_then(|q| q.resolved.nozzle.clone())
    };
    assert_eq!(nozzle("R1").as_deref(), Some("CN040"), "0402");
    assert_eq!(nozzle("C1").as_deref(), Some("CN065"), "0603");
    assert_eq!(nozzle("U1").as_deref(), Some("CN140"), "SOT-23-5");
    assert_eq!(nozzle("U4").as_deref(), Some("CN220"), "SOIC-8");
    assert_eq!(
        nozzle("U3").as_deref(),
        Some("CN220"),
        "QFN-32 body under 5 mm"
    );
    Ok(())
}

#[test]
fn an_override_beats_the_rule() -> Result<(), String> {
    let mut p = showcase()?;
    for c in &mut p.components {
        if c.reference == "R1" {
            c.fields.push(ComponentField {
                key: FIELD_NOZZLE.into(),
                value: "CN065".into(),
            });
            c.fields.push(ComponentField {
                key: "Placement speed".into(),
                value: "40".into(),
            });
        }
    }
    let plan = plan_placement(&p, &yy1()?.pick_place, None);
    let r1 = plan
        .parts
        .iter()
        .find(|q| q.reference == "R1")
        .ok_or("R1 missing")?;
    assert_eq!(r1.resolved.nozzle.as_deref(), Some("CN065"));
    assert!(r1.resolved.nozzle_overridden);
    assert_eq!(r1.resolved.speed_pct, 40);
    let r2 = plan
        .parts
        .iter()
        .find(|q| q.reference == "R2")
        .ok_or("R2 missing")?;
    assert_eq!(r2.resolved.nozzle.as_deref(), Some("CN040"));
    assert!(!r2.resolved.nozzle_overridden);
    Ok(())
}

#[test]
fn the_rules_are_data_so_editing_a_row_changes_the_choice() -> Result<(), String> {
    let mut profile = yy1()?;
    let row = profile
        .pick_place
        .placement_rule
        .iter_mut()
        .find(|r| r.package == "0402")
        .ok_or("no 0402 rule")?;
    row.nozzle = "CN065".into();
    profile.validate()?;
    let plan = plan_placement(&showcase()?, &profile.pick_place, None);
    let r1 = plan
        .parts
        .iter()
        .find(|q| q.reference == "R1")
        .ok_or("R1")?;
    assert_eq!(r1.resolved.nozzle.as_deref(), Some("CN065"));
    Ok(())
}

#[test]
fn a_rule_naming_an_unlisted_nozzle_is_refused() -> Result<(), String> {
    let mut profile = yy1()?;
    profile.pick_place.placement_rule[3].nozzle = "CN999".into();
    assert!(profile.validate().is_err());
    Ok(())
}

#[test]
fn a_part_with_no_suitable_nozzle_is_a_problem() -> Result<(), String> {
    let mut pp: PickPlaceSection = yy1()?.pick_place;
    pp.placement_rule.clear();
    let plan = plan_placement(&showcase()?, &pp, None);
    assert!(
        plan.problems
            .iter()
            .any(|q| q.code == "placement_nozzle" && q.message.contains("R1")),
        "{:?}",
        plan.problems
    );
    Ok(())
}

#[test]
fn an_override_naming_a_nozzle_the_machine_lacks_is_a_problem() -> Result<(), String> {
    let mut p = showcase()?;
    for c in &mut p.components {
        if c.reference == "R1" {
            c.fields.push(ComponentField {
                key: FIELD_NOZZLE.into(),
                value: "XN99".into(),
            });
        }
    }
    let plan = plan_placement(&p, &yy1()?.pick_place, None);
    assert!(plan.problems.iter().any(|q| q.message.contains("XN99")));
    Ok(())
}

#[test]
fn more_part_types_than_feeders_is_a_problem() -> Result<(), String> {
    let mut pp: PickPlaceSection = yy1()?.pick_place;
    for bank in &mut pp.feeders {
        if bank.name == "tape" {
            bank.last = bank.first + 1;
        }
    }
    let plan = plan_placement(&showcase()?, &pp, None);
    assert!(
        plan.problems.iter().any(|q| q.code == "placement_feeders"),
        "{:?}",
        plan.problems
    );
    Ok(())
}

#[test]
fn more_nozzle_types_than_slots_is_a_problem() -> Result<(), String> {
    let mut pp: PickPlaceSection = yy1()?.pick_place;
    pp.nozzle_slots = 2;
    let plan = plan_placement(&showcase()?, &pp, None);
    assert!(
        plan.problems
            .iter()
            .any(|q| q.code == "placement_nozzle_types")
    );
    Ok(())
}

#[test]
fn a_test_point_is_left_out_by_default() -> Result<(), String> {
    let plan = plan_placement(&showcase()?, &yy1()?.pick_place, None);
    let tp = plan
        .parts
        .iter()
        .find(|q| q.reference == "TP1")
        .ok_or("TP1")?;
    assert!(tp.skip);
    Ok(())
}

#[test]
fn the_override_properties_are_read_as_typed_values() -> Result<(), String> {
    let mut p = showcase()?;
    let c = &mut p.components[0];
    c.fields.push(ComponentField {
        key: "Placement feeder".into(),
        value: "7".into(),
    });
    c.fields.push(ComponentField {
        key: "Placement pick height".into(),
        value: "-0.5".into(),
    });
    let over = PartPlacement::from_component(c)?;
    assert_eq!(over.feeder, Some(7));
    assert_eq!(over.pick_height_mm, Some(-0.5));
    c.fields.push(ComponentField {
        key: "Placement speed".into(),
        value: "250".into(),
    });
    assert!(PartPlacement::from_component(c).is_err());
    Ok(())
}
