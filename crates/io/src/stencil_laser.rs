//! Solder paste stencil plots and board laser cut paths as SVG and DXF.
//!
//! Stencil apertures come from surface-mounted pads on one side, shrunk by a
//! percent reduction. Board cut paths follow the edge outline and cutouts with
//! optional kerf compensation. Coordinates are board millimetres.
use crate::{DxfWriter, gerber};
use pcbcraft_doc::{Footprint, Layer, Pad, Project};
use pcbcraft_geom::{Point, hull, polygon_is_simple};

const ALIGNMENT_HOLE_DIAMETER_MM: f64 = 2.0;
const STENCIL_FRAME_GROW_MM: f64 = 0.5;

#[derive(Clone, Debug)]
pub struct StencilPlotOptions {
    pub side: Layer,
    pub reduction_pct: f64,
    pub frame_margin_mm: f64,
    pub alignment_holes: bool,
}

#[derive(Clone, Debug)]
pub struct LaserBoardCutOptions {
    pub kerf_offset_mm: f64,
    pub margin_mm: f64,
}

/// Closed rings for paste stencil apertures on one side (surface pads only).
pub fn stencil_aperture_rings(
    p: &Project,
    side: Layer,
    reduction_pct: f64,
) -> Result<Vec<Vec<Point>>, String> {
    p.validate()?;
    if !reduction_pct.is_finite() || !(0.0..=50.0).contains(&reduction_pct) {
        return Err("stencil reduction must be 0 to 50 percent".into());
    }
    let scale = 1.0 - reduction_pct / 100.0;
    let mut rings = Vec::new();
    for fp in &p.board.footprints {
        for pad in &fp.pads {
            if !gerber::pad_on_side(fp, pad, side) || !gerber::pad_is_smd(pad) {
                continue;
            }
            let Some(position) = fp.pad_position(&pad.number) else {
                continue;
            };
            let ring = scaled_pad_ring(pad, fp, position, scale)?;
            if polygon_is_simple(&ring) {
                rings.push(ring);
            }
        }
    }
    Ok(rings)
}

/// Outer frame ring around board content for a stencil sheet.
pub fn stencil_frame_ring(p: &Project, frame_margin_mm: f64) -> Result<Vec<Point>, String> {
    p.validate()?;
    if !frame_margin_mm.is_finite() || !(0.0..=50.0).contains(&frame_margin_mm) {
        return Err("stencil frame margin must be 0 to 50 millimetres".into());
    }
    let (min, max) = board_content_bounds(p);
    let grow = frame_margin_mm + STENCIL_FRAME_GROW_MM;
    let frame = hull::octagon(
        &[
            Point::new(min.x - grow, min.y - grow),
            Point::new(max.x + grow, min.y - grow),
            Point::new(max.x + grow, max.y + grow),
            Point::new(min.x - grow, max.y + grow),
        ],
        STENCIL_FRAME_GROW_MM,
    );
    close_ring(frame)
}

/// Registration holes outside the frame for aligning the stencil sheet.
pub fn stencil_alignment_rings(
    p: &Project,
    frame_margin_mm: f64,
) -> Result<Vec<Vec<Point>>, String> {
    p.validate()?;
    let (min, max) = board_content_bounds(p);
    let cy = (min.y + max.y) / 2.0;
    let reach = frame_margin_mm + STENCIL_FRAME_GROW_MM + 3.0;
    let radius = ALIGNMENT_HOLE_DIAMETER_MM / 2.0;
    Ok(vec![
        circle_ring(Point::new(min.x - reach, cy), radius),
        circle_ring(Point::new(max.x + reach, cy), radius),
    ])
}

