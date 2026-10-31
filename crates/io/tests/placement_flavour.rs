//! Placement flavours on the showcase design with some parts moved to the bottom.
//!
//! Context: golden files per bundled flavour, a rotation property check under every
//! bottom-side policy, and the rule for parts that are not to be fitted.
//!
//! # Dependencies
//! - crates/io/src/placement_flavour.rs
//! - examples/showcase.pcbcraft
//! - crates/io/tests/fixtures/placement/

use pcbcraft_doc::{ComponentField, Layer, Project};
use pcbcraft_geom::Point;
use pcbcraft_io::{
    AngleRange, BottomRotation, DnpRule, PlacementFlavour, PlacementOptions, RotationOffset,
    bundled_placement_flavours, flavour_rotation, placement_flavoured,
};
use std::path::PathBuf;

/// References moved to the bottom face in the fixture.
const BOTTOM: [&str; 7] = ["R1", "R2", "C1", "D2", "R4", "U7", "J3"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The showcase with a bottom face, an auxiliary origin, a bottom fiducial and two
/// parts marked not to be fitted by field.
fn fixture() -> Result<Project, String> {
    let mut p = pcbcraft_io::load(&root().join("../../examples/showcase.pcbcraft"))?;
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
    p.board.drill_origin = Point::new(1.5, 2.0);
    if let Some(f) = p.board.fiducials.last_mut() {
        f.layer = Layer::Back;
    }
    for c in &mut p.components {
        let key = match c.reference.as_str() {
            "C5" => "DNP",
            "R5" => "Exclude from BOM",
            _ => continue,
        };
        c.fields.push(ComponentField {
            key: key.into(),
            value: "yes".into(),
        });
    }
    p.validate()?;
    Ok(p)
}

fn flavour(id: &str) -> Result<PlacementFlavour, String> {
    bundled_placement_flavours()?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| format!("no bundled flavour {id}"))
}

fn check_golden(name: &str, text: &str) -> Result<(), String> {
    let path = root().join("tests/fixtures/placement").join(name);
    if std::env::var("PCBCRAFT_BLESS").is_ok() {
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
    }
    let want = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
    if want == text {
        Ok(())
    } else {
        Err(format!("{name} differs from its golden file"))
    }
}

fn golden(id: &str) -> Result<(), String> {
    let out = placement_flavoured(&fixture()?, &flavour(id)?, &PlacementOptions::default())?;
    for (name, text) in &out.files {
        check_golden(&format!("{id}.{name}"), text)?;
    }
    Ok(())
}

#[test]
fn golden_cpl_mm() -> Result<(), String> {
    golden("cpl_mm")
}

#[test]
fn golden_cpl_inch_split_by_side() -> Result<(), String> {
    golden("cpl_inch")
}

#[test]
fn golden_xyrs_mil() -> Result<(), String> {
    golden("xyrs_mil")
}

#[test]
fn golden_bottom_view_mm() -> Result<(), String> {
    golden("bottom_view_mm")
}

#[test]
fn every_bundled_flavour_is_valid_and_ids_are_unique() -> Result<(), String> {
    let all = bundled_placement_flavours()?;
    assert!(all.len() >= 4);
    let mut ids: Vec<&str> = all.iter().map(|f| f.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), all.len());
    Ok(())
}

#[test]
fn split_flavour_puts_each_part_in_the_file_of_its_side() -> Result<(), String> {
    let out = placement_flavoured(
        &fixture()?,
        &flavour("cpl_inch")?,
        &PlacementOptions::default(),
    )?;
    assert_eq!(out.files.len(), 2);
    let top = &out.files[0].1;
    let bottom = &out.files[1].1;
    for r in ["R1", "R2", "C1", "D2", "R4", "U7"] {
        assert!(
            bottom.contains(&format!("\n{r},")),
            "{r} belongs on the bottom"
        );
        assert!(!top.contains(&format!("\n{r},")), "{r} must not be on top");
    }
    assert!(top.contains("\nU1,"));
    Ok(())
}

#[test]
fn smt_only_leaves_through_hole_parts_out_and_other_flavours_keep_them() -> Result<(), String> {
    let p = fixture()?;
    let mm = placement_flavoured(&p, &flavour("cpl_mm")?, &PlacementOptions::default())?;
    assert!(!mm.files[0].1.contains("\nJ1,"));
    let mil = placement_flavoured(&p, &flavour("xyrs_mil")?, &PlacementOptions::default())?;
    assert!(mil.files[0].1.contains("\nJ1\t"));
    Ok(())
}

