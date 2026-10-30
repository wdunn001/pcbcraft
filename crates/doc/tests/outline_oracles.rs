//! Geometry and outline oracle tests for pads and board outlines.
use pcbcraft_doc::{
    circle_through, outline_area, outline_signed_area, validate_closed_outline, Graphic, Pad,
};
use pcbcraft_geom::Point;
use pcbcraft_library::FootprintPad;

type T = Result<(), String>;

#[test]
fn circle_through_finds_the_circumcircle_of_an_equilateral_triangle() -> T {
    let a = Point::new(0.0, 0.0);
    let b = Point::new(2.0, 0.0);
    let c = Point::new(1.0, 3.0_f64.sqrt());
    let (centre, radius) = circle_through(a, b, c).ok_or("non-collinear triangle")?;
    assert!((centre.x - 1.0).abs() < 1e-9);
    assert!((centre.y - 3.0_f64.sqrt() / 3.0).abs() < 1e-9);
    let want_r = 2.0 / 3.0_f64.sqrt();
    assert!((radius - want_r).abs() < 1e-9);
    assert!(circle_through(a, b, Point::new(1.0, 0.0)).is_none());
    Ok(())
}

#[test]
fn outline_signed_area_uses_the_shoelace_formula() {
    let tri = vec![
        Point::new(0.0, 0.0),
        Point::new(4.0, 0.0),
        Point::new(0.0, 3.0),
    ];
    assert!((outline_signed_area(&tri) - 6.0).abs() < 1e-12);
    assert!((outline_area(&tri) - 6.0).abs() < 1e-12);
    assert_eq!(outline_signed_area(&[Point::default()]), 0.0);
}

#[test]
fn validate_closed_outline_accepts_a_square_and_rejects_degenerate() -> T {
    let square = vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
        Point::new(0.0, 0.0),
    ];
    validate_closed_outline(&square)?;
    let open = vec![
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(1.0, 1.0),
        Point::new(0.0, 1.0),
    ];
    assert!(validate_closed_outline(&open).is_err());
    let bowtie = vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(10.0, 0.0),
        Point::new(0.0, 10.0),
        Point::new(0.0, 0.0),
    ];
    assert!(validate_closed_outline(&bowtie).is_err());
    let two = vec![Point::new(0.0, 0.0), Point::new(1.0, 0.0)];
    assert!(validate_closed_outline(&two).is_err());
    Ok(())
}

#[test]
fn graphic_vertex_rule_counts_vertices_per_shape() {
    assert_eq!(Graphic::vertex_rule("line"), Some((2, 2)));
    assert_eq!(Graphic::vertex_rule("polygon"), Some((3, 256)));
    assert_eq!(Graphic::vertex_rule("spiral"), None);
}

#[test]
fn pad_outline_error_and_contact_radius_match_hand_formulas() {
    let pad = Pad {
        number: "1".into(),
        offset: Point::default(),
        diameter: 1.0,
        net: None,
        castellated: None,
        geometry: Some(FootprintPad {
            number: "1".into(),
            pad_type: "smd".into(),
            shape: "roundrect".into(),
            position: Point::default(),
            rotation: 0.0,
            size: Point::new(2.0, 2.0),
            layers: vec!["F.Cu".into()],
            drill: None,
            roundrect_ratio: Some(0.25),
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        }),
    };
    let radius = 2.0_f64.min(2.0) * 0.25;
    let want_err = radius * (1.0 - (std::f64::consts::PI / 64.0).cos());
    assert!((pad.outline_error() - want_err).abs() < 1e-12);
    assert!((pad.contact_radius() - 0.5).abs() < 1e-12);
}

#[test]
fn pad_drill_outline_points_sample_the_plated_hole() -> T {
    let pad = Pad {
        number: "1".into(),
        offset: Point::default(),
        diameter: 1.6,
        net: None,
        castellated: None,
        geometry: Some(FootprintPad {
            number: "1".into(),
            pad_type: "thru_hole".into(),
            shape: "circle".into(),
            position: Point::default(),
            rotation: 0.0,
            size: Point::new(1.6, 1.6),
            layers: vec!["*.Cu".into()],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(0.8, 0.8),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        }),
    };
    let outline = pad.drill_outline_points();
    assert!(outline.len() >= 4, "a round hole closes with sampled vertices");
    let first = *outline.first().ok_or("hole outline empty")?;
    let last = *outline.last().ok_or("hole outline empty")?;
    assert!(first.distance(last) < 1e-6, "hole outline closes");
    let drill_r = 0.4;
    for pt in &outline {
        assert!((pt.distance(Point::default()) - drill_r).abs() < 1e-6);
    }
    Ok(())
}