pub fn stencil_plot_svg(p: &Project, options: &StencilPlotOptions) -> Result<String, String> {
    let (width_mm, height_mm, min) = page_size(p, options.frame_margin_mm)?;
    let apertures = stencil_aperture_rings(p, options.side, options.reduction_pct)?;
    let frame = stencil_frame_ring(p, options.frame_margin_mm)?;
    let alignment = if options.alignment_holes {
        stencil_alignment_rings(p, options.frame_margin_mm)?
    } else {
        vec![]
    };
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width_mm}mm\" height=\"{height_mm}mm\" viewBox=\"0 0 {width_mm} {height_mm}\">\n"
    );
    svg.push_str("  <g id=\"apertures\">\n");
    for ring in &apertures {
        svg.push_str("    ");
        svg.push_str(&svg_path(ring, min, "#0000ff"));
        svg.push('\n');
    }
    svg.push_str("  </g>\n  <g id=\"frame\">\n    ");
    svg.push_str(&svg_path(&frame, min, "#0000ff"));
    svg.push_str("\n  </g>\n");
    if !alignment.is_empty() {
        svg.push_str("  <g id=\"alignment-holes\">\n");
        for ring in &alignment {
            svg.push_str("    ");
            svg.push_str(&svg_path(ring, min, "#0000ff"));
            svg.push('\n');
        }
        svg.push_str("  </g>\n");
    }
    svg.push_str("</svg>\n");
    Ok(svg)
}

pub fn stencil_plot_dxf(p: &Project, options: &StencilPlotOptions) -> Result<String, String> {
    let apertures = stencil_aperture_rings(p, options.side, options.reduction_pct)?;
    let frame = stencil_frame_ring(p, options.frame_margin_mm)?;
    let alignment = if options.alignment_holes {
        stencil_alignment_rings(p, options.frame_margin_mm)?
    } else {
        vec![]
    };
    let mut w = DxfWriter::new();
    for ring in apertures {
        w.polyline("Stencil", &ring_to_xy(&ring), true)?;
    }
    w.polyline("Frame", &ring_to_xy(&frame), true)?;
    for ring in alignment {
        w.polyline("Alignment", &ring_to_xy(&ring), true)?;
    }
    Ok(w.finish())
}

pub fn laser_board_cut_svg(p: &Project, options: &LaserBoardCutOptions) -> Result<String, String> {
    p.validate()?;
    if !options.kerf_offset_mm.is_finite() || options.kerf_offset_mm.abs() > 2.0 {
        return Err("kerf offset must be between -2 and 2 millimetres".into());
    }
    if !options.margin_mm.is_finite() || !(0.0..=50.0).contains(&options.margin_mm) {
        return Err("laser cut margin must be 0 to 50 millimetres".into());
    }
    let rings = board_cut_rings(p, options.kerf_offset_mm)?;
    let (width_mm, height_mm, min) = page_size(p, options.margin_mm)?;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width_mm}mm\" height=\"{height_mm}mm\" viewBox=\"0 0 {width_mm} {height_mm}\">\n  <g id=\"board-cut\">\n"
    );
    for ring in rings {
        svg.push_str("    ");
        svg.push_str(&svg_path(&ring, min, "#0000ff"));
        svg.push('\n');
    }
    svg.push_str("  </g>\n</svg>\n");
    Ok(svg)
}

pub fn laser_board_cut_dxf(p: &Project, options: &LaserBoardCutOptions) -> Result<String, String> {
    p.validate()?;
    if !options.kerf_offset_mm.is_finite() || options.kerf_offset_mm.abs() > 2.0 {
        return Err("kerf offset must be between -2 and 2 millimetres".into());
    }
    if !options.margin_mm.is_finite() || !(0.0..=50.0).contains(&options.margin_mm) {
        return Err("laser cut margin must be 0 to 50 millimetres".into());
    }
    let rings = board_cut_rings(p, options.kerf_offset_mm)?;
    let mut w = DxfWriter::new();
    for ring in rings {
        w.polyline("Edge", &ring_to_xy(&ring), true)?;
    }
    Ok(w.finish())
}

fn board_cut_rings(p: &Project, kerf_offset_mm: f64) -> Result<Vec<Vec<Point>>, String> {
    let outline = board_outline_ring(p);
    let offset = crate::cam_drill::offset_ring(&outline, kerf_offset_mm);
    if !polygon_is_simple(&offset) {
        return Err("board outline with kerf offset is not a simple ring".into());
    }
    let mut rings = vec![offset];
    for cutout in &p.board.cutouts {
        let hole = circle_ring(cutout.center, cutout.radius + kerf_offset_mm);
        if polygon_is_simple(&hole) {
            rings.push(hole);
        }
    }
    Ok(rings)
}

