//! Board import of current-generation files: nets by name, six copper
//! layers, blind and buried vias, keepouts, stackup rows and 3D references.
//!
//! The fixtures are small files written for this repository.
use pcbcraft_io::{
    kicad_board, read_board_interchange, read_board_interchange_report,
    read_board_interchange_with_rules,
};

const MODERN: &str = include_str!("fixtures/board_import_modern_6layer.kicad_pcb");
const MODERN_8: &str = include_str!("fixtures/board_import_modern_8layer.kicad_pcb");
const ORDINAL: &str = include_str!("fixtures/board_import_ordinal_nets.kicad_pcb");

fn net_names(p: &pcbcraft_doc::Project) -> Vec<String> {
    let mut v: Vec<String> = p.nets.iter().map(|n| n.name.clone()).collect();
    v.sort();
    v
}

#[test]
fn nets_named_in_objects_import_like_ordinal_nets() -> Result<(), String> {
    let modern = read_board_interchange(MODERN)?;
    let ordinal = read_board_interchange(ORDINAL)?;
    assert_eq!(net_names(&modern), net_names(&ordinal));
    assert_eq!(net_names(&modern), ["+3V3", "GND", "SIG/A"]);
    // Pads carry the net by name.
    let r1 = modern
        .components
        .iter()
        .find(|c| c.reference == "R1")
        .ok_or("R1 missing")?;
    assert!(r1.pins.iter().all(|p| p.net.is_some()));
    Ok(())
}

#[test]
fn empty_net_name_means_no_net_on_a_pad() -> Result<(), String> {
    let text = ORDINAL.replace(
        "(net 1 \"GND\"))\n\t\t(pad \"\"",
        "(net 0 \"\"))\n\t\t(pad \"\"",
    );
    let p = read_board_interchange(&text)?;
    assert!(p.nets.iter().any(|n| n.name == "GND"));
    let text = "(kicad_pcb (version 20260206) (layers (0 \"F.Cu\" signal) (2 \"B.Cu\" signal))\n\
        (footprint \"L:F\" (layer \"F.Cu\") (at 5 5) (property \"Reference\" \"R1\") \
         (pad \"1\" smd rect (at 0 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\") (net \"\")) \
         (pad \"2\" smd rect (at 2 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\") (net \"N1\"))))";
    let p = read_board_interchange(text)?;
    let c = p.components.first().ok_or("no part")?;
    assert_eq!(c.pins.iter().filter(|x| x.net.is_none()).count(), 1);
    assert_eq!(p.nets.len(), 1);
    Ok(())
}

#[test]
fn mounting_hole_listing_copper_layers_imports_as_unplated() -> Result<(), String> {
    let p = read_board_interchange(MODERN)?;
    let hole = p
        .board
        .footprints
        .iter()
        .flat_map(|f| &f.pads)
        .find(|pad| {
            pad.geometry
                .as_ref()
                .is_some_and(|g| g.pad_type == "unplated")
        })
        .ok_or("no unplated pad")?;
    let g = hole.geometry.as_ref().ok_or("no geometry")?;
    assert!(g.layers.iter().all(|l| !l.ends_with(".Cu")));
    assert!(hole.net.is_none());
    Ok(())
}

#[test]
fn object_ids_are_distinct_and_uuids_are_kept() -> Result<(), String> {
    let p = read_board_interchange(MODERN)?;
    let ids: Vec<u64> = p.ids().collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids.len(), sorted.len(), "ids repeat");
    // Segments whose uuids differ only in the last digits stay distinct.
    assert_eq!(p.board.tracks.len(), 3);
    let kept = p.board.tracks.iter().all(|t| {
        t.preserved
            .nodes
            .iter()
            .any(|n| n.starts_with("(uuid") && n.contains("33333333"))
    });
    assert!(kept, "uuid text stays on the track");
    Ok(())
}

