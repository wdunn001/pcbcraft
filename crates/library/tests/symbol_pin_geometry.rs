use pcbcraft_geom::Point;
use pcbcraft_library::{PIN_GRAPHICAL_STYLES, SymbolPin};

fn pin(style: &str, visible: bool) -> SymbolPin {
    SymbolPin {
        number: "1".into(),
        name: "P".into(),
        position: Point::new(5.0, 10.0),
        angle: 90.0,
        length: 2.54,
        electrical_type: "input".into(),
        unit: 1,
        body_style: 1,
        graphical_style: style.into(),
        visible,
    }
}

#[test]
fn graphical_style_supported_matches_standard_token_list() {
    for token in PIN_GRAPHICAL_STYLES {
        assert!(SymbolPin::graphical_style_supported(token));
    }
    assert!(!SymbolPin::graphical_style_supported("mystery_shape"));
    assert!(!SymbolPin::graphical_style_supported(""));
}

#[test]
fn stem_end_uses_kicad_angle_in_y_down_space() {
    let horizontal = pin("line", true);
    let mut h = horizontal.clone();
    h.angle = 0.0;
    h.position = Point::new(0.0, 0.0);
    assert_eq!(h.stem_end(), Point::new(2.54, 0.0));
    let vertical = pin("line", true);
    assert_eq!(vertical.stem_end(), Point::new(5.0, 7.46));
}

#[test]
fn circle_points_emits_sixteen_points_on_the_radius() {
    let pts = SymbolPin::circle_points(Point::new(1.0, 2.0), 3.0);
    assert_eq!(pts.len(), 16);
    assert!((pts[0].x - 4.0).abs() < 1e-9 && (pts[0].y - 2.0).abs() < 1e-9);
    let quarter = pts[4];
    assert!((quarter.x - 1.0).abs() < 1e-9 && (quarter.y - 5.0).abs() < 1e-9);
}

#[test]
fn decoration_bubbles_draw_respects_visibility_flag() {
    let hidden = pin("inverted", false);
    assert!(hidden.decoration_bubbles().is_empty());
    let forced = hidden.decoration_bubbles_draw(false);
    assert_eq!(forced.len(), 1);
    let (center, radius) = forced[0];
    assert!((hidden.position.distance(center) - radius).abs() < 0.05);
    assert!(pin("clock", true).decoration_bubbles_draw(true).is_empty());
}

#[test]
fn decoration_paths_draw_skips_line_and_builds_clock_wedge() {
    assert!(pin("line", true).decoration_paths_draw(true).is_empty());
    let paths = pin("clock", true).decoration_paths_draw(true);
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].len(), 3);
    let hidden = pin("clock", false);
    assert!(hidden.decoration_paths().is_empty());
    assert_eq!(hidden.decoration_paths_draw(false).len(), 1);
}
