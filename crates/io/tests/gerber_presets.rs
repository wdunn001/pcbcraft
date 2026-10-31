//! Golden, round-trip and property tests for the Gerber naming presets and
//! drill options (issue #225).
//!
//! Goldens list each file name with its X2 file function, so they do not move
//! when a plotter changes what a copper file draws. Set `UPDATE_GOLDEN=1` to
//! rewrite them.
use pcbcraft_doc::{LayerRecord, Project};
use pcbcraft_io::gerber_preset::{
    DrillUnits, GerberOptions, NAMING_PRESETS, NamingPreset, ZeroSuppression, gerber_files_with,
    preview_names, read_drill_file,
};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sample() -> Result<Project, String> {
    pcbcraft_io::load(&root().join("tests/fixtures/sample_led_panel_base.json"))
}

/// The showcase board without its pour fills: on this base the plotter
/// refuses the filled pours (they exceed its object limit), a limit this
/// work leaves alone.
fn showcase() -> Result<Project, String> {
    let mut p = pcbcraft_io::load(&root().join("../../examples/showcase.pcbcraft"))?;
    p.board.zones.clear();
    Ok(p)
}

fn function_of(text: &str) -> String {
    text.lines()
        .find_map(|l| {
            let l = l.strip_prefix("; #@! ").unwrap_or(l);
            let l = l.strip_prefix('%').unwrap_or(l);
            l.strip_prefix("TF.FileFunction,")
                .map(|v| v.trim_end_matches("*%").trim_end_matches('*').to_owned())
        })
        .unwrap_or_else(|| "-".into())
}

fn manifest(p: &Project, preset: NamingPreset) -> Result<String, String> {
    let files = gerber_files_with(p, &GerberOptions::for_preset(preset))?;
    let mut out = String::new();
    for (name, text) in &files {
        out.push_str(&format!("{name}\t{}\n", function_of(text)));
    }
    Ok(out)
}

fn check_golden(file: &str, got: &str) -> Result<(), String> {
    let path = root().join("tests/fixtures/gerber_presets").join(file);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, got).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let want = std::fs::read_to_string(&path).map_err(|e| format!("{file}: {e}"))?;
    assert_eq!(
        got, want,
        "golden {file} differs; UPDATE_GOLDEN=1 rewrites it"
    );
    Ok(())
}

#[test]
fn each_preset_matches_its_golden_on_the_sample_and_the_showcase() -> Result<(), String> {
    for preset in NAMING_PRESETS {
        check_golden(
            &format!("sample-{}.txt", preset.id()),
            &manifest(&sample()?, preset)?,
        )?;
        check_golden(
            &format!("showcase-{}.txt", preset.id()),
            &manifest(&showcase()?, preset)?,
        )?;
    }
    Ok(())
}

#[test]
fn renaming_leaves_every_file_function_unchanged() -> Result<(), String> {
    let p = showcase()?;
    let project: Vec<String> = gerber_files_with(&p, &GerberOptions::default())?
        .values()
        .map(|t| function_of(t))
        .collect();
    for preset in [NamingPreset::Plain, NamingPreset::Protel] {
        let mut got: Vec<String> = gerber_files_with(&p, &GerberOptions::for_preset(preset))?
            .values()
            .map(|t| function_of(t))
            .collect();
        let mut want = project.clone();
        got.sort();
        want.sort();
        assert_eq!(got, want, "{}", preset.id());
    }
    Ok(())
}

fn drill_golden(name: &str, options: &GerberOptions) -> Result<(), String> {
    let files = gerber_files_with(&sample_with_holes()?, options)?;
    let mut out = String::new();
    for (n, text) in &files {
        if n.ends_with(".drl") || n.ends_with(".XLN") {
            out.push_str(&format!("--- {n}\n{text}"));
        }
    }
    check_golden(name, &out)
}

fn sample_with_holes() -> Result<Project, String> {
    let mut p = showcase()?;
    p.board.drill_origin = pcbcraft_geom::Point::new(1.0, 2.0);
    Ok(p)
}