fn board_outline_ring(p: &Project) -> Vec<Point> {
    match &p.board.outline {
        Some(o) if o.len() >= 3 => {
            let mut ring = o.clone();
            if ring.first() != ring.last()
                && let Some(first) = ring.first().copied()
            {
                ring.push(first);
            }
            ring
        }
        _ => vec![
            Point::new(0.0, 0.0),
            Point::new(p.board.width, 0.0),
            Point::new(p.board.width, p.board.height),
            Point::new(0.0, p.board.height),
            Point::new(0.0, 0.0),
        ],
    }
}

fn scaled_pad_ring(
    pad: &Pad,
    fp: &Footprint,
    position: Point,
    scale: f64,
) -> Result<Vec<Point>, String> {
    let local: Vec<Point> = pad
        .outline_points()
        .into_iter()
        .map(|pt| Point::new(pt.x * scale, pt.y * scale))
        .collect();
    let mut ring: Vec<Point> = local.iter().map(|pt| position.offset(*pt)).collect();
    close_ring(ring)
}

fn close_ring(mut ring: Vec<Point>) -> Result<Vec<Point>, String> {
    if ring.len() < 3 {
        return Err("a stencil ring needs at least three points".into());
    }
    if ring.first() != ring.last()
        && let Some(first) = ring.first().copied()
    {
        ring.push(first);
    }
    Ok(ring)
}

fn board_content_bounds(p: &Project) -> (Point, Point) {
    let mut min = Point::new(p.board.width, p.board.height);
    let mut max = Point::default();
    let extend = |min: &mut Point, max: &mut Point, point: Point| {
        if point.x.is_finite() && point.y.is_finite() {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
        }
    };
    for fp in &p.board.footprints {
        for pad in &fp.pads {
            if let Some(pos) = fp.pad_position(&pad.number) {
                extend(&mut min, &mut max, pos);
            }
        }
    }
    for t in &p.board.tracks {
        extend(&mut min, &mut max, t.start);
        extend(&mut min, &mut max, t.end);
    }
    for fid in &p.board.fiducials {
        extend(&mut min, &mut max, fid.position);
    }
    if !min.x.is_finite() {
        return (Point::default(), Point::new(p.board.width, p.board.height));
    }
    (
        Point::new(min.x.min(0.0), min.y.min(0.0)),
        Point::new(max.x.max(p.board.width), max.y.max(p.board.height)),
    )
}

fn page_size(p: &Project, margin_mm: f64) -> Result<(f64, f64, Point), String> {
    let (min, max) = board_content_bounds(p);
    let grow = margin_mm + STENCIL_FRAME_GROW_MM + 5.0;
    let min = Point::new(min.x - grow, min.y - grow);
    let max = Point::new(max.x + grow, max.y + grow);
    let width = (max.x - min.x).max(1.0);
    let height = (max.y - min.y).max(1.0);
    Ok((width, height, min))
}

fn svg_path(ring: &[Point], origin: Point, stroke: &str) -> String {
    let mut d = String::from("<path fill=\"none\" stroke=\"");
    d.push_str(stroke);
    d.push_str("\" stroke-width=\"0.1\" d=\"");
    for (index, point) in ring.iter().enumerate() {
        let x = point.x - origin.x;
        let y = point.y - origin.y;
        if index == 0 {
            d.push_str(&format!("M {x:.6} {y:.6}"));
        } else {
            d.push_str(&format!(" L {x:.6} {y:.6}"));
        }
    }
    d.push_str(" Z\"/>");
    d
}

fn ring_to_xy(ring: &[Point]) -> Vec<(f64, f64)> {
    ring.iter().map(|p| (p.x, p.y)).collect()
}

