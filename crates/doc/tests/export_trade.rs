use pcbcraft_doc::{ExportTradeSettings, Project, TariffExposureRow};

#[test]
fn export_trade_defaults_stay_empty_on_round_trip() -> Result<(), String> {
    let project = Project::default();
    assert!(project.export_trade.is_empty());
    let json = serde_json::to_string(&project).map_err(|e| e.to_string())?;
    let back: Project = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    assert!(back.export_trade.is_empty());
    back.validate()?;
    Ok(())
}

#[test]
fn tariff_row_bounds_are_enforced() {
    let mut settings = ExportTradeSettings::default();
    settings.tariff_rows.push(TariffExposureRow {
        hts_prefix: "8542".into(),
        origin: "CN".into(),
        rate_percent: 25.0,
        effective_from: "2026-01-01".into(),
        effective_to: String::new(),
        note: String::new(),
    });
    assert!(settings.validate().is_ok());
    settings.tariff_rows[0].origin = "CHINA".into();
    assert!(settings.validate().is_err());
}
