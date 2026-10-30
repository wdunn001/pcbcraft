//! Story test for issue #697 (runs without the UI crate).
use pcbcraft_doc::{Component, ComponentField, ExportTradeSettings, Project, TariffExposureRow};
use pcbcraft_rules::export_trade_report;

fn part(id: u64, reference: &str, fields: &[(&str, &str)]) -> Component {
    Component {
        id,
        reference: reference.into(),
        value: "part".into(),
        kind: "resistor".into(),
        position: pcbcraft_geom::Point::new(0.0, 0.0),
        rotation: 0.0,
        pins: vec![],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: fields
            .iter()
            .map(|(k, v)| ComponentField {
                key: (*k).into(),
                value: (*v).into(),
            })
            .collect(),
        sheet: 0,
    }
}

#[test]
fn a_newcomer_sees_export_codes_and_origin_totals_for_a_bill_of_materials() -> Result<(), String> {
    let mut project = Project::default();
    project.export_trade.itar_flag = true;
    project.export_trade.tariff_rows = vec![
        TariffExposureRow {
            hts_prefix: "8542".into(),
            origin: "CN".into(),
            rate_percent: 25.0,
            effective_from: "2026-01-01".into(),
            effective_to: String::new(),
            note: "Fixture rate".into(),
        },
        TariffExposureRow {
            hts_prefix: "8542".into(),
            origin: "CN".into(),
            rate_percent: 10.0,
            effective_from: "2024-01-01".into(),
            effective_to: "2025-12-31".into(),
            note: "Expired row".into(),
        },
    ];
    project.components = vec![
        part(
            1,
            "U1",
            &[
                ("ECCN", "5A002"),
                ("HTS", "8542.31.0000"),
                ("Country of Origin", "CN"),
                ("Manufacturer", "Synth Corp Nine"),
            ],
        ),
        part(
            2,
            "U2",
            &[
                ("ECCN", "EAR99"),
                ("HTS", "8542.39.0000"),
                ("Country of Origin", "CN"),
                ("Manufacturer", "Local Vendor"),
            ],
        ),
        part(3, "U3", &[("Country of Origin", "US")]),
    ];
    project.validate()?;

    let report = export_trade_report(&project, "2026-06-01")?;
    assert_eq!(report.highest_eccn.as_deref(), Some("5A002"));
    assert_eq!(report.unknown_eccn_count, 1);
    assert_eq!(report.unknown_hts_count, 1);
    assert!(report.itar_flag);
    assert_eq!(report.origin_totals.get("CN"), Some(&2));
    assert_eq!(report.origin_totals.get("US"), Some(&1));
    assert_eq!(report.screening_hits.len(), 1);
    assert_eq!(report.screening_hits[0].reference, "U1");
    assert_eq!(report.screening_snapshot_retrieved, "2026-03-01");
    assert_eq!(report.tariff_estimates.len(), 2);
    assert!(
        report
            .tariff_estimates
            .iter()
            .all(|e| (e.rate_percent - 25.0).abs() < f64::EPSILON)
    );
    assert!(report.stale_tariff_row_indexes.contains(&1));

    project.export_trade.tariff_rows[0].rate_percent = 5.0;
    let updated = export_trade_report(&project, "2026-06-01")?;
    assert!(
        updated
            .tariff_estimates
            .iter()
            .all(|e| (e.rate_percent - 5.0).abs() < f64::EPSILON)
    );
    Ok(())
}
