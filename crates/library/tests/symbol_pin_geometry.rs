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
fn graphical_style_supported_matches_standard_token_list() -> Result<(), String> {
    for token in PIN_GRAPHICAL_STYLES {
        if !SymbolPin::graphical_style_supported(token) {
            return Err(format!("expected supported style {token}"));
        }
    }
    if SymbolPin::graphical_style_supported("mystery_shape") {
        return Err("mystery_shape must be rejected".into());
    }
    if SymbolPin::graphical_style_supported("") {
        return Err("empty style must be rejected".into());
    }
    Ok(())
}

#[test]
fn stem_end_uses_kicad_angle_in_y_down_space() -> Result<(), String> {
    let horizontal = pin("line", true);
    let mut h = horizontal.clone();
    h.angle = 0.0;
    h.position = Point::new(0.0, 0.0);
    if h.stem_end() != Point::new(2.54, 0.0) {
        return Err("horizontal stem_end mismatch".into());
    }
    let vertical = pin("line", true);
    if vertical.stem_end() != Point::new(5.0, 7.46) {
        return Err("vertical stem_end mismatch".into());
    }
    Ok(())
}

#[test]
fn circle_points_emits_sixteen_points_on_the_radius() -> Result<(), String> {
    let center = Point::new(1.0, 2.0);
    let radius = 3.0;
    let pts = SymbolPin::circle_points(center, radius);
    if pts.len() != 16 {
        return Err(format!("expected 16 points, got {}", pts.len()));
    }
    for p in &pts {
        if (center.distance(*p) - radius).abs() >= 1e-9 {
            return Err("circle_points must lie on the circle".into());
        }
    }
    if (pts[0].x - 4.0).abs() >= 1e-9 || (pts[0].y - 2.0).abs() >= 1e-9 {
        return Err("first sample point mismatch".into());
    }
    Ok(())
}

#[test]
fn decoration_bubbles_draw_respects_visibility_flag() -> Result<(), String> {
    let hidden = pin("inverted", false);
    if !hidden.decoration_bubbles().is_empty() {
        return Err("hidden pin must not emit bubbles".into());
    }
    let forced = hidden.decoration_bubbles_draw(false);
    if forced.len() != 1 {
        return Err("inverted pin must draw one bubble when forced".into());
    }
    let (center, bubble_radius) = forced[0];
    if (hidden.position.distance(center) - bubble_radius).abs() >= 0.05 {
        return Err("bubble must sit one radius from the pin".into());
    }
    if !pin("clock", true).decoration_bubbles_draw(true).is_empty() {
        return Err("clock style must not draw bubbles".into());
    }
    Ok(())
}

#[test]
fn decoration_paths_draw_skips_line_and_builds_clock_wedge() -> Result<(), String> {
    if !pin("line", true).decoration_paths_draw(true).is_empty() {
        return Err("line style must not draw decoration paths".into());
    }
    let paths = pin("clock", true).decoration_paths_draw(true);
    if paths.len() != 1 || paths[0].len() != 3 {
        return Err("clock wedge must be a single three-point path".into());
    }
    let hidden = pin("clock", false);
    if !hidden.decoration_paths().is_empty() {
        return Err("hidden clock pin must not emit paths".into());
    }
    if hidden.decoration_paths_draw(false).len() != 1 {
        return Err("forced clock pin must draw one path".into());
    }
    Ok(())
}
