//! The per-span drill table (issue #802): rows as data, the same rows in the
//! drill map sheet and the readme, and the tolerance attribute.

mod common;
use common::*;
use pcbcraft_io::{DrillFunction, SpanKind, drill_table, drill_table_lines};

/// The drill table lines the readme prints, header first.
fn readme_lines(readme: &str) -> Vec<String> {
    readme
        .split("DRILL TABLE\n")
        .nth(1)
        .map(|rest| {
            rest.lines()
                .take_while(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The drill table lines the drill map sheet carries as comments.
fn map_lines(map: &str) -> Vec<String> {
    map.lines()
        .filter_map(|l| l.strip_prefix("G04 DRILL TABLE "))
        .map(|l| {
            l.trim_end_matches(" *")
                .trim_start_matches("ROW ")
                .to_string()
        })
        .collect()
}

#[test]
fn the_table_has_one_row_per_span_function_and_diameter() -> T {
    let rows = drill_table(&golden_board()?)?;
    let summary: Vec<(u8, u8, SpanKind, bool, DrillFunction, u64, usize)> = rows
        .iter()
        .map(|r| {
            (
                r.from_layer,
                r.to_layer,
                r.kind,
                r.plated,
                r.function,
                (r.diameter_mm * 1000.0).round() as u64,
                r.count,
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (1, 2, SpanKind::Blind, true, DrillFunction::Via, 150, 1),
            (1, 4, SpanKind::Through, true, DrillFunction::Via, 200, 1),
            (1, 4, SpanKind::Through, true, DrillFunction::Via, 300, 3),
            (
                1,
                4,
                SpanKind::Through,
                true,
                DrillFunction::ComponentPad,
                800,
                1
            ),
            (
                1,
                4,
                SpanKind::Through,
                false,
                DrillFunction::Mounting,
                3200,
                1
            ),
            (2, 3, SpanKind::Buried, true, DrillFunction::Via, 150, 1),
        ],
        "three vias of one size share a row; a pad hole and a via of another size do not"
    );
    Ok(())
}

#[test]
fn a_two_layer_board_with_only_through_holes_gets_the_table_and_the_same_files() -> T {
    let mut p = board(2);
    p.board.vias = vec![via(10, 10.0, 0.3, None, None, None)?];
    p.board.vias.push(via(11, 12.0, 0.3, None, None, None)?);
    p.validate()?;
    let rows = drill_table(&p)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].count, 2);
    assert_eq!(rows[0].kind, SpanKind::Through);
    let files = pcbcraft_io::gerber_files(&p)?;
    assert!(
        files.keys().all(|k| !k.starts_with("pcb-Drill-L")),
        "no per-span files appear for a through-only board: {:?}",
        files.keys().collect::<Vec<_>>()
    );
    let plated = files.get("pcb-Plated.drl").ok_or("no plated file")?;
    assert!(
        !plated.contains("TA.DrillTolerance"),
        "no tolerance recorded, no attribute: {plated}"
    );
    Ok(())
}

#[test]
fn the_readme_and_the_drill_map_list_the_same_rows() -> T {
    let p = golden_board()?;
    let files = pcbcraft_io::gerber_files(&p)?;
    let expected = drill_table_lines(&drill_table(&p)?);
    assert_eq!(expected.len(), 7, "a header and six rows");
    let readme = files.get("README-fabrication.txt").ok_or("no readme")?;
    assert_eq!(readme_lines(readme), expected, "{readme}");
    let map = files.get("pcb-DrillMap.gbr").ok_or("no drill map")?;
    assert_eq!(map_lines(map), expected, "{map}");
    Ok(())
}

#[test]
fn a_drill_tolerance_is_written_only_when_the_board_records_one() -> T {
    let mut p = golden_board()?;
    let with = pcbcraft_io::gerber_files(&p)?;
    for name in ["pcb-Plated.drl", "pcb-NPTH.drl", "pcb-Drill-L1-L2.drl"] {
        let text = with.get(name).ok_or_else(|| format!("no {name}"))?;
        assert!(
            text.contains("; #@! TA.DrillTolerance,0.050,0.050\n"),
            "{name}: {text}"
        );
    }
    assert!(
        drill_table(&p)?
            .iter()
            .all(|r| r.tolerance_mm == Some(0.05))
    );
    p.board.machining = None;
    let without = pcbcraft_io::gerber_files(&p)?;
    for (name, text) in &without {
        if name.ends_with(".drl") {
            assert!(!text.contains("DrillTolerance"), "{name}: {text}");
        }
    }
    assert!(drill_table(&p)?.iter().all(|r| r.tolerance_mm.is_none()));
    Ok(())
}

#[test]
fn golden_drill_table_lines_for_the_four_layer_board() -> T {
    let lines = drill_table_lines(&drill_table(&golden_board()?)?);
    golden("drill_table_lines.csv", &(lines.join("\n") + "\n"))?;
    let files = pcbcraft_io::gerber_files(&golden_board()?)?;
    let map = files.get("pcb-DrillMap.gbr").ok_or("no drill map")?;
    golden("drill_table_map.gbr", map)
}

#[test]
fn the_table_counts_every_hole_on_two_four_and_six_layers() -> T {
    for layers in [2u8, 4, 6] {
        let last = layers - 1;
        let mut p = board(layers);
        let mut id = 10;
        let mut expect_vias = 0usize;
        let mut spans: Vec<Option<(u8, u8)>> = vec![None];
        if layers > 2 {
            spans.extend([
                Some((0, 1)),
                Some((1, layers - 2)),
                Some((layers - 2, last)),
            ]);
        }
        for (i, span) in spans.iter().enumerate() {
            for _ in 0..=(i + 1) {
                p.board
                    .vias
                    .push(via(id, 5.0 + id as f64, 0.3, *span, None, None)?);
                id += 1;
                expect_vias += 1;
            }
        }
        p.board.holes = vec![hole(90, 3.0, 3.0, 2.2), hole(91, 6.0, 3.0, 2.2)];
        p.validate()?;
        let rows = drill_table(&p)?;
        let vias: usize = rows
            .iter()
            .filter(|r| r.function == DrillFunction::Via)
            .map(|r| r.count)
            .sum();
        let mounting: usize = rows
            .iter()
            .filter(|r| r.function == DrillFunction::Mounting)
            .map(|r| r.count)
            .sum();
        assert_eq!(
            vias, expect_vias,
            "{layers} layers: every via is counted once"
        );
        assert_eq!(mounting, 2, "{layers} layers");
        let mut keys: Vec<_> = rows
            .iter()
            .map(|r| {
                (
                    r.from_layer,
                    r.to_layer,
                    r.plated,
                    r.function as u8,
                    (r.diameter_mm * 1000.0) as u64,
                )
            })
            .collect();
        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), before, "{layers} layers: no row repeats");
        let ordered = rows
            .windows(2)
            .all(|w| (w[0].from_layer, w[0].to_layer) <= (w[1].from_layer, w[1].to_layer));
        assert!(ordered, "{layers} layers: rows run in span order");
        let excellon_hits: usize = pcbcraft_io::gerber_files(&p)?
            .iter()
            .filter(|(n, _)| n.ends_with(".drl") && n.as_str() != "pcb-Drill.drl")
            .map(|(_, t)| {
                t.lines()
                    .filter(|l| l.starts_with('X') && !l.contains("G85"))
                    .count()
            })
            .sum();
        let total: usize = rows.iter().map(|r| r.count).sum();
        assert_eq!(
            excellon_hits, total,
            "{layers} layers: the files hold the holes the table counts"
        );
    }
    Ok(())
}