#[test]
fn fiducials_are_listed_only_where_the_flavour_asks() -> Result<(), String> {
    let p = fixture()?;
    let n = p.board.fiducials.len();
    let with = placement_flavoured(&p, &flavour("xyrs_mil")?, &PlacementOptions::default())?;
    assert_eq!(with.files[0].1.matches("\nFID").count(), n);
    let without = placement_flavoured(&p, &flavour("cpl_mm")?, &PlacementOptions::default())?;
    assert_eq!(without.files[0].1.matches("\nFID").count(), 0);
    Ok(())
}

fn with_rule(id: &str, rule: DnpRule) -> Result<PlacementFlavour, String> {
    let mut f = flavour(id)?;
    f.dnp = rule;
    if rule == DnpRule::Flag
        && !f
            .columns
            .iter()
            .any(|c| c.field == pcbcraft_io::ColumnField::Populate)
    {
        f.columns.push(pcbcraft_io::PlacementColumn {
            header: "Populate".into(),
            field: pcbcraft_io::ColumnField::Populate,
        });
    }
    Ok(f)
}

#[test]
fn excluded_and_unpopulated_parts_follow_the_flavour_rule() -> Result<(), String> {
    let p = fixture()?;
    // C5 and R5 are excluded by field; the Lean variant leaves out C14 (id 69).
    let lean = PlacementOptions {
        variant: Some("Lean".into()),
        ..PlacementOptions::default()
    };
    let held = ["C5", "R5", "C14"];
    for base in ["cpl_mm", "xyrs_mil"] {
        let dropped = placement_flavoured(&p, &with_rule(base, DnpRule::Drop)?, &lean)?;
        let kept = placement_flavoured(&p, &with_rule(base, DnpRule::Keep)?, &lean)?;
        let flagged = placement_flavoured(&p, &with_rule(base, DnpRule::Flag)?, &lean)?;
        let delim = flavour(base)?.delimiter;
        for r in held {
            let row = format!("\n{r}{delim}");
            assert!(
                !dropped.files[0].1.contains(&row),
                "{base}: {r} must be dropped"
            );
            assert!(kept.files[0].1.contains(&row), "{base}: {r} must be kept");
            let line = flagged.files[0]
                .1
                .lines()
                .find(|l| l.starts_with(&format!("{r}{delim}")))
                .ok_or_else(|| format!("{base}: {r} must be flagged, not dropped"))?;
            assert!(
                line.ends_with(&format!("{delim}N")),
                "{base}: {r} is marked N: {line}"
            );
        }
        let fitted = flagged.files[0]
            .1
            .lines()
            .find(|l| l.starts_with(&format!("U1{delim}")))
            .ok_or("U1 missing")?;
        assert!(fitted.ends_with(&format!("{delim}Y")));
    }
    // Without the variant only the field-marked parts are held back.
    let plain = placement_flavoured(
        &p,
        &with_rule("cpl_mm", DnpRule::Drop)?,
        &PlacementOptions::default(),
    )?;
    assert!(plain.files[0].1.contains("\nC14,"));
    assert!(!plain.files[0].1.contains("\nC5,"));
    assert!(
        placement_flavoured(
            &p,
            &flavour("cpl_mm")?,
            &PlacementOptions {
                variant: Some("Nope".into()),
                ..PlacementOptions::default()
            }
        )
        .is_err()
    );
    Ok(())
}

/// Angle written for a part turned by `delta` degrees: the independent oracle is
/// the policy table written out by hand, not the function under test.
fn expected(stored: f64, bottom: bool, policy: BottomRotation) -> f64 {
    let r = match (bottom, policy) {
        (true, BottomRotation::Negate) => -stored,
        (true, BottomRotation::Mirror) => 180.0 - stored,
        _ => stored,
    };
    let mut r = r % 360.0;
    if r < 0.0 {
        r += 360.0;
    }
    r
}

fn circular_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    if d > 180.0 { d - 360.0 } else { d }
}

