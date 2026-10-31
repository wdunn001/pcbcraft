use pcbcraft_geom::Point;
use pcbcraft_library::custom_polygon_error;

#[test]
fn custom_polygon_error_accepts_anchor_inside_simple_ring() {
    let anchor = Point::new(1.0, 1.0);
    let square = vec![
        Point::new(-2.0, -2.0),
        Point::new(2.0, -2.0),
        Point::new(2.0, 2.0),
        Point::new(-2.0, 2.0),
    ];
    assert!(custom_polygon_error(anchor, &square).is_none());
}

#[test]
fn custom_polygon_error_rejects_too_few_vertices_and_bad_anchor() {
    let anchor = Point::new(1.0, 1.0);
    assert_eq!(
        custom_polygon_error(anchor, &[Point::new(0.0, 0.0), Point::new(1.0, 0.0)]),
        Some("custom pad polygon needs 3 to 256 vertices")
    );
    let bowtie = vec![
        Point::new(-1.0, -1.0),
        Point::new(1.0, 1.0),
        Point::new(-1.0, 1.0),
        Point::new(1.0, -1.0),
    ];
    assert_eq!(
        custom_polygon_error(anchor, &bowtie),
        Some(
            "custom pad polygon must be simple, with non-zero area and no repeated vertices"
        )
    );
    assert_eq!(
        custom_polygon_error(Point::new(0.0, 0.0), &square_ring()),
        Some("custom pad anchor size must be positive")
    );
}

#[test]
fn custom_polygon_error_rejects_anchor_outside_polygon() {
    let anchor = Point::new(4.0, 4.0);
    assert_eq!(
        custom_polygon_error(anchor, &square_ring()),
        Some("custom pad anchor rectangle must lie inside the polygon")
    );
}

fn square_ring() -> Vec<Point> {
    vec![
        Point::new(-2.0, -2.0),
        Point::new(2.0, -2.0),
        Point::new(2.0, 2.0),
        Point::new(-2.0, 2.0),
    ]
}
