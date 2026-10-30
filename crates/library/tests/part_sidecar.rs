use pcbcraft_library::{
    PartAlternate, PartProvenance, PartSidecar, PartSidecarField, PART_SIDECAR_FORMAT,
    PART_SIDECAR_VERSION, read_part_sidecar, write_part_sidecar,
};
use std::path::Path;

#[test]
fn part_sidecar_round_trips_through_disk() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = dir.path().join("R1.pcbcraft-part.json");
    let sidecar = PartSidecar {
        format: PART_SIDECAR_FORMAT.into(),
        version: PART_SIDECAR_VERSION,
        symbol: "R1".into(),
        footprint: "R_0805".into(),
        value: "10k".into(),
        kind: "library".into(),
        fields: vec![PartSidecarField {
            key: "Manufacturer".into(),
            value: "Acme".into(),
        }],
        alternates: vec![PartAlternate {
            manufacturer: "Other".into(),
            mpn: "ALT".into(),
            datasheet: String::new(),
            distributor: String::new(),
            sku: String::new(),
        }],
        provenance: Some(PartProvenance {
            notice: "Copied from project demo.".into(),
            source_project: "demo".into(),
        }),
    };
    write_part_sidecar(Path::new(&path), &sidecar)?;
    let again = read_part_sidecar(Path::new(&path))?;
    assert_eq!(again, sidecar);
    Ok(())
}