#[test]
fn drill_options_match_their_goldens() -> Result<(), String> {
    drill_golden(
        "drill-merged-mm-decimal.txt",
        &GerberOptions {
            merge_drills: true,
            units: DrillUnits::Mm,
            ..GerberOptions::default()
        },
    )?;
    drill_golden(
        "drill-split-inch-leading-24.txt",
        &GerberOptions {
            units: DrillUnits::Inch,
            zeros: ZeroSuppression::Leading,
            format: Some((2, 4)),
            ..GerberOptions::default()
        },
    )?;
    drill_golden(
        "drill-split-mm-trailing-33.txt",
        &GerberOptions {
            units: DrillUnits::Mm,
            zeros: ZeroSuppression::Trailing,
            format: Some((3, 3)),
            ..GerberOptions::default()
        },
    )
}

fn sorted(mut v: Vec<(i64, i64, i64)>) -> Vec<(i64, i64, i64)> {
    v.sort();
    v
}

/// Holes as (diameter, x, y) in micrometres, from every drill file.
fn holes_of(
    files: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<(i64, i64, i64)>, String> {
    let mut out = Vec::new();
    for (name, text) in files {
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with(".drl") || lower.ends_with(".xln")) || lower.contains("rout") {
            continue;
        }
        if lower.contains("via_l")
            || lower.contains("drill-l")
            || lower.contains("-l") && lower.ends_with(".xln")
        {
            continue;
        }
        for h in read_drill_file(text)? {
            out.push((
                (h.diameter_mm * 1000.0).round() as i64,
                (h.x_mm * 1000.0).round() as i64,
                (h.y_mm * 1000.0).round() as i64,
            ));
        }
    }
    Ok(sorted(out))
}

