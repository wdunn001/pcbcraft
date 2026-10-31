//! Panel layout and separation oracle tests.
use pcbcraft_doc::{
    Board, PanelGrid, PanelSeparation, PanelSettings, Project, board_copy_size,
    panel_layout_from_grid, panel_layout_with_settings, panel_score_lines, panel_tab_channel_width,
    panel_tab_segments,
};
use pcbcraft_geom::Point;

type T = Result<(), String>;

#[test]
fn board_copy_size_uses_outline_extents_when_present() -> T {
    let mut board = Board::default();
    board.width = 50.0;
    board.height = 40.0;
    board.outline = Some(vec![
        Point::new(5.0, 0.0),
        Point::new(15.0, 0.0),
        Point::new(15.0, 8.0),
        Point::new(5.0, 8.0),
    ]);
    let (w, h) = board_copy_size(&board)?;
    assert!((w - 10.0).abs() < 1e-12);
    assert!((h - 8.0).abs() < 1e-12);
    board.outline = None;
    let (w, h) = board_copy_size(&board)?;
    assert!((w - 50.0).abs() < 1e-12 && (h - 40.0).abs() < 1e-12);
    Ok(())
}

#[test]
fn panel_layout_from_grid_places_copy_origins_on_a_hand_grid() -> T {
    let board = Board::default();
    let grid = PanelGrid {
        columns: 2,
        rows: 1,
        pitch_x: 4.0,
        pitch_y: 0.0,
        rail: 2.0,
    };
    let layout = panel_layout_from_grid(&board, &grid)?;
    assert_eq!(layout.copies.len(), 2);
    assert!((layout.panel_width - (2.0 * layout.board_width + 4.0 + 4.0)).abs() < 1e-9);
    assert_eq!(layout.copies[0].index, 1);
    assert!((layout.copies[1].x - (layout.copies[0].x + layout.board_width + 4.0)).abs() < 1e-9);
    Ok(())
}

#[test]
fn panel_layout_with_settings_requires_valid_fields() -> T {
    let board = Board::default();
    let mut settings = PanelSettings::default();
    settings.enabled = true;
    settings.columns = 1;
    settings.rows = 1;
    settings.tab_channel = 1.5;
    assert!(settings.validate_fields().is_err());
    settings.tab_channel = 1.8;
    settings.validate_fields()?;
    let layout = panel_layout_with_settings(&board, &settings)?;
    assert_eq!(layout.copies.len(), 1);
    Ok(())
}

#[test]
fn panel_layout_from_grid_rejects_zero_columns() {
    let board = Board::default();
    let grid = PanelGrid {
        columns: 0,
        rows: 1,
        pitch_x: 1.0,
        pitch_y: 0.0,
        rail: 0.0,
    };
    assert!(panel_layout_from_grid(&board, &grid).is_err());
}

#[test]
fn panel_score_lines_on_a_two_by_three_grid() -> T {
    let mut p = Project::default();
    p.panel.enabled = true;
    p.panel.columns = 2;
    p.panel.rows = 3;
    p.panel.pitch_x = 2.0;
    p.panel.pitch_y = 2.0;
    p.panel.rail = 2.0;
    p.panel.separation = PanelSeparation::VScore;
    p.board.width = 20.0;
    p.board.height = 10.0;
    let scores = panel_score_lines(&p)?;
    assert_eq!(scores.len(), 3, "one vertical and two horizontal score lines");
    Ok(())
}

#[test]
fn panel_score_and_tab_helpers_follow_separation_mode() -> T {
    let mut p = Project::default();
    p.panel.enabled = true;
    p.panel.columns = 2;
    p.panel.rows = 1;
    p.panel.pitch_x = 10.0;
    p.panel.rail = 0.0;
    p.panel.separation = PanelSeparation::VScore;
    let layout = panel_layout_from_grid(&p.board, &PanelGrid::from(&p.panel)).map_err(|e| e)?;
    let scores = panel_score_lines(&p)?;
    assert_eq!(scores.len(), 1);
    let mid_x = layout.board_width + p.panel.pitch_x / 2.0;
    assert!((scores[0].0.x - mid_x).abs() < 1e-9);
    p.panel.separation = PanelSeparation::Tab;
    assert!(panel_score_lines(&p)?.is_empty());
    let tabs = panel_tab_segments(&p)?;
    assert_eq!(tabs.len(), 1);
    assert_eq!(panel_tab_channel_width(&p), Some(p.panel.tab_channel));
    p.panel.separation = PanelSeparation::VScore;
    assert_eq!(panel_tab_channel_width(&p), None);
    Ok(())
}
