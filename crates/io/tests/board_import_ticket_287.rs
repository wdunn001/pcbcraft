//! Board interchange import: ticket #287 acceptance tests.
//!
//! Verifies round-trip fidelity for simple and complex boards,
//! preservation of unknown source nodes, and malformed file errors.
use pcbcraft_io::{kicad_board, read_board_interchange, read_board_interchange_report};

const SIMPLE: &str = include_str!("fixtures/board_import_simple_2layer.kicad_pcb");
const ZONES_VIAS: &str = include_str!("fixtures/board_import_zones_vias.kicad_pcb");
const UNKNOWN: &str = include_str!("fixtures/board_import_unknown_nodes.kicad_pcb");

#[test]
fn simple_board_round_trip_is_equal() -> Result<(), String> {
    let first = read_board_interchange(SIMPLE)?;
    let exported = kicad_board(&first)?;
    let second = read_board_interchange(&exported)?;
    assert_eq!(second, first, "round-trip changed the project");
    Ok(())
}

#[test]
fn zones_and_vias_round_trip_is_equal() -> Result<(), String> {
    let first = read_board_interchange(ZONES_VIAS)?;
    let exported = kicad_board(&first)?;
    let second = read_board_interchange(&exported)?;
    assert_eq!(second, first, "round-trip changed the project");
    Ok(())
}

#[test]
fn unknown_nodes_are_preserved_in_export() -> Result<(), String> {
    let (first, report) = read_board_interchange_report(UNKNOWN)?;
    assert!(
        !report.notices.is_empty(),
        "unknown nodes should produce notices"
    );
    let exported = kicad_board(&first)?;
    assert!(
        exported.contains("keep-me"),
        "board-level unknown node not preserved"
    );
    assert!(
        exported.contains("also-keep"),
        "footprint-level unknown node not preserved"
    );
    assert!(
        exported.contains("segment-keep"),
        "segment-level unknown node not preserved"
    );
    assert!(
        exported.contains("zone-keep"),
        "zone-level unknown node not preserved"
    );
    let second = read_board_interchange(&exported)?;
    assert_eq!(second, first, "re-import changed the project");
    Ok(())
}

#[test]
fn malformed_board_error_names_line() -> Result<(), String> {
    let bad = "(kicad_pcb (version 1)\n (segment (start 0 0) (end 1)\n)";
    let result = read_board_interchange(bad);
    assert!(result.is_err(), "malformed board must error");
    let msg = match result {
        Err(e) => e,
        Ok(_) => return Err("expected import failure".into()),
    };
    assert!(
        msg.contains("line"),
        "error should name a line number: {msg}"
    );
    Ok(())
}
