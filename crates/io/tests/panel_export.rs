use pcbcraft_engine::Session;
use pcbcraft_io::{gerber_files, panel_fabrication_files, panel_placement_text};
use serde_json::json;

fn sample_panel_project() -> Result<pcbcraft_doc::Project, String> {
    let mut s = Session::sample().map_err(|e| e.to_string())?;
    s.execute(
        "panel.set",
        json!({
            "enabled": true,
            "columns": 2,
            "rows": 2,
            "pitch_x": 2.0,
            "pitch_y": 2.0,
            "rail": 5.0,
            "separation": "tab_routing",
            "tab_channel_width": 1.8
        }),
        None,
    )
    .map_err(|e| e.to_string())?;
    Ok(s.project)
}

#[test]
fn panel_gerber_carries_part_array_attribute() -> Result<(), String> {
    let project = sample_panel_project()?;
    let files = panel_fabrication_files(&project)?;
    let front = files
        .get("panel-F.Cu.gbr")
        .ok_or("panel front copper missing")?;
    assert!(
        front.contains("%TF.Part,Array*%"),
        "panel copper declares the array part attribute"
    );
    assert!(
        front.contains("%TA.AperFunction,FiducialPad,Panel*%"),
        "panel fiducials carry the panel fiducial aperture function"
    );
    Ok(())
}

#[test]
fn panel_two_by_two_sample_board_drill_and_placement() -> Result<(), String> {
    let project = sample_panel_project()?;
    let board_drill = gerber_files(&project)?["pcb-Drill.drl"].clone();
    let board_hits = pcbcraft_io::panel::count_excellon_hits(&board_drill);
    let files = panel_fabrication_files(&project)?;
    let panel_drill = files
        .get("panel-Drill.drl")
        .ok_or("panel drill file missing")?;
    assert!(
        panel_drill.contains("MechanicalDrill,Breakout"),
        "breakaway mouse bites are declared in the drill file"
    );
    assert!(
        panel_drill.contains("MechanicalDrill,Tooling"),
        "tooling holes are declared in the drill file"
    );
    let panel_hits = pcbcraft_io::panel::count_excellon_hits(panel_drill);
    assert!(
        panel_hits >= board_hits.saturating_mul(4) + 4,
        "panel drill carries four board copies plus tooling (board {board_hits}, panel {panel_hits})"
    );
    let placement = panel_placement_text(&project)?;
    for reference in ["J1", "R1", "D1"] {
        for index in 1..=4 {
            let token = format!("\"{reference}_{index}\"");
            assert!(
                placement.contains(&token),
                "placement lists each copy with a distinct reference ({token})"
            );
        }
    }
    assert!(
        !placement.contains("FID"),
        "board fiducials are not duplicated into panel placement"
    );
    Ok(())
}
