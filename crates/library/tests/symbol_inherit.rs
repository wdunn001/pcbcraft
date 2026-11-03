//! A chip symbol that inherits its pins from a second file, read through recorded
//! answers; the oracles are counts and text taken straight from the recorded files.
mod common;
use common::{Boxed, Recorded};
use pcbcraft_library::cad::{CadKind, CadRef};
use pcbcraft_library::cad_cache::CadCache;
use pcbcraft_library::online_import::import_part;
use pcbcraft_library::open_library::{CardScope, OpenLibrarySource};
use pcbcraft_library::symbol_inherit::read_symbol_with_parents;
use pcbcraft_library::symbol_sourcing::sourcing_fields;

const FOLDER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/open_library");

fn source() -> Result<OpenLibrarySource, Boxed> {
    let rec = Recorded::new()?;
    Ok(OpenLibrarySource::new(Box::new(rec), CadCache::in_memory()).with_clock(Box::new(|| 1_000)))
}

fn chip() -> Result<CadRef, Boxed> {
    Ok(CadRef::new(
        CadKind::Symbol,
        "MCU_ST_STM32F1",
        "STM32F103R8Tx",
    )?)
}

#[test]
fn a_derived_symbol_gets_its_pins_from_the_parent_file() -> Result<(), Boxed> {
    let (def, files) = read_symbol_with_parents(&source()?, &chip()?)?;
    // Oracle: the pin rows of the recorded parent file, counted as plain text.
    let parent = std::fs::read_to_string(format!(
        "{FOLDER}/MCU_ST_STM32F1-STM32F103R_8-B_Tx.kicad_sym"
    ))?;
    let rows = parent.matches("\n\t\t\t(pin ").count();
    assert_eq!(rows, 64);
    assert_eq!(def.pins.len(), rows);
    assert_eq!(def.extends.as_deref(), Some("STM32F103R_8-B_Tx"));
    assert_eq!(def.footprint, "Package_QFP:LQFP-64_10x10mm_P0.5mm");
    assert_eq!(files.len(), 2, "the child file and the parent file");
    Ok(())
}

#[test]
fn a_symbol_with_no_parent_reads_as_one_file() -> Result<(), Boxed> {
    let r = CadRef::new(CadKind::Symbol, "Device", "R")?;
    let (def, files) = read_symbol_with_parents(&source()?, &r)?;
    assert_eq!(def.pins.len(), 2);
    assert_eq!(files.len(), 1);
    Ok(())
}

#[test]
fn a_missing_parent_is_an_error_not_a_pinless_part() -> Result<(), Boxed> {
    let r = CadRef::new(CadKind::Symbol, "MCU_ST_STM32F1", "STM32F103RBTx")?;
    assert!(read_symbol_with_parents(&source()?, &r).is_err());
    Ok(())
}

#[test]
fn the_import_carries_the_sourcing_fields_the_symbol_states() -> Result<(), Boxed> {
    let s = source()?;
    let fp = CadRef::new(CadKind::Footprint, "Package_QFP", "LQFP-64_10x10mm_P0.5mm")?;
    let part = import_part(&s, &chip()?, Some(&fp), None)?;
    assert_eq!(part.symbol.pins.len(), 64);
    let pads = part.footprint.as_ref().map(|f| f.pads.len());
    assert_eq!(pads, Some(64));
    assert_eq!(part.notices.len(), 3, "child file, parent file, footprint");
    let params = part.to_params();
    // Oracle: the strings written in the recorded child file.
    let child =
        std::fs::read_to_string(format!("{FOLDER}/MCU_ST_STM32F1-STM32F103R8Tx.kicad_sym"))?;
    assert!(child.contains("https://www.st.com/resource/en/datasheet/stm32f103r8.pdf"));
    let fields = params["fields"].as_array().ok_or("no fields")?;
    let get = |k: &str| {
        fields
            .iter()
            .find(|f| f["key"] == k)
            .and_then(|f| f["value"].as_str())
    };
    assert_eq!(
        get("Datasheet"),
        Some("https://www.st.com/resource/en/datasheet/stm32f103r8.pdf")
    );
    assert_eq!(get("MPN"), Some("STM32F103R8Tx"));
    assert_eq!(
        get("Description"),
        Some(
            "STMicroelectronics Arm Cortex-M3 MCU, 64KB flash, 20KB RAM, 72 MHz, 2.0-3.6V, 51 GPIO, LQFP64"
        )
    );
    assert_eq!(get("Manufacturer"), None, "a maker is never guessed");
    Ok(())
}

#[test]
fn a_generic_symbol_gets_no_part_number() -> Result<(), Boxed> {
    let r = CadRef::new(CadKind::Symbol, "Device", "R")?;
    let (def, _) = read_symbol_with_parents(&source()?, &r)?;
    let fields = sourcing_fields(&def);
    assert!(fields.iter().all(|(k, _)| k != "MPN" && k != "Datasheet"));
    assert!(
        fields
            .iter()
            .any(|(k, v)| k == "Description" && v == "Resistor")
    );
    Ok(())
}

#[test]
fn a_chip_number_alone_finds_its_family_library() -> Result<(), Boxed> {
    let found = source()?.search_cards(CardScope::Symbols, "stm32f103r8")?;
    let names: Vec<&str> = found.iter().map(|p| p.mpn.as_str()).collect();
    assert_eq!(names, ["STM32F103R8Tx"]);
    assert_eq!(found[0].sku, "MCU_ST_STM32F1/STM32F103R8Tx");
    Ok(())
}

#[test]
fn alternate_function_notes_are_kept_in_source_and_do_not_block_a_part() -> Result<(), Boxed> {
    let (mut def, _) = read_symbol_with_parents(&source()?, &chip()?)?;
    // Oracle: pins whose recorded rows carry an `(alternate` line.
    let parent = std::fs::read_to_string(format!(
        "{FOLDER}/MCU_ST_STM32F1-STM32F103R_8-B_Tx.kicad_sym"
    ))?;
    let pins_with_alternates = parent
        .split("\n\t\t\t(pin ")
        .skip(1)
        .filter(|row| {
            let own = row.split("\n\t\t\t)").next().unwrap_or("");
            own.contains("(alternate ")
        })
        .count();
    assert!(pins_with_alternates > 0);
    assert_eq!(def.unsupported.len(), pins_with_alternates);
    assert!(def.lossy_unsupported().is_empty());
    def.unsupported
        .push("pad 1 shape trapezoid requires unsupported geometry".into());
    assert_eq!(def.lossy_unsupported().len(), 1, "a real loss still blocks");
    Ok(())
}