#[test]
#[test]
fn eight_layer_stackup_keeps_order_names_and_dielectrics() -> Result<(), String> {
    let p = read_board_interchange(MODERN_8)?;
    assert_eq!(p.copper_layer_count(), 8);
    let names: Vec<String> = p.copper_layers().iter().map(|l| p.layer_name(*l)).collect();
    assert_eq!(
        names,
        [
            "F.Cu", "In1.Cu", "In2.Cu", "In3.Cu", "In4.Cu", "In5.Cu", "In6.Cu", "B.Cu"
        ]
    );
    let core = p
        .board
        .layer_records
        .iter()
        .find(|r| r.name == "dielectric 6")
        .ok_or("core row missing")?;
    assert_eq!(core.kind, "core");
    assert_eq!(core.dk, Some(4.3));
    assert!((core.thickness - 0.15).abs() < 1e-9);
    assert_eq!(p.board.finish, "HASL");
    let t = p
        .board
        .tracks
        .iter()
        .find(|t| t.preserved.nodes.iter().any(|n| n.contains("000000000005")))
        .ok_or("In5 track missing")?;
    assert_eq!(t.layer.index(), 5);
    let spans: Vec<(u8, u8)> = p
        .board
        .vias
        .iter()
        .map(|v| {
            let (a, b) = p.via_span(v);
            (a.index(), b.index())
        })
        .collect();
    assert_eq!(spans, [(5, 6), (0, 6)]);
    Ok(())
}

#[test]
fn eight_layer_export_and_reimport_keep_the_counts() -> Result<(), String> {
    let first = read_board_interchange(MODERN_8)?;
    let text = kicad_board(&first)?;
    let (second, report) = read_board_interchange_report(&text)?;
    assert_eq!(second.copper_layer_count(), 8);
    assert_eq!(second.board.tracks.len(), first.board.tracks.len());
    assert_eq!(second.board.vias.len(), first.board.vias.len());
    assert_eq!(second.nets.len(), first.nets.len());
    assert!(report.source.layers == 8);
    Ok(())
}

#[test]
fn six_layer_stackup_keeps_order_names_and_dielectrics() -> Result<(), String> {
    let p = read_board_interchange(MODERN)?;
    assert_eq!(p.copper_layer_count(), 6);
    let names: Vec<String> = p.copper_layers().iter().map(|l| p.layer_name(*l)).collect();
    assert_eq!(
        names,
        ["F.Cu", "In1.Cu", "In2.Cu", "In3.Cu", "In4.Cu", "B.Cu"]
    );
    let core = p
        .board
        .layer_records
        .iter()
        .find(|r| r.name == "dielectric 2")
        .ok_or("core row missing")?;
    assert_eq!(core.kind, "core");
    assert_eq!(core.dk, Some(4.3));
    assert_eq!(core.df, Some(0.018));
    assert!((core.thickness - 0.2).abs() < 1e-9);
    assert_eq!(p.board.finish, "ENIG");
    // A track on In4.Cu lands on layer index 4.
    let t = p
        .board
        .tracks
        .iter()
        .find(|t| t.preserved.nodes.iter().any(|n| n.contains("000000000002")))
        .ok_or("track missing")?;
    assert_eq!(t.layer.index(), 4);
    Ok(())
}

#[test]
fn blind_and_buried_vias_keep_their_spans() -> Result<(), String> {
    let p = read_board_interchange(MODERN)?;
    let spans: Vec<(u8, u8)> = p
        .board
        .vias
        .iter()
        .map(|v| {
            let (a, b) = p.via_span(v);
            (a.index(), b.index())
        })
        .collect();
    assert_eq!(spans, [(0, 5), (0, 2), (1, 3), (3, 5)]);
    use pcbcraft_doc::ViaKind;
    assert_eq!(p.via_kind(&p.board.vias[0]), ViaKind::Through);
    assert_eq!(p.via_kind(&p.board.vias[2]), ViaKind::Buried);
    assert!(p.board.via_spans.contains(&(0, 2)) || p.allowed_via_spans().len() > 3);
    Ok(())
}