#[test]
fn turning_a_part_ninety_degrees_moves_the_written_angle_consistently() {
    // Top: +90. Bottom: +90 as placed, -90 negated, -90 mirrored.
    let cases = [
        (false, BottomRotation::AsPlaced, 90.0),
        (false, BottomRotation::Negate, 90.0),
        (false, BottomRotation::Mirror, 90.0),
        (true, BottomRotation::AsPlaced, 90.0),
        (true, BottomRotation::Negate, -90.0),
        (true, BottomRotation::Mirror, -90.0),
    ];
    let mut seed = 12345_u64;
    for (bottom, policy, step) in cases {
        for range in [AngleRange::ZeroTo360, AngleRange::Signed180] {
            for i in 0..400 {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let stored = f64::from(((seed >> 33) % 72_000) as u32) / 100.0 - 360.0;
                let offset = if i % 3 == 0 {
                    0.0
                } else {
                    f64::from((i % 7) as u32) * 15.0
                };
                let a = flavour_rotation(stored, offset, bottom, policy, range);
                let b = flavour_rotation(stored + 90.0, offset, bottom, policy, range);
                assert!(
                    (circular_diff(b, a) - step).abs() < 1e-9
                        || (circular_diff(b, a).abs() - 180.0).abs() < 1e-9 && step.abs() == 180.0,
                    "{bottom} {policy:?} {stored}: {a} -> {b}"
                );
                match range {
                    AngleRange::ZeroTo360 => assert!((0.0..360.0).contains(&a)),
                    AngleRange::Signed180 => assert!(a > -180.0 && a <= 180.0),
                }
                let want = expected(stored + offset, bottom, policy);
                assert!(circular_diff(a, want).abs() < 1e-9, "{a} vs {want}");
            }
        }
    }
}

#[test]
fn the_written_file_follows_the_same_rotation_rule_on_both_faces() -> Result<(), String> {
    for policy in [
        BottomRotation::AsPlaced,
        BottomRotation::Negate,
        BottomRotation::Mirror,
    ] {
        let mut f = flavour("cpl_mm")?;
        f.bottom_rotation = policy;
        f.decimals = 3;
        let read = |p: &Project| -> Result<(f64, f64), String> {
            let out = placement_flavoured(p, &f, &PlacementOptions::default())?;
            let text = &out.files[0].1;
            let angle = |r: &str| -> Result<f64, String> {
                text.lines()
                    .find(|l| l.starts_with(&format!("{r},")))
                    .and_then(|l| l.split(',').nth(3))
                    .and_then(|v| v.parse::<f64>().ok())
                    .ok_or_else(|| format!("no angle for {r}"))
            };
            Ok((angle("R3")?, angle("R1")?))
        };
        let base = fixture()?;
        let mut turned = base.clone();
        let ids: Vec<u64> = turned
            .components
            .iter()
            .filter(|c| c.reference == "R3" || c.reference == "R1")
            .map(|c| c.id)
            .collect();
        for fp in &mut turned.board.footprints {
            if ids.contains(&fp.component) {
                fp.rotation += 90.0;
            }
        }
        let (top0, bottom0) = read(&base)?;
        let (top1, bottom1) = read(&turned)?;
        assert!(
            (circular_diff(top1, top0) - 90.0).abs() < 1e-6,
            "top under {policy:?}"
        );
        let want = if policy == BottomRotation::AsPlaced {
            90.0
        } else {
            -90.0
        };
        assert!(
            (circular_diff(bottom1, bottom0) - want).abs() < 1e-6,
            "bottom under {policy:?}"
        );
    }
    Ok(())
}

#[test]
fn rotation_offsets_apply_before_the_bottom_policy() -> Result<(), String> {
    let mut f = flavour("cpl_mm")?;
    f.bottom_rotation = BottomRotation::Negate;
    f.decimals = 1;
    let p = fixture()?;
    let line = |o: &PlacementOptions| -> Result<String, String> {
        let out = placement_flavoured(&p, &f, o)?;
        out.files[0]
            .1
            .lines()
            .find(|l| l.starts_with("R1,"))
            .map(str::to_string)
            .ok_or_else(|| "R1 missing".to_string())
    };
    // R1 is Chip_0402 at 0 degrees on the bottom: offset 90 gives -90, i.e. 270.
    assert!(line(&PlacementOptions::default())?.ends_with(",0.0,Bottom"));
    let o = PlacementOptions {
        offsets: vec![RotationOffset {
            footprint: "Chip_04".into(),
            offset_deg: 90.0,
        }],
        ..PlacementOptions::default()
    };
    assert!(line(&o)?.ends_with(",270.0,Bottom"));
    Ok(())
}

#[test]
fn a_flavour_record_with_a_bad_value_is_refused() -> Result<(), String> {
    let mut f = flavour("cpl_mm")?;
    f.delimiter = "|".into();
    assert!(f.validate().is_err());
    let mut f = flavour("cpl_mm")?;
    f.dnp = DnpRule::Flag;
    assert!(f.validate().is_err(), "a populate flag needs its column");
    Ok(())
}
