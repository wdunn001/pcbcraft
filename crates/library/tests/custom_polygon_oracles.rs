use pcbcraft_geom::Point;
use pcbcraft_library::custom_polygon_error;

#[test]
fn custom_polygon_error_accepts_anchor_inside_simple_ring() -> Result<(), String> {
    let anchor = Point::new(1.0, 1.0);
    let square = vec![
        Point::new(-2.0, -2.0),
        Point::new(2.0, -2.0),
        Point::new(2.0, 2.0),
        Point::new(-2.0, 2.0),
    ];
    if custom_polygon_error(anchor, &square).is_some() {
        return Err("square ring should validate".into());
    }
    Ok(())
}

#[test]
fn custom_polygon_error_rejects_too_few_vertices_and_bad_anchor() -> Result<(), String> {
    let anchor = Point::new(1.0, 1.0);
    if custom_polygon_error(anchor, &[Point::new(0.0, 0.0), Point::new(1.0, 0.0)])
        != Some("custom pad polygon needs 3 to 256 vertices")
    {
        return Err("two-vertex polygon must fail".into());
    }
    let bowtie = vec![
        Point::new(-1.0, -1.0),
        Point::new(1.0, 1.0),
        Point::new(-1.0, 1.0),
        Point::new(1.0, -1.0),
    ];
    if custom_polygon_error(anchor, &bowtie)
        != Some("custom pad polygon must be simple, with non-zero area and no repeated vertices")
    {
        return Err("self-crossing bowtie must fail".into());
    }
    if custom_polygon_error(Point::new(0.0, 0.0), &square_ring())
        != Some("custom pad anchor size must be positive")
    {
        return Err("zero anchor must fail".into());
    }
    Ok(())
}

#[test]
fn custom_polygon_error_rejects_anchor_outside_polygon() -> Result<(), String> {
    let anchor = Point::new(4.0, 4.0);
    if custom_polygon_error(anchor, &square_ring())
        != Some("custom pad anchor rectangle must lie inside the polygon")
    {
        return Err("outside anchor must fail".into());
    }
    Ok(())
}

fn square_ring() -> Vec<Point> {
    vec![
        Point::new(-2.0, -2.0),
        Point::new(2.0, -2.0),
        Point::new(2.0, 2.0),
        Point::new(-2.0, 2.0),
    ]
}