#[test]
fn keepout_zone_becomes_a_rule_area() -> Result<(), String> {
    let (p, report) = read_board_interchange_report(MODERN)?;
    assert_eq!(p.board.rule_areas.len(), 1);
    let area = &p.board.rule_areas[0];
    let k = area.keepout.ok_or("not a keepout")?;
    assert!(k.tracks && k.vias && k.zones && !k.pads);
    assert!(area.layer.is_none());
    assert!((area.region.width - 10.0).abs() < 1e-6);
    assert_eq!(report.source.keepouts, 1);
    assert_eq!(report.imported.keepouts, 1);
    // The two-layer zone became one zone per layer.
    assert_eq!(p.board.zones.len(), 3);
    Ok(())
}

#[test]
fn model_reference_is_kept_as_written() -> Result<(), String> {
    let (p, report) = read_board_interchange_report(MODERN)?;
    let path = p
        .components
        .iter()
        .filter_map(|c| c.library_footprint.as_ref())
        .filter_map(|l| l.definition.model.as_ref())
        .map(|m| m.path.clone())
        .collect::<Vec<_>>();
    assert!(
        path.iter()
            .any(|p| p == "${KICAD10_3DMODEL_DIR}/Test.3dshapes/Hole_Pair.step")
    );
    assert_eq!(report.imported.models, 2);
    assert!(
        report
            .notices
            .iter()
            .any(|n| n.contains("3D model references"))
    );
    Ok(())
}

#[test]
fn report_counts_match_the_source() -> Result<(), String> {
    let (_, r) = read_board_interchange_report(MODERN)?;
    assert_eq!(r.source.parts, 2);
    assert_eq!(r.imported.parts, 2);
    assert_eq!(r.source.nets, 3);
    assert_eq!(r.imported.nets, 3);
    assert_eq!(r.source.tracks, r.imported.tracks);
    assert_eq!(r.source.vias, r.imported.vias);
    assert_eq!(r.source.layers, 6);
    assert_eq!(r.imported.layers, 6);
    Ok(())
}