#[test]
fn every_drill_style_reads_back_to_the_same_holes() -> Result<(), String> {
    let p = sample_with_holes()?;
    let oracle = holes_of(&gerber_files_with(
        &p,
        &GerberOptions {
            merge_drills: true,
            ..GerberOptions::default()
        },
    )?)?;
    assert!(!oracle.is_empty(), "showcase has holes");
    for merge in [false, true] {
        for units in [DrillUnits::Mm, DrillUnits::Inch] {
            for zeros in [
                ZeroSuppression::Decimal,
                ZeroSuppression::Leading,
                ZeroSuppression::Trailing,
            ] {
                let options = GerberOptions {
                    merge_drills: merge,
                    units,
                    zeros,
                    format: Some((3, 5)),
                    ..GerberOptions::default()
                };
                let got = holes_of(&gerber_files_with(&p, &options)?)?;
                assert_eq!(got.len(), oracle.len(), "{merge} {units:?} {zeros:?}");
                for (a, b) in got.iter().zip(&oracle) {
                    // Decimal inch carries 4 places: 2.5 micrometres of slack.
                    assert!(
                        (a.0 - b.0).abs() <= 3 && (a.1 - b.1).abs() <= 3 && (a.2 - b.2).abs() <= 3,
                        "{merge} {units:?} {zeros:?}: {a:?} vs {b:?}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn merged_drill_holds_both_hole_sets() -> Result<(), String> {
    let p = showcase()?;
    let split = gerber_files_with(&p, &GerberOptions::for_preset(NamingPreset::Protel))?;
    let merged = gerber_files_with(
        &p,
        &GerberOptions {
            naming: NamingPreset::Protel,
            merge_drills: true,
            ..GerberOptions::default()
        },
    )?;
    let stem = split
        .keys()
        .find_map(|k| k.strip_suffix(".GTL"))
        .ok_or("no top copper")?
        .to_owned();
    assert!(split.contains_key(&format!("{stem}.XLN")));
    assert!(split.contains_key(&format!("{stem}-NPTH.XLN")));
    assert!(!merged.contains_key(&format!("{stem}-NPTH.XLN")));
    let count = |m: &std::collections::BTreeMap<String, String>, n: &str| {
        m.get(n)
            .map(|t| read_drill_file(t).map(|h| h.len()))
            .transpose()
    };
    let pth = count(&split, &format!("{stem}.XLN"))?.unwrap_or(0);
    let npth = count(&split, &format!("{stem}-NPTH.XLN"))?.unwrap_or(0);
    assert!(pth > 0 && npth > 0, "showcase has both kinds");
    assert_eq!(count(&merged, &format!("{stem}.XLN"))?, Some(pth + npth));
    Ok(())
}

#[test]
fn options_turn_the_drill_map_job_and_readme_off() -> Result<(), String> {
    let p = sample()?;
    let all = gerber_files_with(&p, &GerberOptions::default())?;
    assert!(all.contains_key("pcb-DrillMap.gbr") && all.contains_key("pcb-job.gbrjob"));
    let bare = gerber_files_with(
        &p,
        &GerberOptions {
            drill_map: false,
            job_file: false,
            readme: false,
            ..GerberOptions::default()
        },
    )?;
    assert!(!bare.contains_key("pcb-DrillMap.gbr"));
    assert!(!bare.contains_key("pcb-job.gbrjob"));
    assert!(!bare.contains_key("README-fabrication.txt"));
    Ok(())
}

#[test]
fn the_job_file_lists_the_renamed_files() -> Result<(), String> {
    let files = gerber_files_with(
        &showcase()?,
        &GerberOptions::for_preset(NamingPreset::Protel),
    )?;
    let job = files
        .iter()
        .find(|(n, _)| n.ends_with(".gbrjob"))
        .map(|(_, t)| t)
        .ok_or("no job file")?;
    for name in files
        .keys()
        .filter(|n| n.ends_with(".GTL") || n.ends_with(".G2L"))
    {
        assert!(
            job.contains(&format!("\"{name}\"")),
            "{name} missing from job"
        );
    }
    assert!(!job.contains("pcb-F.Cu.gbr"));
    Ok(())
}

fn with_layers(base: &Project, copper: usize) -> Result<Project, String> {
    let mut p = base.clone();
    p.board.tracks.clear();
    p.board.vias.clear();
    p.board.zones.clear();
    p.board.footprints.clear();
    let rec = |name: String, kind: &str, t: f64| LayerRecord {
        name,
        kind: kind.into(),
        thickness: t,
        copper_role: None,
        function: None,
        side: None,
        polarity: None,
        material: None,
        dk: None,
        df: None,
        thickness_tol_pct: None,
        copper_weight_oz: None,
        foil: None,
        linked_copper: None,
    };
    let mut rows = Vec::new();
    for i in 0..copper {
        let name = if i == 0 {
            "F.Cu".to_owned()
        } else if i + 1 == copper {
            "B.Cu".to_owned()
        } else {
            format!("In{i}.Cu")
        };
        rows.push(rec(name, "copper", 0.035));
        if i + 1 < copper {
            rows.push(rec(format!("Core {i}"), "core", 0.2));
        }
    }
    p.board.layer_records = rows;
    p.validate()?;
    Ok(p)
}

#[test]
fn every_copper_layer_gets_exactly_one_file_with_the_right_name() -> Result<(), String> {
    let base = sample()?;
    for copper in [2usize, 4, 6] {
        let p = with_layers(&base, copper)?;
        assert_eq!(usize::from(p.copper_layer_count()), copper);
        for preset in NAMING_PRESETS {
            let files = gerber_files_with(&p, &GerberOptions::for_preset(preset))?;
            let stem = "LED_indicator";
            let want: Vec<String> = preview_names(preset, stem, copper as u8, false)
                .into_iter()
                .take(copper)
                .collect();
            let copper_files: BTreeSet<&String> = files
                .iter()
                .filter(|(_, t)| function_of(t).starts_with("Copper,"))
                .map(|(n, _)| n)
                .collect();
            assert_eq!(copper_files.len(), copper, "{} x{copper}", preset.id());
            for (i, name) in want.iter().enumerate() {
                assert!(
                    files.contains_key(name),
                    "{} x{copper}: {name}",
                    preset.id()
                );
                let f = function_of(&files[name]);
                assert!(
                    f.starts_with(&format!("Copper,L{},", i + 1)),
                    "{name} says {f}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn bad_digit_counts_are_refused() {
    let o = GerberOptions {
        format: Some((7, 3)),
        ..GerberOptions::default()
    };
    assert!(o.validate().is_err());
}