fn circle_ring(center: Point, radius: f64) -> Vec<Point> {
    let segments = 32;
    let mut ring = Vec::with_capacity(segments + 1);
    for i in 0..segments {
        let angle = i as f64 / segments as f64 * std::f64::consts::TAU;
        ring.push(Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    if let Some(first) = ring.first().copied() {
        ring.push(first);
    }
    ring
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Footprint, Id, Net, Pad, Project};
    use pcbcraft_library::{Drill, FootprintPad};
    fn smd_rect_pad(number: &str, size_mm: f64, offset: Point) -> Pad {
        Pad {
            number: number.into(),
            offset,
            diameter: size_mm,
            net: Some(1),
            castellated: None,
            geometry: Some(FootprintPad {
                number: number.into(),
                pad_type: "smd".into(),
                shape: "rect".into(),
                position: Point::default(),
                rotation: 0.0,
                size: Point::new(size_mm, size_mm),
                layers: vec!["F.Cu".into(), "F.Paste".into()],
                drill: None,
                roundrect_ratio: None,
                chamfer: None,
                polygon: vec![],
                raw_source: String::new(),
            }),
        }
    }

    fn thru_pad(number: &str, offset: Point) -> Pad {
        Pad {
            number: number.into(),
            offset,
            diameter: 1.0,
            net: Some(1),
            castellated: None,
            geometry: Some(FootprintPad {
                number: number.into(),
                pad_type: "thru_hole".into(),
                shape: "circle".into(),
                position: Point::default(),
                rotation: 0.0,
                size: Point::new(1.0, 1.0),
                layers: vec!["F.Cu".into(), "B.Cu".into()],
                drill: Some(Drill {
                    size: Point::new(0.8, 0.8),
                    offset: Point::default(),
                }),
                roundrect_ratio: None,
                chamfer: None,
                polygon: vec![],
                raw_source: String::new(),
            }),
        }
    }

    fn stencil_fixture_project() -> Project {
        let mut project = Project {
            name: "stencil-fixture".into(),
            ..Project::default()
        };
        project.board.width = 20.0;
        project.board.height = 15.0;
        project.board.footprints = vec![Footprint {
            id: 1,
            component: 2,
            position: Point::new(5.0, 7.5),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![
                smd_rect_pad("1", 1.0, Point::new(-2.0, 0.0)),
                smd_rect_pad("2", 1.0, Point::new(2.0, 0.0)),
                thru_pad("3", Point::new(0.0, 3.0)),
            ],
        }];
        project.components = vec![pcbcraft_doc::Component {
            id: 2,
            reference: "U1".into(),
            value: "part".into(),
            footprint: Some(1),
            pins: vec![],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
        }];
        project.nets = vec![Net {
            id: 1,
            name: "N".into(),
            target_length: None,
            length_tolerance: None,
        }];
        project
    }

    #[test]
    fn stencil_plot_svg_one_path_per_smd_pad_not_thru_hole() -> Result<(), String> {
        let project = stencil_fixture_project();
        project.validate()?;
        let options = StencilPlotOptions {
            side: Layer::Front,
            reduction_pct: 10.0,
            frame_margin_mm: 2.0,
            alignment_holes: true,
        };
        let svg = stencil_plot_svg(&project, &options)?;
        let apertures = svg
            .split("<g id=\"apertures\">")
            .nth(1)
            .and_then(|chunk| chunk.split("</g>").next())
            .ok_or("apertures group missing")?;
        let pad_paths = apertures.matches("<path ").count();
        assert_eq!(pad_paths, 2, "only surface pads become apertures");
        for chunk in apertures.split("<path ").skip(1) {
            assert!(
                chunk.contains(" Z\""),
                "every aperture path must close with Z"
            );
        }
        assert!(
            svg.contains("id=\"frame\"") && svg.contains("id=\"alignment-holes\""),
            "stencil plot carries frame and alignment hole groups"
        );
        Ok(())
    }

    #[test]
    fn stencil_ten_percent_reduction_shrinks_one_millimetre_pad() -> Result<(), String> {
        let project = stencil_fixture_project();
        let rings = stencil_aperture_rings(&project, Layer::Front, 10.0)?;
        let ring = rings.first().ok_or("expected one aperture ring")?;
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        for p in ring.iter().take(ring.len().saturating_sub(1)) {
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
        }
        let width = max_x - min_x;
        assert!(
            (width - 0.9).abs() < 0.02,
            "10 percent reduction should leave a 0.9 mm wide opening, got {width}"
        );
        Ok(())
    }

    #[test]
    fn laser_board_cut_dxf_offsets_outline_by_kerf() -> Result<(), String> {
        let mut project = Project::default();
        project.board.width = 10.0;
        project.board.height = 8.0;
        let options = LaserBoardCutOptions {
            kerf_offset_mm: 0.1,
            margin_mm: 1.0,
        };
        let dxf = laser_board_cut_dxf(&project, &options)?;
        assert!(dxf.contains("$INSUNITS\n70\n4\n"));
        assert!(dxf.contains("Edge"));
        let svg = laser_board_cut_svg(&project, &options)?;
        assert!(svg.contains("id=\"board-cut\""));
        Ok(())
    }
}