#[test]
fn export_and_reimport_keep_the_counts() -> Result<(), String> {
    let first = read_board_interchange(MODERN)?;
    let text = kicad_board(&first)?;
    let (second, report) = read_board_interchange_report(&text)?;
    assert_eq!(second.copper_layer_count(), 6);
    assert_eq!(second.board.tracks.len(), first.board.tracks.len());
    assert_eq!(second.board.vias.len(), first.board.vias.len());
    assert_eq!(second.board.zones.len(), first.board.zones.len());
    assert_eq!(second.board.rule_areas.len(), first.board.rule_areas.len());
    assert_eq!(second.nets.len(), first.nets.len());
    assert_eq!(second.components.len(), first.components.len());
    let spans = |p: &pcbcraft_doc::Project| {
        p.board
            .vias
            .iter()
            .map(|v| {
                let (a, b) = p.via_span(v);
                (a.index(), b.index())
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(spans(&first), spans(&second));
    assert_eq!(
        first.board.layer_records.len(),
        second.board.layer_records.len()
    );
    assert!(report.source.layers == 6);
    Ok(())
}

#[test]
fn unknown_constructs_become_notices_not_errors() -> Result<(), String> {
    let text = MODERN.replace(
        "(via (at 112 116)",
        "(via (at 112 116) (mystery 1) (layers \"Nowhere.Cu\" \"B.Cu\")",
    );
    let (p, report) = read_board_interchange_report(&text)?;
    assert!(!p.board.vias.is_empty());
    let odd = "(kicad_pcb (version 1) (segment) (via) (zone) (footprint) (footprint \"x\" (at a b)) (arc (start 0 0)))";
    let (_, report2) = read_board_interchange_report(odd)?;
    assert!(!report2.notices.is_empty());
    let _ = report;
    Ok(())
}

/// A tiny deterministic generator so the mutation test needs no dependency.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// Mutate the fixtures at random and require that the reader never panics.
/// Set `PCBCRAFT_FUZZ_ROUNDS` to run longer than the default 300 rounds.
#[test]
fn mutated_files_never_panic() {
    let rounds: usize = std::env::var("PCBCRAFT_FUZZ_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let mut rng = Lcg(0x5eed);
    for round in 0..rounds {
        let base = if round % 2 == 0 { MODERN } else { ORDINAL };
        let mut bytes: Vec<u8> = base.bytes().collect();
        for _ in 0..(1 + rng.next() % 6) {
            if bytes.is_empty() {
                break;
            }
            let at = (rng.next() as usize) % bytes.len();
            match rng.next() % 6 {
                0 => {
                    bytes.remove(at);
                }
                1 => bytes.insert(at, b'('),
                2 => bytes.insert(at, b')'),
                3 => bytes.truncate(at),
                4 => {
                    let end = (at + 1 + (rng.next() as usize) % 40).min(bytes.len());
                    let chunk: Vec<u8> = bytes[at..end].to_vec();
                    bytes.splice(at..at, chunk);
                }
                _ => {
                    let digits = b"09-.e\"";
                    bytes[at] = digits[(rng.next() as usize) % digits.len()];
                }
            }
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        // Either outcome is fine; reaching here means no panic.
        let _ = read_board_interchange_report(&text);
    }
}

/// The OpenDrone corpus is CERN-OHL-S-2.0: no source file may embed it.
#[test]
fn no_source_embeds_corpus_files() -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name()
                    .is_some_and(|n| n != "target" && n != "node_modules")
                {
                    stack.push(p);
                }
            } else if p.extension().is_some_and(|x| x == "rs")
                && !p.ends_with("board_import_modern.rs")
            {
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                for line in text.lines() {
                    let embeds = line.contains("include_str!") || line.contains("include_bytes!");
                    if embeds && line.to_lowercase().contains("opendrone") {
                        return Err(format!("{} embeds a corpus file", p.display()));
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn imported_board_has_no_schematic_and_no_erc() -> Result<(), String> {
    let p = read_board_interchange(MODERN)?;
    assert!(p.board_only);
    assert!(!p.auto_sync);
    Ok(())
}

#[test]
fn pour_outline_past_the_board_is_trimmed_to_it() -> Result<(), String> {
    let text = MODERN.replace(
        "(polygon (pts (xy 104 104) (xy 108 104) (xy 108 108) (xy 104 108)))",
        "(polygon (pts (xy 90 90) (xy 150 90) (xy 150 140) (xy 90 140)))",
    );
    let (p, report) = read_board_interchange_report(&text)?;
    let z = p
        .board
        .zones
        .iter()
        .find(|z| z.outline.iter().any(|q| (q.x - p.board.width).abs() < 1e-6))
        .ok_or("no trimmed zone")?;
    assert!(
        z.outline
            .iter()
            .all(|q| q.x >= -1e-6 && q.x <= p.board.width + 1e-6)
    );
    assert!(
        z.outline
            .iter()
            .all(|q| q.y >= -1e-6 && q.y <= p.board.height + 1e-6)
    );
    assert!(report.notices.iter().any(|n| n.contains("trimmed")));
    Ok(())
}

#[test]
fn design_rules_come_from_the_project_file() -> Result<(), String> {
    let pro = r#"{"board":{"design_settings":{"rules":{"min_clearance":0.09,"min_track_width":0.09,"min_through_hole_diameter":0.2,"min_via_annular_width":0.075}}},
        "net_settings":{"classes":[{"name":"Default","clearance":0.1}]}}"#;
    let (p, report) = read_board_interchange_with_rules(MODERN, Some(pro))?;
    assert!((p.board.clearance - 0.1).abs() < 1e-9);
    let rules = p.board.rules.ok_or("no rules")?;
    assert_eq!(rules.min_track, Some(0.09));
    assert_eq!(rules.min_ring_via, Some(0.075));
    assert!(report.notices.iter().any(|n| n.contains("design rules")));
    // Garbage is ignored, never an error.
    let (q, _) = read_board_interchange_with_rules(MODERN, Some("not json"))?;
    assert!((q.board.clearance - read_board_interchange(MODERN)?.board.clearance).abs() < 1e-9);
    Ok(())
}
